//! The 90-byte Razer report and the handful of commands Lumen uses.

use serde::{Deserialize, Serialize};

pub const REPORT_LEN: usize = 90;

/// Reply status in byte 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    New,
    Busy,
    Ok,
    Fail,
    Timeout,
    Unsupported,
    Unknown(u8),
}

impl From<u8> for Status {
    fn from(v: u8) -> Self {
        match v {
            0 => Status::New,
            1 => Status::Busy,
            2 => Status::Ok,
            3 => Status::Fail,
            4 => Status::Timeout,
            5 => Status::Unsupported,
            other => Status::Unknown(other),
        }
    }
}

/// Where an effect write lands: RAM only, or the flash the device boots from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Store {
    NoStore = 0x00,
    VarStore = 0x01,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub status: u8,
    pub transaction_id: u8,
    pub remaining_packets: u16,
    pub protocol_type: u8,
    pub data_size: u8,
    pub class: u8,
    pub command: u8,
    pub args: [u8; 80],
}

impl Report {
    pub fn new(class: u8, command: u8, data_size: u8) -> Self {
        Self { status: 0, transaction_id: 0, remaining_packets: 0, protocol_type: 0, data_size, class, command, args: [0; 80] }
    }

    pub fn with_args(mut self, args: &[u8]) -> Self {
        self.args[..args.len()].copy_from_slice(args);
        self
    }

    pub fn status(&self) -> Status {
        Status::from(self.status)
    }

    /// XOR of bytes 2..88 of the encoded report.
    fn crc(bytes: &[u8; REPORT_LEN]) -> u8 {
        bytes[2..88].iter().fold(0, |acc, b| acc ^ b)
    }

    pub fn encode(&self) -> [u8; REPORT_LEN] {
        let mut b = [0u8; REPORT_LEN];
        b[0] = self.status;
        b[1] = self.transaction_id;
        b[2..4].copy_from_slice(&self.remaining_packets.to_be_bytes());
        b[4] = self.protocol_type;
        b[5] = self.data_size;
        b[6] = self.class;
        b[7] = self.command;
        b[8..88].copy_from_slice(&self.args);
        b[88] = Self::crc(&b);
        b
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < REPORT_LEN {
            return None;
        }
        let mut args = [0u8; 80];
        args.copy_from_slice(&bytes[8..88]);
        Some(Self {
            status: bytes[0],
            transaction_id: bytes[1],
            remaining_packets: u16::from_be_bytes([bytes[2], bytes[3]]),
            protocol_type: bytes[4],
            data_size: bytes[5],
            class: bytes[6],
            command: bytes[7],
            args,
        })
    }
}

// ---- standard commands (class 0x00) ---------------------------------------

pub fn get_firmware_version() -> Report {
    Report::new(0x00, 0x81, 0x02)
}

pub fn get_serial() -> Report {
    Report::new(0x00, 0x82, 0x16)
}

pub fn get_device_mode() -> Report {
    Report::new(0x00, 0x84, 0x02)
}

/// `0x00` = normal (firmware runs its stored effect), `0x03` = driver mode.
pub fn set_device_mode(mode: u8) -> Report {
    Report::new(0x00, 0x04, 0x02).with_args(&[mode, 0x00])
}

// ---- misc / power (class 0x07) ---------------------------------------------

/// Reply `args[1]` is 0–255.
pub fn get_battery_level() -> Report {
    Report::new(0x07, 0x80, 0x02)
}

/// Reply `args[1]` is 0 or 1.
pub fn get_charging_status() -> Report {
    Report::new(0x07, 0x84, 0x02)
}

// ---- extended matrix (class 0x0F) -----------------------------------------

/// Built-in effects the firmware can run (and persist) on its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Effect {
    Off,
    Static { r: u8, g: u8, b: u8 },
    /// `None` breathes through random colours.
    Breathing { color: Option<[u8; 3]> },
    Spectrum,
    /// 1 = left→right, 2 = right→left (as the firmware counts them).
    Wave { direction: u8 },
    Reactive { speed: u8, r: u8, g: u8, b: u8 },
    Starlight { speed: u8, color: Option<[u8; 3]> },
}

/// Argument layouts follow OpenRazer's `razer_chroma_extended_matrix_effect_*`.
pub fn extended_matrix_effect(store: Store, led: u8, effect: Effect) -> Report {
    let s = store as u8;
    match effect {
        Effect::Off => Report::new(0x0F, 0x02, 0x06).with_args(&[s, led, 0x00, 0, 0, 0]),
        Effect::Static { r, g, b } => Report::new(0x0F, 0x02, 0x09).with_args(&[s, led, 0x01, 0x00, 0x00, 0x01, r, g, b]),
        Effect::Breathing { color: Some([r, g, b]) } => Report::new(0x0F, 0x02, 0x09).with_args(&[s, led, 0x02, 0x01, 0x00, 0x01, r, g, b]),
        Effect::Breathing { color: None } => Report::new(0x0F, 0x02, 0x06).with_args(&[s, led, 0x02, 0x00, 0, 0]),
        Effect::Spectrum => Report::new(0x0F, 0x02, 0x06).with_args(&[s, led, 0x03, 0, 0, 0]),
        Effect::Wave { direction } => Report::new(0x0F, 0x02, 0x06).with_args(&[s, led, 0x04, direction, 0x28, 0]),
        Effect::Reactive { speed, r, g, b } => Report::new(0x0F, 0x02, 0x09).with_args(&[s, led, 0x05, 0x00, speed, 0x01, r, g, b]),
        Effect::Starlight { speed, color: Some([r, g, b]) } => {
            Report::new(0x0F, 0x02, 0x09).with_args(&[s, led, 0x07, speed, 0x00, 0x01, r, g, b])
        }
        Effect::Starlight { speed, color: None } => Report::new(0x0F, 0x02, 0x06).with_args(&[s, led, 0x07, speed, 0, 0]),
    }
}

pub fn extended_matrix_brightness(store: Store, led: u8, value: u8) -> Report {
    Report::new(0x0F, 0x04, 0x03).with_args(&[store as u8, led, value])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_layout_and_crc() {
        let mut r = extended_matrix_effect(Store::VarStore, 0, Effect::Static { r: 0, g: 120, b: 255 });
        r.transaction_id = 0x3F;
        let b = r.encode();
        assert_eq!(&b[..8], &[0x00, 0x3F, 0x00, 0x00, 0x00, 0x09, 0x0F, 0x02]);
        assert_eq!(&b[8..17], &[0x01, 0x00, 0x01, 0x00, 0x00, 0x01, 0, 120, 255]);
        assert_eq!(b[88], b[2..88].iter().fold(0, |a, x| a ^ x));
        assert_eq!(b[89], 0);
        assert_eq!(Report::decode(&b).unwrap(), r);
    }

    #[test]
    fn battery_commands() {
        let r = get_battery_level();
        assert_eq!((r.class, r.command, r.data_size), (0x07, 0x80, 0x02));
        let r = get_charging_status();
        assert_eq!((r.class, r.command, r.data_size), (0x07, 0x84, 0x02));
    }

    #[test]
    fn status_mapping() {
        assert_eq!(Status::from(2), Status::Ok);
        assert_eq!(Status::from(3), Status::Fail);
        assert_eq!(Status::from(9), Status::Unknown(9));
    }
}
