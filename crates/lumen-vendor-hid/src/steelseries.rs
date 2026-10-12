//! SteelSeries wireless mice (vendor ID 0x1038).
//!
//! Protocol as implemented by flozz/rivalcfg (`rivalcfg/devices/aerox3_wireless_*.py`,
//! `prime_wireless_*.py`, `rival3_wireless*.py`, `rival650.py`; `rivalcfg/mouse.py`
//! `Mouse.battery`): a short output report on the vendor interface, one input
//! report back within about 200 ms.
//!
//! * Aerox 3 / 5 / 9 Wireless and Prime (Mini) Wireless, interface 3 (usage
//!   page 0xFFC0): request `92` on the cable PID, `D2` (`92 | 0x40`, the
//!   wireless flag) on the dongle PID. Reply byte +1: bit 7 = charging,
//!   `((b & 0x7F) - 1) * 5` = percent; `FF` means the mouse is off or asleep
//!   (the dongle answers regardless).
//! * Rival 3 Wireless (Gen 2), interface 3, and Rival 650 (Wireless),
//!   interface 0: request `AA 01`; reply +0 = percent, +2 = charging. The
//!   Rival 650 dongle stays silent while the mouse is off.
//!
//! rivalcfg's own tracker notes the Aerox query "only works sometimes" on
//! Windows, so the request is retried a few times.

use std::thread::sleep;
use std::time::Duration;

use hidapi::{HidApi, HidDevice};

use crate::{collections, drain, first_answer, read_until, Battery, Error, Result};

pub const VENDOR_ID: u16 = 0x1038;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// `92` / `D2` query, 5 % steps.
    Aerox,
    /// `AA 01` query, plain percent.
    Rival,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Product {
    pub pid: u16,
    pub family: Family,
    /// Product ID of the 2.4 GHz dongle rather than the cable.
    pub wireless: bool,
    pub interface: i32,
    pub name: &'static str,
}

const fn aerox(pid: u16, wireless: bool, name: &'static str) -> Product {
    Product { pid, family: Family::Aerox, wireless, interface: 3, name }
}

const fn rival(pid: u16, wireless: bool, interface: i32, name: &'static str) -> Product {
    Product { pid, family: Family::Rival, wireless, interface, name }
}

/// Every model rivalcfg has a `battery_level` command for.
pub const PRODUCTS: &[Product] = &[
    aerox(0x183A, false, "Aerox 3 Wireless"),
    aerox(0x1838, true, "Aerox 3 Wireless"),
    aerox(0x187A, false, "Aerox 3 Wireless CS2 Dragon Lore"),
    aerox(0x1878, true, "Aerox 3 Wireless CS2 Dragon Lore"),
    aerox(0x1854, false, "Aerox 5 Wireless"),
    aerox(0x1852, true, "Aerox 5 Wireless"),
    aerox(0x185E, false, "Aerox 5 Wireless Destiny 2"),
    aerox(0x185C, true, "Aerox 5 Wireless Destiny 2"),
    aerox(0x1862, false, "Aerox 5 Wireless Diablo IV"),
    aerox(0x1860, true, "Aerox 5 Wireless Diablo IV"),
    aerox(0x185A, false, "Aerox 9 Wireless"),
    aerox(0x1858, true, "Aerox 9 Wireless"),
    aerox(0x1876, false, "Aerox 9 Wireless WoW"),
    aerox(0x1874, true, "Aerox 9 Wireless WoW"),
    aerox(0x1842, false, "Prime Wireless"),
    aerox(0x1840, true, "Prime Wireless"),
    aerox(0x184A, false, "Prime Mini Wireless"),
    aerox(0x1848, true, "Prime Mini Wireless"),
    rival(0x1830, true, 3, "Rival 3 Wireless"),
    rival(0x1872, true, 3, "Rival 3 Wireless Gen 2"),
    rival(0x172B, false, 0, "Rival 650"),
    rival(0x1726, true, 0, "Rival 650 Wireless"),
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
    let candidates = collections(api, VENDOR_ID, pid, |d| {
        if d.interface_number() != product.interface {
            return None;
        }
        Some(if d.usage_page() == 0xFFC0 { 0 } else { 1 })
    })?;
    first_answer(api, candidates, |dev| match product.family {
        Family::Aerox => query_aerox(dev, product.wireless),
        Family::Rival => query_rival(dev),
    })
}

fn query_aerox(dev: &HidDevice, wireless: bool) -> Result<Battery> {
    let cmd: u8 = if wireless { 0x92 | 0x40 } else { 0x92 };
    let mut req = [0u8; 65];
    req[1] = cmd;
    let mut outcome = Err(Error::Asleep);
    for attempt in 0..3 {
        if attempt > 0 {
            sleep(Duration::from_millis(150));
        }
        drain(dev);
        dev.write(&req)?;
        match read_until(dev, Duration::from_millis(250), |r| parse_aerox(r, cmd)) {
            Some(Ok(b)) => return Ok(b),
            Some(Err(e)) => outcome = Err(e),
            None => {}
        }
    }
    outcome
}

/// Interpret one input report as an answer to `cmd`; `None` for unrelated traffic.
pub(crate) fn parse_aerox(r: &[u8], cmd: u8) -> Option<Result<Battery>> {
    if r.len() < 2 {
        return None;
    }
    // Observed echoes: `12 ..` for the 92 query on the cable, `40 FF` from an
    // idle dongle. Anything else (`BC 01` power notices, `63 00` idle checks)
    // is not the answer.
    let plain = cmd & 0xBF;
    if ![cmd, plain, plain & 0x7F, 0x40].contains(&r[0]) {
        return None;
    }
    let b = r[1];
    if b == 0xFF {
        return Some(Err(Error::Asleep));
    }
    let raw = b & 0x7F;
    if raw == 0 || raw > 21 {
        return None;
    }
    Some(Ok(Battery { percent: (raw - 1) * 5, charging: b & 0x80 != 0, millivolts: None }))
}

fn query_rival(dev: &HidDevice) -> Result<Battery> {
    let mut req = [0u8; 65];
    req[1] = 0xAA;
    req[2] = 0x01;
    for attempt in 0..2 {
        if attempt > 0 {
            sleep(Duration::from_millis(150));
        }
        drain(dev);
        dev.write(&req)?;
        if let Some(b) = read_until(dev, Duration::from_millis(250), parse_rival) {
            return Ok(b);
        }
    }
    Err(Error::Asleep)
}

pub(crate) fn parse_rival(r: &[u8]) -> Option<Battery> {
    if r.len() < 3 || r[0] > 100 {
        return None;
    }
    Some(Battery { percent: r[0], charging: r[2] != 0, millivolts: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aerox_replies() {
        // Capture (cable): `12 94` = charging, raw 0x14 → 95 %.
        let b = parse_aerox(&[0x12, 0x94], 0x92).unwrap().unwrap();
        assert_eq!(b, Battery { percent: 95, charging: true, millivolts: None });
        // Dongle, mouse off: `40 FF`.
        assert!(matches!(parse_aerox(&[0x40, 0xFF], 0xD2), Some(Err(Error::Asleep))));
        // raw 1 → 0 %, raw 21 → 100 %.
        assert_eq!(parse_aerox(&[0x92, 0x01], 0x92).unwrap().unwrap().percent, 0);
        assert_eq!(parse_aerox(&[0xD2, 0x15], 0xD2).unwrap().unwrap().percent, 100);
        // Unrelated traffic is skipped.
        assert!(parse_aerox(&[0xBC, 0x01], 0xD2).is_none());
        assert!(parse_aerox(&[0x63, 0x00], 0xD2).is_none());
        assert!(parse_aerox(&[0x12, 0x7F], 0x92).is_none()); // raw 127: not a level
    }

    #[test]
    fn rival_replies_and_products() {
        assert_eq!(parse_rival(&[42, 0, 1]), Some(Battery { percent: 42, charging: true, millivolts: None }));
        assert_eq!(parse_rival(&[200, 0, 0]), None);
        assert_eq!(product(0x1838).map(|p| (p.family, p.wireless, p.interface)), Some((Family::Aerox, true, 3)));
        assert_eq!(product(0x1726).map(|p| (p.family, p.interface)), Some((Family::Rival, 0)));
        assert!(!supports(0x1702)); // Rival 100, wired
    }
}
