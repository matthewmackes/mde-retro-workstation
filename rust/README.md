# MDE-Retro Rust shell

A native **Rust** rewrite of the MDE-Retro Windows 2000 desktop shell. Runs on
top of **sway** (sway stays the compositor); replaces the Python/shell scripts,
Waybar, and wofi with one lean binary.

> Status: **in development** on branch `rust-shell`. The taskbar, Start menu,
> file manager, Control Panel, Log Off / Shut Down dialogs and the Setup
> installer (GUI + headless TUI) are built and run; remaining work is fidelity
> polish and the RPM/cutover (the final, gated step). Needs a Rust toolchain —
> see [`DEV-SETUP.md`](DEV-SETUP.md).

## Workspace

| Crate     | What                                                                 |
| --------- | ------------------------------------------------------------------- |
| `mde-ui`  | Win2000 Classic palette, metrics, and the 3D-bevel widget model (iced) |
| `mde`     | the single `mde` binary: `panel`, `menu`, `files`, `control-panel`, `setup`, `install`, `run`, `logoff`, `shutdown` |

- **Toolkit:** iced (pure Rust, wgpu). Taskbar + Start menu use `iced_layershell`
  (wlr-layer-shell); the file manager is a normal xdg-toplevel window.
- **Look:** Windows 2000 Classic — palette/metrics transcribed from
  `../assets/reference/win2000-classic-colors.ini`; verified by the
  [accuracy harness](ACCURACY.md).
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

## Cutover (big-bang)

When the components are done, the sway `config` swaps the script/Waybar/wofi
launchers for `mde panel` / `mde menu` / `mde files`, and `rust-shell` merges to
`main`. Until then `main` remains the working script-based desktop.
