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
crates/openrgb-sdk     OpenRGB SDK protocol v6 client
crates/lumen-core      canvas, effect layers, renderer, profiles, engine
src-tauri              Tauri shell: commands, events, tray, OpenRGB process management
src                    React UI
```
