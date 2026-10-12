#!/bin/sh
# Lumen's deb/rpm install OpenRGB's udev rules (60-openrgb.rules) so the bundled
# OpenRGB can reach USB/HID devices without root. Reload them so devices that are
# already plugged in pick the rules up without a reboot.
if command -v udevadm >/dev/null 2>&1; then
    udevadm control --reload-rules >/dev/null 2>&1 || true
    udevadm trigger >/dev/null 2>&1 || true
fi
exit 0
