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

pub const VENDOR_ID: u16 = 0x03F0;

pub mod pid {
    pub const PULSEFIRE_HASTE_2_WIRELESS: u16 = 0x0F98;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Battery {
    /// 0–100.
    pub percent: u8,
    pub charging: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Hid(#[from] hidapi::HidError),
    #[error("no HyperX device with product ID {0:#06x}")]
    NotFound(u16),
    #[error("no interface of {0:#06x} answered the battery query (mouse asleep?)")]
    NoReply(u16),
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

/// Parse a `51 02 <percent> <phase> <source>` report.
///
/// The header must start the report: the receiver also emits wrapped status
/// packets (`ff 01 <12-byte id> 51 02 00 00 …`) that merely echo the command
/// with zeroed data, and matching those would report a phantom 0 %.
fn parse_reply(buf: &[u8]) -> Option<Battery> {
    if buf.len() < 5 || buf[0] != 0x51 || buf[1] != 0x02 {
        return None;
    }
    let percent = buf[2];
    if percent > 100 {
        return None;
    }
    let charging = match buf[4] {
        0x00 => true,  // on the cable
        0xFF => false, // wireless
        _ => matches!(buf[3], 0x01 | 0x02),
    };
    Some(Battery { percent, charging })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_active_reply() {
        let mut r = [0u8; 64];
        r[0] = 0x51;
        r[1] = 0x02;
        r[2] = 87;
        r[3] = 0x00;
        r[4] = 0xFF;
        assert_eq!(parse_reply(&r), Some(Battery { percent: 87, charging: false }));
        r[4] = 0x00;
        assert_eq!(parse_reply(&r), Some(Battery { percent: 87, charging: true }));
        r[4] = 0x10;
        r[3] = 0x01;
        assert_eq!(parse_reply(&r), Some(Battery { percent: 87, charging: true }));
    }

    #[test]
    fn ignores_garbage_wrappers_and_bad_values() {
        assert_eq!(parse_reply(&[0u8; 8]), None);
        // Wrapped echo seen on Windows: must not become "0 %, charging".
        let wrapped = [0xff, 0x01, 0x9d, 0xb6, 0xcb, 0x70, 0xe1, 0x4e, 0x0d, 0x23, 0x53, 0x54, 0xf4, 0xad, 0x51, 0x02, 0, 0, 0, 0];
        assert_eq!(parse_reply(&wrapped), None);
        let bad = [0x51, 0x02, 200, 0x00, 0xFF];
        assert_eq!(parse_reply(&bad), None);
    }
}
