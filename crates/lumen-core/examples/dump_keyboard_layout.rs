//! Writes the physical key table as JSON for the browser mock, which mirrors
//! the Rust geometry without hardware:
//!
//! `cargo run -p lumen-core --example dump_keyboard_layout > src/lib/keyboard-layout.json`

use lumen_core::keyboard::{slot, Layout, Shape, KEY_NAMES};

fn main() {
    let mut out = String::from("{\n");
    let layouts = [(Layout::Ansi, "ansi"), (Layout::Iso, "iso")];
    for (i, (layout, key)) in layouts.into_iter().enumerate() {
        out.push_str(&format!("  \"{key}\": {{\n"));
        let entries: Vec<String> = KEY_NAMES
            .iter()
            .filter_map(|name| {
                let s = slot(name, layout)?;
                let shape = match s.shape {
                    Shape::Plain => "plain",
                    Shape::IsoEnter => "iso_enter",
                    Shape::Tall => "tall",
                };
                Some(format!("    {}: [{}, {}, {}, \"{shape}\"]", serde_json::to_string(name).unwrap(), s.row, s.x, s.w))
            })
            .collect();
        out.push_str(&entries.join(",\n"));
        out.push_str(if i + 1 < layouts.len() { "\n  },\n" } else { "\n  }\n" });
    }
    out.push_str("}\n");
    print!("{out}");
}
