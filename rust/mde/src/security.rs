//! Windows 10 "Windows Security" dashboard home (E14.4).
//!
//! A small iced window showing the 6 posture tiles (icon + title + status line +
//! an OK/WARN/RISK glyph), fed by [`crate::security_probe`] off the UI thread via
//! an async `Loaded` (the `system_properties.rs` pattern), so the window paints at
//! once and the probes fill in. Era-gated to Windows 10 (E14.10). The per-tile
//! detail pages land in E14.5–E14.9; this is the home grid.

use std::process::ExitCode;

use iced::widget::{column, container, text, Column, Row, Space};
use iced::{Element, Length, Padding, Task};

use crate::security_probe::{self, Level, SecurityStatus, Tile};
use mde_ui::{metrics, palette};

struct Security {
    status: Option<SecurityStatus>,
}

#[derive(Debug, Clone)]
enum Message {
    Loaded(Box<SecurityStatus>),
}

pub fn run(_args: &[String]) -> ExitCode {
    // Era gate (E14.10): the Security dashboard is a Windows 10 surface.
    if !palette::is_windows10() {
        eprintln!(
            "mde security: Windows Security is a Windows 10-era surface — use the Control Panel \
             security tools in this theme."
        );
        return ExitCode::SUCCESS;
    }
    let r = iced::application(|_: &Security| "Windows Security".to_string(), update, view)
        .window_size(iced::Size::new(540.0, 420.0))
        .resizable(false)
        .theme(|_| palette::iced_theme())
        .font(mde_ui::font::REGULAR_BYTES)
        .font(mde_ui::font::BOLD_BYTES)
        .font(mde_ui::font::PLEX_REGULAR_BYTES)
        .font(mde_ui::font::PLEX_BOLD_BYTES)
        .default_font(mde_ui::font::ui())
        .run_with(|| {
            // The probes shell out (firewall-cmd/mokutil/lsblk/clamscan), so run
            // them off-thread and let the window appear immediately.
            (
                Security { status: None },
                Task::perform(async { Box::new(security_probe::probe()) }, Message::Loaded),
            )
        });
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}

fn update(state: &mut Security, message: Message) -> Task<Message> {
    match message {
        Message::Loaded(s) => state.status = Some(*s),
    }
    Task::none()
}

/// The OK/WARN/RISK glyph + its palette colour (E14.2 STATUS roles).
fn level_mark(level: Level) -> (&'static str, palette::Rgb) {
    match level {
        Level::Ok => ("\u{f058}", palette::STATUS_OK), // check-circle
        Level::Warn => ("\u{f071}", palette::STATUS_WARN), // exclamation-triangle
        Level::Risk => ("\u{f057}", palette::STATUS_RISK), // times-circle
    }
}

/// One status tile card.
fn tile_card<'a>(icon: &'static str, t: &Tile) -> Element<'a, Message> {
    let (mark, mark_role) = level_mark(t.level);
    let head = Row::new()
        .align_y(iced::alignment::Vertical::Center)
        .push(
            text(icon)
                .font(mde_ui::font::NERD)
                .size(metrics::TILE_GLYPH_PX)
                .color(palette::color(palette::WINDOW_TEXT)),
        )
        .push(Space::new(Length::Fill, Length::Shrink))
        .push(
            text(mark)
                .font(mde_ui::font::NERD)
                .size(metrics::BUTTON_GLYPH_PX)
                .color(palette::color(mark_role)),
        );
    container(
        Column::new()
            .spacing(6.0)
            .push(head)
            .push(
                text(t.title.clone())
                    .size(metrics::UI_PX)
                    .color(palette::color(palette::WINDOW_TEXT)),
            )
            .push(
                text(t.status.clone())
                    .size(metrics::BADGE_PX)
                    .color(palette::color(palette::GRAY_TEXT)),
            ),
    )
    .width(Length::Fixed(metrics::SECURITY_TILE))
    .height(Length::Fixed(metrics::SECURITY_TILE))
    .padding(12.0)
    .style(|_| container::Style {
        border: iced::Border {
            color: palette::color(palette::WINDOW_FRAME),
            width: 1.0,
            radius: 2.0.into(),
        },
        ..container::Style::default()
    })
    .into()
}

/// The advisory "App & browser control" tile (E14.9 expands these); no fake
/// control, just real status text (§3).
fn advisory_tile() -> Tile {
    Tile {
        title: "App & browser control".to_string(),
        status: "Reputation-based controls are handled by the browser.".to_string(),
        level: Level::Ok,
    }
}

fn view(state: &Security) -> Element<'_, Message> {
    let heading = text("Security at a glance")
        .size(metrics::INFO_TITLE_PX)
        .color(palette::color(palette::WINDOW_TEXT));

    let Some(s) = &state.status else {
        return column![
            heading,
            text("Checking your device's security…")
                .size(metrics::UI_PX)
                .color(palette::color(palette::GRAY_TEXT)),
        ]
        .spacing(12.0)
        .padding(16.0)
        .into();
    };

    // The 6 home tiles: the five probed checks + one advisory, each with an icon.
    let advisory = advisory_tile();
    let tiles: [(&'static str, &Tile); 6] = [
        ("\u{f188}", &s.antivirus),  // bug — Virus & threat
        ("\u{f132}", &s.firewall),   // shield — Firewall & network
        ("\u{f0ac}", &advisory),     // globe — App & browser control
        ("\u{f023}", &s.encryption), // lock — Device encryption
        ("\u{f084}", &s.secureboot), // key — Secure Boot
        ("\u{f2db}", &s.tpm),        // microchip — TPM
    ];

    let mut grid = Column::new().spacing(12.0);
    for chunk in tiles.chunks(3) {
        let mut r = Row::new().spacing(12.0);
        for (icon, t) in chunk {
            r = r.push(tile_card(icon, t));
        }
        grid = grid.push(r);
    }

    Column::new()
        .spacing(14.0)
        .padding(Padding::from(16.0))
        .push(heading)
        .push(grid)
        .into()
}
