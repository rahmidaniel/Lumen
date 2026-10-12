//! Polling rate and DPI stages of the Pulsefire Haste 2 (wired `0x0B97`,
//! wireless `0x0F98`), after https://github.com/fspy/haste2ctl.
//!
//! `32 02` on the vendor interface is answered by `33 02 RR MM AA` and five
//! 5-byte stage records (`u16 LE code, R, G, B`): `RR` is a polling divisor
//! (`Hz = 8000 / RR`), `MM` the bit mask of enabled stages, `AA` the active
//! stage, a stage's DPI is `(code + 1) * 50`. A write is the same table
//! behind `32 01 00 00`, acknowledged by an `ff 01 …` frame whose bytes 14–16
//! are `32 01 <result>` (0 = accepted). Writes are read-modify-write so the
//! stage colours and reserved bytes survive, and they only change the live
//! profile: the mouse falls back to its own after a power cycle, which is why
//! Lumen re-applies them.

use std::time::{Duration, Instant};

use hidapi::{HidApi, HidDevice};

use crate::{Error, Result, VENDOR_ID};

pub const MAX_STAGES: usize = 5;
pub const DPI_MIN: u32 = 50;
pub const DPI_MAX: u32 = 26_000;
pub const DPI_STEP: u32 = 50;

const RR: usize = 2;
const MASK: usize = 3;
const ACTIVE: usize = 4;
const STAGES: usize = 5;
const RECORD: usize = 5;

/// Polling rates and their divisors.
const DIVISORS: [(u32, u8); 7] = [(125, 0x40), (250, 0x20), (500, 0x10), (1000, 0x08), (2000, 0x04), (4000, 0x02), (8000, 0x01)];

/// Rates a model can be set to: the wired mouse reaches 8 kHz, the wireless
/// one is held to what its 2.4 GHz link carries.
pub fn polling_rates(pid: u16) -> &'static [u32] {
    if pid == crate::pid::PULSEFIRE_HASTE_2_WIRELESS {
        &[125, 250, 500, 1000]
    } else {
        &[125, 250, 500, 1000, 2000, 4000, 8000]
    }
}

/// Whether a product has this table.
pub fn supports(pid: u16) -> bool {
    matches!(pid, crate::pid::PULSEFIRE_HASTE_2_WIRELESS | crate::pid::PULSEFIRE_HASTE_2_WIRED)
}

/// The mouse's `33 02` table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    raw: [u8; 64],
}

impl Config {
    pub fn from_reply(reply: &[u8]) -> Option<Config> {
        if reply.len() < STAGES + MAX_STAGES * RECORD || reply[0] != 0x33 || reply[1] != 0x02 || reply[RR] == 0 {
            return None;
        }
        let mut raw = [0u8; 64];
        let n = reply.len().min(64);
        raw[..n].copy_from_slice(&reply[..n]);
        Some(Config { raw })
    }

    pub fn polling_hz(&self) -> u32 {
        8000 / u32::from(self.raw[RR].max(1))
    }

    pub fn set_polling_hz(&mut self, hz: u32) -> Result<()> {
        let divisor = DIVISORS.iter().find(|&&(h, _)| h == hz).map(|&(_, d)| d).ok_or_else(|| Error::Invalid(format!("{hz} Hz is not a polling rate this mouse knows")))?;
        self.raw[RR] = divisor;
        Ok(())
    }

    fn code(&self, stage: usize) -> u16 {
        let o = STAGES + stage * RECORD;
        u16::from_le_bytes([self.raw[o], self.raw[o + 1]])
    }

    /// DPI of every enabled stage, in order.
    pub fn stages(&self) -> Vec<u32> {
        (0..MAX_STAGES).filter(|s| (self.raw[MASK] >> s) & 1 == 1).map(|s| (u32::from(self.code(s)) + 1) * DPI_STEP).collect()
    }

    /// Index of the active stage among the enabled ones.
    pub fn active(&self) -> usize {
        let active = usize::from(self.raw[ACTIVE]);
        (0..active).filter(|s| (self.raw[MASK] >> s) & 1 == 1).count().min(self.stages().len().saturating_sub(1))
    }

    /// Replace the stages (1–5, multiples of 50 between 50 and 26 000 DPI)
    /// and pick the active one. Unused records keep their values.
    pub fn set_stages(&mut self, dpis: &[u32], active: usize) -> Result<()> {
        if dpis.is_empty() || dpis.len() > MAX_STAGES || active >= dpis.len() {
            return Err(Error::Invalid(format!("1–{MAX_STAGES} stages, with the active one among them")));
        }
        for &dpi in dpis {
            if !(DPI_MIN..=DPI_MAX).contains(&dpi) || dpi % DPI_STEP != 0 {
                return Err(Error::Invalid(format!("{dpi} DPI: use a multiple of {DPI_STEP} from {DPI_MIN} to {DPI_MAX}")));
            }
        }
        for (s, &dpi) in dpis.iter().enumerate() {
            let o = STAGES + s * RECORD;
            let code = (dpi / DPI_STEP - 1) as u16;
            self.raw[o..o + 2].copy_from_slice(&code.to_le_bytes());
        }
        self.raw[MASK] = (1u8 << dpis.len()) - 1;
        self.raw[ACTIVE] = active as u8;
        Ok(())
    }

    /// The `32 01` write carrying this table.
    pub fn write_report(&self) -> [u8; 64] {
        let mut out = [0u8; 64];
        out[..4].copy_from_slice(&[0x32, 0x01, 0x00, 0x00]);
        out[4..64].copy_from_slice(&self.raw[2..62]);
        out
    }

    /// Same stages and polling rate (the active stage is the mouse button's business).
    pub fn same_settings(&self, other: &Config) -> bool {
        self.polling_hz() == other.polling_hz() && self.stages() == other.stages()
    }
}

/// Open the vendor interface(s) of `pid`, best first.
fn interfaces(api: &HidApi, pid: u16) -> Result<Vec<HidDevice>> {
    let mut infos: Vec<_> = api.device_list().filter(|d| d.vendor_id() == VENDOR_ID && d.product_id() == pid && d.usage_page() >= 0xFF00).collect();
    if infos.is_empty() {
        return Err(Error::NotFound(pid));
    }
    infos.sort_by_key(|d| if d.interface_number() == 2 { 0 } else { 1 });
    Ok(infos.into_iter().filter_map(|d| api.open_path(d.path()).ok()).collect())
}

/// Wait for a report matching `want`, discarding the receiver's chatter.
fn await_report(dev: &HidDevice, timeout: Duration, want: impl Fn(&[u8]) -> bool) -> Option<Vec<u8>> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let mut buf = [0u8; 128];
        let n = dev.read_timeout(&mut buf, 50).unwrap_or(0);
        if n > 0 && want(&buf[..n]) {
            return Some(buf[..n].to_vec());
        }
    }
    None
}

/// Ask for the table; the receiver occasionally drops a request, so ask twice.
fn request(dev: &HidDevice) -> Option<Config> {
    let mut req = [0u8; 64];
    req[0] = 0x32;
    req[1] = 0x02;
    for _ in 0..2 {
        dev.write(&req).ok()?;
        if let Some(reply) = await_report(dev, Duration::from_millis(500), |r| r.len() > 2 && r[0] == 0x33 && r[1] == 0x02) {
            return Config::from_reply(&reply);
        }
    }
    None
}

/// Read the polling rate and DPI table of the first attached `pid`.
pub fn read_config(api: &HidApi, pid: u16) -> Result<Config> {
    for dev in interfaces(api, pid)? {
        if let Some(config) = request(&dev) {
            return Ok(config);
        }
    }
    Err(Error::NoReply(pid))
}

/// Write `config` and read it back; the mouse must hold what was asked.
pub fn write_config(api: &HidApi, pid: u16, config: &Config) -> Result<Config> {
    for dev in interfaces(api, pid)? {
        if request(&dev).is_none() {
            continue; // not the interface that answers
        }
        dev.write(&config.write_report())?;
        let ack = await_report(&dev, Duration::from_millis(600), |r| r.len() > 16 && r[0] == 0xFF && r[14] == 0x32 && r[15] == 0x01);
        if let Some(code) = ack.map(|a| a[16]).filter(|&c| c != 0) {
            return Err(Error::Invalid(format!("the mouse refused the change (code {code:#04x})")));
        }
        let back = request(&dev).ok_or(Error::NoReply(pid))?;
        return if back.same_settings(config) { Ok(back) } else { Err(Error::Invalid("the mouse did not keep the new settings".into())) };
    }
    Err(Error::NoReply(pid))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Read from a Pulsefire Haste 2 Wireless on 2026-10-12.
    const OBSERVED: [u8; 30] = [
        0x33, 0x02, 0x08, 0x0f, 0x01, 0x07, 0x00, 0xff, 0x00, 0x00, 0x0f, 0x00, 0x00, 0x00, 0xff, 0x1f, 0x00, 0xff, 0xff, 0x00, 0x3f, 0x00, 0x00, 0xff,
        0x00, 0x07, 0x02, 0xff, 0xff, 0xff,
    ];

    fn observed() -> Config {
        let mut reply = [0u8; 64];
        reply[..OBSERVED.len()].copy_from_slice(&OBSERVED);
        Config::from_reply(&reply).unwrap()
    }

    #[test]
    fn decodes_the_observed_table() {
        let c = observed();
        assert_eq!(c.polling_hz(), 1000);
        assert_eq!(c.stages(), [400, 800, 1600, 3200], "the fifth record (26 000) is disabled");
        assert_eq!(c.active(), 1);
    }

    #[test]
    fn edits_keep_everything_else() {
        let mut c = observed();
        c.set_polling_hz(500).unwrap();
        c.set_stages(&[800, 1200, 2400], 2).unwrap();
        assert_eq!(c.polling_hz(), 500);
        assert_eq!(c.stages(), [800, 1200, 2400]);
        assert_eq!(c.active(), 2);
        let w = c.write_report();
        assert_eq!(&w[..4], &[0x32, 0x01, 0x00, 0x00]);
        assert_eq!(&w[4..7], &[0x10, 0x07, 0x02], "divisor, mask for three stages, active");
        assert_eq!(&w[7..12], &[0x0f, 0x00, 0xff, 0x00, 0x00], "stage 1: 800 DPI, colour kept");
        assert_eq!(&w[22..27], &[0x3f, 0x00, 0x00, 0xff, 0x00], "stage 4 keeps its record while disabled");
    }

    #[test]
    fn rejects_values_the_mouse_cannot_hold() {
        let mut c = observed();
        assert!(c.set_polling_hz(300).is_err());
        assert!(c.set_stages(&[825], 0).is_err(), "not a multiple of 50");
        assert!(c.set_stages(&[30_000], 0).is_err());
        assert!(c.set_stages(&[], 0).is_err());
        assert!(c.set_stages(&[400, 800], 2).is_err());
        assert!(Config::from_reply(&[0x51, 0x02, 0x62]).is_none());
        assert_eq!(polling_rates(crate::pid::PULSEFIRE_HASTE_2_WIRELESS).last(), Some(&1000));
    }
}
