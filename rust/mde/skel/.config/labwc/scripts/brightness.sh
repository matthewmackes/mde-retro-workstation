#!/usr/bin/env bash
# Adjust the laptop backlight via logind's SetBrightness (no root, no
# brightnessctl). Usage: brightness.sh up|down
set -u
dev=""
for d in /sys/class/backlight/*/; do dev=$(basename "$d"); break; done
[ -n "$dev" ] || exit 0
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
