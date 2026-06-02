//! Windows 10 Search overlay (Win+S, E5).
//!
//! A layer-shell overlay cloned from [`crate::menu`]/[`crate::popup`]:
//! transparent, all-edge anchored, keyboard-Exclusive, with an auto-focused
//! query field in a bottom-left flyout. A filter-tab row (All / Apps /
//! Documents / Web / Settings) scopes the results:
//!
//!   - **Apps** — installed `.desktop` apps (via [`apps::programs`]), launched.
//!   - **Documents** — a debounced `fd`/`find` under `$HOME`; opens the folder.
//!   - **Settings** — Win10 Settings pages mapped to mde's own surfaces.
//!   - **Web** — a "Search the web" row that hands the query to Firefox.
//!
//! Search is a Windows-10-era affordance: under any other theme `mde search`
//! falls through to the classic Start menu so Win+S still does the right thing.

use std::path::PathBuf;
use std::process::{exit, Command, ExitCode};

use iced::widget::{
    button, container, mouse_area, scrollable, text, text_input, Column, Row, Space,
};
use iced::{
    event, keyboard, Background, Border, Color, Element, Event, Length, Padding, Shadow, Task,
};
use iced_layershell::build_pattern::{application, MainSettings};
use iced_layershell::reexport::{Anchor, KeyboardInteractivity};
use iced_layershell::settings::LayerShellSettings;
use iced_layershell::{to_layer_message, Appearance};

use mde_ui::{frame, metrics, palette};

use crate::apps::{self, App};

const FILTERS: &[&str] = &["All", "Apps", "Documents", "Web", "Settings"];
const WIDTH: f32 = 440.0;
const HEIGHT: f32 = 460.0;
const MAX_HITS: usize = 40;
const QUERY_ID: &str = "mde-search-query";

#[derive(Debug, Clone, Copy, PartialEq)]
enum Filter {
    All,
    Apps,
    Documents,
    Web,
    Settings,
}

impl Filter {
    fn index(self) -> usize {
        match self {
            Filter::All => 0,
            Filter::Apps => 1,
            Filter::Documents => 2,
            Filter::Web => 3,
            Filter::Settings => 4,
        }
    }
    fn from_index(i: usize) -> Filter {
        [
            Filter::All,
            Filter::Apps,
            Filter::Documents,
            Filter::Web,
            Filter::Settings,
        ]
        .get(i)
        .copied()
        .unwrap_or(Filter::All)
    }
}

/// What activating a result does. Both end the overlay.
#[derive(Clone)]
enum Action {
    /// A shell command (in a terminal when `bool`).
    Sh(String, bool),
    /// Open a folder in the file manager (`mde files <dir>`).
    OpenFolder(String),
}

/// One result row.
struct Hit {
    /// Icon-name candidates for `icon_any` (most specific first).
    icon: Vec<String>,
    label: String,
    action: Action,
}

struct Search {
    query: String,
    filter: Filter,
    apps: Vec<App>,
    /// Cached document hits (name, parent dir) + the query they were found for,
    /// so the `fd`/`find` shell-out is debounced to the tick, not per keystroke.
    docs: Vec<(String, String)>,
    docs_query: String,
}

#[to_layer_message]
#[derive(Debug, Clone)]
enum Message {
    Query(String),
    Pick(usize),     // filter tab
    Activate(usize), // result row
    Tick,            // debounce point for the document search
    Close,
    Event(Event),
}

pub fn run(args: &[String]) -> ExitCode {
    // Era guard: Search belongs to the Windows 10 era. Under any other theme,
    // fall through to the classic Start menu so the Win+S keybind still opens
    // something familiar rather than nothing.
    if crate::state::load().theme != "windows10" {
        let mde = std::env::current_exe()
            .ok()
            .and_then(|p| p.to_str().map(String::from))
            .unwrap_or_else(|| "mde".to_string());
        let _ = Command::new(mde).arg("menu").spawn();
        return ExitCode::SUCCESS;
    }
    // No compositor → exit cleanly rather than panic in the layer-shell init.
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return ExitCode::SUCCESS;
    }
    // Optional initial query: `mde search firefox` opens pre-filled (handy as a
    // launcher target, and the only way to script a non-empty capture).
    let initial = args.join(" ").trim().to_string();
    match launch(initial) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mde search: {e}");
            ExitCode::FAILURE
        }
    }
}

fn launch(initial: String) -> Result<(), iced_layershell::Error> {
    application(namespace, update, view)
        .style(style)
        .subscription(|_: &Search| {
            iced::Subscription::batch([
                iced::time::every(std::time::Duration::from_millis(300)).map(|_| Message::Tick),
                event::listen_with(|event, _status, _window| match event {
                    Event::Keyboard(_) => Some(Message::Event(event)),
                    _ => None,
                }),
            ])
        })
        .font(mde_ui::font::REGULAR_BYTES)
        .font(mde_ui::font::BOLD_BYTES)
        .font(mde_ui::font::PLEX_REGULAR_BYTES)
        .font(mde_ui::font::PLEX_BOLD_BYTES)
        .default_font(mde_ui::font::ui())
        .settings(MainSettings {
            layer_settings: LayerShellSettings {
                anchor: Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right,
                exclusive_zone: 0,
                keyboard_interactivity: KeyboardInteractivity::Exclusive,
                ..Default::default()
            },
            ..Default::default()
        })
        .run_with(move || {
            (
                Search {
                    query: initial,
                    filter: Filter::All,
                    apps: flat_apps(),
                    docs: Vec::new(),
                    docs_query: String::new(),
                },
                text_input::focus(text_input::Id::new(QUERY_ID)),
            )
        })
}

fn namespace(_: &Search) -> String {
    "mde-search".to_string()
}

fn style(_: &Search, _: &iced::Theme) -> Appearance {
    Appearance {
        background_color: Color::TRANSPARENT,
        text_color: palette::color(palette::MENU_TEXT),
    }
}

fn update(state: &mut Search, message: Message) -> Task<Message> {
    match message {
        Message::Query(s) => state.query = s,
        Message::Pick(i) => state.filter = Filter::from_index(i),
        Message::Activate(i) => {
            if let Some(hit) = current_hits(state).into_iter().nth(i) {
                run_action(&hit.action);
            }
            exit(0)
        }
        Message::Tick => refresh_docs(state),
        Message::Close => exit(0),
        Message::Event(Event::Keyboard(keyboard::Event::KeyPressed { key, .. })) => match key {
            keyboard::Key::Named(keyboard::key::Named::Escape) => exit(0),
            keyboard::Key::Named(keyboard::key::Named::Enter) => {
                // Enter activates the first ("best match") result.
                if let Some(hit) = current_hits(state).into_iter().next() {
                    run_action(&hit.action);
                }
                exit(0)
            }
            _ => {}
        },
        _ => {}
    }
    Task::none()
}

fn run_action(action: &Action) {
    match action {
        Action::Sh(cmd, terminal) => {
            if *terminal {
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
        Action::OpenFolder(dir) => {
            let mde = std::env::current_exe()
                .ok()
                .and_then(|p| p.to_str().map(String::from))
                .unwrap_or_else(|| "mde".to_string());
            let _ = Command::new(mde).arg("files").arg(dir).spawn();
        }
    }
}

// --- backends --------------------------------------------------------------

/// All installed apps, flattened out of the category groups.
fn flat_apps() -> Vec<App> {
    apps::programs()
        .into_iter()
        .flat_map(|(_, apps)| apps)
        .collect()
}

/// Refresh the cached document hits when the (≥2 char) query changed and the
/// Documents or All tab is showing — the debounce that keeps `fd`/`find` off the
/// per-keystroke path.
fn refresh_docs(state: &mut Search) {
    let q = state.query.trim().to_string();
    let wants = matches!(state.filter, Filter::Documents | Filter::All);
    if wants && q.len() >= 2 && q != state.docs_query {
        state.docs = run_docs(&q);
        state.docs_query = q;
    } else if q.len() < 2 && !state.docs.is_empty() {
        state.docs.clear();
        state.docs_query.clear();
    }
}

/// Find files under `$HOME` matching `q`, newest tools first: `fd` if present,
/// else `find`. Args are passed directly (no shell), so the free-text query
/// can't inject. Returns `(file_name, parent_dir)` pairs, capped.
fn run_docs(q: &str) -> Vec<(String, String)> {
    let Ok(home) = std::env::var("HOME") else {
        return Vec::new();
    };
    if home.is_empty() {
        return Vec::new();
    }
    let fd = Command::new("fd")
        .args([
            "--type",
            "f",
            "--ignore-case",
            "--absolute-path",
            "--max-results",
            "30",
        ])
        .arg(q)
        .arg(&home)
        .output();
    let bytes = match fd {
        Ok(o) if o.status.success() => o.stdout,
        _ => Command::new("find")
            .arg(&home)
            .args(["-maxdepth", "5", "-type", "f", "-iname"])
            .arg(format!("*{q}*"))
            .output()
            .map(|o| o.stdout)
            .unwrap_or_default(),
    };
    String::from_utf8_lossy(&bytes)
        .lines()
        .take(MAX_HITS)
        .filter_map(|line| {
            let p = PathBuf::from(line);
            let name = p.file_name()?.to_string_lossy().into_owned();
            let parent = p.parent()?.to_string_lossy().into_owned();
            Some((name, parent))
        })
        .collect()
}

/// Win10 Settings search hits, mapped to mde surfaces. The modern Settings app
/// (`mde settings`) now ships; these specific hits still deep-link to the
/// matching legacy property sheets (System Properties / Display / Control
/// Panel), the most direct target for each query.
fn settings_pages() -> Vec<(&'static str, Action)> {
    let mde = std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_else(|| "mde".to_string());
    vec![
        (
            "System",
            Action::Sh(format!("'{mde}' system-properties --info"), false),
        ),
        (
            "Devices",
            Action::Sh(format!("'{mde}' system-properties --devices"), false),
        ),
        ("Display", Action::Sh(format!("'{mde}' display"), false)),
        (
            "Network & Internet",
            Action::Sh("nm-connection-editor".into(), false),
        ),
        (
            "Personalization",
            Action::Sh(format!("'{mde}' display"), false),
        ),
        (
            "Control Panel",
            Action::Sh(format!("'{mde}' control-panel"), false),
        ),
    ]
}

/// Percent-encode a query for a URL (encode everything outside the unreserved
/// set), so the web row's DuckDuckGo URL is well-formed and injection-free.
fn url_encode(q: &str) -> String {
    let mut out = String::with_capacity(q.len());
    for b in q.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Build the result list for the current query + filter.
fn current_hits(state: &Search) -> Vec<Hit> {
    let q = state.query.trim().to_lowercase();
    match state.filter {
        Filter::Apps => app_hits(&state.apps, &q),
        Filter::Documents => doc_hits(&state.docs),
        Filter::Settings => settings_hits(&q),
        Filter::Web => web_hits(&state.query),
        Filter::All => {
            if q.is_empty() {
                return Vec::new();
            }
            let mut v = app_hits(&state.apps, &q);
            v.extend(doc_hits(&state.docs));
            v.extend(settings_hits(&q));
            v.extend(web_hits(&state.query));
            v.truncate(MAX_HITS);
            v
        }
    }
}

fn app_hits(apps: &[App], q: &str) -> Vec<Hit> {
    apps.iter()
        .filter(|a| q.is_empty() || a.name.to_lowercase().contains(q))
        .take(MAX_HITS)
        .map(|a| {
            // Guess an icon from the exec's leading token basename.
            let base = a
                .exec
                .split_whitespace()
                .next()
                .and_then(|t| t.rsplit('/').next())
                .unwrap_or("")
                .to_string();
            Hit {
                icon: vec![
                    base,
                    a.name.to_lowercase(),
                    "application-x-executable".into(),
                ],
                label: a.name.clone(),
                action: Action::Sh(a.exec.clone(), a.terminal),
            }
        })
        .collect()
}

fn doc_hits(docs: &[(String, String)]) -> Vec<Hit> {
    docs.iter()
        .map(|(name, parent)| Hit {
            icon: vec!["text-x-generic".into(), "application-x-generic".into()],
            label: name.clone(),
            action: Action::OpenFolder(parent.clone()),
        })
        .collect()
}

fn settings_hits(q: &str) -> Vec<Hit> {
    settings_pages()
        .into_iter()
        .filter(|(label, _)| q.is_empty() || label.to_lowercase().contains(q))
        .map(|(label, action)| Hit {
            icon: vec!["preferences-system".into(), "applications-system".into()],
            label: label.to_string(),
            action,
        })
        .collect()
}

fn web_hits(query: &str) -> Vec<Hit> {
    let q = query.trim();
    if q.is_empty() {
        return Vec::new();
    }
    let url = format!("https://duckduckgo.com/?q={}", url_encode(q));
    vec![Hit {
        icon: vec!["web-browser".into(), "firefox".into()],
        label: format!("Search the web for \"{q}\""),
        action: Action::Sh(format!("firefox '{url}'"), false),
    }]
}

// --- view ------------------------------------------------------------------

fn row_style(_t: &iced::Theme, status: button::Status) -> button::Style {
    let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: hot.then(|| Background::Color(palette::color(palette::HIGHLIGHT))),
        text_color: if hot {
            palette::color(palette::HIGHLIGHT_TEXT)
        } else {
            palette::color(palette::MENU_TEXT)
        },
        border: Border::default(),
        shadow: Shadow::default(),
    }
}

fn view(state: &Search) -> Element<'_, Message> {
    let field = text_input("Type here to search", &state.query)
        .id(text_input::Id::new(QUERY_ID))
        .on_input(Message::Query)
        .padding(8.0)
        .size(metrics::UI_PX);

    let tabs = mde_ui::tab_strip(FILTERS, state.filter.index(), Message::Pick);

    let hits = current_hits(state);
    let results: Element<Message> = if hits.is_empty() {
        let msg = if state.query.trim().is_empty() {
            "Type to search apps, documents, settings and the web"
        } else {
            "No results"
        };
        container(
            text(msg)
                .size(metrics::UI_PX)
                .color(palette::color(palette::GRAY_TEXT)),
        )
        .padding(12.0)
        .into()
    } else {
        let mut col = Column::new().spacing(1.0);
        for (i, hit) in hits.iter().enumerate() {
            let ids: Vec<&str> = hit.icon.iter().map(String::as_str).collect();
            let row = Row::new()
                .spacing(8.0)
                .align_y(iced::alignment::Vertical::Center)
                .push(crate::icons::icon_any(&ids, 22))
                .push(text(hit.label.clone()).size(metrics::UI_PX));
            col = col.push(
                button(row)
                    .on_press(Message::Activate(i))
                    .width(Length::Fill)
                    .padding(Padding::from([4.0, 8.0]))
                    .style(row_style),
            );
        }
        scrollable(col).style(mde_ui::scrollbar).into()
    };

    let body = Column::new()
        .spacing(6.0)
        .padding(8.0)
        .push(field)
        .push(tabs)
        .push(container(results).height(Length::Fill));

    let panel = container(iced::widget::stack![frame::raised(), body])
        .width(Length::Fixed(WIDTH))
        .height(Length::Fixed(HEIGHT));

    // Bottom-left flyout (above the taskbar), with a full-screen catcher to close.
    let positioned = Column::new()
        .push(Space::with_height(Length::Fill))
        .push(Row::new().push(panel).push(Space::with_width(Length::Fill)))
        .push(Space::with_height(Length::Fixed(2.0)));

    mouse_area(container(positioned).padding(Padding {
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        left: 2.0,
    }))
    .on_press(Message::Close)
    .on_right_press(Message::Close)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_index_round_trips() {
        for f in [
            Filter::All,
            Filter::Apps,
            Filter::Documents,
            Filter::Web,
            Filter::Settings,
        ] {
            assert_eq!(Filter::from_index(f.index()), f);
        }
    }

    #[test]
    fn url_encode_is_safe() {
        assert_eq!(url_encode("a b&c"), "a%20b%26c");
        assert_eq!(url_encode("plain"), "plain");
    }

    #[test]
    fn web_hit_only_when_query() {
        assert!(web_hits("  ").is_empty());
        let h = web_hits("rust");
        assert_eq!(h.len(), 1);
        assert!(matches!(&h[0].action, Action::Sh(c, false) if c.contains("duckduckgo")));
    }

    #[test]
    fn settings_filter_matches_substring() {
        assert!(settings_hits("display")
            .iter()
            .any(|h| h.label == "Display"));
        // Empty query lists all the mapped pages.
        assert_eq!(settings_hits("").len(), settings_pages().len());
    }
}
