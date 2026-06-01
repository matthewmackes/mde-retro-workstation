# MDE-Retro — Compliance Evaluation

First sweep against `.claude/CLAUDE.md`, run via the `/audit` skill (6 parallel
passes). Verdicts: **FINISH** = wire up / make real / fix; **REMOVE** = delete dead
surface; **OK** = false positive (reachable / exempt). Generated 2026-06-01.

**Score: 8 REMOVE · 29 FINISH · 7 OK.** The platform is structurally sound — no
real stubs, the compositor boundary is clean, asset staging is consistent — but
**the prose docs badly lag the code** (15 findings) and the **single-color-edge
rule (§2.1) has leaked** in a handful of spots. Resolution status is tracked in
`docs/PROJECT_WORKLIST.md`; ✅ = fixed in the reorg pass, ☐ = open.

---

## 1. Documentation drift (§1) — 15 × FINISH  ✅ fixed in reorg

The headline failure. README/PREVIEW/ACCURACY/DEV-SETUP/LABWC-MIGRATION still
describe a **sway-based, Windows-2000-default, pre-migration** project. Reality
(confirmed in code): compositor is **labwc** (RPM `requires` labwc; `Exec=labwc`
session; `wlr.rs` foreign-toplevel; zero `swaymsg` in `src`); the **default theme
is Carbon dark** (`state.rs` `def_theme="carbon"`, `palette.rs` `DARK=1`, `main.rs`
falls through to `Theme::Carbon`); and the **labwc migration is DONE** (every
phase has a code landing) despite `LABWC-MIGRATION.md` saying "PLAN ONLY".

| Location | Stale claim |
|---|---|
| `README.md` (intro, badges, install) | "desktop for Sway", `wm-sway` badge, `dnf install sway`, sway session |
| `rust/README.md` (intro, table, Cutover §) | "Runs on top of sway", "Win2000 Classic" as the look, "cutover pending / main is script desktop" |
| `rust/PREVIEW.md` (intro, "Not in this preview", test counts) | "riding on top of sway", "not yet cut over / sway config flip", stale `cargo test` counts |
| `rust/ACCURACY.md` (§0 boundary, spot-check) | "mde ↔ **sway** boundary", "the Win2000 look is verified" as the only theme |
| `rust/DEV-SETUP.md` (caveat, deps) | "authored without a live toolchain", missing labwc runtime deps |
| `rust/LABWC-MIGRATION.md` (status, framing) | "**PLAN ONLY**", "Today (sway) → labwc", "drives swaymsg in six files" (now zero) |

## 2. Unreachable / dead code (§3) — 8 × REMOVE

| Location | What | Status |
|---|---|---|
| `mde-ui/src/widget/flag.rs` (+ mod.rs, lib.rs export) | whole Win2000 flag widget — superseded by the raster PNG Start icon; orphans `LOGO_*` | ✅ removed |
| `mde-ui/src/widget/mod.rs` `progress_style` | style fn with no `progress_bar` consumer | ✅ removed |
| `mde-ui/src/palette.rs` `use_beos` | dead back-compat shim (BeOS now via `set_theme`) | ✅ removed |
| `mde-ui/src/widget/frame.rs` `frame::pressed()` | unused constructor (≠ live `Bevel::pressed`) | ✅ removed |
| `mde/src/tui_setup.rs` `_gauge` | dead TUI helper, already `#[allow(dead_code)]` | ✅ removed |
| `mde/src/wlr.rs` `Wm::close` / `Wm::set_maximized` | architecturally unreachable (labwc owns titlebar) — *keep as protocol API or remove?* | ☐ decision |
| `mde/src/outputs.rs` `Output::make` | write-only EDID field | ☐ open |

## 3. Convention violations (§2) — 10 × FINISH

**§2.1 raw hex outside `palette.rs`:**
| Location | Leak | Status |
|---|---|---|
| `panel.rs:178` | Carbon header Gray 100 / white `from_rgb8` | ✅ → `palette::SHELL_HEADER` role |
| `control_panel.rs:362` | disabled gray `0x70` | ✅ → `palette::GRAY_TEXT` |
| `display.rs:795` | desktop teal `0x3a6e6e` | ✅ → `palette::BACKGROUND` |
| `icons.rs:154` | 8-entry Carbon icon-accent table inline | ☐ open (move to palette) |

**§2.2 ground-truth not pinned in `checklist.rs`:**
| Constant | Status |
|---|---|
| `WINDOW_FRAME` sentinel `(0,0,1)` | ✅ pinned |
| `TITLE_TEXT` sentinel `(0xff,0xff,0xfe)` | ✅ pinned |
| `INFO_TITLE_PX = 16` | ✅ pinned |
| `TASKBAR_BUTTON_MIN = 160` | ☐ open (low) |

**§2.3 raw `.size()` literals:** `display.rs:756` (`48.0`), `installer.rs:196,240`
(`10.0`/`15.0`) → ☐ open (add named `metrics` constants).

## 4. Mockups passing as features (§3) — 2 × FINISH  ☐ open

- `display.rs` Effects tab: 3 enabled checkboxes (`ToggleFx*`) write state that is
  never read/persisted. → grey out (`cbox_disabled`) or persist via `state.rs`.
- `taskbar_properties.rs`: "Show clock" + "Use Personalized Menus" enabled but
  discarded. → grey out (matches the file's own honest pattern) or wire.

## 5. Packaging (§4 / release skill) — 2 × FINISH

- ☐ **License gap:** `font.rs` embeds **DroidSans (Apache-2.0)** into the shipped
  binary, but `assets/licenses/` + `NOTICE.md` cover everything *except* Droid.
  → add `DroidSans-Apache-2.0.txt` + a NOTICE entry (IBM Plex, embedded the same
  way, *is* covered).
- ✅ **CLI parity:** `%post`/`%postun` symlinks now include `mde-display`,
  `mde-filedialog`, `mde-taskbar-properties`, matching the documented subcommand set.

## 6. Confirmed OK (no action)

Compositor boundary clean (the only "Active Window" caption is the Display-Props
mock preview); themerc/openbox hex are compositor config (labwc owns title bars);
the installer translucent-black scrim and the menu banner SVG art are documented
§2.1 carve-outs; `__wlr-list` correctly has no symlink; asset staging matches the
asset list exactly; `Workgroup="WORKGROUP"` is authentic read-only display.
