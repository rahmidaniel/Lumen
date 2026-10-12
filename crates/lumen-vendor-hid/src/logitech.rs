//! Logitech HID++ 2.0 battery readout (vendor ID 0x046D): G-series mice and
//! keyboards, on the cable or through a Lightspeed / Unifying receiver.
//!
//! Framing (Solaar `lib/logitech_receiver/base.py`, Linux
//! `hid-logitech-hidpp.c`, libratbag `hidpp20.c`, cvuchener/hidpp): a 20-byte
//! long report `11 <device index> <feature index> <function << 4 | software id>
//! <params…>`. Device index 0xFF addresses a cable-attached device, 1–7 the
//! slots of a receiver. A reply repeats bytes 2–3; `FF` in byte 2 is a
//! HID++ 2.0 error and `8F` a HID++ 1.0 error (what a receiver sends for an
//! unlinked slot: 0x09 asleep / out of range, 0x08 nothing paired), both with
//! the code in byte 5. The software id (we use 0x0C) is echoed, which is the
//! only thing tying a reply to its requester when another program is talking.
//!
//! Battery features are looked up through IRoot.getFeature (feature 0,
//! function 0), in this order:
//! * 0x1004 UNIFIED_BATTERY: getStatus → state of charge %, level bits,
//!   charging status (1 charging, 2 slow, 3 full, 4 error);
//! * 0x1000 BATTERY_STATUS: level %, next level, status (0 discharging,
//!   1/2/4 charging, 3 full); the level reads 0 while charging;
//! * 0x1001 BATTERY_VOLTAGE: millivolts big-endian plus flags (bit 7 external
//!   power, bits 0–2 charge status, bit 3/4 fast/slow charge); percent from
//!   Solaar's discharge curve.
//!
//! On Windows the short (0x10) and long (0x11) collections are separate HID
//! nodes; everything is sent as a long report and every HID++ node of the
//! device is read, because receivers answer their errors on the short one.

use std::time::{Duration, Instant};

use hidapi::{DeviceInfo, HidApi, HidDevice};

use crate::{drain, Battery, Error, Result};

pub const VENDOR_ID: u16 = 0x046D;

const SW_ID: u8 = 0x0C;
const REPORT_SHORT: u8 = 0x10;
const REPORT_LONG: u8 = 0x11;
const FEATURE_UNIFIED_BATTERY: u16 = 0x1004;
const FEATURE_BATTERY_STATUS: u16 = 0x1000;
const FEATURE_BATTERY_VOLTAGE: u16 = 0x1001;

/// Lightspeed and Unifying receivers: the battery belongs to a paired slot.
pub const RECEIVERS: &[u16] = &[0xC539, 0xC53A, 0xC53D, 0xC53F, 0xC541, 0xC543, 0xC545, 0xC547, 0xC54D, 0xC52B, 0xC532, 0xC534];

pub fn is_receiver(pid: u16) -> bool {
    RECEIVERS.contains(&pid)
}

/// Any Logitech product is worth one probe; the feature lookup says whether it has a battery.
pub fn supports(_pid: u16) -> bool {
    true
}

/// Query the battery of the attached product with `pid`: the device itself on
/// the cable, or whichever receiver slot answers a ping.
pub fn read_battery(api: &HidApi, pid: u16) -> Result<Battery> {
    let link = Link::open(api, pid)?;
    let slots: &[u8] = if is_receiver(pid) { &[1, 2, 3, 4, 5, 6, 7] } else { &[0xFF] };
    let mut asleep = false;
    let mut last = Err(Error::Unsupported(pid));
    for &index in slots {
        let first = Duration::from_millis(if index == 0xFF { 1000 } else { 2000 });
        match ping(&link, index, first) {
            Ok(()) => return battery(&link, index, pid),
            Err(Error::NothingPaired) => continue,
            Err(Error::Asleep) | Err(Error::Busy) => asleep = true,
            Err(e) => last = Err(e),
        }
    }
    if asleep {
        Err(Error::Asleep)
    } else {
        last
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Node {
    Long,
    Short,
}

/// Which HID++ collection a HID node is, if any. Legacy page 0xFF00 puts the
/// report flag in the usage (1 short, 2 long, 4 very long); page 0xFF43 keeps
/// it in the low byte.
fn node_kind(d: &DeviceInfo) -> Option<Node> {
    let flag = match d.usage_page() {
        0xFF00 => d.usage(),
        0xFF43 => d.usage() & 0xFF,
        _ => return None,
    };
    match flag {
        0x02 => Some(Node::Long),
        0x01 => Some(Node::Short),
        _ => None,
    }
}

struct Link {
    devs: Vec<HidDevice>,
    writer: usize,
}

impl Link {
    fn open(api: &HidApi, pid: u16) -> Result<Link> {
        let mut seen = false;
        let mut long: Vec<&DeviceInfo> = Vec::new();
        let mut short: Vec<&DeviceInfo> = Vec::new();
        for d in api.device_list().filter(|d| d.vendor_id() == VENDOR_ID && d.product_id() == pid) {
            seen = true;
            match node_kind(d) {
                Some(Node::Long) => long.push(d),
                Some(Node::Short) => short.push(d),
                None => {}
            }
        }
        if !seen {
            return Err(Error::NotFound { vid: VENDOR_ID, pid });
        }
        // Platforms other than Windows expose one node per interface whose
        // reported usage is its first collection (the short one), so a short
        // node may be the only way in; it still accepts long reports.
        let mut devs = Vec::new();
        let mut writer = None;
        for (kind, info) in long.iter().map(|d| (Node::Long, *d)).chain(short.iter().map(|d| (Node::Short, *d))) {
            match api.open_path(info.path()) {
                Ok(dev) => {
                    if writer.is_none() || (kind == Node::Long && writer.map(|w| w != 0).unwrap_or(false)) {
                        writer = Some(devs.len());
                    }
                    devs.push(dev);
                }
                Err(e) => tracing::trace!(path = ?info.path(), %e, "cannot open HID++ node"),
            }
        }
        match writer {
            Some(writer) if !devs.is_empty() => Ok(Link { devs, writer }),
            _ => Err(Error::Unsupported(pid)),
        }
    }

    /// One HID++ 2.0 request/reply exchange on `index`.
    fn request(&self, index: u8, feature: u8, function: u8, params: &[u8], timeout: Duration) -> Result<[u8; 16]> {
        let req = frame(index, feature, function, params);
        for attempt in 0..2 {
            for d in &self.devs {
                drain(d);
            }
            self.devs[self.writer].write(&req)?;
            let deadline = Instant::now() + timeout;
            while Instant::now() < deadline {
                for d in &self.devs {
                    let mut buf = [0u8; 64];
                    let n = d.read_timeout(&mut buf, 20).unwrap_or(0);
                    if n == 0 {
                        continue;
                    }
                    match classify(&buf[..n], &req) {
                        Reply::Answer(p) => return Ok(p),
                        Reply::Error { hidpp1, code } => match map_error(hidpp1, code) {
                            Error::Busy if attempt == 0 => {
                                std::thread::sleep(Duration::from_millis(50));
                                break;
                            }
                            e => return Err(e),
                        },
                        Reply::Unrelated => {}
                    }
                }
            }
        }
        Err(Error::Asleep)
    }
}

/// Build a long request frame.
pub(crate) fn frame(index: u8, feature: u8, function: u8, params: &[u8]) -> [u8; 20] {
    let mut req = [0u8; 20];
    req[0] = REPORT_LONG;
    req[1] = index;
    req[2] = feature;
    req[3] = (function << 4) | SW_ID;
    let n = params.len().min(16);
    req[4..4 + n].copy_from_slice(&params[..n]);
    req
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Reply {
    Answer([u8; 16]),
    Error { hidpp1: bool, code: u8 },
    Unrelated,
}

/// Match an incoming report against `req` (bytes 2–3, device index, software id).
pub(crate) fn classify(r: &[u8], req: &[u8; 20]) -> Reply {
    if r.len() < 6 || !matches!(r[0], REPORT_SHORT | REPORT_LONG) {
        return Reply::Unrelated;
    }
    // Bluetooth devices answer a 0xFF request with index 0x00.
    if !(r[1] == req[1] || (req[1] == 0xFF && r[1] == 0x00)) {
        return Reply::Unrelated;
    }
    if r[2] == req[2] && r[3] == req[3] {
        let mut p = [0u8; 16];
        let n = (r.len() - 4).min(16);
        p[..n].copy_from_slice(&r[4..4 + n]);
        return Reply::Answer(p);
    }
    if matches!(r[2], 0xFF | 0x8F) && r[3] == req[2] && r[4] == req[3] {
        return Reply::Error { hidpp1: r[2] == 0x8F, code: r[5] };
    }
    Reply::Unrelated
}

pub(crate) fn map_error(hidpp1: bool, code: u8) -> Error {
    match (hidpp1, code) {
        (true, 0x08) => Error::NothingPaired,
        (true, 0x09) | (true, 0x04) => Error::Asleep,
        (true, 0x07) | (false, 0x08) => Error::Busy,
        (true, 0x01) => Error::Protocol("HID++ 1.0 device".into()),
        (false, 0x06) | (false, 0x07) | (false, 0x09) => Error::Protocol(format!("feature or function unsupported ({code:#04x})")),
        _ => Error::Protocol(format!("HID++ {} error {code:#04x}", if hidpp1 { "1.0" } else { "2.0" })),
    }
}

/// IRoot.ping: proves the slot is linked and speaks HID++ 2.0.
fn ping(link: &Link, index: u8, timeout: Duration) -> Result<()> {
    let p = link.request(index, 0x00, 0x01, &[0x00, 0x00, 0x5A], timeout)?;
    if p[2] == 0x5A {
        Ok(())
    } else {
        Err(Error::Protocol("ping byte not echoed".into()))
    }
}

/// IRoot.getFeature → feature index, `None` when the device lacks it.
fn get_feature(link: &Link, index: u8, feature: u16) -> Result<Option<u8>> {
    let p = link.request(index, 0x00, 0x00, &feature.to_be_bytes(), Duration::from_millis(500))?;
    Ok((p[0] != 0).then_some(p[0]))
}

fn battery(link: &Link, index: u8, pid: u16) -> Result<Battery> {
    let t = Duration::from_millis(500);
    if let Some(f) = get_feature(link, index, FEATURE_UNIFIED_BATTERY)? {
        let caps = link.request(index, f, 0, &[], t)?;
        let status = link.request(index, f, 1, &[], t)?;
        return unified_battery(caps[1], status);
    }
    if let Some(f) = get_feature(link, index, FEATURE_BATTERY_STATUS)? {
        return battery_status(link.request(index, f, 0, &[], t)?);
    }
    if let Some(f) = get_feature(link, index, FEATURE_BATTERY_VOLTAGE)? {
        return battery_voltage(link.request(index, f, 0, &[], t)?);
    }
    Err(Error::Unsupported(pid))
}

/// 0x1004 getStatus: `[state of charge, level bits, charging status, external power]`.
pub(crate) fn unified_battery(capability_flags: u8, st: [u8; 16]) -> Result<Battery> {
    let (soc, level_bits, charging_status) = (st[0], st[1], st[2]);
    let mut percent = if capability_flags & 0x02 != 0 {
        soc
    } else if level_bits & 0x08 != 0 {
        90
    } else if level_bits & 0x04 != 0 {
        50
    } else if level_bits & 0x02 != 0 {
        20
    } else {
        5
    };
    match charging_status {
        4 => return Err(Error::Protocol("charging error".into())),
        3 if percent == 0 => percent = 100,
        _ => {}
    }
    if percent > 100 {
        return Err(Error::Protocol(format!("state of charge {soc}")));
    }
    Ok(Battery { percent, charging: (1..=3).contains(&charging_status), millivolts: None })
}

/// 0x1000 getBatteryLevelStatus: `[level %, next level %, status]`.
pub(crate) fn battery_status(st: [u8; 16]) -> Result<Battery> {
    let (level, status) = (st[0], st[2]);
    if level > 100 {
        return Err(Error::Protocol(format!("battery level {level}")));
    }
    match status {
        0 => Ok(Battery { percent: level, charging: false, millivolts: None }),
        3 => Ok(Battery { percent: 100, charging: true, millivolts: None }),
        1 | 2 | 4 if level > 0 => Ok(Battery { percent: level, charging: true, millivolts: None }),
        1 | 2 | 4 => Err(Error::Protocol("level not reported while charging".into())),
        other => Err(Error::Protocol(format!("battery status {other}"))),
    }
}

/// 0x1001 getBatteryVoltage: `[mV hi, mV lo, flags]`.
pub(crate) fn battery_voltage(v: [u8; 16]) -> Result<Battery> {
    let mv = u16::from_be_bytes([v[0], v[1]]);
    let flags = v[2];
    if !(2500..=4600).contains(&mv) {
        return Err(Error::Protocol(format!("voltage {mv} mV")));
    }
    let external = flags & 0x80 != 0;
    let full = external && flags & 0x07 == 1;
    let charging = external && (flags & 0x07 == 0 || flags & 0x18 != 0);
    let percent = if full { 100 } else { voltage_percent(mv) };
    Ok(Battery { percent, charging: charging || full, millivolts: Some(mv) })
}

/// Solaar's discharge curve for Logitech's Li-Po cells (linear between points).
pub(crate) fn voltage_percent(mv: u16) -> u8 {
    const CURVE: [(u16, u8); 13] = [
        (4186, 100),
        (4067, 90),
        (3989, 80),
        (3922, 70),
        (3859, 60),
        (3811, 50),
        (3778, 40),
        (3751, 30),
        (3717, 20),
        (3671, 10),
        (3646, 5),
        (3579, 2),
        (3500, 0),
    ];
    if mv >= CURVE[0].0 {
        return 100;
    }
    for w in CURVE.windows(2) {
        let (hi, lo) = (w[0], w[1]);
        if mv >= lo.0 {
            let t = (mv - lo.0) as f32 / (hi.0 - lo.0) as f32;
            return (lo.1 as f32 + t * (hi.1 as f32 - lo.1 as f32)).round() as u8;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_and_reply_matching() {
        let req = frame(0xFF, 0x00, 0x00, &[0x10, 0x04]);
        assert_eq!(&req[..6], &[0x11, 0xFF, 0x00, 0x0C, 0x10, 0x04]);
        // getFeature answer: feature index 5, type 0, version 1.
        let ok = [0x11, 0xFF, 0x00, 0x0C, 0x05, 0x00, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        assert!(matches!(classify(&ok, &req), Reply::Answer(p) if p[0] == 5));
        // Bluetooth devices answer with index 0.
        let mut bt = ok;
        bt[1] = 0x00;
        assert!(matches!(classify(&bt, &req), Reply::Answer(_)));
        // Someone else's software id, or an event (sw id 0), is not ours.
        let mut other = ok;
        other[3] = 0x07;
        assert_eq!(classify(&other, &req), Reply::Unrelated);
        // HID++ 1.0 error from a receiver for an asleep slot.
        let req1 = frame(0x01, 0x00, 0x01, &[0, 0, 0x5A]);
        let err = [0x10, 0x01, 0x8F, 0x00, 0x1C, 0x09, 0x00];
        assert_eq!(classify(&err, &req1), Reply::Error { hidpp1: true, code: 0x09 });
        assert!(matches!(map_error(true, 0x09), Error::Asleep));
        assert!(matches!(map_error(true, 0x08), Error::NothingPaired));
        assert!(matches!(map_error(false, 0x08), Error::Busy));
        // HID++ 2.0 error.
        let err2 = [0x11, 0x01, 0xFF, 0x00, 0x1C, 0x06, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(classify(&err2, &req1), Reply::Error { hidpp1: false, code: 0x06 });
        assert_eq!(classify(&[0x20, 0x01, 0x00, 0x1C, 0, 0], &req1), Reply::Unrelated);
    }

    #[test]
    fn battery_feature_decoding() {
        let mut st = [0u8; 16];
        st[0] = 71;
        st[1] = 0x04;
        st[2] = 0;
        assert_eq!(unified_battery(0x03, st).unwrap(), Battery { percent: 71, charging: false, millivolts: None });
        st[2] = 1;
        assert!(unified_battery(0x03, st).unwrap().charging);
        // No state-of-charge support: level bits only.
        st[0] = 0;
        st[1] = 0x08;
        assert_eq!(unified_battery(0x01, st).unwrap().percent, 90);
        st[2] = 3;
        st[1] = 0;
        assert_eq!(unified_battery(0x03, st).unwrap(), Battery { percent: 100, charging: true, millivolts: None });
        st[2] = 4;
        assert!(unified_battery(0x03, st).is_err());

        let mut bs = [0u8; 16];
        bs[0] = 50;
        bs[1] = 30;
        bs[2] = 0;
        assert_eq!(battery_status(bs).unwrap(), Battery { percent: 50, charging: false, millivolts: None });
        bs[0] = 0;
        bs[2] = 1;
        assert!(battery_status(bs).is_err(), "level unknown while charging");
        bs[2] = 3;
        assert_eq!(battery_status(bs).unwrap().percent, 100);

        let mut bv = [0u8; 16];
        bv[0] = 0x0E;
        bv[1] = 0xE3; // 3811 mV
        bv[2] = 0x00;
        assert_eq!(battery_voltage(bv).unwrap(), Battery { percent: 50, charging: false, millivolts: Some(3811) });
        bv[2] = 0x80; // external power, charging
        assert!(battery_voltage(bv).unwrap().charging);
        bv[2] = 0x81; // external power, charge complete
        assert_eq!(battery_voltage(bv).unwrap().percent, 100);
        bv[2] = 0x82; // external power, charge stopped
        assert!(!battery_voltage(bv).unwrap().charging);
    }

    #[test]
    fn voltage_curve() {
        assert_eq!(voltage_percent(4200), 100);
        assert_eq!(voltage_percent(4186), 100);
        assert_eq!(voltage_percent(3811), 50);
        assert_eq!(voltage_percent(3900), 67);
        assert_eq!(voltage_percent(3500), 0);
        assert_eq!(voltage_percent(3400), 0);
    }
}
