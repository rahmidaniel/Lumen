//! Finds the OpenRGB install and keeps a headless SDK server running.
//!
//! OpenRGB is the only thing that talks to the hardware; Lumen just needs its
//! TCP server up. If the user already runs OpenRGB (or its service), we attach
//! to that instead of starting another copy.

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
    pub server_reachable: bool,
    pub started_by_lumen: bool,
}

pub struct Manager {
    exe: Option<PathBuf>,
    child: Mutex<Option<Child>>,
}

impl Manager {
    pub fn new() -> Self {
        let exe = locate();
        match &exe {
            Some(p) => tracing::info!(path = %p.display(), "found OpenRGB"),
            None => tracing::warn!("OpenRGB executable not found"),
        }
        Self { exe, child: Mutex::new(None) }
    }

    pub fn status(&self) -> Status {
        Status {
            exe: self.exe.as_ref().map(|p| p.display().to_string()),
            server_reachable: server_reachable(),
            started_by_lumen: self.child.lock().unwrap().is_some(),
        }
    }

    /// Start a headless OpenRGB server unless one is already listening, then
    /// wait (up to ~20 s — detection takes a few seconds) for the port to open.
    pub async fn ensure_running(&self) -> Result<Status, String> {
        if server_reachable() {
            return Ok(self.status());
        }
        let exe = self.exe.clone().ok_or_else(|| "OpenRGB is not installed (or not found)".to_string())?;

        {
            let mut child = self.child.lock().unwrap();
            let alive = matches!(child.as_mut().map(|c| c.try_wait()), Some(Ok(None)));
            if !alive {
                tracing::info!("starting OpenRGB SDK server");
                let spawned = Command::new(&exe)
                    .args(["--server", "--noautoconnect", "--startminimized"])
                    .spawn()
                    .map_err(|e| format!("failed to start OpenRGB: {e}"))?;
                *child = Some(spawned);
            }
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
}

pub fn server_reachable() -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], DEFAULT_PORT));
    TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok()
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
        let name = if cfg!(windows) { "OpenRGB.exe" } else { "openrgb" };
        candidates.extend(std::env::split_paths(&path).map(|d| d.join(name)));
    }

    candidates.into_iter().find(|p| p.is_file())
}
