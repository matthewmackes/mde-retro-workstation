# MDE-Retro: sway → labwc migration (DONE — historical record)

**Goal:** real Windows 2000 title bars with working **minimize / maximize-restore / close** buttons (and a system-menu icon), which sway cannot draw. `labwc` (a wlroots, Openbox-style stacking compositor) draws *themeable* server-side titlebar buttons and is genuinely Win2000-skinnable.

**Status:** ✅ **DONE** (2026-06-01) — the migration was executed; **labwc is the live compositor**. This document is now a historical record of the plan, not pending work. Every phase has a code landing: foreign-toplevel window control = `wlr.rs`; output management = `outputs.rs` (`wlr-randr --json`); keybinds/theme = `skel/.config/labwc/rc.xml` + the Win2000-MDE Openbox theme; session = `mde/skel/mde-retro.desktop` (`Exec=labwc`); exit = `dialogs.rs` (`labwc --exit`). The "Today (sway)" / "drives swaymsg" framing below is history — there is no `swaymsg` in `rust/mde/src`.

## Why this is bigger than "add buttons"

The Rust shell currently drives **sway IPC** (`swaymsg`) in six files. labwc does not implement the sway/i3 IPC, so each integration needs a Wayland-protocol replacement:

| Touchpoint | File(s) | Today (sway) | labwc replacement |
|---|---|---|---|
| Window list for taskbar | `sway.rs`, `panel.rs` | `swaymsg -t get_tree` | **wlr-foreign-toplevel-management** client (list + titles + states) |
| Focus / close / min / max | `sway.rs`, `popup.rs`, `panel.rs` | `swaymsg [con_id=…] focus/kill/…` | foreign-toplevel `activate` / `close` / `set_minimized` / `set_maximized` |
| Output enumerate + apply | `outputs.rs`, `display.rs` | `swaymsg -t get_outputs`, `output … mode/scale/transform/position` | **wlr-output-management** via `wlr-randr` (CLI, supports `--json`) |
| DPMS (screen saver) | `outputs.rs` | `swaymsg "output * dpms off"` | `wlopm --off '*'` / `wlr-randr --output … --off` |
| Wallpaper | `outputs.rs` | sway `output * bg` directive | run `swaybg` directly (labwc `autostart`) |
| Reconfigure | `outputs.rs`, `popup.rs` | `swaymsg reload` | `labwc --reconfigure` (or SIGHUP) |
| Exit session | `dialogs.rs` | `swaymsg exit` | `labwc --exit` (or SIGTERM) |
| Keybinds | `~/.config/sway/config` | sway bindsym | labwc `rc.xml <keybind>` |
| Panel / menu / popups | (layer-shell) | wlr-layer-shell | **unchanged** — labwc supports layer-shell |
| Tray (SNI) | `tray.rs` | D-Bus | **unchanged** |

The biggest piece of work is the **foreign-toplevel client** (replacing `swaymsg get_tree` + window control), because there is no clean CLI for window *control* on labwc — it needs a real Wayland protocol client (`wayland-client` / `smithay-client-toolkit`).

## Phased plan

**Phase 0 — Spike (no migration yet)**
- Install `labwc`, `wlr-randr`, `wlopm` to a *parallel* session entry; do not replace the sway session.
- Verify the existing layer-shell shell (panel, menu, popups, Display dialog window, tray) renders under labwc unchanged.
- Confirm `wlr-randr --json` reports the eDP-1 modes the Display applet needs.

**Phase 1 — Abstract the WM backend**
- Introduce a `wm` trait/module with two impls: `wm::sway` (current `swaymsg` code, refactored out of `sway.rs`/`outputs.rs`/`popup.rs`/`dialogs.rs`) and `wm::labwc`.
- Operations: `toplevels()`, `focus(id)`, `close(id)`, `set_minimized/maximized(id, bool)`, `outputs()`, `apply_output(...)`, `dpms(on)`, `reconfigure()`, `exit()`.
- Select impl at runtime (env `XDG_CURRENT_DESKTOP`/`MDE_WM`, or detect socket).

**Phase 2 — foreign-toplevel client (labwc window list/control)**
- Add a `wayland-client` dependency; implement a `zwlr_foreign_toplevel_manager_v1` listener on a background thread (mirrors `tray.rs`'s pattern), maintaining the shared toplevel list the panel reads.
- Map taskbar left-click → `activate`; right-click menu → `set_minimized`/`set_maximized`/`close`.

**Phase 3 — output management via wlr-randr**
- Rewrite `outputs.rs` apply/enumerate to shell `wlr-randr` (`--json` parse; `--output NAME --mode/--scale/--transform/--pos/--off`).
- Persistence: write a `kanshi`-style profile or a `wlr-randr` script run from labwc `autostart` (replacing the sway `config.d` fragment). Keep the 15-second revert.

**Phase 4 — labwc config + Win2000 theme (the actual titlebar buttons)**
- `~/.config/labwc/rc.xml`: titlebar button layout (e.g. system-menu icon left; iconify, maximize, close right), focus/raise behaviour, `<keyboard>` keybinds ported from the sway config.
- `~/.config/labwc/autostart`: launch `mde panel`, `swaybg`, `nm-applet`, `swayidle`.
- `~/.config/labwc/menu.xml`: root menu → `mde menu` / desktop Properties → `mde display`.
- **Win2000 Openbox theme** in `~/.local/share/themes/Win2000/openbox-3/`: `themerc` (navy active / silver inactive titlebar, 3D bevel border matching `client.focused` colours) + button bitmaps (`iconify`, `max`, `restore`, `close`, `menu`) drawn as 16px Win2000 glyphs. This is where the min/max/close icons finally live.

**Phase 5 — screen saver / wallpaper / exit**
- `outputs.rs` screensaver: swayidle timeout runs `wlopm --off '*'` (labwc) instead of `swaymsg dpms`.
- Wallpaper apply: spawn `swaybg` directly; persist in `autostart`.
- `dialogs.rs` logoff/shutdown: `labwc --exit`.

**Phase 6 — session wiring + cutover**
- Add a `labwc` session (its own desktop entry / launch script), keep the sway session intact for rollback.
- Smoke-test every shell surface + window control under labwc, then make labwc the default only once verified.

## Risks / unknowns
- **Foreign-toplevel coverage**: confirm labwc exposes title + app-id + min/max state we need (it does activate/close/min/max; verify titles update live).
- **wlr-randr `--json`** availability/version on Fedora 44.
- **Tiling actions** ("Tile Windows", "Minimize All") in the taskbar popup map differently on a stacking WM — re-spec those to labwc actions / foreign-toplevel iconify-all.
- labwc is stacking, so the sway tiling keybinds (`$mod+Shift+arrows` move, splith/splitv) don't translate; the taskbar "Tile" items become snapping/cascade.

## Rollback
Keep the sway session and `~/.config/sway/` untouched throughout; labwc lives in parallel config dirs. Revert = log into the sway session.
