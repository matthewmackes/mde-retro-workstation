# MDE-Retro — Rust shell

A native **Rust** (iced toolkit, no GTK) desktop shell — the live product, running
on **labwc** and defaulting to the **IBM Carbon** theme (Windows 2000 selectable).
Every component is screenshot-verified in an **isolated headless sway** (the nested
test harness still uses sway because it works on any wlroots compositor — that's a
test-rig detail, not the runtime). The labwc cutover is **done**; this is no longer
a "preview" — see "Shipped since the early preview" and "Still open" below for the
current state.

> **Note:** the capture harness still nests *sway* purely because `wlr.rs`'s
> foreign-toplevel client works on any wlroots compositor; the shipped session is
> labwc (`mde/skel/mde-retro.desktop`).

## How to review it

```sh
cd rust
./preview.sh gallery        # regenerate the screenshot gallery (isolated sway)
./preview.sh verify         # run the accuracy harness (cargo test) the same way
./preview.sh files          # launch one component live on your session
./preview.sh <component>    # panel menu files control-panel system-properties
                            #   run properties logoff shutdown setup
```

Screenshots live in `tests/accuracy/captures/gallery/` (one PNG per component
plus `_contact-sheet.png`). Nothing in `preview.sh` edits your compositor config
or installs packages.

## What's in the preview

One multiplexed binary, `mde <subcommand>`, sharing the `mde-ui` look library
(palette + metrics + 3D-bevel widgets; Carbon default, Win2000/BeOS selectable):

| Component | `mde` command | State |
|-----------|---------------|-------|
| Taskbar (Start, Quick Launch, window buttons, tray, clock) | `panel` | layer-shell; window list via wlr-foreign-toplevel; SNI tray |
| Start menu (pinned, Programs/Settings/Search/System Tools, Run, Log Off, Shut Down) | `menu` | layer-shell, keyboard nav, context menu |
| Explorer file manager (menubar, toolbar, address bar, web-view info band + watermark, folder tree, sortable details list, icons, right-click + Edit-menu file ops, keyboard nav) | `files` | full |
| Control Panel (categorized applets, web-view info band, launch + install-missing) | `control-panel` | full |
| System Properties + Device Manager tree | `system-properties` | native iced, live system data |
| Run dialog | `run` | full |
| Properties dialog | `properties` | full |
| Log Off / Shut Down dialogs | `logoff` / `shutdown` | full |
| Setup (NT-style TUI installer + themed GUI preview) | `setup` | TUI engine does real installs; GUI is a visual preview |

## Accuracy

Two layers, both gated by `cargo test`:
1. `mde-ui/tests/checklist.rs` — pins the palette + `SM_*` metrics (no Wayland).
2. `mde/tests/accuracy.rs` — decodes grim captures and spot-checks rendered
   pixels against the Win2000 colors in `tests/accuracy/checklist.toml`.

Reference targets (eyeball, never SSIM'd against foreign-DPI captures):
`tests/accuracy/refs/win2000-{desktop-full,explorer,openfile-dialog,menu}.png`.

`cargo test` (2026-06-01): **green** — 26 mde unit + 16 mde-ui checklist
(palette + SM_* metrics) + 1 accuracy + 1 lib, 0 failed. Every component
was also captured in an isolated headless sway and eyeballed against the
references (`./preview.sh gallery`); the desktop background reads pixel-exact
`#3a6ea5`.

## Shipped since the early preview

Things the old draft listed as gaps that are now live:

- **Icons** render (folders/files in Explorer, applets in Control Panel,
  Start-menu entries, the info-band watermark) by resolving the embedded Carbon
  SVGs → installed Win2k/Haiku → freedesktop theme. Misses fall back to blank
  space (never tofu); the *art* still depends on the asset fetch, so a machine
  without those themes falls back to text-and-bevel cleanly.
- **System tray** (StatusNotifierItem over zbus) plus the native indicators
  (volume / network / battery) — `tray.rs`, wired into the panel tick.
- **Taskbar right-click menu** (Tile / Minimize all / Task Manager / Properties)
  and the **Start-button right-click** menu (Open / Search / Properties) — both
  on the dedicated `popup` layer-shell surface above the bar.
- **Toolkit**: `checkbox_style`, `radio_style`, native `tab_strip`, and
  `group_box` widgets all exist and are used by System Properties, Display, and
  Taskbar Properties (no more button-faked tabs).

## Still open

None of these block reviewing the shell; ranked by value:

- **System Properties**: System Restore / Automatic Updates remain thin (General,
  Computer Name, Hardware, Advanced, and the Remote toggle are live).
- **Device Manager**: covers Processors/Display/Disks/USB; missing some standard
  Win2000 categories, and USB lists raw `lsusb` lines.
- **Toolkit**: no progress-bar widget yet (the GUI installer hand-rolls its bar);
  the scrollbar lacks end-arrow buttons.
- **Window buttons**: app icons not shown; refresh is a 1 Hz poll, not a
  foreign-toplevel event subscription; middle-click close not wired (per-window
  close/maximize is labwc's titlebar by the compositor boundary).
- **Deliberately omitted** (would be dead controls, which the design refuses):
  the Run dialog's Browse… and the Shut Down dialog's Help button.

## Not shipped here (deliberate, gated, or needs you present)

- **The session cutover:** the labwc compositor migration is done in-tree (the
  window layer is wlr-foreign-toplevel, not sway IPC), and the shipped session is
  `mde/skel/mde-retro.desktop`. Pointing your *live* login at it is still an
  operator step done with you present — it replaces the running desktop.
- **The RPM cut:** the last packaging step (`cargo-generate-rpm`, see
  `skills/release`), operator-triggered after the platform is signed off.
- **Asset bundling:** the RPM ships **code-only**; `mde install --assets` fetches
  Chicago95 / Win2k / Haiku art from upstream at first run, so nothing GPL-3 /
  trademark-encumbered is redistributed (see `assets/licenses/NOTICE.md`).
- **Win-key → Start toggle:** a labwc keybind (`rc.xml`), not an mde concern; it
  belongs with the session cutover.
