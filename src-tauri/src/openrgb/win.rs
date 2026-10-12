//! Windows specifics: the OpenRGB service OpenRGB's installer registers, and
//! the Task Scheduler task Lumen uses instead once the user hands OpenRGB over.
//!
//! The task runs OpenRGB "with highest privileges" for the current user, so a
//! normal-user Lumen can start (`schtasks /Run`), end and restart an elevated
//! OpenRGB without any further UAC prompt; registering it, and stopping and
//! disabling the service, is the one elevated step (`openrgb-adopt.ps1`).

use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

use super::ServiceInfo;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
pub const TASK_NAME: &str = "LumenOpenRGB";
const ADOPT_SCRIPT: &str = include_str!("../../packaging/windows/openrgb-adopt.ps1");
const RELEASE_SCRIPT: &str = include_str!("../../packaging/windows/openrgb-release.ps1");

/// Run a console program without a window; `(exit code, stdout + stderr)`.
fn run(program: &str, args: &[&str]) -> Result<(i32, String), String> {
    let out = Command::new(program)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run {program}: {e}"))?;
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    Ok((out.status.code().unwrap_or(-1), text))
}

/// State of the `OpenRGB` Windows service; `None` when it is not installed.
pub fn service() -> Option<ServiceInfo> {
    let (code, query) = run("sc.exe", &["query", "OpenRGB"]).ok()?;
    if code != 0 {
        return None;
    }
    let running = query.contains("RUNNING") && !query.contains("STOP_PENDING");
    let disabled = run("sc.exe", &["qc", "OpenRGB"]).map(|(_, qc)| qc.contains("DISABLED")).unwrap_or(false);
    Some(ServiceInfo { running, disabled })
}

pub fn task_exists() -> bool {
    run("schtasks.exe", &["/Query", "/TN", TASK_NAME]).map(|(code, _)| code == 0).unwrap_or(false)
}

pub fn task_run() -> Result<(), String> {
    match run("schtasks.exe", &["/Run", "/TN", TASK_NAME])? {
        (0, _) => Ok(()),
        (code, out) => Err(format!("could not start the {TASK_NAME} task ({code}): {}", out.trim())),
    }
}

/// End the task, which terminates the OpenRGB process it started.
pub fn task_end() -> Result<(), String> {
    match run("schtasks.exe", &["/End", "/TN", TASK_NAME])? {
        (0, _) => Ok(()),
        (code, out) => Err(format!("could not end the {TASK_NAME} task ({code}): {}", out.trim())),
    }
}

fn current_user() -> String {
    let domain = std::env::var("USERDOMAIN").or_else(|_| std::env::var("COMPUTERNAME")).unwrap_or_default();
    let user = std::env::var("USERNAME").unwrap_or_default();
    if domain.is_empty() {
        user
    } else {
        format!("{domain}\\{user}")
    }
}

/// One-time elevated setup: stop and disable the service, register the task.
pub fn adopt(exe: &Path) -> Result<(), String> {
    let exe = exe.display().to_string();
    let user = current_user();
    run_elevated("lumen-openrgb-adopt.ps1", ADOPT_SCRIPT, &[("Exe", exe.as_str()), ("User", user.as_str())])
}

/// Elevated: remove the task and give OpenRGB back to its service.
pub fn release() -> Result<(), String> {
    run_elevated("lumen-openrgb-release.ps1", RELEASE_SCRIPT, &[])
}

/// Elevated: restart the OpenRGB Windows service (the fallback when OpenRGB
/// is not managed by Lumen).
pub fn restart_service() -> Result<(), String> {
    const SCRIPT: &str = "param([string]$Log)\n$ErrorActionPreference = 'Stop'\ntry { Restart-Service -Name OpenRGB; exit 0 } catch { if ($Log) { \"$($_.Exception.Message)\" | Out-File -FilePath $Log -Encoding utf8 }; exit 1 }\n";
    run_elevated("lumen-openrgb-restart.ps1", SCRIPT, &[])
}

/// Write `script` to the temp directory and run it elevated through UAC,
/// waiting for it to finish. Parameters are passed as `-Name "value"`.
fn run_elevated(name: &str, script: &str, params: &[(&str, &str)]) -> Result<(), String> {
    let dir = std::env::temp_dir();
    let path = dir.join(name);
    let log = dir.join(format!("{name}.log"));
    let _ = std::fs::remove_file(&log);
    std::fs::write(&path, script).map_err(|e| format!("cannot write {}: {e}", path.display()))?;

    let mut inner = format!(
        "-NoProfile -NonInteractive -ExecutionPolicy Bypass -WindowStyle Hidden -File \"{}\" -Log \"{}\"",
        path.display(),
        log.display()
    );
    for (key, value) in params {
        inner.push_str(&format!(" -{key} \"{value}\""));
    }
    // Everything goes through -EncodedCommand, so no command-line quoting is involved.
    let command = format!(
        "$ErrorActionPreference = 'Stop'; try {{ $p = Start-Process -Verb RunAs -Wait -PassThru -WindowStyle Hidden -FilePath powershell -ArgumentList '{}'; exit $p.ExitCode }} catch {{ exit 125 }}",
        inner.replace('\'', "''")
    );
    let encoded = base64(&utf16le(&command));
    let (code, _) = run("powershell.exe", &["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-EncodedCommand", &encoded])?;
    match code {
        0 => Ok(()),
        125 => Err("The administrator prompt was cancelled.".into()),
        _ => {
            let detail = std::fs::read_to_string(&log).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
            Err(detail.unwrap_or_else(|| format!("the elevated step failed (exit code {code})")))
        }
    }
}

fn utf16le(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { TABLE[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { TABLE[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_reference() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        // "exit 0" as PowerShell's -EncodedCommand expects it (UTF-16LE).
        assert_eq!(base64(&utf16le("exit 0")), "ZQB4AGkAdAAgADAA");
    }
}
