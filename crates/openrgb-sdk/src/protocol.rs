//! Packet IDs, header layout and flag bits from OpenRGB's `NetworkProtocol.h`.

use crate::error::{Error, Result};

pub const MAGIC: [u8; 4] = *b"ORGB";
pub const DEFAULT_PORT: u16 = 6742;
pub const HEADER_LEN: usize = 16;

/// Highest protocol version this crate implements (OpenRGB 1.0).
///
/// 0 unversioned · 1 vendor string · 2 profiles · 3 mode brightness ·
/// 4 zone segments · 5 zone/controller flags, LED display names ·
/// 6 unique controller IDs, server name, per-zone modes, remote profile/settings managers.
pub const PROTOCOL_VERSION: u32 = 6;

pub mod packet {
    pub const REQUEST_CONTROLLER_COUNT: u32 = 0;
    pub const REQUEST_CONTROLLER_DATA: u32 = 1;
    pub const ACK: u32 = 10;
    pub const REQUEST_PROTOCOL_VERSION: u32 = 40;
    pub const SET_CLIENT_NAME: u32 = 50;
    pub const SET_SERVER_NAME: u32 = 51;
    pub const SET_CLIENT_FLAGS: u32 = 52;
    pub const SET_SERVER_FLAGS: u32 = 53;
    pub const SET_CLIENT_HOSTNAME: u32 = 54;
    pub const SET_SERVER_HOSTNAME: u32 = 55;

    pub const DEVICE_LIST_UPDATED: u32 = 100;
    pub const DETECTION_STARTED: u32 = 101;
    pub const DETECTION_PROGRESS_CHANGED: u32 = 102;
    pub const DETECTION_COMPLETE: u32 = 103;

    pub const GET_I2C_BUS_INFO: u32 = 120;
    pub const GET_HID_DEVICE_INFO: u32 = 121;
    pub const GET_USB_DEVICE_INFO: u32 = 122;
    pub const GET_SERIAL_PORTS: u32 = 123;
    pub const GET_USB_SERIAL_PORTS: u32 = 124;

    pub const REQUEST_RESCAN_DEVICES: u32 = 140;

    pub const PROFILEMANAGER_GET_PROFILE_LIST: u32 = 150;
    pub const PROFILEMANAGER_SAVE_PROFILE: u32 = 151;
    pub const PROFILEMANAGER_LOAD_PROFILE: u32 = 152;
    pub const PROFILEMANAGER_DELETE_PROFILE: u32 = 153;
    pub const PROFILEMANAGER_UPLOAD_PROFILE: u32 = 154;
    pub const PROFILEMANAGER_DOWNLOAD_PROFILE: u32 = 155;
    pub const PROFILEMANAGER_GET_ACTIVE_PROFILE: u32 = 156;
    pub const PROFILEMANAGER_ACTIVE_PROFILE_CHANGED: u32 = 157;
    pub const PROFILEMANAGER_PROFILE_LOADED: u32 = 158;
    pub const PROFILEMANAGER_PROFILE_ABOUT_TO_LOAD: u32 = 159;
    pub const PROFILEMANAGER_PROFILE_LIST_UPDATED: u32 = 160;
    pub const PROFILEMANAGER_CLEAR_ACTIVE_PROFILE: u32 = 161;

    pub const PLUGINMANAGER_GET_PLUGIN_LIST: u32 = 200;
    pub const PLUGINMANAGER_PLUGIN_SPECIFIC: u32 = 201;

    pub const SETTINGSMANAGER_GET_SETTINGS: u32 = 250;
    pub const SETTINGSMANAGER_GET_SETTINGS_SCHEMA: u32 = 251;
    pub const SETTINGSMANAGER_MODIFY_SETTINGS: u32 = 252;
    pub const SETTINGSMANAGER_SET_SETTINGS: u32 = 253;
    pub const SETTINGSMANAGER_SAVE_SETTINGS: u32 = 254;

    pub const LOGMANAGER_CLEAR_LOG_BUFFER: u32 = 300;
    pub const LOGMANAGER_GET_LOG_BUFFER: u32 = 301;
    pub const LOGMANAGER_GET_LOG_LEVEL: u32 = 302;
    pub const LOGMANAGER_SET_LOG_LEVEL: u32 = 303;
    pub const LOGMANAGER_LOGGED_ENTRY: u32 = 304;

    pub const RGBCONTROLLER_RESIZEZONE: u32 = 1000;
    pub const RGBCONTROLLER_CLEARSEGMENTS: u32 = 1001;
    pub const RGBCONTROLLER_ADDSEGMENT: u32 = 1002;
    pub const RGBCONTROLLER_CONFIGUREZONE: u32 = 1003;
    pub const RGBCONTROLLER_CONFIGUREDEVICE: u32 = 1004;
    pub const RGBCONTROLLER_SETHIDDEN: u32 = 1005;
    pub const RGBCONTROLLER_UPDATELEDS: u32 = 1050;
    pub const RGBCONTROLLER_UPDATEZONELEDS: u32 = 1051;
    pub const RGBCONTROLLER_UPDATESINGLELED: u32 = 1052;
    pub const RGBCONTROLLER_SETCUSTOMMODE: u32 = 1100;
    pub const RGBCONTROLLER_UPDATEMODE: u32 = 1101;
    pub const RGBCONTROLLER_SAVEMODE: u32 = 1102;
    pub const RGBCONTROLLER_UPDATEZONEMODE: u32 = 1103;
    pub const RGBCONTROLLER_SETDEVICESPECIFICCONFIGURATION: u32 = 1130;
    pub const RGBCONTROLLER_SETDEVICESPECIFICZONECONFIGURATION: u32 = 1131;
    pub const RGBCONTROLLER_SIGNALUPDATE: u32 = 1150;
}

/// Bits for `SET_CLIENT_FLAGS` (v6).
pub mod client_flags {
    pub const RGB_CONTROLLER: u32 = 1 << 0;
    pub const LOG_MANAGER: u32 = 1 << 1;
    pub const PROFILE_MANAGER: u32 = 1 << 2;
    pub const PLUGIN_MANAGER: u32 = 1 << 3;
    pub const SETTINGS_MANAGER: u32 = 1 << 4;
    /// Ask to be treated as a local client (granted only from localhost);
    /// needed for save/delete/upload operations.
    pub const REQUEST_LOCAL: u32 = 1 << 16;
}

/// Bits for `SET_SERVER_FLAGS` (v6).
pub mod server_flags {
    pub const RGB_CONTROLLER: u32 = 1 << 0;
    pub const LOG_MANAGER: u32 = 1 << 1;
    pub const PROFILE_MANAGER: u32 = 1 << 2;
    pub const PLUGIN_MANAGER: u32 = 1 << 3;
    pub const SETTINGS_MANAGER: u32 = 1 << 4;
    pub const DETECTION: u32 = 1 << 5;
    pub const DEVICE_INFO: u32 = 1 << 6;
    pub const LOCAL_CLIENT_GRANTED: u32 = 1 << 16;
}

/// Status codes carried by `ACK` packets (v6).
pub mod ack_status {
    pub const OK: u32 = 0;
    pub const ERROR: u32 = 1;
    pub const UNSUPPORTED: u32 = 2;
    pub const NOT_ALLOWED: u32 = 3;
    pub const INVALID_ID: u32 = 4;
    pub const INVALID_DATA: u32 = 5;
}

/// The 16-byte packet header. `dev_id` is the controller's unique ID (v6) or
/// list index (older servers); `size` counts payload bytes after the header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub dev_id: u32,
    pub pkt_id: u32,
    pub size: u32,
}

impl Header {
    pub fn encode(&self) -> [u8; HEADER_LEN] {
        let mut out = [0u8; HEADER_LEN];
        out[0..4].copy_from_slice(&MAGIC);
        out[4..8].copy_from_slice(&self.dev_id.to_le_bytes());
        out[8..12].copy_from_slice(&self.pkt_id.to_le_bytes());
        out[12..16].copy_from_slice(&self.size.to_le_bytes());
        out
    }

    pub fn decode(bytes: &[u8; HEADER_LEN]) -> Result<Header> {
        if bytes[0..4] != MAGIC {
            return Err(Error::BadMagic);
        }
        let le = |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
        Ok(Header { dev_id: le(4), pkt_id: le(8), size: le(12) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_roundtrip() {
        let h = Header { dev_id: 0xA1B2C3D4, pkt_id: packet::RGBCONTROLLER_UPDATELEDS, size: 42 };
        let bytes = h.encode();
        assert_eq!(&bytes[0..4], b"ORGB");
        assert_eq!(Header::decode(&bytes).unwrap(), h);
    }

    #[test]
    fn bad_magic_is_rejected() {
        let mut bytes = Header { dev_id: 0, pkt_id: 0, size: 0 }.encode();
        bytes[0] = b'X';
        assert!(matches!(Header::decode(&bytes), Err(Error::BadMagic)));
    }
}
