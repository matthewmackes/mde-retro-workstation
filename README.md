# MDE-Retro

A **Windows 2000 / 95-style desktop for [Sway](https://swaywm.org/)** (Wayland) on
Fedora. It turns a tiling compositor into a familiar classic-Windows environment:
silver 3D window frames, navy title bars, a gray taskbar with a Start button and
tray clock, floating overlapping windows, click-to-focus, and the keyboard muscle
memory you already have (`Alt+F4`, `Alt+Tab`, `Ctrl+Esc`, `Win+R`, `Win+E`).

It is the *retro theme layer* of a larger personal desktop ("MDE"); everything
here is self-contained and driven by plain Sway + Waybar + wofi config plus a few
small scripts — no patched compositor, no exotic dependencies.

![theme: Windows 2000](https://img.shields.io/badge/theme-Windows%202000-0a246a)
![wm: sway](https://img.shields.io/badge/wm-sway-3a6ea5)
![distro: Fedora%2044](https://img.shields.io/badge/distro-Fedora%2044-d4d0c8)

---

## What you get

| Piece            | How it's done                                                              |
| ---------------- | -------------------------------------------------------------------------- |
| **Desktop**      | Solid Win2000 blue `#3a6ea5`                                               |
| **Window frames**| Silver `#d4d0c8` 3D borders, navy `#0a246a` title bars, white title text   |
| **Behavior**     | Windows float & overlap, click-to-focus (no focus-follows-mouse)           |
| **Taskbar**      | Waybar: ⊞ Start, window-button taskbar, tray, volume, sunken clock         |
| **Start menu**   | `wofi`-based, Win2000-styled (left-click = menu, right-click = System Tools)|
| **Run dialog**   | `Win+R` → wofi run mode                                                     |
| **Control Panel**| Maps installed Fedora tools to Win2000 Control Panel names                  |
| **Icons**        | `Win2k` GTK icon theme, inheriting Chicago95 for coverage                   |
| **Cursors**      | `Chicago95_Standard_Cursors` (arrow, animated hourglass, I-beam, …)         |
| **Sounds**       | Chicago95 sound theme; classic login chime on session start                |
| **Font**         | "Tahoma" everywhere, aliased to Droid Sans where Tahoma isn't installed    |

If Waybar isn't installed the config automatically falls back to a themed
built-in `swaybar`, so the desktop always has a working taskbar.

---

## Install

```sh
git clone https://github.com/<you>/MDE-Retro.git
cd MDE-Retro
./install.sh --assets        # deploy configs + download/install the visual assets
```

`install.sh` **symlinks** the trees in `home/.config/` into your real `~/.config`
(backing up anything in the way to `*.bak.<timestamp>`). Use
`MDE_RETRO_COPY=1 ./install.sh` to copy instead of symlink.

Runtime packages (install what you're missing):

```sh
sudo dnf install sway waybar wofi foot wmenu NetworkManager-applet grim
```

Then log into a Sway session and reload with **`Win+Shift+C`**.

### Optional: a "Windows 2000" login entry

```sh
sudo cp ~/.config/sway/resources/windows2000.desktop /usr/share/wayland-sessions/
```

(The stock "Sway" greeter entry already launches this same config.)

---

## Keyboard cheat sheet

| Action            | Key                  |
| ----------------- | -------------------- |
| Start menu        | `Ctrl+Esc` (or `Win+D`) |
| System Tools menu | `Ctrl+Shift+Esc`     |
| Run               | `Win+R`              |
| Close window      | `Alt+F4`             |
| Switch windows    | `Alt+Tab` / `Alt+Shift+Tab` |
| My Computer       | `Win+E`              |
| Terminal          | `Win+Enter`          |
| Maximize / restore| `Win+Up`             |
| Switch desktops   | `Ctrl+Alt+Left/Right`|
| Move window       | `Win+Shift+Arrows`   |
| Resize mode       | `Win+S`              |
| Log out           | `Ctrl+Alt+Delete`    |

---

## Layout

```
home/.config/
├── sway/
│   ├── config                 # the desktop: theme, behavior, keybindings, taskbar
│   ├── config.d/              # drop-in overrides (e.g. MDE Workbench output)
│   ├── scripts/               # Start menu, Control Panel, taskbar, power, brightness
│   └── resources/             # session .desktop + cached Win2k icon tarball
├── waybar/{config.jsonc,style.css}   # themed taskbar
├── wofi/{config,style.css}           # themed Start menu / Run dialog
├── gtk-3.0,gtk-4.0/settings.ini      # icon/cursor/GTK theme selection
└── fontconfig/fonts.conf             # Tahoma -> humanist sans alias
assets/
├── install-assets.sh          # orchestrator: runs the installers below
├── install-chicago95.sh       # icons/cursors/sounds/GTK theme from grassmunk/Chicago95
└── install-reactos-assets.sh  # optional: harvest ReactOS open-licensed assets
```

The large asset sets (Chicago95 is ~76 MB of icons) are **not** committed — the
installers fetch them from upstream so their licenses travel with them. See
[`LICENSE`](LICENSE) for the asset-licensing note.

---

## About ReactOS

A recurring question: *can ReactOS's desktop shell be imported here?*

**Not as running code.** ReactOS's shell (`explorer.exe`, taskbar, start menu) is
native Win32/NT and renders through `win32k`/GDI — there is no equivalent on
Wayland, so it can't be compiled to run under Sway, and it doesn't run cleanly
under Wine either. Even if it did, it would manage Windows `HWND`s inside a Wine
desktop, not your native Wayland clients.

**The source is still useful two ways**, both of which this project embraces:

1. **As pixel-exact assets** — ReactOS ships open-licensed icons, cursors,
   wallpapers and sounds. `assets/install-reactos-assets.sh` harvests these.
2. **As a faithful spec** — its `GetSysColor` defaults, metrics, fonts and layout
   geometry are the ground truth we reimplement in the native Sway/Waybar/wofi
   stack.

If you want the *literal* ReactOS shell, run ReactOS in a VM (QEMU/KVM); that's a
guest OS, not a Sway integration.

---

## License

Configs and scripts: MIT. Bundled-by-download visual assets keep their upstream
licenses (Chicago95 GPL-3.0, ReactOS GPL-2.0, Win2k icon theme per its store
page). See [`LICENSE`](LICENSE).
