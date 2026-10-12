# Lumen

Cross-platform RGB lighting studio built on OpenRGB. One canvas, every device, in sync.

- **OpenRGB** does device I/O (headless, SDK protocol v6).
- **Lumen** does the rest: a Chroma-Studio-style virtual canvas, layered effects, per-key painting, profiles,
  battery status, save-to-device where hardware allows, and a UI that stays smooth.

See [`docs/PLAN.md`](docs/PLAN.md) for architecture and milestones, and
[`docs/protocols.md`](docs/protocols.md) for the wire protocols Lumen speaks.

Stack: Tauri 2 · Rust · React 19 · TypeScript · Tailwind.

## Installing

Grab the installer for your platform from the
[releases page](../../releases). Every package bundles OpenRGB 1.0, so nothing
else needs to be installed. The builds are not code-signed yet, hence the
one-time warnings below.

| Platform | Package | Notes |
|---|---|---|
| Windows 10/11 x64 | `Lumen_<version>_x64-setup.exe` | Per-user install. SmartScreen will warn about an unknown publisher: *More info → Run anyway*. If OpenRGB's own installer is present, Lumen attaches to its service instead of starting the bundled copy. |
| macOS 11+ | `Lumen_<version>_aarch64.dmg` (Apple Silicon) / `Lumen_<version>_x64.dmg` (Intel) | Drag to Applications, then run `xattr -cr /Applications/Lumen.app` once (Gatekeeper blocks unsigned apps). Keyboards need *Input Monitoring* permission for OpenRGB; grant it when macOS asks. |
| Linux x64 | `.deb` (Debian/Ubuntu), `.rpm` (Fedora/openSUSE), `.AppImage` | deb/rpm install OpenRGB's udev rules so devices work without root. The AppImage cannot; install `openrgb` from your distro (its udev rules suffice) or copy OpenRGB's `60-openrgb.rules` to `/etc/udev/rules.d/`. |

Lumen lives in the tray: close the window and lighting keeps running; "Start at
login" in Settings launches it hidden.

## Developing

Prerequisites: [Rust](https://rustup.rs) stable, Node 24+, and the platform
build tools:

- **Windows**: Visual Studio 2022 Build Tools with the C++ workload (Rust's MSVC
  linker). `scripts/setup-windows.ps1` installs them plus OpenRGB — run it from an
  elevated PowerShell: `powershell -ExecutionPolicy Bypass -File scripts\setup-windows.ps1`.
- **Linux** (Debian/Ubuntu): `sudo apt install libwebkit2gtk-4.1-dev
  libayatana-appindicator3-dev librsvg2-dev patchelf libxdo-dev libssl-dev
  libudev-dev` (the last one is for hidapi).
- **macOS**: Xcode command line tools (`xcode-select --install`).

Then:

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

Layout:

```
crates/openrgb-sdk          OpenRGB SDK protocol v6 client
crates/lumen-core           canvas, effect layers, renderer, profiles, engine
crates/lumen-vendor-razer   Razer HID sidecar: battery, onboard (boot) effects
crates/lumen-vendor-hyperx  HyperX HID sidecar: battery
crates/lumen-vendor-hid     Logitech, Glorious, ASUS ROG, SteelSeries, Corsair sidecars: battery (from published protocols, untested on hardware)
src-tauri                   Tauri shell: commands, events, tray, OpenRGB process management
src                         React UI
scripts                     fetch-openrgb.mjs (bundling), setup-windows.ps1, measure-footprint.ps1
.github/workflows           ci.yml (tests on 3 OSes), release.yml (installers + draft release)
```

## Packaging

```sh
npm run tauri build
```

builds the installer for the current platform (`src-tauri/target/release/bundle/`):
NSIS on Windows, `.app` + `.dmg` on macOS, `.deb` + `.rpm` + `.AppImage` on Linux.
The platform-specific parts of the Tauri config live in
`src-tauri/tauri.{windows,macos,linux}.conf.json`.

The packages are self-contained: `scripts/fetch-openrgb.mjs` (run automatically by
the build) downloads the official OpenRGB 1.0 asset for the target platform and
architecture, verifies its SHA-256 and places it under `src-tauri/resources/openrgb/`,
which the bundler ships as `openrgb/` next to the app:

| platform | bundled form | how Lumen runs it |
|---|---|---|
| Windows | portable `OpenRGB.exe` | directly |
| macOS | the official `OpenRGB.zip` | unpacked once into Lumen's data dir with `ditto` (the `.app` holds framework symlinks the bundler would flatten) |
| Linux | `OpenRGB.AppImage` + `60-openrgb.rules` | AppImage with `APPIMAGE_EXTRACT_AND_RUN=1` (no FUSE needed); deb/rpm install the udev rules and reload udev |

At runtime Lumen first attaches to an OpenRGB that is already listening on 6742
(e.g. the service OpenRGB's own installer registers), otherwise starts the bundled
copy as a silent, window-less SDK server (`--server --noautoconnect`) and stops it
again on exit; if that fails it falls back to a system-wide install.
`LUMEN_OPENRGB_PATH` overrides the executable. OpenRGB is GPL-2.0-or-later and is
bundled unmodified with its licence text — see `THIRD_PARTY_NOTICES.md`.

Footprint: the window is disposable (closing it destroys the WebView while lighting
continues from the tray), preview frames exist only while it is visible and are capped at
25 fps (8 fps at reduced resolution when the window is not focused), static scenes render
once, and the render rate is selectable (30/60 fps). "Start at login" launches hidden
via `--hidden`. Measured with `scripts/measure-footprint.ps1` on the Windows release
build while streaming a 60 fps effect to four devices (2026-10-11):

| state | processes | CPU (one core) | working set | private |
|---|---|---|---|---|
| closed to tray (engine only) | 1 | ~2 % | 30 MB | 5 MB |
| window open | 7 (Lumen + WebView2) | ~15 % | ~400 MB | ~210 MB |

The open-window cost is Chromium presenting the live canvas (GPU process + browser), not
the effect engine; it disappears entirely when the window is closed.

## Updates

Lumen checks GitHub Releases for newer versions: on demand from Settings → Updates, and
once a day in the background (switchable off there). Nothing is installed until you press
*Install and restart*. The release workflow signs every installer with the project's updater
key and attaches a `latest.json` manifest to the release; the app verifies the signature
against the public key in `src-tauri/tauri.conf.json` before installing, then restarts.

The private key lives outside the repository: `~/.tauri/lumen.key` on the maintainer's
machine (a local `tauri build` reads it through `TAURI_SIGNING_PRIVATE_KEY`) and the
`TAURI_SIGNING_PRIVATE_KEY` secret in GitHub Actions. Losing it means existing installs can
no longer verify updates, so keep a backup. The feed URL
`https://github.com/rahmidaniel/lumen/releases/latest/download/latest.json` is only served
by GitHub for public repositories, so the repository has to be public for updates to reach
users.

## OpenRGB lifecycle

Lumen prefers to own the OpenRGB process it talks to: started with Lumen, restarted if
it stops answering (after ten seconds, at most every two minutes), stopped when Lumen
quits. On macOS and Linux the bundled copy is simply a child process. On Windows the
OpenRGB installer's service gets in the way: it runs as SYSTEM outside Lumen's control
(it needs that elevation for I2C devices such as graphics cards and motherboards), it
outlives Lumen, and it can wedge. Settings → OpenRGB → *Let Lumen manage OpenRGB* fixes
that with a single admin prompt (`src-tauri/packaging/windows/openrgb-adopt.ps1`): the
service is stopped and disabled, and a Task Scheduler task `LumenOpenRGB` that runs the
bundled OpenRGB with highest privileges is registered for your account. From then on
Lumen starts, restarts and stops that task without prompts, and OpenRGB keeps its
elevated device access. *Hand back to the service* undoes it. Until then, Lumen attaches
to the service and can only restart it through an admin prompt.

## Releases

GitHub Actions does the cross-platform builds (`.github/workflows/release.yml`):

1. Bump the version in `src-tauri/tauri.conf.json`, `package.json` and the workspace
   `Cargo.toml`, commit.
2. Tag and push: `git tag v0.1.0 && git push origin main v0.1.0`.
3. The workflow builds Windows (NSIS), macOS (Apple Silicon and Intel DMGs) and Linux
   (deb, rpm, AppImage) in parallel and attaches them to a **draft** release named after
   the tag. Review it on GitHub and publish.

`ci.yml` runs `npm run build` and `cargo test --workspace` on all three OSes for every
push and pull request.

Code signing is not set up. For macOS, add the `APPLE_*` secrets that
[tauri-action](https://github.com/tauri-apps/tauri-action) documents and pass them
through the workflow's `env:`; for Windows, a code-signing certificate can be wired into
`bundle.windows` in the Tauri config. Until then users see the warnings listed under
*Installing*.
