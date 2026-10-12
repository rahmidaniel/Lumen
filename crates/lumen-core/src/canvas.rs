//! The virtual desk: where each device sits, how it is drawn, and where every
//! LED ends up.
//!
//! Coordinates are millimetres with the origin at the top-left and +y pointing
//! down (screen convention). Device geometry is computed here once and shared
//! by the renderer (which samples effects at each LED's world position) and the
//! UI (which draws the same glyphs), so the two can never disagree.

use std::collections::BTreeMap;

use openrgb_sdk::{Device, DeviceType, MatrixMap};
use serde::{Deserialize, Serialize};

use crate::keyboard::{self, Layout as KeyboardLayout, Shape};

/// A device's footprint on the desk.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// Degrees, clockwise, about the footprint centre.
    #[serde(default)]
    pub rotation: f32,
}

impl Placement {
    pub fn center(&self) -> (f32, f32) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    /// Map a point given in local footprint coordinates to desk coordinates.
    pub fn to_world(&self, lx: f32, ly: f32) -> (f32, f32) {
        let (cx, cy) = self.center();
        let (dx, dy) = (lx - self.width / 2.0, ly - self.height / 2.0);
        let (sin, cos) = self.rotation.to_radians().sin_cos();
        (cx + dx * cos - dy * sin, cy + dx * sin + dy * cos)
    }
}

/// Placements keyed by OpenRGB controller ID.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub placements: BTreeMap<u32, Placement>,
}

impl Layout {
    /// Give every device without a placement a sensible default. Returns true if anything was added.
    pub fn ensure_placements(&mut self, devices: &[Device]) -> bool {
        let mut added = false;
        let mut per_kind: BTreeMap<i32, usize> = BTreeMap::new();
        for d in devices {
            let ordinal = per_kind.entry(d.kind.0).or_insert(0);
            if let std::collections::btree_map::Entry::Vacant(slot) = self.placements.entry(d.id) {
                slot.insert(default_placement(d.kind, *ordinal));
                added = true;
            }
            *ordinal += 1;
        }
        added
    }
}

/// Typical real-world footprint (mm) and a starting position per device kind,
/// arranged like a desk: keyboard centre, mouse to the right, mat underneath,
/// PC internals below.
pub fn default_placement(kind: DeviceType, ordinal: usize) -> Placement {
    let (x, y, width, height) = match kind {
        DeviceType::KEYBOARD => (0.0, 0.0, 360.0, 140.0),
        DeviceType::KEYPAD => (-120.0, 20.0, 90.0, 110.0),
        DeviceType::MOUSE => (430.0, 8.0, 65.0, 125.0),
        DeviceType::MOUSEMAT => (-200.0, -120.0, 900.0, 400.0),
        DeviceType::HEADSET => (540.0, -80.0, 180.0, 200.0),
        DeviceType::HEADSET_STAND => (540.0, -80.0, 110.0, 260.0),
        DeviceType::LEDSTRIP => (-150.0, -170.0, 500.0, 15.0),
        DeviceType::LIGHT => (-150.0, -320.0, 120.0, 120.0),
        DeviceType::GPU => (0.0, 330.0, 290.0, 110.0),
        DeviceType::MOTHERBOARD => (-380.0, 300.0, 305.0, 244.0),
        DeviceType::DRAM => (-60.0, 300.0, 135.0, 40.0),
        DeviceType::COOLER => (-300.0, 300.0, 120.0, 120.0),
        _ => (0.0, 480.0, 150.0, 100.0),
    };
    Placement { x: x + ordinal as f32 * (width + 20.0), y, width, height, rotation: 0.0 }
}

/// Outline of a device body, in local millimetres.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Body {
    /// Generic rounded rectangle.
    Rect { radius: f32 },
    /// Keyboard case; `layout` is known when the keys were placed physically.
    Keyboard { radius: f32, layout: Option<KeyboardLayout> },
    /// Mouse mat with a lit edge.
    Mat { radius: f32 },
    /// Mouse silhouette.
    Mouse,
    /// Graphics card seen from the fan side: I/O bracket on the left (that
    /// many mm wide), decorative fans, lighting along the top edge.
    Gpu { radius: f32, bracket: f32, fans: u8 },
}

/// How one LED is drawn, centred on the glyph's (x, y) unless it is a strip.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Glyph {
    /// Keycap.
    Key { w: f32, h: f32 },
    /// ISO Enter keycap: `w` × `h` bounding box whose lower half is `lower_w` wide and flush right.
    IsoEnter { w: f32, h: f32, lower_w: f32 },
    /// Square cell of a non-keyboard matrix.
    Cell { w: f32, h: f32 },
    /// Point light.
    Dot { r: f32 },
    /// Lit section of an edge strip: a stroked polyline.
    Strip { points: Vec<[f32; 2]>, width: f32 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LedGlyph {
    pub zone: usize,
    /// LED index within the device (OpenRGB order).
    pub led: usize,
    pub x: f32,
    pub y: f32,
    pub glyph: Glyph,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DeviceGeometry {
    pub device: u32,
    pub width: f32,
    pub height: f32,
    pub body: Body,
    pub leds: Vec<LedGlyph>,
}

/// One LED's resolved position: desk coordinates plus normalized `(u, v)`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LedPoint {
    /// Index into the engine's device list.
    pub device_index: usize,
    /// OpenRGB controller ID.
    pub device: u32,
    pub zone: usize,
    /// LED index within the device (OpenRGB order).
    pub led: usize,
    pub x: f32,
    pub y: f32,
    pub u: f32,
    pub v: f32,
}

/// Geometry for every placed device plus the world-space LED points derived
/// from it. Devices without a placement are skipped.
pub fn build(devices: &[Device], layout: &Layout) -> (Vec<DeviceGeometry>, Vec<LedPoint>) {
    let mut geometry = Vec::new();
    let mut points = Vec::new();
    for (device_index, device) in devices.iter().enumerate() {
        let Some(placement) = layout.placements.get(&device.id) else { continue };
        let g = device_geometry(device, placement);
        for led in &g.leds {
            let (x, y) = placement.to_world(led.x, led.y);
            points.push(LedPoint { device_index, device: device.id, zone: led.zone, led: led.led, x, y, u: 0.0, v: 0.0 });
        }
        geometry.push(g);
    }
    normalize(&mut points);
    (geometry, points)
}

pub fn led_points(devices: &[Device], layout: &Layout) -> Vec<LedPoint> {
    build(devices, layout).1
}

fn normalize(points: &mut [LedPoint]) {
    if points.is_empty() {
        return;
    }
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in points.iter() {
        min_x = min_x.min(p.x);
        min_y = min_y.min(p.y);
        max_x = max_x.max(p.x);
        max_y = max_y.max(p.y);
    }
    for p in points.iter_mut() {
        p.u = if max_x - min_x > f32::EPSILON { (p.x - min_x) / (max_x - min_x) } else { 0.5 };
        p.v = if max_y - min_y > f32::EPSILON { (p.y - min_y) / (max_y - min_y) } else { 0.5 };
    }
}

/// Body shape and one glyph per LED, in the footprint's local coordinates.
///
/// Matrix zones become keycaps when the LEDs carry OpenRGB key names
/// (`"Key: …"`), otherwise cells. Multi-LED zones without a matrix run around
/// the edge on mouse mats and along rows elsewhere; single-LED zones stack down
/// the centre line.
pub fn device_geometry(device: &Device, p: &Placement) -> DeviceGeometry {
    let (w, h) = (p.width.max(1.0), p.height.max(1.0));
    let mat_radius = (w.min(h) * 0.06).clamp(4.0, 40.0);
    let gpu_bracket = (w * 0.045).clamp(6.0, 14.0);
    // Keyboards whose LED names follow OpenRGB's "Key: …" convention are laid
    // out on a real ANSI/ISO layout instead of the firmware's logical grid.
    let key_layout = matches!(device.kind, DeviceType::KEYBOARD | DeviceType::KEYPAD)
        .then(|| KeyboardLayout::detect(device.leds.iter().map(|l| l.name.as_str())))
        .flatten();
    let mut physical = false;

    let has_matrix = |z: &openrgb_sdk::Zone| z.matrix.as_ref().is_some_and(|m| m.width > 0 && m.height > 0);
    let line_zones = device.zones.iter().filter(|z| !has_matrix(z) && z.leds_count > 1).count();
    let single_zones = device.zones.iter().filter(|z| !has_matrix(z) && z.leds_count == 1).count();
    let (mut line_rank, mut single_rank, mut gpu_rank, mut offset) = (0usize, 0usize, 0usize, 0usize);

    let mut leds = Vec::with_capacity(device.leds.len());
    for (zi, zone) in device.zones.iter().enumerate() {
        let count = zone.leds_count as usize;
        match &zone.matrix {
            Some(m) if m.width > 0 && m.height > 0 => match key_layout {
                Some(layout) if physical_key_glyphs(device, zi, offset, m, layout, w, h, &mut leds) => physical = true,
                _ => matrix_glyphs(device, zi, offset, m, w, h, &mut leds),
            },
            _ if device.kind == DeviceType::GPU && count >= 1 => {
                gpu_bar_glyphs(zi, offset, count, w, h, gpu_bracket, gpu_rank, &mut leds);
                gpu_rank += 1;
            }
            _ if count == 1 => {
                let y = h * (single_rank as f32 + 1.0) / (single_zones as f32 + 1.0);
                single_rank += 1;
                let r = (w.min(h) * 0.09).clamp(2.0, 8.0);
                leds.push(LedGlyph { zone: zi, led: offset, x: w / 2.0, y, glyph: Glyph::Dot { r } });
            }
            _ if count > 1 => {
                if device.kind == DeviceType::MOUSEMAT {
                    strip_glyphs(zi, offset, count, w, h, mat_radius, &mut leds);
                } else {
                    let y = h * (line_rank as f32 + 1.0) / (line_zones as f32 + 1.0);
                    line_rank += 1;
                    line_glyphs(zi, offset, count, w, h, y, &mut leds);
                }
            }
            _ => {}
        }
        offset += count;
    }
    leds.retain(|g| g.led < device.leds.len());

    let body = match device.kind {
        DeviceType::KEYBOARD | DeviceType::KEYPAD => {
            Body::Keyboard { radius: (w.min(h) * 0.04).clamp(2.0, 8.0), layout: key_layout.filter(|_| physical) }
        }
        DeviceType::MOUSEMAT => Body::Mat { radius: mat_radius },
        DeviceType::MOUSE => Body::Mouse,
        DeviceType::GPU => Body::Gpu {
            radius: (h * 0.06).clamp(2.0, 8.0),
            bracket: gpu_bracket,
            fans: (((w - gpu_bracket) / (h * 0.95)).round() as i32).clamp(1, 3) as u8,
        },
        _ => Body::Rect { radius: (w.min(h) * 0.08).clamp(2.0, 12.0) },
    };
    DeviceGeometry { device: device.id, width: w, height: h, body, leds }
}

/// Keycap width in key units for OpenRGB's standard key names; 1u otherwise.
fn key_units(name: &str) -> f32 {
    match name {
        "Key: Space" => 6.25,
        "Key: Right Shift" => 2.75,
        "Key: Enter" | "Key: Left Shift" => 2.25,
        "Key: Backspace" | "Key: Number Pad 0" => 2.0,
        "Key: Caps Lock" => 1.75,
        "Key: Tab" | "Key: \\ (ANSI)" => 1.5,
        "Key: Left Control" | "Key: Left Windows" | "Key: Left Alt" | "Key: Right Alt" | "Key: Right Fn" | "Key: Menu"
        | "Key: Right Control" | "Key: Right Windows" => 1.25,
        _ => 1.0,
    }
}

/// Lay a matrix zone out as a real keyboard: standard key names go to their
/// physical slots (see [`keyboard::slot`]), other named LEDs keep their relative
/// matrix position, unnamed filler cells get no glyph. The result is scaled to
/// fit the device footprint with the key proportions kept. Returns `false`
/// when no key could be placed, so the caller can fall back to the grid.
#[allow(clippy::too_many_arguments)]
fn physical_key_glyphs(
    device: &Device,
    zone: usize,
    offset: usize,
    m: &MatrixMap,
    layout: KeyboardLayout,
    w: f32,
    h: f32,
    out: &mut Vec<LedGlyph>,
) -> bool {
    struct Item {
        led: usize,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        shape: Shape,
    }
    let mut items: Vec<Item> = Vec::new();
    let mut extras: Vec<(usize, u32, u32)> = Vec::new();
    for row in 0..m.height {
        for col in 0..m.width {
            let Some(i) = m.led_at(row, col) else { continue };
            let led = offset + i as usize;
            let name = device.leds.get(led).map(|l| l.name.as_str()).unwrap_or("");
            if name.is_empty() {
                continue;
            }
            match keyboard::slot(name, layout) {
                Some(s) => items.push(Item { led, x: s.x, y: s.y(), w: s.w, h: s.h(), shape: s.shape }),
                None => extras.push((led, row, col)),
            }
        }
    }
    if items.is_empty() {
        return false;
    }
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for it in &items {
        min_x = min_x.min(it.x);
        max_x = max_x.max(it.x + it.w);
        min_y = min_y.min(it.y);
        max_y = max_y.max(it.y + it.h);
    }
    // LEDs with non-standard names (media keys, logos) keep their matrix
    // position, scaled into the same span as the placed keys.
    for (led, row, col) in extras {
        let x = min_x + (col as f32 + 0.5) / m.width as f32 * (max_x - min_x) - 0.5;
        let y = if m.height == 6 {
            keyboard::row_y(row as u8)
        } else {
            min_y + row as f32 / (m.height.max(2) - 1) as f32 * (max_y - 1.0 - min_y)
        };
        items.push(Item { led, x, y, w: 1.0, h: 1.0, shape: Shape::Plain });
    }

    let margin = 0.4;
    let span_x = max_x - min_x + 2.0 * margin;
    let span_y = max_y - min_y + 2.0 * margin;
    let unit = (w / span_x).min(h / span_y);
    let ox = (w - span_x * unit) / 2.0 + (margin - min_x) * unit;
    let oy = (h - span_y * unit) / 2.0 + (margin - min_y) * unit;
    let gap = unit * 0.1;
    for it in items {
        let glyph = match it.shape {
            Shape::IsoEnter => Glyph::IsoEnter { w: it.w * unit - gap, h: it.h * unit - gap, lower_w: 1.25 * unit - gap },
            _ => Glyph::Key { w: it.w * unit - gap, h: it.h * unit - gap },
        };
        out.push(LedGlyph { zone, led: it.led, x: ox + (it.x + it.w / 2.0) * unit, y: oy + (it.y + it.h / 2.0) * unit, glyph });
    }
    true
}

fn matrix_glyphs(device: &Device, zone: usize, offset: usize, m: &MatrixMap, w: f32, h: f32, out: &mut Vec<LedGlyph>) {
    let cell_w = w / m.width as f32;
    let cell_h = h / m.height as f32;
    let pad_x = cell_w * 0.07;
    let pad_y = cell_h * 0.09;
    let keyboard = device.leds.iter().any(|l| l.name.starts_with("Key:"));
    // Tiny "matrices" (a mouse reporting a 1×1 grid) are really point lights.
    let dots = !keyboard && (device.kind == DeviceType::MOUSE || m.width * m.height < 4);
    let dot_r = (cell_w.min(cell_h) * 0.3).min(w.min(h) * 0.09).clamp(1.5, 8.0);

    for row in 0..m.height {
        let lit: Vec<(u32, usize)> =
            (0..m.width).filter_map(|col| m.led_at(row, col).map(|i| (col, offset + i as usize))).collect();
        for (i, &(col, led)) in lit.iter().enumerate() {
            let cx = (col as f32 + 0.5) * cell_w;
            let cy = (row as f32 + 0.5) * cell_h;
            if dots {
                out.push(LedGlyph { zone, led, x: cx, y: cy, glyph: Glyph::Dot { r: dot_r } });
                continue;
            }
            if !keyboard {
                out.push(LedGlyph { zone, led, x: cx, y: cy, glyph: Glyph::Cell { w: cell_w - 2.0 * pad_x, h: cell_h - 2.0 * pad_y } });
                continue;
            }

            // A keycap may grow into empty neighbouring cells (that is where the
            // matrix leaves room for wide keys) up to its nominal width.
            let name = device.leds.get(led).map(|l| l.name.as_str()).unwrap_or("");
            let cap = key_units(name) * cell_w - 2.0 * pad_x;
            let left_mid = (i > 0).then(|| ((lit[i - 1].0 as f32 + 0.5) * cell_w + cx) / 2.0);
            let right_mid = lit.get(i + 1).map(|&(nc, _)| ((nc as f32 + 0.5) * cell_w + cx) / 2.0);
            let lo = left_mid.unwrap_or(0.0) + pad_x;
            let hi = right_mid.unwrap_or(w) - pad_x;
            let kw = cap.min(hi - lo).max(cell_w * 0.5);
            let wide = cap > cell_w;
            let kx = match (wide, left_mid, right_mid) {
                (true, None, Some(_)) => hi - kw / 2.0, // row start: grow leftwards from the neighbour
                (true, Some(_), None) => lo + kw / 2.0, // row end: grow rightwards from the neighbour
                _ => cx.clamp(lo + kw / 2.0, hi - kw / 2.0),
            };
            out.push(LedGlyph { zone, led, x: kx, y: cy, glyph: Glyph::Key { w: kw, h: cell_h - 2.0 * pad_y } });
        }
    }
}

fn line_glyphs(zone: usize, offset: usize, count: usize, w: f32, h: f32, y: f32, out: &mut Vec<LedGlyph>) {
    let pitch = w / count as f32;
    let r = (pitch * 0.35).min(h * 0.3).clamp(1.5, 6.0);
    for i in 0..count {
        out.push(LedGlyph { zone, led: offset + i, x: pitch * (i as f32 + 0.5), y, glyph: Glyph::Dot { r } });
    }
}

/// Graphics card lighting: a segmented light bar along the top edge (the edge
/// that faces the side panel), right of centre where logos sit. A second zone
/// gets the bottom edge; further ones fall back to a row across the card.
#[allow(clippy::too_many_arguments)]
fn gpu_bar_glyphs(zone: usize, offset: usize, count: usize, w: f32, h: f32, bracket: f32, rank: usize, out: &mut Vec<LedGlyph>) {
    if rank >= 2 {
        return line_glyphs(zone, offset, count, w, h, h * 0.5, out);
    }
    let width = (h * 0.055).clamp(3.0, 8.0);
    let inset = width / 2.0 + (h * 0.05).clamp(3.0, 6.0);
    let y = if rank == 0 { inset } else { h - inset };
    let card = w - bracket;
    let x0 = bracket + card * if count == 1 { 0.6 } else { 0.36 };
    let x1 = w - card * 0.05;
    let seg = (x1 - x0) / count as f32;
    let gap = (seg * 0.12).min(3.0);
    for i in 0..count {
        let a = x0 + seg * i as f32 + gap;
        let b = x0 + seg * (i as f32 + 1.0) - gap;
        out.push(LedGlyph { zone, led: offset + i, x: (a + b) / 2.0, y, glyph: Glyph::Strip { points: vec![[a, y], [b, y]], width } });
    }
}

/// Edge-lit mat: the LEDs split a clockwise loop just inside the rounded
/// outline, starting at the top-left corner.
fn strip_glyphs(zone: usize, offset: usize, count: usize, w: f32, h: f32, radius: f32, out: &mut Vec<LedGlyph>) {
    let width = (w.min(h) * 0.025).clamp(3.0, 10.0);
    let path = EdgePath::rounded_loop(w, h, radius, width / 2.0 + 1.0);
    let seg = path.length / count as f32;
    let gap = (seg * 0.12).min(3.0);
    for i in 0..count {
        let s0 = i as f32 * seg + gap;
        let s1 = (i + 1) as f32 * seg - gap;
        let samples = (((s1 - s0) / 6.0).ceil() as usize).max(4);
        let points = (0..=samples).map(|k| path.point_at(s0 + (s1 - s0) * k as f32 / samples as f32)).collect();
        let [x, y] = path.point_at((i as f32 + 0.5) * seg);
        out.push(LedGlyph { zone, led: offset + i, x, y, glyph: Glyph::Strip { points, width } });
    }
}

enum Piece {
    Line { from: [f32; 2], to: [f32; 2], len: f32 },
    Arc { center: [f32; 2], r: f32, a0: f32, a1: f32, len: f32 },
}

impl Piece {
    fn len(&self) -> f32 {
        match self {
            Piece::Line { len, .. } | Piece::Arc { len, .. } => *len,
        }
    }

    fn at(&self, t: f32) -> [f32; 2] {
        match self {
            Piece::Line { from, to, .. } => [from[0] + (to[0] - from[0]) * t, from[1] + (to[1] - from[1]) * t],
            Piece::Arc { center, r, a0, a1, .. } => {
                let a = a0 + (a1 - a0) * t;
                [center[0] + r * a.cos(), center[1] + r * a.sin()]
            }
        }
    }
}

/// A closed path made of straight runs and quarter arcs, parametrized by arc length.
struct EdgePath {
    pieces: Vec<Piece>,
    length: f32,
}

impl EdgePath {
    /// Clockwise (in screen coordinates) loop inset into a `w`×`h` rounded
    /// rectangle, starting where the top edge leaves the top-left corner.
    fn rounded_loop(w: f32, h: f32, radius: f32, inset: f32) -> Self {
        use std::f32::consts::{FRAC_PI_2, PI};
        let r = (radius - inset).max(0.5);
        let (l, t, rt, b) = (inset, inset, w - inset, h - inset);
        let line = |from: [f32; 2], to: [f32; 2]| Piece::Line { from, to, len: ((to[0] - from[0]).powi(2) + (to[1] - from[1]).powi(2)).sqrt() };
        let arc = |center: [f32; 2], a0: f32, a1: f32| Piece::Arc { center, r, a0, a1, len: r * (a1 - a0).abs() };
        let pieces = vec![
            line([l + r, t], [rt - r, t]),
            arc([rt - r, t + r], -FRAC_PI_2, 0.0),
            line([rt, t + r], [rt, b - r]),
            arc([rt - r, b - r], 0.0, FRAC_PI_2),
            line([rt - r, b], [l + r, b]),
            arc([l + r, b - r], FRAC_PI_2, PI),
            line([l, b - r], [l, t + r]),
            arc([l + r, t + r], PI, PI + FRAC_PI_2),
        ];
        let length = pieces.iter().map(Piece::len).sum();
        Self { pieces, length }
    }

    fn point_at(&self, s: f32) -> [f32; 2] {
        let mut d = s.rem_euclid(self.length);
        for piece in &self.pieces {
            if d <= piece.len() {
                return piece.at(if piece.len() > 0.0 { d / piece.len() } else { 0.0 });
            }
            d -= piece.len();
        }
        self.pieces.last().map(|p| p.at(1.0)).unwrap_or([0.0, 0.0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openrgb_sdk::{Led, Zone, ZoneType};

    fn zone(name: &str, kind: ZoneType, count: u32, matrix: Option<MatrixMap>) -> Zone {
        Zone {
            name: name.into(),
            kind,
            leds_min: count,
            leds_max: count,
            leds_count: count,
            matrix,
            segments: vec![],
            flags: 0,
            active_mode: None,
            modes: vec![],
            display_name: None,
        }
    }

    fn device(id: u32, kind: DeviceType, zones: Vec<Zone>, names: Option<Vec<&str>>) -> Device {
        let n: u32 = zones.iter().map(|z| z.leds_count).sum();
        let leds = match names {
            Some(names) => names.into_iter().enumerate().map(|(i, n)| Led { name: n.into(), value: i as u32 }).collect(),
            None => (0..n).map(|i| Led { name: format!("LED {i}"), value: i }).collect(),
        };
        Device {
            id,
            kind,
            name: format!("dev{id}"),
            vendor: String::new(),
            description: String::new(),
            version: String::new(),
            serial: String::new(),
            location: String::new(),
            modes: vec![],
            active_mode: 0,
            zones,
            leds,
            colors: vec![],
            led_display_names: vec![],
            flags: 0,
            display_name: String::new(),
            configuration: String::new(),
        }
    }

    fn placement(w: f32, h: f32) -> Placement {
        Placement { x: 0.0, y: 0.0, width: w, height: h, rotation: 0.0 }
    }

    #[test]
    fn placement_rotation_about_center() {
        let p = Placement { x: 0.0, y: 0.0, width: 100.0, height: 50.0, rotation: 90.0 };
        let (x, y) = p.to_world(0.0, 0.0); // top-left corner rotated 90° cw about (50, 25)
        assert!((x - 75.0).abs() < 1e-3 && (y + 25.0).abs() < 1e-3, "got ({x}, {y})");
    }

    #[test]
    fn plain_matrix_becomes_cells_and_skips_gaps() {
        let m = MatrixMap { height: 2, width: 2, map: vec![0, MatrixMap::NO_LED, 1, 2] };
        let d = device(1, DeviceType::LEDSTRIP, vec![zone("Grid", ZoneType::MATRIX, 3, Some(m))], None);
        let g = device_geometry(&d, &placement(100.0, 100.0));
        assert_eq!(g.leds.len(), 3);
        assert!(g.leds.iter().all(|l| matches!(l.glyph, Glyph::Cell { .. })));
        assert_eq!(g.leds.iter().map(|l| l.led).collect::<Vec<_>>(), vec![0, 1, 2]);
        assert!(g.leds[0].x < g.leds[2].x && g.leds[0].y < g.leds[2].y);
    }

    #[test]
    fn keycaps_grow_into_gaps_up_to_their_nominal_width() {
        // Row: [LShift, Z, X, _, _, Space, _, _, Enter] in a 9-column matrix.
        let n = MatrixMap::NO_LED;
        let m = MatrixMap { height: 1, width: 9, map: vec![0, 1, 2, n, n, 3, n, n, 4] };
        let names = vec!["Key: Left Shift", "Key: Z", "Key: X", "Key: Space", "Key: Enter"];
        let d = device(1, DeviceType::KEYBOARD, vec![zone("Keys", ZoneType::MATRIX, 5, Some(m.clone()))], Some(names));
        // Standard names take the physical layout; this exercises the grid fallback directly.
        let mut leds = Vec::new();
        matrix_glyphs(&d, 0, 0, &m, 180.0, 20.0, &mut leds); // 20 mm cells
        let g = DeviceGeometry { device: 1, width: 180.0, height: 20.0, body: Body::Rect { radius: 0.0 }, leds };
        let width = |i: usize| match g.leds[i].glyph {
            Glyph::Key { w, .. } => w,
            _ => panic!("expected keycaps"),
        };
        let (shift, z, x, space, enter) = (width(0), width(1), width(2), width(3), width(4));
        assert!((z - x).abs() < 1e-3 && z < 20.0, "plain keys are one cell wide: {z}");
        assert!(shift > z && shift <= 2.25 * 20.0, "shift grows: {shift}");
        assert!(space > 2.5 * 20.0, "space fills the gap: {space}");
        assert!(space <= 6.25 * 20.0);
        assert!(enter > z && g.leds[4].x < 180.0, "enter grows to the right but stays on the board");
        // Keycaps never overlap.
        let mut spans: Vec<(f32, f32)> = g.leds.iter().map(|l| match l.glyph {
            Glyph::Key { w, .. } => (l.x - w / 2.0, l.x + w / 2.0),
            _ => unreachable!(),
        }).collect();
        spans.sort_by(|a, b| a.0.total_cmp(&b.0));
        for pair in spans.windows(2) {
            assert!(pair[0].1 <= pair[1].0 + 1e-3, "overlap: {pair:?}");
        }
    }

    #[test]
    fn mousemat_strip_runs_around_the_edge() {
        let d = device(2, DeviceType::MOUSEMAT, vec![zone("Mat", ZoneType::LINEAR, 19, None)], None);
        let g = device_geometry(&d, &placement(900.0, 400.0));
        assert!(matches!(g.body, Body::Mat { .. }));
        assert_eq!(g.leds.len(), 19);
        for l in &g.leds {
            let Glyph::Strip { ref points, width } = l.glyph else { panic!("expected strips") };
            assert!(points.len() >= 5 && width > 0.0);
            let edge_dist = l.x.min(900.0 - l.x).min(l.y).min(400.0 - l.y);
            assert!(edge_dist < 30.0, "LED {} is {edge_dist} mm from the edge", l.led);
        }
        assert!(g.leds[0].y < 10.0 && g.leds[0].x < 450.0, "LED 0 starts on the top edge, left half");
    }

    #[test]
    fn single_zones_stack_down_the_centre() {
        let zones = vec![zone("Scroll", ZoneType::SINGLE, 1, None), zone("Logo", ZoneType::SINGLE, 1, None)];
        let d = device(3, DeviceType::MOUSE, zones, None);
        let g = device_geometry(&d, &placement(60.0, 120.0));
        assert_eq!(g.body, Body::Mouse);
        assert_eq!(g.leds.len(), 2);
        assert!((g.leds[0].x - 30.0).abs() < 1e-3 && g.leds[0].y < g.leds[1].y);
    }

    fn tkl_names() -> Vec<&'static str> {
        vec![
            "Key: Escape", "", "Key: F1", "Key: F2", "Key: F3", "Key: F4", "Key: F5", "Key: F6", "Key: F7", "Key: F8", "Key: F9", "Key: F10", "Key: F11", "Key: F12", "", "", "",
            "Key: `", "Key: 1", "Key: 2", "Key: 3", "Key: 4", "Key: 5", "Key: 6", "Key: 7", "Key: 8", "Key: 9", "Key: 0", "Key: -", "Key: =", "Key: Backspace", "Key: Insert", "Key: Home", "Key: Page Up",
            "Key: Tab", "Key: Q", "Key: W", "Key: E", "Key: R", "Key: T", "Key: Y", "Key: U", "Key: I", "Key: O", "Key: P", "Key: [", "Key: ]", "Key: \\ (ANSI)", "Key: Delete", "Key: End", "Key: Page Down",
            "Key: Caps Lock", "Key: A", "Key: S", "Key: D", "Key: F", "Key: G", "Key: H", "Key: J", "Key: K", "Key: L", "Key: ;", "Key: '", "", "Key: Enter", "", "", "",
            "Key: Left Shift", "", "Key: Z", "Key: X", "Key: C", "Key: V", "Key: B", "Key: N", "Key: M", "Key: ,", "Key: .", "Key: /", "", "Key: Right Shift", "", "Key: Up Arrow", "",
            "Key: Left Control", "Key: Left Windows", "Key: Left Alt", "", "", "", "Key: Space", "", "", "", "Key: Right Alt", "Key: Right Fn", "Key: Menu", "Key: Right Control", "Key: Left Arrow", "Key: Down Arrow", "Key: Right Arrow",
        ]
    }

    #[test]
    fn standard_key_names_get_a_physical_layout() {
        // The DeathStalker V2 Pro TKL as OpenRGB reports it: a full 17×6 grid with unnamed filler cells.
        let names = tkl_names();
        assert_eq!(names.len(), 102);
        let m = MatrixMap { height: 6, width: 17, map: (0..102).collect() };
        let d = device(1, DeviceType::KEYBOARD, vec![zone("Keyboard", ZoneType::MATRIX, 102, Some(m))], Some(names.clone()));
        let g = device_geometry(&d, &placement(360.0, 140.0));
        assert!(matches!(g.body, Body::Keyboard { layout: Some(KeyboardLayout::Ansi), .. }), "{:?}", g.body);
        assert_eq!(g.leds.len(), names.iter().filter(|n| !n.is_empty()).count(), "filler cells get no glyph");
        let width = |key: &str| {
            let i = names.iter().position(|n| *n == key).unwrap();
            match g.leds.iter().find(|l| l.led == i).unwrap().glyph {
                Glyph::Key { w, .. } => w,
                ref other => panic!("{key}: {other:?}"),
            }
        };
        let unit = width("Key: 1") / 0.9; // caps are their nominal width minus a 0.1 u gap
        assert!((width("Key: Backspace") - 1.9 * unit).abs() < 1e-3);
        assert!((width("Key: Enter") - 2.15 * unit).abs() < 1e-3);
        assert!((width("Key: Space") - 6.15 * unit).abs() < 1e-3);
        assert!((width("Key: Left Shift") - 2.15 * unit).abs() < 1e-3);
        for l in &g.leds {
            assert!(l.x > 0.0 && l.x < 360.0 && l.y > 0.0 && l.y < 140.0, "{l:?}");
        }
    }

    #[test]
    fn iso_names_give_an_iso_enter_and_short_left_shift() {
        let names = vec!["Key: Left Shift", "Key: \\ (ISO)", "Key: Z", "Key: #", "Key: Enter"];
        let m = MatrixMap { height: 1, width: 5, map: (0..5).collect() };
        let d = device(1, DeviceType::KEYBOARD, vec![zone("Keyboard", ZoneType::MATRIX, 5, Some(m))], Some(names));
        let g = device_geometry(&d, &placement(360.0, 140.0));
        assert!(matches!(g.body, Body::Keyboard { layout: Some(KeyboardLayout::Iso), .. }));
        let unit = match g.leds[2].glyph {
            Glyph::Key { w, .. } => w / 0.9,
            ref o => panic!("{o:?}"),
        };
        match g.leds[0].glyph {
            Glyph::Key { w, .. } => assert!((w - 1.15 * unit).abs() < 1e-3, "ISO left shift is 1.25 u"),
            ref o => panic!("{o:?}"),
        }
        assert!(matches!(g.leds[4].glyph, Glyph::IsoEnter { .. }));
        assert!(g.leds[4].y < g.leds[0].y, "ISO Enter is centred above the shift row");
    }

    #[test]
    fn gpu_lighting_is_a_bar_along_the_top_edge() {
        let d = device(4, DeviceType::GPU, vec![zone("GPU", ZoneType::LINEAR, 4, None)], None);
        let g = device_geometry(&d, &placement(290.0, 110.0));
        let Body::Gpu { bracket, fans, .. } = g.body else { panic!("expected a GPU body, got {:?}", g.body) };
        assert_eq!(fans, 3);
        assert_eq!(g.leds.len(), 4);
        for pair in g.leds.windows(2) {
            assert!(pair[0].x < pair[1].x, "segments run left to right");
        }
        for led in &g.leds {
            assert!(led.y < 110.0 * 0.15, "bar sits on the top edge: y = {}", led.y);
            assert!(led.x > bracket && led.x < 290.0, "bar stays on the card: x = {}", led.x);
            assert!(matches!(led.glyph, Glyph::Strip { .. }));
        }
    }

    #[test]
    fn edge_path_is_continuous_and_closed() {
        let path = EdgePath::rounded_loop(100.0, 50.0, 10.0, 2.0);
        let start = path.point_at(0.0);
        let end = path.point_at(path.length - 1e-3);
        assert!((start[0] - end[0]).abs() < 0.1 && (start[1] - end[1]).abs() < 0.1, "{start:?} vs {end:?}");
        let mut prev = start;
        for i in 1..=200 {
            let p = path.point_at(path.length * i as f32 / 200.0);
            let step = ((p[0] - prev[0]).powi(2) + (p[1] - prev[1]).powi(2)).sqrt();
            assert!(step < path.length / 200.0 * 1.5 + 0.5, "jump at sample {i}: {step}");
            prev = p;
        }
    }

    #[test]
    fn world_points_are_normalized_across_devices() {
        let kb = device(1, DeviceType::KEYBOARD, vec![zone("Keys", ZoneType::LINEAR, 4, None)], None);
        let mouse = device(2, DeviceType::MOUSE, vec![zone("Logo", ZoneType::SINGLE, 1, None)], None);
        let mut layout = Layout::default();
        assert!(layout.ensure_placements(&[kb.clone(), mouse.clone()]));
        let (geometry, pts) = build(&[kb, mouse], &layout);
        assert_eq!(geometry.len(), 2);
        let (min_u, max_u) = pts.iter().fold((1.0f32, 0.0f32), |(lo, hi), p| (lo.min(p.u), hi.max(p.u)));
        assert!(min_u.abs() < 1e-6 && (max_u - 1.0).abs() < 1e-6);
        assert_eq!(pts.last().unwrap().device_index, 1);
    }
}
