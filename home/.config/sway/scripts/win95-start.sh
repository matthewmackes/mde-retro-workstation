#!/usr/bin/env bash
# Windows 95-style application launcher (Start menu / Run dialog).
# Uses wmenu-run, themed to look like a Win95 menu: silver background,
# black text, navy-blue highlight bar with white text.
#
# Usage: win95-start.sh [prompt]
#   prompt defaults to "Run".  The Start menu passes "Programs".

prompt="${1:-Run}"

exec wmenu-run \
    -b \
    -i \
    -f "Tahoma 12" \
    -l 12 \
    -p "$prompt" \
    -N "#d4d0c8" \
    -n "#000000" \
    -M "#0a246a" \
    -m "#ffffff" \
    -S "#0a246a" \
    -s "#ffffff"
