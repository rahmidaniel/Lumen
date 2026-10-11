# Third-party notices

## OpenRGB

Lumen's installer ships an unmodified copy of **OpenRGB** (https://openrgb.org),
copyright Adam Honse and the OpenRGB contributors, licensed under the
**GNU General Public License, version 2 or later**. The bundled build is
`OpenRGB_1.0_Windows_64_81bbe18` from
https://codeberg.org/OpenRGB/OpenRGB/releases/tag/release_1.0; its source code
for exactly that version is at
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
