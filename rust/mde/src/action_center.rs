//! The Windows 10 Action Center pane (E3) — a right-anchored, full-height
//! layer-shell surface that shows the notification history grouped by app, read
//! from the `notifyd` mirror (`~/.config/mde/notifications.json`). Clearing a
//! card (or a whole group) calls the standard `CloseNotification` on the daemon.
//!
//!   mde action-center   open the slide-in pane (Win10 era; WINKEY+A)
//!
//! The quick-action tile grid (E3.5) is live here; some tile-action backends
//! (E3.6) and inline notification actions (which need a daemon action bridge)
//! layer on in later stories.

use std::process::{exit, ExitCode};
use std::time::{Duration, SystemTime};

use iced::alignment::{Horizontal, Vertical};
use iced::widget::{container, mouse_area, row, scrollable, slider, text, Column, Row, Space};
use iced::{
    event, keyboard, Background, Border, Color, Element, Event, Length, Padding, Shadow, Task,
};
use iced_layershell::build_pattern::{application, MainSettings};
use iced_layershell::reexport::{Anchor, KeyboardInteractivity};
use iced_layershell::settings::LayerShellSettings;
use iced_layershell::{to_layer_message, Appearance};

use mde_ui::{metrics, palette};

use crate::notifyd::{self, Notif};

const PANE_W: f32 = 360.0;
const TILE: f32 = 78.0; // square quick-action tile
const COLLAPSED: usize = 4; // first four tiles show collapsed

struct Center {
    notes: Vec<Notif>,
    /// Quick-action tiles in order: (id, currently-on).
    tiles: Vec<(String, bool)>,
    /// Current backlight brightness 0–100, or None when there's no backlight
    /// (`brightnessctl` absent/headless) — the slider is hidden then.
    brightness: Option<u8>,
    /// Whether the quick-action grid is expanded past the first four.
    expanded: bool,
}

#[to_layer_message]
#[derive(Debug, Clone)]
enum Message {
    Clear(u32),         // dismiss one notification
    ClearGroup(String), // dismiss every notification from one app
    ClearAll,           // dismiss every notification
    ToggleTile(String), // flip a quick-action tile + its backend
    ToggleExpand,       // show/hide the tiles past the first four
    Brightness(u8),     // drag the backlight slider (visual only; applied on release)
    BrightnessSet,      // slider released → apply brightnessctl once (E3.6c)
    AllSettings,        // the foot "All settings" link → mde settings, then close
    Close,
    Event(Event),
}

pub fn run_center(_args: &[String]) -> ExitCode {
    // No compositor → nothing to anchor the layer-shell pane to; exit cleanly
    // rather than panic in init (matches search/task-view/project). Guard before
    // the read-stamp so a headless invocation (e.g. the E20.7 no-panic sweep) is a
    // pure no-op and doesn't clear the badge.
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return ExitCode::SUCCESS;
    }
    // Opening the center marks everything read, so the panel badge clears (E3.9).
    notifyd::stamp_last_read();
    match launch() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mde action-center: {e}");
            ExitCode::FAILURE
        }
    }
}

fn launch() -> Result<(), iced_layershell::Error> {
    application(namespace, update, view)
        .style(style)
        .subscription(|_: &Center| {
            event::listen_with(|event, _status, _window| match event {
                Event::Keyboard(_) => Some(Message::Event(event)),
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
                // Full-screen transparent catcher; the pane hugs the right edge.
                anchor: Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right,
                exclusive_zone: 0,
                keyboard_interactivity: KeyboardInteractivity::Exclusive,
                ..Default::default()
            },
            ..Default::default()
        })
        .run_with(|| {
            let tiles = crate::state::load()
                .quick_actions
                .into_iter()
                .filter(|id| tile_label(id).is_some()) // only ids we implement
                .map(|id| {
                    let on = read_tile(&id);
                    (id, on)
                })
                .collect();
            (
                Center {
                    notes: notifyd::load_file().notifications,
                    tiles,
                    brightness: read_brightness(),
                    expanded: false,
                },
                Task::none(),
            )
        })
}

fn namespace(_: &Center) -> String {
    "mde-action-center".to_string()
}

fn style(_: &Center, _: &iced::Theme) -> Appearance {
    Appearance {
        background_color: Color::TRANSPARENT,
        text_color: palette::color(palette::WINDOW_TEXT),
    }
}

fn update(state: &mut Center, message: Message) -> Task<Message> {
    match message {
        Message::Clear(id) => {
            dbus_close(id);
            state.notes.retain(|n| n.id != id);
        }
        Message::ClearGroup(app) => {
            for n in state.notes.iter().filter(|n| n.app_name == app) {
                dbus_close(n.id);
            }
            state.notes.retain(|n| n.app_name != app);
        }
        Message::ClearAll => {
            for n in &state.notes {
                dbus_close(n.id);
            }
            state.notes.clear();
        }
        Message::ToggleTile(id) => {
            if let Some((_, on)) = state.tiles.iter_mut().find(|(t, _)| *t == id) {
                toggle_tile(&id, *on);
                // Re-read the backend so the fill matches reality (some toggles
                // settle async, but the read is best-effort + cheap).
                *on = read_tile(&id);
            }
        }
        Message::ToggleExpand => state.expanded = !state.expanded,
        Message::Brightness(v) => {
            // Track the handle live, but DON'T spawn per drag-tick (E3.6c): the
            // slider fires this on every pixel of motion, and each
            // `brightnessctl` spawn was a `let _`-dropped, unreaped child — a
            // single end-to-end drag forked dozens of zombies. Apply once on
            // release instead (`BrightnessSet`).
            state.brightness = Some(v);
        }
        Message::BrightnessSet => {
            if let Some(v) = state.brightness {
                set_brightness(v);
            }
        }
        Message::AllSettings => {
            launch_mde("settings");
            exit(0);
        }
        Message::Close => exit(0),
        Message::Event(Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::Escape),
            ..
        })) => exit(0),
        _ => {}
    }
    Task::none()
}

/// Run a shell command and capture its stdout (for reading a tile's state).
fn sh(cmd: &str) -> String {
    std::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default()
}

/// Spawn a shell command detached (for a tile's toggle action).
fn run(cmd: &str) {
    let _ = std::process::Command::new("sh").arg("-c").arg(cmd).spawn();
}

/// Apply the backlight level once, when the slider is released (E3.6c). Runs
/// `brightnessctl` directly (no shell) and `.status()`-waits so the child is
/// reaped — `brightnessctl` writes a sysfs file and exits in milliseconds, so
/// the wait is negligible and a drag never leaves defunct processes behind.
fn set_brightness(v: u8) {
    let _ = std::process::Command::new("brightnessctl")
        .arg("set")
        .arg(format!("{v}%"))
        .status();
}

/// Read the current backlight as a 0–100 percentage, or None when there's no
/// backlight (`brightnessctl` absent, or a headless/desktop box) — no slider then.
fn read_brightness() -> Option<u8> {
    // `brightnessctl -m` → CSV "device,class,current,percent,max"; field 4 is "NN%".
    let pct = sh("brightnessctl -m");
    pct.split(',')
        .nth(3)?
        .trim()
        .trim_end_matches('%')
        .parse()
        .ok()
}

/// Launch an `mde <sub>` window from a foot link, using the running binary.
fn launch_mde(sub: &str) {
    let exe = std::env::current_exe().unwrap_or_else(|_| "mde".into());
    let _ = std::process::Command::new(exe).arg(sub).spawn();
}

/// The display label + Nerd glyph for a quick-action tile id, or None if the id
/// isn't one we implement (so unknown ids in `quick_actions` are skipped).
fn tile_label(id: &str) -> Option<(&'static str, &'static str)> {
    Some(match id {
        "wifi" => ("Wi-Fi", "\u{f1eb}"),             // fa-wifi
        "bluetooth" => ("Bluetooth", "\u{f293}"),    // fa-bluetooth
        "airplane" => ("Airplane", "\u{f072}"),      // fa-plane
        "mute" => ("Mute", "\u{f6a9}"),              // fa-volume-mute
        "focus" => ("Focus assist", "\u{f1f6}"),     // fa-bell-slash (Do Not Disturb)
        "nightlight" => ("Night light", "\u{f186}"), // fa-moon
        _ => return None,
    })
}

/// Read a quick-action tile's on/off state from its Linux backend.
fn read_tile(id: &str) -> bool {
    match id {
        "wifi" => sh("nmcli -t radio wifi").trim() == "enabled",
        "bluetooth" => !sh("rfkill list bluetooth").contains("Soft blocked: yes"),
        // Airplane = every radio soft-blocked (no device left unblocked).
        "airplane" => {
            let l = sh("rfkill list");
            !l.contains("Soft blocked: no") && l.contains("Soft blocked")
        }
        "mute" => sh("wpctl get-volume @DEFAULT_AUDIO_SINK@").contains("MUTED"),
        // Focus assist is shell-free: it's our own persisted state (E3.7).
        "focus" => crate::state::load().focus_assist,
        "nightlight" => !sh("pgrep -x wlsunset").trim().is_empty(),
        _ => false,
    }
}

/// Toggle a quick-action tile's backend given its current state.
fn toggle_tile(id: &str, on: bool) {
    match id {
        "wifi" => run(if on {
            "nmcli radio wifi off"
        } else {
            "nmcli radio wifi on"
        }),
        "bluetooth" => run(if on {
            "rfkill block bluetooth"
        } else {
            "rfkill unblock bluetooth"
        }),
        "airplane" => run(if on {
            "rfkill unblock all"
        } else {
            "rfkill block all"
        }),
        "mute" => run("wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle"),
        // Focus assist flips our persisted state (notifyd reads it per Notify).
        "focus" => {
            let mut st = crate::state::load();
            st.focus_assist = !on;
            let _ = crate::state::save(&st);
        }
        "nightlight" => run(if on { "pkill -x wlsunset" } else { "wlsunset" }),
        _ => {}
    }
}

/// Dismiss a notification via the standard freedesktop `CloseNotification` on
/// whatever daemon owns the name (our `notifyd` in the Win10 era).
fn dbus_close(id: u32) {
    if let Ok(conn) = zbus::blocking::Connection::session() {
        if let Ok(proxy) = zbus::blocking::Proxy::new(
            &conn,
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            "org.freedesktop.Notifications",
        ) {
            let _ = proxy.call::<_, _, ()>("CloseNotification", &(id,));
        }
    }
}

// --- view --------------------------------------------------------------------

fn pad(t: f32, r: f32, b: f32, l: f32) -> Padding {
    Padding {
        top: t,
        right: r,
        bottom: b,
        left: l,
    }
}

fn clear_x(msg: Message) -> Element<'static, Message> {
    mouse_area(
        container(
            text("\u{f00d}") // fa-times (×)
                .size(metrics::UI_PX)
                .font(mde_ui::font::NERD)
                .color(palette::color(palette::GRAY_TEXT)),
        )
        .padding(pad(2.0, 6.0, 2.0, 6.0)),
    )
    .on_press(msg)
    .into()
}

fn view(state: &Center) -> Element<'_, Message> {
    let body: Element<Message> = if state.notes.is_empty() {
        container(
            text("No new notifications")
                .size(metrics::UI_PX)
                .color(palette::color(palette::GRAY_TEXT)),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
    } else {
        let mut col = Column::new().spacing(8.0).width(Length::Fill);
        // Group by app, preserving first-seen order; newest cards first within.
        let mut groups: Vec<(String, Vec<&Notif>)> = Vec::new();
        for n in state.notes.iter().rev() {
            match groups.iter_mut().find(|(a, _)| *a == n.app_name) {
                Some((_, v)) => v.push(n),
                None => groups.push((n.app_name.clone(), vec![n])),
            }
        }
        for (app, notes) in groups {
            // Group header: app icon + name + a group Clear "x".
            let label: String = if app.is_empty() {
                "Notifications".into()
            } else {
                app.clone()
            };
            col = col.push(
                Row::new()
                    .align_y(Vertical::Center)
                    .push(crate::icons::icon_any(
                        &[app.to_lowercase().as_str(), "dialog-information"],
                        16,
                    ))
                    .push(Space::with_width(Length::Fixed(6.0)))
                    .push(
                        text(label)
                            .size(metrics::UI_PX)
                            .font(mde_ui::font::ui_bold())
                            .width(Length::Fill),
                    )
                    .push(clear_x(Message::ClearGroup(app.clone()))),
            );
            for n in notes {
                col = col.push(card(n));
            }
        }
        // "Clear all" at the bottom of the history region.
        col = col.push(
            container(
                mouse_area(
                    text("Clear all")
                        .size(metrics::UI_PX)
                        .color(palette::accent()),
                )
                .on_press(Message::ClearAll),
            )
            .width(Length::Fill)
            .align_x(Horizontal::Right)
            .padding(pad(2.0, 4.0, 0.0, 0.0)),
        );
        scrollable(col).style(mde_ui::scrollbar).into()
    };

    // The flat dark pane: history (fills) over the quick-action tile grid.
    let pane = container(
        Column::new()
            .padding(10.0)
            .spacing(10.0)
            .push(container(body).height(Length::Fill))
            .push(tile_grid(state))
            .push(brightness_slider(state))
            .push(all_settings_link()),
    )
    .width(Length::Fixed(PANE_W))
    .height(Length::Fill)
    .style(|_| container::Style {
        background: Some(Background::Color(palette::color(palette::MENU))),
        border: Border {
            color: palette::color(palette::WINDOW_FRAME),
            width: 1.0,
            radius: 0.0.into(),
        },
        shadow: Shadow {
            color: Color {
                a: 0.35,
                ..Color::BLACK
            },
            offset: iced::Vector::new(-2.0, 0.0),
            blur_radius: 12.0,
        },
        ..container::Style::default()
    });

    iced::widget::stack![
        mouse_area(Space::new(Length::Fill, Length::Fill)).on_press(Message::Close),
        container(pane)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Horizontal::Right)
            .align_y(Vertical::Top),
    ]
    .into()
}

/// One notification card: icon + summary (bold) + body + relative time, with a
/// per-card Clear "x".
fn card(n: &Notif) -> Element<'static, Message> {
    let head = Row::new()
        .align_y(Vertical::Center)
        .push(
            text(n.summary.clone())
                .size(metrics::UI_PX)
                .font(mde_ui::font::ui_bold())
                .width(Length::Fill),
        )
        .push(
            text(rel_time(n.timestamp))
                .size(metrics::UI_PX)
                .color(palette::color(palette::GRAY_TEXT)),
        )
        .push(clear_x(Message::Clear(n.id)));
    let mut inner = Column::new().spacing(2.0).width(Length::Fill).push(head);
    if !n.body.is_empty() {
        inner = inner.push(
            text(n.body.clone())
                .size(metrics::UI_PX)
                .color(palette::color(palette::WINDOW_TEXT)),
        );
    }
    let icon = crate::icons::icon_any(&[n.app_icon.as_str(), "dialog-information"], 24);
    container(row![icon, inner].spacing(8.0).align_y(Vertical::Top))
        .width(Length::Fill)
        .padding(8.0)
        .style(|_| container::Style {
            background: Some(Background::Color(palette::color(palette::WINDOW))),
            border: Border {
                color: palette::color(palette::WINDOW_FRAME),
                width: 1.0,
                radius: 2.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

/// A coarse "Nm ago" / "Nh ago" / "now" relative timestamp.
fn rel_time(t: SystemTime) -> String {
    match SystemTime::now().duration_since(t) {
        Ok(d) => {
            let s = d.as_secs();
            if s < 60 {
                "now".to_string()
            } else if s < 3600 {
                format!("{}m ago", s / 60)
            } else if s < 86_400 {
                format!("{}h ago", s / 3600)
            } else {
                format!("{}d ago", s / 86_400)
            }
        }
        Err(_) => "now".to_string(),
    }
}

/// The Win10 brightness slider, shown below the tile grid only when there's a
/// backlight. The handle tracks the cursor live; the level is applied once on
/// release (`brightnessctl set N%`) so a drag doesn't fork a process per tick
/// (E3.6c). It's the non-toggle quick-action the square tiles can't express (E3.6a).
fn brightness_slider(state: &Center) -> Element<'_, Message> {
    let Some(v) = state.brightness else {
        return Space::new(Length::Shrink, Length::Shrink).into();
    };
    Row::new()
        .spacing(8.0)
        .align_y(Vertical::Center)
        .push(
            text("\u{f185}") // fa-sun
                .size(metrics::BUTTON_GLYPH_PX)
                .font(mde_ui::font::NERD)
                .color(palette::color(palette::WINDOW_TEXT)),
        )
        .push(
            slider(0..=100u8, v, Message::Brightness)
                .on_release(Message::BrightnessSet)
                .width(Length::Fill)
                .style(|_, _| slider::Style {
                    rail: slider::Rail {
                        backgrounds: (
                            Background::Color(palette::accent()),
                            Background::Color(palette::color(palette::WINDOW_FRAME)),
                        ),
                        width: 4.0,
                        border: Border {
                            color: palette::color(palette::WINDOW_FRAME),
                            width: 0.0,
                            radius: 2.0.into(),
                        },
                    },
                    handle: slider::Handle {
                        shape: slider::HandleShape::Circle { radius: 7.0 },
                        background: Background::Color(palette::accent()),
                        border_width: 1.0,
                        border_color: palette::color(palette::WINDOW_FRAME),
                    },
                }),
        )
        .into()
}

/// The Win10 full-width "All settings" link at the foot of the pane: launches
/// `mde settings` and closes the center. The square grid holds toggle/slider
/// quick-actions; this launch affordance sits below them, as in Windows 10 (E3.6a).
fn all_settings_link() -> Element<'static, Message> {
    mouse_area(
        container(
            Row::new()
                .spacing(8.0)
                .align_y(Vertical::Center)
                .push(
                    text("\u{f013}") // fa-gear
                        .size(metrics::BUTTON_GLYPH_PX)
                        .font(mde_ui::font::NERD)
                        .color(palette::color(palette::WINDOW_TEXT)),
                )
                .push(
                    text("All settings")
                        .size(metrics::UI_PX)
                        .color(palette::color(palette::WINDOW_TEXT)),
                ),
        )
        .width(Length::Fill)
        .padding(pad(6.0, 8.0, 6.0, 8.0))
        .style(|_| container::Style {
            background: Some(Background::Color(palette::color(palette::WINDOW))),
            border: Border {
                color: palette::color(palette::WINDOW_FRAME),
                width: 1.0,
                radius: 2.0.into(),
            },
            ..container::Style::default()
        }),
    )
    .on_press(Message::AllSettings)
    .into()
}

/// The quick-action tile grid: the first four tiles, with an Expand link to the
/// rest (Collapse when expanded). Rows of four square toggle tiles.
fn tile_grid(state: &Center) -> Element<'_, Message> {
    if state.tiles.is_empty() {
        return Space::new(Length::Shrink, Length::Shrink).into();
    }
    let shown = if state.expanded {
        state.tiles.len()
    } else {
        state.tiles.len().min(COLLAPSED)
    };
    let mut grid = Column::new().spacing(6.0).width(Length::Fill);
    let mut r = Row::new().spacing(6.0);
    for (i, (id, on)) in state.tiles.iter().take(shown).enumerate() {
        r = r.push(quick_tile(id, *on));
        if (i + 1) % 4 == 0 {
            grid = grid.push(r);
            r = Row::new().spacing(6.0);
        }
    }
    grid = grid.push(r);
    if state.tiles.len() > COLLAPSED {
        let label = if state.expanded { "Collapse" } else { "Expand" };
        grid = grid.push(
            mouse_area(text(label).size(metrics::UI_PX).color(palette::accent()))
                .on_press(Message::ToggleExpand),
        );
    }
    grid.into()
}

/// One square quick-action tile: a Nerd glyph + label, accent-filled when on.
fn quick_tile(id: &str, on: bool) -> Element<'static, Message> {
    let (label, glyph) = tile_label(id).unwrap_or(("", ""));
    let fg = if on {
        palette::color(palette::HIGHLIGHT_TEXT)
    } else {
        palette::color(palette::WINDOW_TEXT)
    };
    let content = Column::new()
        .spacing(4.0)
        .align_x(Horizontal::Center)
        .width(Length::Fill)
        .push(
            text(glyph)
                .size(metrics::TILE_GLYPH_PX)
                .font(mde_ui::font::NERD)
                .color(fg),
        )
        .push(
            text(label)
                .size(metrics::UI_PX)
                .color(fg)
                .align_x(Horizontal::Center),
        );
    mouse_area(
        container(content)
            .width(Length::Fixed(TILE))
            .height(Length::Fixed(TILE))
            .align_x(Horizontal::Center)
            .align_y(Vertical::Center)
            .style(move |_| container::Style {
                background: Some(Background::Color(if on {
                    palette::color(palette::HIGHLIGHT)
                } else {
                    palette::color(palette::WINDOW)
                })),
                border: Border {
                    color: palette::color(palette::WINDOW_FRAME),
                    width: 1.0,
                    radius: 2.0.into(),
                },
                ..container::Style::default()
            }),
    )
    .on_press(Message::ToggleTile(id.to_string()))
    .into()
}

// --- toast (mde toast <id>) --------------------------------------------------

const TOAST_W: f32 = 360.0;
const TOAST_H: f32 = 84.0;
const TOAST_BAR: f32 = 40.0; // clear the Win10 taskbar (WIN10_BAR_H)

struct Toast {
    note: Notif,
}

#[to_layer_message]
#[derive(Debug, Clone)]
enum ToastMsg {
    Tick,  // auto-dismiss timer fired → hide the toast (it stays in history)
    Close, // x / body click → dismiss the notification entirely
}

/// `mde toast <id>` — pop the notification with this id as a bottom-right toast.
/// notifyd spawns one per Notify; it auto-dismisses on a timer or on click.
pub fn run_toast(args: &[String]) -> ExitCode {
    let Some(id) = args.first().and_then(|s| s.parse::<u32>().ok()) else {
        eprintln!("usage: mde toast <id>");
        return ExitCode::from(2);
    };
    let Some(note) = notifyd::load_file()
        .notifications
        .into_iter()
        .find(|n| n.id == id)
    else {
        return ExitCode::SUCCESS; // already cleared — nothing to show
    };
    match launch_toast(note) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mde toast: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Live toast count → this toast's upward offset, so concurrent toasts stack
/// instead of overlapping. Uses a runtime marker dir, pruning dead pids (toasts
/// exit(0) without cleanup, so the next launch reclaims their slot).
fn toast_offset() -> f32 {
    let dir = format!(
        "{}/mde-toasts",
        std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string())
    );
    let _ = std::fs::create_dir_all(&dir);
    let mut live = 0u32;
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            match e.file_name().to_string_lossy().parse::<u32>() {
                Ok(pid) if std::path::Path::new(&format!("/proc/{pid}")).exists() => live += 1,
                _ => {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
    }
    let _ = std::fs::write(format!("{dir}/{}", std::process::id()), "");
    live as f32 * (TOAST_H + 8.0)
}

fn launch_toast(note: Notif) -> Result<(), iced_layershell::Error> {
    // Critical notifications (urgency 2) don't auto-dismiss.
    let critical = note.hint_urgency >= 2;
    let bottom = (TOAST_BAR + 8.0 + toast_offset()) as i32;
    application(
        |_: &Toast| "mde-toast".to_string(),
        toast_update,
        toast_view,
    )
    .style(|_: &Toast, _: &iced::Theme| Appearance {
        background_color: Color::TRANSPARENT,
        text_color: palette::color(palette::WINDOW_TEXT),
    })
    .subscription(move |_: &Toast| {
        if critical {
            iced::Subscription::none()
        } else {
            iced::time::every(Duration::from_secs(5)).map(|_| ToastMsg::Tick)
        }
    })
    .font(mde_ui::font::REGULAR_BYTES)
    .font(mde_ui::font::BOLD_BYTES)
    .font(mde_ui::font::PLEX_REGULAR_BYTES)
    .font(mde_ui::font::PLEX_BOLD_BYTES)
    .default_font(mde_ui::font::ui())
    .settings(MainSettings {
        layer_settings: LayerShellSettings {
            // A SMALL surface (not a full-screen catcher) so clicks elsewhere fall
            // through to the apps below; non-focus-stealing.
            anchor: Anchor::Bottom | Anchor::Right,
            size: Some((TOAST_W as u32, TOAST_H as u32)),
            margin: (0, 12, bottom, 0),
            exclusive_zone: 0,
            keyboard_interactivity: KeyboardInteractivity::None,
            ..Default::default()
        },
        ..Default::default()
    })
    .run_with(move || (Toast { note }, Task::none()))
}

fn toast_update(state: &mut Toast, message: ToastMsg) -> Task<ToastMsg> {
    match message {
        // Timer: the toast just goes away; the record stays in the history.
        ToastMsg::Tick => exit(0),
        // x / body: dismiss the notification entirely.
        ToastMsg::Close => {
            dbus_close(state.note.id);
            exit(0)
        }
        _ => Task::none(),
    }
}

fn toast_view(state: &Toast) -> Element<'_, ToastMsg> {
    let n = &state.note;
    // Critical notifications (urgency >= 2) get a danger-red tint — a red summary
    // and a 2px red border — so they read as urgent at a glance (E3).
    let critical = n.hint_urgency >= 2;
    let head = Row::new()
        .align_y(Vertical::Center)
        .push(
            text(n.summary.clone())
                .size(metrics::UI_PX)
                .font(mde_ui::font::ui_bold())
                .width(Length::Fill)
                .color(if critical {
                    palette::color(palette::URGENT)
                } else {
                    palette::color(palette::WINDOW_TEXT)
                }),
        )
        .push(
            mouse_area(
                container(
                    text("\u{f00d}")
                        .size(metrics::UI_PX)
                        .font(mde_ui::font::NERD)
                        .color(palette::color(palette::GRAY_TEXT)),
                )
                .padding(pad(2.0, 6.0, 2.0, 6.0)),
            )
            .on_press(ToastMsg::Close),
        );
    let mut inner = Column::new().spacing(2.0).width(Length::Fill).push(head);
    if !n.body.is_empty() {
        inner = inner.push(
            text(n.body.clone())
                .size(metrics::UI_PX)
                .color(palette::color(palette::WINDOW_TEXT)),
        );
    }
    let icon = crate::icons::icon_any(&[n.app_icon.as_str(), "dialog-information"], 24);
    // Click the body to dismiss; the toast itself is the whole surface.
    mouse_area(
        container(row![icon, inner].spacing(8.0).align_y(Vertical::Top))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(10.0)
            .style(move |_| container::Style {
                background: Some(Background::Color(palette::color(palette::MENU))),
                border: Border {
                    color: if critical {
                        palette::color(palette::URGENT)
                    } else {
                        palette::color(palette::WINDOW_FRAME)
                    },
                    width: if critical { 2.0 } else { 1.0 },
                    radius: 2.0.into(),
                },
                shadow: Shadow {
                    color: Color {
                        a: 0.4,
                        ..Color::BLACK
                    },
                    offset: iced::Vector::new(-2.0, -2.0),
                    blur_radius: 14.0,
                },
                ..container::Style::default()
            }),
    )
    .on_press(ToastMsg::Close)
    .into()
}
