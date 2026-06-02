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
    label: String,
    command: String,
}

fn sep() -> Item {
    Item {
        label: "".into(),
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
                label: "System".into(),
                command: format!("'{mde}' system-properties --info"),
            },
            Item {
                label: "Device Manager".into(),
                command: format!("'{mde}' system-properties --devices"),
            },
            Item {
                label: "Disk Management".into(),
                command: "sh -c 'gnome-disks || gparted || blivet-gui'".into(),
            },
            Item {
                label: "Network Connections".into(),
                command: "foot -e sh -c 'nmtui || nmcli device'".into(),
            },
            sep(),
            Item {
                label: "Event Viewer".into(),
                command: "foot -e sh -c 'journalctl -e || journalctl'".into(),
            },
            Item {
                label: "Task Manager".into(),
                command: "foot -o font=monospace:size=12 sh -c 'btop || htop || top'".into(),
            },
            sep(),
            Item {
                label: "Terminal".into(),
                command: "foot".into(),
            },
            Item {
                label: "Terminal (Admin)".into(),
                command: "foot -e sh -c 'pkexec bash || sudo -i'".into(),
            },
            Item {
                label: "Power Options".into(),
                command: format!("'{mde}' shutdown"),
            },
            sep(),
            Item {
                label: "Run\u{2026}".into(),
                command: format!("'{mde}' run"),
            },
        ],
        "start" => vec![
            Item {
                label: "Open".into(),
                command: format!("'{mde}' files"),
            },
            Item {
                label: "Search\u{2026}".into(),
                command: format!("'{mde}' files \"$HOME\""),
            },
            sep(),
            Item {
                label: "Properties".into(),
                command: format!("'{mde}' taskbar-properties"),
            },
        ],
        // Taskbar empty-area menu. Per-window Restore/Min/Max/Close now live on
        // the labwc titlebar + its right-click client-menu, so this keeps only
        // the global actions. Win10 routes the settings entry to the Settings
        // Taskbar page; other eras keep Taskbar & Start Menu Properties (E7.10).
        _ => {
            let mut v = vec![
                Item {
                    label: "Task Manager".into(),
                    command: "foot -o font=monospace:size=12 sh -c 'btop || htop || top'".into(),
                },
                sep(),
            ];
            v.push(if mde_ui::palette::is_windows10() {
                Item {
                    label: "Taskbar settings".into(),
                    command: format!("'{mde}' settings personalization --page taskbar"),
                }
            } else {
                Item {
                    label: "Properties".into(),
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

/// `mde jumplist <app_id>` — the Win10 taskbar jump list: app-specific Tasks
/// plus a Recent-files section, in the same bottom-left popup (E2.6).
pub fn run_jumplist(args: &[String]) -> ExitCode {
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return ExitCode::SUCCESS;
    }
    let app_id = args.first().cloned().unwrap_or_default();
    match launch_with(items_for_jumplist(&app_id)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mde jumplist: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Jump-list entries for `app_id`: app-specific Tasks (Firefox new/private
/// window), then the system Recent files, then Open. Tasks the app doesn't
/// define are simply absent — no dead rows (§3).
fn items_for_jumplist(app_id: &str) -> Vec<Item> {
    let mut v = Vec::new();
    let low = app_id.to_lowercase();
    if low.contains("firefox") {
        v.push(Item {
            label: "New Window".into(),
            command: "firefox --new-window".into(),
        });
        v.push(Item {
            label: "New Private Window".into(),
            command: "firefox --private-window".into(),
        });
        v.push(sep());
    }
    let recents = recent_files(6);
    if !recents.is_empty() {
        v.extend(recents);
        v.push(sep());
    }
    if !app_id.is_empty() {
        // Open a fresh instance (best-effort: the app_id is usually the exec).
        v.push(Item {
            label: format!("Open {app_id}"),
            command: shell_quote(app_id),
        });
    }
    if v.is_empty() {
        v.push(Item {
            label: "(no recent items)".into(),
            command: String::new(),
        });
    }
    v
}

/// The system's recent files (`~/.local/share/recently-used.xbel`), newest
/// first, capped at `n`; each opens via `xdg-open`. Empty if the file is absent.
fn recent_files(n: usize) -> Vec<Item> {
    let Some(home) = std::env::var_os("HOME") else {
        return Vec::new();
    };
    let path = std::path::PathBuf::from(home).join(".local/share/recently-used.xbel");
    let Ok(xml) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let mut paths: Vec<String> = Vec::new();
    let mut rest = xml.as_str();
    while let Some(i) = rest.find("href=\"file://") {
        rest = &rest[i + "href=\"file://".len()..];
        if let Some(end) = rest.find('"') {
            paths.push(percent_decode(&rest[..end]));
            rest = &rest[end..];
        } else {
            break;
        }
    }
    paths.reverse(); // xbel appends, so the tail is newest
    paths.dedup();
    paths
        .into_iter()
        .filter(|p| std::path::Path::new(p).exists())
        .take(n)
        .filter_map(|p| {
            let name = std::path::Path::new(&p)
                .file_name()?
                .to_string_lossy()
                .into_owned();
            Some(Item {
                label: name,
                command: format!("xdg-open {}", shell_quote(&p)),
            })
        })
        .collect()
}

/// Decode `%XX` percent-escapes in a recently-used URI path.
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(byte) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Single-quote a path for embedding in a shell command (handles spaces/quotes).
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn launch(kind: String) -> Result<(), iced_layershell::Error> {
    launch_with(items_for(&kind))
}

fn launch_with(items: Vec<Item>) -> Result<(), iced_layershell::Error> {
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
        .run_with(move || (Popup { items }, Task::none()))
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
                button(iced::widget::text(item.label.clone()).size(metrics::UI_PX))
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
