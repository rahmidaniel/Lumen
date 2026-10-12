//! In-app updates through Tauri's updater plugin.
//!
//! The release workflow signs every installer and publishes a `latest.json`
//! manifest with the GitHub release; the app fetches that manifest (URL and
//! public key in `tauri.conf.json`), compares versions, and on request
//! downloads the installer, verifies its signature and runs it. Checks happen
//! on demand from Settings and in the background — once a minute after start,
//! then daily — unless the user turns the automatic check off. The background
//! check never installs anything by itself.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

/// What the UI needs to know about a newer release.
#[derive(Clone, Debug, Serialize)]
pub struct UpdateInfo {
    pub version: String,
    pub current: String,
    /// Release notes (the GitHub release body).
    pub notes: Option<String>,
    pub date: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Progress {
    pub downloaded: u64,
    pub total: Option<u64>,
}

/// The update found by the last check, kept so `install` can use it.
#[derive(Default)]
pub struct UpdateState {
    pending: Mutex<Option<Update>>,
}

impl UpdateState {
    pub fn available(&self) -> Option<UpdateInfo> {
        self.pending.lock().unwrap().as_ref().map(info)
    }
}

fn info(u: &Update) -> UpdateInfo {
    UpdateInfo {
        version: u.version.clone(),
        current: u.current_version.clone(),
        notes: u.body.clone().filter(|b| !b.trim().is_empty()),
        date: u.date.map(|d| d.date().to_string()),
    }
}

/// Ask the release feed for a newer version. Remembers the result and tells
/// the UI about it; `Ok(None)` means this is the latest version.
pub async fn check(app: &AppHandle) -> Result<Option<UpdateInfo>, String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    let found = updater.check().await.map_err(|e| friendly(e.to_string()))?;
    let result = found.as_ref().map(info);
    *app.state::<UpdateState>().pending.lock().unwrap() = found;
    match &result {
        Some(u) => {
            tracing::info!(version = %u.version, current = %u.current, "update available");
            let _ = app.emit("update_available", u.clone());
        }
        None => tracing::debug!("no update available"),
    }
    Ok(result)
}

/// Download, verify and install the remembered update, then restart. Progress
/// goes out as `update_progress` events. On Windows the installer takes over
/// and relaunches Lumen itself.
pub async fn install(app: &AppHandle) -> Result<(), String> {
    let update = app.state::<UpdateState>().pending.lock().unwrap().clone().ok_or_else(|| "No update has been found yet.".to_string())?;
    tracing::info!(version = %update.version, "installing update");
    let downloaded = Arc::new(AtomicU64::new(0));
    let (progress_handle, finish_handle) = (app.clone(), app.clone());
    let (progress_count, finish_count) = (downloaded.clone(), downloaded.clone());
    update
        .download_and_install(
            move |chunk, total| {
                let so_far = progress_count.fetch_add(chunk as u64, Ordering::Relaxed) + chunk as u64;
                let _ = progress_handle.emit("update_progress", Progress { downloaded: so_far, total });
            },
            move || {
                let so_far = finish_count.load(Ordering::Relaxed);
                let _ = finish_handle.emit("update_progress", Progress { downloaded: so_far, total: Some(so_far) });
            },
        )
        .await
        .map_err(|e| friendly(e.to_string()))?;
    app.restart();
}

/// Background checks: a minute after start (launch stays quick), then daily,
/// as long as `enabled()` says so.
pub async fn auto_check(app: AppHandle, enabled: impl Fn() -> bool + Send + 'static) {
    tokio::time::sleep(Duration::from_secs(60)).await;
    loop {
        if enabled() {
            if let Err(e) = check(&app).await {
                tracing::debug!(%e, "automatic update check failed");
            }
        }
        tokio::time::sleep(Duration::from_secs(24 * 60 * 60)).await;
    }
}

/// Turn the updater's error strings into something a settings panel can show.
fn friendly(e: String) -> String {
    let lower = e.to_ascii_lowercase();
    if lower.contains("404") || lower.contains("not found") {
        "No release feed was found. The project's releases must be public for updates to work.".into()
    } else if lower.contains("dns") || lower.contains("connect") || lower.contains("timed out") || lower.contains("network") {
        format!("Could not reach the release feed ({e}).")
    } else {
        e
    }
}
