# Changelog

All notable changes to Lumen. Installers are on the
[releases page](https://github.com/rahmidaniel/lumen/releases); from 0.2.1 on, Lumen
updates itself from Settings → Updates.

## Unreleased

- New app icon, replacing the Tauri placeholder (installer, taskbar, tray, Start menu).

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
