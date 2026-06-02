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
use iced::{
    event, keyboard, Background, Border, Color, Element, Event, Length, Padding, Shadow, Task,
};
use iced_layershell::build_pattern::{application, MainSettings};
use iced_layershell::reexport::{Anchor, KeyboardInteractivity};
use iced_layershell::settings::LayerShellSettings;
use iced_layershell::{to_layer_message, Appearance};

use mde_ui::{metrics, palette};

use crate::wlr;
use crate::workspace;

const COLS: usize = 4;

struct TaskView {
    wm: Option<wlr::Wm>,
    windows: Vec<wlr::Window>,
    ws: Option<workspace::Workspaces>,
    workspaces: Vec<workspace::Workspace>,
    /// Fallback desktop count (E4.5): >0 only when ext-workspace is absent and
    /// `state.virtual_desktops` > 1, so the band shows a fixed strip instead.
    fixed_desktops: u32,
}

#[to_layer_message]
#[derive(Debug, Clone)]
enum Message {
    Tick,            // re-read the window + workspace snapshots (they fill in async)
    Focus(u64),      // raise/focus the clicked window, then close
    ActivateWs(u64), // switch to a virtual desktop, then close
    NewWs,           // create a new virtual desktop (stay open)
    RemoveWs(u64),   // remove a virtual desktop (stay open)
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
            let ws = workspace::start();
            let workspaces = ws.as_ref().map(|w| w.list()).unwrap_or_default();
            // Fallback ladder: with no ext-workspace, fall back to the configured
            // fixed desktop count; a single desktop means no band at all.
            let fixed_desktops = if ws.is_none() {
                crate::state::load().virtual_desktops
            } else {
                0
            };
            (
                TaskView {
                    wm,
                    windows,
                    ws,
                    workspaces,
                    fixed_desktops,
                },
                Task::none(),
            )
        })
}

fn update(state: &mut TaskView, message: Message) -> Task<Message> {
    match message {
        Message::Tick => {
            if let Some(wm) = &state.wm {
                state.windows = wm.windows();
            }
            if let Some(ws) = &state.ws {
                state.workspaces = ws.list();
            }
        }
        Message::Focus(id) => {
            if let Some(wm) = &state.wm {
                wm.focus(id);
            }
            exit(0)
        }
        Message::ActivateWs(id) => {
            if let Some(ws) = &state.ws {
                ws.activate(id);
            }
            exit(0)
        }
        Message::NewWs => {
            if let Some(ws) = &state.ws {
                ws.create(&format!("Desktop {}", state.workspaces.len() + 1));
            }
        }
        Message::RemoveWs(id) => {
            if let Some(ws) = &state.ws {
                ws.remove(id);
            }
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

    // Backdrop catches clicks/Esc to close; the virtual-desktop band sits along
    // the top (when the compositor advertises ext-workspace), the window grid
    // floats centered below it.
    let body = Column::new()
        .width(Length::Fill)
        .height(Length::Fill)
        .push(band(state))
        .push(
            container(content)
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .padding(40.0),
        );
    iced::widget::stack![
        mouse_area(Space::new(Length::Fill, Length::Fill)).on_press(Message::Close),
        body,
    ]
    .into()
}

/// Pick the desktop band per the fallback ladder (E4.4/E4.5):
///   1. live ext-workspace present → the interactive `desktop_band`;
///   2. absent but `virtual_desktops` > 1 → the read-only `fixed_desktop_band`;
///   3. neither → no band (single-desktop grid).
fn band(state: &TaskView) -> Element<'_, Message> {
    if !state.workspaces.is_empty() {
        desktop_band(&state.workspaces)
    } else if state.fixed_desktops > 1 {
        fixed_desktop_band(state.fixed_desktops)
    } else {
        Space::new(Length::Fill, Length::Shrink).into()
    }
}

/// The fallback strip (E4.5): the compositor doesn't advertise ext-workspace, so
/// mde can't read which desktop is active or switch via the protocol. Show the
/// configured desktop count as read-only chips with the keyboard hint — the real
/// switching is the labwc `W-C-Left/Right` binds (E4.6). No fake active state and
/// no dead click: the chips are plain labels, not buttons.
fn fixed_desktop_band<'a>(n: u32) -> Element<'a, Message> {
    let mut row = Row::new().spacing(10.0).align_y(Vertical::Center);
    for i in 1..=n {
        let label = text(format!("Desktop {i}"))
            .size(metrics::UI_PX)
            .color(palette::color(palette::TITLE_TEXT));
        row = row.push(
            container(label)
                .padding(Padding::from([6.0, 12.0]))
                .style(|_| container::Style {
                    background: Some(Background::Color(palette::color(palette::MENU))),
                    border: Border {
                        color: palette::color(palette::WINDOW_FRAME),
                        width: 1.0,
                        radius: 2.0.into(),
                    },
                    ..container::Style::default()
                }),
        );
    }
    // Plain ASCII: the bundled UI font lacks the arrow glyphs, and §2.7 says
    // never render tofu.
    let hint = text("Ctrl+Win+Left / Right to switch")
        .size(metrics::UI_PX)
        .color(palette::color(palette::GRAY_TEXT));
    let col = Column::new()
        .spacing(6.0)
        .align_x(Horizontal::Center)
        .push(row)
        .push(hint);
    container(col)
        .width(Length::Fill)
        .align_x(Horizontal::Center)
        .padding(16.0)
        .into()
}

/// The virtual-desktop band: a centered row of workspace chips (the active one
/// accent-filled, each with a remove ×) plus a trailing "+ New desktop" chip.
fn desktop_band(workspaces: &[workspace::Workspace]) -> Element<'_, Message> {
    if workspaces.is_empty() {
        return Space::new(Length::Fill, Length::Shrink).into();
    }
    let mut row = Row::new().spacing(10.0).align_y(Vertical::Center);
    for w in workspaces {
        row = row.push(ws_chip(w));
    }
    row = row.push(new_ws_chip());
    container(row)
        .width(Length::Fill)
        .align_x(Horizontal::Center)
        .padding(16.0)
        .into()
}

/// One workspace chip: name + a remove ×; accent-filled when it's the active
/// desktop. Clicking the chip switches to it; clicking the × removes it.
fn ws_chip(w: &workspace::Workspace) -> Element<'_, Message> {
    let (bg, fg) = if w.active {
        (palette::accent(), palette::color(palette::HIGHLIGHT_TEXT))
    } else {
        (
            palette::color(palette::MENU),
            palette::color(palette::TITLE_TEXT),
        )
    };
    let label = text(w.name.clone()).size(metrics::UI_PX).color(fg);
    let close = mouse_area(text("\u{2715}").size(metrics::UI_PX).color(fg))
        .on_press(Message::RemoveWs(w.id));
    let inner = Row::new()
        .spacing(8.0)
        .align_y(Vertical::Center)
        .push(label)
        .push(close);
    mouse_area(
        container(inner)
            .padding(Padding::from([6.0, 12.0]))
            .style(move |_| container::Style {
                background: Some(Background::Color(bg)),
                border: Border {
                    color: palette::color(palette::WINDOW_FRAME),
                    width: 1.0,
                    radius: 2.0.into(),
                },
                ..container::Style::default()
            }),
    )
    .on_press(Message::ActivateWs(w.id))
    .into()
}

/// The "+ New desktop" chip at the end of the band.
fn new_ws_chip<'a>() -> Element<'a, Message> {
    let label = text("+ New desktop")
        .size(metrics::UI_PX)
        .color(palette::color(palette::TITLE_TEXT));
    mouse_area(
        container(label)
            .padding(Padding::from([6.0, 12.0]))
            .style(|_| container::Style {
                background: Some(Background::Color(palette::color(palette::MENU))),
                border: Border {
                    color: palette::color(palette::WINDOW_FRAME),
                    width: 1.0,
                    radius: 2.0.into(),
                },
                ..container::Style::default()
            }),
    )
    .on_press(Message::NewWs)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn overlay(fixed: u32, windows: Vec<wlr::Window>) -> TaskView {
        TaskView {
            wm: None,
            windows,
            ws: None,
            workspaces: Vec::new(),
            fixed_desktops: fixed,
        }
    }

    #[test]
    fn overlay_builds_with_fixed_fallback() {
        // E4.5 acceptance: with no ext-workspace and a configured count, the
        // overlay still builds a valid Element (the fixed strip path).
        let st = overlay(4, Vec::new());
        let _el: Element<Message> = view(&st);
    }

    #[test]
    fn overlay_builds_single_desktop() {
        // One desktop → no band, still a valid Element (the terminal rung).
        let st = overlay(1, Vec::new());
        let _el: Element<Message> = view(&st);
    }

    #[test]
    fn overlay_builds_with_a_window() {
        // A window present exercises the tile grid path too.
        let st = overlay(
            0,
            vec![wlr::Window {
                id: 1,
                title: "foot".into(),
                app_id: "foot".into(),
                focused: true,
                minimized: false,
                maximized: false,
            }],
        );
        let _el: Element<Message> = view(&st);
    }
}
