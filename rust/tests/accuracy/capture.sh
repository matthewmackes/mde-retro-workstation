#!/usr/bin/env bash
# Produce the screenshots the accuracy harness checks (layer 2 of ACCURACY.md).
#
# Run inside a Sway session. For each component it launches the binary, lets it
# paint, grabs the active output with grim, and tears it down. Output lands in
# tests/accuracy/captures/ (gitignored); then `cargo test --test accuracy`
# spot-checks the pixels against tests/accuracy/checklist.toml.
#
# Usage:  tests/accuracy/capture.sh [desktop|panel|all]   (default: all)
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
out_dir="$here/captures"
mkdir -p "$out_dir"
rust_root="$(cd "$here/../.." && pwd)"
bin="$rust_root/target/debug/mde"

if [[ -z "${WAYLAND_DISPLAY:-}" ]]; then
    echo "capture.sh: not in a Wayland session (WAYLAND_DISPLAY unset)" >&2
    exit 1
fi
command -v grim >/dev/null || { echo "capture.sh: grim not found" >&2; exit 1; }
[[ -x "$bin" ]] || { echo "capture.sh: build first (cargo build) — $bin missing" >&2; exit 1; }

output="$(swaymsg -t get_outputs | grep -o '"name": "[^"]*"' | head -1 | cut -d'"' -f4)"
echo "capture.sh: output=$output"

# Best-effort wake: a blanked/idle output makes grim read a black framebuffer,
# which the harness (correctly) rejects. Turn DPMS on and nudge the pointer to
# generate a damage event so the compositor repaints before we capture. If the
# session is locked or the screen stays dark, captures will fail the spot-check
# — that's the harness doing its job, not a false negative.
swaymsg "output $output dpms on" >/dev/null 2>&1 || true
swaymsg 'seat - cursor move 10 10' >/dev/null 2>&1 || true
swaymsg 'seat - cursor move -10 -10' >/dev/null 2>&1 || true
sleep 0.5

grab() { grim -o "$output" "$out_dir/$1"; echo "  -> $1"; }

# Crop to a rect in global coords: grab_rect FILE X Y W H. grim's -g (geometry)
# is mutually exclusive with -o, and the rect from swaymsg is already global.
grab_rect() { grim -g "$2,$3 $4x$5" "$out_dir/$1"; echo "  -> $1 ($4x$5 @ $2,$3)"; }

# Find a mapped toplevel whose title contains $1; echo "X Y W H" of its content
# rect, or nothing if it never maps. Polls up to ~8s (iced is slow to first map).
find_window_rect() {
    local needle="$1" i
    for i in $(seq 1 16); do
        local r
        r=$(swaymsg -t get_tree | python3 -c "
import json,sys
needle=sys.argv[1].lower()
t=json.load(sys.stdin)
def walk(n):
    nm=(n.get('name') or '')
    if needle in nm.lower() and n.get('rect') and n.get('pid'):
        r=n['rect']; print(r['x'],r['y'],r['width'],r['height']); raise SystemExit
    for c in n.get('nodes',[])+n.get('floating_nodes',[]): walk(c)
walk(t)
" "$needle" 2>/dev/null)
        [[ -n "$r" ]] && { echo "$r"; return 0; }
        sleep 0.5
    done
    return 1
}

# Snapshot the live desktop as-is (sway background + whatever taskbar is up).
cap_desktop() { echo "[desktop]"; grab desktop.png; }

# Launch the Rust layer-shell taskbar, let it paint, capture, kill it.
cap_panel() {
    echo "[panel]"
    "$bin" panel &
    local pid=$!
    sleep 1.5
    grab panel.png
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
}

# Launch an iced toplevel ($1 = mde subcommand, $2 = title substring,
# $3 = out file), crop the capture to the window's content rect so client-area
# spot-checks are placement-independent. Skips (no file) if it never maps.
cap_window() {
    echo "[$1]"
    "$bin" "$1" >/dev/null 2>&1 &
    local pid=$! rect
    if rect=$(find_window_rect "$2"); then
        # shellcheck disable=SC2086
        grab_rect "$3" $rect
    else
        echo "  !! window '$2' never mapped — skipped" >&2
    fi
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
}

case "${1:-all}" in
    desktop)       cap_desktop ;;
    panel)         cap_panel ;;
    files)         cap_window files "mde files" files.png ;;
    control-panel) cap_window control-panel "Control Panel" control-panel.png ;;
    all)
        cap_desktop
        cap_panel
        cap_window files "mde files" files.png
        cap_window control-panel "Control Panel" control-panel.png
        ;;
    *) echo "usage: capture.sh [desktop|panel|files|control-panel|all]" >&2; exit 2 ;;
esac

echo "capture.sh: done. Verify with:  cargo test --test accuracy -- --nocapture"
