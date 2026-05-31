//! System Properties — the Win2000/XP System Properties dialog, native iced
//! (rank-25 decision: no GTK). A tab strip over a raised content panel, fed by
//! the toolkit-agnostic [`crate::sysinfo`] data layer. The Hardware tab hosts
//! the native Device Manager tree (no HardInfo2).
//!
//! `mde system-properties`            opens the GUI
//! `mde system-properties --info`     prints the General facts (headless)
//! `mde system-properties --devices`  prints the Device Manager tree (headless)

use std::collections::HashSet;
use std::process::{exit, ExitCode};

use iced::widget::{container, scrollable, text, Column, Row, Space};
use iced::{Background, Border, Element, Length, Padding, Shadow, Task};

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
    /// Expanded category indices in the Device Manager tree.
    expanded: HashSet<usize>,
}

#[derive(Debug, Clone)]
enum Message {
    SelectTab(usize),
    ToggleCategory(usize),
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
            let devices = sysinfo::devices();
            // Start with every category expanded (an informative first view).
            let expanded = (0..devices.len()).collect();
            (
                SysProps { current: 0, general: sysinfo::general(), devices, expanded },
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
        Message::ToggleCategory(i) => {
            if !state.expanded.remove(&i) {
                state.expanded.insert(i);
            }
        }
        Message::Close => exit(0),
    }
    Task::none()
}

fn pad(t: f32, r: f32, b: f32, l: f32) -> Padding {
    Padding { top: t, right: r, bottom: b, left: l }
}

// --- view ------------------------------------------------------------------

fn tab_strip(current: usize) -> Element<'static, Message> {
    // Two rows (the XP/Win2000 layout, per SPEC-system.md) so all 7 tabs fit
    // without overflowing the dialog width.
    let make_row = |range: std::ops::Range<usize>| {
        let mut row = Row::new().spacing(2.0).padding(pad(2.0, 4.0, 0.0, 4.0));
        for i in range {
            row = row.push(
                button(text(TABS[i]).size(metrics::UI_PX))
                    .active(i == current)
                    .on_press(Message::SelectTab(i))
                    .padding(pad(2.0, 6.0, 2.0, 6.0)),
            );
        }
        row
    };
    Column::new().push(make_row(0..4)).push(make_row(4..TABS.len())).into()
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

/// Flat tree-row button: navy HIGHLIGHT on hover, white hot text (the Win2000
/// tree-view row look — not a 3D bevel).
fn tree_row_style(_t: &iced::Theme, status: iced::widget::button::Status) -> iced::widget::button::Style {
    let hot = matches!(status, iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed);
    iced::widget::button::Style {
        background: hot.then(|| Background::Color(palette::color(palette::HIGHLIGHT))),
        text_color: if hot {
            palette::color(palette::HIGHLIGHT_TEXT)
        } else {
            palette::color(palette::WINDOW_TEXT)
        },
        border: Border::default(),
        shadow: Shadow::default(),
    }
}

fn hardware_tab(state: &SysProps) -> Element<'static, Message> {
    // Native Device Manager: a collapsible tree (category ▶/▼ → devices), in a
    // sunken white well. Expanding/collapsing is a click on the category row.
    let mut tree = Column::new().spacing(0.0);
    for (i, cat) in state.devices.iter().enumerate() {
        let open = state.expanded.contains(&i);
        // "+"/"-" (the Win2000 tree control) — Droid Sans lacks the ▶/▼ glyphs.
        let marker = if open { "- " } else { "+ " };
        tree = tree.push(
            iced::widget::button(
                text(format!("{marker}{}", cat.name)).size(metrics::UI_PX).font(mde_ui::font::UI_BOLD),
            )
            .on_press(Message::ToggleCategory(i))
            .width(Length::Fill)
            .padding(pad(1.0, 4.0, 1.0, 4.0))
            .style(tree_row_style),
        );
        if open {
            for d in &cat.devices {
                tree = tree.push(
                    container(text(d.clone()).size(metrics::UI_PX)).padding(pad(0.0, 4.0, 0.0, 22.0)),
                );
            }
        }
    }
    let well = iced::widget::stack![
        frame::sunken().face(palette::color(palette::WINDOW)),
        container(scrollable(tree).style(mde_ui::scrollbar)).padding(3.0),
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
        2 => hardware_tab(state),
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
