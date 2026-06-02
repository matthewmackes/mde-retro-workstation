#!/usr/bin/env bash
# Capture the MDE-Retro PREVIEW GALLERY: a screenshot of every shell component,
# taken in an isolated headless nested sway so the result is clean, awake, and
# unoccluded regardless of the host session (it works from an away/headless run).
#
# This is the operator's review artifact — one PNG per component under
# tests/accuracy/captures/gallery/, plus a contact sheet if ImageMagick is here.
#
# Usage:  tests/accuracy/gallery.sh
# Requires: sway (headless backend), grim, swaymsg, a built ./target/debug/mde.
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
rust_root="$(cd "$here/../.." && pwd)"
bin="$rust_root/target/debug/mde"
out="$here/captures/gallery"
conf="$here/gallery-sway.conf"
RT="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"

command -v sway >/dev/null || { echo "gallery: sway not found" >&2; exit 1; }
command -v grim >/dev/null || { echo "gallery: grim not found" >&2; exit 1; }
[[ -x "$bin" ]] || { echo "gallery: build first (cargo build) — $bin missing" >&2; exit 1; }
mkdir -p "$out"

# --- bring up an isolated headless sway, detect its wayland display ----------
before=$(ls "$RT"/wayland-[0-9]* 2>/dev/null | grep -v '\.lock' | sort)
log="$(mktemp /tmp/mde-gallery-sway.XXXXXX.log)"
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
[[ -n "$SWAYSOCK" ]] || { echo "gallery: nested sway failed to start" >&2; cat "$log" >&2; exit 1; }
sleep 0.4
after=$(ls "$RT"/wayland-[0-9]* 2>/dev/null | grep -v '\.lock' | sort)
WL=$(comm -13 <(echo "$before") <(echo "$after") | head -1 | xargs -r basename)
[[ -n "$WL" ]] || { echo "gallery: could not find nested wayland display" >&2; SWAYSOCK="$SWAYSOCK" swaymsg exit 2>/dev/null; exit 1; }
export WAYLAND_DISPLAY="$WL"
echo "gallery: nested sway up (WAYLAND_DISPLAY=$WL, SWAYSOCK=$SWAYSOCK)"

cleanup() { swaymsg exit >/dev/null 2>&1 || true; }
trap cleanup EXIT

# --- helpers -----------------------------------------------------------------
# shot NAME [--crop GEO] [--wait SECS] CMD [ARGS...] — launch a component, let
# it paint, grab the output, then close it before the next shot. By default
# grabs the full output (the window floats centered on the blue desktop, title
# bar and all). --crop "X,Y WxH" grabs just that region (e.g. the taskbar strip,
# which is otherwise a thin band on a big empty desktop). --wait overrides the
# paint delay for slow-to-map components.
shot() {
    local name="$1"; shift
    local crop="" wait=2.2
    while [[ "${1:-}" == --* ]]; do
        case "$1" in
            --crop) crop="$2"; shift 2 ;;
            --wait) wait="$2"; shift 2 ;;
            *) break ;;
        esac
    done
    echo "  [$name] $*"
    "$bin" "$@" >/dev/null 2>&1 &
    local pid=$!
    sleep "$wait"
    if [[ -n "$crop" ]]; then
        grim -g "$crop" "$out/$name.png" 2>/dev/null && echo "    -> $name.png (crop $crop, $(stat -c%s "$out/$name.png" 2>/dev/null) B)" || echo "    !! grab failed"
    else
        grim -o HEADLESS-1 "$out/$name.png" 2>/dev/null && echo "    -> $name.png ($(stat -c%s "$out/$name.png" 2>/dev/null) B)" || echo "    !! grab failed"
    fi
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
    # let the surface fully unmap so the next shot is clean
    sleep 0.4
}

# --- the gallery -------------------------------------------------------------
# Clear any stale single-instance guards first: a leftover menu / Start process
# from an earlier run holds its pid slot, which makes that capture come back
# blank (the surface exits as a duplicate). Kill by the pid FILE (never pkill -f,
# which would match this script's own command line).
for s in mde-menu mde-start-win10; do
    pf="$RT/$s.pid"
    [ -f "$pf" ] && { kill "$(cat "$pf")" 2>/dev/null; rm -f "$pf"; }
done

echo "gallery: capturing components…"
# The bar is a thin strip on a 1280x960 output. The default Carbon theme anchors
# its UI Shell bar to the TOP; the Win2000/BeOS themes use the bottom/left. Crop
# the top strip to match the default. (For a Win2000 capture, use "0,920 1280x40".)
shot panel            --crop "0,0 1280x40" panel
shot start-menu       menu
shot files            files "$HOME"
shot control-panel    control-panel
shot system-properties --wait 3.0 system-properties
shot run-dialog       run
shot properties       properties "Command Prompt" "/usr/bin/foot"
shot log-off          logoff
shot shut-down        shutdown
shot setup            setup --gui

# --- era parity: panel / menu / files under all four themes (E0.9) -----------
# Sandbox the theme via XDG_CONFIG_HOME so we never touch the user's real
# menu.json (state.rs::config_path honours it). BeOS is theme=win2000 + the Haiku
# icon set (main.rs maps that pair to Theme::Beos); the rest are direct theme
# keys. Panel anchor differs per era, so the crop does too: Carbon → top 32px,
# Win2000/Windows10 → bottom (Win10 a taller 40px bar), BeOS → left 115px.
echo "gallery: capturing era parity (carbon · win2000 · windows10 · beos)…"
era_cfg="$(mktemp -d)"; mkdir -p "$era_cfg/mde"
export XDG_CONFIG_HOME="$era_cfg"
for era in "carbon:carbon::0,0 1280x40" \
           "win2000:win2000::0,920 1280x40" \
           "windows10:windows10::0,920 1280x40" \
           "beos:win2000:haiku:0,0 120x960"; do
    IFS=: read -r label theme iconset pcrop <<< "$era"
    printf '{"theme":"%s","theme_mode":"dark","icon_set":"%s"}\n' "$theme" "$iconset" \
        > "$era_cfg/mde/menu.json"
    shot "panel-$label" --crop "$pcrop" panel
    shot "menu-$label"  menu
    shot "files-$label" files "$HOME"
    # The Win10 tiled Start only exists in the Windows 10 era. Seed a few tiles
    # (one widened) so the capture exercises the right tile grid at distinct sizes.
    if [[ "$label" == windows10 ]]; then
        # The Win10 Explorer no-path landing: Quick access + the left nav pane (This
        # PC / Network / Cloud Files), distinct from the files-$label folder view (E8).
        shot "files-win10-quick" files
        printf '{"theme":"windows10","theme_mode":"dark","pinned":[{"name":"Files","command":"mde files"},{"name":"Firefox","command":"firefox","launch_count":5},{"name":"Terminal","command":"foot"}]}\n' \
            > "$era_cfg/mde/menu.json"
        "$bin" start-win10 --resize Files wide >/dev/null 2>&1
        rm -f "$RT/mde-start-win10.pid" # a stale singleton would make Start exit blank
        shot "start-win10" --wait 2.6 start-win10
        # Seed two notifications (swaync owns the live bus here, so the daemon
        # can't run) to exercise the Action Center pane grouping + Clear x's.
        now=$(date +%s)
        cat > "$era_cfg/mde/notifications.json" <<JSON
{"notifications":[
 {"id":1,"app_name":"Files","app_icon":"folder","summary":"Copy complete","body":"3 items copied to Documents","actions":[],"hint_urgency":1,"timestamp":{"secs_since_epoch":$((now-120)),"nanos_since_epoch":0},"transient":false},
 {"id":2,"app_name":"Files","app_icon":"folder","summary":"Download finished","body":"installer.rpm (4.2 MB)","actions":[],"hint_urgency":1,"timestamp":{"secs_since_epoch":$((now-20)),"nanos_since_epoch":0},"transient":false}
],"last_read":{"secs_since_epoch":0,"nanos_since_epoch":0}}
JSON
        shot "action-center" --wait 2.6 action-center
    fi
done
unset XDG_CONFIG_HOME
rm -rf "$era_cfg"

# Era-comparison strip of the four taskbars (the Win10 accent is the tell).
if command -v montage >/dev/null 2>&1; then
    montage "$out"/panel-carbon.png "$out"/panel-win2000.png \
            "$out"/panel-windows10.png "$out"/panel-beos.png \
            -tile 1x -geometry +0+4 -background '#222' -title "MDE-Retro — taskbar per era" \
            "$out/_era-taskbars.png" 2>/dev/null && echo "    -> _era-taskbars.png" || true
fi

# --- optional contact sheet --------------------------------------------------
if command -v montage >/dev/null 2>&1; then
    echo "gallery: assembling contact sheet…"
    montage "$out"/panel.png "$out"/start-menu.png "$out"/files.png \
            "$out"/control-panel.png "$out"/system-properties.png "$out"/run-dialog.png \
            "$out"/properties.png "$out"/log-off.png "$out"/shut-down.png "$out"/setup.png \
            -tile 2x -geometry 640x480+6+6 -background '#d4d0c8' -title "MDE-Retro — preview gallery" \
            "$out/_contact-sheet.png" 2>/dev/null && echo "    -> _contact-sheet.png" || echo "    (montage failed; individual shots are in $out)"
else
    echo "gallery: ImageMagick 'montage' not found — individual shots only (in $out)"
fi

echo "gallery: done. Shots in $out"
ls -1 "$out"/*.png 2>/dev/null
