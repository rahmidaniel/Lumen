//! Finds an OpenRGB to talk to and keeps a headless SDK server running, with
//! its lifetime tied to Lumen's wherever that is possible.
//!
//! OpenRGB is the only thing that talks to the hardware; Lumen just needs its
//! TCP server up. Preference order:
//! 1. a server that is already listening — attach, never start a second copy;
//! 2. on Windows, once the user has handed OpenRGB over to Lumen: the
//!    Task Scheduler task that runs OpenRGB elevated (see [`win`]);
//! 3. the copy bundled with Lumen by `scripts/fetch-openrgb.mjs`:
//!    Windows `openrgb/OpenRGB.exe`, Linux `openrgb/OpenRGB.AppImage`,
//!    macOS `openrgb/OpenRGB.zip` (unpacked once into Lumen's data directory);
//! 4. a system-wide install found on disk.
//!
//! Whatever Lumen started (task or child process) it stops again on exit and
//! restarts when the server stops answering; a server it merely attached to
//! (the OpenRGB Windows service, someone's own OpenRGB) can only be restarted
//! with an elevated prompt.

#[cfg(windows)]
mod win;

use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use openrgb_sdk::DEFAULT_PORT;
use serde::Serialize;

/// The OpenRGB Windows service, when installed.
#[derive(Clone, Debug, Serialize)]
pub struct ServiceInfo {
    pub running: bool,
    pub disabled: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Status {
    pub exe: Option<String>,
    /// The executable is the copy shipped inside Lumen's install.
    pub bundled: bool,
    pub server_reachable: bool,
    /// Lumen started the running server (task or child process) itself.
    pub started_by_lumen: bool,
    /// `managed` (Lumen's elevated task), `child` (a process Lumen spawned),
    /// `service` (the OpenRGB Windows service), `external` (someone else's
    /// server) or `none`.
    pub mode: &'static str,
    pub service: Option<ServiceInfo>,
    /// Lumen could take OpenRGB over on this machine (Windows, with an executable to run).
    pub can_manage: bool,
}

/// Where an OpenRGB executable comes from.
#[derive(Clone, Debug)]
#[allow(dead_code)] // each platform constructs only its own variants
enum Source {
    /// A ready-to-run executable.
    Exe(PathBuf),
    /// Linux: the bundled AppImage. Run in place when it is executable, else
    /// copied into `dest` and marked executable first.
    AppImage { file: PathBuf, dest: PathBuf },
    /// macOS: the official zip shipped in Lumen's resources, unpacked into
    /// `dest` on first use (the .app holds framework symlinks that would not
    /// survive the bundler's resource copy).
    Archive { zip: PathBuf, dest: PathBuf },
}

impl Source {
    fn display(&self) -> String {
        match self {
            Source::Exe(p) | Source::AppImage { file: p, .. } | Source::Archive { zip: p, .. } => p.display().to_string(),
        }
    }

    /// Produce an executable path, doing any one-off preparation (blocking).
    fn resolve(&self) -> Result<PathBuf, String> {
        match self {
            Source::Exe(p) => Ok(p.clone()),
            Source::AppImage { file, dest } => prepare_appimage(file, dest),
            Source::Archive { zip, dest } => unpack_app(zip, dest),
        }
    }
}

#[derive(Clone, Debug)]
struct Candidate {
    source: Source,
    bundled: bool,
}

pub struct Manager {
    /// In preference order: `LUMEN_OPENRGB_PATH`, the bundled copy, system installs.
    candidates: Vec<Candidate>,
    /// Candidate index and resolved executable of the copy Lumen launched last.
    active: Mutex<Option<(usize, PathBuf)>>,
    child: Mutex<Option<Child>>,
    /// Windows: Lumen's elevated task exists, so OpenRGB is ours to run.
    managed: Mutex<bool>,
    /// Lumen started the task's OpenRGB in this session.
    task_started: Mutex<bool>,
}

impl Manager {
    /// `resource_dir` is Tauri's resource directory (the installer places the
    /// bundled OpenRGB under `openrgb/` inside it); `data_dir` is Lumen's
    /// writable data directory, used where the bundled copy must be unpacked.
    pub fn new(resource_dir: Option<PathBuf>, data_dir: PathBuf) -> Self {
        let mut candidates = Vec::new();
        if let Some(p) = std::env::var_os("LUMEN_OPENRGB_PATH").map(PathBuf::from).filter(|p| p.is_file()) {
            candidates.push(Candidate { source: Source::Exe(p), bundled: false });
        }
        if let Some(source) = resource_dir.and_then(|dir| bundled(&dir, &data_dir)) {
            candidates.push(Candidate { source, bundled: true });
        }
        candidates.extend(locate().into_iter().map(|p| Candidate { source: Source::Exe(p), bundled: false }));

        match candidates.first() {
            Some(c) => tracing::info!(source = %c.source.display(), bundled = c.bundled, others = candidates.len() - 1, "found OpenRGB"),
            None => tracing::warn!("OpenRGB executable not found"),
        }
        let managed = task_exists();
        if managed {
            tracing::info!("OpenRGB is managed by Lumen's elevated task");
        }
        Self {
            candidates,
            active: Mutex::new(None),
            child: Mutex::new(None),
            managed: Mutex::new(managed),
            task_started: Mutex::new(false),
        }
    }

    pub fn status(&self) -> Status {
        let active = self.active.lock().unwrap().clone();
        let (exe, bundled) = match active {
            Some((i, exe)) => (Some(exe.display().to_string()), self.candidates[i].bundled),
            None => match self.task_exe().or_else(|| self.candidates.first().map(|c| (c.source.display(), c.bundled))) {
                Some((exe, bundled)) => (Some(exe), bundled),
                None => (None, false),
            },
        };
        let server_reachable = server_reachable();
        let service = service_info();
        let managed = self.managed();
        let child = self.child_alive();
        let mode = if managed {
            "managed"
        } else if child {
            "child"
        } else if service.as_ref().is_some_and(|s| s.running) {
            "service"
        } else if server_reachable {
            "external"
        } else {
            "none"
        };
        Status {
            exe,
            bundled,
            server_reachable,
            started_by_lumen: child || (managed && *self.task_started.lock().unwrap()),
            mode,
            service,
            can_manage: cfg!(windows) && self.task_exe().is_some(),
        }
    }

    fn managed(&self) -> bool {
        *self.managed.lock().unwrap()
    }

    /// The executable Lumen's task runs (or would run): the bundled copy, else any plain executable.
    fn task_exe(&self) -> Option<(String, bool)> {
        let exe = |bundled: bool| {
            self.candidates
                .iter()
                .find(|c| c.bundled == bundled && matches!(c.source, Source::Exe(_)))
                .map(|c| (c.source.display(), c.bundled))
        };
        exe(true).or_else(|| exe(false))
    }

    fn child_alive(&self) -> bool {
        let mut child = self.child.lock().unwrap();
        matches!(child.as_mut().map(|c| c.try_wait()), Some(Ok(None)))
    }

    /// Whether Lumen can restart the server without an elevated prompt.
    pub fn can_restart_quietly(&self) -> bool {
        self.managed() || self.child_alive()
    }

    /// Start a headless OpenRGB server unless one is already listening, then
    /// wait (up to ~20 s per attempt — detection takes a few seconds) for the
    /// port to open. Falls through to the next option when one fails.
    pub async fn ensure_running(&self) -> Result<Status, String> {
        if server_reachable() {
            return Ok(self.status());
        }
        if self.child_alive() {
            return match self.wait_for_server(Duration::from_secs(20), true).await {
                true => Ok(self.status()),
                false => Err("OpenRGB is running but its SDK server did not come up".into()),
            };
        }

        let mut last_err = String::new();
        if self.managed() {
            match task_run() {
                Ok(()) => {
                    *self.task_started.lock().unwrap() = true;
                    tracing::info!("started OpenRGB through Lumen's task");
                    if self.wait_for_server(Duration::from_secs(20), false).await {
                        return Ok(self.status());
                    }
                    last_err = "OpenRGB started through Lumen's task but its SDK server did not come up".into();
                    tracing::warn!("{last_err}");
                    let _ = task_end();
                }
                Err(e) => {
                    tracing::warn!(%e, "Lumen's OpenRGB task could not be started; falling back to a child process");
                    last_err = e;
                }
            }
        }
        if self.candidates.is_empty() {
            return Err(if last_err.is_empty() { "OpenRGB is not installed (or not found)".into() } else { last_err });
        }

        for (i, candidate) in self.candidates.iter().enumerate() {
            let source = candidate.source.clone();
            let exe = match tokio::task::spawn_blocking(move || source.resolve()).await.map_err(|e| e.to_string())? {
                Ok(exe) => exe,
                Err(e) => {
                    tracing::warn!(source = %candidate.source.display(), %e, "OpenRGB candidate unusable");
                    last_err = e;
                    continue;
                }
            };
            tracing::info!(path = %exe.display(), bundled = candidate.bundled, "starting headless OpenRGB SDK server");
            match spawn_server(&exe) {
                Ok(child) => {
                    *self.child.lock().unwrap() = Some(child);
                    *self.active.lock().unwrap() = Some((i, exe.clone()));
                }
                Err(e) => {
                    last_err = format!("failed to start OpenRGB: {e}");
                    tracing::warn!(path = %exe.display(), %e, "failed to start OpenRGB");
                    continue;
                }
            }
            if self.wait_for_server(Duration::from_secs(20), true).await {
                return Ok(self.status());
            }
            last_err = "OpenRGB started but its SDK server did not come up".into();
            tracing::warn!(path = %exe.display(), "{last_err}");
            self.stop();
        }
        Err(last_err)
    }

    /// Poll for the SDK port; with `watch_child`, give up as soon as the child process dies.
    async fn wait_for_server(&self, timeout: Duration, watch_child: bool) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if server_reachable() {
                return true;
            }
            if watch_child && !self.child_alive() {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        false
    }

    /// Restart the server: quietly when Lumen runs it (task or child), through
    /// an elevated prompt when it is the Windows service, not at all otherwise.
    pub async fn restart(&self) -> Result<Status, String> {
        if self.managed() {
            tracing::info!("restarting OpenRGB (Lumen's task)");
            let _ = task_end();
        } else if self.child_alive() {
            tracing::info!("restarting OpenRGB (child process)");
            self.stop();
        } else if service_info().is_some() {
            tracing::info!("restarting the OpenRGB Windows service");
            tokio::task::spawn_blocking(restart_service).await.map_err(|e| e.to_string())??;
            if self.wait_for_server(Duration::from_secs(20), false).await {
                return Ok(self.status());
            }
            return Err("The OpenRGB service was restarted but its SDK server did not come up.".into());
        } else {
            return Err("OpenRGB was not started by Lumen; restart it yourself.".into());
        }

        // Let the port close before starting the next instance.
        let deadline = Instant::now() + Duration::from_secs(5);
        while server_reachable() && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        if server_reachable() {
            return Err("Another OpenRGB is still holding port 6742.".into());
        }
        self.ensure_running().await
    }

    /// Windows: hand OpenRGB over to Lumen (one elevated prompt), then start it.
    pub async fn adopt(&self) -> Result<Status, String> {
        let (exe, _) = self.task_exe().ok_or_else(|| "No OpenRGB executable to run.".to_string())?;
        if !cfg!(windows) {
            return Err("Only needed on Windows: elsewhere OpenRGB already runs as a process Lumen owns.".into());
        }
        // Two instances would fight over the devices.
        self.stop();
        let exe_path = PathBuf::from(exe);
        tokio::task::spawn_blocking(move || adopt(&exe_path)).await.map_err(|e| e.to_string())??;
        *self.managed.lock().unwrap() = task_exists();
        self.ensure_running().await
    }

    /// Windows: give OpenRGB back to its service (one elevated prompt).
    pub async fn release(&self) -> Result<Status, String> {
        if !self.managed() {
            return Err("OpenRGB is not managed by Lumen.".into());
        }
        let _ = task_end();
        tokio::task::spawn_blocking(release).await.map_err(|e| e.to_string())??;
        *self.managed.lock().unwrap() = task_exists();
        *self.task_started.lock().unwrap() = false;
        self.ensure_running().await
    }

    /// Stop whatever Lumen started: the task's OpenRGB or the child process.
    /// A server Lumen merely attached to is left alone.
    pub fn stop(&self) {
        if self.managed() && *self.task_started.lock().unwrap() {
            tracing::info!("stopping OpenRGB (Lumen's task)");
            if let Err(e) = task_end() {
                tracing::warn!(%e, "could not end Lumen's OpenRGB task");
            }
            *self.task_started.lock().unwrap() = false;
        }
        if let Some(mut child) = self.child.lock().unwrap().take() {
            tracing::info!("stopping the OpenRGB server Lumen started");
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

// Platform shims so the manager reads the same on every OS.

#[cfg(windows)]
use win::{adopt, release, restart_service, task_end, task_exists, task_run};
#[cfg(windows)]
fn service_info() -> Option<ServiceInfo> {
    win::service()
}

#[cfg(not(windows))]
fn task_exists() -> bool {
    false
}
#[cfg(not(windows))]
fn task_run() -> Result<(), String> {
    Err("not on this platform".into())
}
#[cfg(not(windows))]
fn task_end() -> Result<(), String> {
    Ok(())
}
#[cfg(not(windows))]
fn adopt(_exe: &Path) -> Result<(), String> {
    Err("not on this platform".into())
}
#[cfg(not(windows))]
fn release() -> Result<(), String> {
    Err("not on this platform".into())
}
#[cfg(not(windows))]
fn restart_service() -> Result<(), String> {
    Err("not on this platform".into())
}
#[cfg(not(windows))]
fn service_info() -> Option<ServiceInfo> {
    None
}

pub fn server_reachable() -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], DEFAULT_PORT));
    TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok()
}

/// Without `--gui`/`--startminimized` OpenRGB runs as a silent server.
fn spawn_server(exe: &Path) -> std::io::Result<Child> {
    let mut cmd = Command::new(exe);
    cmd.args(["--server", "--server-host", "127.0.0.1", "--noautoconnect"]);
    cmd.current_dir(exe.parent().unwrap_or(Path::new(".")));
    #[cfg(target_os = "linux")]
    {
        // Run the AppImage even where FUSE is missing, and keep the library and
        // GTK module paths Lumen's own AppImage runtime exports out of its environment.
        cmd.env("APPIMAGE_EXTRACT_AND_RUN", "1");
        for var in [
            "LD_LIBRARY_PATH",
            "LD_PRELOAD",
            "APPIMAGE",
            "APPDIR",
            "ARGV0",
            "OWD",
            "GDK_PIXBUF_MODULE_FILE",
            "GSETTINGS_SCHEMA_DIR",
            "GTK_PATH",
            "GTK_EXE_PREFIX",
            "GTK_DATA_PREFIX",
            "GIO_MODULE_DIR",
        ] {
            cmd.env_remove(var);
        }
    }
    cmd.spawn()
}

/// The copy `scripts/fetch-openrgb.mjs` placed under `<resources>/openrgb/`, if any.
fn bundled(resource_dir: &Path, data_dir: &Path) -> Option<Source> {
    let dir = resource_dir.join("openrgb");
    let dest = data_dir.join("openrgb");
    let source = if cfg!(windows) {
        Source::Exe(dir.join("OpenRGB.exe"))
    } else if cfg!(target_os = "macos") {
        Source::Archive { zip: dir.join("OpenRGB.zip"), dest }
    } else {
        Source::AppImage { file: dir.join("OpenRGB.AppImage"), dest }
    };
    let present = match &source {
        Source::Exe(p) | Source::AppImage { file: p, .. } | Source::Archive { zip: p, .. } => p.is_file(),
    };
    present.then_some(source)
}

#[cfg(unix)]
fn prepare_appimage(file: &Path, dest: &Path) -> Result<PathBuf, String> {
    use std::os::unix::fs::PermissionsExt;
    let executable = |p: &Path| std::fs::metadata(p).map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false);
    if executable(file) {
        return Ok(file.to_path_buf());
    }
    // Packaging dropped the execute bit (or the resource lives on a noexec
    // mount): run a private copy instead.
    let copy = dest.join("OpenRGB.AppImage");
    let same_size = |a: &Path, b: &Path| match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(x), Ok(y)) => x.len() == y.len(),
        _ => false,
    };
    if !(copy.is_file() && same_size(file, &copy)) {
        std::fs::create_dir_all(dest).map_err(|e| format!("cannot create {}: {e}", dest.display()))?;
        std::fs::copy(file, &copy).map_err(|e| format!("cannot copy OpenRGB AppImage: {e}"))?;
    }
    std::fs::set_permissions(&copy, std::fs::Permissions::from_mode(0o755)).map_err(|e| format!("cannot mark OpenRGB executable: {e}"))?;
    Ok(copy)
}

#[cfg(not(unix))]
fn prepare_appimage(_file: &Path, _dest: &Path) -> Result<PathBuf, String> {
    Err("AppImages only run on Linux".into())
}

/// Unpack `OpenRGB.zip` into `dest/` with `ditto` (keeps the .app's symlinks
/// and permissions). Re-done only when the zip changes (e.g. a Lumen update).
fn unpack_app(zip: &Path, dest: &Path) -> Result<PathBuf, String> {
    let exe = dest.join("OpenRGB.app").join("Contents").join("MacOS").join("OpenRGB");
    let stamp_file = dest.join(".source");
    let stamp = match std::fs::metadata(zip) {
        Ok(m) => format!(
            "{}|{}|{:?}",
            zip.display(),
            m.len(),
            m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs())
        ),
        Err(e) => return Err(format!("cannot read {}: {e}", zip.display())),
    };
    if exe.is_file() && std::fs::read_to_string(&stamp_file).map(|s| s == stamp).unwrap_or(false) {
        return Ok(exe);
    }

    tracing::info!(zip = %zip.display(), dest = %dest.display(), "unpacking bundled OpenRGB");
    if dest.exists() {
        std::fs::remove_dir_all(dest).map_err(|e| format!("cannot clear {}: {e}", dest.display()))?;
    }
    std::fs::create_dir_all(dest).map_err(|e| format!("cannot create {}: {e}", dest.display()))?;
    let out = Command::new("ditto")
        .args(["-x", "-k"])
        .arg(zip)
        .arg(dest)
        .output()
        .map_err(|e| format!("cannot run ditto: {e}"))?;
    if !out.status.success() {
        return Err(format!("unpacking OpenRGB failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    if !exe.is_file() {
        return Err(format!("{} missing after unpacking", exe.display()));
    }
    let _ = std::fs::write(&stamp_file, stamp);
    Ok(exe)
}

fn exe_name() -> &'static str {
    if cfg!(windows) {
        "OpenRGB.exe"
    } else {
        "openrgb"
    }
}

/// System-wide installs, most likely first.
fn locate() -> Vec<PathBuf> {
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
    {
        candidates.push(PathBuf::from("/Applications/OpenRGB.app/Contents/MacOS/OpenRGB"));
        if let Some(home) = std::env::var_os("HOME") {
            candidates.push(PathBuf::from(home).join("Applications/OpenRGB.app/Contents/MacOS/OpenRGB"));
        }
    }
    #[cfg(target_os = "linux")]
    candidates.extend(["/usr/bin/openrgb", "/usr/local/bin/openrgb", "/var/lib/flatpak/exports/bin/org.openrgb.OpenRGB"].map(PathBuf::from));

    if let Some(path) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&path).map(|d| d.join(exe_name())));
    }

    let mut found: Vec<PathBuf> = Vec::new();
    for p in candidates.into_iter().filter(|p| p.is_file()) {
        if !found.contains(&p) {
            found.push(p);
        }
    }
    found
}
