# Third-party notices

## OpenRGB

Lumen's installers ship an unmodified copy of **OpenRGB 1.0** (https://openrgb.org),
copyright Adam Honse and the OpenRGB contributors, licensed under the
**GNU General Public License, version 2 or later**. The bundled builds are the
official release assets from https://codeberg.org/OpenRGB/OpenRGB/releases/tag/release_1.0
(commit `81bbe18`), fetched and SHA-256-verified by `scripts/fetch-openrgb.mjs`:

| Lumen package | OpenRGB asset | shipped as |
|---|---|---|
| Windows installer | `OpenRGB_1.0_Windows_64_81bbe18.zip` | `openrgb/OpenRGB.exe` + portable files |
| macOS (Apple Silicon / Intel) | `OpenRGB_1.0_MacOS_ARM64_81bbe18.zip` / `OpenRGB_1.0_MacOS_Intel_81bbe18.zip` | `openrgb/OpenRGB.zip`, unpacked on first run |
| Linux deb / rpm / AppImage | `OpenRGB_1.0_x86_64_81bbe18.AppImage` | `openrgb/OpenRGB.AppImage` |
| Linux deb / rpm | `60-openrgb.rules` from `openrgb_1.0_amd64_bookworm_81bbe18.deb` | `/usr/lib/udev/rules.d/60-openrgb.rules` |

The source code for exactly that version is at
https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_1.0. The licence text is
installed alongside the binary (`openrgb/LICENSE`).

OpenRGB runs as a separate process and Lumen communicates with it only over its
TCP SDK protocol. Lumen itself is distributed under its own licence (see
`LICENSE`); the two are aggregated in one installer, not combined into one
program.

## hidapi

The `hidapi` crate statically links the HIDAPI C library (https://github.com/libusb/hidapi),
available under the BSD-style HIDAPI licence; Lumen uses it under those terms.

## Other dependencies

Rust and JavaScript dependencies are MIT/Apache-2.0/ISC licensed; their
notices can be regenerated with `cargo about` / `npx license-checker`.
