//! The File-History restore browser — `mde settings backup --restore` (E17.8).
//!
//! A `files.rs`-style browser over the **Timeshift** snapshots (`timeshift --list`),
//! with bottom **Older/Newer** time-navigation, a **Details / Large icons** view
//! toggle, and a green **"Restore to original location"** button (the `STATUS_OK`
//! role — the MackesDE rebrand collapsed the planned `RESTORE_PRIMARY` onto the
//! existing success-green, see E17.1) behind an inline replace-confirm that runs
//! `pkexec timeshift --restore`.
//!
//! §6 reconciliations: Timeshift restores the **whole system** from a point in
//! time, not individual files, so the browsable items are the snapshots
//! (restore points) and "restore" is single-snapshot (not file multi-select). The
//! Options-gear "Restore to…" (filedialog) + multi-select are split to **E17.8a**.
//! `timeshift --list` needs root, so an unprivileged session shows an empty list +
//! a privileged-refresh hint; `MDE_TIMESHIFT_FIXTURE` populates it for captures.

use std::process::{exit, ExitCode};

use iced::widget::{button, column, container, scrollable, text, Column, Row};
use iced::{event, Element, Length, Subscription, Task};

use mde_ui::{metrics, palette};

use crate::sysinfo::Snapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewMode {
    Details,
    Large,
}

struct Restore {
    snaps: Vec<Snapshot>,
    selected: usize,
    view: ViewMode,
    confirm: bool,
}

#[derive(Debug, Clone)]
enum Message {
    Select(usize),
    Older,
    Newer,
    ToggleView,
    AskRestore,
    CancelRestore,
    ConfirmRestore,
    Restored,
    Event(iced::Event),
}

pub fn run(_args: &[String]) -> ExitCode {
    if !palette::is_windows10() {
        eprintln!(
            "mde settings backup --restore: the restore browser is a Windows 10-era surface."
        );
        return ExitCode::SUCCESS;
    }
    match launch() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mde restore: {e}");
            ExitCode::FAILURE
        }
    }
}

fn launch() -> iced::Result {
    iced::application(|_: &Restore| "File History".to_string(), update, view)
        .theme(|_| palette::iced_theme())
        .subscription(|_: &Restore| -> Subscription<Message> {
            event::listen().map(Message::Event)
        })
        .font(mde_ui::font::REGULAR_BYTES)
        .font(mde_ui::font::BOLD_BYTES)
        .font(mde_ui::font::PLEX_REGULAR_BYTES)
        .font(mde_ui::font::PLEX_BOLD_BYTES)
        .default_font(mde_ui::font::ui())
        .run_with(|| {
            (
                Restore {
                    snaps: crate::sysinfo::snapshots(),
                    selected: 0,
                    view: ViewMode::Details,
                    confirm: false,
                },
                Task::none(),
            )
        })
}

fn update(state: &mut Restore, message: Message) -> Task<Message> {
    match message {
        Message::Select(i) => state.selected = i,
        // Newest is index 0; "Older" steps forward in the list, "Newer" back.
        Message::Older => {
            if state.selected + 1 < state.snaps.len() {
                state.selected += 1;
            }
        }
        Message::Newer => state.selected = state.selected.saturating_sub(1),
        Message::ToggleView => {
            state.view = match state.view {
                ViewMode::Details => ViewMode::Large,
                ViewMode::Large => ViewMode::Details,
            }
        }
        Message::AskRestore => state.confirm = true,
        Message::CancelRestore => state.confirm = false,
        Message::ConfirmRestore => {
            state.confirm = false;
            if let Some(s) = state.snaps.get(state.selected) {
                let cmd = crate::sysinfo::timeshift_restore_cmd(&s.name);
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let _ = std::process::Command::new("pkexec").args(&cmd).status();
                        })
                        .await
                        .ok();
                    },
                    |_| Message::Restored,
                );
            }
        }
        Message::Restored => exit(0),
        Message::Event(iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
            ..
        })) => exit(0),
        Message::Event(_) => {}
    }
    Task::none()
}

fn view(state: &Restore) -> Element<'_, Message> {
    let header = text("Restore your files")
        .size(metrics::INFO_TITLE_PX)
        .color(palette::color(palette::WINDOW_TEXT));

    let toggle = button(
        text(match state.view {
            ViewMode::Details => "Large icons",
            ViewMode::Large => "Details",
        })
        .size(metrics::UI_PX),
    )
    .on_press(Message::ToggleView)
    .padding(iced::Padding::from([3.0, 10.0]))
    .style(mde_ui::button_ghost);

    let toolbar = Row::new()
        .spacing(10.0)
        .align_y(iced::alignment::Vertical::Center)
        .push(header)
        .push(iced::widget::horizontal_space())
        .push(toggle);

    // The snapshot list / grid.
    let body: Element<'_, Message> = if state.snaps.is_empty() {
        text("No restore points were found. Set up a backup drive, then create a snapshot.\n(Listing snapshots needs administrator access.)")
            .size(metrics::UI_PX)
            .color(palette::color(palette::GRAY_TEXT))
            .into()
    } else {
        match state.view {
            ViewMode::Details => details_list(state).into(),
            ViewMode::Large => large_grid(state).into(),
        }
    };

    // Bottom bar: Older/Newer time-nav + the restore action (or confirm).
    let nav = Row::new()
        .spacing(8.0)
        .align_y(iced::alignment::Vertical::Center)
        .push(
            button(text("< Older").size(metrics::UI_PX))
                .on_press(Message::Older)
                .padding(iced::Padding::from([4.0, 12.0]))
                .style(mde_ui::button_ghost),
        )
        .push(
            button(text("Newer >").size(metrics::UI_PX))
                .on_press(Message::Newer)
                .padding(iced::Padding::from([4.0, 12.0]))
                .style(mde_ui::button_ghost),
        )
        .push(iced::widget::horizontal_space());

    let action: Element<'_, Message> = if state.snaps.is_empty() {
        iced::widget::horizontal_space().into()
    } else if state.confirm {
        Row::new()
            .spacing(8.0)
            .align_y(iced::alignment::Vertical::Center)
            .push(
                text("Replace files at their original location?")
                    .size(metrics::UI_PX)
                    .color(palette::color(palette::WINDOW_TEXT)),
            )
            .push(
                button(text("Restore").size(metrics::UI_PX))
                    .on_press(Message::ConfirmRestore)
                    .padding(iced::Padding::from([4.0, 14.0]))
                    .style(restore_button),
            )
            .push(
                button(text("Cancel").size(metrics::UI_PX))
                    .on_press(Message::CancelRestore)
                    .padding(iced::Padding::from([4.0, 12.0]))
                    .style(mde_ui::button_ghost),
            )
            .into()
    } else {
        button(
            text("Restore to original location")
                .size(metrics::UI_PX)
                .color(palette::color(palette::HIGHLIGHT_TEXT)),
        )
        .on_press(Message::AskRestore)
        .padding(iced::Padding::from([5.0, 16.0]))
        .style(restore_button)
        .into()
    };

    let bottom = Row::new().push(nav).push(action);

    container(
        column![
            toolbar,
            container(scrollable(body))
                .width(Length::Fill)
                .height(Length::Fill),
            bottom,
        ]
        .spacing(12.0),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .padding(14.0)
    .style(|_: &iced::Theme| container::Style {
        background: Some(palette::color(palette::WINDOW).into()),
        text_color: Some(palette::color(palette::WINDOW_TEXT)),
        ..Default::default()
    })
    .into()
}

/// The green "restore" button style, from the `STATUS_OK` success role (E17.8).
fn restore_button(
    _: &iced::Theme,
    status: iced::widget::button::Status,
) -> iced::widget::button::Style {
    let base = palette::color(palette::STATUS_OK);
    let bg = match status {
        iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed => {
            iced::Color { a: 0.85, ..base }
        }
        _ => base,
    };
    iced::widget::button::Style {
        background: Some(bg.into()),
        text_color: palette::color(palette::HIGHLIGHT_TEXT),
        border: iced::Border {
            radius: 2.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn snap_label(s: &Snapshot) -> String {
    // Turn `2024-01-15_10-00-01` into `2024-01-15 10:00` + any description.
    let pretty = s.name.replacen('_', " ", 1);
    let hm = pretty
        .split_once(' ')
        .map(|(d, t)| {
            let t = t.replacen('-', ":", 2);
            let t = t.rsplit_once(':').map(|(hm, _)| hm).unwrap_or(&t);
            format!("{d} {t}")
        })
        .unwrap_or(pretty);
    if s.desc.is_empty() {
        hm
    } else {
        format!("{hm} — {}", s.desc)
    }
}

fn details_list(state: &Restore) -> Column<'_, Message> {
    let mut col = Column::new().spacing(1.0);
    for (i, s) in state.snaps.iter().enumerate() {
        let sel = i == state.selected;
        let fg = if sel {
            palette::HIGHLIGHT_TEXT
        } else {
            palette::WINDOW_TEXT
        };
        let bg = if sel {
            palette::accent()
        } else {
            palette::color(palette::WINDOW)
        };
        col = col.push(
            button(
                text(snap_label(s))
                    .size(metrics::UI_PX)
                    .color(palette::color(fg)),
            )
            .on_press(Message::Select(i))
            .width(Length::Fill)
            .padding(iced::Padding::from([5.0, 8.0]))
            .style(move |_: &iced::Theme, _| iced::widget::button::Style {
                background: Some(bg.into()),
                text_color: palette::color(fg),
                ..Default::default()
            }),
        );
    }
    col
}

fn large_grid(state: &Restore) -> Column<'_, Message> {
    // A simple wrapped grid of date tiles.
    let mut col = Column::new().spacing(8.0);
    let mut r = Row::new().spacing(8.0);
    for (i, s) in state.snaps.iter().enumerate() {
        let sel = i == state.selected;
        let bg = if sel {
            palette::accent()
        } else {
            palette::color(palette::MENU)
        };
        let fg = if sel {
            palette::HIGHLIGHT_TEXT
        } else {
            palette::WINDOW_TEXT
        };
        let date = s.name.split('_').next().unwrap_or(&s.name).to_string();
        let sub = if s.desc.is_empty() {
            "snapshot".to_string()
        } else {
            s.desc.clone()
        };
        r = r.push(
            button(
                column![
                    text(date).size(metrics::UI_PX).color(palette::color(fg)),
                    text(sub).size(metrics::BADGE_PX).color(palette::color(fg)),
                ]
                .spacing(4.0)
                .align_x(iced::alignment::Horizontal::Center),
            )
            .on_press(Message::Select(i))
            .width(Length::Fixed(140.0))
            .padding(10.0)
            .style(move |_: &iced::Theme, _| iced::widget::button::Style {
                background: Some(bg.into()),
                text_color: palette::color(fg),
                border: iced::Border {
                    radius: 2.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
        );
        if (i + 1) % 4 == 0 {
            col = col.push(r);
            r = Row::new().spacing(8.0);
        }
    }
    col.push(r)
}
