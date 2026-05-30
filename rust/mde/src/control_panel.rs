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
use iced::{Background, Border, Color, Element, Length, Padding, Shadow, Task};

use mde_ui::{frame, palette};

use crate::fedora;

const BLUE: Color = Color::from_rgb(0x1d as f32 / 255.0, 0x5c as f32 / 255.0, 0xa8 as f32 / 255.0);

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
                } else {
                    let _ = fedora::install(&[tool.package]);
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
            button(text(label).size(11.0))
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
    let white = Color::WHITE;
    let bold = mde_ui::font::UI_BOLD;
    let col = Column::new()
        .spacing(8.0)
        .padding(pad(10.0, 12.0, 10.0, 12.0))
        .push(text("Control Panel").size(15.0).font(bold).color(white))
        .push(container(Space::new(Length::Fill, Length::Fixed(2.0))).style(
            |_| container::Style {
                background: Some(Background::Color(Color::WHITE)),
                ..container::Style::default()
            },
        ))
        .push(
            text("Select an item to view its description.")
                .size(11.0)
                .color(white),
        )
        .push(
            text("Configures your computer and adds or removes programs and devices.")
                .size(11.0)
                .color(white),
        )
        .push(Space::new(Length::Fill, Length::Fixed(8.0)))
        .push(text("See also:").size(11.0).color(white))
        .push(text("Administrative Tools").size(11.0).color(white))
        .push(text("Windows Update").size(11.0).color(white));

    container(col)
        .width(Length::Fixed(190.0))
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(BLUE)),
            ..container::Style::default()
        })
        .into()
}

fn grid(state: &ControlPanel) -> Element<'_, Message> {
    let bold = mde_ui::font::UI_BOLD;
    let mut col = Column::new().spacing(0.0).padding(pad(4.0, 4.0, 4.0, 6.0));
    for category in fedora::categories() {
        col = col.push(
            container(text(category).size(11.0).font(bold)).padding(pad(5.0, 6.0, 1.0, 4.0)),
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
            col = col.push(
                button(text(label).size(11.0))
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

fn status_bar<'a>() -> Element<'a, Message> {
    let total = fedora::TOOLS.len();
    let missing = fedora::missing().len();
    container(iced::widget::stack![
        frame::sunken().thickness(1),
        container(text(format!("{total} items, {missing} not installed")).size(11.0))
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
