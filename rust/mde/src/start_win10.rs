//! Windows 10 tiled Start.
//!
//! No-arg `mde start-win10` opens the three-region overlay (left icon rail ·
//! center All-Apps list · right tile grid) as a bottom-left layer-shell surface
//! above the Win10 taskbar; a backdrop click or Esc closes it. The `--*` flags
//! are the headless tile-management CLI (bench-testable without the GUI):
//!
//!   mde start-win10                                 open the tiled Start
//!   mde start-win10 --list-tiles
//!   mde start-win10 --pin <name> <command>
//!   mde start-win10 --unpin <name>
//!   mde start-win10 --resize <name> <small|medium|wide|large>

use std::process::{exit, ExitCode};

use iced::alignment::{Horizontal, Vertical};
use iced::widget::{button, container, mouse_area, row, scrollable, text, Column, Row, Space};
use iced::{
    event, keyboard, Background, Border, Color, Element, Event, Length, Padding, Shadow, Task,
};
use iced_layershell::build_pattern::{application, MainSettings};
use iced_layershell::reexport::{Anchor, KeyboardInteractivity};
use iced_layershell::settings::LayerShellSettings;
use iced_layershell::{to_layer_message, Appearance};

use mde_ui::{metrics, palette};

use crate::state::{self, MenuState, StartTile, TileSize};
use crate::{apps, start_common};

const USAGE: &str = "\
mde start-win10 — Windows 10 Start (no arg opens the tiled Start)
  --list-tiles                list the Start tiles (seeded from pinned items on a fresh config)
  --pin <name> <command>      pin a Medium tile (replacing one of the same name)
  --unpin <name>              remove the tile named <name>
  --resize <name> <size>      set tile size: small | medium | wide | large
";

// --- layout (px) — local layout constants, like panel.rs's bar heights --------
const BAR_H: f32 = 40.0; // mirrors panel::WIN10_BAR_H (the Win10 taskbar height)
const RAIL_W: f32 = 48.0; // the icon rail width
const TILE_CELL: f32 = 48.0; // base small-tile cell; bigger sizes derive via span()
const GAP: f32 = 4.0;
const COL_W: f32 = 260.0; // the All-Apps center column
const TILES_W: f32 = 4.0 * TILE_CELL + 3.0 * GAP; // Field Guide default: 4 medium-tiles wide
const PANEL_H: f32 = 560.0;

pub fn run(args: &[String]) -> ExitCode {
    match args.first().map(String::as_str) {
        Some("--list-tiles") => list_tiles(),
        Some("--pin") => pin_tile(&args[1..]),
        Some("--unpin") => unpin_tile(&args[1..]),
        Some("--resize") => resize_tile(&args[1..]),
        Some("--help") | Some("-h") => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some(flag) if flag.starts_with('-') => {
            eprintln!("mde start-win10: unknown option {flag}\n{USAGE}");
            ExitCode::from(2)
        }
        // No arg (or a stray positional) → the GUI overlay.
        _ => gui(),
    }
}

// --- headless tile CLI -------------------------------------------------------

fn list_tiles() -> ExitCode {
    for t in state::seed_start_tiles(&state::load()) {
        let (cols, rows) = t.size.span();
        println!(
            "{}\t{}\t{}\t{}x{}\t{}",
            t.name,
            t.command,
            t.size.token(),
            cols,
            rows,
            t.group
        );
    }
    ExitCode::SUCCESS
}

fn pin_tile(args: &[String]) -> ExitCode {
    let (Some(name), Some(command)) = (args.first(), args.get(1)) else {
        eprintln!("mde start-win10 --pin <name> <command>");
        return ExitCode::FAILURE;
    };
    let mut st = materialized();
    st.start_tiles.retain(|t| t.name != *name);
    st.start_tiles.push(StartTile {
        name: name.clone(),
        command: command.clone(),
        icon: String::new(),
        size: TileSize::Medium,
        group: String::new(),
    });
    persist(&st)
}

fn unpin_tile(args: &[String]) -> ExitCode {
    let Some(name) = args.first() else {
        eprintln!("mde start-win10 --unpin <name>");
        return ExitCode::FAILURE;
    };
    let mut st = materialized();
    st.start_tiles.retain(|t| t.name != *name);
    persist(&st)
}

fn resize_tile(args: &[String]) -> ExitCode {
    let (Some(name), Some(size)) = (args.first(), args.get(1)) else {
        eprintln!("mde start-win10 --resize <name> <small|medium|wide|large>");
        return ExitCode::FAILURE;
    };
    let sz = TileSize::from_token(size);
    let mut st = materialized();
    let mut hit = false;
    for t in st.start_tiles.iter_mut().filter(|t| t.name == *name) {
        t.size = sz;
        hit = true;
    }
    if !hit {
        eprintln!("mde start-win10: no tile named {name:?}");
        return ExitCode::FAILURE;
    }
    persist(&st)
}

/// Load state with the seed materialized into `start_tiles`, so a mutation never
/// silently drops the first-run seed (the pins) on the floor.
fn materialized() -> MenuState {
    let mut st = state::load();
    st.start_tiles = state::seed_start_tiles(&st);
    st
}

fn persist(st: &MenuState) -> ExitCode {
    match state::save(st) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mde start-win10: save failed: {e}");
            ExitCode::FAILURE
        }
    }
}

// --- the GUI overlay ---------------------------------------------------------

/// One installed application, flattened out of the per-folder `apps::programs()`.
#[derive(Clone)]
struct AppEntry {
    name: String,
    exec: String,
    terminal: bool,
    mtime: u64,
}

struct Start {
    apps: Vec<AppEntry>,
    /// Newest-installed apps (by `.desktop` mtime) — the "Recently added" section.
    recent: Vec<AppEntry>,
    /// Most-launched pins (by `launch_count`) — the "Suggested" section.
    suggested: Vec<(String, String)>,
    tiles: Vec<StartTile>,
}

#[to_layer_message]
#[derive(Debug, Clone)]
enum Message {
    Launch(String, bool), // shell command, run-in-terminal
    Mde(String),          // re-exec this binary with a subcommand (Power, …)
    Close,
    Event(Event),
}

fn gui() -> ExitCode {
    // Singleton: a second Win key press while Start is open is a duplicate; exit
    // quietly rather than stacking another full-screen overlay. Guards its own
    // pid slot (mde-start-win10), distinct from the Carbon menu's.
    if !start_common::acquire_singleton("mde-start-win10") {
        return ExitCode::SUCCESS;
    }
    match launch() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mde start-win10: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Installed apps, flattened across folders, deduped, sorted case-insensitively.
fn all_apps() -> Vec<AppEntry> {
    let mut v: Vec<AppEntry> = apps::programs()
        .into_iter()
        .flat_map(|(_, apps)| apps)
        .map(|a| AppEntry {
            name: a.name,
            exec: a.exec,
            terminal: a.terminal,
            mtime: a.mtime,
        })
        .collect();
    v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    v.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));
    v
}

/// The N newest-installed apps (by `.desktop` mtime), for "Recently added".
fn recent_apps(apps: &[AppEntry], n: usize) -> Vec<AppEntry> {
    let mut by_mtime: Vec<AppEntry> = apps.iter().filter(|a| a.mtime > 0).cloned().collect();
    by_mtime.sort_by(|a, b| b.mtime.cmp(&a.mtime));
    by_mtime.truncate(n);
    by_mtime
}

/// The most-launched pins (launch_count > 0, descending), for "Suggested".
fn suggested_pins(state: &MenuState, n: usize) -> Vec<(String, String)> {
    let mut pins: Vec<_> = state.pinned.iter().filter(|p| p.launch_count > 0).collect();
    pins.sort_by(|a, b| b.launch_count.cmp(&a.launch_count));
    pins.into_iter()
        .take(n)
        .map(|p| (p.name.clone(), p.command.clone()))
        .collect()
}

fn launch() -> Result<(), iced_layershell::Error> {
    application(namespace, update, view)
        .style(style)
        .subscription(subscription)
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
        .run_with(|| {
            let st = state::load();
            let apps = all_apps();
            let recent = recent_apps(&apps, 5);
            let suggested = suggested_pins(&st, 4);
            (
                Start {
                    recent,
                    suggested,
                    tiles: state::seed_start_tiles(&st),
                    apps,
                },
                Task::none(),
            )
        })
}

fn namespace(_: &Start) -> String {
    "mde-start-win10".to_string()
}

fn style(_: &Start, _: &iced::Theme) -> Appearance {
    Appearance {
        background_color: Color::TRANSPARENT,
        text_color: palette::color(palette::MENU_TEXT),
    }
}

fn subscription(_: &Start) -> iced::Subscription<Message> {
    event::listen_with(|event, _status, _window| match event {
        Event::Keyboard(_) => Some(Message::Event(event)),
        _ => None,
    })
}

/// Bump a pin's launch_count (the Suggested ranking) when its command launches
/// from Start, then persist. No-op when the launched command isn't pinned.
fn bump_launch_count(cmd: &str) {
    let mut st = state::load();
    let mut hit = false;
    for p in st.pinned.iter_mut().filter(|p| p.command == cmd) {
        p.launch_count = p.launch_count.saturating_add(1);
        hit = true;
    }
    if hit {
        let _ = state::save(&st);
    }
}

fn update(_: &mut Start, message: Message) -> Task<Message> {
    match message {
        Message::Launch(cmd, terminal) => {
            bump_launch_count(&cmd);
            start_common::launch_cmd(&cmd, terminal);
            exit(0);
        }
        Message::Mde(sub) => {
            start_common::mde_self(&sub);
            exit(0);
        }
        Message::Close => exit(0),
        Message::Event(Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::Escape),
            ..
        })) => exit(0),
        _ => Task::none(),
    }
}

// --- view --------------------------------------------------------------------

fn view(start: &Start) -> Element<'_, Message> {
    let regions = Row::new()
        .spacing(GAP)
        .push(rail())
        .push(container(center_column(start)).width(Length::Fixed(COL_W)))
        .push(container(tiles_view(&start.tiles)).width(Length::Fixed(TILES_W + 16.0)));

    let panel = container(container(regions).padding(8.0))
        .height(Length::Fixed(PANEL_H))
        .style(|_| container::Style {
            background: Some(Background::Color(palette::color(palette::MENU))),
            border: Border {
                color: palette::color(palette::WINDOW_FRAME),
                width: 1.0,
                radius: 2.0.into(),
            },
            shadow: Shadow {
                color: Color {
                    a: 0.35,
                    ..Color::BLACK
                },
                offset: iced::Vector::new(0.0, 2.0),
                blur_radius: 12.0,
            },
            ..container::Style::default()
        });

    // Backdrop click-catcher closes Start; the panel sits bottom-left above the bar.
    iced::widget::stack![
        mouse_area(Space::new(Length::Fill, Length::Fill)).on_press(Message::Close),
        container(panel)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Horizontal::Left)
            .align_y(Vertical::Bottom)
            .padding(Padding {
                top: 0.0,
                right: 0.0,
                bottom: BAR_H + 2.0,
                left: 2.0,
            }),
    ]
    .into()
}

/// The left icon rail: account avatar at the top, Power at the bottom. (System
/// folders, Settings, and the hover-to-expand flyout are E1.4.)
fn rail() -> Element<'static, Message> {
    let avatar = container(crate::icons::icon_any(
        &["system-users", "avatar-default"],
        32,
    ))
    .padding(Padding {
        top: 4.0,
        right: 0.0,
        bottom: 0.0,
        left: 0.0,
    })
    .center_x(Length::Fill);
    let power = button(
        container(crate::icons::icon_any(
            &["system-shutdown", "system-log-out"],
            24,
        ))
        .center_x(Length::Fill),
    )
    .on_press(Message::Mde("shutdown".into()))
    .width(Length::Fill)
    .style(start_common::tile_style());
    container(
        Column::new()
            .width(Length::Fixed(RAIL_W))
            .height(Length::Fill)
            .push(avatar)
            .push(Space::new(Length::Fill, Length::Fill))
            .push(power),
    )
    .into()
}

/// An accent-colored section / alphabet group header.
fn accent_header(label: String) -> Element<'static, Message> {
    container(
        text(label)
            .size(metrics::UI_PX)
            .style(|_: &iced::Theme| text::Style {
                color: Some(palette::accent()),
            }),
    )
    .padding(Padding {
        top: 4.0,
        right: 0.0,
        bottom: 1.0,
        left: 6.0,
    })
    .into()
}

/// A launchable app/pin row: icon + name, accent highlight on hover.
fn app_row(name: String, exec: String, terminal: bool) -> Element<'static, Message> {
    let icon = crate::icons::icon_any(&[name.to_lowercase().as_str()], 16);
    button(
        row![icon, text(name).size(metrics::UI_PX)]
            .spacing(8.0)
            .align_y(Vertical::Center),
    )
    .on_press(Message::Launch(exec, terminal))
    .width(Length::Fill)
    .padding(Padding {
        top: 3.0,
        right: 6.0,
        bottom: 3.0,
        left: 6.0,
    })
    .style(row_style())
    .into()
}

/// The center column: Recently added + Suggested (from real data, hidden when
/// empty) then the #/A–Z All-Apps list — all in one scrollable.
fn center_column(start: &Start) -> Element<'static, Message> {
    let mut col = Column::new().spacing(1.0).width(Length::Fill);
    if !start.recent.is_empty() {
        col = col.push(accent_header("Recently added".into()));
        for a in &start.recent {
            col = col.push(app_row(a.name.clone(), a.exec.clone(), a.terminal));
        }
    }
    if !start.suggested.is_empty() {
        col = col.push(accent_header("Suggested".into()));
        for (name, cmd) in &start.suggested {
            col = col.push(app_row(name.clone(), cmd.clone(), false));
        }
    }
    let mut last = '\0';
    for a in &start.apps {
        let initial = a
            .name
            .chars()
            .next()
            .map(|c| {
                if c.is_ascii_alphabetic() {
                    c.to_ascii_uppercase()
                } else {
                    '#'
                }
            })
            .unwrap_or('#');
        if initial != last {
            last = initial;
            col = col.push(accent_header(initial.to_string()));
        }
        col = col.push(app_row(a.name.clone(), a.exec.clone(), a.terminal));
    }
    scrollable(col).style(mde_ui::scrollbar).into()
}

/// The right tile grid: `start_tiles` in named groups, sized by `TileSize::span`.
fn tiles_view(tiles: &[StartTile]) -> Element<'_, Message> {
    // Group preserving first-seen order.
    let mut groups: Vec<(String, Vec<&StartTile>)> = Vec::new();
    for t in tiles {
        match groups.iter_mut().find(|(g, _)| *g == t.group) {
            Some((_, v)) => v.push(t),
            None => groups.push((t.group.clone(), vec![t])),
        }
    }
    let mut out = Column::new().spacing(8.0).width(Length::Fill);
    for (name, gtiles) in groups {
        let header: String = if name.is_empty() {
            "Pinned".into()
        } else {
            name
        };
        out = out.push(
            text(header)
                .size(metrics::UI_PX)
                .style(|_: &iced::Theme| text::Style {
                    color: Some(palette::accent()),
                }),
        );
        // Greedy wrap at TILES_W.
        let mut grid = Column::new().spacing(GAP);
        let mut roww = Row::new().spacing(GAP);
        let mut used = 0.0;
        for t in gtiles {
            let (cols, rows) = t.size.span();
            let w = cols as f32 * TILE_CELL + (cols as f32 - 1.0) * GAP;
            let h = rows as f32 * TILE_CELL + (rows as f32 - 1.0) * GAP;
            if used + w > TILES_W && used > 0.0 {
                grid = grid.push(roww);
                roww = Row::new().spacing(GAP);
                used = 0.0;
            }
            let key = t.icon.to_lowercase();
            let cmd = t
                .command
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_string();
            let icon = crate::icons::icon_any(&[key.as_str(), cmd.as_str()], 32);
            roww = roww.push(start_common::tile(
                icon,
                t.name.as_str(),
                Message::Launch(t.command.clone(), false),
                None,
                w,
                h,
            ));
            used += w + GAP;
        }
        grid = grid.push(roww);
        out = out.push(grid);
    }
    scrollable(out).style(mde_ui::scrollbar).into()
}

/// A full-width All-Apps row: transparent at rest, accent highlight on hover.
fn row_style() -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    |_theme, status| {
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
}
