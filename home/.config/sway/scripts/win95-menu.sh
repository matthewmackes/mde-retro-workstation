#!/usr/bin/env bash
# Thin wrapper: the Start menu logic lives in win95-menu.py.
#   win95-menu.sh           -> main Start menu (File Manager, Terminal, Programs,
#                              System Tools, Run, Log Off)
#   win95-menu.sh system    -> System Tools menu (Start button RIGHT-click)
#   win95-menu.sh programs  -> full application list
#   win95-menu.sh run       -> Run dialog
# Invoking again while a menu is open toggles it closed (so it can never
# trap keyboard focus).
exec python3 "$HOME/.config/sway/scripts/win95-menu.py" "$@"
