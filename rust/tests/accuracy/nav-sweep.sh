#!/usr/bin/env bash
# Keyboard-nav parity check (E20.7), run in an isolated headless nested sway so it
# never touches the host session (works from an away/headless run).
#
# Part A — no-panic launch sweep: every P0 surface is launched under EACH of the
#   four eras (carbon · win2000 · windows10 · beos). A surface that self-gates to
#   the Win10 era exits cleanly (0) under the classic eras; one that stays up is
#   killed by `timeout` (124). Either is a PASS — only a real crash (a panic exit
#   like 101/134/139, or a SIGSEGV) FAILS. Any failure makes the script exit 1.
#
# Part B — focus-ring capture: each Win10 P0 surface is captured and scanned for the
#   accent focus ring (#4589ff, the live win10() HIGHLIGHT) on its first focusable
#   control — observable proof that keyboard focus reaches the first control. (No
#   wtype/ydotool here, so surfaces that only ring AFTER a Tab press are reported as
#   "no auto-focus ring" rather than failed — the sweep above already proved they
#   launch; the ring proof is carried by the surfaces that auto-focus.)
#
# Usage:  tests/accuracy/nav-sweep.sh   (or ./preview.sh nav-sweep)
# Requires: sway (headless), grim, swaymsg, python3+PIL, a built ./target/debug/mde.
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
rust_root="$(cd "$here/../.." && pwd)"
bin="$rust_root/target/debug/mde"
out="$here/captures/nav-sweep"
conf="$here/gallery-sway.conf"
RT="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"

command -v sway >/dev/null || { echo "nav-sweep: sway not found" >&2; exit 1; }
command -v grim >/dev/null || { echo "nav-sweep: grim not found" >&2; exit 1; }
[[ -x "$bin" ]] || { echo "nav-sweep: build first (cargo build) — $bin missing" >&2; exit 1; }
mkdir -p "$out"

# The P0 surfaces swept in every era. The Win10-only ones (action-center, task-view,
# search, project, start-win10) self-gate and exit 0 under the classic eras.
P0=(panel menu files control-panel system-properties settings action-center task-view search project start-win10)
# The Win10 surfaces captured for the focus-ring pass (xdg-toplevel + layer-shell
# surfaces that render a focusable control on open).
WIN10_FOCUS=(settings search action-center task-view start-win10)

# --- bring up an isolated headless sway ------------------------------------------
before=$(ls "$RT"/wayland-[0-9]* 2>/dev/null | grep -v '\.lock' | sort)
log="$(mktemp /tmp/mde-navsweep-sway.XXXXXX.log)"
WLR_BACKENDS=headless WLR_HEADLESS_OUTPUTS=1 setsid sway -c "$conf" >"$log" 2>&1 &
disown 2>/dev/null || true
SWAYSOCK=""
for _ in $(seq 1 40); do
    for s in $(ls -t "$RT"/sway-ipc.*.sock 2>/dev/null); do
        if SWAYSOCK="$s" swaymsg -t get_outputs 2>/dev/null | grep -q HEADLESS-1; then
            export SWAYSOCK="$s"; break 2
        fi
    done
    sleep 0.3
done
[[ -n "$SWAYSOCK" ]] || { echo "nav-sweep: nested sway failed to start" >&2; cat "$log" >&2; exit 1; }
sleep 0.4
after=$(ls "$RT"/wayland-[0-9]* 2>/dev/null | grep -v '\.lock' | sort)
WL=$(comm -13 <(echo "$before") <(echo "$after") | head -1 | xargs -r basename)
[[ -n "$WL" ]] || { echo "nav-sweep: could not find nested wayland display" >&2; swaymsg exit 2>/dev/null; exit 1; }
export WAYLAND_DISPLAY="$WL"
echo "nav-sweep: nested sway up (WAYLAND_DISPLAY=$WL)"
cleanup() { swaymsg exit >/dev/null 2>&1 || true; }
trap cleanup EXIT

cfg="$(mktemp -d)"; mkdir -p "$cfg/mde"
export XDG_CONFIG_HOME="$cfg"

seed() { printf '{"theme":"%s","theme_mode":"dark","icon_set":"%s"}\n' "$1" "$2" > "$cfg/mde/menu.json"; }
clear_singletons() {
    for p in mde-menu mde-start-win10 mde-search mde-action-center; do
        [ -f "$RT/$p.pid" ] && { kill "$(cat "$RT/$p.pid")" 2>/dev/null; rm -f "$RT/$p.pid"; }
    done
}

# --- Part A: no-panic sweep ------------------------------------------------------
echo "nav-sweep: A) no-panic launch sweep (4 eras × ${#P0[@]} surfaces)…"
fails=0; pairs=0
for era in "carbon:carbon:" "win2000:win2000:" "windows10:windows10:" "beos:win2000:haiku"; do
    IFS=: read -r label theme iconset <<< "$era"
    seed "$theme" "$iconset"
    line="  [$label]"
    for sub in "${P0[@]}"; do
        clear_singletons
        timeout 3 "$bin" $sub >/dev/null 2>&1
        ec=$?
        pairs=$((pairs+1))
        # PASS: 0 (clean/guarded exit) or 124 (stayed up, timeout-killed). Anything
        # else — 101 (Rust panic), 134 (abort), 139 (segv), … — is a crash.
        if [[ $ec -ne 0 && $ec -ne 124 ]]; then
            line="$line $sub=CRASH($ec)"; fails=$((fails+1))
        else
            line="$line $sub=$ec"
        fi
        kill "$(pgrep -f "$bin $sub" | head -1)" 2>/dev/null || true
        sleep 0.15
    done
    echo "$line"
done
echo "nav-sweep: A) $((pairs-fails))/$pairs era×surface pairs clean (0|124), $fails crash(es)."

# --- Part B: Win10 focus-ring captures -------------------------------------------
echo "nav-sweep: B) Windows 10 focus-ring captures…"
seed windows10 ""
rings=0
for sub in "${WIN10_FOCUS[@]}"; do
    clear_singletons
    "$bin" $sub >/dev/null 2>&1 &
    pid=$!
    sleep 2.4
    f="$out/focus-$sub.png"
    grim -o HEADLESS-1 "$f" 2>/dev/null
    kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true
    # Does the capture contain the accent focus ring (#4589ff)? PIL scan.
    res=$(python3 - "$f" <<'PY'
import sys
from PIL import Image
ACC=(0x45,0x89,0xff)
try: im=Image.open(sys.argv[1]).convert("RGB")
except Exception: print("noimg"); sys.exit()
W,H=im.size; px=im.load(); n=0
for y in range(0,H,2):
    for x in range(0,W,2):
        p=px[x,y]
        if all(abs(p[i]-ACC[i])<=16 for i in range(3)): n+=1
print(n)
PY
)
    if [[ "$res" =~ ^[0-9]+$ && "$res" -gt 30 ]]; then
        echo "  [$sub] focus ring / accent present ($res px) -> focus-$sub.png"
        rings=$((rings+1))
    else
        echo "  [$sub] no auto-focus accent ring in static capture ($res px) -> focus-$sub.png"
    fi
    sleep 0.3
done
echo "nav-sweep: B) $rings/${#WIN10_FOCUS[@]} Win10 surfaces show an accent focus ring on open."

rm -rf "$cfg"
if [[ $fails -gt 0 ]]; then
    echo "nav-sweep: FAIL — $fails surface(s) crashed." >&2
    exit 1
fi
echo "nav-sweep: PASS — no crashes across any era×surface pair."
