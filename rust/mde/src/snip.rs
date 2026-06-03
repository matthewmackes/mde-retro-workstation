//! Snip & Sketch-style screenshot tool (E16.4/E16.5), the Win+Shift+S surface.
//!
//! `mde snip <mode>` captures the screen through one shared [`capture`] path. The
//! cursor is never grabbed (grim excludes it unless `-c`, which we never pass):
//!   - **rect** (default) — pick a region with `slurp`, save + copy.
//!   - **full** / **screen** — the whole output, save + copy (the headless one).
//!   - **clip** — the whole output to the **clipboard only**, no file written.
//!   - **window** — raise the focused toplevel (via [`crate::wlr`], since the
//!     foreign-toplevel protocol exposes no window rect) then `slurp` a drag over
//!     it; save + copy.
//!
//! Saved shots land in `~/Pictures/Screenshots/` and are copied to the clipboard as
//! `image/png` (so the clipboard daemon, E16.2, also records them). Win10-era only.

use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};
use std::time::Duration;

/// The snip capture modes (E16.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Rect,
    Full,
    Clip,
    Window,
}

/// Parse the first positional arg into a mode (pure, unit-tested). Default `Rect`.
pub fn mode_from_args(args: &[String]) -> Mode {
    match args
        .iter()
        .find(|a| !a.starts_with('-'))
        .map(String::as_str)
    {
        Some("full") | Some("screen") => Mode::Full,
        Some("clip") => Mode::Clip,
        Some("window") => Mode::Window,
        _ => Mode::Rect,
    }
}

/// `~/Pictures/Screenshots/`.
fn shots_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Pictures").join("Screenshots"))
}

/// The screenshot filename for an epoch-seconds stamp (pure, unit-tested).
fn shot_name(epoch: u64) -> String {
    format!("Screenshot_{epoch}.png")
}

fn epoch_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Pick a region with `slurp`; `None` if it was cancelled (empty / non-zero exit).
fn region() -> Option<String> {
    let o = Command::new("slurp").output().ok()?;
    if !o.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// Raise the focused window, then slurp a drag over it. The foreign-toplevel
/// protocol carries no geometry, so we can't auto-crop — bringing the window
/// forward and letting the user drag is the honest best (E16.5).
fn window_region() -> Option<String> {
    if let Some(wm) = crate::wlr::start() {
        std::thread::sleep(Duration::from_millis(400)); // let the listener populate
        if let Some(w) = wm.windows().into_iter().find(|w| w.focused) {
            wm.focus(w.id);
            std::thread::sleep(Duration::from_millis(150)); // let the raise land
        }
    }
    region()
}

pub fn run(args: &[String]) -> ExitCode {
    if !mde_ui::palette::is_windows10() {
        eprintln!("mde snip: the snipping tool is a Windows 10-era surface.");
        return ExitCode::SUCCESS;
    }
    capture(mode_from_args(args))
}

/// The one capture path for every mode.
pub fn capture(mode: Mode) -> ExitCode {
    let geom = match mode {
        Mode::Rect => match region() {
            Some(g) => Some(g),
            None => return ExitCode::SUCCESS, // user cancelled
        },
        Mode::Window => match window_region() {
            Some(g) => Some(g),
            None => return ExitCode::SUCCESS,
        },
        Mode::Full | Mode::Clip => None,
    };

    // Clipboard-only: pipe grim's PNG straight into wl-copy, no file on disk.
    if mode == Mode::Clip {
        return capture_to_clipboard(geom.as_deref());
    }

    let Some(dir) = shots_dir() else {
        eprintln!("mde snip: no HOME");
        return ExitCode::FAILURE;
    };
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("mde snip: {e}");
        return ExitCode::FAILURE;
    }
    let path = dir.join(shot_name(epoch_now()));

    let mut grim = Command::new("grim");
    if let Some(g) = &geom {
        grim.args(["-g", g]);
    }
    grim.arg(&path);
    if !grim.status().map(|s| s.success()).unwrap_or(false) {
        eprintln!("mde snip: grim failed");
        return ExitCode::FAILURE;
    }

    // Also copy to the clipboard as image/png (so it pastes + the daemon records it).
    if let Ok(f) = std::fs::File::open(&path) {
        let _ = Command::new("wl-copy")
            .args(["--type", "image/png"])
            .stdin(f)
            .status();
    }
    let _ = Command::new("notify-send")
        .args(["Screenshot saved", &path.display().to_string()])
        .spawn();
    println!("{}", path.display());
    ExitCode::SUCCESS
}

/// `grim - | wl-copy --type image/png` — capture to the clipboard with no file.
fn capture_to_clipboard(geom: Option<&str>) -> ExitCode {
    let mut grim = Command::new("grim");
    if let Some(g) = geom {
        grim.args(["-g", g]);
    }
    grim.arg("-").stdout(Stdio::piped());
    let mut grim_child = match grim.spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("mde snip: grim failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    let Some(out) = grim_child.stdout.take() else {
        let _ = grim_child.wait();
        return ExitCode::FAILURE;
    };
    let wl = Command::new("wl-copy")
        .args(["--type", "image/png"])
        .stdin(Stdio::from(out))
        .status();
    let grim_ok = grim_child.wait().map(|s| s.success()).unwrap_or(false);
    if grim_ok && wl.map(|s| s.success()).unwrap_or(false) {
        let _ = Command::new("notify-send")
            .args(["Screenshot copied", "Copied to the clipboard"])
            .spawn();
        ExitCode::SUCCESS
    } else {
        eprintln!("mde snip: clipboard capture failed");
        ExitCode::FAILURE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn screenshot_filename() {
        assert_eq!(shot_name(1_700_000_000), "Screenshot_1700000000.png");
        assert!(shot_name(0).ends_with(".png"));
    }

    #[test]
    fn mode_parsing() {
        assert_eq!(mode_from_args(&args("")), Mode::Rect);
        assert_eq!(mode_from_args(&args("rect")), Mode::Rect);
        assert_eq!(mode_from_args(&args("full")), Mode::Full);
        assert_eq!(mode_from_args(&args("screen")), Mode::Full);
        assert_eq!(mode_from_args(&args("clip")), Mode::Clip);
        assert_eq!(mode_from_args(&args("window")), Mode::Window);
        // A leading flag is skipped; the positional decides.
        assert_eq!(mode_from_args(&args("--foo clip")), Mode::Clip);
        assert_eq!(mode_from_args(&args("garbage")), Mode::Rect);
    }
}
