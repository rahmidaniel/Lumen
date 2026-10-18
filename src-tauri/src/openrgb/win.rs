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
//!
//! Services, ports and processes are queried through the Windows API rather
//! than `sc.exe` and friends: Lumen checks them often, and Windows refuses to
//! start new programs while it shuts down, which is exactly when Lumen stops
//! its service ("sc.exe - Application Error 0xc0000142").

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::ServiceInfo;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// Lumen's OpenRGB service, and the name of Lumen 0.2's scheduled task.
pub const NAME: &str = "LumenOpenRGB";
/// OpenRGB's own service.
const OPENRGB_SERVICE: &str = "OpenRGB";
/// Priority class the adopt step gives OpenRGB (`ABOVE_NORMAL_PRIORITY_CLASS`
/// in Image File Execution Options terms), so its device threads keep up when
/// the PC is busy.
const OPENRGB_PRIORITY: u32 = 6;
const IFEO_PERF_OPTIONS: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\OpenRGB.exe\PerfOptions";
const COMMON_SCRIPT: &str = include_str!("../../packaging/windows/openrgb-common.ps1");
const ADOPT_SCRIPT: &str = include_str!("../../packaging/windows/openrgb-adopt.ps1");
const RELEASE_SCRIPT: &str = include_str!("../../packaging/windows/openrgb-release.ps1");
const STOP_SCRIPT: &str = include_str!("../../packaging/windows/openrgb-stop.ps1");

/// Run a console program without a window; `(exit code, stdout + stderr)`.
/// Only Lumen 0.2's scheduled task still needs one (`schtasks.exe`).
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

/// A service's run state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SvcState {
    Stopped,
    StartPending,
    StopPending,
    Running,
    Other,
}

/// State of OpenRGB's own service; `None` when it is not installed.
pub fn service() -> Option<ServiceInfo> {
    let state = scm::state(OPENRGB_SERVICE)?;
    let disabled = scm::config(OPENRGB_SERVICE).is_some_and(|c| c.disabled);
    Some(ServiceInfo { running: state == SvcState::Running, disabled })
}

/// State of Lumen's own service; `None` when it is not installed.
pub fn lumen_service() -> Option<SvcState> {
    scm::state(NAME)
}

/// The executable Lumen's service runs, from its configuration.
pub fn lumen_service_binary() -> Option<String> {
    scm::config(NAME).map(|c| executable_of(&c.command_line))
}

/// The program in a service's command line (`"C:\…\OpenRGB.exe" --flag` → `C:\…\OpenRGB.exe`).
fn executable_of(command_line: &str) -> String {
    let value = command_line.trim();
    match value.strip_prefix('"') {
        Some(rest) => rest.split('"').next().unwrap_or(rest).to_string(),
        None => value.to_string(),
    }
}

/// Start Lumen's service. Allowed without elevation by the permissions the
/// adopt step gives it; returns once the start request is accepted.
pub fn lumen_service_start() -> Result<(), String> {
    scm::start(NAME).map_err(|e| format!("could not start the {NAME} service: {e}"))
}

/// Ask Lumen's service to stop; returns once the request is delivered.
pub fn lumen_service_stop() -> Result<(), String> {
    scm::stop(NAME).map_err(|e| format!("could not stop the {NAME} service: {e}"))
}

/// Whether OpenRGB is set up to run above normal priority (done by the adopt
/// step from Lumen 0.3.1 on; services cannot be given a priority any other way).
pub fn priority_raised() -> bool {
    registry_dword(IFEO_PERF_OPTIONS, "CpuPriorityClass") == Some(OPENRGB_PRIORITY)
}

fn registry_dword(key: &str, value: &str) -> Option<u32> {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_SUBKEY_WOW6464KEY};
    let (key, value) = (wide(key), wide(value));
    let mut data = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: both strings are NUL-terminated and `data` matches `size`.
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD | RRF_SUBKEY_WOW6464KEY,
            std::ptr::null_mut(),
            (&mut data as *mut u32).cast(),
            &mut size,
        )
    };
    (status == 0).then_some(data)
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// A Windows error code as text: "Access is denied. (os error 5)".
fn os_error(code: u32) -> String {
    std::io::Error::from_raw_os_error(code as i32).to_string()
}

/// The Service Control Manager, called directly.
mod scm {
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_INSUFFICIENT_BUFFER, ERROR_SERVICE_ALREADY_RUNNING, ERROR_SERVICE_NOT_ACTIVE};
    use windows_sys::Win32::System::Services::{
        CloseServiceHandle, ControlService, OpenSCManagerW, OpenServiceW, QueryServiceConfigW, QueryServiceStatus, StartServiceW, QUERY_SERVICE_CONFIGW, SC_HANDLE,
        SC_MANAGER_CONNECT, SERVICE_CONTROL_STOP, SERVICE_DISABLED, SERVICE_QUERY_CONFIG, SERVICE_QUERY_STATUS, SERVICE_RUNNING, SERVICE_START,
        SERVICE_START_PENDING, SERVICE_STATUS, SERVICE_STOP, SERVICE_STOPPED, SERVICE_STOP_PENDING,
    };

    use super::{os_error, wide, SvcState};

    struct Handle(SC_HANDLE);

    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: the handle came from OpenSCManagerW/OpenServiceW and is closed once.
            unsafe { CloseServiceHandle(self.0) };
        }
    }

    /// A service opened with `access`; `Err` carries the Windows error code
    /// (1060 when there is no such service).
    struct Service {
        handle: Handle,
        _manager: Handle,
    }

    fn open(name: &str, access: u32) -> Result<Service, u32> {
        let name = wide(name);
        // SAFETY: plain calls with a NUL-terminated name; every handle is owned by a `Handle`.
        unsafe {
            let manager = OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_CONNECT);
            if manager.is_null() {
                return Err(GetLastError());
            }
            let manager = Handle(manager);
            let service = OpenServiceW(manager.0, name.as_ptr(), access);
            if service.is_null() {
                return Err(GetLastError());
            }
            Ok(Service { handle: Handle(service), _manager: manager })
        }
    }

    pub fn state(name: &str) -> Option<SvcState> {
        let service = open(name, SERVICE_QUERY_STATUS).ok()?;
        // SAFETY: SERVICE_STATUS is plain data and the handle is open.
        let mut status: SERVICE_STATUS = unsafe { std::mem::zeroed() };
        if unsafe { QueryServiceStatus(service.handle.0, &mut status) } == 0 {
            return Some(SvcState::Other);
        }
        Some(state_from(status.dwCurrentState))
    }

    pub(super) fn state_from(code: u32) -> SvcState {
        match code {
            SERVICE_STOPPED => SvcState::Stopped,
            SERVICE_START_PENDING => SvcState::StartPending,
            SERVICE_STOP_PENDING => SvcState::StopPending,
            SERVICE_RUNNING => SvcState::Running,
            _ => SvcState::Other,
        }
    }

    pub struct Config {
        pub disabled: bool,
        pub command_line: String,
    }

    pub fn config(name: &str) -> Option<Config> {
        let service = open(name, SERVICE_QUERY_CONFIG).ok()?;
        let mut needed = 0u32;
        // SAFETY: the first call only asks for the size; the second gets a
        // buffer of that size, aligned for QUERY_SERVICE_CONFIGW (u64 words),
        // whose string pointers point into the same buffer.
        unsafe {
            if QueryServiceConfigW(service.handle.0, std::ptr::null_mut(), 0, &mut needed) == 0 && GetLastError() != ERROR_INSUFFICIENT_BUFFER {
                return None;
            }
            let mut buf = vec![0u64; (needed as usize).div_ceil(8).max(1)];
            let config = buf.as_mut_ptr().cast::<QUERY_SERVICE_CONFIGW>();
            if QueryServiceConfigW(service.handle.0, config, (buf.len() * 8) as u32, &mut needed) == 0 {
                return None;
            }
            let config = &*config;
            Some(Config { disabled: config.dwStartType == SERVICE_DISABLED, command_line: from_wide(config.lpBinaryPathName) })
        }
    }

    unsafe fn from_wide(p: *const u16) -> String {
        if p.is_null() {
            return String::new();
        }
        let len = (0..).take_while(|&i| *p.add(i) != 0).count();
        String::from_utf16_lossy(std::slice::from_raw_parts(p, len))
    }

    pub fn start(name: &str) -> Result<(), String> {
        let service = open(name, SERVICE_START).map_err(os_error)?;
        // SAFETY: no start arguments; the handle is open.
        if unsafe { StartServiceW(service.handle.0, 0, std::ptr::null()) } == 0 {
            match unsafe { GetLastError() } {
                ERROR_SERVICE_ALREADY_RUNNING => {}
                code => return Err(os_error(code)),
            }
        }
        Ok(())
    }

    pub fn stop(name: &str) -> Result<(), String> {
        let service = open(name, SERVICE_STOP).map_err(os_error)?;
        // SAFETY: SERVICE_STATUS is plain data and the handle is open.
        let mut status: SERVICE_STATUS = unsafe { std::mem::zeroed() };
        if unsafe { ControlService(service.handle.0, SERVICE_CONTROL_STOP, &mut status) } == 0 {
            match unsafe { GetLastError() } {
                ERROR_SERVICE_NOT_ACTIVE => {}
                code => return Err(os_error(code)),
            }
        }
        Ok(())
    }
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

/// PID of the process listening on a local TCP (IPv4) port.
pub fn listener_pid(port: u16) -> Option<u32> {
    use windows_sys::Win32::NetworkManagement::IpHelper::{GetExtendedTcpTable, MIB_TCPROW_OWNER_PID, MIB_TCPTABLE_OWNER_PID, TCP_TABLE_OWNER_PID_LISTENER};
    const AF_INET: u32 = 2;
    let mut size = 0u32;
    let mut buf: Vec<u64> = Vec::new();
    // The table can grow between the size query and the read: retry a few times.
    for _ in 0..4 {
        // SAFETY: the buffer holds `size` bytes, aligned for the table (u64 words).
        let status = unsafe { GetExtendedTcpTable(buf.as_mut_ptr().cast(), &mut size, 0, AF_INET, TCP_TABLE_OWNER_PID_LISTENER, 0) };
        if status == 0 {
            // SAFETY: on success the buffer holds a MIB_TCPTABLE_OWNER_PID with `dwNumEntries` rows.
            let rows = unsafe {
                let table = &*buf.as_ptr().cast::<MIB_TCPTABLE_OWNER_PID>();
                std::slice::from_raw_parts(table.table.as_ptr(), table.dwNumEntries as usize)
            };
            return rows.iter().find(|row: &&MIB_TCPROW_OWNER_PID| port_of(row.dwLocalPort) == port).map(|row| row.dwOwningPid);
        }
        buf = vec![0u64; (size as usize).div_ceil(8) + 64];
        size = (buf.len() * 8) as u32;
    }
    None
}

/// TCP tables keep ports in network byte order, in the low 16 bits.
fn port_of(raw: u32) -> u16 {
    u16::from_be(raw as u16)
}

/// Executable path of a process we are allowed to inspect (not SYSTEM services).
pub fn process_path(pid: u32) -> Option<String> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
    // SAFETY: the handle is checked and closed; the buffer length is passed along.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
        CloseHandle(process);
        (ok != 0).then(|| String::from_utf16_lossy(&buf[..len as usize]))
    }
}

pub fn kill_pid(pid: u32) -> Result<(), String> {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError};
    use windows_sys::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
    // SAFETY: the handle is checked and closed.
    unsafe {
        let process = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if process.is_null() {
            return Err(format!("cannot open process {pid}: {}", os_error(GetLastError())));
        }
        let ok = TerminateProcess(process, 1);
        let error = GetLastError();
        CloseHandle(process);
        if ok == 0 {
            return Err(format!("cannot end process {pid}: {}", os_error(error)));
        }
    }
    Ok(())
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

    use super::wide;

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
    fn maps_service_states() {
        assert_eq!(scm::state_from(4), SvcState::Running);
        assert_eq!(scm::state_from(3), SvcState::StopPending);
        assert_eq!(scm::state_from(1), SvcState::Stopped);
        assert_eq!(scm::state_from(2), SvcState::StartPending);
        assert_eq!(scm::state_from(7), SvcState::Other, "paused");
    }

    #[test]
    fn finds_the_executable_in_a_service_command_line() {
        assert_eq!(executable_of(r#""C:\Program Files\Lumen\OpenRGB\OpenRGB.exe""#), r"C:\Program Files\Lumen\OpenRGB\OpenRGB.exe");
        assert_eq!(executable_of(r#""C:\Program Files\OpenRGB\OpenRGB.exe" --server"#), r"C:\Program Files\OpenRGB\OpenRGB.exe");
        assert_eq!(executable_of(r"C:\OpenRGB\OpenRGB.exe "), r"C:\OpenRGB\OpenRGB.exe");
    }

    #[test]
    fn reads_ports_in_network_order() {
        // 6742 = 0x1A56, stored big-endian in the low word.
        assert_eq!(port_of(0x0000_561A), 6742);
    }

    #[test]
    fn queries_services_and_ports_without_helpers() {
        // Every Windows install has these; nothing here needs a child process.
        assert_eq!(scm::state("EventLog"), Some(SvcState::Running));
        assert!(scm::config("EventLog").is_some_and(|c| c.command_line.to_ascii_lowercase().contains("svchost")));
        assert_eq!(scm::state("NoSuchServiceForLumen"), None);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert_eq!(listener_pid(port), Some(std::process::id()));
        let me = process_path(std::process::id()).unwrap();
        assert!(me.to_ascii_lowercase().ends_with(".exe"), "{me}");
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
