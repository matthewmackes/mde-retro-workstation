//! Taskbar — a wlr-layer-shell bar anchored to the bottom edge.
//!
//! A raised Win2000 panel: flag Start button, a window-button taskbar fed by sway
//! IPC (the focused window's button shows pressed), a flexible spacer, and a
//! sunken clock well. Polls sway + the clock once a second.

use std::process::{Child, Command, ExitCode};
use std::time::Duration;

use iced::mouse::ScrollDelta;
use iced::widget::{container, mouse_area, svg, text, Row, Space, Stack};
use iced::{Element, Length, Padding, Task};

/// The Start-button icon (carbon "layout-grid"), recoloured to the UI text colour.
const START_ICON: &[u8] = include_bytes!("start_icon.svg");
use iced_layershell::build_pattern::{application, MainSettings};
use iced_layershell::reexport::Anchor;
use iced_layershell::settings::LayerShellSettings;
use iced_layershell::{to_layer_message, Appearance};

use mde_ui::{button, frame, metrics, palette};

use crate::wlr;

#[derive(Default)]
struct Panel {
    windows: Vec<wlr::Window>,
    /// The wlr-foreign-toplevel client: the window list + focus/minimize control.
    wm: Option<wlr::Wm>,
    clock: String,
    /// Quick Launch pins, loaded from ~/.config/mde/menu.json at startup.
    pinned: Vec<crate::state::PinnedItem>,
    /// The StatusNotifier tray handle (the background watcher) and the latest
    /// snapshot of its items, refreshed each tick.
    tray: Option<crate::tray::Tray>,
    tray_items: Vec<crate::tray::TrayItem>,
    /// Native notification-area indicators (the Win2000 tray staples), polled
    /// each tick: speaker volume %, network state, and battery % + charging.
    volume: Option<(u8, bool)>,
    net: NetState,
    battery: Option<(u8, bool)>,
    /// Whether a laptop backlight exists (gates the brightness tray glyph).
    has_backlight: bool,
    /// The Start menu child process, if open. The panel owns it so a second
    /// Start click toggles it closed instead of stacking another full-screen
    /// overlay (which made the menu "take several clicks" to open), and so it
    /// gets reaped rather than left as a zombie.
    menu: Option<Child>,
    /// Other fire-and-forget children (popups, launched apps) we reap each tick
    /// to keep them from piling up as zombies.
    children: Vec<Child>,
}

/// Network connectivity, summarised for the tray glyph.
#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum NetState {
    #[default]
    Disconnected,
    Wifi,
    Wired,
}

#[to_layer_message]
#[derive(Debug, Clone)]
enum Message {
    Tick,
    Start,
    StartContext,
    TaskbarContext,
    TaskButton(u64),
    MinimizeToggle(u64),
    Brightness(bool),
    Launch(String),
    TrayActivate(usize),
}

pub fn run(_args: &[String]) -> ExitCode {
    match launch() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mde panel: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Load the Hack Nerd Font bytes from the system so iced can render the
/// notification-area glyphs. Leaked to `'static` (one-time, at startup) because
/// the app builder needs `'static` font data; `None` if it isn't installed (the
/// glyphs then fall back to tofu, which we accept rather than crash).
fn nerd_font_bytes() -> Option<&'static [u8]> {
    const PATHS: &[&str] = &[
        "/usr/local/share/fonts/HackNerdFont/HackNerdFont-Regular.ttf",
        "/usr/share/fonts/HackNerdFont/HackNerdFont-Regular.ttf",
        "/usr/share/fonts/hack-nerd/HackNerdFont-Regular.ttf",
    ];
    for p in PATHS {
        if let Ok(bytes) = std::fs::read(p) {
            return Some(Box::leak(bytes.into_boxed_slice()));
        }
    }
    None
}

fn launch() -> Result<(), iced_layershell::Error> {
    let mut app = application(namespace, update, view)
        .style(style)
        .subscription(subscription)
        .font(mde_ui::font::REGULAR_BYTES)
        .font(mde_ui::font::BOLD_BYTES)
        .default_font(mde_ui::font::UI);
    // Register the Nerd Font for glyph icons if present on the system.
    if let Some(bytes) = nerd_font_bytes() {
        app = app.font(bytes);
    }
    app.settings(MainSettings {
            layer_settings: LayerShellSettings {
                size: Some((0, metrics::TASKBAR_HEIGHT as u32)),
                exclusive_zone: metrics::TASKBAR_HEIGHT as i32,
                anchor: Anchor::Bottom | Anchor::Left | Anchor::Right,
                ..Default::default()
            },
            ..Default::default()
        })
        .run_with(|| {
            let panel = Panel {
                pinned: crate::state::load().pinned,
                tray: Some(crate::tray::start()),
                wm: wlr::start(),
                has_backlight: backlight_dir().is_some(),
                ..Panel::default()
            };
            (panel, Task::done(Message::Tick))
        })
}

fn namespace(_state: &Panel) -> String {
    "mde-panel".to_string()
}

fn style(_state: &Panel, _theme: &iced::Theme) -> Appearance {
    Appearance {
        background_color: palette::color(palette::BUTTON_FACE),
        text_color: palette::color(palette::WINDOW_TEXT),
    }
}

fn subscription(_state: &Panel) -> iced::Subscription<Message> {
    iced::time::every(Duration::from_secs(1)).map(|_| Message::Tick)
}

fn update(state: &mut Panel, message: Message) -> Task<Message> {
    match message {
        Message::Tick => {
            state.windows = state.wm.as_ref().map(|w| w.windows()).unwrap_or_default();
            state.clock = clock_now();
            if let Some(t) = &state.tray {
                state.tray_items = t.lock().map(|v| v.clone()).unwrap_or_default();
            }
            state.volume = poll_volume();
            state.net = poll_net();
            state.battery = poll_battery();
            // Reap finished children so they don't linger as zombies, and clear
            // the menu handle once it has closed itself (item picked / clicked
            // away) so the next Start click re-opens it.
            if let Some(child) = &mut state.menu {
                if !matches!(child.try_wait(), Ok(None)) {
                    state.menu = None;
                }
            }
            state.children.retain_mut(|c| matches!(c.try_wait(), Ok(None)));
        }
        Message::TrayActivate(i) => {
            if let Some(it) = state.tray_items.get(i) {
                crate::tray::activate(&it.service, &it.path);
            }
        }
        // Toggle the Start menu: open it if closed, close it if already open.
        // Owning the child (instead of fire-and-forget spawning) is what stops
        // rapid clicks during the menu's start-up from stacking duplicate
        // full-screen overlays.
        Message::Start => match state.menu.take() {
            Some(mut child) => match child.try_wait() {
                Ok(None) => {
                    // Still open → close it (and reap it).
                    let _ = child.kill();
                    let _ = child.wait();
                }
                // Already exited → reopen.
                _ => state.menu = spawn_child(&["menu"]),
            },
            None => state.menu = spawn_child(&["menu"]),
        },
        Message::StartContext => push_child(state, spawn_child(&["popup", "start"])),
        Message::TaskbarContext => push_child(state, spawn_child(&["popup", "taskbar"])),
        // Windows 2000 taskbar-button behaviour:
        //   • a minimized window  → restore (and focus) it
        //   • the focused window   → minimize it
        //   • any other window     → focus/raise it
        Message::TaskButton(id) => {
            if let Some(wm) = &state.wm {
                // Read the live snapshot (not the up-to-1s-stale tick copy) so a
                // focus-then-click-again minimizes without waiting for a poll.
                let (focused, minimized) = wm
                    .windows()
                    .iter()
                    .find(|w| w.id == id)
                    .map(|w| (w.focused, w.minimized))
                    .unwrap_or((false, false));
                if minimized {
                    wm.focus(id);
                } else if focused {
                    wm.set_minimized(id, true);
                } else {
                    wm.focus(id);
                }
            }
        }
        // Right-click a taskbar button to minimize/restore it. (Full
        // Restore/Maximize/Close live on the labwc titlebar + its right-click menu.)
        Message::MinimizeToggle(id) => {
            let minimized = state.windows.iter().find(|w| w.id == id).map(|w| w.minimized).unwrap_or(false);
            if let Some(w) = &state.wm {
                w.set_minimized(id, !minimized);
            }
        }
        Message::Launch(cmd) => {
            if let Ok(child) = Command::new("sh").arg("-c").arg(&cmd).spawn() {
                state.children.push(child);
            }
        }
        Message::Brightness(up) => {
            if let Some(child) = step_brightness(up) {
                state.children.push(child);
            }
        }
        _ => {}
    }
    Task::none()
}

fn view(state: &Panel) -> Element<'_, Message> {
    let mut bar = Row::new()
        .spacing(2.0)
        .height(Length::Fill)
        .push(
            mouse_area(
                button(
                    Row::new()
                        .spacing(4.0)
                        .align_y(iced::Alignment::Center)
                        .push(
                            svg(svg::Handle::from_memory(START_ICON))
                                .width(Length::Fixed(16.0))
                                .height(Length::Fixed(16.0))
                                .style(|_t, _s| svg::Style {
                                    color: Some(palette::color(palette::WINDOW_TEXT)),
                                }),
                        )
                        .push(text("Start").size(metrics::UI_PX).font(mde_ui::font::UI_BOLD)),
                )
                .on_press(Message::Start)
                // Show the button pressed while the menu is open — immediate
                // feedback so a single click clearly registers.
                .active(state.menu.is_some())
                .height(Length::Fill),
            )
            .on_right_press(Message::StartContext),
        )
        .push(Space::with_width(Length::Fixed(6.0)));

    // Quick Launch: pinned apps (from menu.json), between Start and the windows.
    if !state.pinned.is_empty() {
        for item in &state.pinned {
            bar = bar.push(
                button(text(truncate(&item.name, 12)).size(metrics::UI_PX))
                    .on_press(Message::Launch(item.command.clone()))
                    .height(Length::Fill),
            );
        }
        bar = bar.push(Space::with_width(Length::Fixed(6.0)));
    }

    for w in &state.windows {
        // Left-click focuses (and restores a minimized window); right-click opens
        // the window's system menu (Restore / Minimize / Maximize / Close).
        bar = bar.push(
            mouse_area(
                button(text(truncate(&w.title, 22)).size(metrics::UI_PX))
                    .on_press(Message::TaskButton(w.id))
                    .active(w.focused)
                    .width(Length::Fixed(metrics::TASKBAR_BUTTON_MIN as f32))
                    .height(Length::Fill),
            )
            .on_right_press(Message::MinimizeToggle(w.id)),
        );
    }

    // The empty stretch of bar: right-click opens the taskbar context menu.
    bar = bar.push(
        mouse_area(Space::new(Length::Fill, Length::Fill)).on_right_press(Message::TaskbarContext),
    );

    // The notification area: SNI tray icons (as glyphs) + the shell's own
    // Volume / Network / Battery indicators, all in the Nerd Font, then the clock.
    let mut tray = Row::new().spacing(3.0).align_y(iced::Alignment::Center);
    // Third-party SNI items, rendered as glyphs; network-ish ones are skipped
    // because the shell draws network itself just below.
    for (i, item) in state.tray_items.iter().enumerate() {
        if is_network_icon(&item.icon_name) {
            continue;
        }
        tray = tray.push(glyph_button(sni_glyph(&item.icon_name), Message::TrayActivate(i)));
    }
    // Brightness (laptop backlight): scroll to dim/brighten, click opens Display.
    if state.has_backlight {
        tray = tray.push(
            mouse_area(glyph_el('\u{f0335}'))
                .on_press(Message::Launch("mde display".into()))
                .on_scroll(|d| Message::Brightness(scroll_up(&d))),
        );
    }
    // Volume: scroll to change, click to mute, right-click opens the mixer.
    if let Some((pct, muted)) = state.volume {
        tray = tray.push(
            mouse_area(glyph_el(volume_glyph(pct, muted)))
                .on_press(Message::Launch("wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle".into()))
                .on_right_press(Message::Launch("pavucontrol".into()))
                .on_scroll(|d| {
                    if scroll_up(&d) {
                        Message::Launch("wpctl set-volume -l 1.0 @DEFAULT_AUDIO_SINK@ 5%+".into())
                    } else {
                        Message::Launch("wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%-".into())
                    }
                }),
        );
    }
    // Network (click → nm-connection-editor).
    tray = tray.push(glyph_button(net_glyph(state.net), Message::Launch("nm-connection-editor".into())));
    // Battery — click opens Power Options: a real power manager if one is
    // installed, else the shell's Control Panel (Power Management category).
    if let Some((pct, charging)) = state.battery {
        tray = tray.push(glyph_button(
            battery_glyph(pct, charging),
            Message::Launch(
                "xfce4-power-manager-settings || gnome-power-statistics \
                 || mate-power-preferences || gnome-control-center power \
                 || mde control-panel"
                    .into(),
            ),
        ));
    }
    // The Win2000 notification area: a single sunken well holding the tray
    // glyphs on the left and the clock on the right. The content is the stack's
    // *base* (so the well shrinks to fit it — a Fill frame as base would stretch
    // the well across the whole right end of the bar); the sunken bevel is a
    // faceless overlay drawn at that size over the silver bar.
    let notification = Stack::new()
        .push(
            container(
                Row::new()
                    .align_y(iced::Alignment::Center)
                    .height(Length::Fill)
                    .push(tray)
                    .push(Space::with_width(Length::Fixed(6.0)))
                    .push(text(state.clock.clone()).size(metrics::UI_PX)),
            )
            .center_y(Length::Fill)
            .padding(Padding { top: 0.0, right: 8.0, bottom: 0.0, left: 6.0 }),
        )
        .push(frame::sunken().no_face())
        .width(Length::Shrink);
    bar = bar.push(container(notification).height(Length::Fill).padding(2.0));

    Stack::new()
        .push(frame::raised())
        .push(
            container(bar)
                .padding(2.0)
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .into()
}

// --- notification-area indicators ------------------------------------------

/// Default-sink volume as (percent, muted), via wpctl (PipeWire) then pactl.
fn poll_volume() -> Option<(u8, bool)> {
    if let Ok(o) = Command::new("wpctl").args(["get-volume", "@DEFAULT_AUDIO_SINK@"]).output() {
        if o.status.success() {
            // "Volume: 0.45 [MUTED]"
            let s = String::from_utf8_lossy(&o.stdout);
            let muted = s.contains("MUTED");
            if let Some(v) = s.split_whitespace().nth(1).and_then(|t| t.parse::<f32>().ok()) {
                return Some(((v * 100.0).round() as u8, muted));
            }
        }
    }
    if let Ok(o) = Command::new("pactl").args(["get-sink-mute", "@DEFAULT_SINK@"]).output() {
        let muted = String::from_utf8_lossy(&o.stdout).contains("yes");
        if let Ok(v) = Command::new("pactl").args(["get-sink-volume", "@DEFAULT_SINK@"]).output() {
            let s = String::from_utf8_lossy(&v.stdout);
            if let Some(pct) = s.split('/').nth(1).and_then(|t| t.trim().trim_end_matches('%').parse::<u8>().ok()) {
                return Some((pct, muted));
            }
        }
    }
    None
}

/// Network state from nmcli: wired beats wifi beats disconnected.
fn poll_net() -> NetState {
    let Ok(o) = Command::new("nmcli").args(["-t", "-f", "TYPE,STATE", "device"]).output() else {
        return NetState::Disconnected;
    };
    let s = String::from_utf8_lossy(&o.stdout);
    let (mut wifi, mut wired) = (false, false);
    for line in s.lines() {
        let mut it = line.split(':');
        let ty = it.next().unwrap_or("");
        let st = it.next().unwrap_or("");
        if st.starts_with("connected") {
            match ty {
                "ethernet" => wired = true,
                "wifi" => wifi = true,
                _ => {}
            }
        }
    }
    if wired {
        NetState::Wired
    } else if wifi {
        NetState::Wifi
    } else {
        NetState::Disconnected
    }
}

/// The first laptop backlight device directory, if any.
fn backlight_dir() -> Option<std::path::PathBuf> {
    std::fs::read_dir("/sys/class/backlight").ok()?.flatten().map(|e| e.path()).next()
}

/// Step the backlight up/down via logind's SetBrightness (no root). Returns the
/// spawned `busctl` child so the panel can reap it.
fn step_brightness(up: bool) -> Option<Child> {
    let dir = backlight_dir()?;
    let dev = dir.file_name()?.to_str()?.to_string();
    let cur: u32 = std::fs::read_to_string(dir.join("brightness")).ok()?.trim().parse().ok()?;
    let max: u32 = std::fs::read_to_string(dir.join("max_brightness")).ok()?.trim().parse().ok()?;
    let step = (max * 7 / 100).max(1);
    let floor = max * 5 / 100;
    let new = if up { (cur + step).min(max) } else { cur.saturating_sub(step).max(floor) };
    Command::new("busctl")
        .args([
            "call",
            "org.freedesktop.login1",
            "/org/freedesktop/login1/session/auto",
            "org.freedesktop.login1.Session",
            "SetBrightness",
            "ssu",
            "backlight",
            &dev,
            &new.to_string(),
        ])
        .spawn()
        .ok()
}

/// Whether a scroll gesture went up (raise) rather than down (lower).
fn scroll_up(d: &ScrollDelta) -> bool {
    let y = match d {
        ScrollDelta::Lines { y, .. } | ScrollDelta::Pixels { y, .. } => *y,
    };
    y >= 0.0
}

/// Battery as (percent, charging) from sysfs; None when there's no battery.
fn poll_battery() -> Option<(u8, bool)> {
    let rd = std::fs::read_dir("/sys/class/power_supply").ok()?;
    for e in rd.flatten() {
        if e.file_name().to_string_lossy().starts_with("BAT") {
            let cap = std::fs::read_to_string(e.path().join("capacity")).ok()?;
            let pct = cap.trim().parse::<u8>().ok()?;
            let status = std::fs::read_to_string(e.path().join("status")).unwrap_or_default();
            let charging = matches!(status.trim(), "Charging" | "Full" | "Not charging");
            return Some((pct, charging));
        }
    }
    None
}

// Nerd Font glyphs (Font Awesome + Material Design Icon ranges in Hack Nerd Font).
fn volume_glyph(pct: u8, muted: bool) -> char {
    if muted || pct == 0 {
        '\u{f026}' // fa-volume-off
    } else if pct < 50 {
        '\u{f027}' // fa-volume-down
    } else {
        '\u{f028}' // fa-volume-up
    }
}

fn net_glyph(net: NetState) -> char {
    match net {
        NetState::Wifi => '\u{f05a9}',         // md-wifi
        NetState::Wired => '\u{f0200}',        // md-ethernet
        NetState::Disconnected => '\u{f05aa}', // md-wifi-off
    }
}

fn battery_glyph(pct: u8, charging: bool) -> char {
    if charging {
        return '\u{f0084}'; // md-battery-charging
    }
    match pct {
        0..=10 => '\u{f244}',  // fa-battery-empty
        11..=35 => '\u{f243}', // fa-battery-quarter
        36..=60 => '\u{f242}', // fa-battery-half
        61..=85 => '\u{f241}', // fa-battery-three-quarters
        _ => '\u{f240}',       // fa-battery-full
    }
}

/// A Nerd Font glyph mapped from an SNI item's icon name (best-effort), for the
/// "use glyphs for all tray icons" rule. Network-ish items are filtered out
/// upstream (the shell draws network natively), so this covers the rest.
fn sni_glyph(icon_name: &str) -> char {
    let n = icon_name.to_ascii_lowercase();
    if n.contains("bluetooth") {
        '\u{f0293}' // md-bluetooth
    } else if n.contains("volume") || n.contains("audio") || n.contains("sound") {
        '\u{f028}'
    } else if n.contains("battery") || n.contains("power") {
        '\u{f0079}'
    } else if n.contains("display") || n.contains("bright") {
        '\u{f0335}' // md-brightness
    } else if n.contains("update") || n.contains("software") {
        '\u{f06b0}' // md-update
    } else {
        '\u{f0c8}' // md-square (neutral placeholder)
    }
}

/// Whether an SNI item is a NetworkManager-style network icon, which the shell
/// now renders natively (so we don't show it twice).
fn is_network_icon(icon_name: &str) -> bool {
    let n = icon_name.to_ascii_lowercase();
    ["network", "wifi", "wireless", "signal", "nm-", "wired", "ethernet", "vpn"]
        .iter()
        .any(|k| n.contains(k))
}

/// A bare notification-area glyph (no button chrome) for wrapping in a
/// `mouse_area` that wants click + scroll handling.
fn glyph_el(g: char) -> Element<'static, Message> {
    container(text(g.to_string()).font(mde_ui::font::NERD).size(15.0).color(palette::color(palette::WINDOW_TEXT)))
        .padding(Padding { top: 1.0, right: 3.0, bottom: 1.0, left: 3.0 })
        .into()
}

/// A flat (chromeless) notification-area glyph button.
fn glyph_button(g: char, msg: Message) -> Element<'static, Message> {
    iced::widget::button(
        text(g.to_string()).font(mde_ui::font::NERD).size(15.0).color(palette::color(palette::WINDOW_TEXT)),
    )
    .on_press(msg)
    .padding(Padding { top: 1.0, right: 3.0, bottom: 1.0, left: 3.0 })
    .style(|_, _| iced::widget::button::Style { background: None, ..Default::default() })
    .into()
}

fn clock_now() -> String {
    Command::new("date")
        .arg("+%-l:%M %p")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Spawn this binary with `args`, returning the child handle so the panel can
/// reap (and, for the menu, kill) it.
fn spawn_child(args: &[&str]) -> Option<Child> {
    std::env::current_exe().ok().and_then(|exe| Command::new(exe).args(args).spawn().ok())
}

/// Track a spawned child for later reaping (ignores a failed spawn).
fn push_child(state: &mut Panel, child: Option<Child>) {
    if let Some(c) = child {
        state.children.push(c);
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        let head: String = s.chars().take(n.saturating_sub(1)).collect();
        format!("{head}\u{2026}")
    } else {
        s.to_string()
    }
}
