//! Taskbar — a wlr-layer-shell bar anchored to the bottom edge.
//!
//! A raised Win2000 panel: flag Start button, a window-button taskbar fed by sway
//! IPC (the focused window's button shows pressed), a flexible spacer, and a
//! sunken clock well. Polls sway + the clock once a second.

use std::process::{Child, Command, ExitCode};
use std::time::Duration;

use iced::mouse::ScrollDelta;
use iced::widget::{container, image, mouse_area, text, Column, Row, Space, Stack};
use iced::{Color, Element, Length, Padding, Task};

/// Height of the Carbon UI Shell top bar (px) — a touch taller than the Win2000
/// taskbar for the flatter product-header feel.
const CARBON_BAR_H: f32 = 32.0;

/// The Start-button icon (carbon "layout-grid") as a black PNG. Deliberately a
/// raster, not SVG: iced loads the entire system font DB (~20 MB) the first time
/// it renders any SVG, and this was the panel's only guaranteed SVG. A PNG keeps
/// the panel font-DB-free in the common (PNG-icon) case.
const START_ICON: &[u8] = include_bytes!("start_icon.png");

/// Width of the vertical BeOS Deskbar (px).
const BEOS_BAR_W: f32 = 115.0;
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
    /// Tick counter: the expensive subprocess polls run every 5th tick.
    tick: u32,
    /// Local UTC offset (seconds), read once at startup so the clock formats
    /// in-process instead of forking `date` every tick.
    clock_offset: i32,
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
        .font(mde_ui::font::BOLD_BYTES).font(mde_ui::font::PLEX_REGULAR_BYTES).font(mde_ui::font::PLEX_BOLD_BYTES)
        .default_font(mde_ui::font::ui());
    // Register the Nerd Font for glyph icons if present on the system.
    if let Some(bytes) = nerd_font_bytes() {
        app = app.font(bytes);
    }
    // Carbon: a flat UI Shell bar anchored to the TOP edge. BeOS: a vertical
    // Deskbar on the left. Windows 2000: the horizontal taskbar along the bottom.
    // Either way the bar reserves its strip via the exclusive zone.
    let layer_settings = if palette::is_carbon() {
        LayerShellSettings {
            size: Some((0, CARBON_BAR_H as u32)),
            exclusive_zone: CARBON_BAR_H as i32,
            anchor: Anchor::Top | Anchor::Left | Anchor::Right,
            ..Default::default()
        }
    } else if palette::is_beos() {
        LayerShellSettings {
            size: Some((BEOS_BAR_W as u32, 0)),
            exclusive_zone: BEOS_BAR_W as i32,
            anchor: Anchor::Top | Anchor::Left | Anchor::Bottom,
            ..Default::default()
        }
    } else {
        LayerShellSettings {
            size: Some((0, metrics::TASKBAR_HEIGHT as u32)),
            exclusive_zone: metrics::TASKBAR_HEIGHT as i32,
            anchor: Anchor::Bottom | Anchor::Left | Anchor::Right,
            ..Default::default()
        }
    };
    app.settings(MainSettings { layer_settings, ..Default::default() })
        .run_with(|| {
            let panel = Panel {
                pinned: crate::state::load().pinned,
                tray: Some(crate::tray::start()),
                wm: wlr::start(),
                has_backlight: backlight_dir().is_some(),
                clock_offset: utc_offset_secs(),
                ..Panel::default()
            };
            (panel, Task::done(Message::Tick))
        })
}

fn namespace(_state: &Panel) -> String {
    "mde-panel".to_string()
}

fn style(_state: &Panel, _theme: &iced::Theme) -> Appearance {
    // The bar surface comes from the SHELL_HEADER role: under Carbon a flat Gray
    // 100 (dark) / white (light) UI-Shell header, under Win2000/BeOS the silver
    // taskbar. Routed through palette::color() like every other surface (§2.1) —
    // no raw hex here.
    Appearance {
        background_color: palette::color(palette::SHELL_HEADER),
        text_color: palette::color(palette::WINDOW_TEXT),
    }
}

fn subscription(_state: &Panel) -> iced::Subscription<Message> {
    iced::time::every(Duration::from_secs(1)).map(|_| Message::Tick)
}

fn update(state: &mut Panel, message: Message) -> Task<Message> {
    match message {
        Message::Tick => {
            // Cheap every tick (just reads shared memory): the window list and
            // the tray snapshot, so the taskbar stays responsive.
            state.windows = state.wm.as_ref().map(|w| w.windows()).unwrap_or_default();
            if let Some(t) = &state.tray {
                state.tray_items = t.lock().map(|v| v.clone()).unwrap_or_default();
            }
            // The expensive indicators each fork a subprocess (date / wpctl /
            // nmcli). They only need ~minute precision and change rarely, so poll
            // them every 5th tick — cutting ~3 forks/sec to ~0.6/sec.
            if state.tick % 5 == 0 {
                state.clock = clock_now(state.clock_offset);
                state.volume = poll_volume();
                state.net = poll_net();
                state.battery = poll_battery();
            }
            state.tick = state.tick.wrapping_add(1);
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

/// The Start button (carbon grid icon + "Start" label) at width `w` × height
/// `h`, including the shared right-click (Start context menu). Used by both bars.
fn start_button(state: &Panel, w: Length, h: Length) -> Element<'_, Message> {
    mouse_area(
        button(
            Row::new()
                .spacing(4.0)
                .align_y(iced::Alignment::Center)
                .push(
                    image(image::Handle::from_bytes(START_ICON))
                        .width(Length::Fixed(16.0))
                        .height(Length::Fixed(16.0)),
                )
                .push(text("Start").size(metrics::UI_PX).font(mde_ui::font::ui_bold())),
        )
        .on_press(Message::Start)
        .active(state.menu.is_some())
        .width(w)
        .height(h),
    )
    .on_right_press(Message::StartContext)
    .into()
}

/// The notification-area glyphs (SNI items + brightness/volume/network/battery),
/// built once and arranged by either bar orientation.
fn tray_glyphs(state: &Panel) -> Vec<Element<'_, Message>> {
    let mut v: Vec<Element<Message>> = Vec::new();
    for (i, item) in state.tray_items.iter().enumerate() {
        if is_network_icon(&item.icon_name) {
            continue;
        }
        v.push(glyph_button(sni_glyph(&item.icon_name), Message::TrayActivate(i)));
    }
    if state.has_backlight {
        v.push(
            mouse_area(glyph_el('\u{f0335}'))
                .on_press(Message::Launch("mde display".into()))
                .on_scroll(|d| Message::Brightness(scroll_up(&d)))
                .into(),
        );
    }
    if let Some((pct, muted)) = state.volume {
        v.push(
            mouse_area(glyph_el(volume_glyph(pct, muted)))
                .on_press(Message::Launch("wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle".into()))
                .on_right_press(Message::Launch("pavucontrol".into()))
                .on_scroll(|d| {
                    if scroll_up(&d) {
                        Message::Launch("wpctl set-volume -l 1.0 @DEFAULT_AUDIO_SINK@ 5%+".into())
                    } else {
                        Message::Launch("wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%-".into())
                    }
                })
                .into(),
        );
    }
    v.push(glyph_button(net_glyph(state.net), Message::Launch("nm-connection-editor".into())));
    if let Some((pct, charging)) = state.battery {
        v.push(glyph_button(
            battery_glyph(pct, charging),
            Message::Launch(
                "xfce4-power-manager-settings || gnome-power-statistics \
                 || mate-power-preferences || gnome-control-center power \
                 || mde control-panel"
                    .into(),
            ),
        ));
    }
    v
}

/// Dispatch to the Carbon top bar, the vertical BeOS Deskbar, or the horizontal
/// Windows-2000 taskbar.
fn view(state: &Panel) -> Element<'_, Message> {
    if palette::is_carbon() {
        view_carbon(state)
    } else if palette::is_beos() {
        view_vertical(state)
    } else {
        view_horizontal(state)
    }
}

/// The Carbon UI Shell header: a flat top bar. Left: a ≡ switcher button + the
/// "MDE" breadcrumb. Middle: running windows as flat tabs with a 2px accent
/// underline on the focused one. Right: tray glyphs + clock. No bevels, no wells.
fn view_carbon(state: &Panel) -> Element<'_, Message> {
    let text_c = palette::color(palette::WINDOW_TEXT);

    // ≡ switcher + "MDE" breadcrumb (opens the product-switcher menu).
    let start = mouse_area(
        container(
            Row::new()
                .spacing(6.0)
                .align_y(iced::Alignment::Center)
                .push(text("\u{f0c9}").size(15.0).font(mde_ui::font::NERD).color(text_c)) // fa-bars (≡)
                .push(text("MDE").size(metrics::UI_PX).font(mde_ui::font::ui_bold()).color(text_c)),
        )
        .height(Length::Fill)
        .center_y(Length::Fill)
        .padding(Padding { top: 0.0, right: 12.0, bottom: 0.0, left: 12.0 }),
    )
    .on_press(Message::Start)
    .on_right_press(Message::StartContext);

    let mut bar = Row::new()
        .spacing(0.0)
        .height(Length::Fill)
        .align_y(iced::Alignment::Center)
        .push(start);

    // Quick Launch pins as flat ghost buttons.
    for item in &state.pinned {
        bar = bar.push(
            mouse_area(
                container(text(truncate(&item.name, 12)).size(metrics::UI_PX).color(text_c))
                    .height(Length::Fill)
                    .center_y(Length::Fill)
                    .padding(Padding { top: 0.0, right: 10.0, bottom: 0.0, left: 10.0 }),
            )
            .on_press(Message::Launch(item.command.clone())),
        );
    }

    // Running windows: flat tabs, focused one underlined in the accent.
    for w in &state.windows {
        bar = bar.push(carbon_task_button(w, text_c));
    }

    // Empty stretch: right-click opens the taskbar context menu.
    bar = bar.push(
        mouse_area(Space::new(Length::Fill, Length::Fill)).on_right_press(Message::TaskbarContext),
    );

    // Tray glyphs then the clock, flat at the right (no sunken well).
    let mut tray = Row::new().spacing(4.0).align_y(iced::Alignment::Center);
    for g in tray_glyphs(state) {
        tray = tray.push(g);
    }
    bar = bar.push(
        container(
            Row::new()
                .spacing(8.0)
                .align_y(iced::Alignment::Center)
                .height(Length::Fill)
                .push(tray)
                .push(text(state.clock.clone()).size(metrics::UI_PX).color(text_c)),
        )
        .height(Length::Fill)
        .center_y(Length::Fill)
        .padding(Padding { top: 0.0, right: 12.0, bottom: 0.0, left: 6.0 }),
    );

    // A 1px Carbon border-subtle divider along the bottom edge of the header.
    Stack::new()
        .push(container(bar).width(Length::Fill).height(Length::Fill))
        .push(
            Column::new().push(Space::new(Length::Fill, Length::Fill)).push(
                container(Space::new(Length::Fill, Length::Fixed(1.0))).width(Length::Fill).style(|_| {
                    container::Style {
                        background: Some(iced::Background::Color(palette::color(palette::WINDOW_FRAME))),
                        ..container::Style::default()
                    }
                }),
            ),
        )
        .into()
}

/// A Carbon running-window tab: icon + title, flat, with a 2px accent underline
/// when the window is focused. Left-click focuses/minimizes; right-click toggles
/// minimize (same rules as the Win2000 taskbar button).
fn carbon_task_button(w: &wlr::Window, text_c: Color) -> Element<'_, Message> {
    let label = Row::new()
        .spacing(6.0)
        .align_y(iced::Alignment::Center)
        .push(crate::icons::icon_any(&[w.app_id.as_str(), "application-x-executable"], 16))
        .push(text(truncate(&w.title, 18)).size(metrics::UI_PX).color(text_c));
    let underline = if w.focused { palette::accent() } else { Color::TRANSPARENT };
    let col = Column::new()
        .width(Length::Fixed(metrics::TASKBAR_BUTTON_MIN as f32))
        .height(Length::Fill)
        .push(
            container(label)
                .height(Length::Fill)
                .center_y(Length::Fill)
                .padding(Padding { top: 0.0, right: 10.0, bottom: 0.0, left: 10.0 }),
        )
        .push(
            container(Space::new(Length::Fill, Length::Fixed(2.0))).style(move |_| container::Style {
                background: Some(iced::Background::Color(underline)),
                ..container::Style::default()
            }),
        );
    mouse_area(col)
        .on_press(Message::TaskButton(w.id))
        .on_right_press(Message::MinimizeToggle(w.id))
        .into()
}

fn view_horizontal(state: &Panel) -> Element<'_, Message> {
    let mut bar = Row::new()
        .spacing(2.0)
        .height(Length::Fill)
        .push(start_button(state, Length::Shrink, Length::Fill))
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
        let label = Row::new()
            .spacing(4.0)
            .align_y(iced::Alignment::Center)
            .push(crate::icons::icon_any(&[w.app_id.as_str(), "application-x-executable"], 16))
            .push(text(truncate(&w.title, 20)).size(metrics::UI_PX));
        bar = bar.push(
            mouse_area(
                button(label)
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

    // The notification area: tray glyphs then the clock, in one sunken well.
    let mut tray = Row::new().spacing(3.0).align_y(iced::Alignment::Center);
    for g in tray_glyphs(state) {
        tray = tray.push(g);
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

/// A thin etched horizontal divider for the vertical bar.
fn vbar_sep<'a>() -> Element<'a, Message> {
    container(Space::new(Length::Fill, Length::Fixed(1.0)))
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(palette::color(palette::BUTTON_SHADOW))),
            ..container::Style::default()
        })
        .into()
}

/// The BeOS Deskbar: a vertical strip on the left — clock + tray glyphs, then
/// the running-window list, with the Start button pinned at the very bottom.
fn view_vertical(state: &Panel) -> Element<'_, Message> {
    let mut col = Column::new().spacing(3.0).width(Length::Fill).height(Length::Fill);

    // Clock, then the tray glyphs in a centred row (BeOS keeps these near the top).
    col = col.push(
        container(text(state.clock.clone()).size(metrics::UI_PX))
            .width(Length::Fill)
            .center_x(Length::Fill)
            .padding(Padding { top: 2.0, right: 0.0, bottom: 2.0, left: 0.0 }),
    );
    let mut tray = Row::new().spacing(2.0).align_y(iced::Alignment::Center);
    for g in tray_glyphs(state) {
        tray = tray.push(g);
    }
    col = col.push(container(tray).width(Length::Fill).center_x(Length::Fill));
    col = col.push(vbar_sep());

    // Quick-launch pins.
    for item in &state.pinned {
        col = col.push(
            button(text(truncate(&item.name, 14)).size(metrics::UI_PX))
                .on_press(Message::Launch(item.command.clone()))
                .width(Length::Fill),
        );
    }

    // Running windows, stacked; each is icon + title (same Win2000 click rules).
    for w in &state.windows {
        let label = Row::new()
            .spacing(4.0)
            .align_y(iced::Alignment::Center)
            .push(crate::icons::icon_any(&[w.app_id.as_str(), "application-x-executable"], 16))
            .push(text(truncate(&w.title, 11)).size(metrics::UI_PX));
        col = col.push(
            mouse_area(
                button(label)
                    .on_press(Message::TaskButton(w.id))
                    .active(w.focused)
                    .width(Length::Fill)
                    .height(Length::Fixed(28.0)),
            )
            .on_right_press(Message::MinimizeToggle(w.id)),
        );
    }

    // The empty remainder (right-click opens the taskbar context menu) pushes
    // the Start button down to the very bottom of the bar.
    col = col.push(
        mouse_area(Space::new(Length::Fill, Length::Fill)).on_right_press(Message::TaskbarContext),
    );
    col = col.push(vbar_sep());
    col = col.push(start_button(state, Length::Fill, Length::Fixed(24.0)));

    Stack::new()
        .push(frame::raised())
        .push(container(col).padding(2.0).width(Length::Fill).height(Length::Fill))
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

/// The local UTC offset in seconds, read once at startup via `date +%z`
/// (e.g. "-0400" → -14400). Reading it once avoids forking `date` every tick.
fn utc_offset_secs() -> i32 {
    Command::new("date")
        .arg("+%z")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| parse_utc_offset(s.trim()))
        .unwrap_or(0)
}

/// Parse a `+HHMM` / `-HHMM` UTC offset into seconds.
fn parse_utc_offset(s: &str) -> Option<i32> {
    let sign = if s.starts_with('-') { -1 } else { 1 };
    let d = s.trim_start_matches(['+', '-']);
    if d.len() < 4 {
        return None;
    }
    let h: i32 = d.get(0..2)?.parse().ok()?;
    let m: i32 = d.get(2..4)?.parse().ok()?;
    Some(sign * (h * 3600 + m * 60))
}

/// Format an epoch-seconds instant as a Win2000 clock ("3:58 PM") in the given
/// local offset — pure, so no per-tick subprocess.
fn format_clock(epoch_secs: u64, offset_secs: i32) -> String {
    let local = epoch_secs as i64 + offset_secs as i64;
    let day = local.rem_euclid(86_400);
    let h = (day / 3600) as u32;
    let m = ((day % 3600) / 60) as u32;
    let (ampm, h12) = if h < 12 { ("AM", h) } else { ("PM", h - 12) };
    let h12 = if h12 == 0 { 12 } else { h12 };
    format!("{h12}:{m:02} {ampm}")
}

fn clock_now(offset_secs: i32) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_clock(now, offset_secs)
}

#[cfg(test)]
mod tests {
    use super::{format_clock, parse_utc_offset};

    #[test]
    fn utc_offset_parsing() {
        assert_eq!(parse_utc_offset("-0400"), Some(-14400));
        assert_eq!(parse_utc_offset("+0530"), Some(19800));
        assert_eq!(parse_utc_offset("+0000"), Some(0));
        assert_eq!(parse_utc_offset("Z"), None);
    }

    #[test]
    fn clock_formatting() {
        // 1970-01-01 00:00:00 UTC, offset 0 → 12:00 AM
        assert_eq!(format_clock(0, 0), "12:00 AM");
        // 13:05 UTC → 1:05 PM
        assert_eq!(format_clock(13 * 3600 + 5 * 60, 0), "1:05 PM");
        // 12:00 UTC → 12:00 PM (noon)
        assert_eq!(format_clock(12 * 3600, 0), "12:00 PM");
        // 00:30 UTC at -04:00 → previous day 20:30 → 8:30 PM
        assert_eq!(format_clock(30 * 60, -4 * 3600), "8:30 PM");
    }
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
