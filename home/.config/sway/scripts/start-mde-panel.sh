#!/usr/bin/env bash
# Launch exactly one `mde panel` as the taskbar; retire waybar and the built-in
# swaybar. Idempotent: sway runs this via exec_always on startup AND every
# reload, so it kills any prior panel/waybar before starting a fresh one.
pkill -x waybar 2>/dev/null || true
pkill -f 'mde panel' 2>/dev/null || true
swaymsg bar bar-0 mode invisible >/dev/null 2>&1 || true
sleep 0.3
exec "$HOME/.local/bin/mde" panel
