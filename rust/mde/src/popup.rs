//! Context-menu popups for the taskbar and the Start button.
//!
//! The taskbar's own layer-shell surface is only ~28px tall, so it can't host a
//! menu above itself. Instead a right-click on the bar spawns this: a separate
//! full-screen, transparent layer-shell overlay (exactly like the Start menu)
//! that draws a small context menu bottom-left, above the taskbar, and closes on
//! a click outside, Esc, or choosing an item.
//!
//!   mde popup taskbar   Tile / Minimize all / Task Manager / Properties
//!   mde popup start     Open / Search / Properties (the Win2000 Start menu's
//!                       own right-click menu)

use std::process::{exit, Command, ExitCode};

use iced::widget::{button, container, mouse_area, Column, Row, Space};
use iced::{
    event, keyboard, Background, Border, Color, Element, Event, Length, Padding, Shadow, Task,
};
use iced_layershell::build_pattern::{application, MainSettings};
use iced_layershell::reexport::{Anchor, KeyboardInteractivity};
use iced_layershell::settings::LayerShellSettings;
use iced_layershell::{to_layer_message, Appearance};

use mde_ui::{frame, metrics, palette};

/// One menu entry: a label and the shell command it runs (empty command = a
/// separator).
struct Item {
    label: &'static str,
    command: String,
}

fn sep() -> Item {
    Item {
        label: "",
        command: String::new(),
    }
}

struct Popup {
    items: Vec<Item>,
}

#[to_layer_message]
#[derive(Debug, Clone)]
enum Message {
    Click(usize),
    Close,
    Event(Event),
}

/// `mde` path, for the items that launch our own subcommands.
fn mde() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_else(|| "mde".to_string())
}

fn items_for(kind: &str) -> Vec<Item> {
    let mde = mde();
    match kind {
        // Win10 "power-user" menu (Win+X): a flat, separator-grouped single
        // column of admin shortcuts, each mapped to a concrete backend. System
        // facts + Device Manager reuse mde's own surfaces; the rest shell out to
        // the matching system tool (best-effort `||` chains where a GUI tool is
        // optional, so the item still works on a minimal install).
        "quickaccess" => vec![
            Item {
                label: "System",
                command: format!("'{mde}' system-properties --info"),
            },
            Item {
                label: "Device Manager",
                command: format!("'{mde}' system-properties --devices"),
            },
            Item {
                label: "Disk Management",
                command: "sh -c 'gnome-disks || gparted || blivet-gui'".into(),
            },
            Item {
                label: "Network Connections",
                command: "foot -e sh -c 'nmtui || nmcli device'".into(),
            },
            sep(),
            Item {
                label: "Event Viewer",
                command: "foot -e sh -c 'journalctl -e || journalctl'".into(),
            },
            Item {
                label: "Task Manager",
                command: "foot -o font=monospace:size=12 sh -c 'btop || htop || top'".into(),
            },
            sep(),
            Item {
                label: "Terminal",
                command: "foot".into(),
            },
            Item {
                label: "Terminal (Admin)",
                command: "foot -e sh -c 'pkexec bash || sudo -i'".into(),
            },
            Item {
                label: "Power Options",
                command: format!("'{mde}' shutdown"),
            },
            sep(),
            Item {
                label: "Run\u{2026}",
                command: format!("'{mde}' run"),
            },
        ],
        "start" => vec![
            Item {
                label: "Open",
                command: format!("'{mde}' files"),
            },
            Item {
                label: "Search\u{2026}",
                command: format!("'{mde}' files \"$HOME\""),
            },
            sep(),
            Item {
                label: "Properties",
                command: format!("'{mde}' taskbar-properties"),
            },
        ],
        // Desktop right-click: Refresh / New Folder / (era-routed settings).
        // (labwc also serves its own root-menu; this is the panel-driven one.)
        // Win10 routes the last entry to "Personalize" -> Settings; Carbon /
        // Win2000 keep "Properties" -> Display Properties (E7.10).
        "desktop" => {
            let mut v = vec![
                Item {
                    label: "Refresh",
                    command: "labwc --reconfigure".into(),
                },
                sep(),
                Item {
                    label: "New Folder",
                    command: format!("'{mde}' files \"$HOME/Desktop\""),
                },
                sep(),
            ];
            v.push(if mde_ui::palette::is_windows10() {
                Item {
                    label: "Personalize",
                    command: format!("'{mde}' settings personalization"),
                }
            } else {
                Item {
                    label: "Properties",
                    command: format!("'{mde}' display"),
                }
            });
            v
        }
        // Taskbar empty-area menu. Per-window Restore/Min/Max/Close now live on
        // the labwc titlebar + its right-click client-menu, so this keeps only
        // the global actions. Win10 routes the settings entry to the Settings
        // Taskbar page; other eras keep Taskbar & Start Menu Properties (E7.10).
        _ => {
            let mut v = vec![
                Item {
                    label: "Task Manager",
                    command: "foot -o font=monospace:size=12 sh -c 'btop || htop || top'".into(),
                },
                sep(),
            ];
            v.push(if mde_ui::palette::is_windows10() {
                Item {
                    label: "Taskbar settings",
                    command: format!("'{mde}' settings personalization --page taskbar"),
                }
            } else {
                Item {
                    label: "Properties",
                    command: format!("'{mde}' taskbar-properties"),
                }
            });
            v
        }
    }
}

pub fn run(args: &[String]) -> ExitCode {
    // No compositor → nothing to anchor to; exit cleanly rather than panic in
    // the layer-shell init (popups are normally spawned from labwc/the panel).
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return ExitCode::SUCCESS;
    }
    let kind = args
        .first()
        .map(String::as_str)
        .unwrap_or("taskbar")
        .to_string();
    match launch(kind) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mde popup: {e}");
            ExitCode::FAILURE
        }
    }
}

fn launch(kind: String) -> Result<(), iced_layershell::Error> {
    application(namespace, update, view)
        .style(style)
        // Keyboard-only: the popup ignores mouse events in update, so filtering
        // avoids a view rebuild on every mouse motion over the overlay.
        .subscription(|_: &Popup| {
            event::listen_with(|event, _status, _window| match event {
                iced::Event::Keyboard(_) => Some(Message::Event(event)),
                _ => None,
            })
        })
        .font(mde_ui::font::REGULAR_BYTES)
        .font(mde_ui::font::BOLD_BYTES)
        .font(mde_ui::font::PLEX_REGULAR_BYTES)
        .font(mde_ui::font::PLEX_BOLD_BYTES)
        .default_font(mde_ui::font::ui())
        .settings(MainSettings {
            layer_settings: LayerShellSettings {
                anchor: Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right,
                exclusive_zone: 0,
                // Exclusive so the freshly-mapped overlay is focused on map and
                // its first click is delivered (OnDemand eats it to focus). See
                // the note in menu.rs.
                keyboard_interactivity: KeyboardInteractivity::Exclusive,
                ..Default::default()
            },
            ..Default::default()
        })
        .run_with(move || {
            (
                Popup {
                    items: items_for(&kind),
                },
                Task::none(),
            )
        })
}

fn namespace(_: &Popup) -> String {
    "mde-popup".to_string()
}

fn style(_: &Popup, _: &iced::Theme) -> Appearance {
    Appearance {
        background_color: Color::TRANSPARENT,
        text_color: palette::color(palette::MENU_TEXT),
    }
}

fn update(state: &mut Popup, message: Message) -> Task<Message> {
    match message {
        Message::Click(i) => {
            if let Some(item) = state.items.get(i) {
                if !item.command.is_empty() {
                    let _ = Command::new("sh").arg("-c").arg(&item.command).spawn();
                }
            }
            exit(0)
        }
        Message::Close => exit(0),
        Message::Event(Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::Escape),
            ..
        })) => exit(0),
        _ => Task::none(),
    }
}

fn pad(t: f32, r: f32, b: f32, l: f32) -> Padding {
    Padding {
        top: t,
        right: r,
        bottom: b,
        left: l,
    }
}

fn row_style(_t: &iced::Theme, status: button::Status) -> button::Style {
    let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: hot.then(|| Background::Color(palette::color(palette::HIGHLIGHT))),
        text_color: if hot {
            palette::color(palette::HIGHLIGHT_TEXT)
        } else {
            palette::color(palette::MENU_TEXT)
        },
        border: Border::default(),
        shadow: Shadow::default(),
    }
}

fn view(state: &Popup) -> Element<'_, Message> {
    let mut col = Column::new().spacing(0.0);
    for (i, item) in state.items.iter().enumerate() {
        if item.command.is_empty() && item.label.is_empty() {
            col = col.push(
                container(Space::new(Length::Fill, Length::Fixed(5.0)))
                    .padding(pad(2.0, 6.0, 2.0, 6.0)),
            );
        } else {
            col = col.push(
                button(iced::widget::text(item.label).size(metrics::UI_PX))
                    .on_press(Message::Click(i))
                    .width(Length::Fill)
                    .padding(pad(3.0, 24.0, 3.0, 12.0))
                    .style(row_style),
            );
        }
    }
    // Fixed height to the content, so the raised frame wraps the items instead
    // of stretching to fill (frame::raised() defaults to Length::Fill).
    let h: f32 = state
        .items
        .iter()
        .map(|it| {
            if it.command.is_empty() && it.label.is_empty() {
                9.0
            } else {
                22.0
            }
        })
        .sum::<f32>()
        + 6.0;
    let menu = container(iced::widget::stack![
        frame::raised(),
        container(col).padding(2.0)
    ])
    .width(Length::Fixed(220.0))
    .height(Length::Fixed(h));

    // Bottom-left; a full-screen catcher closes it. The overlay surface is
    // already clipped above the taskbar's exclusive zone, so the menu only needs
    // a 2px lift to rest on the bar (not a second TASKBAR_HEIGHT offset).
    let positioned = Column::new()
        .push(Space::with_height(Length::Fill))
        .push(Row::new().push(menu).push(Space::with_width(Length::Fill)))
        .push(Space::with_height(Length::Fixed(2.0)));

    mouse_area(container(positioned).padding(pad(0.0, 0.0, 0.0, 2.0)))
        .on_press(Message::Close)
        .on_right_press(Message::Close)
        .into()
}
