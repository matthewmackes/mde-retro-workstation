# MDE-Retro — Rust shell

A native **Rust** (iced toolkit, no GTK) desktop shell — the live product, running
on **labwc** and defaulting to the **IBM Carbon** theme (Windows 2000 selectable).
Every component is screenshot-verified in an **isolated headless sway** (the nested
test harness still uses sway because it works on any wlroots compositor — that's a
test-rig detail, not the runtime). The labwc cutover is **done**; this is no longer
a "preview". The note below kept a list of pre-cutover gaps that are now closed.

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
plus `_contact-sheet.png`). Nothing in `preview.sh` edits your sway config or
installs packages.

## What's in the preview

One multiplexed binary, `mde <subcommand>`, sharing the `mde-ui` Win2000 look
library (palette + metrics + 3D-bevel widgets):

| Component | `mde` command | State |
|-----------|---------------|-------|
| Taskbar (Start, Quick Launch, window buttons, clock) | `panel` | layer-shell, live sway IPC |
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

`cargo test` (2026-05-30): **green** — 12 mde unit + 15 mde-ui checklist
(palette + SM_* metrics) + 1 accuracy + 1 lib, 0 failed. Every component
was also captured in an isolated headless sway and eyeballed against the
references (`./preview.sh gallery`); the desktop background reads pixel-exact
`#3a6ea5`.

## Known gaps in the preview

- **Icons** now render (folders/files in Explorer, applets in Control Panel,
  Start-menu entries, the info-band watermark) by resolving the installed
  Win2k → Chicago95 → hicolor theme. Misses fall back to blank space. The
  *art* still depends on the asset fetch, so on a machine without those
  themes installed the shell falls back to text-and-bevel cleanly.
- **System tray** (StatusNotifierItem) and the **taskbar right-click menu**
  are not present (see below).

## Still open (from a multi-agent audit of the branch)

A feature-completeness/accuracy audit (per-component finders + adversarial
verifiers) confirmed the fixes above and flagged these as still open. None
block reviewing the preview; ranked by value:

- **System tray** (StatusNotifierItem/DBus) — a subsystem, not a render.
- **Taskbar right-click menu** (Cascade/Tile/Task Manager/Properties) and the
  **Start-button right-click** menu — need a separate popup surface above the
  28px layer-shell bar.
- **System Properties**: Advanced / System Restore / Automatic Updates / Remote
  are labelled placeholders (General, Computer Name, Hardware are live).
- **Device Manager**: covers Processors/Display/Disks/USB; missing some standard
  Win2000 categories, and USB lists raw `lsusb` lines.
- **Toolkit**: no checkbox / radio / native tab / progress-bar / group-box
  widgets yet (System Properties fakes tabs with buttons; the GUI installer
  hand-rolls its progress bar); the scrollbar lacks end-arrow buttons.
- **Window buttons**: app icons not shown; refresh is a 1 Hz poll, not a sway
  event subscription; right/middle-click close not wired.
- **Deliberately omitted** (would be dead controls, which the design refuses):
  the Run dialog's Browse… and the Shut Down dialog's Help button.

## Not in this preview (deliberate, gated, or needs you present)

- **The sway cutover** (`#9`): flipping `~/.config/sway/config` to launch the
  Rust shell — disruptive to the live desktop, done with you present.
- **The RPM cut** (`#13`): the last packaging step, after the platform is
  signed off.
- **Asset-bundling decision (open):** locked decision #7 says the RPM ships
  CODE ONLY and `mde install --assets` *fetches* Chicago95+Win2k at first run;
  `SPEC-installer.md` assumes assets are *bundled* offline. These conflict and
  must be resolved before the RPM. Recommended: keep code-only fetch and have
  `mde setup` call the fetch path (avoids redistributing GPL-3 / Win2k art).
- **System tray** (StatusNotifierItem/DBus): a separate subsystem, not a render.
- **Win-key → Start toggle:** edits the live sway config; belongs with the cutover.
