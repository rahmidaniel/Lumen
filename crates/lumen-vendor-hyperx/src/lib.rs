//! Battery readout for HyperX (HP) wireless mice.
//!
//! Protocol from community reverse engineering of the Pulsefire Haste 2
//! Wireless (<https://github.com/Emanuel4100/omarchy-hyperx-battery>): on the
//! vendor interface (USB interface 2, usage page 0xFF00) a 64-byte output
//! report `0x50 0x02 …` is answered by an input report containing `0x51 0x02`,
//! battery percent at +2 and charging state at +3/+4. On Windows the interface
//! reports numbered reports (0x50 is the report ID), so the 64 bytes are
//! written as-is; the unnumbered framing is kept as a fallback. A sleeping
//! mouse does not answer — callers should keep the last reading.

use std::time::{Duration, Instant};

use hidapi::{HidApi, HidDevice};
use serde::Serialize;

pub mod config;

pub const VENDOR_ID: u16 = 0x03F0;

pub mod pid {
    pub const PULSEFIRE_HASTE_2_WIRELESS: u16 = 0x0F98;
    pub const PULSEFIRE_HASTE_2_WIRED: u16 = 0x0B97;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Battery {
    /// 0–100 (byte +2 of the reply, as the community tools read it).
    pub percent: u8,
    pub charging: bool,
    /// Bytes +6..+7 little-endian; on the Haste 2 Wireless this sits around
    /// 3.9 V and moves with load, so it is the cell voltage (`None` if implausible).
    pub millivolts: Option<u16>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Hid(#[from] hidapi::HidError),
    #[error("no HyperX device with product ID {0:#06x}")]
    NotFound(u16),
    #[error("no interface of {0:#06x} answered (mouse asleep?)")]
    NoReply(u16),
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Query the battery of the first attached HyperX product with `pid`.
pub fn read_battery(api: &HidApi, pid: u16) -> Result<Battery> {
    let mut interfaces: Vec<_> = api.device_list().filter(|d| d.vendor_id() == VENDOR_ID && d.product_id() == pid).collect();
    if interfaces.is_empty() {
        return Err(Error::NotFound(pid));
    }
    // Only vendor-defined collections take the request (the mouse/keyboard
    // collections are locked by Windows; interface 3 is a HID LampArray).
    // The vendor interface is #2 on the Haste 2; try it first, then the rest.
    interfaces.retain(|d| d.usage_page() >= 0xFF00);
    interfaces.sort_by_key(|d| if d.interface_number() == 2 { 0 } else { 1 });

    for info in interfaces {
        let dev = match api.open_path(info.path()) {
            Ok(d) => d,
            Err(e) => {
                tracing::trace!(path = ?info.path(), %e, "cannot open interface");
                continue;
            }
        };
        if let Some(b) = query(&dev) {
            return Ok(b);
        }
    }
    Err(Error::NoReply(pid))
}

fn query(dev: &HidDevice) -> Option<Battery> {
    // Unnumbered report: leading 0x00 report ID, then the 64-byte payload.
    let mut unnumbered = [0u8; 65];
    unnumbered[1] = 0x50;
    unnumbered[2] = 0x02;
    // Fallback framing if the interface uses numbered reports.
    let mut numbered = [0u8; 64];
    numbered[0] = 0x50;
    numbered[1] = 0x02;

    for request in [&numbered[..], &unnumbered[..]] {
        if dev.write(request).is_err() {
            continue;
        }
        let deadline = Instant::now() + Duration::from_millis(400);
        while Instant::now() < deadline {
            let mut buf = [0u8; 128];
            let n = match dev.read_timeout(&mut buf, 100) {
                Ok(n) => n,
                Err(_) => break,
            };
            if n == 0 {
                continue;
            }
            if let Some(b) = parse_reply(&buf[..n]) {
                return Some(b);
            }
        }
    }
    None
}

/// Parse a `51 02 <percent> <phase> <?> <?> <mV lo> <mV hi> …` report.
///
/// The header must start the report: the receiver also emits ACK frames
/// (`ff 01 <12-byte id> 51 02 00 00 …`) that merely echo the command with
/// zeroed data, and matching those would report a phantom 0 %.
///
/// Observed on a Haste 2 Wireless (2026-10-12): `62 00 1a 00 39 0f 01` —
/// +2 = 98, +3 = 0, +4 slowly rising 25→26 (looks like °C), +6..7 = 3897 mV.
fn parse_reply(buf: &[u8]) -> Option<Battery> {
    if buf.len() < 5 || buf[0] != 0x51 || buf[1] != 0x02 {
        return None;
    }
    let percent = buf[2];
    if percent > 100 {
        return None;
    }
    // Phase byte: 0x01 plugged / 0x02 charging per the community daemon; the
    // old "+4 == 0x00 means wired" rule is wrong for this firmware (+4 is ~25).
    let charging = matches!(buf[3], 0x01 | 0x02);
    let millivolts = (buf.len() >= 8)
        .then(|| u16::from_le_bytes([buf[6], buf[7]]))
        .filter(|mv| (2500..=4600).contains(mv));
    Some(Battery { percent, charging, millivolts })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_observed_reply() {
        // Captured from a Haste 2 Wireless: 98 %, not charging, 3897 mV.
        let r = [0x51, 0x02, 0x62, 0x00, 0x1a, 0x00, 0x39, 0x0f, 0x01, 0x00, 0x00, 0x00];
        assert_eq!(parse_reply(&r), Some(Battery { percent: 98, charging: false, millivolts: Some(3897) }));
        let mut charging = r;
        charging[3] = 0x02;
        assert!(parse_reply(&charging).unwrap().charging);
        // Implausible voltage word → no voltage.
        let mut no_volts = r;
        no_volts[6] = 0x00;
        no_volts[7] = 0x00;
        assert_eq!(parse_reply(&no_volts).unwrap().millivolts, None);
    }

    #[test]
    fn ignores_garbage_wrappers_and_bad_values() {
        assert_eq!(parse_reply(&[0u8; 8]), None);
        // ACK frame seen on Windows: must not become "0 %, charging".
        let wrapped = [0xff, 0x01, 0x9d, 0xb6, 0xcb, 0x70, 0xe1, 0x4e, 0x0d, 0x23, 0x53, 0x54, 0xf4, 0xad, 0x51, 0x02, 0, 0, 0, 0];
        assert_eq!(parse_reply(&wrapped), None);
        let bad = [0x51, 0x02, 200, 0x00, 0xFF];
        assert_eq!(parse_reply(&bad), None);
    }
}
