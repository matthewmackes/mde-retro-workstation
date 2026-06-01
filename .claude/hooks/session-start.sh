#!/usr/bin/env bash
# SessionStart hook — surface the load-bearing facts the prose READMEs get wrong,
# plus the count of open worklist items. Output is injected as session context.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

echo "MDE-Retro — read .claude/CLAUDE.md. Load-bearing facts the READMEs still get wrong:"
echo "  • Compositor is labwc (NOT sway). Default theme is Carbon dark (NOT Win2000)."
echo "  • One multiplexed binary: 'mde <subcommand>'. Look lib = mde-ui, shell = mde."
echo "  • No raw hex outside palette.rs; all colour via palette::color() (the one theme edge)."
echo "  • Visual changes: a green 'cargo test' does NOT verify the render — run ./preview.sh gallery."

wl="$root/docs/PROJECT_WORKLIST.md"
if [[ -f "$wl" ]]; then
  open=$(grep -cE '^\s*- \[[ >]\]' "$wl" 2>/dev/null || echo 0)
  echo "Worklist (docs/PROJECT_WORKLIST.md): ${open} open/in-progress item(s)."
fi
