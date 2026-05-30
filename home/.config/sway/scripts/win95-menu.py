#!/usr/bin/env python3
"""
Windows 95 / 2000-style Start menu for the sway Win95 theme.

Modes (argv[1]):
  main      (default) top-level Start menu:
              File Manager, Terminal (pinned at top), Programs, System Tools,
              Run, Log Off.
  programs  the full application list (wofi drun, with icons).
  system    all system / administration tools ("system-*" desktop entries
            plus everything in the System / Settings categories) -- this is
            what the Start button's RIGHT-CLICK opens (Win2000 style).
  run       a Run dialog (wofi run mode).

Behaviour:
  * Toggle: if a menu (wofi) is already open, invoking this again closes it
    (Win95: click Start again to dismiss). Escape also closes it natively.
    This guarantees the menu can never trap keyboard focus.
  * If wofi is not installed, falls back to the themed wmenu launcher
    (win95-start.sh).

Safe testing: set WIN95_MENU_DUMP=1 to print what each menu WOULD show and
exit without ever opening wofi (so it cannot grab the keyboard).
"""

import os
import re
import sys
import glob
import shlex
import shutil
import subprocess

HOME = os.path.expanduser("~")
WOFI_STYLE = f"{HOME}/.config/wofi/style.css"
WOFI_CONF = f"{HOME}/.config/wofi/config"
WMENU_FALLBACK = f"{HOME}/.config/sway/scripts/win95-start.sh"

DUMP = bool(os.environ.get("WIN95_MENU_DUMP"))

# Field codes that must be stripped from a desktop Exec= line.
_FIELD_CODE = re.compile(r'(?<!%)%[fFuUdDnNickvmCI]')


# ---------------------------------------------------------------- helpers
def have(cmd):
    return shutil.which(cmd) is not None


def wofi_available():
    return have("wofi")


def wofi_running():
    return subprocess.run(["pgrep", "-x", "wofi"],
                          stdout=subprocess.DEVNULL,
                          stderr=subprocess.DEVNULL).returncode == 0


def close_menus():
    subprocess.run(["killall", "-q", "wofi"])


def launch(cmd, in_terminal=False):
    """Run cmd fully detached so it survives this script exiting."""
    if in_terminal:
        cmd = "foot -e sh -c " + shlex.quote(cmd)
    subprocess.Popen(["setsid", "-f", "sh", "-c", cmd],
                     stdout=subprocess.DEVNULL,
                     stderr=subprocess.DEVNULL,
                     start_new_session=True)


def strip_exec(exe):
    exe = _FIELD_CODE.sub("", exe)
    exe = exe.replace("%%", "%")
    return exe.strip()


def find_file_manager():
    # Prefer a graphical file manager.
    for fm in ("nautilus", "nemo", "caja", "thunar", "pcmanfm-qt",
               "pcmanfm", "dolphin", "io.elementary.files"):
        if have(fm):
            return fm
    # Then a TUI file manager inside a terminal.
    for tui in ("yazi", "ranger", "lf", "nnn", "mc", "vifm"):
        if have(tui):
            return f"foot -e {tui}"
    # Nothing installed: open a terminal at $HOME so the pin still does
    # something useful. (Install e.g. `pcmanfm` or `nautilus` for a GUI.)
    return 'cd "$HOME" && exec foot'


# ---------------------------------------------------------- desktop files
def _desktop_dirs():
    dirs = [f"{HOME}/.local/share/applications"]
    xdg = os.environ.get("XDG_DATA_DIRS", "/usr/local/share:/usr/share")
    for d in xdg.split(":"):
        d = d.strip()
        if d:
            dirs.append(os.path.join(d, "applications"))
    seen, out = set(), []
    for d in dirs:
        if d not in seen and os.path.isdir(d):
            seen.add(d)
            out.append(d)
    return out


def parse_desktops():
    """Return {name: {'exec','terminal','base','cats'}} for visible apps."""
    entries = {}
    for d in _desktop_dirs():
        for path in glob.glob(os.path.join(d, "*.desktop")):
            data = _parse_one(path)
            if data:
                # First definition wins per dir precedence; don't clobber.
                entries.setdefault(data["name"], data)
    return entries


def _parse_one(path):
    name = exe = None
    cats = []
    terminal = no_display = hidden = False
    in_entry = False
    typ = "Application"
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as fh:
            for raw in fh:
                line = raw.strip()
                if line.startswith("[") and line.endswith("]"):
                    in_entry = (line == "[Desktop Entry]")
                    continue
                if not in_entry or "=" not in line or line.startswith("#"):
                    continue
                key, _, val = line.partition("=")
                key = key.strip()
                val = val.strip()
                if key == "Name" and name is None:
                    name = val
                elif key == "Exec" and exe is None:
                    exe = val
                elif key == "Type":
                    typ = val
                elif key == "Terminal":
                    terminal = val.lower() == "true"
                elif key == "NoDisplay":
                    no_display = val.lower() == "true"
                elif key == "Hidden":
                    hidden = val.lower() == "true"
                elif key == "Categories":
                    cats = [c for c in val.split(";") if c]
    except OSError:
        return None
    if typ != "Application" or no_display or hidden:
        return None
    if not name or not exe:
        return None
    return {
        "name": name,
        "exec": strip_exec(exe),
        "terminal": terminal,
        "base": os.path.basename(path),
        "cats": cats,
    }


def is_system_tool(entry):
    cats = set(entry["cats"])
    # Terminal emulators are not administrative tools (Terminal is already
    # pinned in the main menu), so keep them out of System Tools.
    if "TerminalEmulator" in cats:
        return False
    if entry["base"].startswith("system-"):
        return True
    if "System" in cats:
        return True
    if "Settings" in cats and ("Administration" in cats or "System" in cats):
        return True
    if "Administration" in cats:
        return True
    return False


# --------------------------------------------------------------- wofi I/O
def wofi_dmenu(items, prompt, height):
    """Show a themed wofi dmenu; return the chosen line (or '')."""
    proc = subprocess.run(
        ["wofi", "--dmenu", "--insensitive", "--hide-scroll",
         "--prompt", prompt, "--width", "320", "--height", str(height),
         "--location", "bottom_left", "--xoffset", "0", "--yoffset", "-30",
         "--style", WOFI_STYLE],
        input="\n".join(items), text=True,
        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    return proc.stdout.strip()


def wofi_show(kind):
    """Launch wofi in drun/run mode (it handles exec itself)."""
    subprocess.Popen(
        ["wofi", "--show", kind, "--conf", WOFI_CONF, "--style", WOFI_STYLE],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        start_new_session=True)


# ----------------------------------------------------------------- menus
SEP = "──────────────────"

MAIN_ITEMS = [
    "📁  File Manager",
    "🖥  Terminal",
    SEP,
    "📂  Programs",
    "🛠  Control Panel",
    "⚙  System Tools",
    "🔍  Run...",
    SEP,
    "⏻  Log Off",
]

# Windows 2000 Control Panel: classic applet names mapped to the real
# Fedora/Red Hat tools installed on this machine.  Entries whose backing
# command is missing are hidden automatically.
#   (Windows label, shell command, run-in-terminal)
CONTROL_PANEL = [
    ("📦  Add/Remove Programs",              "dnfdragora",             False),
    ("🔄  Automatic Updates",                "dnfdragora-updater",     False),
    ("🛡  Windows Firewall",                 "firewall-config",        False),
    ("🌐  Network and Dial-up Connections",  "nm-connection-editor",   False),
    ("🔊  Sounds and Multimedia",            "pavucontrol",            False),
    ("💽  Disk Management",                  "gnome-disks",            False),
    ("🗄  Partition Manager",                "gparted",                False),
    ("🗃  Storage Spaces",                   "blivet-gui",             False),
    ("👤  Users and Passwords",              "seahorse",               False),
    ("🌍  Regional Options",                 "system-config-language", False),
    ("⌨  Keyboard and Input",                "im-chooser",             False),
    ("📋  Event Viewer",                     "gnome-abrt",             False),
    ("🔒  Security Center (SELinux)",        "sealert -b",             False),
    ("💿  Create Installation Media",        "mediawriter",            False),
    ("🖥  System",     "hostnamectl; echo; read -p 'Press Enter to close '", True),
    ("🕐  Date and Time", "timedatectl; echo; read -p 'Press Enter to close '", True),
]


def show_main():
    if DUMP:
        print("=== MAIN START MENU ===")
        print("\n".join(MAIN_ITEMS))
        return
    if not wofi_available():
        launch(f'{shlex.quote(WMENU_FALLBACK)} Programs')
        return
    choice = wofi_dmenu(MAIN_ITEMS, "Start", height=320)
    if not choice or choice == SEP:
        return
    if choice.startswith("📁"):
        launch(find_file_manager())
    elif choice.startswith("🖥"):
        launch("foot")
    elif choice.startswith("📂"):
        show_programs()
    elif choice.startswith("🛠"):
        show_control()
    elif choice.startswith("⚙"):
        show_system()
    elif choice.startswith("🔍"):
        show_run()
    elif choice.startswith("⏻"):
        launch("swaymsg exit")


def show_programs():
    if DUMP:
        print("=== PROGRAMS === (wofi drun: full application list with icons)")
        return
    if not wofi_available():
        launch(f'{shlex.quote(WMENU_FALLBACK)} Programs')
        return
    wofi_show("drun")


def show_system():
    entries = parse_desktops()
    tools = sorted((e for e in entries.values() if is_system_tool(e)),
                   key=lambda e: e["name"].lower())
    if DUMP:
        print(f"=== SYSTEM TOOLS ({len(tools)}) ===")
        for e in tools:
            print(f'{e["name"]:32}  [{e["base"]}]  cats={";".join(e["cats"])}')
        return
    if not tools:
        return
    label_map = {f'⚙  {e["name"]}': e for e in tools}
    labels = list(label_map.keys())
    choice = wofi_dmenu(labels, "System Tools", height=460)
    entry = label_map.get(choice)
    if entry:
        launch(entry["exec"], in_terminal=entry["terminal"])


CONTROL_PANEL_APP = f"{HOME}/.config/sway/scripts/control-panel.py"


def show_control():
    if DUMP:
        print(f"=== CONTROL PANEL === (opens GTK window: {CONTROL_PANEL_APP})")
        return
    # The Control Panel is a real window (GTK3), not a menu.
    launch(f"python3 {shlex.quote(CONTROL_PANEL_APP)}")


def show_run():
    if DUMP:
        print("=== RUN === (wofi run mode)")
        return
    if not wofi_available():
        launch(f'{shlex.quote(WMENU_FALLBACK)} Run')
        return
    wofi_show("run")


# ------------------------------------------------------------------ main
def main():
    mode = sys.argv[1] if len(sys.argv) > 1 else "main"

    # Toggle: a second invocation while a menu is open closes it.
    if not DUMP and wofi_running():
        close_menus()
        return

    if mode == "programs":
        show_programs()
    elif mode == "system":
        show_system()
    elif mode == "control":
        show_control()
    elif mode == "run":
        show_run()
    else:
        show_main()


if __name__ == "__main__":
    main()
