//! Effects are pure functions of `(u, v, t)` plus a per-LED seed. Colours are
//! floats in 0..1 (sRGB, no gamma handling yet) until the final conversion.

use std::f64::consts::TAU;

use openrgb_sdk::Color;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct Rgb {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Rgb {
    pub const BLACK: Rgb = Rgb { r: 0.0, g: 0.0, b: 0.0 };
    pub const WHITE: Rgb = Rgb { r: 1.0, g: 1.0, b: 1.0 };

    pub const fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    pub fn from_u8(r: u8, g: u8, b: u8) -> Self {
        Self::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
    }

    pub fn from_color(c: Color) -> Self {
        Self::from_u8(c.r, c.g, c.b)
    }

    pub fn to_color(self) -> Color {
        let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        Color::new(q(self.r), q(self.g), q(self.b))
    }

    /// `h` in 0..1 (wraps), `s` and `v` in 0..1.
    pub fn from_hsv(h: f32, s: f32, v: f32) -> Self {
        let h6 = (h.rem_euclid(1.0)) * 6.0;
        let i = h6.floor();
        let f = h6 - i;
        let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
        let (r, g, b) = match i as i32 {
            0 => (v, t, p),
            1 => (q, v, p),
            2 => (p, v, t),
            3 => (p, q, v),
            4 => (t, p, v),
            _ => (v, p, q),
        };
        Self::new(r, g, b)
    }

    pub fn scale(self, k: f32) -> Self {
        Self::new(self.r * k, self.g * k, self.b * k)
    }

    pub fn lerp(a: Rgb, b: Rgb, t: f32) -> Self {
        Self::new(a.r + (b.r - a.r) * t, a.g + (b.g - a.g) * t, a.b + (b.b - a.b) * t)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub rgb: Rgb,
    pub a: f32,
}

impl Rgba {
    pub const TRANSPARENT: Rgba = Rgba { rgb: Rgb::BLACK, a: 0.0 };

    pub fn opaque(rgb: Rgb) -> Self {
        Self { rgb, a: 1.0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stop {
    /// Position along the gradient, 0..1.
    pub pos: f32,
    pub color: Rgb,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Palette {
    Rainbow,
    Stops { stops: Vec<Stop> },
}

impl Palette {
    /// Sample at `t` in 0..1. Stop palettes wrap so waves loop seamlessly.
    pub fn sample(&self, t: f32) -> Rgb {
        match self {
            Palette::Rainbow => Rgb::from_hsv(t, 1.0, 1.0),
            Palette::Stops { stops } => sample_stops(stops, t.rem_euclid(1.0), true),
        }
    }
}

/// Piecewise-linear interpolation over stops sorted by `pos`.
fn sample_stops(stops: &[Stop], t: f32, wrap: bool) -> Rgb {
    match stops {
        [] => Rgb::BLACK,
        [only] => only.color,
        _ => {
            let mut sorted: Vec<Stop> = stops.to_vec();
            sorted.sort_by(|a, b| a.pos.total_cmp(&b.pos));
            let first = sorted[0];
            let last = sorted[sorted.len() - 1];
            if t <= first.pos {
                return if wrap { wrap_lerp(last, first, t + 1.0) } else { first.color };
            }
            if t >= last.pos {
                return if wrap { wrap_lerp(last, first, t) } else { last.color };
            }
            for pair in sorted.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                if t >= a.pos && t <= b.pos {
                    let span = (b.pos - a.pos).max(f32::EPSILON);
                    return Rgb::lerp(a.color, b.color, (t - a.pos) / span);
                }
            }
            last.color
        }
    }
}

/// Interpolate across the wrap-around gap from `last` (at its pos) to `first` (at pos + 1).
fn wrap_lerp(last: Stop, first: Stop, t: f32) -> Rgb {
    let span = (first.pos + 1.0 - last.pos).max(f32::EPSILON);
    Rgb::lerp(last.color, first.color, ((t - last.pos) / span).clamp(0.0, 1.0))
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Effect {
    /// One colour everywhere.
    Static { color: Rgb },
    /// Fixed gradient across the canvas along `angle` (degrees; 0 = left→right).
    Gradient {
        stops: Vec<Stop>,
        #[serde(default)]
        angle: f32,
    },
    /// Whole canvas cycles through the hue wheel; `speed` in cycles per second.
    Spectrum { speed: f32 },
    /// Palette travels across the canvas. `wavelength` in canvas widths,
    /// `speed` in cycles per second, `angle` in degrees.
    Wave { angle: f32, speed: f32, wavelength: f32, palette: Palette },
    /// Colour fades in and out; `speed` in breaths per second.
    Breathing { color: Rgb, speed: f32 },
    /// Random LEDs twinkle. `density` 0..1 is the share of LEDs lit per cycle,
    /// `speed` is twinkles per second per LED.
    Starlight { color: Rgb, density: f32, speed: f32 },
}

/// Inputs for one LED sample.
#[derive(Clone, Copy, Debug)]
pub struct SampleCtx {
    pub u: f32,
    pub v: f32,
    /// Seconds since the engine started.
    pub t: f64,
    /// Stable per-LED hash, for effects that need randomness.
    pub seed: u32,
}

impl Effect {
    /// Whether the output changes over time. Static effects are rendered once
    /// and the engine idles until something edits the scene.
    pub fn is_animated(&self) -> bool {
        match self {
            Effect::Static { .. } | Effect::Gradient { .. } => false,
            Effect::Spectrum { speed } | Effect::Wave { speed, .. } | Effect::Breathing { speed, .. } => *speed != 0.0,
            Effect::Starlight { .. } => true,
        }
    }

    pub fn sample(&self, ctx: &SampleCtx) -> Rgba {
        match self {
            Effect::Static { color } => Rgba::opaque(*color),
            Effect::Gradient { stops, angle } => {
                let p = project(ctx.u, ctx.v, *angle).clamp(0.0, 1.0);
                Rgba::opaque(sample_stops(stops, p, false))
            }
            Effect::Spectrum { speed } => {
                let hue = (ctx.t * *speed as f64).rem_euclid(1.0) as f32;
                Rgba::opaque(Rgb::from_hsv(hue, 1.0, 1.0))
            }
            Effect::Wave { angle, speed, wavelength, palette } => {
                let p = project(ctx.u, ctx.v, *angle) as f64;
                let wl = (*wavelength as f64).max(1e-3);
                let phase = (p / wl - ctx.t * *speed as f64).rem_euclid(1.0) as f32;
                Rgba::opaque(palette.sample(phase))
            }
            Effect::Breathing { color, speed } => {
                let k = 0.5 - 0.5 * (TAU * ctx.t * *speed as f64).cos();
                Rgba::opaque(color.scale(k as f32))
            }
            Effect::Starlight { color, density, speed } => {
                let cycle_len = 1.0 / (*speed as f64).max(1e-3);
                let local = ctx.t + hash01(ctx.seed) as f64 * cycle_len;
                let cycle = (local / cycle_len).floor();
                let frac = (local / cycle_len - cycle) as f32;
                let roll = hash01(ctx.seed ^ (cycle as u32).wrapping_mul(0x9E37_79B9));
                if roll < *density {
                    let intensity = 1.0 - (2.0 * frac - 1.0).abs();
                    Rgba { rgb: *color, a: intensity }
                } else {
                    Rgba::TRANSPARENT
                }
            }
        }
    }
}

/// Project `(u, v)` onto the unit direction at `angle` degrees, keeping 0.5 at the centre.
fn project(u: f32, v: f32, angle: f32) -> f32 {
    let (sin, cos) = angle.to_radians().sin_cos();
    (u - 0.5) * cos + (v - 0.5) * sin + 0.5
}

/// Integer hash → 0..1. (lowbias32 by Chris Wellons.)
pub fn hash01(mut x: u32) -> f32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(u: f32, v: f32, t: f64) -> SampleCtx {
        SampleCtx { u, v, t, seed: 42 }
    }

    #[test]
    fn hsv_primaries() {
        assert_eq!(Rgb::from_hsv(0.0, 1.0, 1.0).to_color(), Color::new(255, 0, 0));
        assert_eq!(Rgb::from_hsv(1.0 / 3.0, 1.0, 1.0).to_color(), Color::new(0, 255, 0));
        assert_eq!(Rgb::from_hsv(2.0 / 3.0, 1.0, 1.0).to_color(), Color::new(0, 0, 255));
        assert_eq!(Rgb::from_hsv(1.0, 1.0, 1.0).to_color(), Color::new(255, 0, 0), "hue wraps");
    }

    #[test]
    fn gradient_interpolates_and_clamps() {
        let g = Effect::Gradient {
            stops: vec![Stop { pos: 0.0, color: Rgb::BLACK }, Stop { pos: 1.0, color: Rgb::WHITE }],
            angle: 0.0,
        };
        assert_eq!(g.sample(&ctx(0.0, 0.5, 0.0)).rgb.to_color(), Color::new(0, 0, 0));
        assert_eq!(g.sample(&ctx(0.5, 0.5, 0.0)).rgb.to_color(), Color::new(128, 128, 128));
        assert_eq!(g.sample(&ctx(1.0, 0.5, 0.0)).rgb.to_color(), Color::new(255, 255, 255));
    }

    #[test]
    fn wave_moves_with_time() {
        let w = Effect::Wave { angle: 0.0, speed: 1.0, wavelength: 1.0, palette: Palette::Rainbow };
        let at_zero = w.sample(&ctx(0.0, 0.5, 0.0)).rgb;
        let quarter_later = w.sample(&ctx(0.0, 0.5, 0.25)).rgb;
        let full_cycle = w.sample(&ctx(0.0, 0.5, 1.0)).rgb;
        assert_ne!(at_zero.to_color(), quarter_later.to_color());
        assert_eq!(at_zero.to_color(), full_cycle.to_color(), "period is 1/speed");
        // Travelling right: the colour at u=0.25, t=0 appears at u=0.5 when t=0.25.
        assert_eq!(w.sample(&ctx(0.25, 0.5, 0.0)).rgb.to_color(), w.sample(&ctx(0.5, 0.5, 0.25)).rgb.to_color());
    }

    #[test]
    fn breathing_hits_zero_and_full() {
        let b = Effect::Breathing { color: Rgb::WHITE, speed: 0.5 };
        assert_eq!(b.sample(&ctx(0.0, 0.0, 0.0)).rgb.to_color(), Color::BLACK);
        assert_eq!(b.sample(&ctx(0.0, 0.0, 1.0)).rgb.to_color(), Color::WHITE);
    }

    #[test]
    fn starlight_density_bounds() {
        let never = Effect::Starlight { color: Rgb::WHITE, density: 0.0, speed: 1.0 };
        let always = Effect::Starlight { color: Rgb::WHITE, density: 1.0, speed: 1.0 };
        for seed in 0..50u32 {
            let c = SampleCtx { u: 0.0, v: 0.0, t: 0.3, seed };
            assert_eq!(never.sample(&c).a, 0.0);
            assert!(always.sample(&c).a > 0.0);
        }
    }

    #[test]
    fn stop_palette_wraps_smoothly() {
        let p = Palette::Stops {
            stops: vec![Stop { pos: 0.25, color: Rgb::BLACK }, Stop { pos: 0.75, color: Rgb::WHITE }],
        };
        // Midway across the wrap gap (0.75 → 1.25) is grey.
        assert_eq!(p.sample(0.0).to_color(), Color::new(128, 128, 128));
        assert_eq!(p.sample(0.25).to_color(), Color::BLACK);
        assert_eq!(p.sample(0.75).to_color(), Color::WHITE);
    }

    #[test]
    fn hash_is_uniform_enough() {
        let mean = (0..10_000u32).map(hash01).sum::<f32>() / 10_000.0;
        assert!((mean - 0.5).abs() < 0.02, "mean {mean}");
    }
}
