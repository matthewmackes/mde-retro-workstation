#!/usr/bin/env bash
# Windows 2000 desktop session launcher (used by the lightdm session entry).
# Starts sway with the Win2000 theme that lives in ~/.config/sway/config.
export XDG_CURRENT_DESKTOP=sway
export XDG_SESSION_DESKTOP=Windows2000
export XCURSOR_THEME=Chicago95_Standard_Cursors
export XCURSOR_SIZE=24
exec sway
