//! Data model for controllers as described by the server.
//!
//! Enumerations are newtypes over their wire integer so that values added by
//! newer OpenRGB releases still parse; the named constants cover what 1.0 ships.

use std::fmt;

use serde::{Deserialize, Serialize};

/// An RGB colour. On the wire OpenRGB packs it as `0x00BBGGRR` (bytes R, G, B, 0).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const BLACK: Color = Color { r: 0, g: 0, b: 0 };
    pub const WHITE: Color = Color { r: 255, g: 255, b: 255 };

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub const fn to_u32(self) -> u32 {
        self.r as u32 | (self.g as u32) << 8 | (self.b as u32) << 16
    }

    pub const fn from_u32(v: u32) -> Self {
        Self { r: (v & 0xFF) as u8, g: ((v >> 8) & 0xFF) as u8, b: ((v >> 16) & 0xFF) as u8 }
    }
}

impl fmt::Debug for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

macro_rules! wire_enum {
    ($(#[$meta:meta])* $name:ident($repr:ty) { $($variant:ident = $value:expr => $label:literal),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub $repr);

        impl $name {
            $(pub const $variant: $name = $name($value);)*

            pub fn label(self) -> &'static str {
                match self {
                    $($name::$variant => $label,)*
                    _ => "unknown",
                }
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", self.label(), self.0)
            }
        }
    };
}

wire_enum! {
    /// `device_type` from `RGBController.h`.
    DeviceType(i32) {
        MOTHERBOARD = 0 => "motherboard",
        DRAM = 1 => "dram",
        GPU = 2 => "gpu",
        COOLER = 3 => "cooler",
        LEDSTRIP = 4 => "ledstrip",
        KEYBOARD = 5 => "keyboard",
        MOUSE = 6 => "mouse",
        MOUSEMAT = 7 => "mousemat",
        HEADSET = 8 => "headset",
        HEADSET_STAND = 9 => "headset_stand",
        GAMEPAD = 10 => "gamepad",
        LIGHT = 11 => "light",
        SPEAKER = 12 => "speaker",
        VIRTUAL = 13 => "virtual",
        STORAGE = 14 => "storage",
        CASE = 15 => "case",
        MICROPHONE = 16 => "microphone",
        ACCESSORY = 17 => "accessory",
        KEYPAD = 18 => "keypad",
        LAPTOP = 19 => "laptop",
        MONITOR = 20 => "monitor",
        UNKNOWN = 21 => "unknown",
    }
}

wire_enum! {
    /// `zone_type` from `RGBControllerInterface.h`.
    ZoneType(i32) {
        SINGLE = 0 => "single",
        LINEAR = 1 => "linear",
        MATRIX = 2 => "matrix",
        LINEAR_LOOP = 3 => "linear_loop",
        MATRIX_LOOP_X = 4 => "matrix_loop_x",
        MATRIX_LOOP_Y = 5 => "matrix_loop_y",
        SEGMENTED = 6 => "segmented",
    }
}

impl ZoneType {
    /// Zones laid out as a grid (with or without wrap-around).
    pub fn is_matrix(self) -> bool {
        matches!(self, ZoneType::MATRIX | ZoneType::MATRIX_LOOP_X | ZoneType::MATRIX_LOOP_Y)
    }
}

wire_enum! {
    /// `MODE_COLORS_*`: how a mode takes its colours.
    ColorMode(u32) {
        NONE = 0 => "none",
        PER_LED = 1 => "per_led",
        MODE_SPECIFIC = 2 => "mode_specific",
        RANDOM = 3 => "random",
    }
}

wire_enum! {
    /// `MODE_DIRECTION_*`.
    Direction(u32) {
        LEFT = 0 => "left",
        RIGHT = 1 => "right",
        UP = 2 => "up",
        DOWN = 3 => "down",
        HORIZONTAL = 4 => "horizontal",
        VERTICAL = 5 => "vertical",
        UP_LEFT = 6 => "up_left",
        UP_RIGHT = 7 => "up_right",
        DOWN_LEFT = 8 => "down_left",
        DOWN_RIGHT = 9 => "down_right",
    }
}

/// `CONTROLLER_FLAG_*` bits (v5+ `Device::flags`).
pub mod controller_flags {
    pub const LOCAL: u32 = 1 << 0;
    pub const REMOTE: u32 = 1 << 1;
    pub const VIRTUAL: u32 = 1 << 2;
    pub const HIDDEN: u32 = 1 << 3;
    pub const RESET_BEFORE_UPDATE: u32 = 1 << 8;
    pub const MANUALLY_CONFIGURABLE_NAME: u32 = 1 << 16;
    pub const MANUALLY_CONFIGURABLE_DEVICE_SPECIFIC: u32 = 1 << 17;
    pub const MANUALLY_CONFIGURED_NAME: u32 = 1 << 24;
    pub const MANUALLY_CONFIGURED_DEVICE_SPECIFIC: u32 = 1 << 25;
}

/// `ZONE_FLAG_*` bits (v5+ `Zone::flags`).
pub mod zone_flags {
    pub const MANUALLY_CONFIGURABLE_SIZE_EFFECTS_ONLY: u32 = 1 << 0;
    pub const MANUALLY_CONFIGURABLE_SIZE: u32 = 1 << 1;
    pub const MANUALLY_CONFIGURABLE_NAME: u32 = 1 << 2;
    pub const MANUALLY_CONFIGURABLE_TYPE: u32 = 1 << 3;
    pub const MANUALLY_CONFIGURABLE_MATRIX_MAP: u32 = 1 << 4;
    pub const MANUALLY_CONFIGURABLE_SEGMENTS: u32 = 1 << 5;
    pub const MANUALLY_CONFIGURABLE_DEVICE_SPECIFIC: u32 = 1 << 6;
    pub const ZONE_GEOMETRY_MAY_CHANGE: u32 = 1 << 24;
}

/// `SEGMENT_FLAG_*` bits (v6 `Segment::flags`).
pub mod segment_flags {
    pub const GROUP_START: u32 = 1 << 0;
    pub const GROUP_MEMBER: u32 = 1 << 1;
}

/// `MODE_FLAG_*` bit set.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModeFlags(pub u32);

impl ModeFlags {
    pub const HAS_SPEED: ModeFlags = ModeFlags(1 << 0);
    pub const HAS_DIRECTION_LR: ModeFlags = ModeFlags(1 << 1);
    pub const HAS_DIRECTION_UD: ModeFlags = ModeFlags(1 << 2);
    pub const HAS_DIRECTION_HV: ModeFlags = ModeFlags(1 << 3);
    pub const HAS_BRIGHTNESS: ModeFlags = ModeFlags(1 << 4);
    pub const HAS_PER_LED_COLOR: ModeFlags = ModeFlags(1 << 5);
    pub const HAS_MODE_SPECIFIC_COLOR: ModeFlags = ModeFlags(1 << 6);
    pub const HAS_RANDOM_COLOR: ModeFlags = ModeFlags(1 << 7);
    /// The device can persist this mode to flash via `SaveMode`.
    pub const MANUAL_SAVE: ModeFlags = ModeFlags(1 << 8);
    /// The device persists this mode on its own whenever it is applied.
    pub const AUTOMATIC_SAVE: ModeFlags = ModeFlags(1 << 9);
    pub const REQUIRES_ENTIRE_DEVICE: ModeFlags = ModeFlags(1 << 10);
    pub const HAS_DIRECTION_DIAG: ModeFlags = ModeFlags(1 << 11);

    pub const fn contains(self, other: ModeFlags) -> bool {
        self.0 & other.0 == other.0
    }
}

impl fmt::Debug for ModeFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ModeFlags({:#06x})", self.0)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mode {
    pub name: String,
    /// Device-specific mode value; only present on the wire before v6.
    pub value: i32,
    pub flags: ModeFlags,
    pub speed_min: u32,
    pub speed_max: u32,
    pub brightness_min: u32,
    pub brightness_max: u32,
    pub colors_min: u32,
    pub colors_max: u32,
    pub speed: u32,
    pub brightness: u32,
    pub direction: Direction,
    pub color_mode: ColorMode,
    pub colors: Vec<Color>,
}

impl Mode {
    pub fn has(&self, flag: ModeFlags) -> bool {
        self.flags.contains(flag)
    }

    /// True for modes that accept per-LED colours (what `UpdateLEDs` drives).
    pub fn is_per_led(&self) -> bool {
        self.has(ModeFlags::HAS_PER_LED_COLOR)
    }

    pub fn is_saveable(&self) -> bool {
        self.has(ModeFlags::MANUAL_SAVE)
    }
}

impl Default for Direction {
    fn default() -> Self {
        Direction::LEFT
    }
}

impl Default for ColorMode {
    fn default() -> Self {
        ColorMode::NONE
    }
}

/// LED index grid for a matrix zone. Entries equal to [`MatrixMap::NO_LED`] are empty cells.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatrixMap {
    pub height: u32,
    pub width: u32,
    /// Row-major, `height * width` entries; each is an LED index within the zone.
    pub map: Vec<u32>,
}

impl MatrixMap {
    pub const NO_LED: u32 = 0xFFFF_FFFF;

    /// LED index at (`row`, `col`), or `None` for an empty cell or out-of-range position.
    pub fn led_at(&self, row: u32, col: u32) -> Option<u32> {
        if row >= self.height || col >= self.width {
            return None;
        }
        let v = self.map[(row * self.width + col) as usize];
        (v != Self::NO_LED).then_some(v)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Segment {
    pub name: String,
    pub kind: ZoneType,
    pub start_idx: u32,
    pub leds_count: u32,
    pub matrix: Option<MatrixMap>,
    pub flags: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Zone {
    pub name: String,
    pub kind: ZoneType,
    pub leds_min: u32,
    pub leds_max: u32,
    pub leds_count: u32,
    pub matrix: Option<MatrixMap>,
    pub segments: Vec<Segment>,
    pub flags: u32,
    /// v6: zone-level active mode, when the device supports per-zone modes.
    pub active_mode: Option<i32>,
    pub modes: Vec<Mode>,
    pub display_name: Option<String>,
}

impl Zone {
    pub fn is_resizable(&self) -> bool {
        self.leds_min != self.leds_max
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Led {
    pub name: String,
    /// Device-specific LED value; only present on the wire before v6.
    pub value: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    /// Unique controller ID (v6) or list index (older servers). Use it as `dev_id`.
    pub id: u32,
    pub kind: DeviceType,
    pub name: String,
    pub vendor: String,
    pub description: String,
    pub version: String,
    pub serial: String,
    pub location: String,
    pub modes: Vec<Mode>,
    pub active_mode: i32,
    pub zones: Vec<Zone>,
    pub leds: Vec<Led>,
    /// Current colour of every LED, in LED order.
    pub colors: Vec<Color>,
    /// v5: user-facing LED names (same length as `leds` when present).
    pub led_display_names: Vec<String>,
    /// v5: controller flags.
    pub flags: u32,
    /// v6: user-chosen display name (falls back to `name`).
    pub display_name: String,
    /// v6: device-specific configuration JSON.
    pub configuration: String,
}

impl Device {
    /// Index of the mode to use for per-LED streaming: "Direct" if present,
    /// otherwise the first mode that accepts per-LED colours.
    pub fn direct_mode_index(&self) -> Option<usize> {
        self.modes
            .iter()
            .position(|m| m.name.eq_ignore_ascii_case("direct") && m.is_per_led())
            .or_else(|| self.modes.iter().position(Mode::is_per_led))
    }

    /// Modes the device can persist with `SaveMode`.
    pub fn saveable_modes(&self) -> impl Iterator<Item = (usize, &Mode)> {
        self.modes.iter().enumerate().filter(|(_, m)| m.is_saveable())
    }

    /// Index of the first LED belonging to `zone_idx`.
    pub fn zone_led_offset(&self, zone_idx: usize) -> usize {
        self.zones.iter().take(zone_idx).map(|z| z.leds_count as usize).sum()
    }

    /// Hidden by the user in OpenRGB (v5+ flag).
    pub fn is_hidden(&self) -> bool {
        self.flags & controller_flags::HIDDEN != 0
    }

    pub fn is_virtual(&self) -> bool {
        self.flags & controller_flags::VIRTUAL != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_packs_as_bgr_little_endian() {
        let c = Color::new(0x11, 0x22, 0x33);
        assert_eq!(c.to_u32(), 0x0033_2211);
        assert_eq!(Color::from_u32(c.to_u32()), c);
        assert_eq!(format!("{c:?}"), "#112233");
    }

    #[test]
    fn enum_labels_and_unknowns() {
        assert_eq!(DeviceType::MOUSEMAT.label(), "mousemat");
        assert_eq!(DeviceType(999).label(), "unknown");
        assert_eq!(format!("{:?}", ZoneType::MATRIX), "matrix(2)");
    }

    #[test]
    fn matrix_lookup() {
        let m = MatrixMap { height: 2, width: 2, map: vec![0, MatrixMap::NO_LED, 1, 2] };
        assert_eq!(m.led_at(0, 0), Some(0));
        assert_eq!(m.led_at(0, 1), None);
        assert_eq!(m.led_at(1, 1), Some(2));
        assert_eq!(m.led_at(2, 0), None);
    }
}
