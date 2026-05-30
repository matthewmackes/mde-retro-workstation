//! Small Win2000-style dialogs: Log Off (confirm) and Shut Down (dropdown).
//!
//! Each runs as its own subcommand/process (`mde logoff`, `mde shutdown`) in a
//! small fixed window; sway draws the navy title bar. Buttons use the mde-ui
//! 3D push button; actions go through swaymsg / systemctl.

use std::fmt;
use std::process::{exit, Command, ExitCode};

use iced::widget::{container, pick_list, text, Column, Row, Space};
use iced::{Background, Element, Length, Padding, Task};

use mde_ui::{button, palette};

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
}

// ---------------- Log Off ----------------

#[derive(Default)]
struct LogOff;

pub fn logoff() -> ExitCode {
    let r = iced::application(|_: &LogOff| "Log Off Windows".to_string(), logoff_update, logoff_view)
        .window_size(iced::Size::new(320.0, 140.0))
        .resizable(false)
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
        _ => Task::none(),
    }
}

fn logoff_view(_: &LogOff) -> Element<'_, M> {
    let buttons = Row::new()
        .spacing(8.0)
        .push(Space::with_width(Length::Fill))
        .push(button(text("Yes").size(11.0)).on_press(M::Confirm).width(Length::Fixed(76.0)))
        .push(button(text("No").size(11.0)).on_press(M::Cancel).width(Length::Fixed(76.0)));

    let body = Column::new()
        .spacing(16.0)
        .push(text("Are you sure you want to log off?").size(11.0))
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
    .font(mde_ui::font::REGULAR_BYTES)
    .font(mde_ui::font::BOLD_BYTES)
    .default_font(mde_ui::font::UI)
    .run_with(|| (Shutdown { sel: Choice::ShutDown }, Task::none()));
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}

fn shutdown_update(state: &mut Shutdown, m: M) -> Task<M> {
    match m {
        M::Pick(c) => state.sel = c,
        M::Cancel => exit(0),
        M::Confirm => {
            let mut cmd = match state.sel {
                Choice::LogOff => Command::new("swaymsg"),
                _ => Command::new("systemctl"),
            };
            match state.sel {
                Choice::LogOff => {
                    cmd.arg("exit");
                }
                Choice::ShutDown => {
                    cmd.arg("poweroff");
                }
                Choice::Restart => {
                    cmd.arg("reboot");
                }
                Choice::StandBy => {
                    cmd.arg("suspend");
                }
            }
            let _ = cmd.spawn();
            exit(0);
        }
    }
    Task::none()
}

fn shutdown_view(state: &Shutdown) -> Element<'_, M> {
    let choices = vec![Choice::LogOff, Choice::ShutDown, Choice::Restart, Choice::StandBy];
    let drop = pick_list(choices, Some(state.sel.clone()), M::Pick).text_size(11.0);

    let buttons = Row::new()
        .spacing(8.0)
        .push(Space::with_width(Length::Fill))
        .push(button(text("OK").size(11.0)).on_press(M::Confirm).width(Length::Fixed(76.0)))
        .push(button(text("Cancel").size(11.0)).on_press(M::Cancel).width(Length::Fixed(76.0)));

    let body = Column::new()
        .spacing(14.0)
        .push(text("What do you want the computer to do?").size(11.0))
        .push(drop)
        .push(buttons);

    silver(body)
}
