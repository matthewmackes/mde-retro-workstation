#!/usr/bin/env bash
# ============================================================================
#  MDE-Retro installer
#  A Windows 2000 / 95 -style desktop for Sway (Wayland) on Fedora.
#
#  Deploys the config trees in home/.config/ into your real ~/.config by
#  symlinking (default) or copying (MDE_RETRO_COPY=1). Existing files are
#  backed up to <name>.bak.<timestamp> before anything is replaced.
#
#  Usage:
#    ./install.sh              # symlink configs into ~/.config
#    MDE_RETRO_COPY=1 ./install.sh   # copy instead of symlink
#    ./install.sh --assets     # also run the asset installers (Chicago95, ...)
# ============================================================================
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC="$REPO/home/.config"
DEST="${XDG_CONFIG_HOME:-$HOME/.config}"
STAMP="$(date +%Y%m%d-%H%M%S)"
RUN_ASSETS=0

for arg in "$@"; do
    case "$arg" in
        --assets) RUN_ASSETS=1 ;;
        -h|--help) sed -n '2,14p' "$0"; exit 0 ;;
        *) echo "Unknown option: $arg" >&2; exit 2 ;;
    esac
done

echo ">> MDE-Retro: deploying configs into $DEST"
mkdir -p "$DEST"

# Each top-level config component we manage.
COMPONENTS=(sway waybar wofi gtk-3.0 gtk-4.0 fontconfig)

link_one() {
    local name="$1" src="$SRC/$name" dst="$DEST/$name"
    [ -e "$src" ] || { echo "   skip $name (not in repo)"; return; }

    # If the destination is already our symlink to the same place, leave it.
    if [ -L "$dst" ] && [ "$(readlink -f "$dst")" = "$(readlink -f "$src")" ]; then
        echo "   ok   $name (already linked)"
        return
    fi
    # Back up anything real that is in the way.
    if [ -e "$dst" ] || [ -L "$dst" ]; then
        mv "$dst" "$dst.bak.$STAMP"
        echo "   bak  $name -> $name.bak.$STAMP"
    fi

    if [ "${MDE_RETRO_COPY:-0}" = "1" ]; then
        cp -r "$src" "$dst"
        echo "   copy $name"
    else
        ln -s "$src" "$dst"
        echo "   link $name"
    fi
}

for c in "${COMPONENTS[@]}"; do link_one "$c"; done

# Make sure all helper scripts are executable (git preserves the bit, but be safe).
chmod +x "$SRC/sway/scripts/"*.sh "$SRC/sway/scripts/"*.py 2>/dev/null || true
chmod +x "$REPO/assets/"*.sh 2>/dev/null || true

if [ "$RUN_ASSETS" = "1" ]; then
    echo ">> Running asset installers..."
    "$REPO/assets/install-assets.sh"
else
    echo ">> Skipped assets. Run:  ./assets/install-assets.sh   (or ./install.sh --assets)"
fi

cat <<'EONEXT'

>> Done. Next steps:
   1. Install runtime packages (if missing):
        sudo dnf install sway waybar wofi foot wmenu \
                         NetworkManager-applet grim
   2. Install the visual assets (icons / cursors / sounds / GTK theme):
        ./assets/install-assets.sh
   3. Log into a Sway session and reload:   Win+Shift+C   (or `swaymsg reload`)
   4. Optional: make a "Windows 2000" greeter entry selectable in lightdm:
        sudo cp ~/.config/sway/resources/windows2000.desktop \
                /usr/share/wayland-sessions/

   Cheat sheet:  Start = Ctrl+Esc   Run = Win+R   Close = Alt+F4
                 Switch = Alt+Tab    My Computer = Win+E   Terminal = Win+Enter
EONEXT
