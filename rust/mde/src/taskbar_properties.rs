//! Taskbar and Start Menu Properties — the Win2000 property sheet reached from
//! Start ▸ Settings ▸ Taskbar & Start Menu and the taskbar/Start right-click
//! Properties. A General tab (taskbar appearance + Start-menu options) and an
//! Advanced tab, OK/Cancel/Apply.
//!
//! The one option wired to real behaviour is **Show small icons in Start menu**
//! (persisted to `start_small_icons`; the Start menu reads it on next open).
//! The remaining checkboxes are the authentic Win2000 set; those the sway-based
//! shell doesn't enforce are shown disabled, the classic "not applicable" look.

use std::process::ExitCode;

use iced::widget::{checkbox, container, text, Column, Row, Space};
use iced::{Background, Element, Length, Padding, Task};

use mde_ui::{button, frame, group_box, metrics, palette};

const TABS: &[&str] = &["General", "Advanced"];

struct TaskbarProps {
    tab: usize,
    small_icons: bool,
    // Cosmetic, for the authentic control set (not enforced by the shell).
    always_on_top: bool,
}

#[derive(Debug, Clone)]
enum Message {
    SelectTab(usize),
    ToggleSmallIcons(bool),
    Apply,
    Ok,
    Cancel,
}

pub fn run(_args: &[String]) -> ExitCode {
    match gui() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mde taskbar-properties: {e}");
            ExitCode::FAILURE
        }
    }
}

fn gui() -> iced::Result {
    iced::application(
        |_: &TaskbarProps| "Taskbar and Start Menu Properties".to_string(),
        update,
        view,
    )
    .window_size(iced::Size::new(380.0, 430.0))
    .resizable(false)
    .theme(|_| mde_ui::palette::iced_theme())
    .font(mde_ui::font::REGULAR_BYTES)
    .font(mde_ui::font::BOLD_BYTES)
    .font(mde_ui::font::PLEX_REGULAR_BYTES)
    .font(mde_ui::font::PLEX_BOLD_BYTES)
    .default_font(mde_ui::font::ui())
    .run_with(|| {
        let st = crate::state::load();
        (
            TaskbarProps {
                tab: 0,
                small_icons: st.start_small_icons,
                always_on_top: true,
            },
            Task::none(),
        )
    })
}

/// Write the wired settings back to the shell state.
fn persist(state: &TaskbarProps) {
    let mut st = crate::state::load();
    st.start_small_icons = state.small_icons;
    let _ = crate::state::save(&st);
}

fn update(state: &mut TaskbarProps, message: Message) -> Task<Message> {
    match message {
        Message::SelectTab(i) => state.tab = i,
        Message::ToggleSmallIcons(b) => state.small_icons = b,
        Message::Apply => persist(state),
        Message::Ok => {
            persist(state);
            std::process::exit(0);
        }
        Message::Cancel => std::process::exit(0),
    }
    Task::none()
}

fn pad(t: f32, r: f32, b: f32, l: f32) -> Padding {
    Padding {
        top: t,
        right: r,
        bottom: b,
        left: l,
    }
}

fn cbox<'a>(label: &str, on: bool, msg: fn(bool) -> Message) -> Element<'a, Message> {
    checkbox(label.to_string(), on)
        .on_toggle(msg)
        .style(mde_ui::checkbox_style)
        .text_size(metrics::UI_PX)
        .into()
}

/// A disabled (greyed) checkbox — present for fidelity, not enforced here.
fn cbox_disabled<'a>(label: &str, on: bool) -> Element<'a, Message> {
    checkbox(label.to_string(), on)
        .style(mde_ui::checkbox_style)
        .text_size(metrics::UI_PX)
        .into()
}

/// A small mock of the taskbar (Start button + clock), the General-tab preview.
fn taskbar_preview() -> Element<'static, Message> {
    let bar = Row::new()
        .spacing(4.0)
        .align_y(iced::Alignment::Center)
        .padding(pad(2.0, 4.0, 2.0, 4.0))
        .push(button(
            text("Start")
                .size(metrics::UI_PX)
                .font(mde_ui::font::ui_bold()),
        ))
        .push(Space::with_width(Length::Fill))
        .push(container(text("3:14 PM").size(metrics::UI_PX)).padding(pad(1.0, 6.0, 1.0, 6.0)));
    container(iced::widget::stack![frame::raised(), bar])
        .width(Length::Fixed(220.0))
        .height(Length::Fixed(28.0))
        .into()
}

fn general_tab(state: &TaskbarProps) -> Element<'_, Message> {
    let taskbar = Column::new()
        .spacing(6.0)
        .push(cbox_disabled(
            "Always keep the taskbar on top of other windows",
            state.always_on_top,
        ))
        .push(cbox_disabled("Auto hide the taskbar", false))
        // The panel always draws the clock; greyed (not a discarded toggle).
        .push(cbox_disabled("Show clock", true));

    let start_menu = Column::new()
        .spacing(6.0)
        .push(cbox(
            "Show small icons in Start menu",
            state.small_icons,
            Message::ToggleSmallIcons,
        ))
        // Personalized Menus isn't implemented; greyed for fidelity.
        .push(cbox_disabled("Use Personalized Menus", true));

    Column::new()
        .spacing(10.0)
        .push(container(taskbar_preview()).width(Length::Fill).center_x(Length::Fill).padding(pad(4.0, 0.0, 4.0, 0.0)))
        .push(group_box("Taskbar", taskbar))
        .push(group_box("Start menu", start_menu))
        .push(
            text("\u{201c}Show small icons in Start menu\u{201d} takes effect the next time you open the Start menu. Greyed options are managed by labwc.")
                .size(metrics::UI_PX - 1.0),
        )
        .into()
}

fn advanced_tab(_state: &TaskbarProps) -> Element<'static, Message> {
    let settings = Column::new()
        .spacing(6.0)
        .push(cbox_disabled("Display Administrative Tools", false))
        .push(cbox_disabled("Expand Control Panel", false))
        .push(cbox_disabled("Expand My Documents", false))
        .push(cbox_disabled("Scroll the Programs menu", false));
    Column::new()
        .spacing(10.0)
        .push(group_box("Start menu settings", settings))
        .push(text("These Advanced options are shown for fidelity; the labwc-based shell does not enforce them yet.").size(metrics::UI_PX - 1.0))
        .into()
}

fn tab_content(state: &TaskbarProps) -> Element<'_, Message> {
    match state.tab {
        0 => general_tab(state),
        _ => advanced_tab(state),
    }
}

fn view(state: &TaskbarProps) -> Element<'_, Message> {
    let panel = iced::widget::stack![
        frame::raised(),
        container(tab_content(state))
            .padding(12.0)
            .width(Length::Fill)
            .height(Length::Fill),
    ];

    let buttons = Row::new()
        .spacing(8.0)
        .push(Space::with_width(Length::Fill))
        .push(
            button(text("OK").size(metrics::UI_PX))
                .on_press(Message::Ok)
                .default(true)
                .width(Length::Fixed(80.0)),
        )
        .push(
            button(text("Cancel").size(metrics::UI_PX))
                .on_press(Message::Cancel)
                .width(Length::Fixed(80.0)),
        )
        .push(
            button(text("Apply").size(metrics::UI_PX))
                .on_press(Message::Apply)
                .width(Length::Fixed(80.0)),
        );

    let body = Column::new()
        .spacing(6.0)
        .padding(pad(6.0, 10.0, 10.0, 10.0))
        .push(mde_ui::tab_strip(TABS, state.tab, Message::SelectTab))
        .push(container(panel).height(Length::Fill))
        .push(buttons);

    container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(palette::color(palette::MENU))),
            ..container::Style::default()
        })
        .into()
}
