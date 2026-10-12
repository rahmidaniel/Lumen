//! Opening a Razer device and exchanging reports with it.
//!
//! A Razer product exposes several HID interfaces; only one accepts the 90-byte
//! control report. We find it by probing: send a harmless firmware-version
//! query on each interface (and each plausible transaction ID) until one
//! answers. Replies are matched on transaction ID, class and command because
//! other software (OpenRGB, Synapse) may be talking to the device at the same
//! time and a `GetFeature` can return *their* reply.

use std::ffi::CString;
use std::thread::sleep;
use std::time::Duration;

use hidapi::{HidApi, HidDevice};
use serde::Serialize;

use crate::protocol::{self, Effect, Report, Status, Store, REPORT_LEN};
use crate::{pid, Error, Result, VENDOR_ID};

/// One physical Razer product, as seen through its HID interfaces.
#[derive(Clone, Debug, Serialize)]
pub struct RazerInfo {
    pub pid: u16,
    pub product: String,
    pub serial: Option<String>,
    pub interfaces: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Battery {
    /// 0–100.
    pub percent: u8,
    pub charging: bool,
}

pub struct RazerDevice {
    hid: HidDevice,
    pid: u16,
    transaction_id: u8,
}

/// Per-product transaction IDs. Most devices use one ID for everything, but
/// OpenRazer shows the wired DeathStalker V2 Pro answering power queries on
/// 0x1F and effect writes on 0x3F, and the wireless receivers using 0x9F.
#[derive(Clone, Copy, Debug)]
struct Quirks {
    /// IDs to try when probing, most likely first.
    probe: &'static [u8],
    /// Override for class 0x07 (battery) and device-mode commands.
    power: Option<u8>,
    /// Override for extended-matrix effect writes.
    effect: Option<u8>,
}

fn quirks(pid: u16) -> Quirks {
    match pid {
        pid::STRIDER_CHROMA => Quirks { probe: &[0x3F], power: None, effect: None },
        pid::DEATHSTALKER_V2_PRO_WIRELESS | pid::DEATHSTALKER_V2_PRO_TKL_WIRELESS => {
            Quirks { probe: &[0x9F, 0x1F, 0x3F], power: Some(0x9F), effect: Some(0x9F) }
        }
        pid::DEATHSTALKER_V2_PRO_WIRED | pid::DEATHSTALKER_V2_PRO_TKL_WIRED => {
            Quirks { probe: &[0x1F, 0x3F], power: Some(0x1F), effect: Some(0x3F) }
        }
        _ => Quirks { probe: &[0x3F, 0x1F, 0x9F, 0xFF], power: None, effect: None },
    }
}

impl RazerDevice {
    /// Every Razer product currently attached (deduplicated by PID + serial).
    pub fn list(api: &HidApi) -> Vec<RazerInfo> {
        let mut out: Vec<RazerInfo> = Vec::new();
        for info in api.device_list().filter(|d| d.vendor_id() == VENDOR_ID) {
            let serial = info.serial_number().map(str::to_owned).filter(|s| !s.is_empty());
            if let Some(existing) = out.iter_mut().find(|r| r.pid == info.product_id() && r.serial == serial) {
                existing.interfaces += 1;
                continue;
            }
            out.push(RazerInfo {
                pid: info.product_id(),
                product: info.product_string().unwrap_or("").to_owned(),
                serial,
                interfaces: 1,
            });
        }
        out
    }

    /// Open the control interface of the first attached product with `pid`.
    pub fn open(api: &HidApi, pid: u16) -> Result<RazerDevice> {
        let paths: Vec<CString> = api
            .device_list()
            .filter(|d| d.vendor_id() == VENDOR_ID && d.product_id() == pid)
            .map(|d| d.path().to_owned())
            .collect();
        if paths.is_empty() {
            return Err(Error::NotFound(pid));
        }
        for path in &paths {
            let hid = match api.open_path(path) {
                Ok(h) => h,
                Err(e) => {
                    tracing::debug!(?path, %e, "cannot open interface");
                    continue;
                }
            };
            let mut dev = RazerDevice { hid, pid, transaction_id: 0 };
            for &tid in quirks(pid).probe {
                dev.transaction_id = tid;
                match dev.exchange(protocol::get_firmware_version(), 2) {
                    Ok(_) => {
                        tracing::info!(pid = format_args!("{pid:#06x}"), tid = format_args!("{tid:#04x}"), "Razer control interface found");
                        return Ok(dev);
                    }
                    Err(e) => tracing::trace!(?path, tid = format_args!("{tid:#04x}"), %e, "probe failed"),
                }
            }
        }
        Err(Error::NoControlInterface(pid))
    }

    pub fn pid(&self) -> u16 {
        self.pid
    }

    pub fn transaction_id(&self) -> u8 {
        self.transaction_id
    }

    /// Send `report` with the probed transaction ID and wait for the reply.
    pub fn exchange(&self, report: Report, attempts: usize) -> Result<Report> {
        self.exchange_with(self.transaction_id, report, attempts)
    }

    /// Send `report` with an explicit transaction ID, re-sending up to
    /// `attempts` times. After each send the reply is polled a few times with
    /// growing delays: wired devices answer within ~1 ms, wireless dongles
    /// (OpenRazer waits 4.9 ms for the DeathStalker V2) need noticeably longer.
    pub fn exchange_with(&self, transaction_id: u8, mut report: Report, attempts: usize) -> Result<Report> {
        report.transaction_id = transaction_id;
        let encoded = report.encode();
        let mut out = [0u8; REPORT_LEN + 1]; // leading report ID byte (0)
        out[1..].copy_from_slice(&encoded);

        let mut pending: Option<Status> = None;
        for _ in 0..attempts.max(1) {
            self.hid.send_feature_report(&out)?;
            for poll in 0..5u64 {
                sleep(Duration::from_micros(1200 + poll * 2200));
                let mut buf = [0u8; REPORT_LEN + 1];
                self.hid.get_feature_report(&mut buf)?;
                let Some(reply) = Report::decode(&buf[1..]) else { continue };
                let ours = reply.transaction_id == transaction_id && reply.class == report.class && reply.command == report.command;
                if !ours {
                    continue;
                }
                match reply.status() {
                    Status::Ok => return Ok(reply),
                    status @ (Status::Busy | Status::New) => pending = Some(status),
                    other => return Err(Error::Status(other)),
                }
            }
        }
        Err(pending.map(Error::Status).unwrap_or(Error::NoReply))
    }

    pub fn firmware_version(&self) -> Result<(u8, u8)> {
        let r = self.exchange(protocol::get_firmware_version(), 20)?;
        Ok((r.args[0], r.args[1]))
    }

    pub fn serial(&self) -> Result<String> {
        let r = self.exchange(protocol::get_serial(), 20)?;
        let end = r.args[..22].iter().position(|&b| b == 0).unwrap_or(22);
        Ok(String::from_utf8_lossy(&r.args[..end]).trim().to_owned())
    }

    pub(crate) fn power_tid(&self) -> u8 {
        quirks(self.pid).power.unwrap_or(self.transaction_id)
    }

    fn effect_tid(&self) -> u8 {
        quirks(self.pid).effect.unwrap_or(self.transaction_id)
    }

    pub fn battery(&self) -> Result<Battery> {
        let tid = self.power_tid();
        let level = self.exchange_with(tid, protocol::get_battery_level(), 20)?.args[1];
        let charging = self.exchange_with(tid, protocol::get_charging_status(), 20)?.args[1] != 0;
        Ok(Battery { percent: ((level as u32 * 100 + 127) / 255) as u8, charging })
    }

    /// `0x00` normal, `0x03` driver mode.
    pub fn device_mode(&self) -> Result<u8> {
        Ok(self.exchange_with(self.power_tid(), protocol::get_device_mode(), 20)?.args[0])
    }

    pub fn set_device_mode(&self, mode: u8) -> Result<()> {
        self.exchange_with(self.power_tid(), protocol::set_device_mode(mode), 20).map(|_| ())
    }

    /// Run a built-in effect; with [`Store::VarStore`] the device also keeps it
    /// as its power-on effect (verified on the Strider Chroma). Make sure
    /// nothing is streaming frames to the device while this is sent.
    pub fn set_effect(&self, store: Store, effect: Effect) -> Result<()> {
        self.exchange_with(self.effect_tid(), protocol::extended_matrix_effect(store, 0, effect), 20).map(|_| ())
    }

    pub fn set_brightness(&self, store: Store, value: u8) -> Result<()> {
        self.exchange_with(self.effect_tid(), protocol::extended_matrix_brightness(store, 0, value), 20).map(|_| ())
    }
}
