# Accuracy harness — "accuracy is job 1"

Each theme's look is verified, not eyeballed. Note the **default theme is now
Carbon dark**, so a default-config capture is the Carbon palette, not the Win2000
`#3a6ea5` desktop — every capture must say which theme (`menu.json`
`theme`/`theme_mode`) it asserts, while the layer-1 checklist pins the Win2000
ground-truth values regardless of the active theme. The compositor that draws
title bars / frames / z-order is **labwc** (the capture harness nests headless
*sway* only because it's a convenient wlroots driver — see §2). Two layers, both
wired into `cargo test`:

| Layer | Test | Needs Wayland? |
| --- | --- | --- |
| 1 — static metric checklist | `cargo test -p mde-ui` (`mde-ui/tests/checklist.rs`) | no — gates every build |
| 2 — rendered screenshot spot-check | `cargo test --test accuracy` (`mde/tests/accuracy.rs`) | yes — skips when headless |

## 0. Who draws what — the mde ↔ labwc boundary

This is the project's central architectural fact, and every accuracy decision
flows from it, so it is stated here rather than left implicit:

- **labwc draws** every toplevel window's **title bar** and resize **frame**, and
  window **z-order/stacking**. The colors come from the shipped openbox-style
  theme `themes/Win2000-MDE/openbox-3/themerc` (focused title
  `window.active.title.bg.color: #0a246a`, border `#d4d0c8`) — *not* from `mde`.
  The desktop **background** (`#3a6ea5`) is painted by the wallpaper layer
  (`swaybg`), also outside mde.
- **mde draws** the client area inside each window, plus its own layer-shell
  surfaces: the **taskbar** (`mde panel`), the **Start-menu** (`mde menu`), and
  the **context-menu** popups (`mde popup`).

Two consequences worth naming, because they were silently decided once and
should be deliberate:

- The **gradient caption** (`palette::ACTIVE_TITLE_GRADIENT` → `#a6caf0`) is the
  known casualty: the themerc sets `window.active.title.bg: flat solid`, so the
  Win2000 navy→blue gradient is *not* rendered today. The constant is kept as
  recorded ground truth; the gradient only returns if mde ever draws its own
  client-side title rows (which it deliberately does **not** — that would make
  mde a window manager; see the "tensions to keep"). Flat navy is the conscious
  trade, not an oversight.
- `metrics::SIZE_FRAME` / `FIXED_FRAME` are likewise **labwc-owned today** —
  transcribed for completeness, applied by labwc's frame, not by mde.

The harness reflects this split: layer 2 already asserts one compositor-drawn
surface (the desktop background `#3a6ea5`). Asserting the title-bar navy / frame
silver means capturing *including* a window's decoration rect (not just the
client crop) — a planned extension so both halves of the desktop are verified.

## 1. Metric checklist (static)

`mde-ui` encodes the targets in code (`palette.rs`, `metrics.rs`).
`mde-ui/tests/checklist.rs` pins the ground truth so any accidental drift in a
color or metric fails CI; `widget/bevel.rs` additionally asserts the
raised/sunken mirror. The checklist below is what a rendered component must
satisfy (✓ = covered by a layer-1 test):

- [x] Desktop background `#3a6ea5`
- [x] Window frame silver `#d4d0c8`; sizing frame 3px, fixed frame 1px
- [x] Active title bar `#0a246a` → gradient to `#a6caf0`; height 18px; Tahoma Bold
- [x] Inactive title bar `#808080`
- [x] 3D bevel: raised = white/`#dfdfdf` (TL) over `#808080`/`#404040` (BR)
- [x] Selection / highlight `#0a246a`, text white
- [x] Taskbar height 28px, raised bevel; sunken clock well
- [x] Scrollbars 16px; menu rows 18px
- [x] UI font Tahoma 8pt everywhere

## 2. Screenshot spot-check (dynamic)

`tests/accuracy/` captures the live shell and asserts that the *rendered*
output paints the ground-truth colors at known coordinates — catching theming
regressions the static layer can't see.

```
tests/accuracy/
  refs/             reference Win2000 PNGs (desktop, explorer, menu, open-dialog)
  captures/         grim output (gitignored)
  checklist.toml    per-component spot-check points (coord + expected color + tol)
  capture.sh        launches each component and grim-captures it
```

Flow (run inside a Sway session):

```
cargo build
tests/accuracy/capture.sh all            # -> captures/desktop.png, panel.png, ...
cargo test --test accuracy -- --nocapture
```

`capture.sh` launches each component, lets it paint, and `grim`s the active
output. `mde/tests/accuracy.rs` then decodes each PNG (pure-Rust `png` crate)
and compares the pixel at every `[[capture.*.point]]` to its expected hex
within a per-channel tolerance. Coordinates are resolution-independent
(negative = from the far edge), so the same checklist works at any output size.

Why spot-check our own render instead of SSIM-diffing the `refs/` photos: the
reference screenshots are real Win2000 captures at a foreign resolution/DPI, so
a whole-image diff is dominated by scale/content misalignment, not color
fidelity. Asserting exact palette values at fixed points is both stricter on
what matters (the colors) and free of that noise. The `refs/` PNGs remain the
visual target for manual eyeballing and future per-region comparison.

> Skips automatically when `WAYLAND_DISPLAY` is unset (headless CI), and skips
> any component whose capture PNG hasn't been generated — so a partial capture
> run still verifies what it has.

**Capture in a controlled session.** The spot-check points assume the component
under test is unoccluded and the output is awake. Run `capture.sh` on a clean
MDE-Retro desktop (no other windows over the desktop/taskbar regions) or, best,
in a **nested** sway on a headless WLR output (`WLR_BACKENDS=headless sway`) so
the geometry is fixed and nothing covers the captured regions. A blanked/idle
screen (black framebuffer) or a foreign window sitting over a checkpoint will
make the affected point fail — by design; the harness is asserting what is
actually on screen. `capture.sh` issues a best-effort DPMS-on + pointer nudge
first, but it cannot move the user's windows out of the way.

Verified live (clean desktop, screen awake): desktop background, taskbar face,
and the raised-bevel highlight all matched the Win2000 palette exactly (Δ0), on
both the compositor's wallpaper layer and the Rust `mde panel`.
