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

use iced::widget::{checkbox, container, radio, scrollable, text, Column, Row, Space};
use iced::{Background, Border, Element, Length, Padding, Shadow, Task};

use mde_ui::{button, frame, group_box, metrics, palette};

use crate::sysinfo::{self, Advanced, AutoMode, DeviceCategory, General};

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
    advanced: Advanced,
    /// Local (unsaved) selections on the interactive tabs; initialised from the
    /// live state once the scan lands. Apply persists them via a privileged tool.
    auto_sel: AutoMode,
    remote_on: bool,
    /// Expanded category indices in the Device Manager tree.
    expanded: HashSet<usize>,
    /// False until the (slow) scan finishes, so the data tabs can show "Loading…"
    /// instead of empty/false state that looks like real data.
    scanned: bool,
}

#[derive(Debug, Clone)]
enum Message {
    SelectTab(usize),
    ToggleCategory(usize),
    Loaded(Vec<DeviceCategory>, Advanced),
    SetAuto(AutoMode),
    ToggleRemote(bool),
    Launch(String),
    Close,
}

/// Run a shell command detached (the Apply / launch buttons on the data tabs).
fn spawn(cmd: &str) {
    let _ = std::process::Command::new("sh").arg("-c").arg(cmd).spawn();
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
        .font(mde_ui::font::BOLD_BYTES).font(mde_ui::font::PLEX_REGULAR_BYTES).font(mde_ui::font::PLEX_BOLD_BYTES)
        .default_font(mde_ui::font::ui())
        .run_with(|| {
            // The General facts are cheap (/proc + os-release), so load them now;
            // the device scan (lspci/lsblk/lsusb) + the advanced probes would
            // block the first paint, so fetch them in a task and let the window
            // appear at once.
            (
                SysProps {
                    current: 0,
                    general: sysinfo::general(),
                    devices: Vec::new(),
                    advanced: Advanced::default(),
                    auto_sel: AutoMode::Off,
                    remote_on: false,
                    expanded: HashSet::new(),
                    scanned: false,
                },
                Task::perform(
                    async { (sysinfo::devices(), sysinfo::advanced()) },
                    |(d, a)| Message::Loaded(d, a),
                ),
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
        Message::Loaded(devs, adv) => {
            // Start with every category expanded (an informative first view),
            // and seed the interactive tabs from the live state.
            state.expanded = (0..devs.len()).collect();
            state.devices = devs;
            state.auto_sel = adv.auto_updates;
            state.remote_on = adv.remote_running;
            state.advanced = adv;
            state.scanned = true;
        }
        Message::SetAuto(mode) => state.auto_sel = mode,
        Message::ToggleRemote(on) => state.remote_on = on,
        Message::Launch(cmd) => spawn(&cmd),
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
    // without overflowing the dialog width — now drawn with the authentic
    // property-sheet tab control (active tab merges into the page).
    let row1 = mde_ui::tab_strip(&TABS[0..4], current, Message::SelectTab);
    let sel2 = if current >= 4 { current - 4 } else { usize::MAX };
    let row2 = mde_ui::tab_strip(&TABS[4..], sel2, |i| Message::SelectTab(i + 4));
    Column::new().spacing(0.0).push(row1).push(row2).into()
}

/// A "Label: value" line.
fn field<'a>(label: &'a str, value: String) -> Element<'a, Message> {
    Row::new()
        .spacing(6.0)
        .push(text(label).size(metrics::UI_PX).font(mde_ui::font::ui_bold()).width(Length::Fixed(120.0)))
        .push(text(value).size(metrics::UI_PX))
        .into()
}

fn general_tab(g: &General) -> Element<'static, Message> {
    Column::new()
        .spacing(8.0)
        .push(text("System:").size(metrics::UI_PX).font(mde_ui::font::ui_bold()))
        .push(field("", format!("{} {}", g.product, g.version)))
        .push(field("Kernel", g.kernel.clone()))
        .push(Space::new(Length::Fill, Length::Fixed(6.0)))
        .push(text("Registered to:").size(metrics::UI_PX).font(mde_ui::font::ui_bold()))
        .push(field("", g.user.clone()))
        .push(Space::new(Length::Fill, Length::Fixed(6.0)))
        .push(text("Computer:").size(metrics::UI_PX).font(mde_ui::font::ui_bold()))
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
    if !state.scanned {
        tree = tree.push(
            container(text("Scanning hardware devices\u{2026}").size(metrics::UI_PX))
                .padding(pad(2.0, 4.0, 2.0, 4.0)),
        );
    }
    for (i, cat) in state.devices.iter().enumerate() {
        let open = state.expanded.contains(&i);
        // "+"/"-" (the Win2000 tree control) — Droid Sans lacks the ▶/▼ glyphs.
        let marker = if open { "- " } else { "+ " };
        tree = tree.push(
            iced::widget::button(
                text(format!("{marker}{}", cat.name)).size(metrics::UI_PX).font(mde_ui::font::ui_bold()),
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

fn loading() -> Element<'static, Message> {
    Column::new().push(text("Loading\u{2026}").size(metrics::UI_PX)).into()
}

/// A small raised button that runs `cmd` when pressed.
fn launch_button(label: &str, cmd: String) -> Element<'static, Message> {
    button(text(label.to_string()).size(metrics::UI_PX))
        .on_press(Message::Launch(cmd))
        .padding(pad(2.0, 10.0, 2.0, 10.0))
        .into()
}

fn advanced_tab(state: &SysProps) -> Element<'static, Message> {
    if !state.scanned {
        return loading();
    }
    let a = &state.advanced;
    let perf = Column::new()
        .spacing(6.0)
        .push(field("Swappiness", a.swappiness.clone()))
        .push(field("zram device", a.zram.clone()));
    let boot = Column::new()
        .spacing(6.0)
        .push(field("Default boot entry", a.grub_default.clone()))
        .push(field("Boot menu timeout", format!("{} s", a.grub_timeout)));
    Column::new()
        .spacing(10.0)
        .push(group_box("Performance", perf))
        .push(group_box("Startup and Recovery", boot))
        .push(field("Environment variables", format!("{} set", a.env_count)))
        .push(
            Row::new().push(Space::with_width(Length::Fill)).push(launch_button(
                "Environment Variables\u{2026}",
                "foot sh -c 'env | sort | less'".to_string(),
            )),
        )
        .into()
}

fn restore_tab(state: &SysProps) -> Element<'static, Message> {
    if !state.scanned {
        return loading();
    }
    let installed = state.advanced.timeshift_installed;
    // A read-only checkbox (no on_toggle ⇒ disabled) reflecting real state.
    let status = checkbox("Timeshift snapshot tool is installed", installed).style(mde_ui::checkbox_style);
    let body = Column::new()
        .spacing(8.0)
        .push(text("System Restore rolls the system back using Timeshift snapshots.").size(metrics::UI_PX))
        .push(status)
        .push(
            Row::new().push(Space::with_width(Length::Fill)).push(launch_button(
                if installed { "Open Timeshift\u{2026}" } else { "Install Timeshift\u{2026}" },
                if installed {
                    "timeshift-launcher".to_string()
                } else {
                    "pkexec dnf install -y timeshift".to_string()
                },
            )),
        );
    Column::new().push(group_box("System Restore", body)).into()
}

fn updates_tab(state: &SysProps) -> Element<'static, Message> {
    if !state.scanned {
        return loading();
    }
    let opt = |label: &str, mode: AutoMode| {
        radio(label.to_string(), mode, Some(state.auto_sel), Message::SetAuto).style(mde_ui::radio_style).size(13.0).text_size(metrics::UI_PX)
    };
    let group = Column::new()
        .spacing(6.0)
        .push(opt("Turn off automatic updates", AutoMode::Off))
        .push(opt("Download updates automatically, notify before installing", AutoMode::DownloadOnly))
        .push(opt("Install updates automatically", AutoMode::Install));
    // Apply via the dnf-automatic timer (privileged; the radios pick the posture).
    let apply_cmd = match state.auto_sel {
        AutoMode::Off => "pkexec systemctl disable --now dnf-automatic.timer".to_string(),
        _ => "pkexec systemctl enable --now dnf-automatic.timer".to_string(),
    };
    Column::new()
        .spacing(10.0)
        .push(group_box("Keep my computer up to date", group))
        .push(Row::new().push(Space::with_width(Length::Fill)).push(launch_button("Apply", apply_cmd)))
        .into()
}

fn remote_tab(state: &SysProps) -> Element<'static, Message> {
    if !state.scanned {
        return loading();
    }
    let avail = state.advanced.remote_available;
    let check = checkbox("Allow remote connections to this computer", state.remote_on)
        .on_toggle(Message::ToggleRemote)
        .style(mde_ui::checkbox_style);
    let apply_cmd = if state.remote_on {
        "wayvnc 0.0.0.0 5900".to_string()
    } else {
        "pkill -x wayvnc".to_string()
    };
    let body = Column::new()
        .spacing(8.0)
        .push(text("Remote Desktop lets you connect to this computer over VNC (wayvnc).").size(metrics::UI_PX))
        .push(check)
        .push(field("Listen address", "0.0.0.0:5900".to_string()))
        .push(
            Row::new().push(Space::with_width(Length::Fill)).push(if avail {
                launch_button("Apply", apply_cmd)
            } else {
                launch_button("Install wayvnc\u{2026}", "pkexec dnf install -y wayvnc".to_string())
            }),
        );
    Column::new().push(group_box("Remote Desktop", body)).into()
}

fn tab_content(state: &SysProps) -> Element<'static, Message> {
    match state.current {
        0 => general_tab(&state.general),
        1 => computer_name_tab(&state.general),
        2 => hardware_tab(state),
        3 => advanced_tab(state),
        4 => restore_tab(state),
        5 => updates_tab(state),
        6 => remote_tab(state),
        _ => loading(),
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
