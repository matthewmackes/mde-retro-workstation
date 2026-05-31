#!/usr/bin/env bash
# Stage the bundled Win2000 theme into target/rpm-assets/ for `cargo generate-rpm`.
# Sources the installed freedesktop theme (populated by `mde install --assets`,
# which fetches Win2k + Chicago95 from upstream). The 76MB Chicago95 icon
# fallback is intentionally NOT staged — it stays a fetch-at-first-run asset.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"   # rust/
stage="$here/target/rpm-assets"
share="${XDG_DATA_HOME:-$HOME/.local/share}"
rm -rf "$stage"; mkdir -p "$stage/icons" "$stage/sounds"
cp -r "$share/icons/Win2k" "$stage/icons/"
cp -r "$share/icons/Chicago95_Standard_Cursors" "$stage/icons/"
cp -r "$share/sounds/Chicago95" "$stage/sounds/"
find "$stage" -name 'icon-theme.cache' -delete
rm -rf "$stage/icons/Chicago95_Standard_Cursors/build"  # source files, not the theme
echo "staged $(du -sh "$stage" | cut -f1) into $stage"
