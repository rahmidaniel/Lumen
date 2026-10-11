# Lumen — plan

## Context

Razer Synapse is the only thing that lights these devices the way you want, it needs to be
running, and it is heavy. OpenRGB can drive every device in the room but its Qt UI is clunky,
it can't sync devices into one scene the way Razer Chroma Studio does, and it can't save
effects to device memory for most hardware. Aurora and the other alternatives were worse.

Lumen is a new desktop app built **on OpenRGB's bones**: OpenRGB (headless) does device I/O,
Lumen does everything above it — a Chroma-Studio-style virtual canvas that syncs every device
into one scene, a fast, well-designed UI, battery status, and "save boot effect to device"
where the hardware allows it. Cross-platform (Windows first; macOS/Linux builds follow).

Hardware on the dev machine (from OpenRGB 1.0's own detection log, 2026-10-09):

| Device | USB ID | Notes |
|---|---|---|
| Razer Strider Chroma (mouse mat) | 1532:0C05 | 19 LEDs; saves built-in effects via VARSTORE when Synapse is closed |
| Razer DeathStalker V2 Pro TKL | 1532:0296 / 0298 | per-key matrix; wireless → battery |
| HyperX Pulsefire Haste 2 Wireless | 03F0:0F98 | wireless → battery |
| ASUS TUF RTX 3080 | NvAPI I2C | OpenRGB only |
| Gigabyte motherboard (RGB Fusion 2 SMBus) | — | needs PawnIO + admin; optional |

## Principles

1. **OpenRGB is the only LED writer.** Lumen never streams LED frames over USB itself, so two
   programs never fight over a device.
2. **Vendor sidecars only for what the SDK lacks** — battery, save-to-flash, device mode. They
   send short, rare, reply-matched transactions and pause the engine while they run.
3. **Engine in Rust, UI in web.** The render loop never runs in the webview.
4. **Headless-first.** Lumen lives in the tray; the window is optional.

## Architecture

```
React/TS UI  ⇄  Tauri IPC (commands + events)  ⇄  src-tauri (app shell, tray, autostart)
                                                     │
                                                     ├─ lumen-core        canvas, effect layers, 60 fps renderer, profiles
                                                     │     └─ openrgb-sdk TCP :6742, protocol v6  →  OpenRGB --server  →  devices
                                                     └─ lumen-vendor-*    hidapi sidecars: battery %, charging, VARSTORE, device mode
```

### Repo layout

```
lumen/
  Cargo.toml              Cargo workspace: src-tauri + crates/*
  package.json            pnpm · Vite · React 19 · TypeScript · Tailwind
  src/                    UI — app/ (shell, routing), features/{devices,canvas,effects,profiles,settings},
                          components/ui (design system), lib/ipc.ts (typed command/event wrappers)
  src-tauri/              Tauri 2 shell: commands, events, tray, autostart, OpenRGB process management
  crates/
    openrgb-sdk/          protocol v6 client: framing, device-description parser, UpdateLEDs, SaveMode,
                          profile/settings packets, DEVICE_LIST_UPDATED notifications, reconnect
    lumen-core/           device registry, canvas model, effect layers, scheduler/renderer, profile serde
    lumen-vendor-razer/   Razer HID: battery level (0x07/0x80), charging (0x07/0x84), VARSTORE effects, mode
    lumen-vendor-hyperx/  Pulsefire Haste 2 Wireless battery (M3)
  docs/
```

### Key design

- **Device model** — `Device { id (OpenRGB stable id), name, kind, zones[{kind, leds, matrix map, segments}],
  modes, flags, extras { battery: Option<Battery>, persistence: Capability } }`. Zone kinds: single,
  linear, matrix (keyboards ship an LED-index grid).
- **Canvas** — one normalized 2D scene. Each device has a placement (position, rotation, scale);
  every LED resolves to a canvas coordinate: matrix zones from the matrix map, linear zones along a
  line, singles as a point. Sensible auto-layout by device kind (keyboard centre, mouse right,
  mat underneath, GPU/motherboard behind) that the user then drags into place.
- **Effects** — a layer stack. Layer = effect (`(x, y, t, params) → rgba`) + target mask
  (all / devices / zones) + blend mode (normal, add, screen, multiply) + opacity. v1 effects: Static,
  Gradient, Spectrum cycle, Wave (angle/speed), Breathing, Starlight. Later: Ripple on keypress,
  Audio visualizer, Ambient (screen colour).
- **Render loop** — fixed timestep (60 Hz default, 30 Hz option). Compute per-LED colours, diff
  against the last frame per device, send `UpdateLEDs` only to devices that changed, per-device rate
  caps for hardware that chokes above 30 Hz, back-pressure on the socket.
- **Profiles** — JSON in `%APPDATA%/Lumen/profiles/` (placements, layers, device settings). One is
  the startup profile. Event-bound profiles (suspend/resume, game running) later.
- **Save to device** — per-device capability: (a) OpenRGB mode has `MANUAL_SAVE` →
  `SaveMode` (packet 1102); (b) Razer sidecar writes a built-in effect with VARSTORE (static / breath /
  spectrum / wave) after pausing the engine — proven on the Strider with `razer-strider/strider.ps1`;
  (c) neither → the UI says so plainly.
- **Battery** — sidecar polls on connect and every 60 s. Razer: class 0x07, cmd 0x80 (level 0–255,
  response arg[1]) and cmd 0x84 (charging 0/1). Shown as a badge in the device list and the tray
  tooltip; low-battery notification.
- **OpenRGB management** — locate the install (registry, known paths, PATH); start
  `OpenRGB.exe --server --noautoconnect --startminimized` or attach to a running server/service;
  reconnect with backoff; surface status in the UI. Detect Razer Synapse processes and warn, since
  they fight over the same HID reports. Bundling OpenRGB in the installer is a later option
  (GPLv2 — Lumen talks to it over TCP as a separate program, so Lumen itself can be MIT/Apache).
- **IPC** — commands: `list_devices`, `set_placement`, `set_layers`, `save_profile`, `load_profile`,
  `save_to_device`, `openrgb_status`…; events: `devices_changed`, `preview_frame` (≤30 fps,
  typed-array payload), `battery_changed`, `openrgb_status`.
- **UI** — dark-first design system; left rail (Devices · Canvas · Effects · Profiles · Settings),
  centre stage with live LED preview on a Canvas2D surface, right-hand inspector. Transitions with
  `motion`; preview rendering via `requestAnimationFrame` on typed arrays so there is no GC jank.

## Milestones

| # | Milestone | Done when |
|---|---|---|
| M0 | Toolchain + scaffold | ✅ 2026-10-11 — Rust/Node/Build Tools installed; `npm run tauri dev` opens the Lumen window; all crates compile, 43 tests pass |
| M1 | OpenRGB bridge + live preview | ✅ 2026-10-11 — attaches to the OpenRGB 1.0 service (protocol v6), 4 devices parsed, 60 fps wave across mat/keyboard/mouse/GPU, live preview with keycap/strip geometry |
| M2 | Canvas, effects, profiles | ✅ (v1) — drag/zoom/pan canvas, six effects, blend modes, masks, profiles, scene autosave. Still to do: ripple/audio/ambient effects, LED-order editor for strips |
| M3 | Device extras | 🟡 — DeathStalker battery via Razer sidecar (0x9F, stream hold) ✅; Strider "save to device" UI wired (VARSTORE) — verify by replug; HyperX battery sidecar written, verifying; no device exposes OpenRGB `MANUAL_SAVE`, so that path is moot; Synapse-conflict warning pending |
| M4 | Ship | Tray + close-to-tray ✅; autostart, hotplug polish, settings, MSI/NSIS installer, release-profile CPU check, macOS/Linux CI pending |

## Verification

- `openrgb-sdk`: round-trip tests against packet fixtures captured from OpenRGB 1.0 (device
  descriptions for all four devices, UpdateLEDs, SaveMode).
- `lumen-core`: canvas sampling and effect determinism tests (same `t` → same frame).
- Integration (manual, each milestone): OpenRGB headless + Lumen on this PC — all devices listed,
  a wave visibly travels across mat → keyboard → mouse in one motion, CPU < 2 % idle and < 5 %
  while streaming, no dropped frames in the preview.
- Battery values compared against the devices' own indicators; save-to-device verified by
  unplug/replug with Synapse and Lumen both closed.

## Risks

- Razer Synapse contention → process detection + warning; later an "exclusive mode" toggle.
- Per-device quirks (frame-rate limits, mode switching needed before per-LED control) → per-device
  profile table in `lumen-core`.
- OpenRGB protocol/version drift → negotiate version on connect; pin fixtures to 1.0.
