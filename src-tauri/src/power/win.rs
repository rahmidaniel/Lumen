//! A hidden window that receives Windows' power and session notifications.
//!
//! It has to be a real top-level window rather than a message-only one:
//! suspend/resume and end-of-session messages are broadcast to top-level
//! windows only. It is never shown and lives on its own thread, so it does not
//! depend on Lumen's main window (which comes and goes).

use std::sync::atomic::Ordering;
use std::sync::OnceLock;

use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, TRUE, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Power::{RegisterPowerSettingNotification, POWERBROADCAST_SETTING};
use windows_sys::Win32::System::RemoteDesktop::{WTSRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION};
use windows_sys::Win32::System::SystemInformation::GetTickCount;
use windows_sys::Win32::System::SystemServices::GUID_CONSOLE_DISPLAY_STATE;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW, TranslateMessage, DEVICE_NOTIFY_WINDOW_HANDLE, MSG,
    PBT_APMRESUMEAUTOMATIC, PBT_APMSUSPEND, PBT_POWERSETTINGCHANGE, WM_ENDSESSION, WM_POWERBROADCAST, WM_QUERYENDSESSION, WM_WTSSESSION_CHANGE,
    WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP, WTS_SESSION_LOCK, WTS_SESSION_UNLOCK,
};

use super::{PowerEvent, SESSION_ENDING};

type Handler = Box<dyn Fn(PowerEvent) + Send + Sync>;
static HANDLER: OnceLock<Handler> = OnceLock::new();

pub fn watch(handler: Handler) {
    if HANDLER.set(handler).is_err() {
        return; // already watching
    }
    // SAFETY: `pump` only touches the window it creates on this thread.
    if let Err(e) = std::thread::Builder::new().name("lumen-power".into()).spawn(|| unsafe { pump() }) {
        tracing::warn!(%e, "cannot watch power events");
    }
}

fn emit(event: PowerEvent) {
    tracing::info!(?event, "power event");
    if let Some(handler) = HANDLER.get() {
        handler(event);
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn same_guid(a: &GUID, b: &GUID) -> bool {
    (a.data1, a.data2, a.data3, a.data4) == (b.data1, b.data2, b.data3, b.data4)
}

unsafe fn pump() {
    let class = wide("LumenPowerWatcher");
    let title = wide("Lumen power events");
    let instance = GetModuleHandleW(std::ptr::null());
    let wc = WNDCLASSW { lpfnWndProc: Some(window_proc), hInstance: instance, lpszClassName: class.as_ptr(), ..std::mem::zeroed() };
    if RegisterClassW(&wc) == 0 {
        tracing::warn!("cannot register the power-event window class");
        return;
    }
    let hwnd = CreateWindowExW(
        WS_EX_TOOLWINDOW,
        class.as_ptr(),
        title.as_ptr(),
        WS_POPUP,
        0,
        0,
        0,
        0,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        instance,
        std::ptr::null(),
    );
    if hwnd.is_null() {
        tracing::warn!("cannot create the power-event window");
        return;
    }
    if WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION) == 0 {
        tracing::warn!("lock/unlock notifications unavailable");
    }
    if RegisterPowerSettingNotification(hwnd, &GUID_CONSOLE_DISPLAY_STATE, DEVICE_NOTIFY_WINDOW_HANDLE) == 0 {
        tracing::warn!("display on/off notifications unavailable");
    }
    let mut msg: MSG = std::mem::zeroed();
    while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_POWERBROADCAST => {
            match wparam as u32 {
                PBT_APMSUSPEND => emit(PowerEvent::Suspend),
                // Sent on every resume; PBT_APMRESUMESUSPEND only follows when a person woke the PC.
                PBT_APMRESUMEAUTOMATIC => emit(PowerEvent::Resume),
                PBT_POWERSETTINGCHANGE if lparam != 0 => {
                    let setting = &*(lparam as *const POWERBROADCAST_SETTING);
                    if same_guid(&setting.PowerSetting, &GUID_CONSOLE_DISPLAY_STATE) && setting.DataLength >= 1 {
                        // 0 = off, 1 = on, 2 = dimmed (still on).
                        emit(PowerEvent::Display(setting.Data[0] != 0));
                    }
                }
                _ => {}
            }
            TRUE as LRESULT
        }
        WM_WTSSESSION_CHANGE => {
            match wparam as u32 {
                WTS_SESSION_LOCK => emit(PowerEvent::Lock(true)),
                WTS_SESSION_UNLOCK => emit(PowerEvent::Lock(false)),
                _ => {}
            }
            0
        }
        // Windows asks every window before it ends the session, and only then
        // tells them it does. Lumen's process is gone the moment that news
        // reaches tao's own window (tao exits right there), so the lights go
        // out on the question, while there is still time.
        WM_QUERYENDSESSION => {
            if !SESSION_ENDING.swap(true, Ordering::SeqCst) {
                emit(PowerEvent::Shutdown);
            }
            TRUE as LRESULT
        }
        WM_ENDSESSION => {
            if wparam != 0 {
                if !SESSION_ENDING.swap(true, Ordering::SeqCst) {
                    emit(PowerEvent::Shutdown);
                }
            } else if SESSION_ENDING.swap(false, Ordering::SeqCst) {
                emit(PowerEvent::ShutdownCancelled);
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

pub fn idle_seconds() -> Option<u64> {
    let mut info = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    // SAFETY: plain out-parameter call.
    if unsafe { GetLastInputInfo(&mut info) } == 0 {
        return None;
    }
    // Both are milliseconds since boot in 32 bits; the difference survives the wrap.
    let now = unsafe { GetTickCount() };
    Some(u64::from(now.wrapping_sub(info.dwTime)) / 1000)
}
