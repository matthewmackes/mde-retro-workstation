//! First-run asset installer (`mde install --assets`).
//!
//! Per locked decision #7, the RPM ships CODE ONLY — the binary plus the asset
//! *installer scripts* (which are code). The visual assets themselves are
//! fetched from upstream at runtime so their licenses travel with the bytes and
//! nothing third-party is redistributed:
//!   * Chicago95 (icons/cursors/sounds/GTK theme) — github grassmunk/Chicago95
//!   * Win2k icon theme                            — KDE-Store item 1120706
//!
//! This is a *per-user* operation: the orchestrator deploys into the caller's
//! `~/.local/share`, and the Win2k step reads the cached tarball + generates
//! its aliases under `~/.config/labwc` — so the config tree must be deployed
//! first (the system installer does that, then triggers this per user).
//!
//! Usage:
//!   mde install [--assets] [--only chicago95|win2k] [--dry-run]

use std::path::PathBuf;
use std::process::{Command, ExitCode};

const USAGE: &str = "\
mde install — fetch the MDE-Retro visual assets (per user)

USAGE:
    mde install [--assets] [--only chicago95|win2k] [--dry-run]

Fetches Chicago95 + the Win2k icon theme from upstream into ~/.local/share
(nothing is redistributed by the RPM). Run after the config tree is deployed.";

pub fn run(args: &[String]) -> ExitCode {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let dry = args.iter().any(|a| a == "--dry-run");
    let only = args
        .iter()
        .position(|a| a == "--only")
        .and_then(|i| args.get(i + 1))
        .cloned();
    if let Some(o) = &only {
        if o != "chicago95" && o != "win2k" {
            eprintln!("mde install: --only takes 'chicago95' or 'win2k', got '{o}'");
            return ExitCode::from(2);
        }
    }

    let Some(script) = locate_orchestrator() else {
        eprintln!(
            "mde install: asset installer not found.\n\
             Looked in /usr/share/mde/scripts and the dev tree. On an installed\n\
             system this ships with the `mde` RPM; in a checkout, run from the repo."
        );
        return ExitCode::FAILURE;
    };

    if dry {
        println!("mde install --assets (dry run)");
        println!("  orchestrator : {}", script.display());
        match &only {
            Some(o) => println!("  scope        : --only {o}"),
            None => println!("  scope        : Chicago95 + Win2k icon theme"),
        }
        println!("  deploys into : ~/.local/share/{{icons,themes,sounds}} (this user)");
        println!("  source       : fetched from upstream at runtime (not redistributed)");
        return ExitCode::SUCCESS;
    }

    let mut cmd = Command::new("bash");
    cmd.arg(&script);
    if let Some(o) = only {
        cmd.arg("--only").arg(o);
    }
    match cmd.status() {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        Ok(s) => {
            eprintln!("mde install: asset installer exited with {s}");
            ExitCode::from(s.code().unwrap_or(1).clamp(1, 255) as u8)
        }
        Err(e) => {
            eprintln!("mde install: failed to run {}: {e}", script.display());
            ExitCode::FAILURE
        }
    }
}

/// Find `install-assets.sh`: the RPM ships it under `/usr/share/mde/scripts`;
/// in a dev checkout it lives at `<repo>/assets/`, next to the `rust/` tree.
fn locate_orchestrator() -> Option<PathBuf> {
    let mut candidates = vec![
        PathBuf::from("/usr/share/mde/scripts/install-assets.sh"),
        PathBuf::from("/usr/share/mde/assets/install-assets.sh"),
    ];
    if let Ok(exe) = std::env::current_exe() {
        // exe = <repo>/rust/target/<profile>/mde -> ancestors().nth(3) = <repo>/rust
        if let Some(rust_dir) = exe.ancestors().nth(3) {
            candidates.push(rust_dir.join("../assets/install-assets.sh"));
        }
    }
    candidates.into_iter().find(|p| p.exists())
}
