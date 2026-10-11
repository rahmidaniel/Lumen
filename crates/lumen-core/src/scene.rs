//! A scene is what a profile stores: the layout plus an ordered stack of layers.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::canvas::{LedPoint, Layout};
use crate::effects::{Effect, Palette, Rgb, Rgba};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlendMode {
    #[default]
    Normal,
    Add,
    Screen,
    Multiply,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ZoneRef {
    pub device: u32,
    pub zone: usize,
}

/// Which LEDs a layer paints.
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Mask {
    #[default]
    All,
    Devices { ids: Vec<u32> },
    Zones { zones: Vec<ZoneRef> },
}

impl Mask {
    pub fn includes(&self, p: &LedPoint) -> bool {
        match self {
            Mask::All => true,
            Mask::Devices { ids } => ids.contains(&p.device),
            Mask::Zones { zones } => zones.iter().any(|z| z.device == p.device && z.zone == p.zone),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub id: Uuid,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub effect: Effect,
    #[serde(default = "default_one")]
    pub opacity: f32,
    #[serde(default)]
    pub blend: BlendMode,
    #[serde(default)]
    pub mask: Mask,
}

impl Layer {
    pub fn new(name: impl Into<String>, effect: Effect) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            enabled: true,
            effect,
            opacity: 1.0,
            blend: BlendMode::Normal,
            mask: Mask::All,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    #[serde(default)]
    pub layout: Layout,
    #[serde(default)]
    pub layers: Vec<Layer>,
    /// Master brightness, 0..1.
    #[serde(default = "default_one")]
    pub brightness: f32,
}

impl Default for Scene {
    /// A rainbow wave sweeping left to right across everything — the first-run look.
    fn default() -> Self {
        Self {
            layout: Layout::default(),
            layers: vec![Layer::new(
                "Rainbow wave",
                Effect::Wave { angle: 0.0, speed: 0.25, wavelength: 1.0, palette: Palette::Rainbow },
            )],
            brightness: 1.0,
        }
    }
}

/// Composite `src` over `dst` with the layer's opacity and blend mode.
pub fn blend(dst: Rgb, src: Rgba, opacity: f32, mode: BlendMode) -> Rgb {
    let a = (src.a * opacity).clamp(0.0, 1.0);
    if a <= 0.0 {
        return dst;
    }
    let s = src.rgb;
    let mixed = match mode {
        BlendMode::Normal => s,
        BlendMode::Add => Rgb::new(dst.r + s.r, dst.g + s.g, dst.b + s.b),
        BlendMode::Screen => Rgb::new(
            1.0 - (1.0 - dst.r) * (1.0 - s.r),
            1.0 - (1.0 - dst.g) * (1.0 - s.g),
            1.0 - (1.0 - dst.b) * (1.0 - s.b),
        ),
        BlendMode::Multiply => Rgb::new(dst.r * s.r, dst.g * s.g, dst.b * s.b),
    };
    Rgb::lerp(dst, mixed, a)
}

fn default_true() -> bool {
    true
}

fn default_one() -> f32 {
    1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blend_modes() {
        let grey = Rgb::new(0.5, 0.5, 0.5);
        let src = Rgba::opaque(Rgb::new(0.5, 0.5, 0.5));
        assert_eq!(blend(grey, src, 1.0, BlendMode::Normal), grey);
        assert_eq!(blend(grey, src, 1.0, BlendMode::Add).r, 1.0);
        assert_eq!(blend(grey, src, 1.0, BlendMode::Screen).r, 0.75);
        assert_eq!(blend(grey, src, 1.0, BlendMode::Multiply).r, 0.25);
        assert_eq!(blend(grey, Rgba::opaque(Rgb::WHITE), 0.5, BlendMode::Normal).r, 0.75);
        assert_eq!(blend(grey, Rgba::TRANSPARENT, 1.0, BlendMode::Normal), grey);
    }

    #[test]
    fn scene_json_roundtrip_with_defaults() {
        let scene = Scene::default();
        let json = serde_json::to_string(&scene).unwrap();
        let back: Scene = serde_json::from_str(&json).unwrap();
        assert_eq!(back, scene);

        // Older/partial files still load.
        let minimal: Scene = serde_json::from_str(r#"{"layers":[{"id":"8c4c0e6a-0a8a-4b9e-9a9f-0d1e2f3a4b5c","name":"x","effect":{"type":"static","color":{"r":1,"g":0,"b":0}}}]}"#).unwrap();
        assert!(minimal.layers[0].enabled);
        assert_eq!(minimal.layers[0].opacity, 1.0);
        assert_eq!(minimal.brightness, 1.0);
    }

    #[test]
    fn mask_matching() {
        let p = LedPoint { device_index: 0, device: 7, zone: 1, led: 3, x: 0.0, y: 0.0, u: 0.0, v: 0.0 };
        assert!(Mask::All.includes(&p));
        assert!(Mask::Devices { ids: vec![7] }.includes(&p));
        assert!(!Mask::Devices { ids: vec![8] }.includes(&p));
        assert!(Mask::Zones { zones: vec![ZoneRef { device: 7, zone: 1 }] }.includes(&p));
        assert!(!Mask::Zones { zones: vec![ZoneRef { device: 7, zone: 0 }] }.includes(&p));
    }
}
