#!/usr/bin/env python3
"""
Install the WinClassic / KDE-Store "Windows 2000" icon theme as a proper
freedesktop icon theme usable by GTK apps (Waybar's wlr/taskbar, wofi, ...).

The upstream tarball (133-Win2k-2.2.2-1.tgz, a 2001 KDE 2.x icon set) has:
  * size dirs 16x16 .. 64x64, with KDE-2 category dirs
    (actions / apps / devices / filesystems / mimetypes)
  * KDE-2 icon names (konqueror, konsole, folder_open, ...)
  * NO index.theme

This script:
  1. extracts it to ~/.local/share/icons/Win2k
  2. writes a spec-compliant index.theme (Inherits hicolor + Adwaita so any
     icon we don't cover falls back gracefully)
  3. creates freedesktop-named aliases that point at the closest Win2000
     icon, so modern apps actually pick them up (Firefox -> konqueror,
     foot/terminal -> konsole, folders, trash, system tools, mimetypes...)
  4. refreshes the GTK icon cache

Re-running is safe (idempotent): it rebuilds the theme from the tarball.
"""

import os
import re
import shutil
import tarfile
import subprocess

HOME = os.path.expanduser("~")
TARBALL = f"{HOME}/.config/sway/resources/133-Win2k-2.2.2-1.tgz"
THEME_DIR = f"{HOME}/.local/share/icons/Win2k"
THEME_NAME = "Win2k"

# KDE-2 category dir -> freedesktop Context
CONTEXT = {
    "actions": "Actions",
    "apps": "Applications",
    "devices": "Devices",
    "filesystems": "Places",
    "mimetypes": "MimeTypes",
}

# freedesktop icon name -> source file (relative to a size dir).
# Only created when the source actually exists at that size.
ALIASES = [
    # --- Applications -------------------------------------------------
    ("firefox", "apps/konqueror.png"),
    ("web-browser", "apps/konqueror.png"),
    ("foot", "apps/konsole.png"),
    ("org.codeberg.dnkl.foot", "apps/konsole.png"),
    ("utilities-terminal", "apps/konsole.png"),
    ("terminal", "apps/konsole.png"),
    ("text-editor", "apps/kate.png"),
    ("accessories-text-editor", "apps/kate.png"),
    ("system-file-manager", "apps/kfm.png"),
    ("preferences-system", "apps/kcontrol.png"),
    ("systemsettings", "apps/kcontrol.png"),
    ("preferences-desktop", "apps/kcontrol.png"),
    ("help-browser", "apps/khelpcenter.png"),
    ("preferences-desktop-font", "apps/fonts.png"),
    ("preferences-desktop-keyboard", "apps/keyboard.png"),
    ("utilities-system-monitor", "apps/ksysguard.png"),
    ("gparted", "apps/kcmpartitions.png"),
    ("printer", "devices/printer1.png"),
    ("preferences-system-printer", "devices/printer1.png"),
    ("clock", "apps/clock.png"),
    ("preferences-desktop-screensaver", "apps/kscreensaver.png"),
    # --- Places -------------------------------------------------------
    ("folder", "filesystems/folder.png"),
    ("folder-open", "filesystems/folder_open.png"),
    ("inode-directory", "filesystems/folder.png"),
    ("user-home", "filesystems/folder_home.png"),
    ("folder-home", "filesystems/folder_home.png"),
    ("user-desktop", "filesystems/desktop.png"),
    ("user-trash", "filesystems/trashcan_empty.png"),
    ("user-trash-full", "filesystems/trashcan_full.png"),
    ("network-workgroup", "filesystems/network.png"),
    ("network-server", "filesystems/network.png"),
    ("folder-remote", "filesystems/network.png"),
    ("emblem-important", "filesystems/folder_important.png"),
    # --- Generic executable fallback ---------------------------------
    ("application-x-executable", "filesystems/exec.png"),
    ("application-default-icon", "filesystems/exec.png"),
    ("exec", "filesystems/exec.png"),
    # --- MimeTypes ----------------------------------------------------
    ("text-x-generic", "mimetypes/txt.png"),
    ("text-plain", "mimetypes/txt.png"),
    ("text-html", "mimetypes/html.png"),
    ("image-x-generic", "mimetypes/image.png"),
    ("audio-x-generic", "mimetypes/sound.png"),
    ("video-x-generic", "mimetypes/video.png"),
    ("font-x-generic", "mimetypes/font.png"),
    ("application-x-shellscript", "mimetypes/shellscript.png"),
    ("text-x-script", "mimetypes/shellscript.png"),
    ("package-x-generic", "mimetypes/rpm.png"),
    ("application-x-rpm", "mimetypes/rpm.png"),
]

SIZE_RE = re.compile(r"^(\d+)x\d+$")


def extract():
    if os.path.isdir(THEME_DIR):
        shutil.rmtree(THEME_DIR)
    os.makedirs(os.path.dirname(THEME_DIR), exist_ok=True)
    with tarfile.open(TARBALL, "r:gz") as tf:
        members = tf.getmembers()
        # Strip the leading "Win2k-2.2.2-1/" component.
        top = members[0].name.split("/")[0] + "/"
        for m in members:
            if not m.name.startswith(top):
                continue
            m.name = m.name[len(top):]
            if m.name:
                tf.extract(m, THEME_DIR)
    print(f"extracted -> {THEME_DIR}")


def make_aliases():
    created = 0
    for entry in sorted(os.listdir(THEME_DIR)):
        sdir = os.path.join(THEME_DIR, entry)
        if not (os.path.isdir(sdir) and SIZE_RE.match(entry)):
            continue
        for name, src in ALIASES:
            src_path = os.path.join(sdir, src)
            if not os.path.isfile(src_path):
                continue
            dst = os.path.join(sdir, os.path.dirname(src), name + ".png")
            if os.path.abspath(dst) == os.path.abspath(src_path):
                continue  # already named correctly
            if not os.path.exists(dst):
                shutil.copyfile(src_path, dst)
                created += 1
    print(f"created {created} freedesktop-named aliases")


def write_index_theme():
    sizes = []
    for entry in sorted(os.listdir(THEME_DIR)):
        m = SIZE_RE.match(entry)
        if m and os.path.isdir(os.path.join(THEME_DIR, entry)):
            sizes.append((entry, int(m.group(1))))

    dirs = []
    blocks = []
    for entry, size in sizes:
        sdir = os.path.join(THEME_DIR, entry)
        for cat in sorted(os.listdir(sdir)):
            cat_path = os.path.join(sdir, cat)
            if not os.path.isdir(cat_path) or cat not in CONTEXT:
                continue
            rel = f"{entry}/{cat}"
            dirs.append(rel)
            blocks.append(
                f"[{rel}]\nSize={size}\nContext={CONTEXT[cat]}\n"
                f"Type=Threshold\n")

    header = (
        "[Icon Theme]\n"
        f"Name={THEME_NAME}\n"
        "Comment=Windows 2000 icon theme (KDE-Store 1120706), bridged to "
        "freedesktop naming\n"
        "Inherits=hicolor,Adwaita\n"
        "Directories=" + ",".join(dirs) + "\n\n")

    with open(os.path.join(THEME_DIR, "index.theme"), "w") as fh:
        fh.write(header + "\n".join(blocks) + "\n")
    print(f"wrote index.theme with {len(dirs)} directories")


def refresh_cache():
    if shutil.which("gtk-update-icon-cache"):
        subprocess.run(["gtk-update-icon-cache", "-f", "-t", THEME_DIR],
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        print("refreshed icon cache")
    else:
        print("gtk-update-icon-cache not found (cache optional)")


def main():
    if not os.path.isfile(TARBALL):
        raise SystemExit(f"missing tarball: {TARBALL}")
    extract()
    make_aliases()
    write_index_theme()
    refresh_cache()
    print(f"\nDone. Set gtk-icon-theme-name={THEME_NAME} in your GTK settings.")


if __name__ == "__main__":
    main()
