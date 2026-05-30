//! System Properties — the Win2000/XP System Properties dialog, native iced
//! (rank-25 decision: no GTK). A tab strip over a raised content panel, fed by
//! the toolkit-agnostic [`crate::sysinfo`] data layer. The Hardware tab hosts
//! the native Device Manager tree (no HardInfo2).
//!
//! `mde system-properties`            opens the GUI
//! `mde system-properties --info`     prints the General facts (headless)
//! `mde system-properties --devices`  prints the Device Manager tree (headless)

use std::process::{exit, ExitCode};

use iced::widget::{container, scrollable, text, Column, Row, Space};
use iced::{Background, Element, Length, Padding, Task};

use mde_ui::{button, frame, metrics, palette};

use crate::sysinfo::{self, DeviceCategory, General};

const TABS: &[&str] = &[
    "General",
    "Computer Name",
    "Hardware",
    "Advanced",
    "System Restore",
    "Automatic Updates",
    "Remote",
];

struct SysProps {
    current: usize,
    general: General,
    devices: Vec<DeviceCategory>,
}

#[derive(Debug, Clone)]
enum Message {
    SelectTab(usize),
    Close,
}

/// Dispatch: headless flags print; otherwise open the GUI dialog.
pub fn run(args: &[String]) -> ExitCode {
    if args.iter().any(|a| a == "--info" || a == "--devices") {
        return sysinfo::run(args);
    }
    let r = iced::application(|_: &SysProps| "System Properties".to_string(), update, view)
        .window_size(iced::Size::new(420.0, 460.0))
        .resizable(false)
        .theme(|_| iced::Theme::Light)
        .font(mde_ui::font::REGULAR_BYTES)
        .font(mde_ui::font::BOLD_BYTES)
        .default_font(mde_ui::font::UI)
        .run_with(|| {
            (
                SysProps { current: 0, general: sysinfo::general(), devices: sysinfo::devices() },
                Task::none(),
            )
        });
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}

fn update(state: &mut SysProps, message: Message) -> Task<Message> {
    match message {
        Message::SelectTab(i) => state.current = i,
        Message::Close => exit(0),
    }
    Task::none()
}

fn pad(t: f32, r: f32, b: f32, l: f32) -> Padding {
    Padding { top: t, right: r, bottom: b, left: l }
}

// --- view ------------------------------------------------------------------

fn tab_strip(current: usize) -> Element<'static, Message> {
    let mut row = Row::new().spacing(2.0).padding(pad(2.0, 4.0, 0.0, 4.0));
    for (i, name) in TABS.iter().enumerate() {
        row = row.push(
            button(text(*name).size(metrics::UI_PX))
                .active(i == current)
                .on_press(Message::SelectTab(i))
                .padding(pad(2.0, 6.0, 2.0, 6.0)),
        );
    }
    row.into()
}

/// A "Label: value" line.
fn field<'a>(label: &'a str, value: String) -> Element<'a, Message> {
    Row::new()
        .spacing(6.0)
        .push(text(label).size(metrics::UI_PX).font(mde_ui::font::UI_BOLD).width(Length::Fixed(120.0)))
        .push(text(value).size(metrics::UI_PX))
        .into()
}

fn general_tab(g: &General) -> Element<'static, Message> {
    Column::new()
        .spacing(8.0)
        .push(text("System:").size(metrics::UI_PX).font(mde_ui::font::UI_BOLD))
        .push(field("", format!("{} {}", g.product, g.version)))
        .push(field("Kernel", g.kernel.clone()))
        .push(Space::new(Length::Fill, Length::Fixed(6.0)))
        .push(text("Registered to:").size(metrics::UI_PX).font(mde_ui::font::UI_BOLD))
        .push(field("", g.user.clone()))
        .push(Space::new(Length::Fill, Length::Fixed(6.0)))
        .push(text("Computer:").size(metrics::UI_PX).font(mde_ui::font::UI_BOLD))
        .push(field("Processor", g.cpu.clone()))
        .push(field("Processors", format!("{} logical", g.cores)))
        .push(field("Memory", g.mem_human()))
        .into()
}

fn computer_name_tab(g: &General) -> Element<'static, Message> {
    Column::new()
        .spacing(8.0)
        .push(text("Windows uses the following information to identify your computer on the network.").size(metrics::UI_PX))
        .push(Space::new(Length::Fill, Length::Fixed(6.0)))
        .push(field("Full computer name", g.hostname.clone()))
        .push(field("Workgroup", "WORKGROUP".to_string()))
        .into()
}

fn hardware_tab(devices: &[DeviceCategory]) -> Element<'static, Message> {
    // The native Device Manager tree (sunken white well, category → devices).
    let mut tree = Column::new().spacing(0.0);
    for cat in devices {
        tree = tree.push(text(cat.name).size(metrics::UI_PX).font(mde_ui::font::UI_BOLD));
        for d in &cat.devices {
            tree = tree.push(
                container(text(format!("    {d}")).size(metrics::UI_PX)).padding(pad(0.0, 0.0, 0.0, 8.0)),
            );
        }
    }
    let well = iced::widget::stack![
        frame::sunken().face(palette::color(palette::WINDOW)),
        container(scrollable(tree)).padding(3.0),
    ];
    Column::new()
        .spacing(8.0)
        .push(text("Device Manager lists the hardware devices installed on your computer.").size(metrics::UI_PX))
        .push(container(well).height(Length::Fill))
        .into()
}

fn placeholder(body: &'static str) -> Element<'static, Message> {
    Column::new().push(text(body).size(metrics::UI_PX)).into()
}

fn tab_content(state: &SysProps) -> Element<'static, Message> {
    match state.current {
        0 => general_tab(&state.general),
        1 => computer_name_tab(&state.general),
        2 => hardware_tab(&state.devices),
        3 => placeholder(
            "Advanced: Environment Variables, Performance (zram/swappiness), and Startup & Recovery (default boot entry, GRUB timeout).",
        ),
        4 => placeholder("System Restore: enable and create Timeshift snapshots."),
        5 => placeholder("Automatic Updates: configure the dnf-automatic timer."),
        6 => placeholder(
            "Remote: Remote Desktop via wayvnc (allow users to connect, address host:5900).",
        ),
        _ => placeholder(""),
    }
}

fn view(state: &SysProps) -> Element<'_, Message> {
    let panel = iced::widget::stack![
        frame::raised(),
        container(tab_content(state)).padding(12.0).width(Length::Fill).height(Length::Fill),
    ];

    let buttons = Row::new()
        .spacing(8.0)
        .push(Space::with_width(Length::Fill))
        .push(
            button(text("OK").size(metrics::UI_PX))
                .on_press(Message::Close)
                .default(true)
                .width(Length::Fixed(80.0)),
        )
        .push(button(text("Cancel").size(metrics::UI_PX)).on_press(Message::Close).width(Length::Fixed(80.0)));

    let body = Column::new()
        .spacing(6.0)
        .padding(pad(6.0, 10.0, 10.0, 10.0))
        .push(tab_strip(state.current))
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
