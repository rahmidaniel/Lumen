//! Razer devices speak a 90-byte command protocol over HID feature reports.
//! OpenRGB streams colours through that same channel; this crate only sends the
//! occasional query (battery, firmware) and the writes OpenRGB has no API for:
//! storing an effect in the device's flash, switching device mode, and device
//! settings (polling rate, DPI, sleep timer, low-battery threshold).
//!
//! Protocol details are documented in `docs/protocols.md`; the command tables
//! follow OpenRazer's `razerchromacommon.c`.

mod device;
pub mod protocol;
pub mod settings;

pub use device::{Battery, RazerDevice, RazerInfo};
pub use protocol::{Effect, Report, Status, Store};
pub use settings::{DpiStages, PollingKind};

pub const VENDOR_ID: u16 = 0x1532;

/// Product IDs we know how to talk to. Anything else is still tried with the
/// common transaction IDs.
pub mod pid {
    pub const STRIDER_CHROMA: u16 = 0x0C05;
    pub const DEATHSTALKER_V2_PRO_WIRELESS: u16 = 0x0290;
    pub const DEATHSTALKER_V2_PRO_WIRED: u16 = 0x0292;
    pub const DEATHSTALKER_V2_PRO_TKL_WIRELESS: u16 = 0x0296;
    pub const DEATHSTALKER_V2_PRO_TKL_WIRED: u16 = 0x0298;
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Hid(#[from] hidapi::HidError),
    #[error("no Razer device with product ID {0:#06x}")]
    NotFound(u16),
    #[error("no interface of {0:#06x} answered the probe; is another driver holding it?")]
    NoControlInterface(u16),
    #[error("device answered with status {0:?}")]
    Status(Status),
    #[error("device did not answer")]
    NoReply,
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, Error>;
