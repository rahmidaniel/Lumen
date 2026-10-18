//! Async client for the OpenRGB SDK network protocol.
//!
//! OpenRGB exposes every detected RGB controller over a small TCP protocol
//! (default port 6742). This crate speaks protocol version 6 (OpenRGB 1.0)
//! and negotiates down for older servers.
//!
//! ```no_run
//! use openrgb_sdk::{Client, Color};
//!
//! # async fn demo() -> openrgb_sdk::Result<()> {
//! let (client, _events) = Client::connect_local("my app").await?;
//! for device in client.controllers().await? {
//!     client.set_custom_mode(device.id).await?;
//!     let frame = vec![Color::new(255, 0, 64); device.leds.len()];
//!     client.update_leds(device.id, &frame).await?;
//! }
//! # Ok(())
//! # }
//! ```

pub mod client;
pub mod error;
pub mod model;
pub mod parse;
pub mod protocol;
pub mod wire;

pub use client::{Client, Event, Receipt, DEFAULT_TIMEOUT};
pub use error::{Error, Result};
pub use model::*;
pub use protocol::{DEFAULT_PORT, PROTOCOL_VERSION};
