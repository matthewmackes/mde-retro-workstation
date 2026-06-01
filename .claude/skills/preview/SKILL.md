---
name: preview
description: >-
  Render and verify MDE-Retro visually in an isolated nested sway — the accuracy
  harness. TRIGGER when the user wants to "preview", "show the gallery",
  "screenshot the shell", "verify the render", or confirm a visual change actually
  looks right (Carbon or Win2000). Use this instead of trusting a green `cargo test`
  for any UI change.
---

# preview — render & accuracy verification (MDE-Retro)

The dynamic accuracy harness **silently skips** when headless (`mde/tests/accuracy.rs`
returns early with no `WAYLAND_DISPLAY` / no captures), so `cargo test` alone never
verifies the render. This skill drives the real visual check.

## Commands (run from `rust/`)

```sh
./preview.sh gallery     # regenerate ALL component screenshots in an isolated
                         # nested sway → tests/accuracy/captures/gallery/*.png
                         #   (+ _contact-sheet.png). No effect on the live desktop.
./preview.sh verify      # run the accuracy harness the same isolated way
./preview.sh <component> # launch ONE live on the current session, click around,
                         # then kill it. Components: panel menu files control-panel
                         #   system-properties run properties logoff shutdown setup
```

## How to use

1. Build first (`cargo build`) — `preview.sh` runs `target/debug/mde`.
2. `./preview.sh gallery`, then **Read the PNGs** (`tests/accuracy/captures/gallery/`)
   and confirm the change against the intent. The default theme is **Carbon dark**;
   to check Win2000 or light/blue, set `~/.config/mde/menu.json`
   (`theme`/`theme_mode`/`icon_color`) before rendering and **restore it after**.
3. **Theme-aware capture:** the panel/bar anchors differ by theme. `gallery.sh`
   crops the **top** strip (`0,0 1280x40`) for the Carbon top bar; use `0,920` for a
   Win2000 bottom taskbar. The Start menu / dialogs are captured full.
4. For a static-only check (palette + metric ground truth, always headless-safe):
   `cargo test -p mde-ui`.

## Notes

- Captures assume the component is unoccluded and the screen is awake; a blanked
  screen or an overlapping window fails by design.
- `refs/*.png` are foreign-DPI real Win2000 shots for **eyeballing only** — never
  SSIM-diffed.
