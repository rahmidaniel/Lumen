# How Lumen runs OpenRGB

Lumen does device I/O through [OpenRGB](https://openrgb.org)'s SDK server (protocol v6
over TCP, `127.0.0.1:6742`). This page covers how the OpenRGB it talks to gets onto the
machine, how Lumen finds it, and how the two lifecycles are tied together.

## Bundling

The packages are self-contained: `scripts/fetch-openrgb.mjs` (run automatically by the
build) downloads the official OpenRGB 1.0 asset for the target platform and
architecture, verifies its SHA-256 and places it under `src-tauri/resources/openrgb/`,
which the bundler ships as `openrgb/` next to the app:

| platform | bundled form | how Lumen runs it |
|---|---|---|
| Windows | portable `OpenRGB.exe` | directly |
| macOS | the official `OpenRGB.zip` | unpacked once into Lumen's data dir with `ditto` (the `.app` holds framework symlinks the bundler would flatten) |
| Linux | `OpenRGB.AppImage` + `60-openrgb.rules` | AppImage with `APPIMAGE_EXTRACT_AND_RUN=1` (no FUSE needed); deb/rpm install the udev rules and reload udev |

The assets come from the `openrgb-1.0` pre-release of this repository (a byte-for-byte
mirror of the official files), with Codeberg as the fallback; Codeberg has refused
automated downloads before. OpenRGB is GPL-2.0-or-later and is bundled unmodified with
its licence text — see [`THIRD_PARTY_NOTICES.md`](../THIRD_PARTY_NOTICES.md).

## Finding an OpenRGB

At runtime Lumen first attaches to an OpenRGB that is already listening on 6742 (e.g.
the service OpenRGB's own installer registers), otherwise starts the bundled copy as a
silent, window-less SDK server (`--server --server-host 127.0.0.1 --noautoconnect`) and
stops it again on exit; if that fails it falls back to a system-wide install.
`LUMEN_OPENRGB_PATH` overrides the executable.

## Lifecycle

Lumen prefers to own the OpenRGB process it talks to: started with Lumen, restarted if
it stops answering (after ten seconds, at most every two minutes), stopped when Lumen
quits. If an earlier Lumen was killed and left its OpenRGB running, the next Lumen
recognises the executable and takes the process over. On macOS and Linux the bundled
copy is simply a child process.

## Managed mode on Windows

On Windows the OpenRGB installer's service gets in the way: it runs as SYSTEM outside
Lumen's control (it needs that elevation for I2C devices such as graphics cards and
motherboards), it outlives Lumen, and it can wedge. Settings → OpenRGB → *Let Lumen
manage OpenRGB* fixes that with a single admin prompt
(`src-tauri/packaging/windows/openrgb-adopt.ps1`): the service is stopped and disabled,
and a Task Scheduler task `LumenOpenRGB` that runs the bundled OpenRGB with highest
privileges is registered for your account. From then on Lumen starts, restarts and stops
that task without prompts, and OpenRGB keeps its elevated device access.

If the task still points at another copy of OpenRGB (an earlier install or a development
build), Settings says so and offers *Update the OpenRGB task*. *Hand back to the service*
(`openrgb-release.ps1`) undoes the whole thing. Until you opt in, Lumen attaches to the
service and can only restart it through an admin prompt.
