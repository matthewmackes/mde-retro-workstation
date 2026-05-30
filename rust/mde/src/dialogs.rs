//! Small Win2000-style dialogs: Log Off (confirm) and Shut Down (dropdown).
//!
//! Each runs as its own subcommand/process (`mde logoff`, `mde shutdown`) in a
//! small fixed window; sway draws the navy title bar. Buttons use the mde-ui
//! 3D push button; actions go through swaymsg / systemctl.

use std::fmt;
use std::process::{exit, Command, ExitCode};

use iced::widget::{container, pick_list, text, Column, Row, Space};
use iced::{event, keyboard, Background, Element, Event, Length, Padding, Task};

use mde_ui::{button, metrics, palette};

fn pad(top: f32, right: f32, bottom: f32, left: f32) -> Padding {
    Padding { top, right, bottom, left }
}

fn silver<'a>(content: impl Into<Element<'a, M>>) -> Element<'a, M>
where
    M: 'a,
{
    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(pad(14.0, 12.0, 12.0, 14.0))
        .style(|_| container::Style {
            background: Some(Background::Color(palette::color(palette::MENU))),
            ..container::Style::default()
        })
        .into()
}

// A shared message type for both dialogs (variants used per dialog).
#[derive(Debug, Clone)]
enum M {
    Confirm,
    Cancel,
    Pick(Choice),
    Event(Event),
}

/// Map Enter -> default action, Esc -> Cancel.
fn key_subscription<S>(_: &S) -> iced::Subscription<M> {
    event::listen().map(M::Event)
}

fn is_enter(e: &Event) -> bool {
    matches!(
        e,
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::Enter),
            ..
        })
    )
}

fn is_escape(e: &Event) -> bool {
    matches!(
        e,
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::Escape),
            ..
        })
    )
}

// ---------------- Log Off ----------------

#[derive(Default)]
struct LogOff;

pub fn logoff() -> ExitCode {
    let r = iced::application(|_: &LogOff| "Log Off Windows".to_string(), logoff_update, logoff_view)
        .window_size(iced::Size::new(320.0, 140.0))
        .resizable(false)
        .subscription(key_subscription)
        .font(mde_ui::font::REGULAR_BYTES)
        .font(mde_ui::font::BOLD_BYTES)
        .default_font(mde_ui::font::UI)
        .run();
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}

fn logoff_update(_: &mut LogOff, m: M) -> Task<M> {
    match m {
        M::Confirm => {
            let _ = Command::new("swaymsg").arg("exit").spawn();
            exit(0);
        }
        M::Cancel => exit(0),
        M::Event(e) if is_enter(&e) => {
            let _ = Command::new("swaymsg").arg("exit").spawn();
            exit(0);
        }
        M::Event(e) if is_escape(&e) => exit(0),
        _ => Task::none(),
    }
}

fn logoff_view(_: &LogOff) -> Element<'_, M> {
    let buttons = Row::new()
        .spacing(8.0)
        .push(Space::with_width(Length::Fill))
        .push(
            button(text("Yes").size(metrics::UI_PX))
                .on_press(M::Confirm)
                .default(true)
                .width(Length::Fixed(76.0)),
        )
        .push(button(text("No").size(metrics::UI_PX)).on_press(M::Cancel).width(Length::Fixed(76.0)));

    let body = Column::new()
        .spacing(16.0)
        .push(text("Are you sure you want to log off?").size(metrics::UI_PX))
        .push(buttons);

    silver(body)
}

// ---------------- Shut Down ----------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum Choice {
    LogOff,
    ShutDown,
    Restart,
    StandBy,
}

impl fmt::Display for Choice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Choice::LogOff => "Log off",
            Choice::ShutDown => "Shut down",
            Choice::Restart => "Restart",
            Choice::StandBy => "Stand by",
        })
    }
}

struct Shutdown {
    sel: Choice,
}

pub fn shutdown() -> ExitCode {
    let r = iced::application(
        |_: &Shutdown| "Shut Down Windows".to_string(),
        shutdown_update,
        shutdown_view,
    )
    .window_size(iced::Size::new(340.0, 170.0))
    .resizable(false)
    .subscription(key_subscription)
    .font(mde_ui::font::REGULAR_BYTES)
    .font(mde_ui::font::BOLD_BYTES)
    .default_font(mde_ui::font::UI)
    .run_with(|| (Shutdown { sel: Choice::ShutDown }, Task::none()));
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}

fn do_shutdown(sel: &Choice) -> ! {
    let mut cmd = match sel {
        Choice::LogOff => Command::new("swaymsg"),
        _ => Command::new("systemctl"),
    };
    cmd.arg(match sel {
        Choice::LogOff => "exit",
        Choice::ShutDown => "poweroff",
        Choice::Restart => "reboot",
        Choice::StandBy => "suspend",
    });
    let _ = cmd.spawn();
    exit(0)
}

fn shutdown_update(state: &mut Shutdown, m: M) -> Task<M> {
    match m {
        M::Pick(c) => state.sel = c,
        M::Cancel => exit(0),
        M::Confirm => do_shutdown(&state.sel),
        M::Event(e) if is_enter(&e) => do_shutdown(&state.sel),
        M::Event(e) if is_escape(&e) => exit(0),
        M::Event(_) => {}
    }
    Task::none()
}

fn shutdown_view(state: &Shutdown) -> Element<'_, M> {
    let choices = vec![Choice::LogOff, Choice::ShutDown, Choice::Restart, Choice::StandBy];
    let drop = pick_list(choices, Some(state.sel.clone()), M::Pick).text_size(metrics::UI_PX);

    let buttons = Row::new()
        .spacing(8.0)
        .push(Space::with_width(Length::Fill))
        .push(
            button(text("OK").size(metrics::UI_PX))
                .on_press(M::Confirm)
                .default(true)
                .width(Length::Fixed(76.0)),
        )
        .push(button(text("Cancel").size(metrics::UI_PX)).on_press(M::Cancel).width(Length::Fixed(76.0)));

    let body = Column::new()
        .spacing(14.0)
        .push(text("What do you want the computer to do?").size(metrics::UI_PX))
        .push(drop)
        .push(buttons);

    silver(body)
}
