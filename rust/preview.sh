#!/usr/bin/env bash
# MDE-Retro — preview launcher for the operator's review.
#
# The Rust Win2000 shell is feature-complete in preview but NOT yet cut over
# (the live sway-config flip is the deliberate last step, done with you present).
# This script lets you review it without that commitment:
#
#   ./preview.sh gallery      (re)generate the screenshot gallery in an isolated
#                             headless sway — no effect on your live desktop.
#                             Output: tests/accuracy/captures/gallery/
#   ./preview.sh verify       run the accuracy harness (cargo test) the same way.
#   ./preview.sh <component>  launch one component as a window on your CURRENT
#                             session so you can click around. Components:
#                             panel menu files control-panel system-properties
#                             run properties logoff shutdown setup
#                             (panel/menu are layer-shell and overlay your bar;
#                             kill them when done — they don't touch any config.)
#   ./preview.sh              this help.
#
# Nothing here edits ~/.config/sway or installs anything.
set -uo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
bin="$here/target/debug/mde"

build_if_needed() {
    if [[ ! -x "$bin" ]]; then
        echo "preview: building (cargo build)…"
        ( cd "$here" && cargo build ) || { echo "preview: build failed" >&2; exit 1; }
    fi
}

case "${1:-help}" in
    gallery)
        build_if_needed
        exec bash "$here/tests/accuracy/gallery.sh"
        ;;
    verify)
        build_if_needed
        exec bash "$here/tests/accuracy/nested-sway.sh"
        ;;
    panel|menu|files|control-panel|system-properties|security|run|properties|logoff|shutdown|setup)
        build_if_needed
        sub="$1"; shift
        echo "preview: launching 'mde $sub' on the current session (kill it when done)…"
        case "$sub" in
            properties) exec "$bin" properties "${1:-Command Prompt}" "${2:-/usr/bin/foot}" ;;
            setup)      exec "$bin" setup --gui ;;  # explicit visual preview; installs nothing
            *)          exec "$bin" "$sub" "$@" ;;
        esac
        ;;
    help|-h|--help)
        # Print the leading comment block (lines after the shebang, while they
        # are still comments), stripping the "# " prefix.
        awk 'NR>1 && /^#/ {sub(/^# ?/,""); print; next} NR>1 {exit}' "${BASH_SOURCE[0]}"
        ;;
    *)
        echo "preview: unknown target '$1'. Run ./preview.sh for help." >&2
        exit 2
        ;;
esac
