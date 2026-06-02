#!/usr/bin/env bash
# ============================================================================
#  install-assets.sh  --  orchestrate every visual-asset installer for MDE-Retro
#
#  Order matters: Chicago95 first (broad coverage + cursors + sounds + GTK
#  theme), then the Win2k icon theme (primary Win2000 icons, inherits
#  Chicago95).
#
#  Usage:
#    ./install-assets.sh               # Chicago95 + Win2k icons
#    ./install-assets.sh --only chicago95|win2k
# ============================================================================
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ONLY=""

while [ $# -gt 0 ]; do
    case "$1" in
        --only)    ONLY="${2:-}"; shift ;;
        -h|--help) sed -n '2,13p' "$0"; exit 0 ;;
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
    # The Win2k icon installer ships alongside this script (the RPM puts both under
    # /usr/share/mde/scripts). It fetches the icon tarball, extracts it to
    # ~/.local/share/icons/Win2k, and writes the freedesktop aliases + a
    # spec-compliant index.theme (Inherits hicolor,Adwaita; Chicago95 is the
    # de-facto base via run_chicago95 above + the icon search path).
    if [ -x "$HERE/install-win2k-icons.py" ]; then
        python3 "$HERE/install-win2k-icons.py"
    else
        echo "   skip: $HERE/install-win2k-icons.py not found"
    fi
}

if [ -n "$ONLY" ]; then
    case "$ONLY" in
        chicago95) run_chicago95 ;;
        win2k)     run_win2k ;;
        *) echo "Unknown --only target: $ONLY" >&2; exit 2 ;;
    esac
    exit 0
fi

run_chicago95
run_win2k

echo ">> All requested assets installed."
echo "   Log out and back in to pick up the new icons."
