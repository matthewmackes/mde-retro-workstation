//! The Windows 10 Action Center pane (E3) — a right-anchored, full-height
//! layer-shell surface that shows the notification history grouped by app, read
//! from the `notifyd` mirror (`~/.config/mde/notifications.json`). Clearing a
//! card (or a whole group) calls the standard `CloseNotification` on the daemon.
//!
//!   mde action-center   open the slide-in pane (Win10 era; WINKEY+A)
//!
//! The quick-action tile grid (E3.5/6) and inline notification actions (which need
//! a daemon action bridge) layer on in later stories.

use std::process::{exit, ExitCode};
use std::time::SystemTime;

use iced::alignment::{Horizontal, Vertical};
use iced::widget::{container, mouse_area, row, scrollable, text, Column, Row, Space};
use iced::{
    event, keyboard, Background, Border, Color, Element, Event, Length, Padding, Shadow, Task,
};
use iced_layershell::build_pattern::{application, MainSettings};
use iced_layershell::reexport::{Anchor, KeyboardInteractivity};
use iced_layershell::settings::LayerShellSettings;
use iced_layershell::{to_layer_message, Appearance};

use mde_ui::{metrics, palette};

use crate::notifyd::{self, Notif};

const PANE_W: f32 = 360.0;
const TILE: f32 = 78.0; // square quick-action tile
const COLLAPSED: usize = 4; // first four tiles show collapsed

struct Center {
    notes: Vec<Notif>,
    /// Quick-action tiles in order: (id, currently-on).
    tiles: Vec<(String, bool)>,
    /// Whether the quick-action grid is expanded past the first four.
    expanded: bool,
}

#[to_layer_message]
#[derive(Debug, Clone)]
enum Message {
    Clear(u32),         // dismiss one notification
    ClearGroup(String), // dismiss every notification from one app
    ToggleTile(String), // flip a quick-action tile + its backend
    ToggleExpand,       // show/hide the tiles past the first four
    Close,
    Event(Event),
}

pub fn run_center(_args: &[String]) -> ExitCode {
    match launch() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mde action-center: {e}");
            ExitCode::FAILURE
        }
    }
}

fn launch() -> Result<(), iced_layershell::Error> {
    application(namespace, update, view)
        .style(style)
        .subscription(|_: &Center| {
            event::listen_with(|event, _status, _window| match event {
                Event::Keyboard(_) => Some(Message::Event(event)),
                _ => None,
            })
        })
        .font(mde_ui::font::REGULAR_BYTES)
        .font(mde_ui::font::BOLD_BYTES)
        .font(mde_ui::font::PLEX_REGULAR_BYTES)
        .font(mde_ui::font::PLEX_BOLD_BYTES)
        .default_font(mde_ui::font::ui())
        .settings(MainSettings {
            layer_settings: LayerShellSettings {
                // Full-screen transparent catcher; the pane hugs the right edge.
                anchor: Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right,
                exclusive_zone: 0,
                keyboard_interactivity: KeyboardInteractivity::Exclusive,
                ..Default::default()
            },
            ..Default::default()
        })
        .run_with(|| {
            let tiles = crate::state::load()
                .quick_actions
                .into_iter()
                .filter(|id| tile_label(id).is_some()) // only ids we implement
                .map(|id| {
                    let on = read_tile(&id);
                    (id, on)
                })
                .collect();
            (
                Center {
                    notes: notifyd::load_file().notifications,
                    tiles,
                    expanded: false,
                },
                Task::none(),
            )
        })
}

fn namespace(_: &Center) -> String {
    "mde-action-center".to_string()
}

fn style(_: &Center, _: &iced::Theme) -> Appearance {
    Appearance {
        background_color: Color::TRANSPARENT,
        text_color: palette::color(palette::WINDOW_TEXT),
    }
}

fn update(state: &mut Center, message: Message) -> Task<Message> {
    match message {
        Message::Clear(id) => {
            dbus_close(id);
            state.notes.retain(|n| n.id != id);
        }
        Message::ClearGroup(app) => {
            for n in state.notes.iter().filter(|n| n.app_name == app) {
                dbus_close(n.id);
            }
            state.notes.retain(|n| n.app_name != app);
        }
        Message::ToggleTile(id) => {
            if let Some((_, on)) = state.tiles.iter_mut().find(|(t, _)| *t == id) {
                toggle_tile(&id, *on);
                // Re-read the backend so the fill matches reality (some toggles
                // settle async, but the read is best-effort + cheap).
                *on = read_tile(&id);
            }
        }
        Message::ToggleExpand => state.expanded = !state.expanded,
        Message::Close => exit(0),
        Message::Event(Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::Escape),
            ..
        })) => exit(0),
        _ => {}
    }
    Task::none()
}

/// Run a shell command and capture its stdout (for reading a tile's state).
fn sh(cmd: &str) -> String {
    std::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default()
}

/// Spawn a shell command detached (for a tile's toggle action).
fn run(cmd: &str) {
    let _ = std::process::Command::new("sh").arg("-c").arg(cmd).spawn();
}

/// The display label + Nerd glyph for a quick-action tile id, or None if the id
/// isn't one we implement (so unknown ids in `quick_actions` are skipped).
fn tile_label(id: &str) -> Option<(&'static str, &'static str)> {
    Some(match id {
        "wifi" => ("Wi-Fi", "\u{f1eb}"),             // fa-wifi
        "bluetooth" => ("Bluetooth", "\u{f293}"),    // fa-bluetooth
        "airplane" => ("Airplane", "\u{f072}"),      // fa-plane
        "mute" => ("Mute", "\u{f6a9}"),              // fa-volume-mute
        "nightlight" => ("Night light", "\u{f186}"), // fa-moon
        _ => return None,
    })
}

/// Read a quick-action tile's on/off state from its Linux backend.
fn read_tile(id: &str) -> bool {
    match id {
        "wifi" => sh("nmcli -t radio wifi").trim() == "enabled",
        "bluetooth" => !sh("rfkill list bluetooth").contains("Soft blocked: yes"),
        // Airplane = every radio soft-blocked (no device left unblocked).
        "airplane" => {
            let l = sh("rfkill list");
            !l.contains("Soft blocked: no") && l.contains("Soft blocked")
        }
        "mute" => sh("wpctl get-volume @DEFAULT_AUDIO_SINK@").contains("MUTED"),
        "nightlight" => !sh("pgrep -x wlsunset").trim().is_empty(),
        _ => false,
    }
}

/// Toggle a quick-action tile's backend given its current state.
fn toggle_tile(id: &str, on: bool) {
    match id {
        "wifi" => run(if on {
            "nmcli radio wifi off"
        } else {
            "nmcli radio wifi on"
        }),
        "bluetooth" => run(if on {
            "rfkill block bluetooth"
        } else {
            "rfkill unblock bluetooth"
        }),
        "airplane" => run(if on {
            "rfkill unblock all"
        } else {
            "rfkill block all"
        }),
        "mute" => run("wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle"),
        "nightlight" => run(if on { "pkill -x wlsunset" } else { "wlsunset" }),
        _ => {}
    }
}

/// Dismiss a notification via the standard freedesktop `CloseNotification` on
/// whatever daemon owns the name (our `notifyd` in the Win10 era).
fn dbus_close(id: u32) {
    if let Ok(conn) = zbus::blocking::Connection::session() {
        if let Ok(proxy) = zbus::blocking::Proxy::new(
            &conn,
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            "org.freedesktop.Notifications",
        ) {
            let _ = proxy.call::<_, _, ()>("CloseNotification", &(id,));
        }
    }
}

// --- view --------------------------------------------------------------------

fn pad(t: f32, r: f32, b: f32, l: f32) -> Padding {
    Padding {
        top: t,
        right: r,
        bottom: b,
        left: l,
    }
}

fn clear_x(msg: Message) -> Element<'static, Message> {
    mouse_area(
        container(
            text("\u{f00d}") // fa-times (×)
                .size(metrics::UI_PX)
                .font(mde_ui::font::NERD)
                .color(palette::color(palette::GRAY_TEXT)),
        )
        .padding(pad(2.0, 6.0, 2.0, 6.0)),
    )
    .on_press(msg)
    .into()
}

fn view(state: &Center) -> Element<'_, Message> {
    let body: Element<Message> = if state.notes.is_empty() {
        container(
            text("No new notifications")
                .size(metrics::UI_PX)
                .color(palette::color(palette::GRAY_TEXT)),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
    } else {
        let mut col = Column::new().spacing(8.0).width(Length::Fill);
        // Group by app, preserving first-seen order; newest cards first within.
        let mut groups: Vec<(String, Vec<&Notif>)> = Vec::new();
        for n in state.notes.iter().rev() {
            match groups.iter_mut().find(|(a, _)| *a == n.app_name) {
                Some((_, v)) => v.push(n),
                None => groups.push((n.app_name.clone(), vec![n])),
            }
        }
        for (app, notes) in groups {
            // Group header: app icon + name + a group Clear "x".
            let label: String = if app.is_empty() {
                "Notifications".into()
            } else {
                app.clone()
            };
            col = col.push(
                Row::new()
                    .align_y(Vertical::Center)
                    .push(crate::icons::icon_any(
                        &[app.to_lowercase().as_str(), "dialog-information"],
                        16,
                    ))
                    .push(Space::with_width(Length::Fixed(6.0)))
                    .push(
                        text(label)
                            .size(metrics::UI_PX)
                            .font(mde_ui::font::ui_bold())
                            .width(Length::Fill),
                    )
                    .push(clear_x(Message::ClearGroup(app.clone()))),
            );
            for n in notes {
                col = col.push(card(n));
            }
        }
        scrollable(col).style(mde_ui::scrollbar).into()
    };

    // The flat dark pane: history (fills) over the quick-action tile grid.
    let pane = container(
        Column::new()
            .padding(10.0)
            .spacing(10.0)
            .push(container(body).height(Length::Fill))
            .push(tile_grid(state)),
    )
    .width(Length::Fixed(PANE_W))
    .height(Length::Fill)
    .style(|_| container::Style {
        background: Some(Background::Color(palette::color(palette::MENU))),
        border: Border {
            color: palette::color(palette::WINDOW_FRAME),
            width: 1.0,
            radius: 0.0.into(),
        },
        shadow: Shadow {
            color: Color {
                a: 0.35,
                ..Color::BLACK
            },
            offset: iced::Vector::new(-2.0, 0.0),
            blur_radius: 12.0,
        },
        ..container::Style::default()
    });

    iced::widget::stack![
        mouse_area(Space::new(Length::Fill, Length::Fill)).on_press(Message::Close),
        container(pane)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Horizontal::Right)
            .align_y(Vertical::Top),
    ]
    .into()
}

/// One notification card: icon + summary (bold) + body + relative time, with a
/// per-card Clear "x".
fn card(n: &Notif) -> Element<'static, Message> {
    let head = Row::new()
        .align_y(Vertical::Center)
        .push(
            text(n.summary.clone())
                .size(metrics::UI_PX)
                .font(mde_ui::font::ui_bold())
                .width(Length::Fill),
        )
        .push(
            text(rel_time(n.timestamp))
                .size(metrics::UI_PX)
                .color(palette::color(palette::GRAY_TEXT)),
        )
        .push(clear_x(Message::Clear(n.id)));
    let mut inner = Column::new().spacing(2.0).width(Length::Fill).push(head);
    if !n.body.is_empty() {
        inner = inner.push(
            text(n.body.clone())
                .size(metrics::UI_PX)
                .color(palette::color(palette::WINDOW_TEXT)),
        );
    }
    let icon = crate::icons::icon_any(&[n.app_icon.as_str(), "dialog-information"], 24);
    container(row![icon, inner].spacing(8.0).align_y(Vertical::Top))
        .width(Length::Fill)
        .padding(8.0)
        .style(|_| container::Style {
            background: Some(Background::Color(palette::color(palette::WINDOW))),
            border: Border {
                color: palette::color(palette::WINDOW_FRAME),
                width: 1.0,
                radius: 2.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

/// A coarse "Nm ago" / "Nh ago" / "now" relative timestamp.
fn rel_time(t: SystemTime) -> String {
    match SystemTime::now().duration_since(t) {
        Ok(d) => {
            let s = d.as_secs();
            if s < 60 {
                "now".to_string()
            } else if s < 3600 {
                format!("{}m ago", s / 60)
            } else if s < 86_400 {
                format!("{}h ago", s / 3600)
            } else {
                format!("{}d ago", s / 86_400)
            }
        }
        Err(_) => "now".to_string(),
    }
}

/// The quick-action tile grid: the first four tiles, with an Expand link to the
/// rest (Collapse when expanded). Rows of four square toggle tiles.
fn tile_grid(state: &Center) -> Element<'_, Message> {
    if state.tiles.is_empty() {
        return Space::new(Length::Shrink, Length::Shrink).into();
    }
    let shown = if state.expanded {
        state.tiles.len()
    } else {
        state.tiles.len().min(COLLAPSED)
    };
    let mut grid = Column::new().spacing(6.0).width(Length::Fill);
    let mut r = Row::new().spacing(6.0);
    for (i, (id, on)) in state.tiles.iter().take(shown).enumerate() {
        r = r.push(quick_tile(id, *on));
        if (i + 1) % 4 == 0 {
            grid = grid.push(r);
            r = Row::new().spacing(6.0);
        }
    }
    grid = grid.push(r);
    if state.tiles.len() > COLLAPSED {
        let label = if state.expanded { "Collapse" } else { "Expand" };
        grid = grid.push(
            mouse_area(text(label).size(metrics::UI_PX).color(palette::accent()))
                .on_press(Message::ToggleExpand),
        );
    }
    grid.into()
}

/// One square quick-action tile: a Nerd glyph + label, accent-filled when on.
fn quick_tile(id: &str, on: bool) -> Element<'static, Message> {
    let (label, glyph) = tile_label(id).unwrap_or(("", ""));
    let fg = if on {
        palette::color(palette::HIGHLIGHT_TEXT)
    } else {
        palette::color(palette::WINDOW_TEXT)
    };
    let content = Column::new()
        .spacing(4.0)
        .align_x(Horizontal::Center)
        .width(Length::Fill)
        .push(text(glyph).size(20.0).font(mde_ui::font::NERD).color(fg))
        .push(
            text(label)
                .size(metrics::UI_PX)
                .color(fg)
                .align_x(Horizontal::Center),
        );
    mouse_area(
        container(content)
            .width(Length::Fixed(TILE))
            .height(Length::Fixed(TILE))
            .align_x(Horizontal::Center)
            .align_y(Vertical::Center)
            .style(move |_| container::Style {
                background: Some(Background::Color(if on {
                    palette::color(palette::HIGHLIGHT)
                } else {
                    palette::color(palette::WINDOW)
                })),
                border: Border {
                    color: palette::color(palette::WINDOW_FRAME),
                    width: 1.0,
                    radius: 2.0.into(),
                },
                ..container::Style::default()
            }),
    )
    .on_press(Message::ToggleTile(id.to_string()))
    .into()
}
