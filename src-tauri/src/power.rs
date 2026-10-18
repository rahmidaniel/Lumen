//! What the PC is doing — going to sleep, waking up, locking, turning the
//! screen off, sitting idle — so the lights can follow it.
//!
//! Windows reports these to a hidden window ([`win`]). Other platforms report
//! nothing yet, and the lights simply stay on there.

#[cfg(windows)]
mod win;

use std::sync::atomic::{AtomicBool, Ordering};

/// Something the operating system told us.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerEvent {
    /// The PC is about to sleep or hibernate.
    Suspend,
    /// Back from sleep or hibernation.
    Resume,
    /// The session is ending: shutdown, restart or sign-out.
    Shutdown,
    /// The session goes on after all (a program refused to close, or the
    /// user cancelled the shutdown).
    ShutdownCancelled,
    /// The display turned on (`true`) or off (`false`).
    Display(bool),
    /// The session was locked (`true`) or unlocked (`false`).
    Lock(bool),
}

static SESSION_ENDING: AtomicBool = AtomicBool::new(false);

/// The session is ending (shutdown, restart, sign-out) and Lumen is about to
/// be closed. Windows no longer starts programs then, and OpenRGB going away
/// is expected rather than something to repair.
pub fn session_ending() -> bool {
    SESSION_ENDING.load(Ordering::SeqCst)
}

impl PowerEvent {
    /// The OS gives us only a moment for these, so whoever handles them should
    /// make sure the lights are out before returning.
    pub fn is_urgent(self) -> bool {
        matches!(self, PowerEvent::Suspend | PowerEvent::Shutdown)
    }
}

/// Whether this platform reports power and session events.
pub const SUPPORTED: bool = cfg!(windows);

/// Start listening. `on_event` runs on a dedicated thread; it may block for a
/// moment on urgent events.
pub fn watch(on_event: impl Fn(PowerEvent) + Send + Sync + 'static) {
    #[cfg(windows)]
    win::watch(Box::new(on_event));
    #[cfg(not(windows))]
    let _ = on_event;
}

/// Seconds since the last keyboard or mouse input, where the platform can tell.
pub fn idle_seconds() -> Option<u64> {
    #[cfg(windows)]
    {
        win::idle_seconds()
    }
    #[cfg(not(windows))]
    {
        None
    }
}
