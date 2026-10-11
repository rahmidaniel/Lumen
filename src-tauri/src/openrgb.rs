//! Finds an OpenRGB to talk to and keeps a headless SDK server running.
//!
//! OpenRGB is the only thing that talks to the hardware; Lumen just needs its
//! TCP server up. Preference order:
//! 1. a server that is already listening (the user's own OpenRGB, or the
//!    Windows service OpenRGB 1.0 installs) — attach, never start a second copy;
//! 2. the copy bundled with Lumen (`<resources>/openrgb/OpenRGB.exe`);
//! 3. a system-wide install found on disk.
//! A copy Lumen started is stopped again when Lumen exits.

use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use openrgb_sdk::DEFAULT_PORT;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Status {
    pub exe: Option<String>,
    /// The executable is the copy shipped inside Lumen's install.
    pub bundled: bool,
    pub server_reachable: bool,
    pub started_by_lumen: bool,
}

pub struct Manager {
    exe: Option<PathBuf>,
    bundled: bool,
    child: Mutex<Option<Child>>,
}

impl Manager {
    /// `resource_dir` is Tauri's resource directory; the installer places the
    /// bundled OpenRGB under `openrgb/` inside it.
    pub fn new(resource_dir: Option<PathBuf>) -> Self {
        let bundled = resource_dir.map(|d| d.join("openrgb").join(exe_name())).filter(|p| p.is_file());
        let (exe, is_bundled) = match bundled {
            Some(p) => (Some(p), true),
            None => (locate(), false),
        };
        match &exe {
            Some(p) => tracing::info!(path = %p.display(), bundled = is_bundled, "found OpenRGB"),
            None => tracing::warn!("OpenRGB executable not found"),
        }
        Self { exe, bundled: is_bundled, child: Mutex::new(None) }
    }

    pub fn status(&self) -> Status {
        Status {
            exe: self.exe.as_ref().map(|p| p.display().to_string()),
            bundled: self.bundled,
            server_reachable: server_reachable(),
            started_by_lumen: self.child_alive(),
        }
    }

    fn child_alive(&self) -> bool {
        let mut child = self.child.lock().unwrap();
        matches!(child.as_mut().map(|c| c.try_wait()), Some(Ok(None)))
    }

    /// Start a headless OpenRGB server unless one is already listening, then
    /// wait (up to ~20 s — detection takes a few seconds) for the port to open.
    pub async fn ensure_running(&self) -> Result<Status, String> {
        if server_reachable() {
            return Ok(self.status());
        }
        let exe = self.exe.clone().ok_or_else(|| "OpenRGB is not installed (or not found)".to_string())?;

        if !self.child_alive() {
            tracing::info!(path = %exe.display(), "starting headless OpenRGB SDK server");
            // Without --gui/--startminimized OpenRGB runs as a silent server.
            let spawned = Command::new(&exe)
                .args(["--server", "--server-host", "127.0.0.1", "--noautoconnect"])
                .current_dir(exe.parent().unwrap_or(std::path::Path::new(".")))
                .spawn()
                .map_err(|e| format!("failed to start OpenRGB: {e}"))?;
            *self.child.lock().unwrap() = Some(spawned);
        }

        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            if server_reachable() {
                return Ok(self.status());
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        Err("OpenRGB started but its SDK server did not come up".into())
    }

    /// Stop the OpenRGB copy Lumen started (no-op if we attached to someone else's).
    pub fn stop(&self) {
        if let Some(mut child) = self.child.lock().unwrap().take() {
            tracing::info!("stopping the OpenRGB server Lumen started");
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub fn server_reachable() -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], DEFAULT_PORT));
    TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok()
}

fn exe_name() -> &'static str {
    if cfg!(windows) { "OpenRGB.exe" } else { "openrgb" }
}

fn locate() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("LUMEN_OPENRGB_PATH").map(PathBuf::from).filter(|p| p.is_file()) {
        return Some(p);
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    #[cfg(target_os = "windows")]
    {
        for root in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
            if let Some(base) = std::env::var_os(root) {
                let base = PathBuf::from(base);
                candidates.push(base.join("OpenRGB").join("OpenRGB.exe"));
                candidates.push(base.join("Programs").join("OpenRGB").join("OpenRGB.exe"));
            }
        }
    }
    #[cfg(target_os = "macos")]
    candidates.push(PathBuf::from("/Applications/OpenRGB.app/Contents/MacOS/OpenRGB"));
    #[cfg(target_os = "linux")]
    candidates.extend(["/usr/bin/openrgb", "/usr/local/bin/openrgb", "/var/lib/flatpak/exports/bin/org.openrgb.OpenRGB"].map(PathBuf::from));

    if let Some(path) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&path).map(|d| d.join(exe_name())));
    }

    candidates.into_iter().find(|p| p.is_file())
}
