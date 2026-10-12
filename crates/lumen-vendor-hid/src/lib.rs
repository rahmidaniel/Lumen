//! Battery readout for wireless mice and keyboards whose vendors speak a
//! request/reply HID protocol: Logitech (HID++ 2.0), Glorious, ASUS ROG,
//! SteelSeries and Corsair.
//!
//! Battery reads are read-only: one vendor query per poll, strict reply
//! matching. The one writer is `logitech::write_setting` (DPI, report rate,
//! onboard profile), which only runs when the user changes a setting. The protocols come from the
//! open-source tools cited in each module (and in `docs/protocols.md`); none
//! of these devices were on hand when this was written, so every module is
//! deliberately conservative about what it accepts as an answer and reports
//! "asleep" rather than guessing.

use std::time::{Duration, Instant};

use hidapi::{DeviceInfo, HidApi, HidDevice};
use serde::Serialize;

pub mod asus;
pub mod corsair;
pub mod glorious;
pub mod logitech;
pub mod steelseries;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Battery {
    /// 0–100.
    pub percent: u8,
    pub charging: bool,
    /// Cell voltage where the protocol carries one and the value is plausible.
    pub millivolts: Option<u16>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Hid(#[from] hidapi::HidError),
    #[error("no device {vid:#06x}:{pid:#06x} attached")]
    NotFound { vid: u16, pid: u16 },
    #[error("product {0:#06x} is not in the supported list")]
    Unsupported(u16),
    #[error("device did not answer (asleep or out of range)")]
    Asleep,
    #[error("device is busy; try again")]
    Busy,
    #[error("nothing paired at that receiver slot")]
    NothingPaired,
    #[error("unexpected reply: {0}")]
    Protocol(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Attached HID collections of one product, filtered and ordered by `rank`
/// (lower first; `None` drops the collection).
pub(crate) fn collections(api: &HidApi, vid: u16, pid: u16, rank: impl Fn(&DeviceInfo) -> Option<u8>) -> Result<Vec<&DeviceInfo>> {
    let all: Vec<&DeviceInfo> = api.device_list().filter(|d| d.vendor_id() == vid && d.product_id() == pid).collect();
    if all.is_empty() {
        return Err(Error::NotFound { vid, pid });
    }
    let mut ranked: Vec<(u8, &DeviceInfo)> = all.into_iter().filter_map(|d| rank(d).map(|r| (r, d))).collect();
    ranked.sort_by_key(|(r, _)| *r);
    Ok(ranked.into_iter().map(|(_, d)| d).collect())
}

/// Read input reports for up to `window`, returning the first one `accept` turns into a value.
pub(crate) fn read_until<T>(dev: &HidDevice, window: Duration, accept: impl Fn(&[u8]) -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + window;
    while Instant::now() < deadline {
        let mut buf = [0u8; 128];
        let n = match dev.read_timeout(&mut buf, 100) {
            Ok(n) => n,
            Err(_) => return None,
        };
        if n == 0 {
            continue;
        }
        if let Some(v) = accept(&buf[..n]) {
            return Some(v);
        }
    }
    None
}

/// Discard input reports already queued (button bitmaps, notifications), so
/// the next read sees the reply to our request rather than old traffic.
pub(crate) fn drain(dev: &HidDevice) {
    let mut buf = [0u8; 128];
    for _ in 0..16 {
        match dev.read_timeout(&mut buf, 0) {
            Ok(n) if n > 0 => continue,
            _ => break,
        }
    }
}

/// Open each candidate collection in turn and run `query` until one answers.
pub(crate) fn first_answer<T>(api: &HidApi, candidates: Vec<&DeviceInfo>, query: impl Fn(&HidDevice) -> Result<T>) -> Result<T> {
    let mut last = Err(Error::Asleep);
    for info in candidates {
        let dev = match api.open_path(info.path()) {
            Ok(d) => d,
            Err(e) => {
                tracing::trace!(path = ?info.path(), %e, "cannot open collection");
                continue;
            }
        };
        match query(&dev) {
            Ok(v) => return Ok(v),
            Err(e) => {
                tracing::trace!(path = ?info.path(), %e, "collection gave no battery");
                last = Err(e);
            }
        }
    }
    last
}
