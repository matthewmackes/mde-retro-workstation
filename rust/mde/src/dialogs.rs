//! Small session dialogs: Log Off, Shut Down, and Lock (`mde logoff`,
//! `mde shutdown`, `mde lock`).
//!
//! Each runs as its own subcommand/process in a small fixed window; **labwc**
//! draws the title bar. Under the classic eras these are the Win2000 confirm /
//! dropdown; under the **Windows 10** theme `shutdown` renders a power flyout
//! (Sleep · Shut down · Restart) and `logoff` an account flyout (Lock · Sign
//! out), both one-click. `mde lock` is headless (`loginctl lock-session`).
//! Buttons use the mde-ui 3D push button (flat under Carbon/Win10); actions go
//! through labwc / systemctl / logind.

use std::fmt;
use std::process::{exit, Command, ExitCode};

use iced::widget::{container, pick_list, text, text_input, Column, Row, Space};
use iced::{event, keyboard, Background, Element, Event, Length, Padding, Task};

use mde_ui::{button, metrics, palette};

fn pad(top: f32, right: f32, bottom: f32, left: f32) -> Padding {
    Padding {
        top,
        right,
        bottom,
        left,
    }
}

/// The silver (COLOR_3DFACE) dialog body shared by every dialog here.
fn silver<'a, Msg: 'a>(content: impl Into<Element<'a, Msg>>) -> Element<'a, Msg> {
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

/// The Windows 10 power / account flyout body: flat full-width rows over the
/// charcoal `MENU` surface (the buttons flatten under the Win10 theme). Each row
/// carries a [`Choice`] that the dialog's update acts on immediately (E19.3/E19.4).
fn flyout<'a>(rows: Vec<Element<'a, M>>) -> Element<'a, M> {
    let mut col = Column::new().spacing(2.0);
    for r in rows {
        col = col.push(r);
    }
    container(col)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(8.0)
        .style(|_| container::Style {
            background: Some(Background::Color(palette::color(palette::MENU))),
            ..container::Style::default()
        })
        .into()
}

fn flyout_row<'a>(label: &'a str, choice: Choice) -> Element<'a, M> {
    button(text(label).size(metrics::UI_PX))
        .on_press(M::Pick(choice))
        .width(Length::Fill)
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
    let r = iced::application(
        |_: &LogOff| "Log Off Windows".to_string(),
        logoff_update,
        logoff_view,
    )
    .window_size(iced::Size::new(320.0, 140.0))
    .resizable(false)
    .subscription(key_subscription)
    .font(mde_ui::font::REGULAR_BYTES)
    .font(mde_ui::font::BOLD_BYTES)
    .font(mde_ui::font::PLEX_REGULAR_BYTES)
    .font(mde_ui::font::PLEX_BOLD_BYTES)
    .default_font(mde_ui::font::ui())
    .run();
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}

/// Run `labwc --exit`, reporting failure instead of pretending it worked.
fn do_logoff() -> ! {
    match Command::new("labwc").arg("--exit").status() {
        Ok(s) if s.success() => exit(0),
        Ok(s) => {
            eprintln!("mde logoff: 'labwc --exit' failed ({s})");
            exit(1);
        }
        Err(e) => {
            eprintln!("mde logoff: could not run labwc: {e}");
            exit(1);
        }
    }
}

fn logoff_update(_: &mut LogOff, m: M) -> Task<M> {
    match m {
        M::Confirm => do_logoff(),
        // The Win10 account flyout's Lock row (E19.4) acts immediately.
        M::Pick(c) => do_shutdown(&c),
        M::Cancel => exit(0),
        M::Event(e) if is_enter(&e) => do_logoff(),
        M::Event(e) if is_escape(&e) => exit(0),
        _ => Task::none(),
    }
}

fn logoff_view(_: &LogOff) -> Element<'_, M> {
    if palette::is_windows10() {
        // Win10 account flyout: Lock / Sign out rows (Lock -> logind, Sign out
        // -> the same labwc --exit as the classic "Yes").
        return flyout(vec![
            flyout_row("Lock", Choice::Lock),
            button(text("Sign out").size(metrics::UI_PX))
                .on_press(M::Confirm)
                .width(Length::Fill)
                .into(),
        ]);
    }
    let buttons = Row::new()
        .spacing(8.0)
        .push(Space::with_width(Length::Fill))
        .push(
            button(text("Yes").size(metrics::UI_PX))
                .on_press(M::Confirm)
                .default(true)
                .width(Length::Fixed(76.0)),
        )
        .push(
            button(text("No").size(metrics::UI_PX))
                .on_press(M::Cancel)
                .width(Length::Fixed(76.0)),
        );

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
    Lock,
}

impl fmt::Display for Choice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Choice::LogOff => "Log off",
            Choice::ShutDown => "Shut down",
            Choice::Restart => "Restart",
            Choice::StandBy => "Stand by",
            Choice::Lock => "Lock",
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
    .font(mde_ui::font::PLEX_REGULAR_BYTES)
    .font(mde_ui::font::PLEX_BOLD_BYTES)
    .default_font(mde_ui::font::ui())
    .run_with(|| {
        (
            Shutdown {
                sel: Choice::ShutDown,
            },
            Task::none(),
        )
    });
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}

fn do_shutdown(sel: &Choice) -> ! {
    let mut cmd = match sel {
        Choice::LogOff => Command::new("labwc"),
        Choice::Lock => Command::new("loginctl"),
        _ => Command::new("systemctl"),
    };
    let verb = match sel {
        Choice::LogOff => "--exit",
        Choice::Lock => "lock-session",
        Choice::ShutDown => "poweroff",
        Choice::Restart => "reboot",
        Choice::StandBy => "suspend",
    };
    cmd.arg(verb);
    // Wait for the command and check it: a failed power action (ENOENT, no
    // polkit auth) must not look identical to success.
    match cmd.status() {
        Ok(s) if s.success() => exit(0),
        Ok(s) => {
            eprintln!("mde: '{verb}' command failed ({s})");
            exit(1);
        }
        Err(e) => {
            eprintln!("mde: could not run '{verb}': {e}");
            exit(1);
        }
    }
}

/// `mde lock` — headless: lock the session via logind (`loginctl
/// lock-session`), which labwc/swayidle turns into the swaylock surface. The
/// status is checked so a missing session (no logind seat) exits non-zero rather
/// than looking like success (E19.2).
pub fn lock() -> ExitCode {
    match Command::new("loginctl").arg("lock-session").status() {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        Ok(s) => {
            eprintln!("mde lock: 'loginctl lock-session' failed ({s})");
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("mde lock: could not run loginctl: {e}");
            ExitCode::FAILURE
        }
    }
}

fn shutdown_update(state: &mut Shutdown, m: M) -> Task<M> {
    match m {
        // The Win10 power flyout is one-click: picking a row acts immediately.
        // The classic Win2000 dropdown only selects; OK confirms.
        M::Pick(c) => {
            state.sel = c;
            if palette::is_windows10() {
                do_shutdown(&state.sel);
            }
        }
        M::Cancel => exit(0),
        M::Confirm => do_shutdown(&state.sel),
        M::Event(e) if is_enter(&e) => do_shutdown(&state.sel),
        M::Event(e) if is_escape(&e) => exit(0),
        M::Event(_) => {}
    }
    Task::none()
}

fn shutdown_view(state: &Shutdown) -> Element<'_, M> {
    if palette::is_windows10() {
        // Win10 power flyout: flat full-width Sleep / Shut down / Restart rows;
        // each acts on click (handled in shutdown_update under the era).
        return flyout(vec![
            flyout_row("Sleep", Choice::StandBy),
            flyout_row("Shut down", Choice::ShutDown),
            flyout_row("Restart", Choice::Restart),
        ]);
    }
    let choices = vec![
        Choice::LogOff,
        Choice::ShutDown,
        Choice::Restart,
        Choice::StandBy,
    ];
    let drop = pick_list(choices, Some(state.sel.clone()), M::Pick)
        .text_size(metrics::UI_PX)
        .style(mde_ui::sunken_picklist);

    let buttons = Row::new()
        .spacing(8.0)
        .push(Space::with_width(Length::Fill))
        .push(
            button(text("OK").size(metrics::UI_PX))
                .on_press(M::Confirm)
                .default(true)
                .width(Length::Fixed(76.0)),
        )
        .push(
            button(text("Cancel").size(metrics::UI_PX))
                .on_press(M::Cancel)
                .width(Length::Fixed(76.0)),
        );

    let body = Column::new()
        .spacing(14.0)
        .push(text("What do you want the computer to do?").size(metrics::UI_PX))
        .push(drop)
        .push(buttons);

    silver(body)
}

// ---------------- Run ----------------

struct Run {
    cmd: String,
}

#[derive(Debug, Clone)]
enum RunMsg {
    Input(String),
    Ok,
    Cancel,
    Event(Event),
}

/// The classic Win2000 Run dialog. Replaces the old `wofi --show run`
/// shell-out — `mde run` is its own subcommand, so the Start menu's Run launches
/// native MDE-Retro, not the layer it retires.
pub fn run_dialog() -> ExitCode {
    let r = iced::application(|_: &Run| "Run".to_string(), run_update, run_view)
        .window_size(iced::Size::new(360.0, 172.0))
        .resizable(false)
        .theme(|_| iced::Theme::Light)
        .subscription(|_: &Run| event::listen().map(RunMsg::Event))
        .font(mde_ui::font::REGULAR_BYTES)
        .font(mde_ui::font::BOLD_BYTES)
        .font(mde_ui::font::PLEX_REGULAR_BYTES)
        .font(mde_ui::font::PLEX_BOLD_BYTES)
        .default_font(mde_ui::font::ui())
        .run_with(|| (Run { cmd: String::new() }, Task::none()));
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}

fn exec_and_exit(cmd: &str) -> ! {
    let cmd = cmd.trim();
    if !cmd.is_empty() {
        let _ = Command::new("sh").arg("-c").arg(cmd).spawn();
    }
    exit(0)
}

fn run_update(state: &mut Run, m: RunMsg) -> Task<RunMsg> {
    match m {
        RunMsg::Input(s) => state.cmd = s,
        RunMsg::Ok => exec_and_exit(&state.cmd),
        RunMsg::Cancel => exit(0),
        RunMsg::Event(e) if is_enter(&e) => exec_and_exit(&state.cmd),
        RunMsg::Event(e) if is_escape(&e) => exit(0),
        RunMsg::Event(_) => {}
    }
    Task::none()
}

fn run_view(state: &Run) -> Element<'_, RunMsg> {
    let field = text_input("", &state.cmd)
        .on_input(RunMsg::Input)
        .on_submit(RunMsg::Ok)
        .size(metrics::UI_PX)
        .padding(pad(3.0, 4.0, 3.0, 4.0))
        .style(mde_ui::sunken_field);

    let buttons = Row::new()
        .spacing(8.0)
        .push(Space::with_width(Length::Fill))
        .push(
            button(text("OK").size(metrics::UI_PX))
                .on_press(RunMsg::Ok)
                .default(true)
                .width(Length::Fixed(76.0)),
        )
        .push(
            button(text("Cancel").size(metrics::UI_PX))
                .on_press(RunMsg::Cancel)
                .width(Length::Fixed(76.0)),
        );

    let body = Column::new()
        .spacing(12.0)
        .push(
            text("Type the name of a program, folder, or document, and Windows will open it for you.")
                .size(metrics::UI_PX),
        )
        .push(
            Row::new()
                .spacing(8.0)
                .align_y(iced::Alignment::Center)
                .push(text("Open:").size(metrics::UI_PX))
                .push(field),
        )
        .push(buttons);

    silver(body)
}

// ---------------- Properties ----------------

struct Properties {
    name: String,
    target: String,
}

#[derive(Debug, Clone)]
enum PropMsg {
    Close,
    Event(Event),
}

/// `mde properties <Name> <command>` — a Win2000 launcher Properties dialog
/// (General tab). Invoked by the Start-menu / launcher right-click context menu.
pub fn properties(name: String, target: String) -> ExitCode {
    let r = iced::application(
        |s: &Properties| format!("{} Properties", s.name),
        prop_update,
        prop_view,
    )
    .window_size(iced::Size::new(360.0, 240.0))
    .resizable(false)
    .theme(|_| iced::Theme::Light)
    .subscription(|_: &Properties| event::listen().map(PropMsg::Event))
    .font(mde_ui::font::REGULAR_BYTES)
    .font(mde_ui::font::BOLD_BYTES)
    .font(mde_ui::font::PLEX_REGULAR_BYTES)
    .font(mde_ui::font::PLEX_BOLD_BYTES)
    .default_font(mde_ui::font::ui())
    .run_with(move || (Properties { name, target }, Task::none()));
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}

fn prop_update(_: &mut Properties, m: PropMsg) -> Task<PropMsg> {
    match m {
        PropMsg::Close => exit(0),
        PropMsg::Event(e) if is_enter(&e) || is_escape(&e) => exit(0),
        PropMsg::Event(_) => Task::none(),
    }
}

fn prop_field<'a>(label: &'a str, value: String) -> Element<'a, PropMsg> {
    Row::new()
        .spacing(8.0)
        .push(
            text(label)
                .size(metrics::UI_PX)
                .font(mde_ui::font::ui_bold())
                .width(Length::Fixed(64.0)),
        )
        .push(text(value).size(metrics::UI_PX))
        .into()
}

fn prop_view(state: &Properties) -> Element<'_, PropMsg> {
    let kind = if state.target.contains('/') || !state.target.is_empty() {
        "Application"
    } else {
        "Item"
    };
    let body = Column::new()
        .spacing(10.0)
        .push(
            text(format!("{} — General", state.name))
                .size(metrics::UI_PX)
                .font(mde_ui::font::ui_bold()),
        )
        .push(prop_field("Name:", state.name.clone()))
        .push(prop_field("Type:", kind.to_string()))
        .push(prop_field("Target:", state.target.clone()))
        .push(Space::new(Length::Fill, Length::Fill))
        .push(
            Row::new()
                .spacing(8.0)
                .push(Space::with_width(Length::Fill))
                .push(
                    button(text("OK").size(metrics::UI_PX))
                        .on_press(PropMsg::Close)
                        .default(true)
                        .width(Length::Fixed(80.0)),
                )
                .push(
                    button(text("Cancel").size(metrics::UI_PX))
                        .on_press(PropMsg::Close)
                        .width(Length::Fixed(80.0)),
                ),
        );
    silver(body)
}
