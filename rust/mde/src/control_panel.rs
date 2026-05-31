//! Control Panel — Win2000-named mapping of Fedora system tools.
//!
//! Default (no args) opens the GUI: an Explorer-style window with the blue
//! "web view" info-pane on the left and a white, categorized tool area on the
//! right (matching the My Computer reference). Clicking a tool launches it
//! (CLI tools at 150%); clicking a missing tool installs it via `pkexec dnf`.
//!
//! Headless subcommands remain for scripting:
//!   mde control-panel --list            list tools + [installed]/[MISSING]
//!   mde control-panel --launch N        launch tool number N
//!   mde control-panel --install-missing pkexec dnf the missing ones

use std::process::ExitCode;

use iced::widget::{button, container, scrollable, text, Column, Row, Space};
use iced::{Background, Border, Element, Length, Padding, Shadow, Task};

use mde_ui::{frame, metrics, palette};

use crate::fedora;

pub fn run(args: &[String]) -> ExitCode {
    match args.first().map(String::as_str) {
        Some("--list") => {
            list();
            ExitCode::SUCCESS
        }
        Some("--launch") => launch_n(args.get(1)),
        Some("--install-missing") => install_missing(),
        _ => match gui() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("mde control-panel: {e}");
                ExitCode::FAILURE
            }
        },
    }
}

// --- GUI -------------------------------------------------------------------

#[derive(Default)]
struct ControlPanel {
    selected: Option<usize>,
    last_click: Option<(usize, std::time::Instant)>,
}

#[derive(Debug, Clone)]
enum Message {
    Activate(usize),
    Noop,
}

fn gui() -> iced::Result {
    iced::application(|_: &ControlPanel| "Control Panel - mde".to_string(), update, view)
        .theme(|_| iced::Theme::Light)
        .font(mde_ui::font::REGULAR_BYTES)
        .font(mde_ui::font::BOLD_BYTES)
        .default_font(mde_ui::font::UI)
        .run()
}

fn update(state: &mut ControlPanel, message: Message) -> Task<Message> {
    if let Message::Activate(i) = message {
        // Single-click selects; double-click (<400ms) opens — classic shell.
        let now = std::time::Instant::now();
        let is_double = state
            .last_click
            .map(|(li, lt)| li == i && now.duration_since(lt) < std::time::Duration::from_millis(400))
            .unwrap_or(false);
        if is_double {
            state.last_click = None;
            if let Some(tool) = fedora::TOOLS.get(i) {
                if fedora::is_installed(tool) {
                    let _ = fedora::launch(tool);
                } else if matches!(fedora::install(&[tool.package]), Ok(s) if s.success()) {
                    // Install + open in one gesture, like Win2000 Add/Remove.
                    let _ = fedora::launch(tool);
                }
            }
        } else {
            state.selected = Some(i);
            state.last_click = Some((i, now));
        }
    }
    Task::none()
}

fn pad(top: f32, right: f32, bottom: f32, left: f32) -> Padding {
    Padding { top, right, bottom, left }
}

fn flat(theme: &iced::Theme, status: button::Status) -> button::Style {
    item_style(false)(theme, status)
}

fn item_style(selected: bool) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let hot = selected || matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
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
}

fn menubar<'a>() -> Element<'a, Message> {
    let mut bar = Row::new();
    for label in ["File", "Edit", "View", "Favorites", "Tools", "Help"] {
        bar = bar.push(
            button(text(label).size(metrics::UI_PX))
                .on_press(Message::Noop)
                .padding(pad(2.0, 8.0, 2.0, 8.0))
                .style(flat),
        );
    }
    container(bar)
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(palette::color(palette::MENU))),
            ..container::Style::default()
        })
        .into()
}

fn sidebar<'a>() -> Element<'a, Message> {
    let white = palette::color(palette::WINDOW);
    let bold = mde_ui::font::UI_BOLD;
    let col = Column::new()
        .spacing(8.0)
        .padding(pad(10.0, 12.0, 10.0, 12.0))
        .push(text("Control Panel").size(15.0).font(bold).color(white))
        .push(container(Space::new(Length::Fill, Length::Fixed(2.0))).style(
            move |_| container::Style {
                background: Some(Background::Color(white)),
                ..container::Style::default()
            },
        ))
        .push(
            text("Select an item to view its description.")
                .size(metrics::UI_PX)
                .color(white),
        )
        .push(
            text("Configures your computer and adds or removes programs and devices.")
                .size(metrics::UI_PX)
                .color(white),
        )
        .push(Space::new(Length::Fill, Length::Fixed(8.0)))
        .push(text("See also:").size(metrics::UI_PX).color(white))
        .push(text("Administrative Tools").size(metrics::UI_PX).color(white))
        .push(text("Windows Update").size(metrics::UI_PX).color(white));

    container(col)
        .width(Length::Fixed(190.0))
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(palette::color(palette::INFO_BAND))),
            ..container::Style::default()
        })
        .into()
}

fn grid(state: &ControlPanel) -> Element<'_, Message> {
    let bold = mde_ui::font::UI_BOLD;
    let mut col = Column::new().spacing(0.0).padding(pad(4.0, 4.0, 4.0, 6.0));
    for category in fedora::categories() {
        col = col.push(
            container(text(category).size(metrics::UI_PX).font(bold)).padding(pad(5.0, 6.0, 1.0, 4.0)),
        );
        for (i, tool) in fedora::TOOLS.iter().enumerate() {
            if tool.category != category {
                continue;
            }
            let label = if fedora::is_installed(tool) {
                tool.name.to_string()
            } else {
                format!("{}  (install)", tool.name)
            };
            let row = Row::new()
                .spacing(5.0)
                .align_y(iced::Alignment::Center)
                .push(crate::icons::icon_any(applet_icon(tool.name, tool.category), 16))
                .push(text(label).size(metrics::UI_PX));
            col = col.push(
                button(row)
                    .on_press(Message::Activate(i))
                    .width(Length::Fill)
                    .padding(pad(2.0, 8.0, 2.0, 8.0))
                    .style(item_style(state.selected == Some(i))),
            );
        }
    }
    iced::widget::stack![
        frame::sunken().face(palette::color(palette::WINDOW)),
        container(scrollable(col)).padding(2.0),
    ]
    .into()
}

/// Freedesktop icon-name candidates for a Control Panel applet, by name keyword
/// then category. The resolver tries each and falls back to blank space, so an
/// unmatched applet simply shows no icon rather than tofu.
fn applet_icon(name: &str, category: &str) -> &'static [&'static str] {
    let n = name.to_ascii_lowercase();
    let has = |k: &str| n.contains(k);
    if has("add/remove") || has("program") {
        &["applications-other", "system-software-install", "package-x-generic"]
    } else if has("firewall") {
        &["network-firewall", "security-high", "preferences-system-network"]
    } else if has("network") || has("dial-up") || has("dns") {
        &["network-workgroup", "network-wired", "preferences-system-network"]
    } else if has("sound") || has("multimedia") || has("audio") {
        &["multimedia-volume-control", "audio-card", "preferences-desktop-sound"]
    } else if has("display") || has("screen") {
        &["preferences-desktop-display", "video-display"]
    } else if has("partition") || has("disk") || has("storage") || has("drive") || has("usage") || has("smart") || has("health") {
        &["drive-harddisk", "utilities-system-monitor"]
    } else if has("user") || has("password") {
        &["system-users", "preferences-desktop-user", "user-info"]
    } else if has("regional") || has("language") || has("keyboard") {
        &["preferences-desktop-keyboard", "preferences-desktop-locale"]
    } else if has("event") || has("log") {
        &["utilities-log-viewer", "logviewer", "text-x-generic"]
    } else if has("security") || has("lynis") {
        &["security-high", "preferences-system-privacy"]
    } else if has("date") || has("time") {
        &["preferences-system-time", "clock"]
    } else if has("printer") || has("print") {
        &["printer"]
    } else if has("update") {
        &["system-software-update"]
    } else if has("installation media") || has("media") {
        &["media-optical", "drive-optical"]
    } else if has("cockpit") || has("dashboard") || has("web console") {
        &["network-server", "applications-internet"]
    } else if has("stacer") || has("btop") || has("htop") || has("glances") || has("iotop") || has("nvtop") || has("monitor") || has("performance") {
        &["utilities-system-monitor", "applications-system"]
    } else if has("timeshift") || has("snapshot") || has("backup") || has("restore") {
        &["deja-dup", "drive-harddisk", "system-software-install"]
    } else if has("flatpak") || has("dnf") || has("plugin") || has("package") {
        &["package-x-generic", "system-software-install"]
    } else if has("nmap") || has("iperf") || has("wireshark") {
        &["network-workgroup", "applications-internet"]
    } else if has("powertop") || has("sensor") {
        &["battery", "preferences-system-power"]
    } else if has("fastfetch") || has("temporary") || has("clean") {
        &["utilities-terminal", "user-trash"]
    } else if has("service") {
        &["applications-system", "preferences-system"]
    } else if has("system") {
        &["computer", "preferences-system"]
    } else {
        match category {
            "Administrative Tools" => &["applications-system", "preferences-system"],
            "Resource Monitoring" => &["utilities-system-monitor"],
            "Storage & Disk" => &["drive-harddisk"],
            "Backup & Recovery" => &["drive-harddisk", "system-software-install"],
            "Package Management" => &["package-x-generic", "system-software-install"],
            "All-in-One Dashboard" => &["applications-system", "network-server"],
            _ => &["preferences-system", "applications-system", "application-x-executable"],
        }
    }
}

fn status_bar<'a>() -> Element<'a, Message> {
    let total = fedora::TOOLS.len();
    let missing = fedora::missing().len();
    container(iced::widget::stack![
        frame::sunken().thickness(1),
        container(text(format!("{total} items, {missing} not installed")).size(metrics::UI_PX))
            .padding(pad(1.0, 6.0, 1.0, 6.0)),
    ])
    .width(Length::Fill)
    .height(Length::Fixed(18.0))
    .into()
}

fn view(state: &ControlPanel) -> Element<'_, Message> {
    let body = Row::new()
        .push(sidebar())
        .push(container(grid(state)).width(Length::Fill).height(Length::Fill).padding(2.0));

    let content = Column::new()
        .push(menubar())
        .push(container(body).width(Length::Fill).height(Length::Fill))
        .push(status_bar());

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(palette::color(palette::MENU))),
            ..container::Style::default()
        })
        .into()
}

// --- headless backend ------------------------------------------------------

fn list() {
    println!("Control Panel — Fedora system tools\n");
    let mut n = 0;
    for category in fedora::categories() {
        println!("{category}");
        for tool in fedora::TOOLS.iter().filter(|t| t.category == category) {
            n += 1;
            let status = if fedora::is_installed(tool) {
                "installed"
            } else {
                "MISSING  "
            };
            println!("  {:>2}. [{}]  {:<32}  ({})", n, status, tool.name, fedora::binary(tool.command));
        }
        println!();
    }
    let missing = fedora::missing_packages();
    if missing.is_empty() {
        println!("All backing tools are installed.");
    } else {
        println!("{} missing. Packages: {}", missing.len(), missing.join(" "));
    }
}

fn launch_n(arg: Option<&String>) -> ExitCode {
    match arg
        .and_then(|s| s.parse::<usize>().ok())
        .and_then(|n| fedora::TOOLS.get(n.saturating_sub(1)))
    {
        Some(tool) => match fedora::launch(tool) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("launch failed: {e}");
                ExitCode::FAILURE
            }
        },
        None => {
            eprintln!("--launch needs a valid tool number");
            ExitCode::from(2)
        }
    }
}

fn install_missing() -> ExitCode {
    let packages = fedora::missing_packages();
    if packages.is_empty() {
        println!("Nothing to install.");
        return ExitCode::SUCCESS;
    }
    println!("Installing: {}", packages.join(" "));
    match fedora::install(&packages) {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        Ok(s) => {
            eprintln!("dnf exited with {s}");
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("pkexec dnf failed: {e}");
            ExitCode::FAILURE
        }
    }
}
