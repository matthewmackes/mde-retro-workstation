#!/usr/bin/env bash
# ============================================================================
#  install-assets.sh  --  orchestrate every visual-asset installer for MDE-Retro
#
#  Order matters: Chicago95 first (broad coverage + cursors + sounds + GTK
#  theme), then the Win2k icon theme (primary Win2000 icons, inherits
#  Chicago95), then optionally ReactOS assets.
#
#  Usage:
#    ./install-assets.sh               # Chicago95 + Win2k icons
#    ./install-assets.sh --reactos     # also harvest ReactOS assets
#    ./install-assets.sh --only chicago95|win2k|reactos
# ============================================================================
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SWAY_SCRIPTS="${XDG_CONFIG_HOME:-$HOME/.config}/sway/scripts"
WITH_REACTOS=0
ONLY=""

while [ $# -gt 0 ]; do
    case "$1" in
        --reactos) WITH_REACTOS=1 ;;
        --only)    ONLY="${2:-}"; shift ;;
        -h|--help) sed -n '2,14p' "$0"; exit 0 ;;
        *) echo "Unknown option: $1" >&2; exit 2 ;;
    esac
    shift
done

run_chicago95() {
    echo "== Chicago95 =="
    "$HERE/install-chicago95.sh"
}

run_win2k() {
    echo "== Win2k icon theme =="
    # The Win2k icon installer lives with the sway scripts (it generates the
    # freedesktop aliases + index.theme and wires Inherits=Chicago95,...).
    if [ -x "$SWAY_SCRIPTS/install-win2k-icons.py" ]; then
        python3 "$SWAY_SCRIPTS/install-win2k-icons.py"
    else
        echo "   skip: $SWAY_SCRIPTS/install-win2k-icons.py not found"
        echo "   (run ../install.sh first so the sway config tree is deployed)"
    fi
}

run_reactos() {
    echo "== ReactOS assets (optional) =="
    "$HERE/install-reactos-assets.sh"
}

if [ -n "$ONLY" ]; then
    case "$ONLY" in
        chicago95) run_chicago95 ;;
        win2k)     run_win2k ;;
        reactos)   run_reactos ;;
        *) echo "Unknown --only target: $ONLY" >&2; exit 2 ;;
    esac
    exit 0
fi

run_chicago95
run_win2k
[ "$WITH_REACTOS" = "1" ] && run_reactos

echo ">> All requested assets installed."
echo "   Reload the desktop with Win+Shift+C (or: swaymsg reload)."
