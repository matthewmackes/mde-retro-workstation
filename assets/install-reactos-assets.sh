#!/usr/bin/env bash
# ============================================================================
#  install-reactos-assets.sh  --  harvest ReactOS's open-licensed visual assets
#
#  WHAT THIS DOES (and does NOT do)
#  --------------------------------
#  ReactOS's desktop SHELL (explorer.exe, taskbar, start menu) is native
#  Win32/NT code that renders through win32k/GDI. It cannot be compiled to run
#  on Wayland/Sway, and it does not run cleanly under Wine -- so there is no
#  "shell" to import as running code. See README.md -> "About ReactOS".
#
#  What IS reusable is ReactOS's open-licensed ART: wallpapers, cursors, sounds
#  and theme bitmaps. This script does a sparse, shallow checkout of just the
#  media/ tree from reactos/reactos (GPL-2.0) and deploys what it finds. It is
#  defensive: upstream paths drift, so anything missing is simply skipped.
#
#  Nothing from ReactOS is committed to this repo; it is fetched at install
#  time so the GPL-2.0 terms travel with the bytes. Provenance of individual
#  bitmaps in ReactOS has historically been audited -- prefer Chicago95 assets
#  for anything you intend to redistribute.
#
#  Upstream: https://github.com/reactos/reactos  (GPL-2.0)
# ============================================================================
set -euo pipefail

REPO_URL="https://github.com/reactos/reactos.git"
CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/mde-retro"
SRC="$CACHE/reactos"
SHARE="${XDG_DATA_HOME:-$HOME/.local/share}"
BG_DIR="$SHARE/backgrounds/reactos"

mkdir -p "$CACHE" "$BG_DIR" "$SHARE/sounds" "$SHARE/icons"

cat <<'EONOTE'
>> ReactOS asset harvest
   This fetches only the media/ tree (sparse, shallow) -- tens of MB, not the
   whole 1GB+ source. The ReactOS shell itself is NOT installed (it cannot run
   on Sway). Press Ctrl-C within 5s to abort.
EONOTE
sleep 5

echo ">> Cloning media/ from reactos/reactos into $SRC"
if [ -d "$SRC/.git" ]; then
    git -C "$SRC" sparse-checkout set media 2>/dev/null || true
    git -C "$SRC" pull --ff-only --depth 1 || true
else
    git clone --no-checkout --depth 1 --filter=blob:none "$REPO_URL" "$SRC"
    git -C "$SRC" sparse-checkout init --cone
    git -C "$SRC" sparse-checkout set media
    git -C "$SRC" checkout
fi

harvest() {
    # $1 = glob (relative to $SRC), $2 = destination dir, $3 = label
    local glob="$1" dst="$2" label="$3" n=0 f
    shopt -s nullglob globstar
    for f in $SRC/$glob; do
        [ -f "$f" ] || continue
        mkdir -p "$dst"
        cp -f "$f" "$dst/"
        n=$((n+1))
    done
    shopt -u nullglob globstar
    echo "   $label: $n file(s) -> $dst"
}

# Wallpapers (.bmp/.png/.jpg) -> backgrounds/reactos
harvest "media/wallpaper/**/*.bmp" "$BG_DIR" "wallpapers(bmp)"
harvest "media/wallpaper/**/*.png" "$BG_DIR" "wallpapers(png)"
harvest "media/wallpaper/**/*.jpg" "$BG_DIR" "wallpapers(jpg)"

# Cursors (.cur/.ani) -> a ReactOS cursor staging dir (X11 cursor themes need
# conversion; staged here for the curious -- Chicago95 cursors remain primary)
harvest "media/**/*.cur" "$SHARE/icons/ReactOS_cursors/raw" "cursors(.cur)"
harvest "media/**/*.ani" "$SHARE/icons/ReactOS_cursors/raw" "cursors(.ani)"

# Sounds (.wav) -> sounds/ReactOS
harvest "media/**/*.wav" "$SHARE/sounds/ReactOS" "sounds(.wav)"

# Theme bitmaps (.bmp) under media/themes -> staging for reference
harvest "media/themes/**/*.bmp" "$CACHE/reactos-theme-bitmaps" "theme-bitmaps"

cat <<EODONE

>> ReactOS assets staged.
   Wallpapers:  $BG_DIR
   Sounds:      $SHARE/sounds/ReactOS
   Cursors:     $SHARE/icons/ReactOS_cursors/raw   (.cur/.ani -- not yet an
                X11 cursor theme; convertible with win2xcur if you want them)

   To use a ReactOS wallpaper in Sway, point the desktop at it:
       output * bg $BG_DIR/<file> fill
   (replacing the 'output * bg #3a6ea5 solid_color' line in sway/config).

   The Win2000 Classic color/font spec ReactOS implements is transcribed in:
       assets/reference/win2000-classic-colors.ini
EODONE
