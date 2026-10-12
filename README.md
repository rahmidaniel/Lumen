# Lumen

**One app for all your RGB gear, whatever the brand.**

Razer has Synapse, Logitech has G HUB, Corsair has iCUE, SteelSeries has GG, ASUS has
Armoury Crate, HyperX has NGENUITY, Glorious has CORE. Each one is a separate download,
a separate background service, and a separate idea of what "sync" means. Lumen replaces
the lot with a single small, cross-platform app: one canvas where every device lights up
together, and one place to see battery levels and manage devices.

## What it does

- **Unified lighting.** Every device sits on one virtual desk, drawn to scale: keyboards on
  their real ANSI or ISO layout, mats as edge-lit loops, mice, graphics cards. A layer stack
  of effects (static, gradient, spectrum, wave, breathing, starlight) runs across all of them
  as one motion, in the spirit of Razer Chroma Studio, but for every brand at once.
- **Per-key painting.** A paint layer colours individual keys and LEDs on top of any effect.
- **Battery at a glance.** Wireless mice and keyboards show their charge on the Devices home,
  in the top bar and in the tray tooltip.
- **Boot lighting.** Where the hardware has onboard memory (Razer so far), Lumen writes the
  effect the device shows before any software has started.
- **Profiles** for saving and switching whole scenes.
- **Light footprint.** Lives in the tray; closing the window frees the UI entirely while
  lighting keeps running. Starts with your OS if you want it to.
- **Self-contained and self-updating.** Installers bundle everything needed, and Lumen
  updates itself from GitHub Releases.

## Supported devices

Lighting goes through [OpenRGB](https://openrgb.org), which Lumen bundles and manages for
you, so any device OpenRGB supports works: thousands of keyboards, mice, mats, headsets,
graphics cards, motherboards, RAM, fans and LED strips across Razer, Logitech, Corsair,
SteelSeries, ASUS, MSI, Gigabyte, HyperX, NZXT, Cooler Master and many more. See OpenRGB's
[supported devices list](https://openrgb.org/devices.html).

Things OpenRGB does not do, Lumen does itself, per vendor:

| Brand | Battery | Onboard boot lighting | Status |
|---|---|---|---|
| Razer | ✅ | ✅ | tested on hardware |
| HyperX | ✅ | – | tested on hardware |
| Logitech (HID++, Lightspeed/Unifying) | ✅ | – | from published protocol, untested |
| Corsair (Bragi and legacy wireless) | ✅ | – | from published protocol, untested |
| SteelSeries (Aerox, Prime, Rival) | ✅ | – | from published protocol, untested |
| ASUS ROG wireless mice | ✅ | – | from published protocol, untested |
| Glorious (CORE wireless) | ✅ | – | from published protocol, untested |

If you own one of the untested devices, a report of what Lumen shows is very welcome.

## How it works

- **OpenRGB** does device I/O (headless SDK server, protocol v6). Lumen ships it, starts it,
  restarts it if it hangs, and stops it on exit.
- **Lumen** does everything else: the canvas, the effect engine (60 fps in Rust), profiles,
  vendor battery and onboard-memory sidecars, and the UI.

See [`docs/PLAN.md`](docs/PLAN.md) for architecture and milestones, and
[`docs/protocols.md`](docs/protocols.md) for the wire protocols Lumen speaks.

Stack: Tauri 2 · Rust · React 19 · TypeScript · Tailwind.

## Screenshots

**Devices home.** Every device OpenRGB reports, drawn to scale on one desk: the keyboard
on its real ANSI/ISO layout, the mat as an edge-lit loop, the mouse, the graphics card.
Battery gauges (with cell voltage where the mouse reports it) sit next to the devices that
have one, and selecting a Razer device offers its onboard boot lighting.

![Devices home: keyboard, mat, mouse and GPU on the canvas, battery gauges in the panel](docs/screenshots/devices.png)

**Effects.** A layer stack: static colours, gradients, spectrum cycles, waves, breathing
and starlight, each with opacity, blend mode and the devices it applies to. One wave
travels across the mat, keyboard, mouse and GPU as a single motion.

![Effects panel with a spectrum-cycle layer running across all devices](docs/screenshots/effects.png)

**Paint keys.** A paint layer colours individual keys and LEDs: click or drag across the
canvas, Shift to erase. Keys you leave alone keep showing the layers below.

![Paint mode: a few keys painted white on top of the running effect](docs/screenshots/paint.png)

Captured from the Windows build with `scripts/screenshots.ps1`.

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
quits. If an earlier Lumen was killed and left its OpenRGB running, the next Lumen
recognises the executable and takes the process over. On macOS and Linux the bundled
copy is simply a child process. On Windows the
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
   (deb, rpm, AppImage) in parallel, signs them for the updater, and attaches them plus
   the `latest.json` update manifest to a **draft** release named after the tag. Review
   it on GitHub and publish; the updater only sees published, non-pre-release releases.
4. If two jobs finish at the same moment, one can fail while updating the shared
   `latest.json` ("Not Found - update-a-release-asset") and the manifest may miss
   platforms. Re-run the failed jobs (`gh run rerun <run id> --failed`), then rebuild the
   manifest from the signed assets with `node scripts/release-manifest.mjs vX.Y.Z` before
   publishing.

The OpenRGB build assets come from the `openrgb-1.0` pre-release of this repository (a
byte-for-byte mirror of the official files, SHA-256-pinned in `scripts/fetch-openrgb.mjs`),
with Codeberg as the fallback; Codeberg has refused automated downloads before.

`ci.yml` runs `npm run build` and `cargo test --workspace` on all three OSes for every
push and pull request.

Code signing is not set up. For macOS, add the `APPLE_*` secrets that
[tauri-action](https://github.com/tauri-apps/tauri-action) documents and pass them
through the workflow's `env:`; for Windows, a code-signing certificate can be wired into
`bundle.windows` in the Tauri config. Until then users see the warnings listed under
*Installing*.
