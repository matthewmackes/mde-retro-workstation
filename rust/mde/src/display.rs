//! Display Properties — the Windows 2000 Display control panel, wired to
//! Wayland/sway. A tabbed property sheet (Background · Screen Saver · Appearance
//! · Effects · Settings) over the [`crate::outputs`] data layer.
//!
//! Changes preview live through `wlr-randr`/`swaybg`; an Apply (or OK) raises the classic
//! 15-second "Keep these settings?" prompt that auto-reverts if you don't
//! confirm, so a bad resolution can't lock you out. Confirmed settings persist
//! to a sway `config.d` fragment that sway replays at login.
//!
//!   mde display            opens the GUI
//!   mde display --outputs  prints the detected outputs (headless)

use std::process::ExitCode;

use iced::widget::{checkbox, container, pick_list, scrollable, text, Column, Row, Space, Stack};
use iced::{Background, Border, Color, Element, Length, Padding, Shadow, Task};

use mde_ui::{button, frame, group_box, metrics, palette};

use crate::outputs::{self, Desired, DesiredOutput, Mode, Output, ScreenSaver, Wallpaper};

const TABS: &[&str] = &[
    "Background",
    "Screen Saver",
    "Appearance",
    "Effects",
    "Settings",
];
const REVERT_SECS: u32 = 15;

// --- pick-list option types (each needs Display + PartialEq + Clone) --------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ResChoice(i32, i32);
impl std::fmt::Display for ResChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} x {}", self.0, self.1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RefreshChoice(i32); // mHz
impl std::fmt::Display for RefreshChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            Mode {
                width: 0,
                height: 0,
                refresh_mhz: self.0
            }
            .refresh_label()
        )
    }
}

/// Display scale as a whole-number percent (Eq-friendly; f64 isn't). On a
/// fixed panel this is the reliable way to change the *effective* desktop size:
/// logical resolution = native / scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScaleChoice(u32);
impl ScaleChoice {
    const ALL: [ScaleChoice; 5] = [
        ScaleChoice(100),
        ScaleChoice(125),
        ScaleChoice(150),
        ScaleChoice(175),
        ScaleChoice(200),
    ];
    fn factor(self) -> f64 {
        self.0 as f64 / 100.0
    }
    fn from_factor(s: f64) -> ScaleChoice {
        ScaleChoice((s * 100.0).round() as u32)
    }
}
impl std::fmt::Display for ScaleChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}%", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Orient {
    Landscape,
    Portrait,
    LandscapeFlipped,
    PortraitFlipped,
}
impl Orient {
    const ALL: [Orient; 4] = [
        Orient::Landscape,
        Orient::Portrait,
        Orient::LandscapeFlipped,
        Orient::PortraitFlipped,
    ];
    fn token(self) -> &'static str {
        match self {
            Orient::Landscape => "normal",
            Orient::Portrait => "90",
            Orient::LandscapeFlipped => "180",
            Orient::PortraitFlipped => "270",
        }
    }
    fn from_token(t: &str) -> Orient {
        match t {
            "90" => Orient::Portrait,
            "180" => Orient::LandscapeFlipped,
            "270" => Orient::PortraitFlipped,
            _ => Orient::Landscape,
        }
    }
}
impl std::fmt::Display for Orient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Orient::Landscape => "Landscape",
            Orient::Portrait => "Portrait (90\u{00b0})",
            Orient::LandscapeFlipped => "Landscape (flipped)",
            Orient::PortraitFlipped => "Portrait (270\u{00b0})",
        };
        f.write_str(s)
    }
}

// BgMode, the wallpaper scan, Browse, and the preview render now live in
// `crate::wallpaper` (shared with the Win10 Personalization ▸ Background page).
use crate::wallpaper::{self, BgMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scheme {
    Standard,
    HighContrastBlack,
    Brick,
    Spruce,
}
impl Scheme {
    const ALL: [Scheme; 4] = [
        Scheme::Standard,
        Scheme::HighContrastBlack,
        Scheme::Brick,
        Scheme::Spruce,
    ];
    fn key(self) -> &'static str {
        match self {
            Scheme::Standard => "win2k-standard",
            Scheme::HighContrastBlack => "high-contrast-black",
            Scheme::Brick => "win2k-brick",
            Scheme::Spruce => "win2k-spruce",
        }
    }
}
impl std::fmt::Display for Scheme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Scheme::Standard => "Windows Standard",
            Scheme::HighContrastBlack => "High Contrast Black",
            Scheme::Brick => "Brick",
            Scheme::Spruce => "Spruce",
        };
        f.write_str(s)
    }
}

/// The icon set selectable on the Appearance tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IconSet {
    Win2000,
    Haiku,
}
impl IconSet {
    const ALL: [IconSet; 2] = [IconSet::Win2000, IconSet::Haiku];
    /// The state key persisted in menu.json.
    fn key(self) -> &'static str {
        match self {
            IconSet::Win2000 => "win2k",
            IconSet::Haiku => "haiku",
        }
    }
    /// The freedesktop icon-theme directory name (for gtk-icon-theme-name).
    fn theme(self) -> &'static str {
        match self {
            IconSet::Win2000 => "Win2k",
            IconSet::Haiku => "Haiku",
        }
    }
    fn from_key(k: &str) -> Self {
        match k {
            "haiku" => IconSet::Haiku,
            _ => IconSet::Win2000,
        }
    }
}
impl std::fmt::Display for IconSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            IconSet::Win2000 => "Windows 2000 (Classic)",
            IconSet::Haiku => "Haiku",
        })
    }
}

/// The look-and-feel theme selectable on the Appearance tab (state key `theme`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Theme {
    Carbon,
    Win2000,
    Windows10,
}
impl Theme {
    const ALL: [Theme; 3] = [Theme::Carbon, Theme::Win2000, Theme::Windows10];
    fn key(self) -> &'static str {
        match self {
            Theme::Carbon => "carbon",
            Theme::Win2000 => "win2000",
            Theme::Windows10 => "windows10",
        }
    }
    fn from_key(k: &str) -> Self {
        match k {
            "win2000" => Theme::Win2000,
            "windows10" => Theme::Windows10,
            _ => Theme::Carbon,
        }
    }
}
impl std::fmt::Display for Theme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Theme::Carbon => "IBM Carbon",
            Theme::Win2000 => "Windows 2000 (Classic)",
            Theme::Windows10 => "Windows 10",
        })
    }
}

/// Carbon light/dark mode (state key `theme_mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ThemeMode {
    Dark,
    Light,
}
impl ThemeMode {
    const ALL: [ThemeMode; 2] = [ThemeMode::Dark, ThemeMode::Light];
    fn key(self) -> &'static str {
        match self {
            ThemeMode::Dark => "dark",
            ThemeMode::Light => "light",
        }
    }
    fn from_key(k: &str) -> Self {
        match k {
            "light" => ThemeMode::Light,
            _ => ThemeMode::Dark,
        }
    }
}
impl std::fmt::Display for ThemeMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ThemeMode::Dark => "Dark (Gray 90)",
            ThemeMode::Light => "Light (Gray 10)",
        })
    }
}

/// The icon accent hue (state key `icon_color`); each auto-shades per mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IconColor {
    Neutral,
    Blue,
    Orange,
    Red,
}
impl IconColor {
    const ALL: [IconColor; 4] = [
        IconColor::Neutral,
        IconColor::Blue,
        IconColor::Orange,
        IconColor::Red,
    ];
    fn key(self) -> &'static str {
        match self {
            IconColor::Neutral => "neutral",
            IconColor::Blue => "blue",
            IconColor::Orange => "orange",
            IconColor::Red => "red",
        }
    }
    fn from_key(k: &str) -> Self {
        match k {
            "blue" => IconColor::Blue,
            "orange" => IconColor::Orange,
            "red" => IconColor::Red,
            _ => IconColor::Neutral,
        }
    }
}
impl std::fmt::Display for IconColor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            IconColor::Neutral => "Neutral",
            IconColor::Blue => "Blue",
            IconColor::Orange => "Orange",
            IconColor::Red => "Red",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WaitChoice(u32); // minutes
impl std::fmt::Display for WaitChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            0 => f.write_str("(None)"),
            1 => f.write_str("1 minute"),
            n => write!(f, "{n} minutes"),
        }
    }
}
const WAIT_OPTIONS: [WaitChoice; 8] = [
    WaitChoice(0),
    WaitChoice(1),
    WaitChoice(2),
    WaitChoice(5),
    WaitChoice(10),
    WaitChoice(15),
    WaitChoice(30),
    WaitChoice(60),
];

/// Where to place the selected (non-primary) output relative to the primary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    RightOf,
    LeftOf,
    Above,
    Below,
}
impl Place {
    const ALL: [Place; 4] = [Place::RightOf, Place::LeftOf, Place::Above, Place::Below];
}
impl std::fmt::Display for Place {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Place::RightOf => "Right of primary",
            Place::LeftOf => "Left of primary",
            Place::Above => "Above primary",
            Place::Below => "Below primary",
        };
        f.write_str(s)
    }
}

// --- state -----------------------------------------------------------------

struct Display {
    tab: usize,
    live: Vec<Output>,
    desired: Vec<DesiredOutput>,
    selected: usize,
    /// Snapshot to restore on Cancel / revert (the geometry at open).
    baseline: Vec<DesiredOutput>,

    // Background
    wallpapers: Vec<String>,
    wp_selected: Option<usize>,
    bg_mode: BgMode,

    // Screen saver
    wait: WaitChoice,
    lock: bool,

    // Appearance
    scheme: Scheme,
    icon_set: IconSet,
    theme: Theme,
    theme_mode: ThemeMode,
    icon_color: IconColor,

    /// Identify overlay: flash each output's number in its preview.
    identify: bool,

    /// Revert prompt: seconds remaining and the close-on-keep flag.
    revert: Option<u32>,
    close_on_keep: bool,
}

#[derive(Debug, Clone)]
enum Message {
    SelectTab(usize),
    SelectOutput(usize),
    SetResolution(ResChoice),
    SetRefresh(RefreshChoice),
    SetOrient(Orient),
    SetScale(ScaleChoice),
    SetPlace(Place),
    Identify,

    SelectWallpaper(usize),
    SetBgMode(BgMode),
    Browse,
    Browsed(Option<String>),

    SetWait(WaitChoice),
    ToggleLock(bool),

    SetScheme(Scheme),
    SetIconSet(IconSet),
    SetTheme(Theme),
    SetThemeMode(ThemeMode),
    SetIconColor(IconColor),

    Apply,
    Ok,
    Cancel,
    KeepSettings,
    RevertNow,
    Tick,
}

pub fn run(args: &[String]) -> ExitCode {
    if args.iter().any(|a| a == "--outputs") {
        for o in outputs::query() {
            println!(
                "{}  {}  {}  {:?}  scale={}  transform={}  @({},{})",
                o.name,
                if o.active { "on" } else { "off" },
                o.label(),
                o.current.map(|m| m.res_label()),
                o.scale,
                o.transform,
                o.x,
                o.y
            );
        }
        return ExitCode::SUCCESS;
    }
    match gui() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mde display: {e}");
            ExitCode::FAILURE
        }
    }
}

fn gui() -> iced::Result {
    iced::application(|_: &Display| "Display Properties".to_string(), update, view)
        .window_size(iced::Size::new(440.0, 540.0))
        .resizable(false)
        .theme(|_| iced::Theme::Light)
        .subscription(|_: &Display| {
            iced::time::every(std::time::Duration::from_secs(1)).map(|_| Message::Tick)
        })
        .font(mde_ui::font::REGULAR_BYTES)
        .font(mde_ui::font::BOLD_BYTES)
        .font(mde_ui::font::PLEX_REGULAR_BYTES)
        .font(mde_ui::font::PLEX_BOLD_BYTES)
        .default_font(mde_ui::font::ui())
        .run_with(|| {
            let live = outputs::query();
            let desired = outputs::desired_from(&live);
            let selected = live.iter().position(|o| o.focused).unwrap_or(0);
            let wallpapers = wallpaper::scan();
            let st0 = crate::state::load();
            (
                Display {
                    tab: 4, // open on Settings (the display-manager core)
                    baseline: desired.clone(),
                    desired,
                    live,
                    selected,
                    wallpapers,
                    wp_selected: None,
                    bg_mode: BgMode::Fill,
                    wait: WaitChoice(0),
                    lock: true,
                    scheme: Scheme::Standard,
                    icon_set: IconSet::from_key(&st0.icon_set),
                    theme: Theme::from_key(&st0.theme),
                    theme_mode: ThemeMode::from_key(&st0.theme_mode),
                    icon_color: IconColor::from_key(&st0.icon_color),
                    identify: false,
                    revert: None,
                    close_on_keep: false,
                },
                Task::none(),
            )
        })
}

// --- update ----------------------------------------------------------------

fn update(state: &mut Display, message: Message) -> Task<Message> {
    match message {
        Message::SelectTab(i) => state.tab = i,
        Message::SelectOutput(i) => {
            if i < state.desired.len() {
                state.selected = i;
            }
        }
        Message::SetResolution(ResChoice(w, h)) => {
            if let Some(d) = state.desired.get_mut(state.selected) {
                d.width = w;
                d.height = h;
                // Snap refresh to one this resolution actually supports.
                if let Some(o) = state.live.get(state.selected) {
                    if let Some(best) = o.refreshes_at(w, h).first() {
                        d.refresh_mhz = best.refresh_mhz;
                    }
                }
            }
        }
        Message::SetRefresh(RefreshChoice(mhz)) => {
            if let Some(d) = state.desired.get_mut(state.selected) {
                d.refresh_mhz = mhz;
            }
        }
        Message::SetOrient(o) => {
            if let Some(d) = state.desired.get_mut(state.selected) {
                d.transform = o.token().to_string();
            }
        }
        Message::SetScale(s) => {
            if let Some(d) = state.desired.get_mut(state.selected) {
                d.scale = s.factor();
            }
        }
        Message::SetPlace(p) => place_selected(state, p),
        Message::Identify => state.identify = !state.identify,

        Message::SelectWallpaper(i) => state.wp_selected = Some(i),
        Message::SetBgMode(m) => state.bg_mode = m,
        Message::Browse => {
            return Task::perform(async { wallpaper::browse() }, Message::Browsed);
        }
        Message::Browsed(Some(path)) => {
            state.wallpapers.push(path);
            state.wp_selected = Some(state.wallpapers.len() - 1);
        }
        Message::Browsed(None) => {}

        Message::SetWait(w) => state.wait = w,
        Message::ToggleLock(b) => state.lock = b,

        Message::SetScheme(s) => state.scheme = s,
        Message::SetIconSet(set) => {
            state.icon_set = set;
            apply_icon_set(set);
        }
        Message::SetTheme(t) => {
            state.theme = t;
            apply_appearance(state);
        }
        Message::SetThemeMode(m) => {
            state.theme_mode = m;
            apply_appearance(state);
        }
        Message::SetIconColor(c) => {
            state.icon_color = c;
            apply_appearance(state);
        }

        Message::Apply => {
            ensure_backends();
            outputs::apply_live(&build_desired(state));
            state.revert = Some(REVERT_SECS);
            state.close_on_keep = false;
        }
        Message::Ok => {
            ensure_backends();
            outputs::apply_live(&build_desired(state));
            state.revert = Some(REVERT_SECS);
            state.close_on_keep = true;
        }
        Message::Cancel => {
            // Undo any live preview, then quit.
            outputs::revert_to(&baseline_desired(state));
            std::process::exit(0);
        }
        Message::KeepSettings => {
            let _ = outputs::persist(&build_desired(state));
            state.baseline = state.desired.clone();
            state.revert = None;
            if state.close_on_keep {
                std::process::exit(0);
            }
        }
        Message::RevertNow => {
            outputs::revert_to(&baseline_desired(state));
            state.desired = state.baseline.clone();
            state.revert = None;
            state.close_on_keep = false;
        }
        Message::Tick => {
            if let Some(n) = state.revert {
                if n <= 1 {
                    // Timed out — auto-revert.
                    outputs::revert_to(&baseline_desired(state));
                    state.desired = state.baseline.clone();
                    state.revert = None;
                    state.close_on_keep = false;
                } else {
                    state.revert = Some(n - 1);
                }
            }
        }
    }
    Task::none()
}

/// Build the full desired state (geometry + wallpaper + saver + scheme) the
/// Apply/persist paths consume.
fn build_desired(state: &Display) -> Desired {
    Desired {
        outputs: state.desired.clone(),
        wallpaper: state
            .wp_selected
            .and_then(|i| state.wallpapers.get(i))
            .map(|p| Wallpaper {
                path: p.clone(),
                mode: state.bg_mode.swaybg().to_string(),
            }),
        screensaver: Some(ScreenSaver {
            minutes: state.wait.0,
            lock: state.lock,
        }),
        scheme: Some(state.scheme.key().to_string()),
    }
}

/// The baseline state for revert (only geometry needs undoing live).
fn baseline_desired(state: &Display) -> Desired {
    Desired {
        outputs: state.baseline.clone(),
        ..Desired::default()
    }
}

/// Place the selected non-primary output relative to the primary (focused) one,
/// updating its logical position.
fn place_selected(state: &mut Display, p: Place) {
    let Some(primary) = state.live.iter().position(|o| o.focused) else {
        return;
    };
    if primary == state.selected {
        return;
    }
    let (px, py, pw, ph) = logical_rect(&state.desired[primary]);
    let (_, _, sw, sh) = logical_rect(&state.desired[state.selected]);
    let (nx, ny) = match p {
        Place::RightOf => (px + pw, py),
        Place::LeftOf => (px - sw, py),
        Place::Above => (px, py - sh),
        Place::Below => (px, py + ph),
    };
    let d = &mut state.desired[state.selected];
    d.x = nx;
    d.y = ny;
}

/// Logical (x, y, w, h) of a desired output, accounting for rotation + scale.
fn logical_rect(d: &DesiredOutput) -> (i32, i32, i32, i32) {
    let rotated = d.transform == "90" || d.transform == "270";
    let (mut w, mut h) = if rotated {
        (d.height, d.width)
    } else {
        (d.width, d.height)
    };
    if d.scale > 0.0 {
        w = (w as f64 / d.scale).round() as i32;
        h = (h as f64 / d.scale).round() as i32;
    }
    (d.x, d.y, w, h)
}

fn ensure_backends() {
    let missing = outputs::missing_backends();
    if !missing.is_empty() {
        let _ = crate::fedora::install(&missing);
    }
}

// --- view helpers -----------------------------------------------------------

fn pad(t: f32, r: f32, b: f32, l: f32) -> Padding {
    Padding {
        top: t,
        right: r,
        bottom: b,
        left: l,
    }
}

fn label(s: &str) -> Element<'static, Message> {
    text(s.to_string()).size(metrics::UI_PX).into()
}

fn bold(s: &str) -> Element<'static, Message> {
    text(s.to_string())
        .size(metrics::UI_PX)
        .font(mde_ui::font::ui_bold())
        .into()
}

fn tab_strip(current: usize) -> Element<'static, Message> {
    mde_ui::tab_strip(TABS, current, Message::SelectTab)
}

/// The classic monitor graphic: a raised silver bezel around a "screen" filled
/// with the wallpaper preview (or the desktop color), plus a small stand. Shows
/// the output number when Identify is on.
fn monitor_graphic<'a>(
    screen: Element<'a, Message>,
    number: Option<usize>,
) -> Element<'a, Message> {
    let inner = container(screen)
        .width(Length::Fixed(180.0))
        .height(Length::Fixed(135.0))
        .padding(2.0);
    let mut screen_stack =
        Stack::new().push(iced::widget::stack![frame::sunken().thickness(2), inner]);
    if let Some(n) = number {
        screen_stack = screen_stack.push(
            container(
                text(format!("{}", n + 1))
                    .size(metrics::IDENTIFY_PX)
                    .font(mde_ui::font::ui_bold())
                    .color(Color::WHITE),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill),
        );
    }
    let bezel = container(iced::widget::stack![
        frame::raised().thickness(2),
        container(screen_stack).padding(8.0)
    ])
    .style(|_| container::Style {
        background: Some(Background::Color(palette::color(palette::BUTTON_FACE))),
        ..container::Style::default()
    });
    let stand = container(Space::new(Length::Fixed(60.0), Length::Fixed(10.0))).style(|_| {
        container::Style {
            background: Some(Background::Color(palette::color(palette::BUTTON_FACE))),
            border: Border {
                color: palette::color(palette::BUTTON_SHADOW),
                width: 1.0,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        }
    });
    Column::new()
        .align_x(iced::Alignment::Center)
        .push(bezel)
        .push(stand)
        .into()
}

/// The preview "screen" content: the chosen wallpaper, or a flat desktop color.
fn screen_preview(state: &Display) -> Element<'static, Message> {
    let selected = state
        .wp_selected
        .and_then(|i| state.wallpapers.get(i))
        .map(String::as_str);
    wallpaper::preview(selected)
}

// --- tabs -------------------------------------------------------------------

fn background_tab(state: &Display) -> Element<'_, Message> {
    let preview = monitor_graphic(
        screen_preview(state),
        state.identify.then_some(state.selected),
    );

    let mut list = Column::new().spacing(0.0);
    if state.wallpapers.is_empty() {
        list = list.push(
            container(label("(no images found in Pictures / backgrounds)"))
                .padding(pad(2.0, 4.0, 2.0, 4.0)),
        );
    }
    for (i, wp) in state.wallpapers.iter().enumerate() {
        let name = std::path::Path::new(wp)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(wp);
        list = list.push(
            iced::widget::button(text(name.to_string()).size(metrics::UI_PX))
                .on_press(Message::SelectWallpaper(i))
                .width(Length::Fill)
                .padding(pad(2.0, 6.0, 2.0, 6.0))
                .style(row_style(state.wp_selected == Some(i))),
        );
    }
    let well = iced::widget::stack![
        frame::sunken().face(palette::color(palette::WINDOW)),
        container(scrollable(list).style(mde_ui::scrollbar)).padding(2.0),
    ];

    let controls = Column::new()
        .spacing(8.0)
        .push(bold(
            "Select a background picture or HTML document as Wallpaper:",
        ))
        .push(container(well).height(Length::Fixed(150.0)))
        .push(
            Row::new()
                .spacing(8.0)
                .align_y(iced::Alignment::Center)
                .push(button(text("Browse\u{2026}").size(metrics::UI_PX)).on_press(Message::Browse))
                .push(Space::with_width(Length::Fill))
                .push(label("Picture Position:"))
                .push(
                    pick_list(
                        BgMode::ALL.to_vec(),
                        Some(state.bg_mode),
                        Message::SetBgMode,
                    )
                    .style(mde_ui::sunken_picklist)
                    .text_size(metrics::UI_PX),
                ),
        );

    Column::new()
        .spacing(12.0)
        .push(
            container(preview)
                .width(Length::Fill)
                .center_x(Length::Fill),
        )
        .push(controls)
        .into()
}

fn screensaver_tab(state: &Display) -> Element<'_, Message> {
    let preview = monitor_graphic(screen_preview(state), None);
    let group = Column::new()
        .spacing(8.0)
        .push(
            Row::new()
                .spacing(8.0)
                .align_y(iced::Alignment::Center)
                .push(label("Wait:"))
                .push(
                    pick_list(WAIT_OPTIONS.to_vec(), Some(state.wait), Message::SetWait)
                        .style(mde_ui::sunken_picklist)
                        .text_size(metrics::UI_PX),
                ),
        )
        .push(
            checkbox("On resume, password protect (swaylock)", state.lock)
                .on_toggle(Message::ToggleLock)
                .style(mde_ui::checkbox_style)
                .text_size(metrics::UI_PX),
        )
        .push(label(if outputs::have("swayidle") {
            "Uses swayidle to blank the screen (and swaylock to lock) after the wait."
        } else {
            "swayidle is not installed \u{2014} Apply will offer to install it."
        }));

    Column::new()
        .spacing(12.0)
        .push(
            container(preview)
                .width(Length::Fill)
                .center_x(Length::Fill),
        )
        .push(group_box("Energy saving / screen saver", group))
        .into()
}

/// Persist the icon set and apply the whole paired theme — GTK icon theme + UI
/// font, the labwc title colour — then restart the shell so every surface
/// (taskbar + GTK apps) adopts it.
/// Persist the Carbon theme/mode/icon-color and restart the shell so the
/// taskbar and every app process re-read the palette + icon tint. Also recolors
/// the labwc window frame to match (Carbon header gray vs Win2000 navy).
fn apply_appearance(state: &Display) {
    let mut st = crate::state::load();
    st.theme = state.theme.key().to_string();
    st.theme_mode = state.theme_mode.key().to_string();
    st.icon_color = state.icon_color.key().to_string();
    let _ = crate::state::save(&st);
    // Window-frame color: Carbon uses a flat header (Gray 100 dark / white light),
    // Win2000 keeps navy, Windows 10 a flat neutral header (#1f1f1f dark / white
    // light; accent-on-titlebar is E2/E7). These are labwc themerc config strings,
    // not iced colors — the same §2.1-exempt precedent as the arms above. Reuse
    // the labwc themerc rewriter.
    let (bg, fg) = match (state.theme, state.theme_mode) {
        (Theme::Carbon, ThemeMode::Dark) => ("#161616", "#f4f4f4"),
        (Theme::Carbon, ThemeMode::Light) => ("#ffffff", "#161616"),
        (Theme::Win2000, _) => ("#0a246a", "#ffffff"),
        (Theme::Windows10, ThemeMode::Dark) => ("#1f1f1f", "#ffffff"),
        (Theme::Windows10, ThemeMode::Light) => ("#ffffff", "#1f1f1f"),
    };
    set_labwc_title_colors(bg, fg);
    restart_shell();
}

/// Kill every mde surface (incl. this Display window) and relaunch the taskbar,
/// detached so it outlives the `pkill`. Shared by the appearance appliers.
fn restart_shell() {
    if let Ok(exe) = std::env::current_exe() {
        let exe = exe.to_string_lossy().to_string();
        let _ = std::process::Command::new("setsid")
            .arg("sh")
            .arg("-c")
            .arg(format!(
                "sleep 0.3; pkill -x mde; sleep 0.4; exec '{exe}' panel"
            ))
            .spawn();
    }
}

fn apply_icon_set(set: IconSet) {
    let beos = set == IconSet::Haiku;
    let mut st = crate::state::load();
    st.icon_set = set.key().to_string();
    let _ = crate::state::save(&st);
    gtk_settings("gtk-icon-theme-name", Some(set.theme()));
    // Desktop-wide BeOS font: make sure the embedded IBM Plex Sans is on disk for
    // GTK/fontconfig, then point GTK at it (Windows 2000 reverts to the default).
    if beos {
        ensure_plex_installed();
        gtk_settings("gtk-font-name", Some("IBM Plex Sans 10"));
    } else {
        gtk_settings("gtk-font-name", None);
    }
    set_labwc_title(beos);
    restart_shell();
}

/// Recolor the labwc active title bar for the theme — BeOS yellow tab with black
/// text, or the Windows 2000 navy with white text — then reconfigure labwc live.
fn set_labwc_title(beos: bool) {
    let (bg, fg) = if beos {
        ("#ffd700", "#000000")
    } else {
        ("#0a246a", "#ffffff")
    };
    set_labwc_title_colors(bg, fg);
}

/// Rewrite the labwc active-title bg/text colors in the themerc and reconfigure
/// labwc live. Shared by the icon-set (BeOS/Win2000) and Carbon appliers.
fn set_labwc_title_colors(bg: &str, fg: &str) {
    let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
        return;
    };
    let path = home.join(".local/share/themes/Win2000-MDE/openbox-3/themerc");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    let mut out = String::new();
    for line in text.lines() {
        let t = line.trim_start();
        if t.starts_with("window.active.title.bg.color:") {
            out.push_str(&format!("window.active.title.bg.color: {bg}\n"));
        } else if t.starts_with("window.active.label.text.color:") {
            out.push_str(&format!("window.active.label.text.color: {fg}\n"));
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    if std::fs::write(&path, out).is_ok() {
        let _ = std::process::Command::new("labwc")
            .arg("--reconfigure")
            .spawn();
    }
}

/// Set (`Some`) or remove (`None`) a key in the GTK 3 + GTK 4 settings.ini,
/// so GTK apps follow the shell's icon theme and font.
fn gtk_settings(key: &str, value: Option<&str>) {
    let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
        return;
    };
    for ver in ["gtk-3.0", "gtk-4.0"] {
        let path = home.join(".config").join(ver).join("settings.ini");
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        let mut lines: Vec<String> = Vec::new();
        let mut has_header = false;
        for line in existing.lines() {
            let t = line.trim_start();
            if t.starts_with("[Settings]") {
                has_header = true;
            }
            // Drop any existing line for this exact key (key followed by '=').
            if t.starts_with(key) && t[key.len()..].trim_start().starts_with('=') {
                continue;
            }
            lines.push(line.to_string());
        }
        if !has_header {
            lines.insert(0, "[Settings]".to_string());
        }
        if let Some(v) = value {
            let pos = lines
                .iter()
                .position(|l| l.trim_start().starts_with("[Settings]"))
                .map(|i| i + 1)
                .unwrap_or(0);
            lines.insert(pos, format!("{key} = {v}"));
        }
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&path, lines.join("\n") + "\n");
    }
}

/// Write the embedded IBM Plex Sans faces to `~/.local/share/fonts` (if absent)
/// so GTK/fontconfig apps can resolve "IBM Plex Sans", then refresh the cache.
fn ensure_plex_installed() {
    let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
        return;
    };
    let dir = home.join(".local/share/fonts");
    let _ = std::fs::create_dir_all(&dir);
    let faces: [(&str, &[u8]); 2] = [
        ("IBMPlexSans-Regular.ttf", mde_ui::font::PLEX_REGULAR_BYTES),
        ("IBMPlexSans-Bold.ttf", mde_ui::font::PLEX_BOLD_BYTES),
    ];
    let mut wrote = false;
    for (name, bytes) in faces {
        let p = dir.join(name);
        if !p.exists() && std::fs::write(&p, bytes).is_ok() {
            wrote = true;
        }
    }
    if wrote {
        let _ = std::process::Command::new("fc-cache")
            .arg("-f")
            .arg(&dir)
            .spawn();
    }
}

fn appearance_tab(state: &Display) -> Element<'_, Message> {
    // A mock window preview, recolored by the chosen scheme.
    let (border_hex, _bg, _txt) = outputs::scheme_colors(state.scheme.key());
    let title_color = parse_hex(border_hex);
    let title = container(
        text("Active Window")
            .size(metrics::UI_PX)
            .font(mde_ui::font::ui_bold())
            .color(Color::WHITE),
    )
    .width(Length::Fill)
    .padding(pad(2.0, 6.0, 2.0, 6.0))
    .style(move |_| container::Style {
        background: Some(Background::Color(title_color)),
        ..container::Style::default()
    });
    let mock = iced::widget::stack![
        frame::raised().thickness(2),
        Column::new()
            .push(title)
            .push(container(label("Window Text")).padding(8.0))
    ];
    let preview = container(mock)
        .width(Length::Fixed(220.0))
        .height(Length::Fixed(90.0));

    let group = Column::new()
        .spacing(8.0)
        .push(
            Row::new()
                .spacing(8.0)
                .align_y(iced::Alignment::Center)
                .push(label("Scheme:"))
                .push(pick_list(Scheme::ALL.to_vec(), Some(state.scheme), Message::SetScheme).style(mde_ui::sunken_picklist).text_size(metrics::UI_PX)),
        )
        .push(
            Row::new()
                .spacing(8.0)
                .align_y(iced::Alignment::Center)
                .push(label("Icon set:"))
                .push(pick_list(IconSet::ALL.to_vec(), Some(state.icon_set), Message::SetIconSet).style(mde_ui::sunken_picklist).text_size(metrics::UI_PX)),
        )
        .push(
            Row::new()
                .spacing(8.0)
                .align_y(iced::Alignment::Center)
                .push(label("Theme:"))
                .push(pick_list(Theme::ALL.to_vec(), Some(state.theme), Message::SetTheme).style(mde_ui::sunken_picklist).text_size(metrics::UI_PX)),
        )
        .push(
            Row::new()
                .spacing(8.0)
                .align_y(iced::Alignment::Center)
                .push(label("Mode:"))
                .push(pick_list(ThemeMode::ALL.to_vec(), Some(state.theme_mode), Message::SetThemeMode).style(mde_ui::sunken_picklist).text_size(metrics::UI_PX)),
        )
        .push(
            Row::new()
                .spacing(8.0)
                .align_y(iced::Alignment::Center)
                .push(label("Icon colour:"))
                .push(pick_list(IconColor::ALL.to_vec(), Some(state.icon_color), Message::SetIconColor).style(mde_ui::sunken_picklist).text_size(metrics::UI_PX)),
        )
        .push(label("Theme sets the look-and-feel: IBM Carbon (flat, Plex font, light/dark) or Windows 2000 (classic 3D). Mode picks Carbon's light/dark. Icon colour tints the shell icons (each hue auto-shades for the mode). Changing any of these restarts the shell. The Scheme/Icon set above pair with the classic theme."));

    Column::new()
        .spacing(12.0)
        .push(
            container(preview)
                .width(Length::Fill)
                .center_x(Length::Fill),
        )
        .push(group_box("Appearance", group))
        .into()
}

fn effects_tab(_state: &Display) -> Element<'_, Message> {
    // labwc has no compositor effect engine, so these have no live effect and
    // aren't persisted. Render them greyed (no on_toggle) for fidelity rather
    // than as enabled toggles that silently discard their state.
    let fx = |text: &str, on: bool| {
        checkbox(text.to_string(), on)
            .style(mde_ui::checkbox_style)
            .text_size(metrics::UI_PX)
    };
    let group = Column::new()
        .spacing(8.0)
        .push(fx("Use transition effects for menus and tooltips", true))
        .push(fx("Show window contents while dragging", true))
        .push(fx("Use large icons", false))
        .push(label("Note: labwc has no compositor effect engine, so these are shown greyed for fidelity — no live visual effect."));
    Column::new()
        .spacing(12.0)
        .push(group_box("Visual effects", group))
        .into()
}

fn settings_tab(state: &Display) -> Element<'_, Message> {
    let preview = monitor_graphic(
        screen_preview(state),
        state.identify.then_some(state.selected),
    );

    // Output picker (the "Display:" dropdown), when more than one.
    let mut head = Column::new().spacing(8.0);
    if state.desired.len() > 1 {
        let mut row = Row::new()
            .spacing(4.0)
            .align_y(iced::Alignment::Center)
            .push(label("Display:"));
        for (i, o) in state.live.iter().enumerate() {
            row = row.push(
                button(text(format!("{}. {}", i + 1, o.name)).size(metrics::UI_PX))
                    .active(i == state.selected)
                    .on_press(Message::SelectOutput(i)),
            );
        }
        head = head.push(row);
    }

    let controls: Element<Message> = match (
        state.live.get(state.selected),
        state.desired.get(state.selected),
    ) {
        (Some(o), Some(d)) => {
            let res_opts: Vec<ResChoice> = o
                .resolutions()
                .into_iter()
                .map(|(w, h)| ResChoice(w, h))
                .collect();
            let cur_res = ResChoice(d.width, d.height);
            let refresh_opts: Vec<RefreshChoice> = o
                .refreshes_at(d.width, d.height)
                .into_iter()
                .map(|m| RefreshChoice(m.refresh_mhz))
                .collect();

            let area = Row::new()
                .spacing(8.0)
                .align_y(iced::Alignment::Center)
                .push(label("Screen area:"))
                .push(
                    pick_list(res_opts, Some(cur_res), Message::SetResolution)
                        .style(mde_ui::sunken_picklist)
                        .text_size(metrics::UI_PX),
                );

            let refresh = Row::new()
                .spacing(8.0)
                .align_y(iced::Alignment::Center)
                .push(label("Refresh rate:"))
                .push(
                    pick_list(
                        refresh_opts,
                        Some(RefreshChoice(d.refresh_mhz)),
                        Message::SetRefresh,
                    )
                    .style(mde_ui::sunken_picklist)
                    .text_size(metrics::UI_PX),
                );

            let orient = Row::new()
                .spacing(8.0)
                .align_y(iced::Alignment::Center)
                .push(label("Orientation:"))
                .push(
                    pick_list(
                        Orient::ALL.to_vec(),
                        Some(Orient::from_token(&d.transform)),
                        Message::SetOrient,
                    )
                    .style(mde_ui::sunken_picklist)
                    .text_size(metrics::UI_PX),
                );

            // Scale: the effective "screen area" on a fixed panel. Show the
            // resulting logical resolution next to the percentage.
            let (eff_w, eff_h) = if d.scale > 0.0 {
                (
                    (d.width as f64 / d.scale).round() as i32,
                    (d.height as f64 / d.scale).round() as i32,
                )
            } else {
                (d.width, d.height)
            };
            let scale = Row::new()
                .spacing(8.0)
                .align_y(iced::Alignment::Center)
                .push(label("Scale:"))
                .push(
                    pick_list(
                        ScaleChoice::ALL.to_vec(),
                        Some(ScaleChoice::from_factor(d.scale)),
                        Message::SetScale,
                    )
                    .style(mde_ui::sunken_picklist)
                    .text_size(metrics::UI_PX),
                )
                .push(label(&format!("\u{2192} {eff_w} x {eff_h}")));

            // Colors: present but fixed at True Color (Wayland is always 32-bit).
            let colors = Row::new()
                .spacing(8.0)
                .align_y(iced::Alignment::Center)
                .push(label("Colors:"))
                .push(
                    container(label("True Color (32 bit)"))
                        .padding(pad(1.0, 6.0, 1.0, 6.0))
                        .style(|_| container::Style {
                            background: Some(Background::Color(palette::color(
                                palette::BUTTON_FACE,
                            ))),
                            text_color: Some(palette::color(palette::GRAY_TEXT)),
                            border: Border {
                                color: palette::color(palette::BUTTON_SHADOW),
                                width: 1.0,
                                radius: 0.0.into(),
                            },
                            ..container::Style::default()
                        }),
                );

            let mut col = Column::new()
                .spacing(8.0)
                .push(area)
                .push(refresh)
                .push(orient)
                .push(scale)
                .push(colors);

            // Multi-monitor placement of the selected (non-primary) output.
            if state.desired.len() > 1 && !o.focused {
                col = col.push(
                    Row::new()
                        .spacing(8.0)
                        .align_y(iced::Alignment::Center)
                        .push(label("Position:"))
                        .push(
                            pick_list(Place::ALL.to_vec(), None::<Place>, Message::SetPlace)
                                .style(mde_ui::sunken_picklist)
                                .text_size(metrics::UI_PX),
                        ),
                );
            }
            col.into()
        }
        _ => label("No displays detected."),
    };

    let identify = button(text("Identify").size(metrics::UI_PX)).on_press(Message::Identify);

    Column::new()
        .spacing(12.0)
        .push(
            container(preview)
                .width(Length::Fill)
                .center_x(Length::Fill),
        )
        .push(head)
        .push(group_box("Display", controls))
        .push(
            Row::new()
                .push(Space::with_width(Length::Fill))
                .push(identify),
        )
        .into()
}

fn tab_content(state: &Display) -> Element<'_, Message> {
    match state.tab {
        0 => background_tab(state),
        1 => screensaver_tab(state),
        2 => appearance_tab(state),
        3 => effects_tab(state),
        4 => settings_tab(state),
        _ => label("?"),
    }
}

fn row_style(
    selected: bool,
) -> impl Fn(&iced::Theme, iced::widget::button::Status) -> iced::widget::button::Style {
    move |_t, status| {
        let hot = selected
            || matches!(
                status,
                iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed
            );
        iced::widget::button::Style {
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

fn parse_hex(s: &str) -> Color {
    let h = s.trim_start_matches('#');
    let r = u8::from_str_radix(&h[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&h[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&h[4..6], 16).unwrap_or(0);
    Color::from_rgb8(r, g, b)
}

fn view(state: &Display) -> Element<'_, Message> {
    let panel = iced::widget::stack![
        frame::raised(),
        container(tab_content(state))
            .padding(12.0)
            .width(Length::Fill)
            .height(Length::Fill),
    ];

    let buttons = Row::new()
        .spacing(8.0)
        .push(Space::with_width(Length::Fill))
        .push(
            button(text("OK").size(metrics::UI_PX))
                .on_press(Message::Ok)
                .default(true)
                .width(Length::Fixed(80.0)),
        )
        .push(
            button(text("Cancel").size(metrics::UI_PX))
                .on_press(Message::Cancel)
                .width(Length::Fixed(80.0)),
        )
        .push(
            button(text("Apply").size(metrics::UI_PX))
                .on_press(Message::Apply)
                .width(Length::Fixed(80.0)),
        );

    let body = Column::new()
        .spacing(6.0)
        .padding(pad(6.0, 10.0, 10.0, 10.0))
        .push(tab_strip(state.tab))
        .push(container(panel).height(Length::Fill))
        .push(buttons);

    let root = container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(palette::color(palette::MENU))),
            ..container::Style::default()
        });

    let mut layers = Stack::new().push(root);
    if let Some(secs) = state.revert {
        layers = layers.push(revert_dialog(secs));
    }
    layers.into()
}

/// The modal "Keep these settings?" confirm with its countdown.
fn revert_dialog(secs: u32) -> Element<'static, Message> {
    let body = Column::new()
        .spacing(10.0)
        .align_x(iced::Alignment::Center)
        .padding(pad(16.0, 16.0, 12.0, 16.0))
        .push(bold("Display Settings"))
        .push(label("Your desktop has been reconfigured."))
        .push(label(&format!(
            "Do you want to keep these settings? Reverting in {secs} seconds\u{2026}"
        )))
        .push(
            Row::new()
                .spacing(8.0)
                .push(
                    button(text("Yes").size(metrics::UI_PX))
                        .on_press(Message::KeepSettings)
                        .width(Length::Fixed(70.0)),
                )
                .push(
                    button(text("No").size(metrics::UI_PX))
                        .on_press(Message::RevertNow)
                        .width(Length::Fixed(70.0)),
                ),
        );
    let panel = container(iced::widget::stack![frame::raised(), container(body)])
        .width(Length::Fixed(340.0))
        .height(Length::Fixed(150.0));
    // A dim catcher behind the dialog (also closes nothing — modal).
    Stack::new()
        .push(
            container(Space::new(Length::Fill, Length::Fill)).style(|_| container::Style {
                background: Some(Background::Color(Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.35,
                })),
                ..container::Style::default()
            }),
        )
        .push(
            container(panel)
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill),
        )
        .into()
}
