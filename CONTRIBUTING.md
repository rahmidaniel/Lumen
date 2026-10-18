# Contributing to Lumen

Thanks for helping. There are three useful ways in, and only one of them needs a
toolchain.

## Report how your hardware behaves

Lumen talks to a lot of devices it has never been tested with. If you own a Logitech,
Corsair, SteelSeries, ASUS ROG or Glorious wireless device, or run Lumen on macOS or
Linux, a [device report](https://github.com/rahmidaniel/lumen/issues/new?template=device_report.yml)
saying what works and what doesn't is the single most valuable contribution right now.
Battery readings are written from published protocols and need real hardware to confirm.

If a device does not show up at all, check first whether OpenRGB supports it
([device list](https://openrgb.org/devices.html)): Lumen can only light what OpenRGB
can reach.

## Report bugs and ideas

Use the [issue forms](https://github.com/rahmidaniel/lumen/issues/new/choose). For bugs,
the Lumen version (Settings → Updates), your OS and the devices involved make the
difference between a quick fix and a guessing game.

Security problems go through a private report instead; see [SECURITY.md](SECURITY.md).

## Work on the code

### Prerequisites

[Rust](https://rustup.rs) stable, Node 24+, and the platform build tools:

- **Windows**: Visual Studio 2022 Build Tools with the C++ workload (Rust's MSVC
  linker). `scripts/setup-windows.ps1` installs them plus OpenRGB — run it from an
  elevated PowerShell: `powershell -ExecutionPolicy Bypass -File scripts\setup-windows.ps1`.
- **Linux** (Debian/Ubuntu): `sudo apt install libwebkit2gtk-4.1-dev
  libayatana-appindicator3-dev librsvg2-dev patchelf libxdo-dev libssl-dev
  libudev-dev` (the last one is for hidapi).
- **macOS**: Xcode command line tools (`xcode-select --install`).

### Running

```sh
npm install
cargo test --workspace        # protocol, canvas, effects, profile, sidecar tests
npm run tauri dev             # starts Vite + the Tauri shell; attaches to OpenRGB's SDK server
                              # (OpenRGB 1.0 installs a Windows service that is already listening
                              # on 6742; otherwise Lumen launches OpenRGB headless itself)
npm run dev                   # UI only, in a browser, against an in-memory mock backend
```

`npm run tauri dev` needs an OpenRGB to talk to: either install OpenRGB, or run
`npm run fetch:openrgb` once and point Lumen at it with
`LUMEN_OPENRGB_PATH=src-tauri/resources/openrgb/OpenRGB.exe` (Windows; on Linux
the AppImage, on macOS the unpacked `OpenRGB.app/Contents/MacOS/OpenRGB`).

Only one copy of Lumen runs at a time, and a dev build counts: with an installed
Lumen (0.3.2 or later) running, a dev build just brings up that copy's window and
exits. Quit the installed one first (tray → *Quit Lumen*).

No RGB hardware needed for UI work: `npm run dev` serves the real UI against
`src/lib/mock.ts`, which fakes a keyboard, mat, mouse and graphics card and renders the
layer stack with the same maths as the Rust engine. URL switches show the less common
states: `?layout=iso` (ISO keyboard), `?nodevices` (the empty states), `?conflicts` (the
vendor-app warning) and `?legacy` (the Lumen 0.2 task warning).

Lumen logs to `%LOCALAPPDATA%\dev.lumen.studio\logs` (Windows), `~/Library/Logs/dev.lumen.studio`
(macOS) or `~/.local/share/dev.lumen.studio/logs` (Linux), one file a day for a week;
`RUST_LOG` changes the level. Settings → Help → *Copy diagnostics* bundles the essentials
for an issue.

### Layout

```
crates/openrgb-sdk          OpenRGB SDK protocol v6 client
crates/lumen-core           canvas, effect layers, renderer, profiles, engine
crates/lumen-vendor-razer   Razer HID sidecar: battery, onboard (boot) effects
crates/lumen-vendor-hyperx  HyperX HID sidecar: battery
crates/lumen-vendor-hid     Logitech, Glorious, ASUS ROG, SteelSeries, Corsair sidecars: battery (from published protocols, untested on hardware)
src-tauri                   Tauri shell: commands, events, tray, OpenRGB process management
src                         React UI
scripts                     bundling, release, screenshot and measurement helpers
.github/workflows           ci.yml (tests on 3 OSes), release.yml (installers + draft release)
```

[`docs/PLAN.md`](docs/PLAN.md) has the architecture and milestones,
[`docs/protocols.md`](docs/protocols.md) the vendor wire protocols,
[`docs/openrgb.md`](docs/openrgb.md) how OpenRGB is bundled and supervised, and
[`docs/releasing.md`](docs/releasing.md) the release process.

### Adding a battery sidecar

Vendor protocols live in `crates/lumen-vendor-hid` (one module per vendor, read-only HID
requests, parsers unit-tested with example reports) and are wired up as a `Target`
in `src-tauri/src/sidecar.rs`. Document the request/response bytes and where they come
from in `docs/protocols.md`. Never write to a device from a battery sidecar.

### Packaging

```sh
npm run tauri build
```

builds the installer for the current platform (`src-tauri/target/release/bundle/`):
NSIS on Windows, `.app` + `.dmg` on macOS, `.deb` + `.rpm` + `.AppImage` on Linux.
The platform-specific parts of the Tauri config live in
`src-tauri/tauri.{windows,macos,linux}.conf.json`. The build fetches the bundled
OpenRGB itself; see [`docs/openrgb.md`](docs/openrgb.md).

### Footprint

The window is disposable (closing it destroys the WebView while lighting continues from
the tray), preview frames exist only while it is visible and are capped at 25 fps (8 fps
at reduced resolution when the window is not focused), static scenes render once, and
the render rate is selectable (30/60 fps). "Start at login" launches hidden via
`--hidden`. Measured with `scripts/measure-footprint.ps1` on the Windows release build
while streaming a 60 fps effect to four devices (2026-10-11):

| state | processes | CPU (one core) | working set | private |
|---|---|---|---|---|
| closed to tray (engine only) | 1 | ~2 % | 30 MB | 5 MB |
| window open | 7 (Lumen + WebView2) | ~15 % | ~400 MB | ~210 MB |

The open-window cost is Chromium presenting the live canvas (GPU process + browser), not
the effect engine; it disappears entirely when the window is closed. Changes to the
engine or the preview path should keep the tray number where it is.

### Screenshots and the demo animation

- `scripts/screenshots.mjs` captures `docs/screenshots/*.png` from a running Lumen through
  its WebView2's DevTools port: start Lumen (or `npm run tauri dev`) with
  `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333` set, then run it.
  It clicks through the UI on its own and puts back whatever it changes.
- `scripts/demo-capture.mjs` records `docs/screenshots/demo.webp`, the loop at the top of
  the README, from `npm run dev` in headless Edge or Chrome (`npm i --no-save sharp`
  first).

### Pull requests

- Keep them focused; one change per PR is easier to review and to revert.
- `cargo test --workspace` and `npm run build` must pass (CI runs both on Windows, macOS
  and Linux).
- Say how you tested, and on which hardware. For UI changes, add a screenshot.
- Match the surrounding code: its naming, its idioms, and comments that explain *why*
  rather than *what*. TypeScript runs in strict mode.

By contributing you agree that your work is released under the project's
[MIT licence](LICENSE).
