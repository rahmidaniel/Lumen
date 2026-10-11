//! Vendor sidecar: the few things OpenRGB's protocol cannot do for us.
//!
//! Today that is Razer only — battery level and writing a power-on effect to
//! the device's flash. The sidecar never streams colours; it sends short,
//! reply-matched HID transactions and, for flash writes, pauses the engine so
//! nothing else is talking to the device at that moment.

use std::time::Duration;

use hidapi::HidApi;
use lumen_core::engine::{Battery, Command, DeviceExtras, DeviceInfo, EngineHandle, Onboard};
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
}

/// Parse `vid_xxxx&pid_xxxx` out of an OpenRGB HID location such as
/// `HID: \\?\hid#vid_1532&pid_0c05&mi_00#…`.
fn usb_ids(location: &str) -> Option<(u16, u16)> {
    let lower = location.to_ascii_lowercase();
    let hex4 = |key: &str| {
        let idx = lower.find(key)? + key.len();
        u16::from_str_radix(lower.get(idx..idx + 4)?, 16).ok()
    };
    Some((hex4("vid_")?, hex4("pid_")?))
}

pub fn target_for(location: &str) -> Option<Target> {
    match usb_ids(location)? {
        (lumen_vendor_razer::VENDOR_ID, pid) => Some(Target::Razer(pid)),
        (lumen_vendor_hyperx::VENDOR_ID, pid) => Some(Target::HyperX(pid)),
        _ => None,
    }
}

/// Razer product ID from an OpenRGB HID location, if it is a Razer device.
pub fn razer_pid(location: &str) -> Option<u16> {
    match target_for(location) {
        Some(Target::Razer(pid)) => Some(pid),
        _ => None,
    }
}

fn sidecar_devices(devices: &[DeviceInfo]) -> Vec<(u32, Target)> {
    devices.iter().filter_map(|d| target_for(&d.location).map(|t| (d.id, t))).collect()
}

/// Poll battery state for every Razer device OpenRGB knows about: on each
/// device-list change and then every minute.
pub async fn run(engine: EngineHandle) {
    let mut snapshots = engine.watch_snapshot();
    let mut known: Vec<(u32, Target)> = Vec::new();
    let mut ticker = tokio::time::interval(Duration::from_secs(60));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            changed = snapshots.changed() => {
                if changed.is_err() {
                    return;
                }
                let now = sidecar_devices(&snapshots.borrow().devices);
                if now == known {
                    continue;
                }
                known = now;
            }
            _ = ticker.tick() => {}
        }
        for &(id, target) in &known {
            if let Some(extras) = poll_device(&engine, id, target).await {
                engine.send(Command::SetExtras(id, extras)).await;
            }
        }
    }
}

/// Query one device with its stream held, so the reply buffer is ours.
async fn poll_device(engine: &EngineHandle, id: u32, target: Target) -> Option<DeviceExtras> {
    let result = with_device_held(engine, id, move || {
        let api = HidApi::new().map_err(|e| e.to_string())?;
        match target {
            Target::Razer(pid) => {
                let dev = RazerDevice::open(&api, pid).map_err(|e| e.to_string())?;
                let battery = match dev.battery() {
                    Ok(b) => {
                        tracing::info!(pid = format_args!("{pid:#06x}"), percent = b.percent, charging = b.charging, "battery");
                        Some(Battery { percent: b.percent, charging: b.charging })
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
                tracing::info!(pid = format_args!("{pid:#06x}"), percent = b.percent, charging = b.charging, "battery");
                Ok(DeviceExtras { battery: Some(Battery { percent: b.percent, charging: b.charging }), onboard: None })
            }
        }
    })
    .await;
    match result {
        Ok(extras) => Some(extras),
        Err(e) => {
            tracing::debug!(?target, %e, "sidecar device not reachable");
            None
        }
    }
}

/// Hold the engine's stream to `id`, run the blocking HID work, release.
async fn with_device_held<T, F>(engine: &EngineHandle, id: u32, work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    engine.send(Command::HoldDevice(id, true)).await;
    // Let frames already queued at OpenRGB drain before we talk.
    tokio::time::sleep(Duration::from_millis(150)).await;
    let result = tokio::task::spawn_blocking(work).await.map_err(|e| e.to_string()).and_then(|r| r);
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
        assert_eq!(target_for(r"HID: \\?\hid#vid_046d&pid_c539&mi_02#…"), None);
        assert_eq!(target_for("I2C: 0x29"), None);
    }
}
