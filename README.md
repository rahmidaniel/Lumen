# Lumen

Cross-platform RGB lighting studio built on OpenRGB. One canvas, every device, in sync.

- **OpenRGB** does device I/O (headless, SDK protocol v6).
- **Lumen** does the rest: a Chroma-Studio-style virtual canvas, layered effects, profiles,
  battery status, save-to-device where hardware allows, and a UI that stays smooth.

See [`docs/PLAN.md`](docs/PLAN.md) for architecture and milestones, and
[`docs/protocols.md`](docs/protocols.md) for the wire protocols Lumen speaks.

Stack: Tauri 2 · Rust · React 19 · TypeScript · Tailwind.

## Developing

Prerequisites: [Rust](https://rustup.rs) (MSVC toolchain), Node 24+, and
OpenRGB 1.0. On Windows the MSVC linker comes from the Visual Studio 2022
Build Tools (C++ workload); `scripts/setup-windows.ps1` installs OpenRGB and the
Build Tools — run it from an elevated PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\setup-windows.ps1
```

Then:

```sh
npm install
cargo test --workspace        # protocol, canvas, effects, profile tests
npm run tauri dev             # starts Vite + the Tauri shell; attaches to OpenRGB's SDK server
                              # (OpenRGB 1.0 installs a Windows service that is already listening
                              # on 6742; otherwise Lumen launches OpenRGB headless itself)
npm run dev                   # UI only, in a browser, against an in-memory mock backend
```

Layout:

```
crates/openrgb-sdk          OpenRGB SDK protocol v6 client
crates/lumen-core           canvas, effect layers, renderer, profiles, engine
crates/lumen-vendor-razer   Razer HID sidecar: battery, onboard (boot) effects
crates/lumen-vendor-hyperx  HyperX HID sidecar: battery
src-tauri                   Tauri shell: commands, events, tray, OpenRGB process management
src                         React UI
```

## Packaging

```sh
npm run tauri build           # NSIS installer in src-tauri/target/release/bundle/nsis/
```

The installer is self-contained: `scripts/fetch-openrgb.ps1` (run automatically by the
build) downloads the official OpenRGB 1.0 portable build, verifies its SHA-256 and ships
it inside the installer under `openrgb/`. At runtime Lumen attaches to an OpenRGB that is
already running (e.g. the service OpenRGB's own installer registers) and otherwise starts
the bundled copy as a silent, window-less SDK server, stopping it again on exit.
OpenRGB is GPL-2.0-or-later and is bundled unmodified with its licence text —
see `THIRD_PARTY_NOTICES.md`.

Footprint: the window is disposable (closing it destroys the WebView while lighting
continues from the tray), preview frames exist only while it is visible, static scenes
render once, and the render rate is selectable (30/60 fps). "Start with Windows" launches
hidden via `--hidden`.
