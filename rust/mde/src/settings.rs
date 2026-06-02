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

use iced::widget::{button, column, container, mouse_area, scrollable, text, Column, Row, Space};
use iced::{Background, Border, Color, Element, Length, Padding, Task};

use mde_ui::{metrics, palette};

use crate::fedora;

const COLS: usize = 4;

/// What a settings page does when opened.
#[derive(Clone, Copy)]
enum Kind {
    /// Greyed rail entry; the pane states it's a later milestone.
    Deferred,
    /// The native Personalization ▸ Colors page (E6.4).
    Colors,
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
                title: "Colors",
                kind: Kind::Colors,
            },
            Page {
                title: "Background",
                kind: Kind::Mde("display"),
            },
            Page {
                title: "Lock screen",
                kind: Kind::Deferred,
            },
            Page {
                title: "Themes",
                kind: Kind::Deferred,
            },
            Page {
                title: "Start",
                kind: Kind::Deferred,
            },
            Page {
                title: "Taskbar",
                kind: Kind::Deferred,
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
                kind: Kind::Tool("dnfdragora"),
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
        caption: "Windows Update, recovery, backup",
        icons: &["system-software-update", "security-high"],
        pages: &[
            Page {
                title: "Windows Security",
                kind: Kind::Tool("firewall-config"),
            },
            Page {
                title: "Windows Update",
                kind: Kind::Cmd("dnfdragora --update-only", false),
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
    accent: u8,
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
    SetAccent(u8),
}

pub fn run(args: &[String]) -> ExitCode {
    if matches!(args.first().map(String::as_str), Some("--list")) {
        return list();
    }
    // Optional deep-link: `mde settings personalization` opens straight to that
    // category (a Win10 `ms-settings:`-style entry, and the way to script a
    // drill-in capture).
    let initial = args.first().and_then(|a| category_index(a));
    match gui(initial) {
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

/// `mde settings --list` — print the page → backend map for every live page.
fn list() -> ExitCode {
    for cat in CATEGORIES {
        for p in cat.pages {
            let backend = match p.kind {
                Kind::Deferred => continue,
                Kind::Colors => "(native: Colors)".to_string(),
                Kind::Mde(s) => format!("mde {s}"),
                Kind::Tool(c) => format!("tool: {c}"),
                Kind::Cmd(c, _) => format!("cmd: {c}"),
            };
            println!("{} \u{25b8} {} -> {}", cat.title, p.title, backend);
        }
    }
    ExitCode::SUCCESS
}

fn gui(initial: Option<usize>) -> iced::Result {
    iced::application(|_: &Settings| "Settings - mde".to_string(), update, view)
        .theme(|_| iced::Theme::Light)
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
                page: 0,
                dark: st.theme_mode != "light",
                accent: accent_index(&st.icon_color),
                installed: HashMap::new(),
            };
            cache_install(&mut s);
            (s, Task::none())
        })
}

fn accent_index(key: &str) -> u8 {
    match key {
        "blue" => 0,
        "orange" => 1,
        "red" => 2,
        _ => 3,
    }
}

fn accent_key(idx: u8) -> &'static str {
    match idx {
        0 => "blue",
        1 => "orange",
        2 => "red",
        _ => "neutral",
    }
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
        Message::Open => open_current(state),
        Message::SetDark(d) => {
            state.dark = d;
            palette::set_dark(d);
            persist(state);
        }
        Message::SetAccent(a) => {
            state.accent = a;
            palette::set_accent(a);
            persist(state);
        }
    }
    Task::none()
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
        Kind::Deferred | Kind::Colors => {}
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
    st.icon_color = accent_key(state.accent).to_string();
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
        View::Home => home(),
        View::Category(c) => category(state, c),
    };
    container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(16.0)
        .style(bg)
        .into()
}

fn home() -> Element<'static, Message> {
    let title = text("Settings")
        .size(metrics::INFO_TITLE_PX)
        .color(palette::color(palette::WINDOW_TEXT));
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
    Column::new()
        .spacing(16.0)
        .push(title)
        .push(scrollable(grid).style(mde_ui::scrollbar))
        .into()
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

/// Personalization ▸ Colors (E6.4): Light/Dark choice + accent swatches. Both
/// re-skin the window live and persist to `state.rs`.
fn colors_page(state: &Settings) -> Element<'_, Message> {
    let mode_label = text("Choose your color")
        .size(metrics::UI_PX)
        .color(palette::color(palette::WINDOW_TEXT));
    let light = mode_button("Light", !state.dark, Message::SetDark(false));
    let dark = mode_button("Dark", state.dark, Message::SetDark(true));
    let modes = Row::new().spacing(8.0).push(light).push(dark);

    let accent_label = text("Choose your accent color")
        .size(metrics::UI_PX)
        .color(palette::color(palette::WINDOW_TEXT));
    let mut swatches = Row::new().spacing(8.0);
    for idx in 0u8..4 {
        swatches = swatches.push(accent_swatch(idx, idx == state.accent, state.dark));
    }

    column![
        mode_label,
        modes,
        Space::with_height(Length::Fixed(8.0)),
        accent_label,
        swatches
    ]
    .spacing(8.0)
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

fn accent_swatch<'a>(idx: u8, selected: bool, dark: bool) -> Element<'a, Message> {
    let color = palette::color(palette::icon_accent(idx, dark));
    let sw = container(Space::new(Length::Fixed(34.0), Length::Fixed(34.0))).style(move |_t| {
        container::Style {
            background: Some(Background::Color(color)),
            border: Border {
                color: if selected {
                    palette::color(palette::WINDOW_TEXT)
                } else {
                    palette::color(palette::WINDOW_FRAME)
                },
                width: if selected { 2.0 } else { 1.0 },
                radius: 2.0.into(),
            },
            ..container::Style::default()
        }
    });
    mouse_area(sw).on_press(Message::SetAccent(idx)).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accent_key_index_round_trip() {
        for idx in 0u8..4 {
            assert_eq!(accent_index(accent_key(idx)), idx);
        }
    }

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
