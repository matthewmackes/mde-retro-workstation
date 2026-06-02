# MDE-Retro — Claude Workspace Instructions

- **Project:** MDE Retro Workstation — a native **Rust / iced** desktop shell for
  Wayland (**labwc**), with switchable **Windows 2000 Classic** and **IBM Carbon**
  themes. Repo `matthewmackes/mde-retro-workstation`, default branch `main`.
- **Worklist:** [`docs/PROJECT_WORKLIST.md`](../docs/PROJECT_WORKLIST.md) — the single source of truth for what's open.
- **Memory:** durable preferences live in Claude auto-memory; see `[[mde-rust-shell]]`, `[[mde-carbon-theme]]`, `[[labwc-migration-done]]`, `[[execute-all-no-order]]`.
- Adapted from the AI-guidance patterns in `github.com/matthewmackes/MDE` (`.claude/`), trimmed to this focused single-developer Rust shell.

This is an **operational rulebook**, not an architecture tour — architecture facts
live in §1, the rest is how to *act* in this repo. When rules conflict, the **newer
lock wins silently**; authority ranks **Memory > this file > `rust/SPEC-*.md` > worklist body**.

---

## §0 — Commit & Push Rulebook

- **§0.1 Separate authorizations.** Committing and pushing are each their own
  explicit ask. Do **not** commit or push unsolicited. "Save it", "ship it", or a
  `/ship` run authorizes commits; pushing still needs its own go-ahead.
- **§0.2 Branch policy.** Work on `main` (the only branch). For risky or
  outward-facing visual reworks, branch first (`carbon/<topic>`), then ask before
  merging. Never force-push `main`; never `--amend`/`--no-verify` a pushed commit.
- **§0.3 Explicit staging.** Stage named pathspecs — `git add -- <file>…` or
  `git commit <file>…`. **Never** `git add -A/./-u`: it sweeps unrelated in-flight
  edits (e.g. the pre-existing packaging `r5` work) into the wrong commit.
- **§0.4 Commit messages.** Read `git log` first to match voice. Explain **why,
  not what**. Verb taxonomy: `add` / `update` / `fix` / `refactor` / `packaging` /
  `docs`. Use a HEREDOC body to preserve newlines. End every message with:
  `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.
- **§0.5 Destructive ops need confirmation.** `git reset --hard`, `clean -f`,
  `rm -rf` outside `target/`, history rewrites, branch/remote deletion, and any
  GitHub state change (repo visibility, default branch, releases) are confirmed
  first unless the user just asked for exactly that.
- **§0.6 Outward-facing = confirm.** Pushing, publishing, opening PRs, anything
  third parties can see. Public is hard to reverse (indexing/caching). Secrets-scan
  before a first public push.

## §1 — Architecture (the load-bearing facts)

> **The prose READMEs lag the code.** `README.md`, `rust/PREVIEW.md`,
> `rust/ACCURACY.md`, `rust/DEV-SETUP.md` still say "rides on sway" and "Windows
> 2000 is the look". **Reality:** the live compositor is **labwc** (sway IPC was
> hard-cut to wlr-foreign-toplevel in `wlr.rs`), and the **default theme is Carbon
> dark.** Trust `state.rs` defaults, `palette.rs` `Theme`, `main.rs`, and
> `mde/Cargo.toml`, *not* the prose. (Fixing this drift is itself worklist work.)

- **Workspace:** the live project is `rust/` — a cargo workspace with two members.
  - **`mde-ui`** = the **look library** (no Wayland, no shell logic): `palette.rs`,
    `metrics.rs`, `font.rs`, `widget/` (`bevel.rs` 3D-edge data; `mod.rs` iced style
    fns + `draw_edge`/`fill`; `button.rs`/`frame.rs`/`groupbox.rs`/`infoband.rs`/`tabs.rs`).
  - **`mde`** = the **shell**: one multiplexed binary. `main.rs` dispatches
    subcommands from `argv[1]` (`mde panel`) or the `mde-<cmd>` symlink basename.
    Surfaces: `panel.rs`, `menu.rs`, `popup.rs` (layer-shell); `files.rs`,
    `control_panel.rs`, `display.rs`, `system_properties.rs`, `dialogs.rs`,
    `filedialog.rs`, `installer.rs`/`tui_setup.rs` (xdg-toplevels). Window control:
    `wlr.rs` (wlr-foreign-toplevel on a bg thread). Tray: `tray.rs` (SNI/zbus).
    Icons: `icons.rs` + `embedded_icons.rs`. State: `state.rs`.
- **Toolkit:** iced 0.13 (wgpu, image/svg/advanced/tokio) + iced_layershell 0.13;
  ratatui/crossterm (TUI installer); zbus (tray); pure-Rust text (cosmic-text, no
  freetype) and TLS (rustls, no openssl).
- **Theme system — one edge.** `palette.rs` holds Win2000 role colors as
  `Rgb=(u8,u8,u8)`. The **single** edge `palette::color(rgb) -> iced::Color` remaps
  per the active `Theme {Win2000, Beos, Carbon}` before producing the color, so no
  call site changes when the theme switches. **Carbon is the default (dark).**
  `main.rs` calls `set_theme`/`set_dark`/`set_accent` from state at startup. See
  `rust/SPEC-carbon-theme.md`.
- **Compositor boundary (load-bearing):** mde draws **only** client areas + its
  layer-shell surfaces. **labwc** draws title bars, frames, z-order. Never make mde
  draw client-side title rows — that would make it a window manager.

## §2 — Project conventions (the spine; accuracy is job 1)

- **§2.1 No raw hex outside `palette.rs`.** Nothing else may name an RGB/hex
  literal. All colors are palette role constants through `palette::color()` (the one
  theme-remap edge). App-chrome colors (`INFO_BAND`, `SETUP_GRADIENT_*`, `LOGO_*`)
  live in `palette.rs` too; `checklist.rs` pins them.
- **§2.2 Ground truth is pinned in tests.** `mde-ui/tests/checklist.rs` encodes the
  exact Win2000 palette + `SM_*` metrics from `assets/reference/win2000-classic-colors.ini`.
  Change a palette/metric value **only** with a reference to back it, and update the
  matching assertion in the same commit. The Carbon sentinels
  (`TITLE_TEXT`/`HIGHLIGHT_TEXT` = `0xff,0xff,0xfe`; `WINDOW_FRAME` = `0x00,0x00,0x01`)
  are intentional — don't "fix" them to pure white/black.
- **§2.3 Metrics are single-source.** Every `.size(...)` uses `metrics::UI_PX`
  (=11), never a scattered literal; `INFO_TITLE_PX` (16) is the one larger size.
- **§2.4 Don't launder the font gap.** The shipped UI font is **Droid Sans**
  (Win2000) / **IBM Plex Sans** (Carbon/BeOS), never Tahoma. `UI_FONT_TARGET="Tahoma"`
  records the unattainable target separately; assert the shipped substitute.
- **§2.5 Bevel in one place.** `bevel.rs` (data) + `draw_edge` (quads). `raised`/
  `sunken` stay exact mirror images (unit-tested). Carbon `draw_edge` flattens to a
  2px-radius 1px-bordered fill on purpose.
- **§2.6 State stays compatible.** `~/.config/mde/menu.json` (`state.rs`): every
  field `#[serde(default)]` with explicit default fns; the manual `Default` impl must
  agree with `parse("{}")` (a test enforces it); `save()` is atomic. Garbage → defaults.
- **§2.7 Icons via `icon_any`.** Embedded Carbon SVGs first, then the freedesktop
  chain; missing → empty `Space` (never tofu). New shell icons go in `embedded_icons.rs`.

## §3 — Definition of Done (no stubs, runtime-reachable)

Code existing is **never** "done". A change is done only when it is **reachable from a
runtime entry point and observably works**:

- Reachable from an `mde <subcommand>` path (or an iced `update`/`view`/subscription
  it feeds), and verified by launching it (`./preview.sh <component>` or a
  `timeout 3 ./target/debug/mde <sub>` no-panic check).
- **No stubs:** no `todo!()` / `unimplemented!()` / `panic!("not yet")`, no stub
  `match` arms, no `pub mod foo;` with zero external `foo::` refs, no commit body
  saying "wiring lands in a follow-up". If it can't ship complete in one commit,
  re-split at write time.
- **No mockups passing as features:** no `demo_data`/placeholder constants or
  "coming soon" strings standing in for real behavior.
- Builds clean (`cargo build`), `cargo test` green, and — for any **visual** change —
  confirmed against the reference via the accuracy harness / gallery (§4), not eyeballed once.
- **Refuse / surface** "just stub it", "scaffold the module", "phase 2 wires it".

## §4 — Build · test · preview · release (run from `rust/`)

```sh
cargo build            # debug → target/debug/mde   (preview.sh uses this)
cargo build --release  # lean release → target/release/mde
cargo test             # ACCURACY GATE: checklist.rs (static) + accuracy.rs (dynamic) + unit tests
cargo test -p mde-ui   # static layer-1 only (palette + metrics, always headless-safe)
cargo clippy --all-targets   # lint (treat warnings as work)
cargo fmt --all              # format

git config core.hooksPath .githooks   # ONE-TIME per clone: enable the rustfmt
                                       # pre-commit hook (.githooks/pre-commit),
                                       # which blocks a commit with fmt drift.

./preview.sh gallery   # screenshot gallery in an ISOLATED nested sway → tests/accuracy/captures/gallery/
./preview.sh verify    # run the accuracy harness the same isolated way
./preview.sh <comp>    # launch ONE component live: panel menu files control-panel system-properties run setup …

# Release (operator-triggered only — see skills/release):
tests/stage-rpm-assets.sh && cargo generate-rpm -p mde   # → target/generate-rpm/mde-*.rpm
```

> **Accuracy harness silently skips.** `mde/tests/accuracy.rs` passes early when
> `WAYLAND_DISPLAY` is unset or captures are absent — a green `cargo test` does **not**
> mean the render was verified. Run `./preview.sh gallery`/`verify` first for visual work.

## §5 — Worklist

`docs/PROJECT_WORKLIST.md` is the **one** durable tracker. In-session Task tools are a
scratchpad; the file wins on any divergence. `rust/SPEC-*.md` are design docs, **not** a
parallel worklist — lift actionable items out of them into the worklist.

Status legend (locked): **`[ ]` Open · `[>]` In Progress · `[✓]` Done · `[!]` Blocked.**
No `[~]` deferred and no `[x]` — flip to `[>]` before substantive edits; only `[✓]` when
the §3 Definition of Done holds. No silent deferrals.

## §6 — Autonomy

On "execute" / "continue" / "ship it": work the highest-priority open tasks first, run
independent work in parallel, mark `[>]` before editing, implement **fully** (§3), add
follow-up tasks for any debt, and keep going until a real obstacle. **Standing
authorization:** make best-choice decisions on loose specs (record the choice in the
commit body), move past blocked tasks, improve design "in the spirit", add worklist items.
**Still gated (stop and ask):** pushing, cutting an RPM/release, the labwc/session
cutover, and any §0.5 destructive op. Clarifying questions go one at a time via
multiple-choice (`AskUserQuestion`), per memory.

## §7 — Gotchas (hard-won; don't relearn these)

- **labwc `rc.xml` `<mouse>` MUST start with `<default/>`** — labwc treats `<mouse>` as a
  full replacement; without it every mousebind dies (windows unmovable, titlebar buttons dead).
- **Per-process theme load.** Each subcommand is its own process; `main.rs` re-reads
  `menu.json` and sets the palette at every launch. A theme change isn't live across
  already-running surfaces — relaunch them.
- **Layer-shell anchors are per-theme** in `panel.rs`: Carbon → **top** (32px),
  Win2000 → bottom (28px), BeOS → left (115px). `gallery.sh` crops the **top** strip
  `0,0 1280x40` to match the Carbon default (use `0,920` for a Win2000 capture).
- **Panel stays SVG-free** where possible — the first SVG render loads iced's ~20MB
  font DB. The Start icon is a raster PNG on purpose; tray glyphs use the system Nerd Font.
- **`state.json` is `~/.config/mde/menu.json`** (`state.rs::config_path`, honors `XDG_CONFIG_HOME`).
- **The script `install.sh` at the repo root is the OLD sway installer**, not the Rust
  release path. Packaging is `cargo-generate-rpm` (see skills/release).

## §8 — File index

| Path | What |
|---|---|
| `rust/mde-ui/src/palette.rs` | color roles + the `color()` theme-remap edge (the one place hex lives) |
| `rust/mde-ui/src/metrics.rs` · `font.rs` | `SM_*` metrics · UI font families |
| `rust/mde-ui/src/widget/` | 3D bevel + iced widget styles (flat under Carbon) |
| `rust/mde-ui/tests/checklist.rs` | pinned Win2000 ground-truth (palette + metrics) |
| `rust/mde/src/main.rs` | subcommand dispatch + startup theme select |
| `rust/mde/src/{panel,menu,popup}.rs` | layer-shell surfaces |
| `rust/mde/src/{files,control_panel,display,system_properties,dialogs}.rs` | app windows |
| `rust/mde/src/{icons,embedded_icons,state}.rs` | icon resolution/tint · persisted state |
| `rust/SPEC-*.md` | design specs (carbon-theme, installer, rebrand, startmenu, system) |
| `rust/tests/accuracy/` + `preview.sh` | the screenshot accuracy harness + preview launcher |
| `docs/PROJECT_WORKLIST.md` | the single worklist |
