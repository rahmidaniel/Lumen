//! Corsair wireless mice and keyboards (vendor ID 0x1B1C).
//!
//! Two generations, both as implemented by ckb-next (`src/daemon/device.c`
//! `nxp_get_battery_info`, `device_bragi.c` `bragi_get_battery_info`,
//! `bragi_common.c` `bragi_get_property`) and OpenRGB's Corsair controllers
//! (interface/usage-page selection, retry behaviour):
//!
//! * **Bragi** ("V2": Dark Core RGB Pro / Pro SE, Ironclaw RGB Wireless,
//!   Harpoon RGB Wireless, K57 RGB Wireless, Slipstream receivers) on
//!   interface 1, usage page 0xFF42. Property read `(08 | id) 02 <property>`
//!   where `id` is 0 for a cable-attached device and 9 (child 1) for a device
//!   behind a receiver; reply `(08 | id) 02 <error> <value LE24>`.
//!   Property 0x0F = battery level in 0.1 %, 0x10 = status (1 charging,
//!   2 discharging, 3 full). A receiver answers 0 while its device sleeps.
//! * **NXP** (legacy: Dark Core RGB / SE, K63 Wireless) on interface 1, usage
//!   page 0xFFC2. Request `0E 50`; reply echoes `0E 50`, byte +4 is a level
//!   index 0–4 → {0, 15, 30, 50, 100} %, byte +5 the status (1 discharging,
//!   2 charging, 3 full). Older firmware only answers on the feature-report
//!   path, which is tried when the interrupt path stays silent.
//!
//! The Dark Core RGB SE is Bragi in OpenRGB but NXP in ckb-next; both are tried.

use std::thread::sleep;
use std::time::Duration;

use hidapi::{DeviceInfo, HidApi, HidDevice};

use crate::{collections, drain, first_answer, read_until, Battery, Error, Result};

pub const VENDOR_ID: u16 = 0x1B1C;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    Bragi,
    Nxp,
    /// Sources disagree: try Bragi, then NXP.
    Either,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Product {
    pub pid: u16,
    pub protocol: Protocol,
    /// This PID is a receiver; the battery belongs to the device paired to it.
    pub receiver: bool,
    pub name: &'static str,
}

const fn p(pid: u16, protocol: Protocol, receiver: bool, name: &'static str) -> Product {
    Product { pid, protocol, receiver, name }
}

/// Every wireless model ckb-next reads a battery from (plus the generic Slipstream receiver).
pub const PRODUCTS: &[Product] = &[
    p(0x1B80, Protocol::Bragi, false, "Dark Core RGB Pro"),
    p(0x1B81, Protocol::Bragi, true, "Dark Core RGB Pro receiver"),
    p(0x1B7E, Protocol::Bragi, false, "Dark Core RGB Pro SE"),
    p(0x1B7F, Protocol::Bragi, true, "Dark Core RGB Pro SE receiver"),
    p(0x1B4C, Protocol::Bragi, false, "Ironclaw RGB Wireless"),
    p(0x1B66, Protocol::Bragi, true, "Ironclaw RGB Wireless receiver"),
    p(0x1B5E, Protocol::Bragi, false, "Harpoon RGB Wireless"),
    p(0x1B65, Protocol::Bragi, true, "Harpoon RGB Wireless receiver"),
    p(0x1B6E, Protocol::Bragi, false, "K57 RGB Wireless"),
    p(0x1B62, Protocol::Bragi, true, "K57 RGB Wireless receiver"),
    p(0x1BA6, Protocol::Bragi, true, "Slipstream receiver"),
    p(0x1B35, Protocol::Nxp, false, "Dark Core RGB"),
    p(0x1B64, Protocol::Nxp, true, "Dark Core RGB receiver"),
    p(0x1B4B, Protocol::Either, false, "Dark Core RGB SE"),
    p(0x1B51, Protocol::Either, true, "Dark Core RGB SE receiver"),
    p(0x1B45, Protocol::Nxp, false, "K63 Wireless"),
    p(0x1B8C, Protocol::Nxp, false, "K63 Wireless"),
    p(0x1B50, Protocol::Nxp, true, "K63 Wireless receiver"),
    p(0x1B8F, Protocol::Nxp, true, "K63 Wireless receiver"),
];

pub fn product(pid: u16) -> Option<&'static Product> {
    PRODUCTS.iter().find(|p| p.pid == pid)
}

pub fn supports(pid: u16) -> bool {
    product(pid).is_some()
}

/// Query the battery of the attached product with `pid`.
pub fn read_battery(api: &HidApi, pid: u16) -> Result<Battery> {
    let product = product(pid).ok_or(Error::Unsupported(pid))?;
    let protocols: &[Protocol] = match product.protocol {
        Protocol::Bragi => &[Protocol::Bragi],
        Protocol::Nxp => &[Protocol::Nxp],
        Protocol::Either => &[Protocol::Bragi, Protocol::Nxp],
    };
    let mut last = Err(Error::Asleep);
    for &protocol in protocols {
        let page: u16 = if protocol == Protocol::Bragi { 0xFF42 } else { 0xFFC2 };
        let candidates = collections(api, VENDOR_ID, pid, |d| rank(d, page))?;
        let result = first_answer(api, candidates, |dev| match protocol {
            Protocol::Nxp => query_nxp(dev),
            _ => query_bragi(dev, if product.receiver { 9 } else { 0 }),
        });
        match result {
            Ok(b) => return Ok(b),
            Err(e) => last = Err(e),
        }
    }
    last
}

/// Vendor collection first, then any other vendor-defined page, then any
/// non-boot interface; never the boot mouse/keyboard collection.
fn rank(d: &DeviceInfo, page: u16) -> Option<u8> {
    if d.usage_page() == page {
        Some(0)
    } else if d.usage_page() >= 0xFF00 {
        Some(1)
    } else if d.interface_number() >= 1 {
        Some(2)
    } else {
        None
    }
}

const BRAGI_MAGIC: u8 = 0x08;
const BRAGI_GET: u8 = 0x02;
const BRAGI_BATTERY_LEVEL: u8 = 0x0F;
const BRAGI_BATTERY_STATUS: u8 = 0x10;

fn bragi_get(dev: &HidDevice, id: u8, property: u8) -> Result<u32> {
    let mut req = [0u8; 65];
    req[1] = BRAGI_MAGIC | id;
    req[2] = BRAGI_GET;
    req[3] = property;
    drain(dev);
    dev.write(&req)?;
    let reply = read_until(dev, Duration::from_millis(400), |r| parse_bragi(r, property)).ok_or(Error::Asleep)?;
    match reply {
        (0, value) => Ok(value),
        (err, _) => Err(Error::Protocol(format!("bragi error {err} reading property {property:#04x}"))),
    }
}

/// `(error code, value)` from a Bragi GET reply; `None` for unrelated reports.
pub(crate) fn parse_bragi(r: &[u8], _property: u8) -> Option<(u8, u32)> {
    if r.len() < 6 || r[0] & 0xF8 != BRAGI_MAGIC || r[1] != BRAGI_GET {
        return None;
    }
    Some((r[2], u32::from_le_bytes([r[3], r[4], r[5], 0])))
}

fn query_bragi(dev: &HidDevice, id: u8) -> Result<Battery> {
    // A receiver whose device has not woken answers zero rather than an error;
    // OpenRGB polls a few times before believing either.
    let mut outcome = Err(Error::Asleep);
    for attempt in 0..3 {
        if attempt > 0 {
            sleep(Duration::from_millis(120));
        }
        let level = match bragi_get(dev, id, BRAGI_BATTERY_LEVEL) {
            Ok(v) => v,
            Err(e) => {
                outcome = Err(e);
                continue;
            }
        };
        let status = bragi_get(dev, id, BRAGI_BATTERY_STATUS).unwrap_or(0);
        match bragi_battery(level, status) {
            Some(b) => return Ok(b),
            None => outcome = Err(Error::Asleep),
        }
    }
    outcome
}

/// Level in 0.1 % units plus status enum → reading; `None` for a sleeping device.
pub(crate) fn bragi_battery(level: u32, status: u32) -> Option<Battery> {
    let percent = (level / 10).min(100) as u8;
    let charging = matches!(status, 1 | 3);
    if percent == 0 && !charging {
        return None;
    }
    Some(Battery { percent, charging, millivolts: None })
}

const NXP_GET: u8 = 0x0E;
const NXP_FIELD_BATTERY: u8 = 0x50;
const NXP_LEVELS: [u8; 5] = [0, 15, 30, 50, 100];

fn query_nxp(dev: &HidDevice) -> Result<Battery> {
    let mut req = [0u8; 65];
    req[1] = NXP_GET;
    req[2] = NXP_FIELD_BATTERY;
    drain(dev);
    dev.write(&req)?;
    if let Some(r) = read_until(dev, Duration::from_millis(1000), parse_nxp) {
        return r;
    }
    // Firmware before 1.20 only speaks feature reports (OpenRGB's fallback).
    dev.send_feature_report(&req)?;
    let mut buf = [0u8; 65];
    dev.get_feature_report(&mut buf)?;
    parse_nxp(&buf[1..]).unwrap_or(Err(Error::Asleep))
}

/// Interpret a report that echoes `0E 50`; `None` for anything else.
pub(crate) fn parse_nxp(r: &[u8]) -> Option<Result<Battery>> {
    if r.len() < 6 || r[0] != NXP_GET || r[1] != NXP_FIELD_BATTERY {
        return None;
    }
    let index = r[4] as usize;
    let status = r[5];
    if index >= NXP_LEVELS.len() {
        return Some(Err(Error::Protocol(format!("battery level index {index}"))));
    }
    let percent = NXP_LEVELS[index];
    let charging = matches!(status, 2 | 3);
    if percent == 0 && status == 0 {
        return Some(Err(Error::Asleep));
    }
    Some(Ok(Battery { percent, charging, millivolts: None }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bragi_values() {
        assert_eq!(parse_bragi(&[0x09, 0x02, 0x00, 0xE8, 0x03, 0x00], 0x0F), Some((0, 1000)));
        assert_eq!(parse_bragi(&[0x08, 0x02, 0x05, 0, 0, 0], 0x0F), Some((5, 0)));
        assert_eq!(parse_bragi(&[0x08, 0x01, 0x36, 0, 0x02], 0x0F), None); // notification, not a GET reply
        assert_eq!(bragi_battery(735, 2), Some(Battery { percent: 73, charging: false, millivolts: None }));
        assert_eq!(bragi_battery(1000, 3), Some(Battery { percent: 100, charging: true, millivolts: None }));
        assert_eq!(bragi_battery(0, 2), None); // receiver answering for a sleeping mouse
        assert_eq!(bragi_battery(0, 1).map(|b| b.percent), Some(0)); // empty but on the cable
    }

    #[test]
    fn nxp_values() {
        let mut r = [0u8; 64];
        r[0] = 0x0E;
        r[1] = 0x50;
        r[4] = 3;
        r[5] = 2;
        assert_eq!(parse_nxp(&r).unwrap().unwrap(), Battery { percent: 50, charging: true, millivolts: None });
        r[4] = 7;
        assert!(matches!(parse_nxp(&r), Some(Err(Error::Protocol(_)))));
        r[4] = 0;
        r[5] = 0;
        assert!(matches!(parse_nxp(&r), Some(Err(Error::Asleep))));
        assert!(parse_nxp(&[0x0E, 0x51, 0, 0, 1, 1]).is_none());
    }

    #[test]
    fn products() {
        assert_eq!(product(0x1B81).map(|p| (p.protocol, p.receiver)), Some((Protocol::Bragi, true)));
        assert_eq!(product(0x1B4B).map(|p| p.protocol), Some(Protocol::Either));
        assert!(!supports(0x1B93)); // Katar Pro, wired
    }
}
