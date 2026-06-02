//! `mde about` — the winver-style "About MDE Retro Workstation" dialog.
//!
//! A small Win2000 System-Properties-flavored box: the brand logo + product,
//! the MDE version over the Fedora base line, who it's registered to, and live
//! system specs (CPU / memory / kernel), with an OK button.

use std::process::ExitCode;

use iced::widget::{button, container, text, Column, Row, Space};
use iced::{Element, Length, Padding, Task};

use mde_ui::{frame, metrics, palette};

use crate::sysinfo;

/// The brand logo (carbon grid on the black/blue tile).
const LOGO: &[u8] = include_bytes!("../../assets/branding/mde-logo.svg");
const PRODUCT: &str = "MDE Retro Workstation";

#[derive(Debug, Clone)]
enum Message {
    Ok,
}

struct About {
    g: sysinfo::General,
    registered: String,
}

pub fn run(_args: &[String]) -> ExitCode {
    let r = iced::application(|_: &About| format!("About {PRODUCT}"), update, view)
        .theme(|_| mde_ui::palette::iced_theme())
        .window_size(iced::Size::new(400.0, 340.0))
        .font(mde_ui::font::REGULAR_BYTES)
        .font(mde_ui::font::BOLD_BYTES)
        .font(mde_ui::font::PLEX_REGULAR_BYTES)
        .font(mde_ui::font::PLEX_BOLD_BYTES)
        .default_font(mde_ui::font::ui())
        .run_with(|| {
            let g = sysinfo::general();
            let registered = registered_name(&g.user);
            (About { g, registered }, Task::none())
        });
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mde about: {e}");
            ExitCode::FAILURE
        }
    }
}

/// The account's display name: the GECOS full name, else the username.
fn registered_name(user: &str) -> String {
    if let Ok(passwd) = std::fs::read_to_string("/etc/passwd") {
        for line in passwd.lines() {
            let f: Vec<&str> = line.split(':').collect();
            if f.first() == Some(&user) {
                if let Some(name) = f.get(4).and_then(|g| g.split(',').next()).map(str::trim) {
                    if !name.is_empty() {
                        return name.to_string();
                    }
                }
            }
        }
    }
    user.to_string()
}

fn update(_state: &mut About, message: Message) -> Task<Message> {
    match message {
        Message::Ok => std::process::exit(0),
    }
}

fn label(s: String) -> iced::widget::Text<'static> {
    text(s).size(metrics::UI_PX)
}

fn view(state: &About) -> Element<'_, Message> {
    let g = &state.g;
    let bold = mde_ui::font::ui_bold();

    // Header: logo + product / version / Fedora base.
    let header = Row::new()
        .spacing(12.0)
        .align_y(iced::Alignment::Center)
        .push(
            iced::widget::svg(iced::widget::svg::Handle::from_memory(LOGO))
                .width(Length::Fixed(56.0))
                .height(Length::Fixed(56.0)),
        )
        .push(
            Column::new()
                .spacing(2.0)
                .push(text(PRODUCT).size(metrics::INFO_TITLE_PX).font(bold))
                .push(label(format!("Version {}", env!("CARGO_PKG_VERSION"))))
                .push(label(format!(
                    "Built on Fedora {}",
                    fedora_version(&g.version)
                ))),
        );

    let specs = Column::new()
        .spacing(3.0)
        .push(label("This product is registered to:".into()))
        .push(
            text(state.registered.clone())
                .size(metrics::UI_PX)
                .font(bold),
        )
        .push(Space::with_height(Length::Fixed(8.0)))
        .push(label(format!("Processor: {} ({} cores)", g.cpu, g.cores)))
        .push(label(format!("Memory: {}", g.mem_human())))
        .push(label(format!("Kernel: {}", g.kernel)))
        .push(label(format!("Computer: {}", g.hostname)));

    let ok = container(
        button(text("OK").size(metrics::UI_PX))
            .on_press(Message::Ok)
            .padding(Padding {
                top: 2.0,
                right: 18.0,
                bottom: 2.0,
                left: 18.0,
            }),
    )
    .width(Length::Fill)
    .align_x(iced::alignment::Horizontal::Right);

    let body = Column::new()
        .spacing(10.0)
        .padding(16.0)
        .push(header)
        .push(rule())
        .push(specs)
        .push(Space::with_height(Length::Fill))
        .push(ok);

    container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(palette::color(palette::MENU))),
            ..container::Style::default()
        })
        .into()
}

/// A 2px etched divider.
fn rule() -> Element<'static, Message> {
    iced::widget::stack![frame::sunken().thickness(1)]
        .width(Length::Fill)
        .height(Length::Fixed(2.0))
        .into()
}

/// Pull a short Fedora version number out of the os-release VERSION string
/// (e.g. "44 (Workstation Edition)" → "44"); falls back to the whole string.
fn fedora_version(version: &str) -> String {
    version
        .split_whitespace()
        .next()
        .unwrap_or(version)
        .to_string()
}
