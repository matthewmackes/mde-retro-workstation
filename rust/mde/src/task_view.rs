//! Windows 10 Task View (WINKEY+Tab, E4) — a full-screen dimmed overlay showing
//! every open window as a labelled tile; clicking a tile focuses that window.
//!
//! The window snapshot comes from wlr-foreign-toplevel (`wlr::Wm`, its own
//! background client like the panel's). mde NEVER moves windows — labwc owns
//! geometry; Task View only reads the list and asks labwc to activate one.
//!
//!   mde task-view
//!
//! The virtual-desktop band (ext-workspace) + Snap Assist are later E4 stories.

use std::process::{exit, ExitCode};
use std::time::Duration;

use iced::alignment::{Horizontal, Vertical};
use iced::widget::{container, mouse_area, scrollable, text, Column, Row, Space};
use iced::{event, keyboard, Background, Border, Color, Element, Event, Length, Shadow, Task};
use iced_layershell::build_pattern::{application, MainSettings};
use iced_layershell::reexport::{Anchor, KeyboardInteractivity};
use iced_layershell::settings::LayerShellSettings;
use iced_layershell::{to_layer_message, Appearance};

use mde_ui::{metrics, palette};

use crate::wlr;

const COLS: usize = 4;

struct TaskView {
    wm: Option<wlr::Wm>,
    windows: Vec<wlr::Window>,
}

#[to_layer_message]
#[derive(Debug, Clone)]
enum Message {
    Tick,       // re-read the window snapshot (it fills in asynchronously)
    Focus(u64), // raise/focus the clicked window, then close
    Close,
    Event(Event),
}

pub fn run(_args: &[String]) -> ExitCode {
    // No compositor → nothing to show; exit cleanly rather than panic in the
    // layer-shell init.
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return ExitCode::SUCCESS;
    }
    match launch() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mde task-view: {e}");
            ExitCode::FAILURE
        }
    }
}

fn launch() -> Result<(), iced_layershell::Error> {
    application(|_: &TaskView| "mde-task-view".to_string(), update, view)
        .style(|_: &TaskView, _: &iced::Theme| Appearance {
            // A dimmed scrim over the desktop.
            background_color: Color {
                a: 0.82,
                ..Color::BLACK
            },
            text_color: palette::color(palette::TITLE_TEXT),
        })
        .subscription(|_: &TaskView| {
            iced::Subscription::batch([
                iced::time::every(Duration::from_millis(250)).map(|_| Message::Tick),
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
        .run_with(|| {
            let wm = wlr::start();
            let windows = wm.as_ref().map(|w| w.windows()).unwrap_or_default();
            (TaskView { wm, windows }, Task::none())
        })
}

fn update(state: &mut TaskView, message: Message) -> Task<Message> {
    match message {
        Message::Tick => {
            if let Some(wm) = &state.wm {
                state.windows = wm.windows();
            }
        }
        Message::Focus(id) => {
            if let Some(wm) = &state.wm {
                wm.focus(id);
            }
            exit(0)
        }
        Message::Close => exit(0),
        Message::Event(Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::Escape),
            ..
        })) => exit(0),
        _ => {}
    }
    Task::none()
}

fn view(state: &TaskView) -> Element<'_, Message> {
    let content: Element<Message> = if state.windows.is_empty() {
        text("No open windows")
            .size(metrics::INFO_TITLE_PX)
            .color(palette::color(palette::TITLE_TEXT))
            .into()
    } else {
        let mut grid = Column::new().spacing(16.0);
        let mut row = Row::new().spacing(16.0);
        for (i, w) in state.windows.iter().enumerate() {
            row = row.push(tile(w));
            if (i + 1) % COLS == 0 {
                grid = grid.push(row);
                row = Row::new().spacing(16.0);
            }
        }
        grid = grid.push(row);
        scrollable(container(grid).center_x(Length::Shrink))
            .style(mde_ui::scrollbar)
            .into()
    };

    // Backdrop catches clicks/Esc to close; the grid floats centered.
    iced::widget::stack![
        mouse_area(Space::new(Length::Fill, Length::Fill)).on_press(Message::Close),
        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .padding(40.0),
    ]
    .into()
}

/// One window tile: a card with the app icon over its title; a 2px accent border
/// when focused. Click focuses the window.
fn tile(w: &wlr::Window) -> Element<'_, Message> {
    let border_c = if w.focused {
        palette::accent()
    } else {
        palette::color(palette::WINDOW_FRAME)
    };
    let inner = Column::new()
        .spacing(8.0)
        .align_x(Horizontal::Center)
        .push(Space::with_height(Length::Fill))
        .push(crate::icons::icon_any(
            &[w.app_id.as_str(), "application-x-executable"],
            48,
        ))
        .push(Space::with_height(Length::Fill))
        .push(
            text(truncate(&w.title, 22))
                .size(metrics::UI_PX)
                .align_x(Horizontal::Center)
                .color(palette::color(palette::TITLE_TEXT)),
        );
    mouse_area(
        container(inner)
            .width(Length::Fixed(metrics::TASKVIEW_TILE))
            .height(Length::Fixed(metrics::TASKVIEW_TILE * 0.7))
            .align_x(Horizontal::Center)
            .align_y(Vertical::Center)
            .padding(10.0)
            .style(move |_| container::Style {
                background: Some(Background::Color(palette::color(palette::MENU))),
                border: Border {
                    color: border_c,
                    width: 2.0,
                    radius: 2.0.into(),
                },
                shadow: Shadow {
                    color: Color {
                        a: 0.4,
                        ..Color::BLACK
                    },
                    offset: iced::Vector::new(0.0, 2.0),
                    blur_radius: 10.0,
                },
                ..container::Style::default()
            }),
    )
    .on_press(Message::Focus(w.id))
    .into()
}

/// Trim a title to `max` chars with an ellipsis (windows have long titles).
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('\u{2026}');
        t
    }
}
