//! Logging to a file, and the diagnostics report behind Settings → "Copy
//! diagnostics": what a bug report needs (versions, OpenRGB setup, devices,
//! recent logs) as Markdown that pastes straight into a GitHub issue.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter};

use crate::AppState;

const LOG_PREFIX: &str = "lumen";

fn filter() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,lumen=debug,lumen_lib=debug,lumen_core=debug,openrgb_sdk=info"))
}

/// Log to stdout (development) and to a daily file in `dir`, keeping a week.
/// Release builds on Windows have no console, so the file is the only record.
pub fn init_logging(dir: Option<PathBuf>) -> Option<PathBuf> {
    let file = dir.as_ref().and_then(|dir| {
        // The appender prunes old files before it creates the folder; make it first.
        std::fs::create_dir_all(dir).ok()?;
        tracing_appender::rolling::Builder::new()
            .rotation(tracing_appender::rolling::Rotation::DAILY)
            .filename_prefix(LOG_PREFIX)
            .filename_suffix("log")
            .max_log_files(7)
            .build(dir)
            .ok()
    });
    let logs_to_file = file.is_some();
    let registry = tracing_subscriber::registry().with(filter()).with(fmt::layer());
    let result = match file {
        Some(file) => registry.with(fmt::layer().with_ansi(false).with_writer(file)).try_init(),
        None => registry.try_init(),
    };
    if result.is_err() {
        return None; // a subscriber is already installed (tests)
    }
    logs_to_file.then_some(dir).flatten()
}

/// The newest file in `dir` whose name starts with `prefix`.
fn newest(dir: &Path, prefix: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(prefix))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
}

/// The last `n` lines of a text file.
fn tail(path: &Path, n: usize) -> Vec<String> {
    let Ok(bytes) = std::fs::read(path) else { return Vec::new() };
    let text = String::from_utf8_lossy(&bytes);
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..].iter().map(|l| l.to_string()).collect()
}

/// Where OpenRGB writes its own logs, for the modes Lumen runs it in.
fn openrgb_log_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    #[cfg(windows)]
    {
        if let Some(pf) = std::env::var_os("ProgramFiles").map(PathBuf::from) {
            dirs.push(pf.join(r"Lumen\OpenRGB\service_config\logs"));
            dirs.push(pf.join(r"OpenRGB\service_config\logs"));
        }
        if let Some(appdata) = std::env::var_os("APPDATA").map(PathBuf::from) {
            dirs.push(appdata.join(r"OpenRGB\logs"));
        }
    }
    #[cfg(not(windows))]
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        dirs.push(home.join(".config/OpenRGB/logs"));
        dirs.push(home.join("Library/Application Support/OpenRGB/logs"));
    }
    dirs
}

fn os_description() -> String {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let query = |value: &str| -> Option<String> {
            let out = std::process::Command::new("reg.exe")
                .args(["query", r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion", "/v", value])
                .creation_flags(0x0800_0000)
                .output()
                .ok()?;
            let text = String::from_utf8_lossy(&out.stdout).into_owned();
            text.lines().find(|l| l.contains(value)).and_then(|l| l.split_whitespace().last()).map(str::to_string)
        };
        let build = query("CurrentBuild").unwrap_or_default();
        let release = query("DisplayVersion").unwrap_or_default();
        // ProductName still says "Windows 10" on Windows 11; the build number tells them apart.
        let name = if build.parse::<u32>().is_ok_and(|b| b >= 22000) { "Windows 11" } else { "Windows 10" };
        format!("{name} {release} (build {build}, {})", std::env::consts::ARCH)
    }
    #[cfg(target_os = "macos")]
    {
        let version = std::process::Command::new("sw_vers").arg("-productVersion").output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
        format!("macOS {} ({})", version.unwrap_or_default(), std::env::consts::ARCH)
    }
    #[cfg(target_os = "linux")]
    {
        let pretty = std::fs::read_to_string("/etc/os-release").ok().and_then(|s| {
            s.lines().find_map(|l| l.strip_prefix("PRETTY_NAME=")).map(|v| v.trim_matches('"').to_string())
        });
        format!("{} ({})", pretty.unwrap_or_else(|| "Linux".into()), std::env::consts::ARCH)
    }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        format!("{} ({})", std::env::consts::OS, std::env::consts::ARCH)
    }
}

fn yes(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}

/// Everything a bug report needs, as Markdown.
pub fn report(app: &AppHandle) -> String {
    let state = app.state::<AppState>();
    let snap = state.engine.snapshot();
    let openrgb = state.openrgb.status();
    let settings = state.settings.lock().unwrap().clone();
    let policy = state.lights.get();
    let conflicts = state.conflicts.lock().unwrap().clone();

    let mut r = String::new();
    let _ = writeln!(r, "### Lumen diagnostics\n");
    let _ = writeln!(r, "- **Lumen** {} · {}", app.package_info().version, os_description());
    let _ = writeln!(
        r,
        "- **OpenRGB**: mode `{}`, server {}, started by Lumen: {}, bundled: {}{}{}",
        openrgb.mode,
        if openrgb.server_reachable { "reachable" } else { "not reachable" },
        yes(openrgb.started_by_lumen),
        yes(openrgb.bundled),
        if openrgb.legacy_task { ", **Lumen 0.2 task**" } else { "" },
        if openrgb.stale { ", **service runs an outdated copy**" } else { "" },
    );
    let _ = writeln!(r, "  - executable: `{}`", openrgb.exe.as_deref().unwrap_or("not found"));
    if let Some(s) = &openrgb.service {
        let _ = writeln!(r, "  - OpenRGB's own service: running {}, disabled {}", yes(s.running), yes(s.disabled));
    }
    let _ = writeln!(
        r,
        "- **Engine**: connected {}, {} devices, {} fps, paused {}, lights {}{}",
        yes(snap.connected),
        snap.devices.len(),
        snap.fps,
        yes(snap.paused),
        match snap.blackout {
            Some(b) => format!("off ({b:?})"),
            None => "on".into(),
        },
        snap.problem.as_ref().map(|p| format!(", problem: {p}")).unwrap_or_default(),
    );
    let _ = writeln!(
        r,
        "- **Settings**: lights off with sleep {}, display {}, lock {}, idle {} · battery alerts {} · shortcuts {} · auto-update {}",
        yes(policy.settings.sleep),
        yes(policy.settings.display_off),
        yes(policy.settings.lock),
        if policy.settings.idle_minutes == 0 { "never".to_string() } else { format!("{} min", policy.settings.idle_minutes) },
        yes(settings.battery_alerts.unwrap_or(true)),
        yes(settings.shortcuts),
        yes(settings.auto_update.unwrap_or(true)),
    );
    let running: Vec<&str> = conflicts.iter().map(|c| c.name).collect();
    let _ = writeln!(r, "- **Vendor apps running**: {}", if running.is_empty() { "none".to_string() } else { running.join(", ") });

    let _ = writeln!(r, "\n#### Devices\n");
    if snap.devices.is_empty() {
        let _ = writeln!(r, "None reported by OpenRGB.");
    } else {
        let _ = writeln!(r, "| Device | Vendor | Type | LEDs | Zones | Per-LED | Battery | Location |");
        let _ = writeln!(r, "|---|---|---|---|---|---|---|---|");
        for d in &snap.devices {
            let battery = d.battery.map(|b| format!("{}%{}", b.percent, if b.charging { " charging" } else { "" })).unwrap_or_else(|| "–".into());
            let _ = writeln!(
                r,
                "| {} | {} | {} | {} | {} | {} | {} | `{}` |",
                d.display_name,
                d.vendor,
                d.kind_label,
                d.led_count,
                d.zones.len(),
                yes(d.streamable),
                battery,
                d.location
            );
        }
    }

    let mut section = |title: &str, lines: Vec<String>| {
        if lines.is_empty() {
            return;
        }
        let _ = writeln!(r, "\n<details><summary>{title}</summary>\n\n```\n{}\n```\n</details>", lines.join("\n"));
    };
    if let Some(log) = state.log_dir.as_deref().and_then(|d| newest(d, LOG_PREFIX)) {
        section("Lumen log (last 200 lines)", tail(&log, 200));
    }
    if let Some(log) = openrgb_log_dirs().iter().filter_map(|d| newest(d, "OpenRGB")).max_by_key(|p| p.metadata().and_then(|m| m.modified()).ok()) {
        section(&format!("OpenRGB log `{}` (last 80 lines)", log.display()), tail(&log, 80));
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tails_and_finds_the_newest_log() {
        let dir = std::env::temp_dir().join(format!("lumen-diag-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let old = dir.join("lumen.2026-01-01.log");
        let new = dir.join("lumen.2026-01-02.log");
        std::fs::write(&old, "a\n").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&new, (1..=5).map(|i| format!("line {i}\n")).collect::<String>()).unwrap();
        std::fs::write(dir.join("other.log"), "x").unwrap();
        assert_eq!(newest(&dir, "lumen").as_deref(), Some(new.as_path()));
        assert_eq!(tail(&new, 2), ["line 4", "line 5"]);
        assert_eq!(tail(&new, 50).len(), 5);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn describes_this_os() {
        assert!(!os_description().is_empty());
    }
}
