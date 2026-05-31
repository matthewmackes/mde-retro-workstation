//! Taskbar — a wlr-layer-shell bar anchored to the bottom edge.
//!
//! A raised Win2000 panel: ⊞ Start button, a window-button taskbar fed by sway
//! IPC (the focused window's button shows pressed), a flexible spacer, and a
//! sunken clock well. Polls sway + the clock once a second.

use std::process::{Command, ExitCode};
use std::time::Duration;

use iced::widget::{container, text, Row, Space, Stack};
use iced::{Element, Length, Padding, Task};
use iced_layershell::build_pattern::{application, MainSettings};
use iced_layershell::reexport::Anchor;
use iced_layershell::settings::LayerShellSettings;
use iced_layershell::{to_layer_message, Appearance};

use mde_ui::{button, frame, metrics, palette};

use crate::sway;

#[derive(Default)]
struct Panel {
    windows: Vec<sway::Window>,
    clock: String,
    /// Quick Launch pins, loaded from ~/.config/mde/menu.json at startup.
    pinned: Vec<crate::state::PinnedItem>,
}

#[to_layer_message]
#[derive(Debug, Clone)]
enum Message {
    Tick,
    Start,
    Focus(i64),
    Launch(String),
}

pub fn run(_args: &[String]) -> ExitCode {
    match launch() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mde panel: {e}");
            ExitCode::FAILURE
        }
    }
}

fn launch() -> Result<(), iced_layershell::Error> {
    application(namespace, update, view)
        .style(style)
        .subscription(subscription)
        .font(mde_ui::font::REGULAR_BYTES)
        .font(mde_ui::font::BOLD_BYTES)
        .default_font(mde_ui::font::UI)
        .settings(MainSettings {
            layer_settings: LayerShellSettings {
                size: Some((0, metrics::TASKBAR_HEIGHT as u32)),
                exclusive_zone: metrics::TASKBAR_HEIGHT as i32,
                anchor: Anchor::Bottom | Anchor::Left | Anchor::Right,
                ..Default::default()
            },
            ..Default::default()
        })
        .run_with(|| {
            let panel = Panel { pinned: crate::state::load().pinned, ..Panel::default() };
            (panel, Task::done(Message::Tick))
        })
}

fn namespace(_state: &Panel) -> String {
    "mde-panel".to_string()
}

fn style(_state: &Panel, _theme: &iced::Theme) -> Appearance {
    Appearance {
        background_color: palette::color(palette::BUTTON_FACE),
        text_color: palette::color(palette::WINDOW_TEXT),
    }
}

fn subscription(_state: &Panel) -> iced::Subscription<Message> {
    iced::time::every(Duration::from_secs(1)).map(|_| Message::Tick)
}

fn update(state: &mut Panel, message: Message) -> Task<Message> {
    match message {
        Message::Tick => {
            state.windows = sway::windows().unwrap_or_default();
            state.clock = clock_now();
        }
        Message::Start => spawn_self("menu"),
        Message::Focus(id) => {
            let _ = sway::focus(id);
        }
        Message::Launch(cmd) => {
            let _ = Command::new("sh").arg("-c").arg(&cmd).spawn();
        }
        _ => {}
    }
    Task::none()
}

fn view(state: &Panel) -> Element<'_, Message> {
    let mut bar = Row::new()
        .spacing(2.0)
        .height(Length::Fill)
        .push(
            button(text("\u{2756} Start").size(metrics::UI_PX))
                .on_press(Message::Start)
                .height(Length::Fill),
        )
        .push(Space::with_width(Length::Fixed(6.0)));

    // Quick Launch: pinned apps (from menu.json), between Start and the windows.
    if !state.pinned.is_empty() {
        for item in &state.pinned {
            bar = bar.push(
                button(text(truncate(&item.name, 12)).size(metrics::UI_PX))
                    .on_press(Message::Launch(item.command.clone()))
                    .height(Length::Fill),
            );
        }
        bar = bar.push(Space::with_width(Length::Fixed(6.0)));
    }

    for w in &state.windows {
        bar = bar.push(
            button(text(truncate(&w.title, 22)).size(metrics::UI_PX))
                .on_press(Message::Focus(w.id))
                .active(w.focused)
                .width(Length::Fixed(metrics::TASKBAR_BUTTON_MIN as f32))
                .height(Length::Fill),
        );
    }

    bar = bar.push(Space::with_width(Length::Fill));

    let clock = Stack::new().push(frame::sunken()).push(
        container(text(state.clock.clone()).size(metrics::UI_PX))
            .center_y(Length::Fill)
            .padding(Padding {
                top: 0.0,
                right: 8.0,
                bottom: 0.0,
                left: 8.0,
            }),
    );
    bar = bar.push(
        container(clock)
            .width(Length::Fixed(92.0))
            .height(Length::Fill)
            .padding(2.0),
    );

    Stack::new()
        .push(frame::raised())
        .push(
            container(bar)
                .padding(2.0)
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .into()
}

fn clock_now() -> String {
    Command::new("date")
        .arg("+%-l:%M %p")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn spawn_self(sub: &str) {
    if let Ok(exe) = std::env::current_exe() {
        let _ = Command::new(exe).arg(sub).spawn();
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        let head: String = s.chars().take(n.saturating_sub(1)).collect();
        format!("{head}\u{2026}")
    } else {
        s.to_string()
    }
}
