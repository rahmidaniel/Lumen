//! Vendor sidecar: the few things OpenRGB's protocol cannot do for us.
//!
//! Today that is Razer (battery level and writing a power-on effect to the
//! device's flash) and battery readout for HyperX, Logitech, Glorious, ASUS
//! ROG, SteelSeries and Corsair wireless gear. The sidecar never streams
//! colours; it sends short, reply-matched HID transactions and, for flash
//! writes, pauses the engine so nothing else is talking to the device at that
//! moment.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use hidapi::HidApi;
use lumen_core::engine::{Battery, Command, DeviceExtras, DeviceInfo, EngineHandle, Onboard};
use lumen_vendor_hid::{asus, corsair, glorious, logitech, steelseries};
use lumen_vendor_razer::{Effect as RazerEffect, RazerDevice, Store};
use serde::Deserialize;

/// Built-in effects a device can store as its power-on lighting (UI contract).
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OnboardEffect {
    Off,
    Static { color: Rgb },
    Spectrum,
    Wave { direction: u8 },
    Breathing { color: Option<Rgb> },
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Rgb {
    r: f32,
    g: f32,
    b: f32,
}

impl Rgb {
    fn bytes(self) -> [u8; 3] {
        let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        [q(self.r), q(self.g), q(self.b)]
    }
}

impl From<OnboardEffect> for RazerEffect {
    fn from(e: OnboardEffect) -> Self {
        match e {
            OnboardEffect::Off => RazerEffect::Off,
            OnboardEffect::Static { color } => {
                let [r, g, b] = color.bytes();
                RazerEffect::Static { r, g, b }
            }
            OnboardEffect::Spectrum => RazerEffect::Spectrum,
            OnboardEffect::Wave { direction } => RazerEffect::Wave { direction: if direction == 2 { 2 } else { 1 } },
            OnboardEffect::Breathing { color } => RazerEffect::Breathing { color: color.map(Rgb::bytes) },
        }
    }
}

/// A device one of the vendor sidecars knows how to talk to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Razer(u16),
    HyperX(u16),
    Logitech(u16),
    Glorious(u16),
    Asus(u16),
    SteelSeries(u16),
    Corsair(u16),
}

/// Parse `vid_xxxx&pid_xxxx` out of a Windows HID location such as
/// `HID: \\?\hid#vid_1532&pid_0c05&mi_00#…`.
fn parse_usb_ids(location: &str) -> Option<(u16, u16)> {
    let lower = location.to_ascii_lowercase();
    let hex4 = |key: &str| {
        let idx = lower.find(key)? + key.len();
        u16::from_str_radix(lower.get(idx..idx + 4)?, 16).ok()
    };
    Some((hex4("vid_")?, hex4("pid_")?))
}

fn classify(vid: u16, pid: u16) -> Option<Target> {
    match vid {
        lumen_vendor_razer::VENDOR_ID => Some(Target::Razer(pid)),
        lumen_vendor_hyperx::VENDOR_ID => Some(Target::HyperX(pid)),
        logitech::VENDOR_ID if logitech::supports(pid) => Some(Target::Logitech(pid)),
        glorious::VENDOR_ID if glorious::supports(pid) => Some(Target::Glorious(pid)),
        asus::VENDOR_ID if asus::supports(pid) => Some(Target::Asus(pid)),
        steelseries::VENDOR_ID if steelseries::supports(pid) => Some(Target::SteelSeries(pid)),
        corsair::VENDOR_ID if corsair::supports(pid) => Some(Target::Corsair(pid)),
        _ => None,
    }
}

/// Which sidecar, if any, can talk to the device at an OpenRGB `location`.
///
/// Windows locations embed the USB IDs. On Linux and macOS OpenRGB reports
/// hidapi's device path instead (`HID: /dev/hidraw3`, `HID: DevSrvsID:4295…`),
/// so the path is looked up in the HID enumeration to recover vendor/product.
pub fn target_for(location: &str) -> Option<Target> {
    if let Some((vid, pid)) = parse_usb_ids(location) {
        return classify(vid, pid);
    }
    let path = location.strip_prefix("HID:")?.trim();
    if path.is_empty() {
        return None;
    }
    let api = HidApi::new().ok()?;
    let found = api
        .device_list()
        .find(|d| d.path().to_str().is_ok_and(|p| p.eq_ignore_ascii_case(path)))
        .and_then(|d| classify(d.vendor_id(), d.product_id()));
    found
}

/// Razer product ID from an OpenRGB HID location, if it is a Razer device.
pub fn razer_pid(location: &str) -> Option<u16> {
    match target_for(location) {
        Some(Target::Razer(pid)) => Some(pid),
        _ => None,
    }
}

/// Outcome of one battery poll.
enum Poll {
    Reading(DeviceExtras),
    /// No answer right now (asleep, out of range, busy): keep the last reading and ask again later.
    Quiet,
    /// The device has nothing to report (on the cable with no battery feature): stop asking.
    Unsupported,
}

/// Poll battery state for every sidecar device OpenRGB knows about: on each
/// device-list change, then every minute — with a quick retry while a device
/// has never answered (wireless gear dozes and misses the first probe).
/// Devices that turn out to have no battery are left alone afterwards.
pub async fn run(engine: EngineHandle) {
    let mut snapshots = engine.watch_snapshot();
    let mut known: Vec<(u32, Target)> = Vec::new();
    let mut answered: HashSet<u32> = HashSet::new();
    let mut unsupported: HashSet<u32> = HashSet::new();
    // Location → sidecar target; resolving may enumerate HID devices, so do it once per location.
    let mut resolved: HashMap<String, Option<Target>> = HashMap::new();
    let mut ticker = tokio::time::interval(Duration::from_secs(60));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut retry: Option<Duration> = None;

    loop {
        tokio::select! {
            changed = snapshots.changed() => {
                if changed.is_err() {
                    return;
                }
                let devices: Vec<DeviceInfo> = snapshots.borrow().devices.clone();
                resolved.retain(|location, _| devices.iter().any(|d| &d.location == location));
                let now: Vec<(u32, Target)> = devices
                    .iter()
                    .filter_map(|d| {
                        let target = *resolved.entry(d.location.clone()).or_insert_with(|| target_for(&d.location));
                        target.map(|t| (d.id, t))
                    })
                    .collect();
                if now == known {
                    continue;
                }
                known = now;
                answered.retain(|id| known.iter().any(|(k, _)| k == id));
                unsupported.retain(|id| known.iter().any(|(k, _)| k == id));
            }
            _ = ticker.tick() => {}
            _ = async {
                match retry {
                    Some(d) => tokio::time::sleep(d).await,
                    None => std::future::pending().await,
                }
            } => {}
        }
        retry = None;
        for &(id, target) in &known {
            if unsupported.contains(&id) {
                continue;
            }
            match poll_device(&engine, id, target).await {
                Poll::Reading(extras) => {
                    answered.insert(id);
                    engine.send(Command::SetExtras(id, extras)).await;
                }
                Poll::Quiet => {
                    if !answered.contains(&id) {
                        retry = Some(Duration::from_secs(15));
                    }
                }
                Poll::Unsupported => {
                    tracing::info!(id, ?target, "no battery to read; not polling this device again");
                    unsupported.insert(id);
                }
            }
        }
    }
}

#[derive(Debug)]
enum PollError {
    Unsupported,
    Other(String),
}

impl From<String> for PollError {
    fn from(s: String) -> Self {
        PollError::Other(s)
    }
}

impl From<lumen_vendor_hid::Error> for PollError {
    fn from(e: lumen_vendor_hid::Error) -> Self {
        match e {
            lumen_vendor_hid::Error::Unsupported(_) => PollError::Unsupported,
            other => PollError::Other(other.to_string()),
        }
    }
}

/// Query one device with its stream held, so the reply buffer is ours.
async fn poll_device(engine: &EngineHandle, id: u32, target: Target) -> Poll {
    let result: Result<DeviceExtras, PollError> = with_device_held(engine, id, move || {
        let api = HidApi::new().map_err(|e| e.to_string())?;
        match target {
            Target::Razer(pid) => {
                let dev = RazerDevice::open(&api, pid).map_err(|e| e.to_string())?;
                let battery = match dev.battery() {
                    Ok(b) => {
                        tracing::info!(pid = format_args!("{pid:#06x}"), percent = b.percent, charging = b.charging, "battery");
                        Some(Battery { percent: b.percent, charging: b.charging, millivolts: None })
                    }
                    Err(e) => {
                        tracing::debug!(pid = format_args!("{pid:#06x}"), %e, "no battery reading");
                        None
                    }
                };
                Ok(DeviceExtras { battery, onboard: Some(Onboard::Razer) })
            }
            Target::HyperX(pid) => {
                let b = lumen_vendor_hyperx::read_battery(&api, pid).map_err(|e| e.to_string())?;
                tracing::info!(pid = format_args!("{pid:#06x}"), percent = b.percent, charging = b.charging, millivolts = ?b.millivolts, "battery");
                Ok(DeviceExtras {
                    battery: Some(Battery { percent: b.percent, charging: b.charging, millivolts: b.millivolts }),
                    onboard: None,
                })
            }
            Target::Logitech(pid) | Target::Glorious(pid) | Target::Asus(pid) | Target::SteelSeries(pid) | Target::Corsair(pid) => {
                let b = match target {
                    Target::Logitech(_) => logitech::read_battery(&api, pid),
                    Target::Glorious(_) => glorious::read_battery(&api, pid),
                    Target::Asus(_) => asus::read_battery(&api, pid),
                    Target::SteelSeries(_) => steelseries::read_battery(&api, pid),
                    _ => corsair::read_battery(&api, pid),
                }?;
                tracing::info!(?target, percent = b.percent, charging = b.charging, millivolts = ?b.millivolts, "battery");
                Ok(DeviceExtras {
                    battery: Some(Battery { percent: b.percent, charging: b.charging, millivolts: b.millivolts }),
                    onboard: None,
                })
            }
        }
    })
    .await;
    match result {
        Ok(extras) => Poll::Reading(extras),
        Err(PollError::Unsupported) => Poll::Unsupported,
        Err(PollError::Other(e)) => {
            tracing::debug!(?target, %e, "sidecar device not reachable");
            Poll::Quiet
        }
    }
}

/// Hold the engine's stream to `id`, run the blocking HID work, release.
async fn with_device_held<T, E, F>(engine: &EngineHandle, id: u32, work: F) -> Result<T, E>
where
    T: Send + 'static,
    E: Send + 'static + From<String>,
    F: FnOnce() -> Result<T, E> + Send + 'static,
{
    engine.send(Command::HoldDevice(id, true)).await;
    // Let frames already queued at OpenRGB drain before we talk.
    tokio::time::sleep(Duration::from_millis(150)).await;
    let result = match tokio::task::spawn_blocking(work).await {
        Ok(r) => r,
        Err(e) => Err(E::from(e.to_string())),
    };
    engine.send(Command::HoldDevice(id, false)).await;
    result
}

/// Write `effect` to the device's flash as its power-on lighting. The stream to
/// that device is held while the write happens so the firmware sees a quiet bus.
pub async fn save_to_device(engine: &EngineHandle, device: &DeviceInfo, effect: OnboardEffect) -> Result<(), String> {
    let pid = razer_pid(&device.location).ok_or_else(|| "This device has no onboard memory Lumen can write.".to_string())?;
    let razer_effect: RazerEffect = effect.into();

    let result = with_device_held(engine, device.id, move || {
        let api = HidApi::new().map_err(|e| e.to_string())?;
        let dev = RazerDevice::open(&api, pid).map_err(|e| e.to_string())?;
        dev.set_effect(Store::VarStore, razer_effect).map_err(|e| e.to_string())
    })
    .await;

    match &result {
        Ok(()) => tracing::info!(device = %device.name, ?razer_effect, "stored power-on effect"),
        Err(e) => tracing::warn!(device = %device.name, %e, "storing power-on effect failed"),
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_targets_from_openrgb_location() {
        assert_eq!(razer_pid(r"HID: \\?\hid#vid_1532&pid_0c05&mi_00#7&1c57187&0&0000#{4d1e55b2}"), Some(0x0C05));
        assert_eq!(target_for(r"HID: \\?\HID#VID_1532&PID_0296&MI_01&Col02#…"), Some(Target::Razer(0x0296)));
        assert_eq!(target_for(r"HID: \\?\hid#vid_03f0&pid_0f98&mi_01#…"), Some(Target::HyperX(0x0F98)));
        assert_eq!(razer_pid(r"HID: \\?\hid#vid_03f0&pid_0f98&mi_01#…"), None);
        assert_eq!(target_for(r"HID: \\?\hid#vid_046d&pid_c539&mi_02#…"), Some(Target::Logitech(0xC539)));
        assert_eq!(target_for(r"HID: \\?\hid#vid_258a&pid_2022&mi_02#…"), Some(Target::Glorious(0x2022)));
        assert_eq!(target_for(r"HID: \\?\hid#vid_0b05&pid_1a68&mi_00#…"), Some(Target::Asus(0x1A68)));
        assert_eq!(target_for(r"HID: \\?\hid#vid_1038&pid_1838&mi_03#…"), Some(Target::SteelSeries(0x1838)));
        assert_eq!(target_for(r"HID: \\?\hid#vid_1b1c&pid_1b81&mi_01#…"), Some(Target::Corsair(0x1B81)));
        assert_eq!(target_for(r"HID: \\?\hid#vid_0b05&pid_0001&mi_00#…"), None, "unknown products of a known vendor are left alone");
        assert_eq!(target_for(r"HID: \\?\hid#vid_2516&pid_0070&mi_01#…"), None);
        assert_eq!(target_for("I2C: 0x29"), None);
    }

    #[test]
    fn unknown_hid_paths_resolve_to_nothing() {
        // Linux/macOS-style locations go through the HID enumeration; a path
        // that no device has must not match anything.
        assert_eq!(target_for("HID: /dev/hidraw999"), None);
        assert_eq!(target_for("HID: DevSrvsID:0"), None);
        assert_eq!(target_for("HID: "), None);
    }
}
