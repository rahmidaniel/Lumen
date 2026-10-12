//! Finds an OpenRGB to talk to and keeps a headless SDK server running, with
//! its lifetime tied to Lumen's wherever that is possible.
//!
//! OpenRGB is the only thing that talks to the hardware; Lumen just needs its
//! TCP server up. Preference order:
//! 1. a server that is already listening — attach, never start a second copy;
//! 2. on Windows, once the user has handed OpenRGB over to Lumen: Lumen's own
//!    Windows service, which runs a protected copy of OpenRGB (see [`win`]);
//! 3. the copy bundled with Lumen by `scripts/fetch-openrgb.mjs`:
//!    Windows `openrgb/OpenRGB.exe`, Linux `openrgb/OpenRGB.AppImage`,
//!    macOS `openrgb/OpenRGB.zip` (unpacked once into Lumen's data directory);
//! 4. a system-wide install found on disk.
//!
//! Whatever Lumen started (its service or a child process) it stops again on
//! exit and restarts when the server stops answering or goes away; a server it
//! merely attached to (the OpenRGB Windows service, someone's own OpenRGB) can
//! only be restarted with an elevated prompt.

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
    /// Lumen started the running server (its service or a child process) itself.
    pub started_by_lumen: bool,
    /// `managed` (Lumen's own service, or Lumen 0.2's task), `child` (a
    /// process Lumen spawned), `service` (the OpenRGB Windows service),
    /// `external` (someone else's server) or `none`.
    pub mode: &'static str,
    pub service: Option<ServiceInfo>,
    /// Lumen could take OpenRGB over on this machine (Windows, with an executable to run).
    pub can_manage: bool,
    /// Lumen's service runs a different OpenRGB than the one shipped with this
    /// install (an older Lumen's, or a development build's): update it.
    pub stale: bool,
    /// Still managed through Lumen 0.2's scheduled task, which other programs
    /// can abuse to get administrator rights: switch to the service.
    pub legacy_task: bool,
}

/// How Lumen controls OpenRGB on this machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Managed {
    /// Not at all: attach to whatever runs, else start a child process.
    No,
    /// Lumen's own Windows service, installed in one elevated step.
    Service,
    /// Lumen 0.2's scheduled task. Still works until the user switches over.
    LegacyTask,
}

fn detect_managed() -> Managed {
    if lumen_service().is_some() {
        Managed::Service
    } else if task_exists() {
        Managed::LegacyTask
    } else {
        Managed::No
    }
}

/// A path as a plain string, without Windows' `\\?\` verbatim prefix, so it
/// can be compared with what other tools report and handed to scripts.
pub fn plain_path(p: &Path) -> String {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        s.into_owned()
    }
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
    managed: Mutex<Managed>,
    /// Lumen started (or found running) the managed OpenRGB in this session.
    started: Mutex<bool>,
    /// Cached answer to "does the service run a different OpenRGB than ours?".
    stale: Mutex<Option<bool>>,
    /// A bundled or system OpenRGB left behind by an earlier Lumen (crash,
    /// force-kill) that we attached to and now own.
    orphan: Mutex<Option<u32>>,
    /// Start, restart, hand-over: one at a time, so the supervisor and a button
    /// press cannot both launch an OpenRGB.
    ops: tokio::sync::Mutex<()>,
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
        let managed = detect_managed();
        match managed {
            Managed::Service => tracing::info!("OpenRGB is managed by Lumen's service"),
            Managed::LegacyTask => tracing::warn!("OpenRGB is managed by Lumen 0.2's scheduled task; switch to the service in Settings"),
            Managed::No => {}
        }
        Self {
            candidates,
            active: Mutex::new(None),
            child: Mutex::new(None),
            managed: Mutex::new(managed),
            started: Mutex::new(false),
            stale: Mutex::new(None),
            orphan: Mutex::new(None),
            ops: tokio::sync::Mutex::new(()),
        }
    }

    /// When attaching to a server nobody else is responsible for, check
    /// whether it is one of the executables Lumen itself would run; if so,
    /// treat it as ours so it is stopped on exit and restarted when stuck.
    fn adopt_orphan(&self) {
        if self.managed() != Managed::No || self.child_alive() || self.orphan.lock().unwrap().is_some() {
            return;
        }
        let Some(pid) = listener_pid(DEFAULT_PORT) else { return };
        let Some(path) = process_path(pid) else { return };
        let ours = self.candidates.iter().any(|c| matches!(&c.source, Source::Exe(p) if plain_path(p).eq_ignore_ascii_case(&path)));
        if ours {
            tracing::info!(pid, %path, "attached to an OpenRGB left running by an earlier Lumen; taking it over");
            *self.orphan.lock().unwrap() = Some(pid);
        }
    }

    fn orphan_alive(&self) -> bool {
        match *self.orphan.lock().unwrap() {
            Some(pid) => listener_pid(DEFAULT_PORT) == Some(pid),
            None => false,
        }
    }

    pub fn status(&self) -> Status {
        let managed = self.managed();
        let active = self.active.lock().unwrap().clone();
        let (exe, bundled) = match (managed, active) {
            (Managed::Service, _) => (Some(plain_path(&protected_exe())), true),
            (_, Some((i, exe))) => (Some(exe.display().to_string()), self.candidates[i].bundled),
            (_, None) => match self.own_exe().or_else(|| self.candidates.first().map(|c| (c.source.display(), c.bundled))) {
                Some((exe, bundled)) => (Some(exe), bundled),
                None => (None, false),
            },
        };
        let server_reachable = server_reachable();
        let service = service_info();
        let child = self.child_alive() || self.orphan_alive();
        let mode = if managed != Managed::No {
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
            started_by_lumen: child || (managed != Managed::No && *self.started.lock().unwrap()),
            mode,
            service,
            can_manage: cfg!(windows) && self.own_exe().is_some(),
            stale: managed == Managed::Service && self.stale(),
            legacy_task: managed == Managed::LegacyTask,
        }
    }

    fn managed(&self) -> Managed {
        *self.managed.lock().unwrap()
    }

    /// The executable Lumen hands to its service: the bundled copy, else any plain executable.
    fn own_exe(&self) -> Option<(String, bool)> {
        let exe = |bundled: bool| {
            self.candidates.iter().find_map(|c| match &c.source {
                Source::Exe(p) if c.bundled == bundled => Some((plain_path(p), c.bundled)),
                _ => None,
            })
        };
        exe(true).or_else(|| exe(false))
    }

    /// Whether Lumen's service runs something other than this install's OpenRGB.
    fn stale(&self) -> bool {
        if let Some(known) = *self.stale.lock().unwrap() {
            return known;
        }
        let stale = match (lumen_service_binary(), self.own_exe()) {
            (Some(binary), Some((own, _))) => {
                let protected = protected_exe();
                !binary.eq_ignore_ascii_case(&plain_path(&protected)) || !same_file_contents(Path::new(&own), &protected)
            }
            _ => false,
        };
        *self.stale.lock().unwrap() = Some(stale);
        stale
    }

    fn child_alive(&self) -> bool {
        let mut child = self.child.lock().unwrap();
        matches!(child.as_mut().map(|c| c.try_wait()), Some(Ok(None)))
    }

    /// Whether Lumen can restart the server without an elevated prompt.
    pub fn can_restart_quietly(&self) -> bool {
        self.managed() != Managed::No || self.child_alive() || self.orphan_alive()
    }

    /// Whether keeping a server running is Lumen's job: it manages OpenRGB, or
    /// it started (or took over) the one that was running.
    pub fn responsible(&self) -> bool {
        self.managed() != Managed::No || self.child.lock().unwrap().is_some() || self.orphan.lock().unwrap().is_some()
    }

    /// Start a headless OpenRGB server unless one is already listening, then
    /// wait (up to ~20 s per attempt — detection takes a few seconds) for the
    /// port to open. Falls through to the next option when one fails.
    pub async fn ensure_running(&self) -> Result<Status, String> {
        let _op = self.ops.lock().await;
        self.ensure_running_locked().await
    }

    async fn ensure_running_locked(&self) -> Result<Status, String> {
        if server_reachable() {
            if self.managed() == Managed::Service && lumen_service() == Some(SvcState::Running) {
                // Lumen's own service, still up from an earlier session: ours to stop.
                *self.started.lock().unwrap() = true;
            }
            self.adopt_orphan();
            return Ok(self.status());
        }
        if self.child_alive() {
            return match self.wait_for_server(Duration::from_secs(20), true).await {
                true => Ok(self.status()),
                false => Err("OpenRGB is running but its SDK server did not come up".into()),
            };
        }
        // A child that died, or an orphan that went away, is history.
        self.child.lock().unwrap().take();
        self.orphan.lock().unwrap().take();

        let mut last_err = String::new();
        match self.managed() {
            Managed::Service => match lumen_service_start() {
                Ok(()) => {
                    *self.started.lock().unwrap() = true;
                    tracing::info!("started Lumen's OpenRGB service");
                    if self.wait_for_server(Duration::from_secs(20), false).await {
                        return Ok(self.status());
                    }
                    last_err = "Lumen's OpenRGB service started but its SDK server did not come up".into();
                    tracing::warn!("{last_err}");
                    let _ = lumen_service_stop();
                }
                Err(e) => {
                    tracing::warn!(%e, "Lumen's OpenRGB service could not be started; falling back to a child process");
                    last_err = e;
                }
            },
            Managed::LegacyTask => match task_run() {
                Ok(()) => {
                    *self.started.lock().unwrap() = true;
                    tracing::info!("started OpenRGB through Lumen 0.2's task");
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
            },
            Managed::No => {}
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

    /// Wait for Lumen's service to report STOPPED.
    async fn wait_service_stopped(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            if !matches!(lumen_service(), Some(SvcState::Running | SvcState::StopPending | SvcState::StartPending)) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    /// Restart the server: quietly when Lumen runs it (service, task or child),
    /// through an elevated prompt when it is OpenRGB's own Windows service, not
    /// at all otherwise. A managed OpenRGB that ignores the stop request needs
    /// an admin prompt too; `allow_prompt` false (the supervisor) gives up then.
    pub async fn restart(&self, allow_prompt: bool) -> Result<Status, String> {
        let _op = self.ops.lock().await;
        // Taken out of the mutex up front: a guard must not live across the awaits below.
        let orphan = if self.managed() != Managed::No || self.child_alive() { None } else { self.orphan.lock().unwrap().take() };
        match self.managed() {
            Managed::Service => {
                tracing::info!("restarting OpenRGB (Lumen's service)");
                let _ = lumen_service_stop();
                if !self.wait_service_stopped(Duration::from_secs(10)).await {
                    if !allow_prompt {
                        return Err("OpenRGB is stuck and ignores stop requests. Restart it from Settings → OpenRGB (one admin prompt).".into());
                    }
                    tracing::warn!("OpenRGB ignores the stop request; ending it through an admin prompt");
                    tokio::task::spawn_blocking(force_stop_lumen_service).await.map_err(|e| e.to_string())??;
                }
            }
            Managed::LegacyTask => {
                tracing::info!("restarting OpenRGB (Lumen 0.2's task)");
                let _ = task_end();
            }
            Managed::No if self.child_alive() => {
                tracing::info!("restarting OpenRGB (child process)");
                self.stop();
            }
            Managed::No if orphan.is_some() => {
                let pid = orphan.unwrap_or_default();
                tracing::info!(pid, "restarting OpenRGB (process taken over from an earlier Lumen)");
                if let Err(e) = kill_pid(pid) {
                    tracing::warn!(%e, "could not end the OpenRGB process");
                }
            }
            Managed::No if service_info().is_some() => {
                if !allow_prompt {
                    return Err("The OpenRGB service is not responding; restart it from Settings → OpenRGB (admin prompt).".into());
                }
                tracing::info!("restarting the OpenRGB Windows service");
                tokio::task::spawn_blocking(restart_service).await.map_err(|e| e.to_string())??;
                if self.wait_for_server(Duration::from_secs(20), false).await {
                    return Ok(self.status());
                }
                return Err("The OpenRGB service was restarted but its SDK server did not come up.".into());
            }
            Managed::No => return Err("OpenRGB was not started by Lumen; restart it yourself.".into()),
        }

        // Let the port close before starting the next instance.
        let deadline = Instant::now() + Duration::from_secs(5);
        while server_reachable() && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        if server_reachable() {
            return Err("Another OpenRGB is still holding port 6742.".into());
        }
        self.ensure_running_locked().await
    }

    /// Windows: hand OpenRGB over to Lumen (one elevated prompt), then start
    /// it. Also how an existing setup is updated or moved off Lumen 0.2's task.
    pub async fn adopt(&self) -> Result<Status, String> {
        let _op = self.ops.lock().await;
        if !cfg!(windows) {
            return Err("Only needed on Windows: elsewhere OpenRGB already runs as a process Lumen owns.".into());
        }
        let (exe, _) = self.own_exe().ok_or_else(|| "No OpenRGB executable to run.".to_string())?;
        let source = Path::new(&exe).parent().map(Path::to_path_buf).ok_or_else(|| format!("{exe} has no folder"))?;
        // Two instances would fight over the devices; the script ends what it replaces.
        self.stop();
        tokio::task::spawn_blocking(move || adopt(&source)).await.map_err(|e| e.to_string())??;
        *self.managed.lock().unwrap() = detect_managed();
        *self.stale.lock().unwrap() = None;
        self.ensure_running_locked().await
    }

    /// Windows: remove Lumen's service and give OpenRGB back to its own
    /// service, or to a plain child process (one elevated prompt).
    pub async fn release(&self) -> Result<Status, String> {
        let _op = self.ops.lock().await;
        if self.managed() == Managed::No {
            return Err("OpenRGB is not managed by Lumen.".into());
        }
        self.stop();
        tokio::task::spawn_blocking(release).await.map_err(|e| e.to_string())??;
        *self.managed.lock().unwrap() = detect_managed();
        *self.stale.lock().unwrap() = None;
        self.ensure_running_locked().await
    }

    /// Stop whatever Lumen started: its service, the task's OpenRGB or the
    /// child process. A server Lumen merely attached to is left alone.
    pub fn stop(&self) {
        if std::mem::take(&mut *self.started.lock().unwrap()) {
            match self.managed() {
                Managed::Service => {
                    tracing::info!("stopping Lumen's OpenRGB service");
                    if let Err(e) = lumen_service_stop() {
                        tracing::warn!(%e, "could not stop Lumen's OpenRGB service");
                    }
                }
                Managed::LegacyTask => {
                    tracing::info!("stopping OpenRGB (Lumen 0.2's task)");
                    if let Err(e) = task_end() {
                        tracing::warn!(%e, "could not end Lumen's OpenRGB task");
                    }
                }
                Managed::No => {}
            }
        }
        if let Some(mut child) = self.child.lock().unwrap().take() {
            tracing::info!("stopping the OpenRGB server Lumen started");
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(pid) = self.orphan.lock().unwrap().take() {
            tracing::info!(pid, "stopping the OpenRGB process taken over from an earlier Lumen");
            if let Err(e) = kill_pid(pid) {
                tracing::warn!(%e, "could not end the OpenRGB process");
            }
        }
    }
}

/// Compare two files byte for byte (the OpenRGB executable is ~15 MB).
fn same_file_contents(a: &Path, b: &Path) -> bool {
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(x), Ok(y)) if x.len() == y.len() => matches!((std::fs::read(a), std::fs::read(b)), (Ok(x), Ok(y)) if x == y),
        _ => false,
    }
}

// Platform shims so the manager reads the same on every OS.

#[cfg(windows)]
use win::{
    adopt, force_stop_lumen_service, kill_pid, listener_pid, lumen_service, lumen_service_binary, lumen_service_start, lumen_service_stop, process_path,
    release, restart_service, task_end, task_exists, task_run, SvcState,
};
#[cfg(windows)]
fn service_info() -> Option<ServiceInfo> {
    win::service()
}
#[cfg(windows)]
fn protected_exe() -> PathBuf {
    win::protected_dir().join("OpenRGB.exe")
}

#[cfg(not(windows))]
mod shims {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    #[allow(dead_code)] // never constructed: there is no service here
    pub enum SvcState {
        Stopped,
        StartPending,
        StopPending,
        Running,
        Other,
    }
    pub fn lumen_service() -> Option<SvcState> {
        None
    }
    pub fn lumen_service_binary() -> Option<String> {
        None
    }
    pub fn lumen_service_start() -> Result<(), String> {
        Err("not on this platform".into())
    }
    pub fn lumen_service_stop() -> Result<(), String> {
        Ok(())
    }
    pub fn force_stop_lumen_service() -> Result<(), String> {
        Err("not on this platform".into())
    }
    pub fn task_exists() -> bool {
        false
    }
    pub fn task_run() -> Result<(), String> {
        Err("not on this platform".into())
    }
    pub fn task_end() -> Result<(), String> {
        Ok(())
    }
    pub fn adopt(_source: &Path) -> Result<(), String> {
        Err("not on this platform".into())
    }
    pub fn release() -> Result<(), String> {
        Err("not on this platform".into())
    }
    pub fn restart_service() -> Result<(), String> {
        Err("not on this platform".into())
    }
    pub fn service_info() -> Option<ServiceInfo> {
        None
    }
    pub fn listener_pid(_port: u16) -> Option<u32> {
        None
    }
    pub fn process_path(_pid: u32) -> Option<String> {
        None
    }
    pub fn kill_pid(_pid: u32) -> Result<(), String> {
        Err("not on this platform".into())
    }
    pub fn protected_exe() -> PathBuf {
        PathBuf::new()
    }
}
#[cfg(not(windows))]
use shims::*;

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
