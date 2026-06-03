//! The Windows 10 taskbar **Firefox jump list** (E18.3): a flat, themed
//! layer-shell popup of the browser's quick tasks. It reuses `popup.rs`'s
//! layer-shell launcher (no title bar, era-anchored), so only the browser-specific
//! item list lives here. The Recent section — Firefox history from
//! `places.sqlite` — lands in E18.4, inserted between Tasks and the footer.
//!
//!   mde browser-jumplist   open the Firefox jump list (panel right-click, E18.6)

use std::process::ExitCode;

use crate::popup::{launch_with, sep, Item};

pub fn run(args: &[String]) -> ExitCode {
    // No compositor → nothing to anchor to; exit cleanly (the popup is normally
    // spawned by the panel), matching popup.rs.
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return ExitCode::SUCCESS;
    }
    // `--pin <idx>` (E18.6) identifies which Quick-Launch pin was right-clicked.
    // Accepted (validated) for forward-compat per-pin anchoring; the popup currently
    // edge-anchors near the taskbar (the pin sits there on the Win10 bottom bar).
    let _pin = args
        .iter()
        .position(|a| a == "--pin")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse::<usize>().ok());
    match launch_with(items()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mde browser-jumplist: {e}");
            ExitCode::FAILURE
        }
    }
}

/// The Firefox jump-list entries: Tasks (New / New Private window), then the Recent
/// section (top history from `places.sqlite`, E18.4), then a footer that launches
/// the browser. The Recent section omits itself cleanly when there's no profile or
/// the read fails.
fn items() -> Vec<Item> {
    let mut v = vec![
        Item::new("New Window", "firefox --new-window"),
        Item::new("New Private Window", "firefox --private-window"),
    ];
    let recent = crate::browser::recent_sites();
    if !recent.is_empty() {
        v.push(sep());
        for (title, url) in recent {
            v.push(Item::new(title, crate::browser::open_url_cmd(&url)));
        }
    }
    v.push(sep());
    v.push(Item::new("Firefox", "firefox"));
    v
}
