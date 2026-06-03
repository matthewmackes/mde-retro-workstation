#!/usr/bin/env bash
# Generate the LightDM-gtk-greeter "Windows 10" theme (E10.9) into assets/greeter/.
# The CSS colours come from `mde greeter --css` (palette-sourced via palette::hex,
# no hand-hex — §2.1); the conf from `mde greeter --conf`. E10.10 installs win10.css
# as a GTK theme and the conf into /etc/lightdm via the RPM %post.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"   # repo root
bin="$root/rust/target/debug/mde"
[ -x "$bin" ] || bin="$root/rust/target/release/mde"
[ -x "$bin" ] || { echo "build mde first: (cd rust && cargo build)"; exit 1; }

out="$root/assets/greeter"
mkdir -p "$out"
# Reuse the project's own login background — install-branding.sh already installs
# mde-wallpaper.png to this path (rust/assets/branding/scripts/install-branding.sh),
# so the greeter ships no duplicate/Fedora-licensed binary.
bg="/usr/share/backgrounds/mde-retro/login.png"

"$bin" greeter --css >"$out/win10.css"
"$bin" greeter --conf "$bg" >"$out/lightdm-gtk-greeter.conf"

echo "staged greeter theme into $out:"
ls -1 "$out"
