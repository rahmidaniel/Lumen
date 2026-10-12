//! Windows specifics: the OpenRGB service OpenRGB's installer registers, and
//! the `LumenOpenRGB` service Lumen uses instead once the user hands OpenRGB
//! over.
//!
//! Lumen's service runs a copy of the bundled OpenRGB from
//! `Program Files\Lumen\OpenRGB` (only administrators can change it) as
//! LocalSystem, which OpenRGB needs for SMBus devices. Its permissions let
//! interactive users start and stop it, so a normal-user Lumen ties
//! OpenRGB's lifetime to its own without prompts; installing it (and
//! retiring OpenRGB's own service) is the one elevated step
//! (`openrgb-adopt.ps1`).
//!
//! Lumen 0.2 used a scheduled task instead, which ran OpenRGB elevated from
//! the per-user install folder; any program of the user's could swap that
//! executable. It is still recognised (and used) until the user switches over,
//! which removes it.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::ServiceInfo;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// Lumen's OpenRGB service, and the name of Lumen 0.2's scheduled task.
pub const NAME: &str = "LumenOpenRGB";
/// OpenRGB's own service.
const OPENRGB_SERVICE: &str = "OpenRGB";
const COMMON_SCRIPT: &str = include_str!("../../packaging/windows/openrgb-common.ps1");
const ADOPT_SCRIPT: &str = include_str!("../../packaging/windows/openrgb-adopt.ps1");
const RELEASE_SCRIPT: &str = include_str!("../../packaging/windows/openrgb-release.ps1");
const STOP_SCRIPT: &str = include_str!("../../packaging/windows/openrgb-stop.ps1");

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

/// A service's run state, from `sc query`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SvcState {
    Stopped,
    StartPending,
    StopPending,
    Running,
    Other,
}

fn parse_state(query: &str) -> SvcState {
    let Some(line) = query.lines().find(|l| l.trim_start().starts_with("STATE")) else { return SvcState::Other };
    if line.contains("STOP_PENDING") {
        SvcState::StopPending
    } else if line.contains("START_PENDING") {
        SvcState::StartPending
    } else if line.contains("RUNNING") {
        SvcState::Running
    } else if line.contains("STOPPED") {
        SvcState::Stopped
    } else {
        SvcState::Other
    }
}

fn svc_state_of(name: &str) -> Option<SvcState> {
    match run("sc.exe", &["query", name]).ok()? {
        (0, query) => Some(parse_state(&query)),
        _ => None, // 1060: no such service
    }
}

/// State of OpenRGB's own service; `None` when it is not installed.
pub fn service() -> Option<ServiceInfo> {
    let state = svc_state_of(OPENRGB_SERVICE)?;
    let disabled = run("sc.exe", &["qc", OPENRGB_SERVICE]).map(|(_, qc)| qc.contains("DISABLED")).unwrap_or(false);
    Some(ServiceInfo { running: state == SvcState::Running, disabled })
}

/// State of Lumen's own service; `None` when it is not installed.
pub fn lumen_service() -> Option<SvcState> {
    svc_state_of(NAME)
}

/// The executable Lumen's service runs, from its configuration.
pub fn lumen_service_binary() -> Option<String> {
    let (code, qc) = run("sc.exe", &["qc", NAME]).ok()?;
    if code != 0 {
        return None;
    }
    let line = qc.lines().find(|l| l.trim_start().starts_with("BINARY_PATH_NAME"))?;
    let value = line.split_once(':')?.1.trim();
    let value = value.strip_prefix('"').map(|v| v.split('"').next().unwrap_or(v)).unwrap_or(value);
    Some(value.to_string())
}

/// Start Lumen's service. Allowed without elevation by the permissions the
/// adopt step gives it; returns once the start request is accepted.
pub fn lumen_service_start() -> Result<(), String> {
    match run("sc.exe", &["start", NAME])? {
        (0, _) | (1056, _) => Ok(()), // 1056: already running
        (code, out) => Err(format!("could not start the {NAME} service ({code}): {}", last_line(&out))),
    }
}

/// Ask Lumen's service to stop; returns once the request is delivered.
pub fn lumen_service_stop() -> Result<(), String> {
    match run("sc.exe", &["stop", NAME])? {
        (0, _) | (1062, _) => Ok(()), // 1062: not running
        (code, out) => Err(format!("could not stop the {NAME} service ({code}): {}", last_line(&out))),
    }
}

fn last_line(out: &str) -> &str {
    out.lines().map(str::trim).rfind(|l| !l.is_empty()).unwrap_or("")
}

/// Where the adopt step puts Lumen's protected copy of OpenRGB.
pub fn protected_dir() -> PathBuf {
    let base = std::env::var_os("ProgramFiles").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Program Files"));
    base.join("Lumen").join("OpenRGB")
}

pub fn task_exists() -> bool {
    run("schtasks.exe", &["/Query", "/TN", NAME]).map(|(code, _)| code == 0).unwrap_or(false)
}

pub fn task_run() -> Result<(), String> {
    match run("schtasks.exe", &["/Run", "/TN", NAME])? {
        (0, _) => Ok(()),
        (code, out) => Err(format!("could not start the {NAME} task ({code}): {}", out.trim())),
    }
}

/// End the task, which terminates the OpenRGB process it started.
pub fn task_end() -> Result<(), String> {
    match run("schtasks.exe", &["/End", "/TN", NAME])? {
        (0, _) => Ok(()),
        (code, out) => Err(format!("could not end the {NAME} task ({code}): {}", out.trim())),
    }
}

/// PID of the process listening on a local TCP port.
pub fn listener_pid(port: u16) -> Option<u32> {
    let (_, out) = run("netstat.exe", &["-ano", "-p", "TCP"]).ok()?;
    let suffix = format!(":{port}");
    for line in out.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() >= 5 && cols[0] == "TCP" && cols[3] == "LISTENING" && cols[1].ends_with(&suffix) {
            return cols[4].parse().ok();
        }
    }
    None
}

/// Executable path of a process we are allowed to inspect (not SYSTEM services).
pub fn process_path(pid: u32) -> Option<String> {
    let (code, out) = run(
        "powershell.exe",
        &["-NoProfile", "-NonInteractive", "-Command", &format!("(Get-Process -Id {pid} -ErrorAction SilentlyContinue).Path")],
    )
    .ok()?;
    (code == 0).then(|| out.trim().to_string()).filter(|s| !s.is_empty())
}

pub fn kill_pid(pid: u32) -> Result<(), String> {
    match run("taskkill.exe", &["/PID", &pid.to_string(), "/F"])? {
        (0, _) => Ok(()),
        (code, out) => Err(format!("taskkill failed ({code}): {}", out.trim())),
    }
}

/// One-time elevated setup: retire OpenRGB's service and Lumen 0.2's task,
/// copy the OpenRGB folder `source` somewhere protected, install the service.
pub fn adopt(source: &Path) -> Result<(), String> {
    let source = super::plain_path(source);
    run_elevated(ADOPT_SCRIPT, &[("Source", source.as_str())])
}

/// Elevated: remove Lumen's service (and task) and give OpenRGB back to its own service.
pub fn release() -> Result<(), String> {
    run_elevated(RELEASE_SCRIPT, &[])
}

/// Elevated: end Lumen's service when it ignores a stop request.
pub fn force_stop_lumen_service() -> Result<(), String> {
    run_elevated(STOP_SCRIPT, &[("Service", NAME)])
}

/// Elevated: restart OpenRGB's own service (when Lumen merely attaches to it).
pub fn restart_service() -> Result<(), String> {
    run_elevated(STOP_SCRIPT, &[("Service", OPENRGB_SERVICE), ("Then", "start")])
}

/// `'…'` for PowerShell, with embedded quotes doubled.
fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// The complete program an elevated PowerShell runs: the shared helpers, then
/// `script` as a script block called with `params` and `-Log`.
fn elevated_program(script: &str, params: &[(&str, &str)], log: &str) -> String {
    let mut program = format!(". {{\n{COMMON_SCRIPT}\n}}\n& {{\n{script}\n}}");
    for (key, value) in params.iter().chain([("Log", log)].iter()) {
        program.push_str(&format!(" -{key} {}", ps_quote(value)));
    }
    program.push('\n');
    program
}

/// Run `script` in an elevated PowerShell (one UAC prompt) and wait for it.
///
/// The script travels inside the elevated process's own command line
/// (`-EncodedCommand`) rather than through a file in the user's temp folder,
/// so nothing can swap it between Lumen writing it and the elevated shell
/// reading it. Only the error message comes back through a file.
fn run_elevated(script: &str, params: &[(&str, &str)]) -> Result<(), String> {
    let log = std::env::temp_dir().join(format!("lumen-elevated-{}.log", std::process::id()));
    let _ = std::fs::remove_file(&log);
    let program = elevated_program(script, params, &log.display().to_string());
    let args = format!("-NoProfile -NonInteractive -ExecutionPolicy Bypass -WindowStyle Hidden -EncodedCommand {}", base64(&utf16le(&program)));
    let code = shell::run_as_admin(&shell::powershell(), &args)?;
    if code == 0 {
        return Ok(());
    }
    let detail = std::fs::read_to_string(&log).ok().map(|s| s.trim().trim_start_matches('\u{feff}').to_string()).filter(|s| !s.is_empty());
    let _ = std::fs::remove_file(&log);
    Err(detail.unwrap_or_else(|| format!("the elevated step failed (exit code {code})")))
}

mod shell {
    use std::path::PathBuf;

    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_CANCELLED};
    use windows_sys::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE};
    use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
    use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    /// Windows PowerShell by full path: whatever runs elevated must not be
    /// found through a search that a user-writable folder could win.
    pub fn powershell() -> String {
        let mut buf = [0u16; 260];
        // SAFETY: the buffer and its length match.
        let len = unsafe { GetSystemDirectoryW(buf.as_mut_ptr(), buf.len() as u32) } as usize;
        let system = if len > 0 && len < buf.len() { PathBuf::from(String::from_utf16_lossy(&buf[..len])) } else { PathBuf::from(r"C:\Windows\System32") };
        system.join(r"WindowsPowerShell\v1.0\powershell.exe").display().to_string()
    }

    /// Start `file args` through UAC ("runas") and wait; the process's exit code.
    pub fn run_as_admin(file: &str, args: &str) -> Result<u32, String> {
        let (verb, file, args) = (wide("runas"), wide(file), wide(args));
        // SAFETY: COM is initialised for this (blocking) thread around the
        // call; every pointer outlives it; the process handle is closed.
        unsafe {
            let com = CoInitializeEx(std::ptr::null(), (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32);
            let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
            info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
            info.lpVerb = verb.as_ptr();
            info.lpFile = file.as_ptr();
            info.lpParameters = args.as_ptr();
            info.nShow = SW_HIDE;
            let ok = ShellExecuteExW(&mut info);
            let error = GetLastError();
            if com >= 0 {
                CoUninitialize();
            }
            if ok == 0 {
                return Err(if error == ERROR_CANCELLED {
                    "The administrator prompt was cancelled.".to_string()
                } else {
                    format!("could not start the elevated step (error {error})")
                });
            }
            if info.hProcess.is_null() {
                return Err("the elevated step did not start".to_string());
            }
            WaitForSingleObject(info.hProcess, INFINITE);
            let mut code = 0u32;
            GetExitCodeProcess(info.hProcess, &mut code);
            CloseHandle(info.hProcess);
            Ok(code)
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

    #[test]
    fn parses_service_states() {
        let running = "SERVICE_NAME: LumenOpenRGB\r\n        TYPE               : 10  WIN32_OWN_PROCESS\r\n        STATE              : 4  RUNNING\r\n";
        assert_eq!(parse_state(running), SvcState::Running);
        assert_eq!(parse_state("        STATE              : 3  STOP_PENDING\r\n"), SvcState::StopPending);
        assert_eq!(parse_state("        STATE              : 1  STOPPED\r\n"), SvcState::Stopped);
        assert_eq!(parse_state("        STATE              : 2  START_PENDING\r\n"), SvcState::StartPending);
        assert_eq!(parse_state("garbage"), SvcState::Other);
    }

    #[test]
    fn elevated_program_quotes_parameters() {
        let p = elevated_program("param($Source, $Log)\nexit 0", &[("Source", r"C:\Users\O'Brien\Lumen\openrgb")], r"C:\Temp\x.log");
        assert!(p.starts_with(". {\n"), "helpers are dot-sourced first");
        assert!(p.contains("function Stop-ServiceHard"));
        assert!(p.trim_end().ends_with(r"-Source 'C:\Users\O''Brien\Lumen\openrgb' -Log 'C:\Temp\x.log'"));
    }

    #[test]
    fn elevated_programs_fit_on_a_command_line() {
        // CreateProcess allows 32 767 characters; leave room for the switches.
        for script in [ADOPT_SCRIPT, RELEASE_SCRIPT, STOP_SCRIPT] {
            let program = elevated_program(script, &[("Source", r"C:\Users\someone\AppData\Local\Lumen\openrgb")], r"C:\Temp\lumen.log");
            assert!(base64(&utf16le(&program)).len() < 30_000);
        }
    }

    #[test]
    fn this_machine_has_windows_powershell() {
        assert!(std::path::Path::new(&shell::powershell()).is_file());
    }
}
