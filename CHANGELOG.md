# Changelog

All notable changes to Lumen. Installers are on the
[releases page](https://github.com/rahmidaniel/lumen/releases); from 0.2.1 on, Lumen
updates itself from Settings → Updates.

## Unreleased

### Security

- **Lumen 0.2's *Let Lumen manage OpenRGB* could be misused to get administrator
  rights.** Its scheduled task ran OpenRGB with highest privileges straight from the
  per-user install folder, with OpenRGB's plugins loaded from the user's profile, so any
  program running as the user could swap a file there and have it run elevated without a
  prompt. Managed mode is now a Windows service that runs a copy of OpenRGB from
  `Program Files\Lumen\OpenRGB`, which only administrators can change. If you turned the
  old mode on, Lumen shows **Security fix needed**: Settings → OpenRGB → *Switch to the
  secure setup* replaces the task (one admin prompt).
- Elevated steps no longer run a script from the temp folder: the script travels inside
  the elevated PowerShell's own command line.

### New

- **Lights follow the PC** (Windows): off while it sleeps, shuts down or turns the screen
  off, and optionally while it is locked or idle (Settings → Lights).
- A **lights switch** in the top bar, the tray menu and as a shortcut, remembered across
  restarts. Pause now freezes the effects only.
- **Low-battery notifications** at 20 % and 5 %.
- A warning when **another lighting app** (Synapse, G HUB, iCUE, SteelSeries GG, Armoury
  Crate, NGENUITY, SignalRGB, …) is running, with how to get it out of the way.
- **Tray menu** with the lights switch, brightness and profiles.
- Optional **global shortcuts**: Ctrl+Alt+Shift+L for the lights, ↑/↓ for brightness, →/←
  for profiles.
- **Settings → Help**: copy a diagnostics report for bug reports and open the log folder.
  Release builds now write a log file (they had none).
- When OpenRGB runs but finds no devices, the canvas explains the usual causes instead of
  spinning forever.
- **Device settings**: polling rate, DPI stages, sleep timer and power-saving threshold for
  Razer devices; polling rate and DPI stages for the HyperX Pulsefire Haste 2 (wired and
  wireless). Read from the device when you select it, and read back after every change. The
  Haste 2 forgets them when switched off, so Lumen puts them back.
- New app icon, replacing the Tauri placeholder (installer, taskbar, tray, Start menu).
- README screenshots and demo of the current version.

### Fixed

- When OpenRGB crashed or was ended, Lumen never started it again; now it does within a
  few seconds.

## 0.2.2 — 2026-10-12

Fixes for the Windows *Let Lumen manage OpenRGB* mode:

- The task Lumen registers to run OpenRGB with administrator rights could keep pointing
  at the copy of OpenRGB from an earlier install (or a development build) after an
  update. Settings → OpenRGB now says so and offers **Update the OpenRGB task**, and an
  OpenRGB started by that older task is adopted like any other Lumen-owned process.
- Paths from Tauri's resource directory carry Windows' `\\?\` prefix; they are now
  normalised before Lumen compares them or hands them to the elevated scripts, which
  also fixes recognising an OpenRGB left behind by a killed Lumen.

## 0.2.1 — 2026-10-12

First release with in-app updates: Settings → Updates finds and installs newer versions,
and a daily background check can be switched off.

- **Paint keys**: a layer type that colours individual keys and LEDs by clicking or
  dragging on the canvas (Shift erases). Unpainted LEDs keep showing the layers below.
- **Real keyboard layouts**: keyboards with OpenRGB's standard key names are drawn on a
  physical ANSI or ISO layout (detected from the keys present, ISO Enter included)
  instead of the firmware's grid.
- **Graphics cards** are drawn as a card with bracket, fans and light bar.
- **Battery readout** for Logitech (HID++), Glorious, ASUS ROG, SteelSeries and Corsair
  wireless devices, in addition to Razer and HyperX. Written from the published
  protocols and untested on real hardware.
- **OpenRGB lifecycle**: Lumen starts, restarts and stops the OpenRGB it runs. On
  Windows, Settings → OpenRGB → *Let Lumen manage OpenRGB* retires the OpenRGB Windows
  service in favour of an elevated task Lumen controls (one admin prompt, once).
- Clear status when OpenRGB is running but not answering, with a one-click restart.
- Tighter keycap glow, so light no longer bleeds into neighbouring keys.

## 0.1.0 — 2026-10-12

First public build.

- One canvas for every device OpenRGB reports, drawn to scale: drag, zoom and pan.
- Layered effects (static, gradient, spectrum, wave, breathing, starlight) with
  opacity, blend modes and per-device masks, streamed at 30 or 60 fps.
- Profiles, and the current scene restored on start.
- Battery levels for Razer and HyperX wireless devices on the Devices home, in the top
  bar and in the tray tooltip.
- Boot lighting written to the onboard memory of Razer devices.
- Tray app: closing the window keeps lighting running; optional start at login.
- Installers for Windows, macOS (Apple Silicon and Intel) and Linux (deb, rpm,
  AppImage), each bundling OpenRGB 1.0.
