//! Lumen's engine.
//!
//! Devices reported by OpenRGB are placed on a virtual desk ([`canvas`]); a
//! stack of effect [`scene::Layer`]s is sampled at every LED's position by the
//! [`render::Renderer`]; the [`engine`] runs that at a fixed rate and streams the
//! resulting frames to OpenRGB, which is the only thing that ever writes to
//! hardware.

pub mod canvas;
pub mod effects;
pub mod engine;
pub mod profile;
pub mod render;
pub mod scene;

pub use canvas::{Body, DeviceGeometry, Glyph, LedGlyph, LedPoint, Layout, Placement};
pub use effects::{Effect, Palette, Rgb, Rgba, Stop};
pub use engine::{Battery, DeviceExtras, DeviceInfo, EngineConfig, EngineHandle, FrameSet, Onboard, Snapshot, ZoneInfo};
pub use profile::{Profile, ProfileStore};
pub use render::Renderer;
pub use scene::{BlendMode, Layer, Mask, Scene, ZoneRef};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("invalid profile JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    OpenRgb(#[from] openrgb_sdk::Error),
    #[error("profile not found: {0}")]
    ProfileNotFound(String),
    #[error("not connected to OpenRGB")]
    NotConnected,
}

pub type Result<T> = std::result::Result<T, Error>;
