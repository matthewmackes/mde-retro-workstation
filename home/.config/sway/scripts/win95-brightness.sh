#!/usr/bin/env bash
# Adjust the internal display backlight via logind (no root, no brightnessctl).
#   win95-brightness.sh up|down
set -u
dev=intel_backlight
base=/sys/class/backlight/$dev
[ -r "$base/brightness" ] || exit 0
cur=$(<"$base/brightness"); max=$(<"$base/max_brightness")
step=$(( max * 7 / 100 )); [ "$step" -lt 1 ] && step=1
floor=$(( max * 5 / 100 ))
case "${1:-}" in
    up)   new=$(( cur + step )); [ "$new" -gt "$max" ]   && new=$max ;;
    down) new=$(( cur - step )); [ "$new" -lt "$floor" ] && new=$floor ;;
    *)    echo "usage: $0 up|down" >&2; exit 1 ;;
esac
busctl call org.freedesktop.login1 /org/freedesktop/login1/session/auto \
    org.freedesktop.login1.Session SetBrightness ssu backlight "$dev" "$new" \
    >/dev/null 2>&1
