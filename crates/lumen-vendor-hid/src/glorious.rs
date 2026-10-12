//! Glorious "CORE v1" wireless mice (vendor ID 0x258A): Model O / O- / D / D-
//! Wireless, Model O / D 2 PRO, Series One Pro, Series 2 PRO.
//!
//! Protocol, as implemented by korkje/mxw (`src/report/battery.rs`),
//! dkbednarczyk/mxw (`src/util/status.rs`), AwesomeTy18/GloriousBatteryMonitor
//! (`MouseV2ProBatteryChecker.cs`) and OpenMouse (`glorious-classic`): on USB
//! interface 2's vendor collection (usage page 0xFF01; 0xFF00 on older
//! firmware; 0xFFFF on the 4K/8K PRO models) send the unnumbered 64-byte
//! feature report `00 00 02 02 00 83 …` (register 0x83 = battery), wait about
//! 60 ms, then read the feature report back. Counting from the report-ID byte:
//!
//! | +1 | status: `A1` awake, `A4` asleep, `A0` waking up, `A2` command failed |
//! | +6 | must echo `0x83`; anything else is a stale reply from an earlier command |
//! | +7 | charging flag (1 = charging) |
//! | +8 | battery percent 0–100 (the vendor tools show 0 as 1 %) |
//!
//! The dongle answers even when the mouse is off: status `A4`/`A0` then means
//! the reading is not to be trusted. Nothing wakes the mouse but user input.
//!
//! The Pixart-based "Model x 2 Wireless" family (vendor ID 0x093A) does not
//! answer queries at all; it pushes battery frames on its own and is not
//! handled here.

use std::thread::sleep;
use std::time::Duration;

use hidapi::{HidApi, HidDevice};

use crate::{collections, first_answer, Battery, Error, Result};

pub const VENDOR_ID: u16 = 0x258A;

/// `(cable PID, 2.4 GHz dongle PID, name)` for every model the CORE v1 battery tools list.
pub const PRODUCTS: &[(u16, u16, &str)] = &[
    (0x2011, 0x2022, "Model O Wireless"),
    (0x2013, 0x2024, "Model O- Wireless"),
    (0x2012, 0x2023, "Model D Wireless"),
    (0x2014, 0x2025, "Model D- Wireless"),
    (0x2015, 0x2027, "Model O Pro"),
    (0x2018, 0x2031, "Series One Pro"),
    (0x2019, 0x2033, "Model O 2 PRO"),
    (0x201A, 0x2034, "Model D 2 PRO"),
    (0x201D, 0x2039, "Series 2 PRO"),
    (0x201B, 0x2035, "Model O 2 PRO 4K/8K"),
    (0x201C, 0x2036, "Model D 2 PRO 4K/8K"),
];

pub fn supports(pid: u16) -> bool {
    PRODUCTS.iter().any(|&(wired, dongle, _)| pid == wired || pid == dongle)
}

const REGISTER_BATTERY: u8 = 0x83;

/// Query the battery of the attached product with `pid` (cable or dongle).
pub fn read_battery(api: &HidApi, pid: u16) -> Result<Battery> {
    if !supports(pid) {
        return Err(Error::Unsupported(pid));
    }
    // Interface 2 only; prefer the usage pages the tools use, but keep any
    // other interface-2 collection as a last resort for platforms where
    // hidapi cannot report usage pages.
    let candidates = collections(api, VENDOR_ID, pid, |d| {
        if d.interface_number() != 2 {
            return None;
        }
        Some(match d.usage_page() {
            0xFF01 => 0,
            0xFF00 => 1,
            0xFFFF => 2,
            _ => 3,
        })
    })?;
    first_answer(api, candidates, query)
}

fn request() -> [u8; 65] {
    let mut r = [0u8; 65];
    r[3] = 0x02; // target: mouse
    r[4] = 0x02; // data length
    r[6] = REGISTER_BATTERY;
    r
}

fn query(dev: &HidDevice) -> Result<Battery> {
    dev.send_feature_report(&request())?;
    for _ in 0..5 {
        sleep(Duration::from_millis(60));
        let mut buf = [0u8; 65];
        dev.get_feature_report(&mut buf)?;
        match parse_reply(&buf) {
            Reply::Battery(b) => return Ok(b),
            Reply::Asleep => return Err(Error::Asleep),
            Reply::Stale => continue,
            Reply::Failed(status) => return Err(Error::Protocol(format!("status {status:#04x}"))),
        }
    }
    Err(Error::Protocol("only stale replies to the battery request".into()))
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Reply {
    Battery(Battery),
    Asleep,
    /// Register byte is not 0x83 (or the value is garbage): read again.
    Stale,
    Failed(u8),
}

/// Parse the feature report read back after the battery request (report-ID byte at +0).
pub(crate) fn parse_reply(buf: &[u8]) -> Reply {
    if buf.len() < 9 || buf[6] != REGISTER_BATTERY || buf[8] > 100 {
        return Reply::Stale;
    }
    match buf[1] {
        0xA1 => Reply::Battery(Battery { percent: buf[8].max(1), charging: buf[7] == 1, millivolts: None }),
        0xA4 | 0xA0 => Reply::Asleep,
        other => Reply::Failed(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_matches_the_vendor_tools() {
        let r = request();
        assert_eq!(&r[..7], &[0x00, 0x00, 0x00, 0x02, 0x02, 0x00, 0x83]);
        assert!(r[7..].iter().all(|&b| b == 0));
    }

    #[test]
    fn parses_awake_asleep_and_stale_replies() {
        let mut awake = [0u8; 65];
        awake[1] = 0xA1;
        awake[3] = 0x02;
        awake[6] = 0x83;
        awake[7] = 0x01;
        awake[8] = 77;
        assert_eq!(parse_reply(&awake), Reply::Battery(Battery { percent: 77, charging: true, millivolts: None }));

        let mut zero = awake;
        zero[7] = 0;
        zero[8] = 0;
        assert_eq!(parse_reply(&zero), Reply::Battery(Battery { percent: 1, charging: false, millivolts: None }));

        let mut asleep = awake;
        asleep[1] = 0xA4;
        assert_eq!(parse_reply(&asleep), Reply::Asleep);
        asleep[1] = 0xA0;
        assert_eq!(parse_reply(&asleep), Reply::Asleep);

        let mut failed = awake;
        failed[1] = 0xA2;
        assert_eq!(parse_reply(&failed), Reply::Failed(0xA2));

        let mut stale = awake;
        stale[6] = 0x81; // firmware-version echo left over from another tool
        assert_eq!(parse_reply(&stale), Reply::Stale);
        let mut garbage = awake;
        garbage[8] = 200;
        assert_eq!(parse_reply(&garbage), Reply::Stale);
        assert_eq!(parse_reply(&[0u8; 4]), Reply::Stale);
    }

    #[test]
    fn supported_pids() {
        assert!(supports(0x2011) && supports(0x2022) && supports(0x2036));
        assert!(!supports(0x0036)); // wired Sinowealth Model O: no battery
    }
}
