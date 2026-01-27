#!/usr/bin/env bash
# Enable libinput high-resolution scrolling for Engineer Bo Full Scroll Dial.
#
# How it works:
#   The hwdb tells libinput the physical wheel geometry so it can interpret
#   high-res scroll events correctly. Standard configurations:
#
#     15° angle, 24 detents/rev  (this device)
#     20° angle, 18 detents/rev
#
#   libinput uses the "v120" system where 120 units = 1 detent. High-res
#   devices send fractional values (e.g. 3, 5, 40) that sum to 120 per click.
#
# hwdb match format:
#   Must use "mouse:usb:v<vendor>p<product>:name:<name>:*" (lowercase hex)
#   NOT "evdev:name:..." which won't match input devices.
#   See /usr/lib/udev/hwdb.d/70-mouse.hwdb for examples.
#
# Verify with:
#   udevadm info --query=property /dev/input/eventX | grep MOUSE_WHEEL
#   sudo libinput debug-events --device /dev/input/eventX
#   (look for fractional values and * marker indicating high-res)
#
# Reference: https://wayland.freedesktop.org/libinput/doc/latest/wheel-api.html

set -euo pipefail

DEVICE_NAME="Engineer Bo Full Scroll Dial"
VENDOR="feed"
PRODUCT="beef"
ANGLE="${ANGLE:-15}"
COUNT="${COUNT:-24}"
HWDB_PATH="/etc/udev/hwdb.d/90-engineer-bo-wheel.hwdb"
MATCH="mouse:usb:v${VENDOR}p${PRODUCT}:name:${DEVICE_NAME}:*"
EVENT_PATH=""

if [[ $(id -u) -ne 0 ]]; then
  echo "Please run as root (sudo) to write hwdb and trigger udev."
  exit 1
fi

if command -v libinput >/dev/null 2>&1; then
  EVENT_PATH=$(libinput list-devices | awk -v name="$DEVICE_NAME" '
    $0 ~ "^Device:" && found {exit}
    $0 ~ "^Device:" {found=0}
    $0 ~ "^Device:" && index($0, name) {found=1}
    found && /Kernel:/ {print $2; exit}
  ')
fi

cat > "$HWDB_PATH" <<EOF
${MATCH}
 MOUSE_WHEEL_CLICK_ANGLE=${ANGLE}
 MOUSE_WHEEL_CLICK_COUNT=${COUNT}
 MOUSE_HWHEEL_CLICK_ANGLE=${ANGLE}
 MOUSE_HWHEEL_CLICK_COUNT=${COUNT}
EOF

echo "Wrote $HWDB_PATH with angle=${ANGLE}, count=${COUNT}"

systemd-hwdb update

if [[ -n "$EVENT_PATH" && -e "$EVENT_PATH" ]]; then
  udevadm trigger "$EVENT_PATH"
else
  udevadm trigger -s input
fi

echo "Reloaded hwdb. Current match entry:"
systemd-hwdb query "$MATCH" || true
echo "Replug device or verify with: sudo libinput debug-events --device ${EVENT_PATH:-/dev/input/eventX}"
