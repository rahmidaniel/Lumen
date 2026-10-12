//! Vendor apps that fight Lumen over the same devices. With two programs
//! writing colours, LEDs flicker between both, and some apps (iCUE, G HUB)
//! hold their devices so OpenRGB cannot reach them at all. Lumen cannot stop
//! them, but it can say which one is running.

use serde::Serialize;

/// A running app that takes over lighting.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Conflict {
    /// Stable key, e.g. for "don't warn me about this again".
    pub id: &'static str,
    pub name: &'static str,
    /// Whose devices it takes over.
    pub brands: &'static str,
}

struct KnownApp {
    id: &'static str,
    name: &'static str,
    brands: &'static str,
    /// Lower-case executable names without `.exe`. Only processes that drive
    /// lighting: background helpers that leave the LEDs alone (Razer's Chroma
    /// SDK service, for one) are not listed.
    processes: &'static [&'static str],
}

const APPS: &[KnownApp] = &[
    KnownApp { id: "razer-synapse", name: "Razer Synapse", brands: "Razer", processes: &["razerappengine", "razer synapse 3", "razer synapse service", "rzsynapse"] },
    KnownApp { id: "logitech-ghub", name: "Logitech G HUB", brands: "Logitech", processes: &["lghub", "lghub_agent"] },
    KnownApp { id: "logitech-lgs", name: "Logitech Gaming Software", brands: "Logitech", processes: &["lcore"] },
    KnownApp { id: "corsair-icue", name: "Corsair iCUE", brands: "Corsair", processes: &["icue", "corsair.service"] },
    KnownApp { id: "steelseries-gg", name: "SteelSeries GG", brands: "SteelSeries", processes: &["steelseriesgg", "steelseriesengine", "steelseriesengine3"] },
    KnownApp { id: "asus-armoury-crate", name: "ASUS Armoury Crate", brands: "ASUS", processes: &["armourycrate", "lightingservice"] },
    KnownApp { id: "hyperx-ngenuity", name: "HyperX NGENUITY", brands: "HyperX", processes: &["ngenuity2"] },
    KnownApp { id: "glorious-core", name: "Glorious CORE", brands: "Glorious", processes: &["glorious core", "gloriouscore"] },
    KnownApp { id: "msi-mystic-light", name: "MSI Mystic Light", brands: "MSI", processes: &["ledkeeper2", "mysticlight"] },
    KnownApp { id: "nzxt-cam", name: "NZXT CAM", brands: "NZXT", processes: &["nzxt cam"] },
    KnownApp { id: "signalrgb", name: "SignalRGB", brands: "every brand", processes: &["signalrgb", "signalrgblauncher"] },
];

/// `OpenRGB.exe`, `C:\x\iCUE.EXE`, `lghub` → `openrgb`, `icue`, `lghub`.
pub fn normalize(process: &str) -> String {
    let name = process.rsplit(['\\', '/']).next().unwrap_or(process).trim().to_ascii_lowercase();
    name.strip_suffix(".exe").map(str::to_string).unwrap_or(name)
}

/// Which known apps appear among `running` (already normalised names).
pub fn detect(running: &[String]) -> Vec<Conflict> {
    APPS.iter()
        .filter(|app| app.processes.iter().any(|p| running.iter().any(|r| r == p)))
        .map(|app| Conflict { id: app.id, name: app.name, brands: app.brands })
        .collect()
}

/// Names of the running processes, normalised. Empty where the platform gives us nothing.
pub fn running_processes() -> Vec<String> {
    let names = platform::process_names();
    let mut out: Vec<String> = names.iter().map(|n| normalize(n)).collect();
    out.sort();
    out.dedup();
    out
}

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};

    pub fn process_names() -> Vec<String> {
        let mut names = Vec::new();
        // SAFETY: a process snapshot walked with a correctly sized entry, then closed.
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                return names;
            }
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            let mut ok = Process32FirstW(snapshot, &mut entry);
            while ok != 0 {
                let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
                names.push(String::from_utf16_lossy(&entry.szExeFile[..len]));
                ok = Process32NextW(snapshot, &mut entry);
            }
            CloseHandle(snapshot);
        }
        names
    }
}

#[cfg(target_os = "linux")]
mod platform {
    pub fn process_names() -> Vec<String> {
        let Ok(entries) = std::fs::read_dir("/proc") else { return Vec::new() };
        entries
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit()))
            .filter_map(|e| std::fs::read_to_string(e.path().join("comm")).ok())
            .map(|s| s.trim().to_string())
            .collect()
    }
}

#[cfg(target_os = "macos")]
mod platform {
    pub fn process_names() -> Vec<String> {
        match std::process::Command::new("ps").args(["-axco", "comm="]).output() {
            Ok(out) => String::from_utf8_lossy(&out.stdout).lines().map(|l| l.trim().to_string()).collect(),
            Err(_) => Vec::new(),
        }
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
mod platform {
    pub fn process_names() -> Vec<String> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_paths_case_and_extension() {
        assert_eq!(normalize(r"C:\Program Files\Corsair\iCUE.EXE"), "icue");
        assert_eq!(normalize("RazerAppEngine.exe"), "razerappengine");
        assert_eq!(normalize("/usr/bin/signalrgb"), "signalrgb");
        assert_eq!(normalize("lghub_agent"), "lghub_agent");
    }

    #[test]
    fn finds_lighting_apps_but_not_their_quiet_helpers() {
        let running: Vec<String> = ["explorer", "razerappengine", "rzsdkserver", "rzdevicemanager", "lghub_agent", "lumen", "openrgb"].map(String::from).to_vec();
        let found: Vec<&str> = detect(&running).iter().map(|c| c.id).collect();
        assert_eq!(found, ["razer-synapse", "logitech-ghub"]);
        assert!(detect(&["rzsdkserver".to_string()]).is_empty(), "the Chroma SDK service alone does not fight Lumen");
    }

    #[test]
    fn this_machine_lists_its_own_processes() {
        // Lumen's own test binary is running, so the list cannot be empty here.
        if cfg!(any(windows, target_os = "linux", target_os = "macos")) {
            assert!(!running_processes().is_empty());
        }
    }
}
