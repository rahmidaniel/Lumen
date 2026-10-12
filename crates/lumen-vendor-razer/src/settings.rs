//! Device settings: polling rate, DPI, DPI stages, sleep timer and
//! low-battery threshold. Encodings follow OpenRazer's
//! `driver/razerchromacommon.c`; which of them a product supports is not
//! listed anywhere public, so callers read first and only offer what the
//! device answers sensibly (a keyboard, for one, answers the DPI query with
//! zeros).

use crate::device::RazerDevice;
use crate::protocol::{Report, Store};
use crate::{Error, Result};

/// Classic polling-rate codes (`0x00/0x05`, `0x00/0x85`).
const POLLING: [(u16, u8); 3] = [(1000, 0x01), (500, 0x02), (125, 0x08)];
/// HyperPolling codes (`0x00/0x40`, `0x00/0xC0`): `8000 / Hz`.
const HYPER_POLLING: [(u16, u8); 7] = [(8000, 0x01), (4000, 0x02), (2000, 0x04), (1000, 0x08), (500, 0x10), (250, 0x20), (125, 0x40)];

/// Sleep timer bounds the firmware accepts, in seconds.
pub const IDLE_MIN: u16 = 60;
pub const IDLE_MAX: u16 = 900;
/// Low-battery threshold bounds, raw (`raw * 100 / 255` %): 5 % – 25 %.
pub const THRESHOLD_MIN: u8 = 0x0C;
pub const THRESHOLD_MAX: u8 = 0x3F;
/// Most stages a Razer mouse keeps.
pub const MAX_DPI_STAGES: usize = 5;

/// Which polling-rate command family a device speaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PollingKind {
    Classic,
    HyperPolling,
}

impl PollingKind {
    /// The rates a device of this kind can be set to, fastest first.
    pub fn rates(self) -> Vec<u16> {
        match self {
            PollingKind::Classic => POLLING.iter().map(|&(hz, _)| hz).collect(),
            PollingKind::HyperPolling => HYPER_POLLING.iter().map(|&(hz, _)| hz).collect(),
        }
    }
}

/// DPI stages: what the mouse button cycles through.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DpiStages {
    /// `(x, y)` per stage.
    pub stages: Vec<(u16, u16)>,
    /// 0-based index of the stage in use.
    pub active: usize,
}

pub(crate) fn polling_code(kind: PollingKind, hz: u16) -> Option<u8> {
    let table: &[(u16, u8)] = match kind {
        PollingKind::Classic => &POLLING,
        PollingKind::HyperPolling => &HYPER_POLLING,
    };
    table.iter().find(|&&(h, _)| h == hz).map(|&(_, c)| c)
}

pub(crate) fn polling_hz(kind: PollingKind, code: u8) -> Option<u16> {
    let table: &[(u16, u8)] = match kind {
        PollingKind::Classic => &POLLING,
        PollingKind::HyperPolling => &HYPER_POLLING,
    };
    table.iter().find(|&&(_, c)| c == code).map(|&(h, _)| h)
}

/// Raw low-battery threshold → percent (OpenRazer: `0x0C` = 5 %, `0x26` = 15 %, `0x3F` = 25 %).
pub fn threshold_percent(raw: u8) -> u8 {
    ((u16::from(raw) * 100 + 127) / 255) as u8
}

/// Percent → raw low-battery threshold, clamped to what the firmware accepts.
pub fn threshold_raw(percent: u8) -> u8 {
    ((u16::from(percent) * 255 / 100) as u8).clamp(THRESHOLD_MIN, THRESHOLD_MAX)
}

pub(crate) fn encode_dpi_stages(stages: &DpiStages) -> Result<Report> {
    if stages.stages.is_empty() || stages.stages.len() > MAX_DPI_STAGES || stages.active >= stages.stages.len() {
        return Err(Error::Invalid("1–5 DPI stages, with the active one among them".into()));
    }
    let mut args = vec![Store::VarStore as u8, (stages.active + 1) as u8, stages.stages.len() as u8];
    for (i, &(x, y)) in stages.stages.iter().enumerate() {
        // Stage numbers are 1-based in the device's own replies; mirror them.
        args.push((i + 1) as u8);
        args.extend_from_slice(&x.to_be_bytes());
        args.extend_from_slice(&y.to_be_bytes());
        args.extend_from_slice(&[0, 0]);
    }
    Ok(Report::new(0x04, 0x06, 0x26).with_args(&args))
}

/// `varstore, active (1-based), count, then 7-byte records: number, x, y, reserved`.
/// `None` when the device has no stages (keyboards answer with zeros).
pub(crate) fn decode_dpi_stages(args: &[u8]) -> Option<DpiStages> {
    let count = usize::from(*args.get(2)?);
    if count == 0 || count > MAX_DPI_STAGES {
        return None;
    }
    let mut stages = Vec::with_capacity(count);
    for i in 0..count {
        let o = 3 + i * 7;
        let record = args.get(o..o + 5)?;
        let (x, y) = (u16::from_be_bytes([record[1], record[2]]), u16::from_be_bytes([record[3], record[4]]));
        if x == 0 || y == 0 {
            return None;
        }
        stages.push((x, y));
    }
    let active = usize::from(args[1]).saturating_sub(1).min(count - 1);
    Some(DpiStages { stages, active })
}

impl RazerDevice {
    fn query(&self, class: u8, command: u8, size: u8, args: &[u8]) -> Result<Report> {
        self.exchange(Report::new(class, command, size).with_args(args), 20)
    }

    fn power_query(&self, class: u8, command: u8, size: u8, args: &[u8]) -> Result<Report> {
        self.exchange_with(self.power_tid(), Report::new(class, command, size).with_args(args), 20)
    }

    /// Current polling rate, trying the HyperPolling commands first (devices
    /// that have them reject the classic ones, and vice versa).
    pub fn polling_rate(&self) -> Result<(PollingKind, u16)> {
        if let Ok(r) = self.query(0x00, 0xC0, 0x02, &[]) {
            if let Some(hz) = polling_hz(PollingKind::HyperPolling, r.args[1]) {
                return Ok((PollingKind::HyperPolling, hz));
            }
        }
        let r = self.query(0x00, 0x85, 0x01, &[])?;
        polling_hz(PollingKind::Classic, r.args[0]).map(|hz| (PollingKind::Classic, hz)).ok_or(Error::Status(r.status()))
    }

    pub fn set_polling_rate(&self, kind: PollingKind, hz: u16) -> Result<()> {
        let code = polling_code(kind, hz).ok_or_else(|| Error::Invalid(format!("{hz} Hz is not a rate this device offers")))?;
        match kind {
            PollingKind::Classic => self.query(0x00, 0x05, 0x01, &[code]).map(|_| ()),
            // Synapse sends HyperPolling changes twice, with 0x00 and 0x01 in the first byte.
            PollingKind::HyperPolling => {
                self.query(0x00, 0x40, 0x02, &[0x00, code])?;
                self.query(0x00, 0x40, 0x02, &[0x01, code]).map(|_| ())
            }
        }
    }

    /// Current DPI; `None` when the device has no sensor to set.
    pub fn dpi(&self) -> Result<Option<(u16, u16)>> {
        let r = self.query(0x04, 0x85, 0x07, &[Store::VarStore as u8])?;
        let (x, y) = (u16::from_be_bytes([r.args[1], r.args[2]]), u16::from_be_bytes([r.args[3], r.args[4]]));
        Ok((x > 0 && y > 0).then_some((x, y)))
    }

    pub fn set_dpi(&self, x: u16, y: u16) -> Result<()> {
        let mut args = vec![Store::VarStore as u8];
        args.extend_from_slice(&x.to_be_bytes());
        args.extend_from_slice(&y.to_be_bytes());
        args.extend_from_slice(&[0, 0]);
        self.query(0x04, 0x05, 0x07, &args).map(|_| ())
    }

    pub fn dpi_stages(&self) -> Result<Option<DpiStages>> {
        let r = self.query(0x04, 0x86, 0x26, &[Store::VarStore as u8])?;
        Ok(decode_dpi_stages(&r.args))
    }

    pub fn set_dpi_stages(&self, stages: &DpiStages) -> Result<()> {
        let report = encode_dpi_stages(stages)?;
        self.exchange(report, 20).map(|_| ())
    }

    /// Seconds without input before a wireless device powers its radio down.
    pub fn idle_time(&self) -> Result<u16> {
        let r = self.power_query(0x07, 0x83, 0x02, &[])?;
        Ok(u16::from_be_bytes([r.args[0], r.args[1]]))
    }

    pub fn set_idle_time(&self, seconds: u16) -> Result<()> {
        let s = seconds.clamp(IDLE_MIN, IDLE_MAX);
        self.power_query(0x07, 0x03, 0x02, &s.to_be_bytes()).map(|_| ())
    }

    /// Raw threshold below which the device saves power (see [`threshold_percent`]).
    pub fn low_battery_threshold(&self) -> Result<u8> {
        Ok(self.power_query(0x07, 0x81, 0x01, &[])?.args[0])
    }

    pub fn set_low_battery_threshold(&self, raw: u8) -> Result<()> {
        self.power_query(0x07, 0x01, 0x01, &[raw.clamp(THRESHOLD_MIN, THRESHOLD_MAX)]).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polling_codes_round_trip() {
        assert_eq!(polling_code(PollingKind::Classic, 1000), Some(0x01));
        assert_eq!(polling_hz(PollingKind::Classic, 0x08), Some(125));
        assert_eq!(polling_code(PollingKind::Classic, 8000), None, "classic devices top out at 1000 Hz");
        assert_eq!(polling_code(PollingKind::HyperPolling, 4000), Some(0x02));
        assert_eq!(polling_hz(PollingKind::HyperPolling, 0x08), Some(1000));
        assert_eq!(PollingKind::Classic.rates(), [1000, 500, 125]);
    }

    #[test]
    fn threshold_scale_matches_openrazer() {
        assert_eq!(threshold_raw(5), 0x0C);
        assert_eq!(threshold_raw(15), 0x26);
        assert_eq!(threshold_raw(25), 0x3F);
        assert_eq!(threshold_raw(90), 0x3F, "clamped to what the firmware takes");
        assert_eq!(threshold_percent(0x0C), 5);
        assert_eq!(threshold_percent(0x16), 9, "the DeathStalker V2 Pro TKL ships with this");
        assert_eq!(threshold_percent(0x3F), 25);
    }

    #[test]
    fn dpi_stages_encode_and_decode() {
        let stages = DpiStages { stages: vec![(800, 800), (1800, 1800), (3200, 3200)], active: 1 };
        let report = encode_dpi_stages(&stages).unwrap();
        assert_eq!((report.class, report.command, report.data_size), (0x04, 0x06, 0x26));
        assert_eq!(&report.args[..3], &[0x01, 0x02, 0x03], "varstore, active stage 2 (1-based), 3 stages");
        assert_eq!(&report.args[3..10], &[0x01, 0x03, 0x20, 0x03, 0x20, 0x00, 0x00]);
        assert_eq!(&report.args[10..12], &[0x02, 0x07]);
        // The reply carries the same layout.
        assert_eq!(decode_dpi_stages(&report.args), Some(stages));
    }

    #[test]
    fn keyboards_have_no_dpi_stages() {
        // DeathStalker V2 Pro TKL: answers the query, with nothing in it.
        let mut args = [0u8; 80];
        args[0] = 0x01;
        assert_eq!(decode_dpi_stages(&args), None);
        assert!(encode_dpi_stages(&DpiStages { stages: vec![], active: 0 }).is_err());
        assert!(encode_dpi_stages(&DpiStages { stages: vec![(800, 800)], active: 1 }).is_err());
    }
}
