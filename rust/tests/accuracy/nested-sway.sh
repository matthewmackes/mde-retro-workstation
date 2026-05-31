#!/usr/bin/env bash
# Spin up an isolated, headless nested sway and run the accuracy harness inside
# it — no physical screen, no idle/occlusion, fixed geometry. This is the clean
# way to grim-verify the shell from anywhere (incl. an away/headless session).
#
# Usage:
#   tests/accuracy/nested-sway.sh            # launch nested sway, capture, verify
#   tests/accuracy/nested-sway.sh shell      # launch + print the env, leave it up
#
# Requires: sway (with the headless backend), grim, swaymsg, a built ./target/debug/mde.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
rust_root="$(cd "$here/../.." && pwd)"
log="$(mktemp /tmp/mde-nested-sway.XXXXXX.log)"

command -v sway >/dev/null || { echo "nested-sway: sway not found" >&2; exit 1; }
[[ -x "$rust_root/target/debug/mde" ]] || { echo "nested-sway: build first (cargo build)" >&2; exit 1; }

echo "nested-sway: launching headless sway…"
WLR_BACKENDS=headless WLR_HEADLESS_OUTPUTS=1 setsid \
    sway -c "$here/nested-sway.conf" >"$log" 2>&1 &
disown 2>/dev/null || true

# Find the new sway's IPC socket (the most recently created one).
sock=""
for _ in $(seq 1 40); do
    sock="$(ls -t "${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"/sway-ipc.*.sock 2>/dev/null | head -1)"
    if [[ -n "$sock" ]] && SWAYSOCK="$sock" swaymsg -t get_outputs >/dev/null 2>&1; then
        # Confirm it's the HEADLESS one (not the user's live session).
        SWAYSOCK="$sock" swaymsg -t get_outputs | grep -q HEADLESS-1 && break
    fi
    sock=""
done
[[ -n "$sock" ]] || { echo "nested-sway: failed to start (see $log)" >&2; cat "$log" >&2; exit 1; }

# The wayland display name is the pid-matched socket's sibling; derive from log.
wl="$(grep -oE "wayland-[0-9]+" "$log" | head -1)"
# Fallback: pick the highest-numbered wayland socket (the newest).
[[ -n "$wl" ]] || wl="$(basename "$(ls -t "${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"/wayland-[0-9]* 2>/dev/null | grep -v '\.lock' | head -1)")"

export WAYLAND_DISPLAY="$wl" SWAYSOCK="$sock"
echo "nested-sway: up on WAYLAND_DISPLAY=$WAYLAND_DISPLAY  SWAYSOCK=$SWAYSOCK"

if [[ "${1:-}" == "shell" ]]; then
    echo "Run e.g.:  WAYLAND_DISPLAY=$WAYLAND_DISPLAY SWAYSOCK=$SWAYSOCK ./target/debug/mde files"
    echo "Stop it:   swaymsg -t command exit   (with the env above) or kill the sway pid"
    exit 0
fi

echo "nested-sway: capturing + verifying…"
bash "$here/capture.sh" all
( cd "$rust_root" && cargo test --test accuracy -- --nocapture )

echo "nested-sway: done. Stop the compositor with:  SWAYSOCK=$SWAYSOCK swaymsg exit"
