# MDE-Retro Rust shell

A native **Rust** desktop shell. Runs on **labwc** (a wlroots stacking
compositor; the old sway IPC was hard-cut to wlr-foreign-toplevel in `wlr.rs`),
and replaces the Python/shell scripts, Waybar, and wofi with one lean binary. It
ships **three themes** — **IBM Carbon** (the default, dark), Windows 2000 Classic,
and BeOS — switchable in Display ▸ Appearance.

> Status: **live** (the labwc cutover is done; this shell is the product on
> `main`). The taskbar, Start menu, file manager, Control Panel, Display, System
> Properties, dialogs and the Setup installer (GUI + headless TUI) are built and
> run. Needs a Rust toolchain — see [`DEV-SETUP.md`](DEV-SETUP.md).

## Workspace

| Crate     | What                                                                 |
| --------- | ------------------------------------------------------------------- |
| `mde-ui`  | the look library: role palette + theme-remap edge (Win2000/Carbon/BeOS), metrics, 3D-bevel/flat widget model (iced) |
| `mde`     | the single `mde` binary: `panel`, `menu`, `files`, `control-panel`, `setup`, `install`, `run`, `logoff`, `shutdown` |

- **Toolkit:** iced (pure Rust, wgpu). Taskbar + Start menu use `iced_layershell`
  (wlr-layer-shell); the file manager is a normal xdg-toplevel window.
- **Look:** three themes routed through one `palette::color()` edge — **IBM
  Carbon** (default, dark; flat surfaces, IBM Plex Sans), Windows 2000 Classic
  (palette/metrics transcribed from `../assets/reference/win2000-classic-colors.ini`),
  and BeOS. Verified by the [accuracy harness](ACCURACY.md).
- **Binary:** one `mde` multiplexed by subcommand (or by `mde-*` symlink).
- **Packaging:** `cargo generate-rpm -p mde` (code-only RPM; assets fetched on
  first run via `mde install --assets`).

## Build

```sh
cd rust
cargo build --release      # -> target/release/mde
cargo test                 # static accuracy checklist (+ screenshot spot-check in a Wayland session)
cargo generate-rpm -p mde  # -> target/generate-rpm/mde-*.rpm
```

`cargo test` is the accuracy gate: the static checklist (`mde-ui/tests/checklist.rs`)
pins the Win2000 palette/metrics so a typo fails the test, not the user's eye, and
[`.github/workflows/ci.yml`](../.github/workflows/ci.yml) runs it on every push. The
dynamic screenshot layer needs a Wayland session and is run with `capture.sh` (see
[`ACCURACY.md`](ACCURACY.md)).

## Cutover (done)

The cutover happened: the session is **labwc** launching `mde panel` / `mde menu`
/ `mde files` (see `mde/skel/mde-retro.desktop` `Exec=labwc` and
`skel/.config/labwc/`), the Rust shell is the product on `main`, and window
control is wlr-foreign-toplevel (`wlr.rs`), not sway IPC. The old script-based
sway desktop lives on only as the legacy `../install.sh` path.
