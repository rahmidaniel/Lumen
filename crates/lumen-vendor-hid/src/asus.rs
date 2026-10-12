//! ASUS ROG wireless mice (vendor ID 0x0B05) on their own 2.4 GHz dongle or
//! on the cable.
//!
//! Protocol, as implemented by seerge/g-helper (`AsusMouse.cs`:
//! `GetBatteryReportPacket`, `ParseBattery`, `ParseChargingState`),
//! Humblemonk/rogctl (`protocol.rs`) and OpenMouse (`asus`): on USB interface
//! 0's vendor collection (usage page 0xFF01) write the unnumbered 64-byte
//! report `12 07 00 …`. The mouse answers with an input report that echoes
//! `12 07`; counting from the first data byte (hidapi delivers unnumbered
//! reports without a report-ID byte):
//!
//! | +4 | battery: percent, or a 0–4 level (×25) on older models |
//! | +5 | auto power-off setting, +6 low-battery warning (not used) |
//! | +7..8 | little-endian millivolts (read by one tool; unverified) |
//! | +9 | charging state, non-zero = charging |
//!
//! `FF AA` instead of the echo, an all-zero report, or silence means the mouse
//! is asleep or out of range; 0 % while not charging is the dongle answering
//! for an absent mouse. Reports from the ROG Omni / SpeedNova receivers use
//! report ID 3, which shifts everything by one byte; both framings are accepted.
//! Unsolicited `12 01` button reports share the collection and are skipped.

use std::time::Duration;

use hidapi::{HidApi, HidDevice};

use crate::{collections, drain, first_answer, read_until, Battery, Error, Result};

pub const VENDOR_ID: u16 = 0x0B05;

/// How the battery byte is encoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scale {
    Percent,
    /// 0–4, each step a quarter (Keris Wireless, Chakram).
    Quarters,
}

/// `(cable PID, dongle PID, scale, name)`, from g-helper's per-model classes.
pub const PRODUCTS: &[(u16, u16, Scale, &str)] = &[
    (0x195E, 0x1960, Scale::Quarters, "Keris Wireless"),
    (0x1A66, 0x1A68, Scale::Percent, "Keris Wireless AimPoint"),
    (0x197D, 0x197F, Scale::Percent, "Gladius III Wireless"),
    (0x1A70, 0x1A72, Scale::Percent, "Gladius III Wireless AimPoint"),
    (0x18E3, 0x18E5, Scale::Quarters, "Chakram"),
    (0x1A18, 0x1A1A, Scale::Percent, "Chakram X"),
    (0x1977, 0x1979, Scale::Percent, "Spatha X"),
    (0x1A92, 0x1A94, Scale::Percent, "Harpe Ace Aim Lab Edition"),
];

pub fn scale(pid: u16) -> Option<Scale> {
    PRODUCTS.iter().find(|p| p.0 == pid || p.1 == pid).map(|p| p.2)
}

pub fn supports(pid: u16) -> bool {
    scale(pid).is_some()
}

/// Query the battery of the attached product with `pid` (cable or dongle).
pub fn read_battery(api: &HidApi, pid: u16) -> Result<Battery> {
    let scale = scale(pid).ok_or(Error::Unsupported(pid))?;
    let candidates = collections(api, VENDOR_ID, pid, |d| {
        if d.interface_number() != 0 {
            return None;
        }
        Some(if d.usage_page() == 0xFF01 { 0 } else { 1 })
    })?;
    first_answer(api, candidates, |dev| query(dev, scale))
}

fn query(dev: &HidDevice, scale: Scale) -> Result<Battery> {
    drain(dev);
    let mut req = [0u8; 65];
    req[1] = 0x12;
    req[2] = 0x07;
    dev.write(&req)?;
    match read_until(dev, Duration::from_millis(500), classify) {
        Some(Reply::Battery(raw)) => interpret(raw, scale),
        Some(Reply::Asleep) | None => Err(Error::Asleep),
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Reply {
    /// The first ten data bytes of a `12 07` reply.
    Battery([u8; 10]),
    Asleep,
}

/// Classify one input report; `None` for unrelated traffic.
pub(crate) fn classify(report: &[u8]) -> Option<Reply> {
    // Omni / SpeedNova receivers prefix report ID 3.
    let body = if report.len() >= 3 && report[0] == 0x03 && report[1] == 0x12 { &report[1..] } else { report };
    if body.len() < 4 {
        return None;
    }
    if body[0] == 0xFF && body[1] == 0xAA {
        return Some(Reply::Asleep);
    }
    if body.iter().take(4).all(|&b| b == 0) {
        return Some(Reply::Asleep);
    }
    if body[0] != 0x12 || body[1] != 0x07 || body.len() < 10 {
        return None;
    }
    let mut raw = [0u8; 10];
    raw.copy_from_slice(&body[..10]);
    Some(Reply::Battery(raw))
}

pub(crate) fn interpret(raw: [u8; 10], scale: Scale) -> Result<Battery> {
    let level = raw[4];
    let charging = raw[9] != 0;
    let percent = match scale {
        Scale::Percent => level,
        Scale::Quarters => level.saturating_mul(25),
    };
    if percent > 100 {
        return Err(Error::Protocol(format!("battery byte {level:#04x}")));
    }
    if percent == 0 && !charging {
        // g-helper: "likely only the dongle connected and the mouse is either sleeping or turned off".
        return Err(Error::Asleep);
    }
    let millivolts = Some(u16::from_le_bytes([raw[7], raw[8]])).filter(|mv| (2500..=4600).contains(mv));
    Ok(Battery { percent, charging, millivolts })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_rogctl_capture() {
        // Harpe II Ace on a SpeedNova receiver (report ID 3): 80 %, power-off
        // 3 min, warning 20 %, 4056 mV, not charging.
        let r = [0x03, 0x12, 0x07, 0x00, 0x00, 0x50, 0x02, 0x14, 0xd8, 0x0f, 0x00, 0x00, 0x01];
        let Some(Reply::Battery(raw)) = classify(&r) else { panic!("not classified as battery") };
        let b = interpret(raw, Scale::Percent).unwrap();
        assert_eq!(b, Battery { percent: 80, charging: false, millivolts: Some(4056) });

        // Same frame without the report-ID byte, as hidapi delivers unnumbered reports.
        let Some(Reply::Battery(raw)) = classify(&r[1..]) else { panic!("not classified as battery") };
        assert_eq!(interpret(raw, Scale::Percent).unwrap().percent, 80);
    }

    #[test]
    fn quarters_charging_and_sleep_semantics() {
        let mut r = [0u8; 12];
        r[0] = 0x12;
        r[1] = 0x07;
        r[4] = 3;
        r[9] = 1;
        let Some(Reply::Battery(raw)) = classify(&r) else { panic!() };
        assert_eq!(interpret(raw, Scale::Quarters).unwrap(), Battery { percent: 75, charging: true, millivolts: None });

        let mut absent = r;
        absent[4] = 0;
        absent[9] = 0;
        let Some(Reply::Battery(raw)) = classify(&absent) else { panic!() };
        assert!(matches!(interpret(raw, Scale::Percent), Err(Error::Asleep)));

        assert_eq!(classify(&[0xFF, 0xAA, 0, 0]), Some(Reply::Asleep));
        assert_eq!(classify(&[0u8; 16]), Some(Reply::Asleep));
        // Button bitmap and other traffic is skipped, not misread.
        assert_eq!(classify(&[0x12, 0x01, 0x00, 0x04, 0, 0, 0, 0, 0, 0, 0, 0]), None);
        assert_eq!(classify(&[0x12, 0x07, 0x00]), None);
    }

    #[test]
    fn supported_pids() {
        assert_eq!(scale(0x1A68), Some(Scale::Percent));
        assert_eq!(scale(0x1960), Some(Scale::Quarters));
        assert_eq!(scale(0x18B4), None); // Strix Carry: different reply layout, not handled
    }
}
