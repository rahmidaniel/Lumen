//! Lighting has to keep its rhythm while the PC is busy.
//!
//! Windows shares the CPU round-robin among threads of equal priority, and a
//! thread waking from a timer (the engine's frame tick) gets no boost for it.
//! With a game or a build keeping every core busy, frames went out 100–200 ms
//! late and the devices stuttered and fell out of step. Lumen needs about 1 %
//! of one core, so it asks for above-normal priority: enough to be scheduled
//! on time, still below anything the system itself counts on. It also opts
//! out of the power throttling (EcoQoS) Windows may apply to programs without
//! a visible window, which is what Lumen in the tray is.
//!
//! The WebView's processes do not inherit this; the UI stays at normal priority.
//! OpenRGB gets the same treatment where Lumen starts it (see `openrgb`).

/// Raise this process's scheduling priority, where the platform allows it.
pub fn raise() {
    #[cfg(windows)]
    win::raise();
}

#[cfg(windows)]
mod win {
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, ProcessPowerThrottling, SetPriorityClass, SetProcessInformation, ABOVE_NORMAL_PRIORITY_CLASS, PROCESS_POWER_THROTTLING_CURRENT_VERSION,
        PROCESS_POWER_THROTTLING_EXECUTION_SPEED, PROCESS_POWER_THROTTLING_STATE,
    };

    pub fn raise() {
        // SAFETY: the pseudo-handle needs no closing; the state struct is passed with its size.
        unsafe {
            if SetPriorityClass(GetCurrentProcess(), ABOVE_NORMAL_PRIORITY_CLASS) == 0 {
                tracing::warn!(e = %std::io::Error::last_os_error(), "could not raise Lumen's priority");
            }
            // The bit under our control, with its state clear: never throttle.
            let state = PROCESS_POWER_THROTTLING_STATE {
                Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
                ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
                StateMask: 0,
            };
            if SetProcessInformation(
                GetCurrentProcess(),
                ProcessPowerThrottling,
                (&state as *const PROCESS_POWER_THROTTLING_STATE).cast(),
                std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
            ) == 0
            {
                tracing::debug!(e = %std::io::Error::last_os_error(), "power throttling opt-out unavailable");
            }
        }
    }
}
