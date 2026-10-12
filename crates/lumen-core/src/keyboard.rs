//! Physical keyboard layouts.
//!
//! OpenRGB describes a keyboard as the logical LED matrix its firmware
//! addresses, not as where the keys are: a tenkeyless board's 18.25 u of keys
//! get squeezed into 17 equal columns, with unnamed filler LEDs in the gaps.
//! When the LED names follow OpenRGB's `Key: …` convention the keys can be
//! placed on a real layout instead, in key units (1 u = 19.05 mm), and ANSI
//! can be told from ISO by the keys only one of them has.

use serde::{Deserialize, Serialize};

/// Keyboard layout family, decided from the key names present.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    Ansi,
    Iso,
}

impl Layout {
    /// ISO if any ISO-only key is present, ANSI for any other `Key: …` set,
    /// `None` when the names do not follow the convention at all.
    pub fn detect<'a>(names: impl IntoIterator<Item = &'a str>) -> Option<Layout> {
        let mut keyed = false;
        for name in names {
            match name {
                "Key: \\ (ISO)" | "Key: #" | "Key: Enter (ISO)" => return Some(Layout::Iso),
                _ if name.starts_with("Key: ") => keyed = true,
                _ => {}
            }
        }
        keyed.then_some(Layout::Ansi)
    }
}

/// Keycap outline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    Plain,
    /// ISO Enter: 1.5 u on the upper row, 1.25 u on the lower, flush right.
    IsoEnter,
    /// Two rows tall (number pad `+` and Enter).
    Tall,
}

/// A key's place on a full-size board: `row` 0 is the function row, 1–5 the
/// main block; `x` and `w` are key units from the left edge.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Slot {
    pub row: u8,
    pub x: f32,
    pub w: f32,
    pub shape: Shape,
}

impl Slot {
    /// Top edge in key units.
    pub fn y(&self) -> f32 {
        row_y(self.row)
    }

    /// Height in key units.
    pub fn h(&self) -> f32 {
        match self.shape {
            Shape::Plain => 1.0,
            Shape::IsoEnter | Shape::Tall => 2.0,
        }
    }
}

/// Top edge of a row in key units: the function row sits 0.5 u above the main block.
pub fn row_y(row: u8) -> f32 {
    if row == 0 {
        0.0
    } else {
        row as f32 + 0.5
    }
}

const fn k(row: u8, x: f32, w: f32) -> Slot {
    Slot { row, x, w, shape: Shape::Plain }
}

const ISO_ENTER: Slot = Slot { row: 2, x: 13.75, w: 1.5, shape: Shape::IsoEnter };

/// Where a key sits, if its name is one of OpenRGB's standard key names.
pub fn slot(name: &str, layout: Layout) -> Option<Slot> {
    let iso = layout == Layout::Iso;
    Some(match name {
        // Function row.
        "Key: Escape" => k(0, 0.0, 1.0),
        "Key: F1" => k(0, 2.0, 1.0),
        "Key: F2" => k(0, 3.0, 1.0),
        "Key: F3" => k(0, 4.0, 1.0),
        "Key: F4" => k(0, 5.0, 1.0),
        "Key: F5" => k(0, 6.5, 1.0),
        "Key: F6" => k(0, 7.5, 1.0),
        "Key: F7" => k(0, 8.5, 1.0),
        "Key: F8" => k(0, 9.5, 1.0),
        "Key: F9" => k(0, 11.0, 1.0),
        "Key: F10" => k(0, 12.0, 1.0),
        "Key: F11" => k(0, 13.0, 1.0),
        "Key: F12" => k(0, 14.0, 1.0),
        "Key: Print Screen" => k(0, 15.25, 1.0),
        "Key: Scroll Lock" => k(0, 16.25, 1.0),
        "Key: Pause/Break" => k(0, 17.25, 1.0),

        // Number row.
        "Key: `" => k(1, 0.0, 1.0),
        "Key: 1" => k(1, 1.0, 1.0),
        "Key: 2" => k(1, 2.0, 1.0),
        "Key: 3" => k(1, 3.0, 1.0),
        "Key: 4" => k(1, 4.0, 1.0),
        "Key: 5" => k(1, 5.0, 1.0),
        "Key: 6" => k(1, 6.0, 1.0),
        "Key: 7" => k(1, 7.0, 1.0),
        "Key: 8" => k(1, 8.0, 1.0),
        "Key: 9" => k(1, 9.0, 1.0),
        "Key: 0" => k(1, 10.0, 1.0),
        "Key: -" => k(1, 11.0, 1.0),
        "Key: =" => k(1, 12.0, 1.0),
        "Key: Backspace" => k(1, 13.0, 2.0),
        "Key: Insert" => k(1, 15.25, 1.0),
        "Key: Home" => k(1, 16.25, 1.0),
        "Key: Page Up" => k(1, 17.25, 1.0),
        "Key: Num Lock" => k(1, 18.5, 1.0),
        "Key: Number Pad /" => k(1, 19.5, 1.0),
        "Key: Number Pad *" => k(1, 20.5, 1.0),
        "Key: Number Pad -" => k(1, 21.5, 1.0),

        // Top letter row.
        "Key: Tab" => k(2, 0.0, 1.5),
        "Key: Q" => k(2, 1.5, 1.0),
        "Key: W" => k(2, 2.5, 1.0),
        "Key: E" => k(2, 3.5, 1.0),
        "Key: R" => k(2, 4.5, 1.0),
        "Key: T" => k(2, 5.5, 1.0),
        "Key: Y" => k(2, 6.5, 1.0),
        "Key: U" => k(2, 7.5, 1.0),
        "Key: I" => k(2, 8.5, 1.0),
        "Key: O" => k(2, 9.5, 1.0),
        "Key: P" => k(2, 10.5, 1.0),
        "Key: [" => k(2, 11.5, 1.0),
        "Key: ]" => k(2, 12.5, 1.0),
        "Key: \\ (ANSI)" => k(2, 13.5, 1.5),
        // A plain backslash is the ANSI key unless the board is ISO, where it sits next to Left Shift.
        "Key: \\" => {
            if iso {
                k(4, 1.25, 1.0)
            } else {
                k(2, 13.5, 1.5)
            }
        }
        "Key: Delete" => k(2, 15.25, 1.0),
        "Key: End" => k(2, 16.25, 1.0),
        "Key: Page Down" => k(2, 17.25, 1.0),
        "Key: Number Pad 7" => k(2, 18.5, 1.0),
        "Key: Number Pad 8" => k(2, 19.5, 1.0),
        "Key: Number Pad 9" => k(2, 20.5, 1.0),
        "Key: Number Pad +" => Slot { row: 2, x: 21.5, w: 1.0, shape: Shape::Tall },

        // Home row.
        "Key: Caps Lock" => k(3, 0.0, 1.75),
        "Key: A" => k(3, 1.75, 1.0),
        "Key: S" => k(3, 2.75, 1.0),
        "Key: D" => k(3, 3.75, 1.0),
        "Key: F" => k(3, 4.75, 1.0),
        "Key: G" => k(3, 5.75, 1.0),
        "Key: H" => k(3, 6.75, 1.0),
        "Key: J" => k(3, 7.75, 1.0),
        "Key: K" => k(3, 8.75, 1.0),
        "Key: L" => k(3, 9.75, 1.0),
        "Key: ;" => k(3, 10.75, 1.0),
        "Key: '" => k(3, 11.75, 1.0),
        "Key: #" => k(3, 12.75, 1.0),
        "Key: Enter (ISO)" => ISO_ENTER,
        "Key: Enter" => {
            if iso {
                ISO_ENTER
            } else {
                k(3, 12.75, 2.25)
            }
        }
        "Key: Number Pad 4" => k(3, 18.5, 1.0),
        "Key: Number Pad 5" => k(3, 19.5, 1.0),
        "Key: Number Pad 6" => k(3, 20.5, 1.0),

        // Bottom letter row.
        "Key: Left Shift" => {
            if iso {
                k(4, 0.0, 1.25)
            } else {
                k(4, 0.0, 2.25)
            }
        }
        "Key: \\ (ISO)" => k(4, 1.25, 1.0),
        "Key: Z" => k(4, 2.25, 1.0),
        "Key: X" => k(4, 3.25, 1.0),
        "Key: C" => k(4, 4.25, 1.0),
        "Key: V" => k(4, 5.25, 1.0),
        "Key: B" => k(4, 6.25, 1.0),
        "Key: N" => k(4, 7.25, 1.0),
        "Key: M" => k(4, 8.25, 1.0),
        "Key: ," => k(4, 9.25, 1.0),
        "Key: ." => k(4, 10.25, 1.0),
        "Key: /" => k(4, 11.25, 1.0),
        "Key: Right Shift" => k(4, 12.25, 2.75),
        "Key: Up Arrow" => k(4, 16.25, 1.0),
        "Key: Number Pad 1" => k(4, 18.5, 1.0),
        "Key: Number Pad 2" => k(4, 19.5, 1.0),
        "Key: Number Pad 3" => k(4, 20.5, 1.0),
        "Key: Number Pad Enter" => Slot { row: 4, x: 21.5, w: 1.0, shape: Shape::Tall },

        // Modifier row.
        "Key: Left Control" => k(5, 0.0, 1.25),
        "Key: Left Windows" => k(5, 1.25, 1.25),
        "Key: Left Alt" => k(5, 2.5, 1.25),
        "Key: Space" => k(5, 3.75, 6.25),
        "Key: Right Alt" => k(5, 10.0, 1.25),
        "Key: Right Fn" | "Key: Right Windows" => k(5, 11.25, 1.25),
        "Key: Menu" => k(5, 12.5, 1.25),
        "Key: Right Control" => k(5, 13.75, 1.25),
        "Key: Left Arrow" => k(5, 15.25, 1.0),
        "Key: Down Arrow" => k(5, 16.25, 1.0),
        "Key: Right Arrow" => k(5, 17.25, 1.0),
        "Key: Number Pad 0" => k(5, 18.5, 2.0),
        "Key: Number Pad ." => k(5, 20.5, 1.0),

        _ => return None,
    })
}

/// Every key name [`slot`] knows, for tooling that mirrors the table.
pub const KEY_NAMES: &[&str] = &[
    "Key: Escape",
    "Key: F1",
    "Key: F2",
    "Key: F3",
    "Key: F4",
    "Key: F5",
    "Key: F6",
    "Key: F7",
    "Key: F8",
    "Key: F9",
    "Key: F10",
    "Key: F11",
    "Key: F12",
    "Key: Print Screen",
    "Key: Scroll Lock",
    "Key: Pause/Break",
    "Key: `",
    "Key: 1",
    "Key: 2",
    "Key: 3",
    "Key: 4",
    "Key: 5",
    "Key: 6",
    "Key: 7",
    "Key: 8",
    "Key: 9",
    "Key: 0",
    "Key: -",
    "Key: =",
    "Key: Backspace",
    "Key: Insert",
    "Key: Home",
    "Key: Page Up",
    "Key: Num Lock",
    "Key: Number Pad /",
    "Key: Number Pad *",
    "Key: Number Pad -",
    "Key: Tab",
    "Key: Q",
    "Key: W",
    "Key: E",
    "Key: R",
    "Key: T",
    "Key: Y",
    "Key: U",
    "Key: I",
    "Key: O",
    "Key: P",
    "Key: [",
    "Key: ]",
    "Key: \\ (ANSI)",
    "Key: \\",
    "Key: Delete",
    "Key: End",
    "Key: Page Down",
    "Key: Number Pad 7",
    "Key: Number Pad 8",
    "Key: Number Pad 9",
    "Key: Number Pad +",
    "Key: Caps Lock",
    "Key: A",
    "Key: S",
    "Key: D",
    "Key: F",
    "Key: G",
    "Key: H",
    "Key: J",
    "Key: K",
    "Key: L",
    "Key: ;",
    "Key: '",
    "Key: #",
    "Key: Enter (ISO)",
    "Key: Enter",
    "Key: Number Pad 4",
    "Key: Number Pad 5",
    "Key: Number Pad 6",
    "Key: Left Shift",
    "Key: \\ (ISO)",
    "Key: Z",
    "Key: X",
    "Key: C",
    "Key: V",
    "Key: B",
    "Key: N",
    "Key: M",
    "Key: ,",
    "Key: .",
    "Key: /",
    "Key: Right Shift",
    "Key: Up Arrow",
    "Key: Number Pad 1",
    "Key: Number Pad 2",
    "Key: Number Pad 3",
    "Key: Number Pad Enter",
    "Key: Left Control",
    "Key: Left Windows",
    "Key: Left Alt",
    "Key: Space",
    "Key: Right Alt",
    "Key: Right Fn",
    "Key: Right Windows",
    "Key: Menu",
    "Key: Right Control",
    "Key: Left Arrow",
    "Key: Down Arrow",
    "Key: Right Arrow",
    "Key: Number Pad 0",
    "Key: Number Pad .",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_layout_from_names() {
        assert_eq!(Layout::detect(["Key: A", "Key: \\ (ANSI)", "Key: Enter"]), Some(Layout::Ansi));
        assert_eq!(Layout::detect(["Key: A", "Key: #", "Key: Enter"]), Some(Layout::Iso));
        assert_eq!(Layout::detect(["Key: A", "Key: \\ (ISO)"]), Some(Layout::Iso));
        assert_eq!(Layout::detect(["LED 0", "LED 1"]), None);
        assert_eq!(Layout::detect([]), None);
    }

    #[test]
    fn every_listed_name_has_a_slot_and_rows_do_not_overlap() {
        for layout in [Layout::Ansi, Layout::Iso] {
            let mut slots: Vec<(String, Slot)> = Vec::new();
            for name in KEY_NAMES {
                let s = slot(name, layout).unwrap_or_else(|| panic!("{name} missing"));
                // Alternate names for the same physical key may share a slot.
                let alias = matches!(
                    (*name, layout),
                    ("Key: \\", _) | ("Key: Right Windows", _) | ("Key: Enter (ISO)", _) | ("Key: #", Layout::Ansi) | ("Key: \\ (ISO)", Layout::Ansi) | ("Key: \\ (ANSI)", Layout::Iso)
                );
                if !alias {
                    for (other, o) in &slots {
                        let same_rows = s.row < o.row + o.h() as u8 && o.row < s.row + s.h() as u8;
                        let overlap = same_rows && s.x < o.x + o.w - 0.01 && o.x < s.x + s.w - 0.01;
                        assert!(!overlap, "{name} overlaps {other} on {layout:?}");
                    }
                    slots.push((name.to_string(), s));
                }
            }
        }
    }

    #[test]
    fn iso_changes_enter_shift_and_backslash() {
        assert_eq!(slot("Key: Enter", Layout::Ansi).unwrap().w, 2.25);
        assert_eq!(slot("Key: Enter", Layout::Iso).unwrap().shape, Shape::IsoEnter);
        assert_eq!(slot("Key: Left Shift", Layout::Ansi).unwrap().w, 2.25);
        assert_eq!(slot("Key: Left Shift", Layout::Iso).unwrap().w, 1.25);
        assert_eq!(slot("Key: \\", Layout::Iso).unwrap().row, 4);
        assert_eq!(slot("Key: \\", Layout::Ansi).unwrap().row, 2);
        assert_eq!(slot("Key: Media Mute", Layout::Ansi), None);
    }
}
