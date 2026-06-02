//! Windows 10 "Settings" app — a flat category-nav window that replaces the
//! Control Panel in the Win10 era (Control Panel still serves the classic eras;
//! see the per-era routing in `control_panel::run`).
//!
//!   mde settings           the category grid (Home), drilling into categories
//!   mde settings --list    print the page → backend map (headless)
//!
//! Home is an icon-tile grid of the Win10 setting categories. A tile drills into
//! a category: a left page-rail + a right content pane, with a back-arrow home.
//! Pages are one of:
//!   - **Colors** — a native Light/Dark + accent page (re-skins live, persists).
//!   - **Spawn** — opens an existing backend (mde's own surfaces, or a
//!     `fedora::TOOLS` entry that installs-if-missing like the Control Panel).
//!   - **Deferred** — a greyed rail entry whose pane says it's a later milestone;
//!     never a fake working page (§3).

use std::collections::HashMap;
use std::process::{Command, ExitCode};

use iced::widget::{
    button, checkbox, column, container, image, mouse_area, pick_list, scrollable, text,
    text_input, Column, Row, Space,
};
use iced::{Background, Border, Color, Element, Length, Padding, Task};

use mde_ui::{metrics, palette};

use crate::wallpaper::{self, BgMode};
use crate::{fedora, outputs};

/// The Background page's source mode (E7.4): a picture, a solid color, or a
/// slideshow cycling the scanned pictures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BgSource {
    Picture,
    Solid,
    Slideshow,
}
impl BgSource {
    const ALL: [BgSource; 3] = [BgSource::Picture, BgSource::Solid, BgSource::Slideshow];
}

/// Taskbar location (E7.9): the two horizontal edges the Win10 bar supports
/// (left/right need a vertical bar — E7.9a).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaskbarLoc {
    Bottom,
    Top,
}
impl TaskbarLoc {
    const ALL: [TaskbarLoc; 2] = [TaskbarLoc::Bottom, TaskbarLoc::Top];
    fn key(self) -> &'static str {
        match self {
            TaskbarLoc::Bottom => "bottom",
            TaskbarLoc::Top => "top",
        }
    }
    fn from_key(k: &str) -> Self {
        if k == "top" {
            TaskbarLoc::Top
        } else {
            TaskbarLoc::Bottom
        }
    }
}
impl std::fmt::Display for TaskbarLoc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            TaskbarLoc::Bottom => "Bottom",
            TaskbarLoc::Top => "Top",
        })
    }
}

/// Win10 taskbar search affordance (E7.9a): a magnifier button, a wider "Search"
/// pill, or nothing — persisted as `win10_search_mode`, already consumed by
/// `panel.rs` (`win10_search_affordance`, E2.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchMode {
    Button,
    Box,
    Hidden,
}
impl SearchMode {
    const ALL: [SearchMode; 3] = [SearchMode::Button, SearchMode::Box, SearchMode::Hidden];
    fn key(self) -> &'static str {
        match self {
            SearchMode::Button => "button",
            SearchMode::Box => "box",
            SearchMode::Hidden => "hidden",
        }
    }
    fn from_key(k: &str) -> Self {
        match k {
            "box" => SearchMode::Box,
            "hidden" => SearchMode::Hidden,
            _ => SearchMode::Button,
        }
    }
}
impl std::fmt::Display for SearchMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            SearchMode::Button => "Search button",
            SearchMode::Box => "Search box",
            SearchMode::Hidden => "Hidden",
        })
    }
}
impl std::fmt::Display for BgSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            BgSource::Picture => "Picture",
            BgSource::Solid => "Solid color",
            BgSource::Slideshow => "Slideshow",
        })
    }
}

const COLS: usize = 4;

/// What a settings page does when opened.
#[derive(Clone, Copy)]
enum Kind {
    /// Greyed rail entry; the pane states it's a later milestone.
    Deferred,
    /// The native Personalization ▸ Colors page (E6.4).
    Colors,
    /// The native Personalization ▸ Background page (E7.4).
    Background,
    /// The native Personalization ▸ Themes page (E7.7).
    Themes,
    /// The native Personalization ▸ Lock screen page (E7.6).
    LockScreen,
    /// The native Personalization ▸ Start page (E7.8).
    Start,
    /// The native Personalization ▸ Taskbar page (E7.9).
    Taskbar,
    /// Spawn one of mde's own subcommands (`mde <sub>`).
    Mde(&'static str),
    /// Launch a `fedora::TOOLS` entry by its command (install-if-missing).
    Tool(&'static str),
    /// A raw command (in a terminal when `bool`).
    Cmd(&'static str, bool),
}

struct Page {
    title: &'static str,
    kind: Kind,
}

struct Category {
    title: &'static str,
    caption: &'static str,
    icons: &'static [&'static str],
    pages: &'static [Page],
}

/// The Win10 Settings categories (no Gaming, no Search — see the worklist). Each
/// lists its live pages first, then the deferred ones.
const CATEGORIES: &[Category] = &[
    Category {
        title: "System",
        caption: "Display, notifications, power",
        icons: &["preferences-system", "computer"],
        pages: &[
            Page {
                title: "Display",
                kind: Kind::Mde("display"),
            },
            Page {
                title: "About",
                kind: Kind::Mde("system-properties"),
            },
            Page {
                title: "Notifications & actions",
                kind: Kind::Deferred,
            },
            Page {
                title: "Power & sleep",
                kind: Kind::Deferred,
            },
            Page {
                title: "Storage",
                kind: Kind::Deferred,
            },
        ],
    },
    Category {
        title: "Devices",
        caption: "Bluetooth, printers, mouse",
        icons: &["preferences-desktop-peripherals", "input-mouse"],
        pages: &[
            Page {
                title: "Printers & scanners",
                kind: Kind::Tool("system-config-printer"),
            },
            Page {
                title: "Bluetooth & other devices",
                kind: Kind::Deferred,
            },
            Page {
                title: "Mouse",
                kind: Kind::Deferred,
            },
            Page {
                title: "Touchpad",
                kind: Kind::Deferred,
            },
        ],
    },
    Category {
        title: "Phone",
        caption: "Link your Android, iPhone",
        icons: &["phone", "smartphone"],
        pages: &[Page {
            title: "Your Phone",
            kind: Kind::Deferred,
        }],
    },
    Category {
        title: "Network & Internet",
        caption: "Wi-Fi, airplane mode, VPN",
        icons: &["preferences-system-network", "network-wired"],
        pages: &[
            Page {
                title: "Connections",
                kind: Kind::Tool("nm-connection-editor"),
            },
            Page {
                title: "Wi-Fi",
                kind: Kind::Deferred,
            },
            Page {
                title: "VPN",
                kind: Kind::Deferred,
            },
            Page {
                title: "Proxy",
                kind: Kind::Deferred,
            },
        ],
    },
    Category {
        title: "Personalization",
        caption: "Background, lock screen, colors",
        icons: &["preferences-desktop-wallpaper", "preferences-desktop-theme"],
        pages: &[
            Page {
                title: "Background",
                kind: Kind::Background,
            },
            Page {
                title: "Colors",
                kind: Kind::Colors,
            },
            Page {
                title: "Lock screen",
                kind: Kind::LockScreen,
            },
            Page {
                title: "Themes",
                kind: Kind::Themes,
            },
            Page {
                title: "Start",
                kind: Kind::Start,
            },
            Page {
                title: "Taskbar",
                kind: Kind::Taskbar,
            },
        ],
    },
    Category {
        title: "Apps",
        caption: "Uninstall, defaults, optional features",
        icons: &[
            "preferences-desktop-applications",
            "system-software-install",
        ],
        pages: &[
            Page {
                title: "Apps & features",
                kind: Kind::Mde("add-remove"),
            },
            Page {
                title: "Default apps",
                kind: Kind::Deferred,
            },
            Page {
                title: "Startup",
                kind: Kind::Deferred,
            },
        ],
    },
    Category {
        title: "Accounts",
        caption: "Your accounts, sign-in, sync",
        icons: &["system-users", "avatar-default"],
        pages: &[
            Page {
                title: "Users",
                kind: Kind::Tool("seahorse"),
            },
            Page {
                title: "Sign-in options",
                kind: Kind::Deferred,
            },
            Page {
                title: "Sync your settings",
                kind: Kind::Deferred,
            },
        ],
    },
    Category {
        title: "Time & Language",
        caption: "Speech, region, date",
        icons: &["preferences-system-time", "clock"],
        pages: &[
            Page {
                title: "Date & time",
                kind: Kind::Cmd("timedatectl", true),
            },
            Page {
                title: "Region",
                kind: Kind::Deferred,
            },
            Page {
                title: "Language",
                kind: Kind::Deferred,
            },
        ],
    },
    Category {
        title: "Ease of Access",
        caption: "Narrator, magnifier, high contrast",
        icons: &["preferences-desktop-accessibility"],
        pages: &[Page {
            title: "Display",
            kind: Kind::Deferred,
        }],
    },
    Category {
        title: "Privacy",
        caption: "Location, camera, microphone",
        icons: &["preferences-system-privacy", "security-medium"],
        pages: &[Page {
            title: "General",
            kind: Kind::Deferred,
        }],
    },
    Category {
        title: "Update & Security",
        caption: "MackesDE Update, recovery, backup",
        icons: &["system-software-update", "security-high"],
        pages: &[
            Page {
                title: "MackesDE Security",
                kind: Kind::Tool("firewall-config"),
            },
            Page {
                title: "MackesDE Update",
                kind: Kind::Cmd("sudo dnf upgrade", true),
            },
            Page {
                title: "Backup",
                kind: Kind::Deferred,
            },
            Page {
                title: "Recovery",
                kind: Kind::Deferred,
            },
        ],
    },
];

#[derive(Debug, Clone, Copy, PartialEq)]
enum View {
    Home,
    Category(usize),
}

struct Settings {
    view: View,
    page: usize, // selected rail page within the current category
    dark: bool,
    /// Windows 10 UI accent index (E7.5) — into `palette::WIN10_ACCENTS`; drives
    /// selection/highlight via the `win10()` slot, persisted as `win10_accent`.
    win10_accent: u8,
    /// Home-screen search query (E6.6): filters the flat (category, page) list.
    search: String,
    /// Background page (E7.4): source mode, scanned pictures, selection, fit.
    bg_source: BgSource,
    bg_wallpapers: Vec<String>,
    bg_selected: Option<usize>,
    bg_mode: BgMode,
    /// Saved theme bundles (E7.7).
    themes: Vec<crate::state::SavedTheme>,
    /// Start page toggles (E7.8), consumed by `start_win10`.
    start_more_tiles: bool,
    /// Start ▸ "Use Start full screen" (E7.8b).
    start_full_screen: bool,
    start_show_recent: bool,
    start_show_suggested: bool,
    /// Start rail system folders (E7.8a): the chosen `start_win10::START_FOLDERS` keys.
    start_folders: Vec<String>,
    /// Taskbar location (E7.9), consumed by `panel.rs`'s Win10 anchor.
    taskbar_loc: TaskbarLoc,
    /// Win10 taskbar search affordance + Task-View button (E7.9a), consumed by panel.rs.
    search_mode: SearchMode,
    show_taskview: bool,
    /// Lock screen (greeter) picture selection (E7.6).
    lock_selected: Option<usize>,
    /// Cached install state for the `fedora::TOOLS` command of a viewed Tool
    /// page (computed lazily — `is_installed` spawns subprocesses).
    installed: HashMap<&'static str, bool>,
}

#[derive(Debug, Clone)]
enum Message {
    OpenCategory(usize),
    SelectPage(usize),
    Open, // activate the current page's backend
    Back,
    SetDark(bool),
    Search(String),
    Jump(usize, usize), // (category, page) from a search result
    // Background page (E7.4).
    BgSource(BgSource),
    BgSelect(usize),
    BgMode(BgMode),
    BgBrowse,
    BgBrowsed(Option<String>),
    BgApply,
    // Themes page (E7.7).
    SaveTheme,
    ApplyTheme(usize),
    // Start page (E7.8).
    SetStartMore(bool),
    /// Start ▸ "Use Start full screen" (E7.8b).
    SetStartFullScreen(bool),
    SetStartRecent(bool),
    SetStartSuggested(bool),
    /// Toggle a system folder's presence in the Start rail (E7.8a).
    ToggleStartFolder(String, bool),
    // Taskbar page (E7.9).
    SetTaskbarLoc(TaskbarLoc),
    SetSearchMode(SearchMode),
    SetShowTaskview(bool),
    // Lock screen page (E7.6).
    LockSelect(usize),
    LockBrowse,
    LockBrowsed(Option<String>),
    LockApply,
}

pub fn run(args: &[String]) -> ExitCode {
    if args.iter().any(|a| a == "--list") {
        return list();
    }
    // Dry-run: print the LightDM-greeter write script (E7.6) so its logic can be
    // verified without root — point MDE_LOCK_CONF at a temp file and run it.
    if args.first().map(String::as_str) == Some("--lock-script") {
        print!("{}", lock_script());
        return ExitCode::SUCCESS;
    }
    // Parse a positional category name plus an optional `--page <name>` deep-link
    // (E7.3): `mde settings personalization --page taskbar`. A positional that
    // isn't a category pre-fills the Home search box instead (`mde settings
    // display`).
    let mut cat_arg = String::new();
    let mut page_arg: Option<String> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--page" {
            page_arg = it.next().cloned();
        } else if !a.starts_with("--") {
            if !cat_arg.is_empty() {
                cat_arg.push(' ');
            }
            cat_arg.push_str(a);
        }
    }
    let cat_arg = cat_arg.trim().to_string();
    let initial_cat = category_index(&cat_arg);
    let initial_page = match (initial_cat, &page_arg) {
        (Some(c), Some(name)) => page_index(c, name).unwrap_or(0),
        _ => 0,
    };
    let initial_search = if initial_cat.is_none() {
        cat_arg
    } else {
        String::new()
    };
    match gui(initial_cat, initial_page, initial_search) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mde settings: {e}");
            ExitCode::FAILURE
        }
    }
}

/// First category whose title contains `name` (case-insensitive).
fn category_index(name: &str) -> Option<usize> {
    let n = name.to_lowercase();
    CATEGORIES
        .iter()
        .position(|c| c.title.to_lowercase().contains(&n))
}

/// First page in `cat` whose title contains `name` (case-insensitive) — the
/// `--page` deep-link target.
fn page_index(cat: usize, name: &str) -> Option<usize> {
    let n = name.to_lowercase();
    CATEGORIES
        .get(cat)?
        .pages
        .iter()
        .position(|p| p.title.to_lowercase().contains(&n))
}

/// `mde settings --list` — print the page → backend map for every live page.
fn list() -> ExitCode {
    for cat in CATEGORIES {
        for p in cat.pages {
            let backend = match p.kind {
                Kind::Deferred => continue,
                Kind::Colors => "(native: Colors)".to_string(),
                Kind::Background => "(native: Background)".to_string(),
                Kind::Themes => "(native: Themes)".to_string(),
                Kind::Start => "(native: Start)".to_string(),
                Kind::Taskbar => "(native: Taskbar)".to_string(),
                Kind::LockScreen => "(native: Lock screen)".to_string(),
                Kind::Mde(s) => format!("mde {s}"),
                Kind::Tool(c) => format!("tool: {c}"),
                Kind::Cmd(c, _) => format!("cmd: {c}"),
            };
            println!("{} \u{25b8} {} -> {}", cat.title, p.title, backend);
        }
    }
    ExitCode::SUCCESS
}

fn gui(initial: Option<usize>, initial_page: usize, initial_search: String) -> iced::Result {
    iced::application(|_: &Settings| "Settings - mde".to_string(), update, view)
        .theme(|_| mde_ui::palette::iced_theme())
        .window_size(iced::Size::new(940.0, 640.0))
        .font(mde_ui::font::REGULAR_BYTES)
        .font(mde_ui::font::BOLD_BYTES)
        .font(mde_ui::font::PLEX_REGULAR_BYTES)
        .font(mde_ui::font::PLEX_BOLD_BYTES)
        .default_font(mde_ui::font::ui())
        .run_with(move || {
            let st = crate::state::load();
            let mut s = Settings {
                view: initial.map(View::Category).unwrap_or(View::Home),
                page: initial_page,
                dark: st.theme_mode != "light",
                win10_accent: st.win10_accent,
                search: initial_search,
                bg_source: BgSource::Picture,
                bg_wallpapers: wallpaper::scan(),
                bg_selected: None,
                bg_mode: BgMode::Fill,
                themes: st.themes.clone(),
                start_more_tiles: st.start_more_tiles,
                start_full_screen: st.start_full_screen,
                start_show_recent: st.start_show_recent,
                start_show_suggested: st.start_show_suggested,
                start_folders: st.start_folders.clone(),
                taskbar_loc: TaskbarLoc::from_key(&st.taskbar_location),
                search_mode: SearchMode::from_key(&st.win10_search_mode),
                show_taskview: st.win10_show_taskview,
                lock_selected: None,
                installed: HashMap::new(),
            };
            cache_install(&mut s);
            (s, Task::none())
        })
}

fn current_page(state: &Settings) -> Option<&'static Page> {
    match state.view {
        View::Category(c) => CATEGORIES.get(c).and_then(|cat| cat.pages.get(state.page)),
        View::Home => None,
    }
}

fn update(state: &mut Settings, message: Message) -> Task<Message> {
    match message {
        Message::OpenCategory(i) => {
            state.view = View::Category(i);
            state.page = 0;
            cache_install(state);
        }
        Message::SelectPage(i) => {
            state.page = i;
            cache_install(state);
        }
        Message::Back => {
            state.view = View::Home;
            state.page = 0;
        }
        Message::Search(q) => state.search = q,
        Message::Jump(c, p) => {
            state.view = View::Category(c);
            state.page = p;
            cache_install(state);
        }
        Message::Open => open_current(state),
        Message::SetDark(d) => {
            state.dark = d;
            palette::set_dark(d);
            persist(state);
        }
        Message::BgSource(s) => state.bg_source = s,
        Message::BgSelect(i) => state.bg_selected = Some(i),
        Message::BgMode(m) => state.bg_mode = m,
        Message::BgBrowse => {
            return Task::perform(async { wallpaper::browse() }, Message::BgBrowsed);
        }
        Message::BgBrowsed(Some(p)) => {
            state.bg_wallpapers.push(p);
            state.bg_selected = Some(state.bg_wallpapers.len() - 1);
        }
        Message::BgBrowsed(None) => {}
        Message::BgApply => apply_background(state),
        Message::SaveTheme => save_theme(state),
        Message::ApplyTheme(i) => apply_theme(state, i),
        Message::SetStartMore(v) => {
            state.start_more_tiles = v;
            persist(state);
        }
        Message::SetStartFullScreen(v) => {
            state.start_full_screen = v;
            persist(state);
        }
        Message::SetStartRecent(v) => {
            state.start_show_recent = v;
            persist(state);
        }
        Message::SetStartSuggested(v) => {
            state.start_show_suggested = v;
            persist(state);
        }
        Message::ToggleStartFolder(key, on) => {
            state.start_folders.retain(|k| k != &key);
            if on {
                state.start_folders.push(key);
                // Keep the chosen set in the canonical START_FOLDERS order, so the
                // rail order is stable regardless of toggle sequence.
                state.start_folders.sort_by_key(|k| {
                    crate::start_win10::START_FOLDERS
                        .iter()
                        .position(|f| f.0 == k.as_str())
                        .unwrap_or(usize::MAX)
                });
            }
            persist(state);
        }
        Message::SetTaskbarLoc(loc) => {
            state.taskbar_loc = loc;
            persist(state);
        }
        Message::SetSearchMode(m) => {
            state.search_mode = m;
            persist(state);
        }
        Message::SetShowTaskview(v) => {
            state.show_taskview = v;
            persist(state);
        }
        Message::LockSelect(i) => state.lock_selected = Some(i),
        Message::LockBrowse => {
            return Task::perform(async { wallpaper::browse() }, Message::LockBrowsed);
        }
        Message::LockBrowsed(Some(p)) => {
            state.bg_wallpapers.push(p);
            state.lock_selected = Some(state.bg_wallpapers.len() - 1);
        }
        Message::LockBrowsed(None) => {}
        Message::LockApply => {
            if let Some(p) = state.lock_selected.and_then(|i| state.bg_wallpapers.get(i)) {
                set_lock_background(p);
            }
        }
    }
    Task::none()
}

/// Capture the current background + accent + mode as a new saved theme bundle.
fn save_theme(state: &mut Settings) {
    let wallpaper = state
        .bg_selected
        .and_then(|i| state.bg_wallpapers.get(i))
        .cloned()
        .unwrap_or_default();
    let theme = crate::state::SavedTheme {
        name: format!("Custom {}", state.themes.len() + 1),
        wallpaper,
        accent: state.win10_accent,
        dark: state.dark,
    };
    state.themes.push(theme);
    let mut st = crate::state::load();
    st.themes = state.themes.clone();
    let _ = crate::state::save(&st);
}

/// Apply a saved theme bundle: wallpaper (swaybg) + mode. (The accent is no
/// longer per-theme — Carbon Blue is fixed across eras after the rebrand.)
fn apply_theme(state: &mut Settings, i: usize) {
    let Some(t) = state.themes.get(i).cloned() else {
        return;
    };
    if !t.wallpaper.is_empty() {
        let _ = outputs::set_wallpaper(&t.wallpaper, state.bg_mode.swaybg());
    }
    state.dark = t.dark;
    palette::set_dark(t.dark);
    persist(state);
}

/// The sh script that sets the LightDM greeter `background=` to `$1`, updating
/// the value inside `[greeter]` without clobbering the rest of the conf. The
/// path arrives as `$1` (never embedded in the program text) and the value is
/// passed to awk via `-v`, so neither can inject. The conf path honours
/// `$MDE_LOCK_CONF` so the logic is testable without root (E7.6).
fn lock_script() -> String {
    "set -e\n\
     f=\"${MDE_LOCK_CONF:-/etc/lightdm/lightdm-gtk-greeter.conf}\"\n\
     bg=\"$1\"\n\
     mkdir -p \"$(dirname \"$f\")\"\n\
     [ -f \"$f\" ] || printf '[greeter]\\n' > \"$f\"\n\
     grep -q '^\\[greeter\\]' \"$f\" || printf '\\n[greeter]\\n' >> \"$f\"\n\
     awk -v v=\"$bg\" '\n\
     /^\\[greeter\\]/ { print; print \"background=\" v; ingreeter=1; next }\n\
     /^\\[/ { ingreeter=0 }\n\
     ingreeter && /^[[:space:]]*background=/ { next }\n\
     { print }\n\
     ' \"$f\" > \"$f.tmp\" && mv \"$f.tmp\" \"$f\"\n"
        .to_string()
}

/// Set the LightDM greeter (lock-screen) background to `path` via pkexec (the
/// only way to write `/etc`; needs an interactive auth agent — E7.6).
fn set_lock_background(path: &str) {
    let _ = Command::new("pkexec")
        .arg("sh")
        .arg("-c")
        .arg(lock_script())
        .arg("mde-lock")
        .arg(path)
        .status();
}

/// Apply (and persist) the Background page's current selection via swaybg.
fn apply_background(state: &Settings) {
    let mode = state.bg_mode.swaybg();
    match state.bg_source {
        BgSource::Picture => {
            if let Some(p) = state.bg_selected.and_then(|i| state.bg_wallpapers.get(i)) {
                let _ = outputs::set_wallpaper(p, mode);
            }
        }
        BgSource::Solid => {
            // "Solid" = the themed desktop color (no separate color picker yet).
            let _ = outputs::set_solid(&palette::hex(palette::BACKGROUND));
        }
        BgSource::Slideshow => {
            let _ = outputs::set_slideshow(&state.bg_wallpapers, mode, 300);
        }
    }
}

/// Compute (once) whether the current Tool page's command is installed.
fn cache_install(state: &mut Settings) {
    if let Some(Page {
        kind: Kind::Tool(cmd),
        ..
    }) = current_page(state)
    {
        if !state.installed.contains_key(cmd) {
            let present = tool_by_cmd(cmd).map(fedora::is_installed).unwrap_or(false);
            state.installed.insert(cmd, present);
        }
    }
}

fn tool_by_cmd(cmd: &str) -> Option<&'static fedora::Tool> {
    fedora::TOOLS.iter().find(|t| t.command == cmd)
}

/// Open the current page's backend. Tool pages install-if-missing (like the
/// Control Panel); everything else spawns directly.
fn open_current(state: &mut Settings) {
    let Some(page) = current_page(state) else {
        return;
    };
    match page.kind {
        Kind::Deferred
        | Kind::Colors
        | Kind::Background
        | Kind::Themes
        | Kind::Start
        | Kind::Taskbar
        | Kind::LockScreen => {}
        Kind::Mde(sub) => {
            let mde = mde_path();
            let _ = Command::new(mde).arg(sub).spawn();
        }
        Kind::Tool(cmd) => {
            if let Some(tool) = tool_by_cmd(cmd) {
                let present = state.installed.get(cmd).copied().unwrap_or(false);
                if present {
                    let _ = fedora::launch(tool);
                } else if matches!(fedora::install(&[tool.package]), Ok(s) if s.success()) {
                    state.installed.insert(cmd, true);
                    let _ = fedora::launch(tool);
                }
            }
        }
        Kind::Cmd(cmd, terminal) => {
            if terminal {
                let _ = Command::new("foot")
                    .arg("-e")
                    .arg("sh")
                    .arg("-c")
                    .arg(cmd)
                    .spawn();
            } else {
                let _ = Command::new("sh").arg("-c").arg(cmd).spawn();
            }
        }
    }
}

fn mde_path() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_else(|| "mde".to_string())
}

fn persist(state: &Settings) {
    let mut st = crate::state::load();
    st.theme_mode = if state.dark { "dark" } else { "light" }.to_string();
    st.win10_accent = state.win10_accent;
    st.start_more_tiles = state.start_more_tiles;
    st.start_full_screen = state.start_full_screen;
    st.start_show_recent = state.start_show_recent;
    st.start_show_suggested = state.start_show_suggested;
    st.start_folders = state.start_folders.clone();
    st.taskbar_location = state.taskbar_loc.key().to_string();
    st.win10_search_mode = state.search_mode.key().to_string();
    st.win10_show_taskview = state.show_taskview;
    let _ = crate::state::save(&st);
}

// --- view ------------------------------------------------------------------

fn bg(_t: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(palette::color(palette::WINDOW))),
        text_color: Some(palette::color(palette::WINDOW_TEXT)),
        ..container::Style::default()
    }
}

fn view(state: &Settings) -> Element<'_, Message> {
    let body = match state.view {
        View::Home => home(state),
        View::Category(c) => category(state, c),
    };
    container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(16.0)
        .style(bg)
        .into()
}

fn home(state: &Settings) -> Element<'_, Message> {
    let title = text("Settings")
        .size(metrics::INFO_TITLE_PX)
        .color(palette::color(palette::WINDOW_TEXT));
    let find = text_input("Find a setting", &state.search)
        .on_input(Message::Search)
        .padding(8.0)
        .size(metrics::UI_PX)
        .width(Length::Fixed(360.0));

    // A non-empty query filters the flat (category, page) list; otherwise the
    // category tile grid shows.
    let q = state.search.trim().to_lowercase();
    let content: Element<Message> = if q.is_empty() {
        let mut grid = Column::new().spacing(12.0);
        let mut r = Row::new().spacing(12.0);
        for (i, cat) in CATEGORIES.iter().enumerate() {
            r = r.push(home_tile(i, cat));
            if (i + 1) % COLS == 0 {
                grid = grid.push(r);
                r = Row::new().spacing(12.0);
            }
        }
        grid = grid.push(r);
        scrollable(grid).style(mde_ui::scrollbar).into()
    } else {
        search_results(&q)
    };

    Column::new()
        .spacing(16.0)
        .push(title)
        .push(find)
        .push(content)
        .into()
}

/// In-memory filter of every (category, page) by title; clicking a row jumps to
/// that page. No indexer — just a flat scan of the static model (E6.6).
fn search_results(q: &str) -> Element<'static, Message> {
    let mut col = Column::new().spacing(1.0);
    let mut any = false;
    for (ci, cat) in CATEGORIES.iter().enumerate() {
        for (pi, p) in cat.pages.iter().enumerate() {
            let hay = format!("{} {}", cat.title, p.title).to_lowercase();
            if !hay.contains(q) {
                continue;
            }
            any = true;
            // ASCII separator: the bundled UI font lacks ▸ (§2.7 never tofu);
            // `--list` keeps ▸ since it prints to a terminal font.
            let label = format!("{}  >  {}", cat.title, p.title);
            col = col.push(
                button(text(label).size(metrics::UI_PX))
                    .on_press(Message::Jump(ci, pi))
                    .width(Length::Fill)
                    .padding(Padding::from([6.0, 10.0]))
                    .style(|_t, status| {
                        let hot = matches!(status, button::Status::Hovered);
                        button::Style {
                            background: hot
                                .then(|| Background::Color(palette::color(palette::MENU))),
                            text_color: palette::color(palette::MENU_TEXT),
                            border: Border::default(),
                            ..button::Style::default()
                        }
                    }),
            );
        }
    }
    if !any {
        col = col.push(
            text("No matching settings")
                .size(metrics::UI_PX)
                .color(palette::color(palette::GRAY_TEXT)),
        );
    }
    scrollable(col).style(mde_ui::scrollbar).into()
}

fn home_tile(i: usize, cat: &'static Category) -> Element<'static, Message> {
    let inner = Row::new()
        .spacing(10.0)
        .align_y(iced::alignment::Vertical::Center)
        .push(crate::icons::icon_any(cat.icons, 32))
        .push(
            Column::new()
                .spacing(2.0)
                .push(
                    text(cat.title)
                        .size(metrics::UI_PX)
                        .color(palette::color(palette::WINDOW_TEXT)),
                )
                .push(
                    text(cat.caption)
                        .size(metrics::UI_PX)
                        .color(palette::color(palette::GRAY_TEXT)),
                ),
        );
    button(inner)
        .on_press(Message::OpenCategory(i))
        .width(Length::Fixed(210.0))
        .padding(12.0)
        .style(tile_style)
        .into()
}

fn tile_style(_t: &iced::Theme, status: button::Status) -> button::Style {
    let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: Some(Background::Color(palette::color(if hot {
            palette::MENU
        } else {
            palette::WINDOW
        }))),
        text_color: palette::color(palette::WINDOW_TEXT),
        border: Border {
            color: palette::color(palette::WINDOW_FRAME),
            width: 1.0,
            radius: 2.0.into(),
        },
        ..button::Style::default()
    }
}

fn category(state: &Settings, c: usize) -> Element<'_, Message> {
    let cat = &CATEGORIES[c];
    // Header: back arrow + category title.
    let back = button(text("\u{2190}").size(metrics::INFO_TITLE_PX))
        .on_press(Message::Back)
        .padding(Padding::from([2.0, 10.0]))
        .style(tile_style);
    let header = Row::new()
        .spacing(12.0)
        .align_y(iced::alignment::Vertical::Center)
        .push(back)
        .push(
            text(cat.title)
                .size(metrics::INFO_TITLE_PX)
                .color(palette::color(palette::WINDOW_TEXT)),
        );

    // Left rail: one entry per page (deferred greyed).
    let mut rail = Column::new().spacing(1.0).width(Length::Fixed(220.0));
    for (i, p) in cat.pages.iter().enumerate() {
        rail = rail.push(rail_entry(i, p, i == state.page));
    }

    let pane = container(content_pane(state, cat))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(16.0);

    let cols = Row::new()
        .spacing(12.0)
        .push(scrollable(rail).style(mde_ui::scrollbar))
        .push(pane);

    Column::new().spacing(16.0).push(header).push(cols).into()
}

fn rail_entry(i: usize, p: &Page, selected: bool) -> Element<'static, Message> {
    let deferred = matches!(p.kind, Kind::Deferred);
    let fg = if deferred {
        palette::color(palette::GRAY_TEXT)
    } else {
        palette::color(palette::MENU_TEXT)
    };
    let label = text(p.title).size(metrics::UI_PX).color(fg);
    button(label)
        .on_press(Message::SelectPage(i))
        .width(Length::Fill)
        .padding(Padding::from([6.0, 10.0]))
        .style(move |_t, status| {
            let hot = selected || matches!(status, button::Status::Hovered);
            button::Style {
                background: hot.then(|| Background::Color(palette::color(palette::MENU))),
                text_color: fg,
                border: Border {
                    color: if selected {
                        palette::accent()
                    } else {
                        Color::TRANSPARENT
                    },
                    width: if selected { 2.0 } else { 0.0 },
                    radius: 0.0.into(),
                },
                ..button::Style::default()
            }
        })
        .into()
}

fn content_pane<'a>(state: &'a Settings, cat: &'static Category) -> Element<'a, Message> {
    let Some(page) = cat.pages.get(state.page) else {
        return Space::new(Length::Fill, Length::Fill).into();
    };
    let heading = text(page.title)
        .size(metrics::INFO_TITLE_PX)
        .color(palette::color(palette::WINDOW_TEXT));
    let inner: Element<Message> = match page.kind {
        Kind::Colors => colors_page(state),
        Kind::Background => background_page(state),
        Kind::Themes => themes_page(state),
        Kind::Start => start_page(state),
        Kind::Taskbar => taskbar_page(state),
        Kind::LockScreen => lock_page(state),
        Kind::Deferred => text("This page is part of a later milestone.")
            .size(metrics::UI_PX)
            .color(palette::color(palette::GRAY_TEXT))
            .into(),
        Kind::Mde(_) | Kind::Cmd(_, _) => open_button(page.title, true),
        Kind::Tool(cmd) => {
            let present = state.installed.get(cmd).copied().unwrap_or(true);
            open_button(page.title, present)
        }
    };
    Column::new().spacing(16.0).push(heading).push(inner).into()
}

/// A page that just launches an external backend: a single Open button (or
/// "Install & Open" when the backing tool is missing).
fn open_button<'a>(title: &str, present: bool) -> Element<'a, Message> {
    let label = if present {
        format!("Open {title}")
    } else {
        format!("Install & Open {title}")
    };
    Column::new()
        .spacing(10.0)
        .push(
            text(format!("Opens {title} in its own window."))
                .size(metrics::UI_PX)
                .color(palette::color(palette::WINDOW_TEXT)),
        )
        .push(
            button(text(label).size(metrics::UI_PX))
                .on_press(Message::Open)
                .padding(Padding::from([6.0, 16.0]))
                .style(tile_style),
        )
        .into()
}

/// Personalization ▸ Colors (E6.4): the Light/Dark choice (re-skins live + persists).
/// The accent picker was retired in the MackesDE rebrand — the accent is now
/// Carbon Blue for every era, not a per-user pick.
fn colors_page(state: &Settings) -> Element<'_, Message> {
    let mode_label = text("Choose your color")
        .size(metrics::UI_PX)
        .color(palette::color(palette::WINDOW_TEXT));
    let light = mode_button("Light", !state.dark, Message::SetDark(false));
    let dark = mode_button("Dark", state.dark, Message::SetDark(true));
    let modes = Row::new().spacing(8.0).push(light).push(dark);

    column![mode_label, modes].spacing(8.0).into()
}

/// Personalization ▸ Background (E7.4): a live preview, a Picture/Solid/
/// Slideshow source dropdown, a thumbnail strip + Browse + fit dropdown for
/// Picture, and Apply — driving swaybg through the shared `crate::wallpaper`
/// + `outputs` helpers.
fn background_page(state: &Settings) -> Element<'_, Message> {
    let sel = state
        .bg_selected
        .and_then(|i| state.bg_wallpapers.get(i))
        .map(String::as_str);
    // Solid previews the themed desktop color (no picture).
    let preview_sel = if state.bg_source == BgSource::Solid {
        None
    } else {
        sel
    };
    let preview = container(wallpaper::preview::<Message>(preview_sel))
        .width(Length::Fixed(320.0))
        .height(Length::Fixed(180.0))
        .style(|_| container::Style {
            border: Border {
                color: palette::color(palette::WINDOW_FRAME),
                width: 1.0,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        });

    let source = Row::new()
        .spacing(8.0)
        .align_y(iced::alignment::Vertical::Center)
        .push(
            text("Background")
                .size(metrics::UI_PX)
                .color(palette::color(palette::WINDOW_TEXT)),
        )
        .push(
            pick_list(
                BgSource::ALL.to_vec(),
                Some(state.bg_source),
                Message::BgSource,
            )
            .text_size(metrics::UI_PX),
        );

    let mut col = Column::new().spacing(12.0).push(preview).push(source);

    match state.bg_source {
        BgSource::Picture => {
            let mut strip = Row::new().spacing(8.0);
            for (i, wp) in state.bg_wallpapers.iter().enumerate().take(24) {
                strip = strip.push(thumb(
                    wp,
                    state.bg_selected == Some(i),
                    Message::BgSelect(i),
                ));
            }
            col = col
                .push(
                    text("Choose your picture")
                        .size(metrics::UI_PX)
                        .color(palette::color(palette::WINDOW_TEXT)),
                )
                .push(scrollable(strip).style(mde_ui::scrollbar))
                .push(fit_row("Browse", state.bg_mode, true));
        }
        BgSource::Solid => {
            col = col.push(
                text("Uses the themed desktop color.")
                    .size(metrics::UI_PX)
                    .color(palette::color(palette::GRAY_TEXT)),
            );
        }
        BgSource::Slideshow => {
            col = col
                .push(
                    text(format!(
                        "Cycles {} pictures every 5 minutes.",
                        state.bg_wallpapers.len()
                    ))
                    .size(metrics::UI_PX)
                    .color(palette::color(palette::GRAY_TEXT)),
                )
                .push(fit_row("", state.bg_mode, false));
        }
    }

    col.push(
        button(text("Apply").size(metrics::UI_PX))
            .on_press(Message::BgApply)
            .padding(Padding::from([6.0, 18.0]))
            .style(tile_style),
    )
    .into()
}

/// The "Browse … + fit dropdown" row shared by the Picture/Slideshow sources.
/// `browse` controls whether the Browse button shows.
fn fit_row<'a>(browse_label: &str, mode: BgMode, browse: bool) -> Element<'a, Message> {
    let mut row = Row::new()
        .spacing(8.0)
        .align_y(iced::alignment::Vertical::Center);
    if browse {
        row = row.push(
            button(text(browse_label.to_string()).size(metrics::UI_PX))
                .on_press(Message::BgBrowse)
                .padding(Padding::from([4.0, 12.0]))
                .style(tile_style),
        );
    }
    row.push(
        text("Choose a fit")
            .size(metrics::UI_PX)
            .color(palette::color(palette::WINDOW_TEXT)),
    )
    .push(pick_list(BgMode::ALL.to_vec(), Some(mode), Message::BgMode).text_size(metrics::UI_PX))
    .into()
}

/// A clickable wallpaper thumbnail (accent border when selected).
fn thumb(path: &str, selected: bool, on_press: Message) -> Element<'static, Message> {
    let img = image(image::Handle::from_path(path))
        .width(Length::Fixed(96.0))
        .height(Length::Fixed(60.0))
        .content_fit(iced::ContentFit::Cover);
    let border_c = if selected {
        palette::accent()
    } else {
        palette::color(palette::WINDOW_FRAME)
    };
    mouse_area(container(img).style(move |_| container::Style {
        border: Border {
            color: border_c,
            width: if selected { 2.0 } else { 1.0 },
            radius: 0.0.into(),
        },
        ..container::Style::default()
    }))
    .on_press(on_press)
    .into()
}

/// Personalization ▸ Themes (E7.7): a gallery of saved {background, accent,
/// mode} bundles + a "Save theme" button. Selecting a tile re-applies the whole
/// bundle (swaybg + accent + mode) in one action.
fn themes_page(state: &Settings) -> Element<'_, Message> {
    let gallery: Element<Message> = if state.themes.is_empty() {
        text("No saved themes yet. Save your current background, accent and mode as a theme.")
            .size(metrics::UI_PX)
            .color(palette::color(palette::GRAY_TEXT))
            .into()
    } else {
        let mut row = Row::new().spacing(12.0);
        for (i, t) in state.themes.iter().enumerate() {
            row = row.push(theme_tile(i, t));
        }
        scrollable(row).style(mde_ui::scrollbar).into()
    };
    let save = button(text("Save theme").size(metrics::UI_PX))
        .on_press(Message::SaveTheme)
        .padding(Padding::from([6.0, 18.0]))
        .style(tile_style);
    Column::new().spacing(16.0).push(gallery).push(save).into()
}

/// One theme tile: the saved wallpaper thumbnail (or the accent swatch when the
/// bundle keeps the current background) over its name. Click re-applies it.
fn theme_tile(i: usize, t: &crate::state::SavedTheme) -> Element<'static, Message> {
    let preview: Element<Message> = if t.wallpaper.is_empty() {
        // No per-theme accent anymore (rebrand) — preview the fixed Carbon accent.
        let c = palette::accent();
        container(Space::new(Length::Fixed(120.0), Length::Fixed(70.0)))
            .style(move |_| container::Style {
                background: Some(Background::Color(c)),
                border: Border {
                    color: palette::color(palette::WINDOW_FRAME),
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..container::Style::default()
            })
            .into()
    } else {
        container(
            image(image::Handle::from_path(&t.wallpaper))
                .width(Length::Fixed(120.0))
                .height(Length::Fixed(70.0))
                .content_fit(iced::ContentFit::Cover),
        )
        .style(|_| container::Style {
            border: Border {
                color: palette::color(palette::WINDOW_FRAME),
                width: 1.0,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        })
        .into()
    };
    let card = Column::new()
        .spacing(4.0)
        .align_x(iced::alignment::Horizontal::Center)
        .push(preview)
        .push(
            text(t.name.clone())
                .size(metrics::UI_PX)
                .color(palette::color(palette::WINDOW_TEXT)),
        );
    mouse_area(card).on_press(Message::ApplyTheme(i)).into()
}

/// Personalization ▸ Start (E7.8): the toggles the Win10 tiled Start consumes.
fn start_page(state: &Settings) -> Element<'_, Message> {
    let row = |label: &'static str, checked: bool, msg: fn(bool) -> Message| {
        checkbox(label, checked)
            .on_toggle(msg)
            .size(metrics::UI_PX)
            .text_size(metrics::UI_PX)
            .spacing(8.0)
            .style(mde_ui::checkbox_style)
    };
    let mut col = Column::new()
        .spacing(10.0)
        .push(row(
            "Show more tiles",
            state.start_more_tiles,
            Message::SetStartMore,
        ))
        .push(row(
            "Use Start full screen",
            state.start_full_screen,
            Message::SetStartFullScreen,
        ))
        .push(row(
            "Show recently added apps",
            state.start_show_recent,
            Message::SetStartRecent,
        ))
        .push(row(
            "Show most used apps",
            state.start_show_suggested,
            Message::SetStartSuggested,
        ))
        .push(
            text("Choose which folders appear on Start")
                .size(metrics::UI_PX)
                .font(mde_ui::font::ui_bold()),
        );
    // One checkbox per selectable system folder (E7.8a); toggling persists and the
    // Start rail reflects it on next open. Closures capture the key, so these can't
    // use the `fn(bool)`-pointer `row` helper above.
    for entry in crate::start_win10::START_FOLDERS {
        let key = entry.0.to_string();
        let checked = state.start_folders.iter().any(|k| k == &key);
        col = col.push(
            checkbox(entry.1, checked)
                .on_toggle(move |on| Message::ToggleStartFolder(key.clone(), on))
                .size(metrics::UI_PX)
                .text_size(metrics::UI_PX)
                .spacing(8.0)
                .style(mde_ui::checkbox_style),
        );
    }
    col.into()
}

/// Personalization ▸ Taskbar (E7.9): the location dropdown drives the panel
/// anchor; lock / auto-hide are labwc-managed (greyed, present for fidelity).
fn taskbar_page(state: &Settings) -> Element<'_, Message> {
    let location = Row::new()
        .spacing(8.0)
        .align_y(iced::alignment::Vertical::Center)
        .push(
            text("Taskbar location on screen")
                .size(metrics::UI_PX)
                .color(palette::color(palette::WINDOW_TEXT)),
        )
        .push(
            pick_list(
                TaskbarLoc::ALL.to_vec(),
                Some(state.taskbar_loc),
                Message::SetTaskbarLoc,
            )
            .text_size(metrics::UI_PX),
        );
    // Search affordance picker (E7.9a) — drives panel.rs's win10_search_affordance.
    let search = Row::new()
        .spacing(8.0)
        .align_y(iced::alignment::Vertical::Center)
        .push(
            text("Search on the taskbar")
                .size(metrics::UI_PX)
                .color(palette::color(palette::WINDOW_TEXT)),
        )
        .push(
            pick_list(
                SearchMode::ALL.to_vec(),
                Some(state.search_mode),
                Message::SetSearchMode,
            )
            .text_size(metrics::UI_PX),
        );
    // Greyed: labwc owns these (auto-hide / lock are compositor behaviours),
    // present for fidelity but not enforced here — like taskbar_properties.rs.
    let greyed = |label: &'static str| {
        checkbox(label, false)
            .size(metrics::UI_PX)
            .text_size(metrics::UI_PX)
            .spacing(8.0)
            .style(mde_ui::checkbox_style)
    };
    Column::new()
        .spacing(10.0)
        .push(location)
        .push(search)
        // Real toggle: panel.rs hides the Task View button when off (E2.9).
        .push(
            checkbox("Show the Task View button", state.show_taskview)
                .on_toggle(Message::SetShowTaskview)
                .size(metrics::UI_PX)
                .text_size(metrics::UI_PX)
                .spacing(8.0)
                .style(mde_ui::checkbox_style),
        )
        .push(greyed("Lock the taskbar"))
        .push(greyed("Automatically hide the taskbar"))
        .push(
            text(
                "Lock / auto-hide are labwc-managed. \"Use small buttons\" and left/right \
                 (vertical) location are a later milestone.",
            )
            .size(metrics::UI_PX)
            .color(palette::color(palette::GRAY_TEXT)),
        )
        .into()
}

/// Personalization ▸ Lock screen (E7.6): pick a Picture for the LightDM greeter
/// background. Apply writes it via pkexec. (Spotlight rotation / Slideshow don't
/// map to LightDM's single static greeter background, so only Picture ships.)
fn lock_page(state: &Settings) -> Element<'_, Message> {
    let sel = state
        .lock_selected
        .and_then(|i| state.bg_wallpapers.get(i))
        .map(String::as_str);
    let preview = container(wallpaper::preview::<Message>(sel))
        .width(Length::Fixed(320.0))
        .height(Length::Fixed(180.0))
        .style(|_| container::Style {
            border: Border {
                color: palette::color(palette::WINDOW_FRAME),
                width: 1.0,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        });
    let mut strip = Row::new().spacing(8.0);
    for (i, wp) in state.bg_wallpapers.iter().enumerate().take(24) {
        strip = strip.push(thumb(
            wp,
            state.lock_selected == Some(i),
            Message::LockSelect(i),
        ));
    }
    let greyed = |label: &'static str| {
        checkbox(label, true)
            .size(metrics::UI_PX)
            .text_size(metrics::UI_PX)
            .spacing(8.0)
            .style(mde_ui::checkbox_style)
    };
    Column::new()
        .spacing(12.0)
        .push(preview)
        .push(
            text("Background")
                .size(metrics::UI_PX)
                .color(palette::color(palette::WINDOW_TEXT)),
        )
        .push(scrollable(strip).style(mde_ui::scrollbar))
        .push(
            Row::new()
                .spacing(8.0)
                .push(
                    button(text("Browse").size(metrics::UI_PX))
                        .on_press(Message::LockBrowse)
                        .padding(Padding::from([4.0, 12.0]))
                        .style(tile_style),
                )
                .push(
                    button(text("Apply").size(metrics::UI_PX))
                        .on_press(Message::LockApply)
                        .padding(Padding::from([4.0, 16.0]))
                        .style(tile_style),
                ),
        )
        .push(greyed(
            "Show the lock screen background picture on the sign-in screen",
        ))
        .push(
            text(
                "Spotlight (rotating) and Slideshow aren't supported by the LightDM greeter \
                 (single static background); the sign-in toggle is greeter-managed. Apply writes \
                 the greeter background via pkexec (asks for your password).",
            )
            .size(metrics::UI_PX)
            .color(palette::color(palette::GRAY_TEXT)),
        )
        .into()
}

fn mode_button<'a>(label: &'a str, selected: bool, msg: Message) -> Element<'a, Message> {
    button(text(label).size(metrics::UI_PX))
        .on_press(msg)
        .padding(Padding::from([6.0, 18.0]))
        .style(move |_t, status| {
            let hot = selected || matches!(status, button::Status::Hovered);
            button::Style {
                background: Some(Background::Color(palette::color(if hot {
                    palette::HIGHLIGHT
                } else {
                    palette::MENU
                }))),
                text_color: if hot {
                    palette::color(palette::HIGHLIGHT_TEXT)
                } else {
                    palette::color(palette::MENU_TEXT)
                },
                border: Border {
                    color: if selected {
                        palette::accent()
                    } else {
                        palette::color(palette::WINDOW_FRAME)
                    },
                    width: if selected { 2.0 } else { 1.0 },
                    radius: 2.0.into(),
                },
                ..button::Style::default()
            }
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_pages_map_to_real_backends() {
        // Every non-deferred page resolves to a real backend: an mde subcommand,
        // a fedora::TOOLS command that exists, a shell command, or Colors.
        for cat in CATEGORIES {
            for p in cat.pages {
                if let Kind::Tool(cmd) = p.kind {
                    assert!(
                        tool_by_cmd(cmd).is_some(),
                        "{} has no fedora::TOOLS entry",
                        cmd
                    );
                }
            }
        }
    }

    #[test]
    fn home_has_eleven_categories_no_gaming_no_search() {
        assert_eq!(CATEGORIES.len(), 11);
        assert!(!CATEGORIES
            .iter()
            .any(|c| c.title == "Gaming" || c.title == "Search"));
    }
}
