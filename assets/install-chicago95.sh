#!/usr/bin/env bash
# ============================================================================
#  install-chicago95.sh
#  Fetch the Chicago95 project and deploy its classic-Windows assets:
#    * GTK theme            -> ~/.local/share/themes/Chicago95
#    * icon theme(s)        -> ~/.local/share/icons/Chicago95*
#    * cursor theme         -> ~/.local/share/icons/Chicago95_Standard_Cursors
#    * sound theme          -> ~/.local/share/sounds/Chicago95
#
#  Upstream: https://github.com/grassmunk/Chicago95  (GPL-3.0)
#  Nothing is committed to this repo; we clone at install time so the
#  upstream license travels with the bytes.
# ============================================================================
set -euo pipefail

REPO_URL="https://github.com/grassmunk/Chicago95.git"
CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/mde-retro"
SRC="$CACHE/Chicago95"
SHARE="${XDG_DATA_HOME:-$HOME/.local/share}"

mkdir -p "$CACHE" "$SHARE/themes" "$SHARE/icons" "$SHARE/sounds"

echo ">> Chicago95: fetching source into $SRC"
if [ -d "$SRC/.git" ]; then
    git -C "$SRC" pull --ff-only --depth 1 || true
else
    git clone --depth 1 "$REPO_URL" "$SRC"
fi

copy_theme() {
    # $1 = source subdir under $SRC, $2 = destination dir under $SHARE
    local from="$SRC/$1" to="$SHARE/$2"
    [ -d "$from" ] || { echo "   skip $1 (not present upstream)"; return; }
    rm -rf "$to"
    cp -r "$from" "$to"
    echo "   deployed $2"
}

# GTK / window-decoration theme
copy_theme "Theme/Chicago95"                 "themes/Chicago95"

# Icon themes (the main set plus the high-contrast/extra variants if present)
copy_theme "Icons/Chicago95"                 "icons/Chicago95"

# Cursor theme
copy_theme "Cursors/Chicago95_Standard_Cursors" "icons/Chicago95_Standard_Cursors"

# Sound theme (used for the login chime in the Sway config)
copy_theme "sounds/Chicago95"                "sounds/Chicago95"

# Refresh icon caches where the tooling exists (harmless if missing).
for d in "$SHARE/icons/"Chicago95*; do
    [ -d "$d" ] && command -v gtk-update-icon-cache >/dev/null 2>&1 \
        && gtk-update-icon-cache -f "$d" >/dev/null 2>&1 || true
done

echo ">> Chicago95 assets installed under $SHARE"
