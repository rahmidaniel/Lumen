//! Device settings: polling rate, DPI stages, sleep timer and power saving,
//! for the devices a vendor sidecar knows how to configure (Razer, the HyperX
//! Pulsefire Haste 2, and Logitech mice through HID++ 2.0).
//!
//! Settings are read from the device each time the UI asks, offered only when
//! the device answers sensibly, and every change is read back. Razer devices
//! keep what they are told. The Haste 2 only changes its live profile, so
//! Lumen remembers what the user chose (`device-settings.json`) and puts it
//! back whenever the mouse has fallen back to its own (see [`reapply_hyperx`]).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use hidapi::HidApi;
use lumen_vendor_hid::logitech;
use lumen_vendor_hyperx::config as hyperx;
use lumen_vendor_razer::settings::{threshold_percent, threshold_raw, IDLE_MAX, IDLE_MIN};
use lumen_vendor_razer::{DpiStages, RazerDevice};
use serde::{Deserialize, Serialize};

use crate::sidecar::Target;

/// One option of a choice.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ChoiceOption {
    pub value: u32,
    pub label: String,
}

/// A setting as the UI shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Setting {
    Choice { id: &'static str, label: &'static str, hint: &'static str, value: u32, options: Vec<ChoiceOption> },
    DpiStages { id: &'static str, label: &'static str, hint: &'static str, stages: Vec<u32>, active: usize, min: u32, max: u32, step: u32, max_stages: usize },
}

/// Everything adjustable on one device.
#[derive(Clone, Debug, Serialize)]
pub struct DeviceSettings {
    pub device: u32,
    pub settings: Vec<Setting>,
    /// `device` when the device keeps changes itself, `lumen` when Lumen has
    /// to put them back after the device resets.
    pub persistence: &'static str,
}

/// A change from the UI.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Change {
    Choice { id: String, value: u32 },
    DpiStages { stages: Vec<u32>, active: usize },
}

const POLLING_HINT: &str = "How often the device reports to the PC. Higher is a little snappier and uses a little more battery.";
const DPI_HINT: &str = "The DPI button on the device steps through these. The highlighted stage is in use.";
const IDLE_HINT: &str = "Without input the device turns its radio off to save battery; a key press or click wakes it.";
const THRESHOLD_HINT: &str = "Below this charge the device dims its lighting to last longer.";

fn polling_choice(value: u32, rates: impl IntoIterator<Item = u32>) -> Setting {
    let options = rates.into_iter().map(|hz| ChoiceOption { value: hz, label: format!("{hz} Hz") }).collect();
    Setting::Choice { id: "polling_rate", label: "Polling rate", hint: POLLING_HINT, value, options }
}

/// `options`, plus the current value when it is not one of them (so the UI can show it).
fn with_current(mut options: Vec<ChoiceOption>, value: u32, label: impl Fn(u32) -> String) -> Vec<ChoiceOption> {
    if !options.iter().any(|o| o.value == value) {
        options.push(ChoiceOption { value, label: label(value) });
        options.sort_by_key(|o| o.value);
    }
    options
}

fn minutes(seconds: u32) -> String {
    if seconds.is_multiple_of(60) {
        format!("{} min", seconds / 60)
    } else {
        format!("{seconds} s")
    }
}

// ---- Razer ----------------------------------------------------------------

fn razer_settings(dev: &RazerDevice) -> Vec<Setting> {
    let mut out = Vec::new();
    if let Ok((kind, hz)) = dev.polling_rate() {
        out.push(polling_choice(u32::from(hz), kind.rates().into_iter().map(u32::from)));
    }
    match dev.dpi_stages() {
        Ok(Some(stages)) => out.push(Setting::DpiStages {
            id: "dpi_stages",
            label: "DPI stages",
            hint: DPI_HINT,
            stages: stages.stages.iter().map(|&(x, _)| u32::from(x)).collect(),
            active: stages.active,
            min: 100,
            max: 30_000,
            step: 50,
            max_stages: lumen_vendor_razer::settings::MAX_DPI_STAGES,
        }),
        _ => {
            if let Ok(Some((x, _))) = dev.dpi() {
                out.push(Setting::DpiStages { id: "dpi", label: "DPI", hint: "Pointer speed.", stages: vec![u32::from(x)], active: 0, min: 100, max: 30_000, step: 50, max_stages: 1 });
            }
        }
    }
    if let Ok(seconds) = dev.idle_time() {
        if (IDLE_MIN..=IDLE_MAX).contains(&seconds) {
            let options = [60, 120, 180, 300, 600, 900].iter().map(|&s| ChoiceOption { value: s, label: minutes(s) }).collect();
            let value = u32::from(seconds);
            out.push(Setting::Choice { id: "idle_time", label: "Sleep after", hint: IDLE_HINT, value, options: with_current(options, value, minutes) });
        }
    }
    if let Ok(raw) = dev.low_battery_threshold() {
        let value = u32::from(threshold_percent(raw));
        if raw > 0 {
            let options = [5, 10, 15, 20, 25].iter().map(|&p| ChoiceOption { value: p, label: format!("{p} %") }).collect();
            out.push(Setting::Choice {
                id: "low_battery",
                label: "Power saving below",
                hint: THRESHOLD_HINT,
                value,
                options: with_current(options, value, |p| format!("{p} %")),
            });
        }
    }
    out
}

fn apply_razer(dev: &RazerDevice, change: &Change) -> Result<(), String> {
    let result = match change {
        Change::Choice { id, value } => match id.as_str() {
            "polling_rate" => {
                let (kind, _) = dev.polling_rate().map_err(|e| e.to_string())?;
                let hz = u16::try_from(*value).map_err(|_| format!("{value} Hz is out of range"))?;
                dev.set_polling_rate(kind, hz)
            }
            "idle_time" => dev.set_idle_time(u16::try_from(*value).unwrap_or(IDLE_MAX)),
            "low_battery" => dev.set_low_battery_threshold(threshold_raw(u8::try_from(*value).unwrap_or(25))),
            other => return Err(format!("unknown setting {other}")),
        },
        Change::DpiStages { stages, active } => {
            let dpis: Vec<u16> = stages.iter().map(|&d| u16::try_from(d).map_err(|_| format!("{d} DPI is out of range"))).collect::<Result<_, _>>()?;
            if dpis.len() == 1 && dev.dpi_stages().ok().flatten().is_none() {
                dev.set_dpi(dpis[0], dpis[0])
            } else {
                dev.set_dpi_stages(&DpiStages { stages: dpis.iter().map(|&d| (d, d)).collect(), active: *active })
            }
        }
    };
    result.map_err(|e| e.to_string())
}

// ---- Logitech ---------------------------------------------------------------

const ONBOARD_HINT: &str = "An onboard profile (set up in G HUB) decides DPI and report rate while it is on. Switch to Lumen to change them here.";

fn logitech_settings(s: &logitech::Settings) -> Vec<Setting> {
    let mut out = Vec::new();
    if let Some(on) = s.onboard_profile {
        let options = vec![ChoiceOption { value: 1, label: "The mouse's onboard profile".into() }, ChoiceOption { value: 0, label: "Lumen".into() }];
        out.push(Setting::Choice { id: "onboard_profile", label: "Settings come from", hint: ONBOARD_HINT, value: u32::from(on), options });
    }
    if let Some((hz, rates)) = &s.report_rate {
        out.push(polling_choice(*hz, rates.iter().copied()));
    }
    if let Some((dpi, values)) = &s.dpi {
        let uniform = values.windows(2).map(|w| w[1] - w[0]).collect::<std::collections::BTreeSet<_>>();
        if values.len() > 16 && uniform.len() == 1 {
            let step = *uniform.iter().next().unwrap_or(&50);
            out.push(Setting::DpiStages { id: "dpi", label: "DPI", hint: "Pointer speed.", stages: vec![*dpi], active: 0, min: values[0], max: *values.last().unwrap_or(dpi), step, max_stages: 1 });
        } else {
            let options = values.iter().map(|&v| ChoiceOption { value: v, label: format!("{v} DPI") }).collect();
            out.push(Setting::Choice { id: "dpi", label: "DPI", hint: "Pointer speed.", value: *dpi, options: with_current(options, *dpi, |v| format!("{v} DPI")) });
        }
    }
    out
}

fn logitech_change(change: &Change) -> Result<logitech::SettingChange, String> {
    match change {
        Change::Choice { id, value } => match id.as_str() {
            "onboard_profile" => Ok(logitech::SettingChange::OnboardProfile(*value == 1)),
            "polling_rate" => Ok(logitech::SettingChange::ReportRate(*value)),
            "dpi" => Ok(logitech::SettingChange::Dpi(*value)),
            other => Err(format!("unknown setting {other}")),
        },
        Change::DpiStages { stages, .. } => stages.first().map(|&d| logitech::SettingChange::Dpi(d)).ok_or_else(|| "no DPI given".to_string()),
    }
}

fn logitech_error(e: lumen_vendor_hid::Error) -> String {
    match e {
        lumen_vendor_hid::Error::Asleep => "The mouse did not answer. Move it to wake it up, then try again.".into(),
        lumen_vendor_hid::Error::Unsupported(_) => "This device has no settings Lumen can change.".into(),
        other => other.to_string(),
    }
}

// ---- HyperX ---------------------------------------------------------------

fn hyperx_settings(config: &hyperx::Config, pid: u16) -> Vec<Setting> {
    vec![
        polling_choice(config.polling_hz(), hyperx::polling_rates(pid).iter().copied()),
        Setting::DpiStages {
            id: "dpi_stages",
            label: "DPI stages",
            hint: DPI_HINT,
            stages: config.stages(),
            active: config.active(),
            min: hyperx::DPI_MIN,
            max: hyperx::DPI_MAX,
            step: hyperx::DPI_STEP,
            max_stages: hyperx::MAX_STAGES,
        },
    ]
}

/// What the user chose for a HyperX mouse; the active stage stays the mouse button's.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HyperXWanted {
    pub polling_hz: Option<u32>,
    pub stages: Option<Vec<u32>>,
}

impl HyperXWanted {
    fn apply_to(&self, config: &mut hyperx::Config) -> Result<(), String> {
        if let Some(hz) = self.polling_hz {
            config.set_polling_hz(hz).map_err(|e| e.to_string())?;
        }
        if let Some(stages) = &self.stages {
            let active = config.active().min(stages.len().saturating_sub(1));
            config.set_stages(stages, active).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn matches(&self, config: &hyperx::Config) -> bool {
        self.polling_hz.is_none_or(|hz| hz == config.polling_hz()) && self.stages.as_ref().is_none_or(|s| *s == config.stages())
    }
}

/// Settings Lumen re-applies, per model, persisted next to `settings.json`.
pub struct Wanted {
    path: PathBuf,
    hyperx: Mutex<HashMap<String, HyperXWanted>>,
}

impl Wanted {
    pub fn load(dir: &std::path::Path) -> Self {
        let path = dir.join("device-settings.json");
        let hyperx = std::fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        Self { path, hyperx: Mutex::new(hyperx) }
    }

    fn key(pid: u16) -> String {
        format!("hyperx:{pid:04x}")
    }

    fn get(&self, pid: u16) -> Option<HyperXWanted> {
        self.hyperx.lock().unwrap().get(&Self::key(pid)).cloned()
    }

    fn update(&self, pid: u16, change: impl FnOnce(&mut HyperXWanted)) {
        let mut all = self.hyperx.lock().unwrap();
        change(all.entry(Self::key(pid)).or_default());
        if let Err(e) = std::fs::write(&self.path, serde_json::to_string_pretty(&*all).unwrap_or_default()) {
            tracing::warn!(%e, "could not save device settings");
        }
    }

    /// Forget what was chosen for a model: the mouse keeps its own settings again.
    pub fn forget(&self, pid: u16) {
        let mut all = self.hyperx.lock().unwrap();
        if all.remove(&Self::key(pid)).is_some() {
            let _ = std::fs::write(&self.path, serde_json::to_string_pretty(&*all).unwrap_or_default());
        }
    }
}

/// Put the user's choices back if the mouse has fallen back to its own
/// profile (power cycle). Called with the stream to the device held.
pub fn reapply_hyperx(api: &HidApi, pid: u16, wanted: &Wanted) {
    let Some(want) = wanted.get(pid) else { return };
    let Ok(current) = hyperx::read_config(api, pid) else { return };
    if want.matches(&current) {
        return;
    }
    let mut config = current;
    if let Err(e) = want.apply_to(&mut config) {
        tracing::warn!(%e, "stored HyperX settings no longer apply; forgetting them");
        wanted.forget(pid);
        return;
    }
    match hyperx::write_config(api, pid, &config) {
        Ok(_) => tracing::info!(pid = format_args!("{pid:#06x}"), "re-applied the mouse's settings after it reset"),
        Err(e) => tracing::debug!(%e, "could not re-apply mouse settings yet"),
    }
}

// ---- entry points (blocking; run with the device's stream held) ------------

/// Read what `target` can adjust.
pub fn read(target: Target) -> Result<(Vec<Setting>, &'static str), String> {
    let api = HidApi::new().map_err(|e| e.to_string())?;
    match target {
        Target::Razer(pid) => {
            let dev = RazerDevice::open(&api, pid).map_err(|e| e.to_string())?;
            Ok((razer_settings(&dev), "device"))
        }
        Target::HyperX(pid) if hyperx::supports(pid) => {
            let config = hyperx::read_config(&api, pid).map_err(asleep)?;
            Ok((hyperx_settings(&config, pid), "lumen"))
        }
        Target::Logitech(pid) => match logitech::read_settings(&api, pid) {
            Ok(s) => Ok((logitech_settings(&s), "device")),
            Err(lumen_vendor_hid::Error::Unsupported(_)) => Ok((Vec::new(), "device")),
            Err(e) => Err(logitech_error(e)),
        },
        _ => Ok((Vec::new(), "device")),
    }
}

/// Apply one change and read the result back.
pub fn write(target: Target, change: &Change, wanted: &Wanted) -> Result<(Vec<Setting>, &'static str), String> {
    let api = HidApi::new().map_err(|e| e.to_string())?;
    match target {
        Target::Razer(pid) => {
            let dev = RazerDevice::open(&api, pid).map_err(|e| e.to_string())?;
            apply_razer(&dev, change)?;
            Ok((razer_settings(&dev), "device"))
        }
        Target::HyperX(pid) if hyperx::supports(pid) => {
            let mut config = hyperx::read_config(&api, pid).map_err(asleep)?;
            match change {
                Change::Choice { id, value } if id == "polling_rate" => config.set_polling_hz(*value).map_err(|e| e.to_string())?,
                Change::DpiStages { stages, active } => config.set_stages(stages, *active).map_err(|e| e.to_string())?,
                Change::Choice { id, .. } => return Err(format!("unknown setting {id}")),
            }
            let back = hyperx::write_config(&api, pid, &config).map_err(|e| e.to_string())?;
            wanted.update(pid, |w| match change {
                Change::Choice { value, .. } => w.polling_hz = Some(*value),
                Change::DpiStages { stages, .. } => w.stages = Some(stages.clone()),
            });
            Ok((hyperx_settings(&back, pid), "lumen"))
        }
        Target::Logitech(pid) => {
            let settings = logitech::write_setting(&api, pid, logitech_change(change)?).map_err(logitech_error)?;
            Ok((logitech_settings(&settings), "device"))
        }
        _ => Err("Lumen cannot change this device's settings.".into()),
    }
}

fn asleep(e: lumen_vendor_hyperx::Error) -> String {
    match e {
        lumen_vendor_hyperx::Error::NoReply(_) => "The mouse did not answer. Move it to wake it up, then try again.".into(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_values_outside_the_list_are_still_shown() {
        let options = vec![ChoiceOption { value: 5, label: "5 %".into() }, ChoiceOption { value: 10, label: "10 %".into() }];
        let with = with_current(options.clone(), 9, |p| format!("{p} %"));
        assert_eq!(with.iter().map(|o| o.value).collect::<Vec<_>>(), [5, 9, 10]);
        assert_eq!(with_current(options, 10, |p| format!("{p} %")).len(), 2);
        assert_eq!(minutes(900), "15 min");
        assert_eq!(minutes(90), "90 s");
    }

    #[test]
    fn wanted_hyperx_settings_round_trip_through_json() {
        let dir = std::env::temp_dir().join(format!("lumen-devset-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let wanted = Wanted::load(&dir);
        wanted.update(0x0F98, |w| w.polling_hz = Some(500));
        wanted.update(0x0F98, |w| w.stages = Some(vec![400, 800]));
        let again = Wanted::load(&dir);
        assert_eq!(again.get(0x0F98), Some(HyperXWanted { polling_hz: Some(500), stages: Some(vec![400, 800]) }));
        again.forget(0x0F98);
        assert_eq!(Wanted::load(&dir).get(0x0F98), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn logitech_ranges_get_an_editor_and_short_lists_a_choice() {
        let range: Vec<u32> = (100..=25_600).step_by(50).collect();
        let s = logitech::Settings { dpi: Some((1600, range)), report_rate: Some((1000, vec![1000, 500, 250, 125])), onboard_profile: Some(false) };
        let out = logitech_settings(&s);
        assert!(matches!(&out[0], Setting::Choice { id: "onboard_profile", value: 0, .. }));
        assert!(matches!(&out[1], Setting::Choice { id: "polling_rate", value: 1000, .. }));
        assert!(matches!(&out[2], Setting::DpiStages { id: "dpi", min: 100, max: 25_600, step: 50, max_stages: 1, .. }));
        let short = logitech::Settings { dpi: Some((800, vec![400, 800, 1600])), ..Default::default() };
        assert!(matches!(&logitech_settings(&short)[0], Setting::Choice { id: "dpi", value: 800, .. }));
        assert_eq!(logitech_change(&Change::Choice { id: "onboard_profile".into(), value: 0 }).unwrap(), logitech::SettingChange::OnboardProfile(false));
        assert_eq!(logitech_change(&Change::DpiStages { stages: vec![2400], active: 0 }).unwrap(), logitech::SettingChange::Dpi(2400));
    }

    #[test]
    fn changes_deserialize_from_the_ui() {
        let c: Change = serde_json::from_str(r#"{"type":"choice","id":"polling_rate","value":500}"#).unwrap();
        assert_eq!(c, Change::Choice { id: "polling_rate".into(), value: 500 });
        let d: Change = serde_json::from_str(r#"{"type":"dpi_stages","stages":[400,800],"active":1}"#).unwrap();
        assert_eq!(d, Change::DpiStages { stages: vec![400, 800], active: 1 });
    }
}
