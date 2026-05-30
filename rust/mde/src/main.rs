//! `mde` — the single multiplexed entry point for the MDE-Retro Rust shell.
//!
//! Dispatches to a subcommand either by the first argument (`mde panel`) or by
//! the binary's own name when invoked through a symlink (`mde-panel` ->
//! `mde`). One binary keeps the install lean; the heavy UI code lives behind
//! these subcommands.
//!
//! The subcommands are scaffolded; each prints a not-implemented notice until
//! its component lands (see the rust-shell tasks).

use std::env;
use std::path::Path;
use std::process::ExitCode;

mod control_panel;
mod files;
mod install;
mod menu;
mod panel;
mod sway;

const USAGE: &str = "\
mde — Windows 2000 desktop shell for Sway (MDE-Retro)

USAGE:
    mde <COMMAND> [ARGS...]
    mde-<command>            (when invoked via symlink)

COMMANDS:
    panel            Taskbar: Start button, window buttons, tray, clock
    menu [MODE]      Start menu (modes: main, programs, system, run)
    files [PATH]     Explorer-style file manager
    control-panel    Windows 2000 Control Panel
    install [--assets]   Fetch Chicago95 + Win2k assets (first run)

    -h, --help       Show this help
    -V, --version    Show version
";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();

    // Resolve the subcommand from argv[0] basename if it looks like `mde-foo`.
    let argv0 = args
        .first()
        .map(|p| Path::new(p).file_name().and_then(|s| s.to_str()).unwrap_or(""))
        .unwrap_or("");
    let (cmd, rest): (&str, &[String]) = if let Some(sub) = argv0.strip_prefix("mde-") {
        (sub, &args[1..])
    } else {
        match args.get(1) {
            Some(c) => (c.as_str(), &args[2..]),
            None => ("help", &[]),
        }
    };

    match cmd {
        "panel" => panel::run(rest),
        "menu" => menu::run(rest),
        "files" => files::run(rest),
        "control-panel" => control_panel::run(rest),
        "install" => install::run(rest),
        "-V" | "--version" => {
            println!("mde {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        "-h" | "--help" | "help" => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("mde: unknown command '{other}'\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// Shared placeholder used by the scaffolded subcommands.
pub(crate) fn not_implemented(name: &str) -> ExitCode {
    eprintln!("mde {name}: not yet implemented (rust-shell scaffold).");
    ExitCode::from(69) // EX_UNAVAILABLE
}
