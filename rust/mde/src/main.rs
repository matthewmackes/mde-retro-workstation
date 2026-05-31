//! `mde` — the single multiplexed entry point for the MDE-Retro Rust shell.
//!
//! Dispatches to a subcommand either by the first argument (`mde panel`) or by
//! the binary's own name when invoked through a symlink (`mde-panel` ->
//! `mde`). One binary keeps the install lean; the heavy UI code lives behind
//! these subcommands.
//!
//! All subcommands are implemented; see [`USAGE`] for the full set.

use std::env;
use std::path::Path;
use std::process::ExitCode;

mod apps;
mod control_panel;
mod dialogs;
mod fedora;
mod files;
mod icons;
mod install;
mod installer;
mod menu;
mod panel;
mod popup;
mod state;
mod sway;
mod sysinfo;
mod tray;
mod system_properties;
mod tui_setup;

const USAGE: &str = "\
mde — Windows 2000 desktop shell for Sway (MDE-Retro)

USAGE:
    mde <COMMAND> [ARGS...]
    mde-<command>            (when invoked via symlink)

COMMANDS:
    panel            Taskbar: Start button, window buttons, tray, clock
    menu [MODE]      Start menu (modes: main, programs, system, run)
    popup KIND       Context menu (kinds: taskbar, start) for the panel
    files [PATH]     Explorer-style file manager
    control-panel    Windows 2000 Control Panel
    run              Run dialog (type a command to launch)
    properties NAME TARGET   Launcher/file Properties dialog
    system-properties [--info|--devices]   System facts / Device Manager data
    setup [--tui|--gui|--dry-run]   Install/configure MDE-Retro
    install [--assets]   Fetch Chicago95 + Win2k assets (first run)
    logoff           Log Off confirmation dialog
    shutdown         Shut Down dialog

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
        "popup" => popup::run(rest),
        "files" => files::run(rest),
        "control-panel" => control_panel::run(rest),
        "run" => dialogs::run_dialog(),
        "properties" => {
            let name = rest.first().cloned().unwrap_or_default();
            let target = rest.get(1).cloned().unwrap_or_default();
            dialogs::properties(name, target)
        }
        "system-properties" => system_properties::run(rest),
        "logoff" => dialogs::logoff(),
        "shutdown" => dialogs::shutdown(),
        "setup" => installer::dispatch(rest),
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
