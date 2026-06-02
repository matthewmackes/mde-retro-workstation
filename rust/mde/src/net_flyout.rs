//! The Windows 10 network flyout (E15.2): a bottom-right layer-shell panel showing
//! the active connection, the available Wi-Fi networks, Wi-Fi / Airplane toggle
//! pills, and a "Network & Internet settings" link. Reads + sets via `crate::nm`.
//! Win10-era only — the panel network glyph routes here under Win10 (E15.3); other
//! eras open `nm-connection-editor` directly.
//!
//!   mde net-flyout   open the flyout (Win10 era)

use std::process::{exit, ExitCode};

use iced::alignment::{Horizontal, Vertical};
use iced::widget::{button, container, mouse_area, scrollable, text, Column, Row, Space};
use iced::{event, keyboard, Background, Border, Color, Element, Event, Length, Padding, Task};
use iced_layershell::build_pattern::{application, MainSettings};
use iced_layershell::reexport::{Anchor, KeyboardInteractivity};
use iced_layershell::settings::LayerShellSettings;
use iced_layershell::{to_layer_message, Appearance};

use mde_ui::{metrics, palette};

use crate::nm::{self, Conn, Wifi};

const W: f32 = 340.0;

struct Flyout {
    conns: Vec<Conn>,
    wifis: Vec<Wifi>,
    wifi_on: bool,
    airplane: bool,
}

#[to_layer_message]
#[derive(Debug, Clone)]
enum Message {
    ToggleWifi,
    ToggleAirplane,
    OpenSettings,
    Close,
    Event(Event),
}

pub fn run(_args: &[String]) -> ExitCode {
    // Win10-era only; other eras use nm-connection-editor via the panel glyph (E15.3).
    if !palette::is_windows10() {
        return ExitCode::SUCCESS;
    }
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return ExitCode::SUCCESS;
    }
    match launch() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mde net-flyout: {e}");
            ExitCode::FAILURE
        }
    }
}

fn launch() -> Result<(), iced_layershell::Error> {
    application(namespace, update, view)
        .style(style)
        .subscription(|_: &Flyout| {
            event::listen_with(|event, _s, _w| match event {
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
                anchor: Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right,
                exclusive_zone: 0,
                keyboard_interactivity: KeyboardInteractivity::Exclusive,
                ..Default::default()
            },
            ..Default::default()
        })
        .run_with(|| {
            (
                Flyout {
                    conns: nm::active_connections(),
                    wifis: nm::wifi_list(),
                    wifi_on: nm::wifi_enabled(),
                    airplane: nm::airplane_on(),
                },
                Task::none(),
            )
        })
}

fn namespace(_: &Flyout) -> String {
    "mde-net-flyout".to_string()
}

fn style(_: &Flyout, _: &iced::Theme) -> Appearance {
    Appearance {
        background_color: Color::TRANSPARENT,
        text_color: palette::color(palette::WINDOW_TEXT),
    }
}

fn update(state: &mut Flyout, message: Message) -> Task<Message> {
    match message {
        Message::ToggleWifi => {
            nm::radio_wifi(!state.wifi_on);
            state.wifi_on = nm::wifi_enabled();
            state.wifis = nm::wifi_list();
        }
        Message::ToggleAirplane => {
            nm::set_airplane(!state.airplane);
            state.airplane = nm::airplane_on();
            state.wifi_on = nm::wifi_enabled();
        }
        Message::OpenSettings => {
            let exe = std::env::current_exe().unwrap_or_else(|_| "mde".into());
            let _ = std::process::Command::new(exe)
                .args(["settings", "network"])
                .spawn();
            exit(0);
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

/// A toggle pill: accent-filled when on, flat bordered when off.
fn pill(label: &str, on: bool, msg: Message) -> Element<'static, Message> {
    let (bg, fg) = if on {
        (palette::accent(), palette::color(palette::HIGHLIGHT_TEXT))
    } else {
        (
            palette::color(palette::WINDOW),
            palette::color(palette::WINDOW_TEXT),
        )
    };
    button(text(label.to_string()).size(metrics::UI_PX).color(fg))
        .on_press(msg)
        .padding(pad(6.0, 12.0, 6.0, 12.0))
        .style(move |_, _| button::Style {
            background: Some(Background::Color(bg)),
            text_color: fg,
            border: Border {
                color: palette::color(palette::WINDOW_FRAME),
                width: 1.0,
                radius: 2.0.into(),
            },
            ..button::Style::default()
        })
        .into()
}

fn pad(t: f32, r: f32, b: f32, l: f32) -> Padding {
    Padding {
        top: t,
        right: r,
        bottom: b,
        left: l,
    }
}

fn view(state: &Flyout) -> Element<'_, Message> {
    let header = match state.conns.iter().find(|c| c.state == "activated") {
        Some(c) => text(format!("Connected — {}", c.name))
            .size(metrics::UI_PX)
            .color(palette::color(palette::WINDOW_TEXT)),
        None => text("Not connected")
            .size(metrics::UI_PX)
            .color(palette::color(palette::GRAY_TEXT)),
    };
    // Available Wi-Fi networks (display; the connect flow is E15.4).
    let mut list = Column::new().spacing(0.0);
    for w in &state.wifis {
        list = list.push(
            Row::new()
                .spacing(8.0)
                .push(
                    text(w.ssid.clone())
                        .size(metrics::UI_PX)
                        .width(Length::Fill)
                        .color(palette::color(palette::WINDOW_TEXT)),
                )
                .push(
                    text(if w.secured { "\u{f023}" } else { " " }) // nf lock
                        .size(metrics::PANEL_GLYPH_PX)
                        .font(mde_ui::font::NERD)
                        .color(palette::color(palette::GRAY_TEXT)),
                )
                .push(
                    text(format!("{}%", w.signal))
                        .size(metrics::UI_PX)
                        .color(palette::color(palette::GRAY_TEXT)),
                )
                .padding(pad(3.0, 6.0, 3.0, 6.0)),
        );
    }
    let pills = Row::new()
        .spacing(8.0)
        .push(pill("Wi-Fi", state.wifi_on, Message::ToggleWifi))
        .push(pill(
            "Airplane mode",
            state.airplane,
            Message::ToggleAirplane,
        ));
    let settings = mouse_area(
        text("Network & Internet settings")
            .size(metrics::UI_PX)
            .color(palette::accent()),
    )
    .on_press(Message::OpenSettings);

    let panel = container(
        Column::new()
            .spacing(10.0)
            .padding(12.0)
            .push(
                text("Network")
                    .size(metrics::INFO_TITLE_PX)
                    .color(palette::color(palette::WINDOW_TEXT)),
            )
            .push(header)
            .push(container(scrollable(list).style(mde_ui::scrollbar)).height(Length::Fixed(220.0)))
            .push(pills)
            .push(settings),
    )
    .width(Length::Fixed(W))
    .style(|_| container::Style {
        background: Some(Background::Color(palette::color(palette::MENU))),
        border: Border {
            color: palette::color(palette::WINDOW_FRAME),
            width: 1.0,
            radius: 2.0.into(),
        },
        ..container::Style::default()
    });

    // Backdrop click-catcher closes; the panel sits bottom-right above the tray.
    iced::widget::stack![
        mouse_area(Space::new(Length::Fill, Length::Fill)).on_press(Message::Close),
        container(panel)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Horizontal::Right)
            .align_y(Vertical::Bottom)
            // The taskbar's exclusive zone already clips this above the bar (like
            // popup.rs), so only a small lift off the bottom-right is needed.
            .padding(pad(0.0, 4.0, 4.0, 0.0)),
    ]
    .into()
}
