#!/usr/bin/env bash
# Windows 2000-style "Shut Down Windows" menu, themed via the wofi stylesheet.
# Opened from the taskbar power button (waybar custom/power).
set -u

# Don't stack on top of an already-open launcher.
pgrep -x wofi >/dev/null 2>&1 && { pkill -x wofi; exit 0; }

choice=$(printf '%s\n' \
    "⏻  Shut Down" \
    "🔄  Restart" \
    "🚪  Log Off" \
    "🌙  Stand By" \
    "🔒  Lock" |
    wofi --dmenu --insensitive --hide-scroll \
        --prompt "Shut Down Windows" \
        --width 240 --height 250 \
        --location bottom_right --xoffset -6 --yoffset -38 \
        --style "$HOME/.config/wofi/style.css")

case "$choice" in
    *"Shut Down") systemctl poweroff ;;
    *Restart)     systemctl reboot ;;
    *"Log Off")   swaymsg exit ;;
    *"Stand By")  systemctl suspend ;;
    *Lock)        swaylock 2>/dev/null || loginctl lock-session ;;
esac
