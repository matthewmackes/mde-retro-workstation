#!/usr/bin/env bash
# Start (or refresh) the desktop taskbar for the Win95 sway theme.
#
# Goal:
#   * If Waybar is installed  -> hide all of sway's built-in swaybar(s)
#     and run Waybar instead, fully detached so it survives reloads.
#   * If Waybar is NOT installed -> make sure the built-in swaybar(s)
#     are visible (mode dock) so we always have a working fallback.
#
# This script is idempotent: it is safe to run on every `sway reload`.
# It never spawns a duplicate Waybar (it kills any existing one first
# and WAITS for it to exit before launching a fresh one), and it does
# nothing harmful when there are no bar ids.

set -u

# Collect the ids of sway's built-in bars (one per line). The python3
# helper turns the JSON array ["bar-0", ...] into newline-separated ids.
# Tolerate the list being empty, missing, or not an array.
ids=$(swaymsg -t get_bar_config 2>/dev/null \
    | python3 -c 'import sys, json
try:
    data = json.load(sys.stdin)
except Exception:
    sys.exit(0)
if isinstance(data, list):
    for x in data:
        if isinstance(x, str):
            print(x)' 2>/dev/null) || ids=""

set_bar_mode() {
    # $1 = mode to apply to every collected bar id.
    [ -n "$ids" ] || return 0
    while IFS= read -r id; do
        [ -n "$id" ] || continue
        swaymsg bar "$id" mode "$1" >/dev/null 2>&1 || true
    done <<EOF
$ids
EOF
}

if command -v waybar >/dev/null 2>&1; then
    # Waybar is available: hide every built-in swaybar.
    set_bar_mode invisible

    # Kill any Waybar we previously started so we never stack duplicates.
    # pkill returns non-zero when nothing matched -- that is fine.
    pkill -x waybar >/dev/null 2>&1 || true

    # Wait (bounded) for the old instance to actually exit and release its
    # layer-shell surface, otherwise the fresh Waybar can fail to map or
    # both bars briefly overlap. ~1s ceiling so we never hang a reload.
    for _ in 1 2 3 4 5 6 7 8 9 10; do
        pgrep -x waybar >/dev/null 2>&1 || break
        sleep 0.1
    done

    # Launch a fresh Waybar fully detached from this script/session so it
    # survives the exec_always context being torn down on the next reload.
    setsid waybar >/dev/null 2>&1 &
else
    # No Waybar: ensure the fallback swaybar(s) are docked (visible).
    set_bar_mode dock
fi

exit 0