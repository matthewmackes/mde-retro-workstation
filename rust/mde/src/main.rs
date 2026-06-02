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

mod about;
mod action_center;
mod apps;
mod catalogue;
mod control_panel;
mod dialogs;
mod display;
mod embedded_icons;
mod fedora;
mod filedialog;
mod files;
mod icons;
mod install;
mod installer;
mod menu;
mod notifyd;
mod outputs;
mod panel;
mod popup;
mod search;
mod settings;
mod start_common;
mod start_win10;
mod state;
mod sysinfo;
mod system_properties;
mod task_view;
mod taskbar_properties;
mod tray;
mod tui_setup;
mod wallpaper;
mod wlr;
mod workspace;

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
    display [--outputs]   Display Properties (resolution, wallpaper, screen saver)
    filedialog [--save] [--filter ...]   Common Open/Save file dialog (prints path)
    run              Run dialog (type a command to launch)
    properties NAME TARGET   Launcher/file Properties dialog
    system-properties [--info|--devices]   System facts / Device Manager data
    taskbar-properties   Taskbar and Start Menu Properties
    setup [--tui|--gui|--dry-run]   Install/configure MDE-Retro
    install [--assets]   Fetch Chicago95 + Win2k assets (first run)
    logoff           Log Off confirmation dialog
    shutdown         Shut Down dialog

    -h, --help       Show this help
    -V, --version    Show version
";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();

    // Select the shell look-and-feel from persisted state, once, up front, so
    // every subcommand's UI renders in the right theme. Each subcommand is its
    // own process, so this runs at every launch. Carbon (default) brings the IBM
    // Carbon palette + Plex font with a light/dark mode and an icon accent hue;
    // "win2000" keeps the classic look (and the Haiku icon set still maps to the
    // BeOS palette for back-compat).
    {
        use mde_ui::palette::{self, Theme};
        let st = state::load();
        match st.theme.as_str() {
            "win2000" => palette::set_theme(if st.icon_set == "haiku" {
                Theme::Beos
            } else {
                Theme::Win2000
            }),
            "windows10" => palette::set_theme(Theme::Windows10),
            _ => palette::set_theme(Theme::Carbon),
        }
        palette::set_dark(st.theme_mode != "light");
        palette::set_accent(match st.icon_color.as_str() {
            "blue" => 0,
            "orange" => 1,
            "red" => 2,
            _ => 3, // neutral
        });
        // The Windows 10 UI accent (selection/highlight) is its own slot (E7.1),
        // separate from the icon accent above.
        palette::set_win10_accent(st.win10_accent);
    }

    // Resolve the subcommand from argv[0] basename if it looks like `mde-foo`.
    let argv0 = args
        .first()
        .map(|p| {
            Path::new(p)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
        })
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
        "start-win10" => start_win10::run(rest),
        "action-center" => action_center::run_center(rest),
        "toast" => action_center::run_toast(rest),
        "task-view" => task_view::run(rest),
        "search" => search::run(rest),
        "settings" => settings::run(rest),
        // Shortcut: `mde personalization [--page <name>]` opens Settings straight
        // to the Personalization category (the desktop "Personalize" target).
        "personalization" => {
            let mut a = vec!["personalization".to_string()];
            a.extend_from_slice(rest);
            settings::run(&a)
        }
        // Per-era Start dispatcher for the labwc keybind: opens the right Start
        // for the active theme (the startup block above already set it).
        "start" => {
            start_common::mde_self(start_common::active_start_cmd());
            ExitCode::SUCCESS
        }
        "popup" => popup::run(rest),
        "files" => files::run(rest),
        "control-panel" => control_panel::run(rest),
        "display" => display::run(rest),
        "filedialog" => filedialog::run(rest),
        "run" => dialogs::run_dialog(),
        "properties" => {
            let name = rest.first().cloned().unwrap_or_default();
            let target = rest.get(1).cloned().unwrap_or_default();
            dialogs::properties(name, target)
        }
        "about" => about::run(rest),
        "system-properties" => system_properties::run(rest),
        "taskbar-properties" => taskbar_properties::run(rest),
        "__wlr-list" => {
            wlr::debug_list();
            ExitCode::SUCCESS
        }
        "__ws-list" => {
            workspace::debug_list();
            ExitCode::SUCCESS
        }
        "__ws-activate" => {
            if let Some(id) = rest.first().and_then(|s| s.parse().ok()) {
                workspace::debug_activate(id);
            }
            ExitCode::SUCCESS
        }
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
