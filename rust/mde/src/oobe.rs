//! Windows 10-era Out-Of-Box Experience (OOBE) — the first-run setup wizard (E11).
//!
//! A full-screen, multi-stage flow reached by `mde setup --era=win10`. The classic
//! Win2000-blue component-picker Setup (`installer.rs`) is **unchanged**: the
//! `--era=win10` branch in `installer::dispatch` is purely additive. The OOBE forces
//! the Windows 10 palette for its chrome and, on finish, stamps `state.oobe_done` so
//! it shows once (re-run it any time with `--force`).
//!
//! Each stage collects one choice and a **Yes/Next** advances; the backend writes
//! (locale, keymap, …) are built as commands that are *echoed* under `--dry-run`
//! and run otherwise — so the flow is testable without mutating the host.

use std::process::{Command, ExitCode};

use iced::widget::{
    button as ibutton, container, scrollable, text, text_input, Column, Row, Space,
};
use iced::{
    gradient::Linear, Background, Color, Element, Gradient, Length, Padding, Radians, Task,
};

use mde_ui::palette::Theme;
use mde_ui::{font, metrics, palette};

/// Region choices: a display country + the locale it sets (`localectl set-locale`).
const REGIONS: &[(&str, &str)] = &[
    ("United States", "en_US.UTF-8"),
    ("United Kingdom", "en_GB.UTF-8"),
    ("Canada", "en_CA.UTF-8"),
    ("Australia", "en_AU.UTF-8"),
    ("Germany", "de_DE.UTF-8"),
    ("France", "fr_FR.UTF-8"),
    ("Spain", "es_ES.UTF-8"),
    ("Italy", "it_IT.UTF-8"),
];

/// The wizard stages, in order. Only the implemented stages exist here (no stub
/// arms, §3); later stages are added as they land.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Region,
    Keyboard,
    SecondKeyboard,
    Network,
    Account,
    Pin,
    Privacy,
    YourPhone,
    Personalize,
    Finalize,
}

/// The canonical stage order. The *live* flow (`Oobe::flow`) is this minus any stage
/// that doesn't apply (e.g. Network is dropped when a wired link is already up,
/// E11.4), so adding a stage is a one-line edit and navigation can't desync.
const FLOW: &[Stage] = &[
    Stage::Region,
    Stage::Keyboard,
    Stage::SecondKeyboard,
    Stage::Network,
    Stage::Account,
    Stage::Pin,
    Stage::Privacy,
    Stage::YourPhone,
    Stage::Personalize,
    Stage::Finalize,
];

/// The stage after `s` in `flow`, or `None` at the end.
fn flow_next(flow: &[Stage], s: Stage) -> Option<Stage> {
    let i = flow.iter().position(|&x| x == s)?;
    flow.get(i + 1).copied()
}
/// The stage before `s` in `flow`, or `None` at the start.
fn flow_prev(flow: &[Stage], s: Stage) -> Option<Stage> {
    let i = flow.iter().position(|&x| x == s)?;
    i.checked_sub(1).and_then(|j| flow.get(j).copied())
}

/// The four UI accent choices (Personalize, E11.9) — the icon_color keys + their
/// Win10 accent swatch (`palette::icon_accent`, the one accent edge).
const ACCENTS: &[(&str, &str)] = &[
    ("blue", "Blue"),
    ("orange", "Orange"),
    ("red", "Red"),
    ("neutral", "Neutral"),
];

struct Oobe {
    stage: Stage,
    /// The live stage order (FLOW minus skipped stages, e.g. Network when wired).
    flow: Vec<Stage>,
    /// Echo backend commands instead of running them (`--dry-run`).
    dry: bool,
    region: usize,
    layout: usize,
    /// Privacy stage (E11.7): the four toggles, seeded on (Win10 defaults).
    p_location: bool,
    p_diagnostics: bool,
    p_find: bool,
    p_ads: bool,
    /// Second-keyboard stage (E11.3): an optional additional layout (None = skipped).
    second_layout: Option<usize>,
    /// Network stage (E11.4): the scanned Wi-Fi list, the chosen SSID + its password.
    wifis: Vec<crate::nm::Wifi>,
    wifi_sel: Option<usize>,
    wifi_pw: String,
    /// Account stage (E11.5): the local-account fields.
    username: String,
    password: String,
    password2: String,
    /// Your-Phone stage (E11.8): the phone number the user types (informational —
    /// real pairing is the Your Phone app's job once `mde connect` lands).
    phone: String,
    /// Personalize stage (E11.9): accent index into ACCENTS + light/dark.
    accent: usize,
    light: bool,
}

#[derive(Debug, Clone)]
enum Msg {
    PickRegion(usize),
    PickLayout(usize),
    PickSecond(usize),
    SelectWifi(usize),
    WifiPw(String),
    Username(String),
    Password(String),
    Password2(String),
    TogglePrivacy(u8),
    PickAccent(usize),
    SetMode(bool), // true = light
    Phone(String),
    /// Advance without committing the current stage (Skip / Do-it-later).
    Skip,
    Next,
    Back,
    Finish,
}

/// White text on the blue OOBE chrome. Uses the on-accent white sentinel
/// (`HIGHLIGHT_TEXT`), which remaps to pure white under the forced Win10 palette —
/// unlike `WINDOW`, which is the (dark) window surface there.
fn white() -> Color {
    palette::color(palette::HIGHLIGHT_TEXT)
}
fn dim() -> Color {
    palette::color(palette::SETUP_SUBTITLE)
}

/// Index of the REGION whose locale matches `$LANG`, else United States (0).
fn detected_region() -> usize {
    let lang = std::env::var("LANG").unwrap_or_default();
    let head = lang.split('.').next().unwrap_or("");
    REGIONS
        .iter()
        .position(|(_, loc)| loc.split('.').next() == Some(head) && !head.is_empty())
        .unwrap_or(0)
}

/// Index of the keyboard LAYOUT matching the detected locale's country, else US (0).
fn detected_layout() -> usize {
    // Map the detected region's locale country to a layout code (en_US → us, …).
    let (_, loc) = REGIONS[detected_region()];
    let cc = loc
        .split('_')
        .nth(1)
        .and_then(|s| s.split('.').next())
        .unwrap_or("US")
        .to_lowercase();
    crate::keyboard::LAYOUTS
        .iter()
        .position(|(code, _)| *code == cc || (cc == "us" && *code == "us"))
        .unwrap_or(0)
}

/// Is a wired (ethernet) connection already up? Then the OOBE skips the Network
/// stage (E11.4) — a live `802-3-ethernet` in `nm::active_connections`.
fn is_wired() -> bool {
    crate::nm::active_connections()
        .iter()
        .any(|c| c.kind.contains("ethernet") && c.state == "activated")
}

pub fn run(args: &[String]) -> ExitCode {
    let dry = args.iter().any(|a| a == "--dry-run");
    let force = args.iter().any(|a| a == "--force");

    // The OOBE renders in Windows 10 chrome regardless of the persisted theme.
    palette::set_theme(Theme::Windows10);
    palette::set_dark(true);

    // Show once: a completed OOBE is skipped unless re-run with --force (E11.10).
    let st = crate::state::load();
    if st.oobe_done && !force {
        return ExitCode::SUCCESS;
    }

    // No compositor (or an explicit --tui) → text mode. On a real console this is the
    // interactive ratatui Region/Keyboard picker (E11.2); when stdout/stdin isn't a TTY
    // (scripts, CI, the capture harness) it's the non-interactive walkthrough that
    // applies detected defaults — so the path never blocks waiting on a keypress.
    let tui = args.iter().any(|a| a == "--tui");
    if tui || std::env::var_os("WAYLAND_DISPLAY").is_none() {
        use std::io::IsTerminal;
        if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
            return interactive::run(dry);
        }
        return headless(dry);
    }

    // `--stage <name>` starts the wizard at a given stage — a capture seam (so the
    // accuracy gallery can grab each screen without injecting clicks to advance).
    let stage = args
        .iter()
        .position(|a| a == "--stage")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| match s.as_str() {
            "region" => Some(Stage::Region),
            "keyboard" => Some(Stage::Keyboard),
            "secondkeyboard" => Some(Stage::SecondKeyboard),
            "network" => Some(Stage::Network),
            "account" => Some(Stage::Account),
            "pin" => Some(Stage::Pin),
            "privacy" => Some(Stage::Privacy),
            "yourphone" => Some(Stage::YourPhone),
            "personalize" => Some(Stage::Personalize),
            "finalize" => Some(Stage::Finalize),
            _ => None,
        })
        .unwrap_or(Stage::Region);

    // A live wired link skips the Network stage (E11.4): drop it from the flow.
    let wired = is_wired();
    let flow: Vec<Stage> = FLOW
        .iter()
        .copied()
        .filter(|s| !(*s == Stage::Network && wired))
        .collect();
    // Scan Wi-Fi once up front only when the Network stage will be shown.
    let wifis = if wired {
        Vec::new()
    } else {
        crate::nm::wifi_list()
    };

    let init = Oobe {
        stage,
        flow,
        dry,
        region: detected_region(),
        layout: detected_layout(),
        second_layout: None,
        wifis,
        wifi_sel: None,
        wifi_pw: String::new(),
        username: String::new(),
        password: String::new(),
        password2: String::new(),
        phone: String::new(),
        p_location: st.privacy_location,
        p_diagnostics: st.privacy_diagnostics,
        p_find: st.privacy_find_device,
        p_ads: st.privacy_ads,
        accent: ACCENTS
            .iter()
            .position(|(k, _)| *k == st.icon_color)
            .unwrap_or(0),
        light: st.theme_mode == "light",
    };
    let r = iced::application(|_: &Oobe| "MDE-Retro Setup".to_string(), update, view)
        .window_size(iced::Size::new(720.0, 540.0))
        .resizable(false)
        .font(font::REGULAR_BYTES)
        .font(font::BOLD_BYTES)
        .font(font::PLEX_REGULAR_BYTES)
        .font(font::PLEX_BOLD_BYTES)
        .default_font(font::ui())
        .run_with(move || (init, Task::none()));
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}

/// Headless walkthrough (`--tui`/no compositor): apply the detected region + layout
/// (echoing under `--dry-run`), stamp `oobe_done`, and print each step so the path
/// is observable without a display.
fn headless(dry: bool) -> ExitCode {
    let region = detected_region();
    let layout = detected_layout();
    println!("MDE-Retro Windows 10 setup (headless)");
    println!("  Region:   {}", REGIONS[region].0);
    println!("  Keyboard: {}", crate::keyboard::LAYOUTS[layout].1);
    apply_locale(REGIONS[region].1, dry);
    apply_keymap(crate::keyboard::LAYOUTS[layout].0, dry);
    finish(dry);
    println!(
        "  Done. (oobe_done set{})",
        if dry { ", dry-run" } else { "" }
    );
    ExitCode::SUCCESS
}

/// Interactive text-mode Region + Keyboard picker (E11.2) for `mde setup --era=win10
/// --tui` on a real console — arrow-key single-select, pre-highlighted to the detected
/// values, applying the same `localectl`/`keyboard::apply_layout` backends as the GUI
/// (echoed under `--dry-run`). The non-TTY fallback stays [`headless`] (auto-defaults).
mod interactive {
    use super::{apply_keymap, apply_locale, detected_layout, detected_region, finish, REGIONS};
    use crossterm::event::{self, Event, KeyCode};
    use ratatui::layout::{Alignment, Constraint, Layout};
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Paragraph};
    use ratatui::Frame;
    use std::process::ExitCode;

    const BG: Color = Color::Indexed(17); // deep blue field
    const ACCENT: Color = Color::Indexed(33); // Win10 selection blue
    const BAR: Color = Color::Indexed(19);

    /// A single-select cursor over `len` rows. Pure + unit-tested; the ratatui event
    /// loop only mutates state through this, so the navigation logic is verifiable
    /// without a TTY.
    pub(super) struct Picker {
        pub(super) len: usize,
        pub(super) cursor: usize,
    }

    impl Picker {
        pub(super) fn new(len: usize, start: usize) -> Self {
            Self {
                len,
                cursor: if len == 0 { 0 } else { start.min(len - 1) },
            }
        }

        /// Move the cursor by `delta`, clamped to `[0, len-1]` (no wrap). A no-op on an
        /// empty list.
        pub(super) fn move_cursor(&mut self, delta: isize) {
            if self.len == 0 {
                return;
            }
            let max = (self.len - 1) as isize;
            self.cursor = (self.cursor as isize + delta).clamp(0, max) as usize;
        }
    }

    /// Which picker the flow is showing.
    #[derive(PartialEq)]
    enum Phase {
        Region,
        Keyboard,
    }

    pub(super) fn run(dry: bool) -> ExitCode {
        let mut terminal = ratatui::init();
        let res = event_loop(&mut terminal, dry);
        ratatui::restore();
        res
    }

    fn event_loop(terminal: &mut ratatui::DefaultTerminal, dry: bool) -> ExitCode {
        let layouts = crate::keyboard::LAYOUTS;
        let mut phase = Phase::Region;
        let mut region = Picker::new(REGIONS.len(), detected_region());
        let mut keyboard = Picker::new(layouts.len(), detected_layout());
        loop {
            let _ = terminal.draw(|f| draw(f, &phase, &region, &keyboard));
            let Ok(Event::Key(k)) = event::read() else {
                continue;
            };
            // Ignore key-release events (crossterm reports both on some terminals).
            if k.kind != crossterm::event::KeyEventKind::Press {
                continue;
            }
            let active = if phase == Phase::Region {
                &mut region
            } else {
                &mut keyboard
            };
            match k.code {
                KeyCode::Up => active.move_cursor(-1),
                KeyCode::Down => active.move_cursor(1),
                KeyCode::Enter if phase == Phase::Region => phase = Phase::Keyboard,
                KeyCode::Enter => {
                    // Keyboard confirmed → apply both choices and finish.
                    apply_locale(REGIONS[region.cursor].1, dry);
                    apply_keymap(layouts[keyboard.cursor].0, dry);
                    finish(dry);
                    return ExitCode::SUCCESS;
                }
                KeyCode::Backspace if phase == Phase::Keyboard => phase = Phase::Region,
                KeyCode::Esc | KeyCode::F(3) => return ExitCode::from(1),
                _ => {}
            }
        }
    }

    fn draw(f: &mut Frame, phase: &Phase, region: &Picker, keyboard: &Picker) {
        let layouts = crate::keyboard::LAYOUTS;
        let area = f.area();
        f.render_widget(Block::default().style(Style::default().bg(BG)), area);
        let rows = Layout::vertical([
            Constraint::Length(2), // title
            Constraint::Min(1),    // list
            Constraint::Length(1), // hint bar
        ])
        .split(area);

        let (title, items, cur): (&str, Vec<&str>, usize) = match phase {
            Phase::Region => (
                "Choose your region",
                REGIONS.iter().map(|(n, _)| *n).collect(),
                region.cursor,
            ),
            Phase::Keyboard => (
                "Choose your keyboard layout",
                layouts.iter().map(|(_, d)| *d).collect(),
                keyboard.cursor,
            ),
        };

        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                title,
                Style::default()
                    .fg(Color::White)
                    .bg(BG)
                    .add_modifier(Modifier::BOLD),
            )))
            .alignment(Alignment::Center),
            rows[0],
        );

        // Window the list around the cursor so it stays visible on short terminals.
        let h = rows[1].height as usize;
        let offset = if h > 0 && cur >= h { cur + 1 - h } else { 0 };
        let lines: Vec<Line> = items
            .iter()
            .enumerate()
            .skip(offset)
            .take(h.max(1))
            .map(|(i, label)| {
                let st = if i == cur {
                    Style::default()
                        .fg(Color::Black)
                        .bg(ACCENT)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White).bg(BG)
                };
                Line::from(Span::styled(format!("  {label}"), st))
            })
            .collect();
        f.render_widget(Paragraph::new(lines), rows[1]);

        let hint = match phase {
            Phase::Region => " \u{2191}\u{2193} Select   ENTER Next   ESC Cancel ",
            Phase::Keyboard => {
                " \u{2191}\u{2193} Select   ENTER Finish   BACKSPACE Back   ESC Cancel "
            }
        };
        f.render_widget(
            Paragraph::new(hint)
                .style(Style::default().fg(Color::White).bg(BAR))
                .alignment(Alignment::Center),
            rows[2],
        );
    }
}

/// `localectl set-locale LANG=<locale>` — echoed under dry-run.
fn apply_locale(locale: &str, dry: bool) {
    let arg = format!("LANG={locale}");
    if dry {
        println!("  + localectl set-locale {arg}");
        return;
    }
    let _ = Command::new("localectl")
        .args(["set-locale", &arg])
        .status();
}

/// `localectl set-x11-keymap <layout>` — echoed under dry-run. (The OOBE also writes
/// the labwc XKB layout via `keyboard::apply_layout` so it applies without a reboot.)
fn apply_keymap(layout: &str, dry: bool) {
    if dry {
        println!("  + localectl set-x11-keymap {layout}");
        return;
    }
    let _ = Command::new("localectl")
        .args(["set-x11-keymap", layout])
        .status();
    let _ = crate::keyboard::apply_layout(layout);
}

/// Stamp `oobe_done` so the wizard shows once (E11.10) — never echoed; it's our own
/// state, harmless to write even in a dry walkthrough's *test*, but we honour dry by
/// not persisting so a `--dry-run` leaves the user's config untouched.
fn finish(dry: bool) {
    if dry {
        return;
    }
    let mut st = crate::state::load();
    st.oobe_done = true;
    let _ = crate::state::save(&st);
}

/// Persist the four Privacy toggles to `menu.json` and apply the configs they each
/// control (E11.7) — echoed under dry-run. `find_my_device`/Advertising are pure
/// state flags; Location drives a geoclue opt-out marker and Diagnostics a telemetry
/// opt-out marker (small files the toggle owns), so each switch does something real.
fn commit_privacy(state: &Oobe) {
    if state.dry {
        println!("  + privacy: location={} diagnostics={} find_my_device={} ads={}\n  + geoclue: {}\n  + telemetry: {}",
            state.p_location, state.p_diagnostics, state.p_find, state.p_ads,
            if state.p_location { "enabled" } else { "opt-out marker written" },
            if state.p_diagnostics { "enabled" } else { "opt-out marker written" });
        return;
    }
    let mut st = crate::state::load();
    st.privacy_location = state.p_location;
    st.privacy_diagnostics = state.p_diagnostics;
    st.privacy_find_device = state.p_find;
    st.privacy_ads = state.p_ads;
    let _ = crate::state::save(&st);
    // Each external toggle owns one opt-out marker under the config dir.
    if let Some(dir) = crate::state::config_path().and_then(|p| p.parent().map(|d| d.to_path_buf()))
    {
        write_optout(&dir.join("no-geolocation"), !state.p_location);
        write_optout(&dir.join("no-telemetry"), !state.p_diagnostics);
    }
}

/// Create (opt-out on) or remove (opt-out off) a marker file.
fn write_optout(path: &std::path::Path, present: bool) {
    if present {
        let _ = std::fs::write(path, b"opted out by MDE-Retro OOBE\n");
    } else {
        let _ = std::fs::remove_file(path);
    }
}

/// Persist the Personalize choices (accent + light/dark) to `menu.json` (E11.9);
/// applied at the next surface launch (`main.rs` reads them at startup).
fn commit_personalize(state: &Oobe) {
    let accent = ACCENTS[state.accent].0;
    let mode = if state.light { "light" } else { "dark" };
    if state.dry {
        println!("  + personalize: accent={accent} mode={mode}");
        return;
    }
    let mut st = crate::state::load();
    st.icon_color = accent.to_string();
    st.theme_mode = mode.to_string();
    let _ = crate::state::save(&st);
}

/// Apply both keyboard layouts (E11.3): `localectl set-x11-keymap <l1>[,<l2>]` — the
/// combined keymap so the second layout is switchable. Echoed under dry-run.
fn apply_second_keymap(primary: &str, second: Option<&str>, dry: bool) {
    let combined = match second {
        Some(s) => format!("{primary},{s}"),
        None => primary.to_string(),
    };
    if dry {
        println!("  + localectl set-x11-keymap {combined}");
        return;
    }
    let _ = Command::new("localectl")
        .args(["set-x11-keymap", &combined])
        .status();
}

/// Connect to the chosen Wi-Fi (E11.4): `nm::wifi_connect` (= `nmcli device wifi
/// connect`), echoed under dry-run. No selection → nothing to do (e.g. "Skip").
fn commit_network(state: &Oobe) {
    let Some(i) = state.wifi_sel else { return };
    let ssid = &state.wifis[i].ssid;
    if state.dry {
        println!("  + nmcli device wifi connect '{ssid}'");
        return;
    }
    let _ = crate::nm::wifi_connect(ssid, &state.wifi_pw);
}

/// Create the local account (E11.5): `useradd -m -G wheel <user>` + set its password,
/// echoed under dry-run (the only path). Blank/mismatched fields are a no-op (the
/// view keeps Next disabled until they agree, so this is defensive).
fn commit_account(state: &Oobe) {
    if state.username.is_empty() || state.password.is_empty() || state.password != state.password2 {
        return;
    }
    if state.dry {
        println!(
            "  + useradd -m -G wheel {}\n  + passwd {} (password set via chpasswd)",
            state.username, state.username
        );
        return;
    }
    // The privileged engine owns the real write; the OOBE shells it through pkexec.
    let _ = Command::new("pkexec")
        .args(["useradd", "-m", "-G", "wheel", &state.username])
        .status();
    let _ = Command::new("pkexec")
        .arg("sh")
        .arg("-c")
        .arg(format!(
            "printf '%s:%s' '{}' '{}' | chpasswd",
            state.username, state.password
        ))
        .status();
}

fn advance(state: &mut Oobe) {
    if let Some(n) = flow_next(&state.flow, state.stage) {
        state.stage = n;
    }
}

fn update(state: &mut Oobe, msg: Msg) -> Task<Msg> {
    match msg {
        Msg::PickRegion(i) => state.region = i,
        Msg::PickLayout(i) => state.layout = i,
        Msg::PickSecond(i) => state.second_layout = Some(i),
        Msg::SelectWifi(i) => state.wifi_sel = Some(i),
        Msg::WifiPw(s) => state.wifi_pw = s,
        Msg::Username(s) => state.username = s,
        Msg::Password(s) => state.password = s,
        Msg::Password2(s) => state.password2 = s,
        Msg::TogglePrivacy(which) => match which {
            0 => state.p_location = !state.p_location,
            1 => state.p_diagnostics = !state.p_diagnostics,
            2 => state.p_find = !state.p_find,
            _ => state.p_ads = !state.p_ads,
        },
        Msg::PickAccent(i) => state.accent = i,
        Msg::SetMode(light) => state.light = light,
        Msg::Phone(s) => state.phone = s,
        Msg::Skip => {
            // Skip / Do-it-later: advance without committing this stage. For the
            // second-keyboard stage that means clearing any tentative pick.
            if state.stage == Stage::SecondKeyboard {
                state.second_layout = None;
            }
            advance(state);
        }
        Msg::Back => {
            if let Some(p) = flow_prev(&state.flow, state.stage) {
                state.stage = p;
            }
        }
        Msg::Next => {
            // Commit the stage we're leaving, then advance.
            match state.stage {
                Stage::Region => apply_locale(REGIONS[state.region].1, state.dry),
                Stage::Keyboard => {
                    apply_keymap(crate::keyboard::LAYOUTS[state.layout].0, state.dry)
                }
                Stage::SecondKeyboard => apply_second_keymap(
                    crate::keyboard::LAYOUTS[state.layout].0,
                    state.second_layout.map(|i| crate::keyboard::LAYOUTS[i].0),
                    state.dry,
                ),
                Stage::Network => commit_network(state),
                Stage::Account => commit_account(state),
                Stage::Privacy => commit_privacy(state),
                Stage::Personalize => commit_personalize(state),
                // Pin (E11.6) and Your Phone (E11.8) collect nothing to commit yet —
                // a faithful Skip-style advance (PIN is future; phone pairing is the
                // Your Phone app's job once `mde connect` lands).
                Stage::Pin | Stage::YourPhone => {}
                Stage::Finalize => {}
            }
            advance(state);
        }
        Msg::Finish => {
            finish(state.dry);
            std::process::exit(0);
        }
    }
    Task::none()
}

fn pad(t: f32, r: f32, b: f32, l: f32) -> Padding {
    Padding {
        top: t,
        right: r,
        bottom: b,
        left: l,
    }
}

fn bg() -> Background {
    Background::Gradient(Gradient::Linear(
        Linear::new(Radians(std::f32::consts::PI))
            .add_stop(0.0, palette::color(palette::SETUP_GRADIENT_TOP))
            .add_stop(1.0, palette::color(palette::SETUP_GRADIENT_BOTTOM)),
    ))
}

/// A scrollable single-select list (E11.2 `render_picker`, reused for Region and
/// Keyboard): each row is a button; the selected row paints the accent.
fn picker<'a>(
    items: impl Iterator<Item = (usize, &'a str)>,
    selected: usize,
    on_pick: fn(usize) -> Msg,
) -> Element<'a, Msg> {
    let mut col = Column::new().spacing(2.0).padding(pad(0.0, 8.0, 0.0, 8.0));
    for (i, label) in items {
        let sel = i == selected;
        let row = ibutton(text(label).size(metrics::UI_PX).color(white()))
            .width(Length::Fill)
            .padding(pad(6.0, 12.0, 6.0, 12.0))
            .on_press(on_pick(i))
            .style(move |_, _| ibutton::Style {
                background: Some(if sel {
                    Background::Color(palette::color(palette::HIGHLIGHT))
                } else {
                    Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.06))
                }),
                text_color: white(),
                border: iced::Border {
                    radius: 2.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            });
        col = col.push(row);
    }
    scrollable(col)
        .height(Length::Fill)
        .style(mde_ui::scrollbar)
        .into()
}

/// The bottom action strip: an optional Back, a spacer, the primary button.
fn actions<'a>(back: bool, primary: &'a str, on_primary: Msg) -> Element<'a, Msg> {
    actions_full(back, None, primary, on_primary)
}

/// Like [`actions`] but with an optional secondary (Skip / Do-it-later) button to
/// the left of the primary (E11.3/E11.6/E11.8).
fn actions_full<'a>(
    back: bool,
    skip: Option<&'a str>,
    primary: &'a str,
    on_primary: Msg,
) -> Element<'a, Msg> {
    let mut row = Row::new().spacing(8.0).padding(pad(8.0, 24.0, 16.0, 24.0));
    if back {
        row = row.push(
            mde_ui::button(text("Back").size(metrics::UI_PX))
                .on_press(Msg::Back)
                .width(Length::Fixed(96.0)),
        );
    }
    row = row.push(Space::with_width(Length::Fill));
    if let Some(label) = skip {
        row = row.push(
            mde_ui::button(text(label).size(metrics::UI_PX))
                .on_press(Msg::Skip)
                .width(Length::Fixed(120.0)),
        );
    }
    row = row.push(
        mde_ui::button(text(primary).size(metrics::UI_PX))
            .on_press(on_primary)
            .default(true)
            .width(Length::Fixed(120.0)),
    );
    row.into()
}

/// A stage frame: a big heading, a subtitle, the body, and the action strip.
fn frame<'a>(
    heading: &'a str,
    subtitle: &'a str,
    body: Element<'a, Msg>,
    actions: Element<'a, Msg>,
) -> Element<'a, Msg> {
    let header = Column::new()
        .spacing(6.0)
        .padding(pad(28.0, 28.0, 8.0, 28.0))
        .push(
            text(heading)
                .size(metrics::INFO_TITLE_PX)
                .font(font::ui_bold())
                .color(white()),
        )
        .push(text(subtitle).size(metrics::UI_PX).color(dim()));
    let screen = Column::new()
        .push(header)
        .push(
            container(body)
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(pad(0.0, 28.0, 0.0, 28.0)),
        )
        .push(actions);
    container(screen)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(bg()),
            ..container::Style::default()
        })
        .into()
}

/// One Privacy row: an On/Off toggle button + a label/description, on the chrome.
fn privacy_row<'a>(on: bool, which: u8, label: &'a str, desc: &'a str) -> Element<'a, Msg> {
    let pill = ibutton(
        text(if on { "On" } else { "Off" })
            .size(metrics::UI_PX)
            .color(white()),
    )
    .padding(pad(4.0, 14.0, 4.0, 14.0))
    .on_press(Msg::TogglePrivacy(which))
    .style(move |_, _| ibutton::Style {
        background: Some(if on {
            Background::Color(palette::color(palette::HIGHLIGHT))
        } else {
            Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.10))
        }),
        text_color: white(),
        border: iced::Border {
            radius: 2.0.into(),
            ..Default::default()
        },
        ..Default::default()
    });
    Row::new()
        .spacing(14.0)
        .align_y(iced::alignment::Vertical::Center)
        .push(container(pill).width(Length::Fixed(70.0)))
        .push(
            Column::new()
                .push(text(label).size(metrics::UI_PX).color(white()))
                .push(text(desc).size(metrics::BADGE_PX).color(dim())),
        )
        .into()
}

fn privacy_body(state: &Oobe) -> Element<'_, Msg> {
    scrollable(
        Column::new()
            .spacing(12.0)
            .padding(pad(12.0, 8.0, 0.0, 8.0))
            .push(privacy_row(
                state.p_location,
                0,
                "Location",
                "Let apps use your location and location history.",
            ))
            .push(privacy_row(
                state.p_diagnostics,
                1,
                "Diagnostic data",
                "Send diagnostic and usage data to help improve the system.",
            ))
            .push(privacy_row(
                state.p_find,
                2,
                "Find my device",
                "Use location to help you find your device if you lose it.",
            ))
            .push(privacy_row(
                state.p_ads,
                3,
                "Tailored experiences",
                "Use diagnostic data for tips and recommendations.",
            )),
    )
    .height(Length::Fill)
    .style(mde_ui::scrollbar)
    .into()
}

/// A color swatch button for the Personalize accent picker.
fn swatch(i: usize, selected: bool) -> Element<'static, Msg> {
    let rgb = palette::icon_accent(i as u8, true);
    let fill = iced::Color::from_rgb8(rgb.0, rgb.1, rgb.2);
    ibutton(
        text(if selected { "●" } else { " " })
            .size(metrics::UI_PX)
            .color(white()),
    )
    .width(Length::Fixed(44.0))
    .height(Length::Fixed(44.0))
    .on_press(Msg::PickAccent(i))
    .style(move |_, _| ibutton::Style {
        background: Some(Background::Color(fill)),
        text_color: white(),
        border: iced::Border {
            color: white(),
            width: if selected { 2.0 } else { 0.0 },
            radius: 3.0.into(),
        },
        ..Default::default()
    })
    .into()
}

fn personalize_body(state: &Oobe) -> Element<'_, Msg> {
    let mut swatches = Row::new().spacing(10.0);
    for (i, _) in ACCENTS.iter().enumerate() {
        swatches = swatches.push(swatch(i, i == state.accent));
    }
    let mode = Row::new()
        .spacing(8.0)
        .push(
            mde_ui::button(text("Light").size(metrics::UI_PX))
                .on_press(Msg::SetMode(true))
                .default(state.light)
                .width(Length::Fixed(96.0)),
        )
        .push(
            mde_ui::button(text("Dark").size(metrics::UI_PX))
                .on_press(Msg::SetMode(false))
                .default(!state.light)
                .width(Length::Fixed(96.0)),
        );
    Column::new()
        .spacing(18.0)
        .padding(pad(16.0, 8.0, 0.0, 8.0))
        .push(text("Accent color").size(metrics::UI_PX).color(dim()))
        .push(swatches)
        .push(text("Choose your mode").size(metrics::UI_PX).color(dim()))
        .push(mode)
        .into()
}

fn network_body(state: &Oobe) -> Element<'_, Msg> {
    if state.wifis.is_empty() {
        return text("No Wi-Fi networks found. You can connect later from the taskbar.")
            .size(metrics::UI_PX)
            .color(dim())
            .into();
    }
    let list = picker(
        state.wifis.iter().enumerate().map(|(i, w)| {
            (
                i,
                // (label is owned by the iterator's lifetime via the Wifi ssid)
                w.ssid.as_str(),
            )
        }),
        state.wifi_sel.unwrap_or(usize::MAX),
        Msg::SelectWifi,
    );
    // Show a password field for the selected secured network.
    let mut col = Column::new().spacing(10.0).push(list);
    if let Some(i) = state.wifi_sel {
        if state.wifis[i].secured {
            col = col.push(
                text_input("Network security key", &state.wifi_pw)
                    .on_input(Msg::WifiPw)
                    .secure(true)
                    .size(metrics::UI_PX)
                    .padding(pad(6.0, 10.0, 6.0, 10.0))
                    .width(Length::Fixed(260.0)),
            );
        }
    }
    col.into()
}

fn account_body(state: &Oobe) -> Element<'_, Msg> {
    let field = |placeholder: &'static str, value: &str, on: fn(String) -> Msg, secure: bool| {
        text_input(placeholder, value)
            .on_input(on)
            .secure(secure)
            .size(metrics::UI_PX)
            .padding(pad(6.0, 10.0, 6.0, 10.0))
            .width(Length::Fixed(280.0))
    };
    let mut col = Column::new()
        .spacing(12.0)
        .padding(pad(16.0, 8.0, 0.0, 8.0))
        .push(text("User name").size(metrics::UI_PX).color(dim()))
        .push(field("User name", &state.username, Msg::Username, false))
        .push(text("Password").size(metrics::UI_PX).color(dim()))
        .push(field("Password", &state.password, Msg::Password, true))
        .push(text("Confirm password").size(metrics::UI_PX).color(dim()))
        .push(field(
            "Confirm password",
            &state.password2,
            Msg::Password2,
            true,
        ));
    // A live mismatch hint (the commit also guards defensively).
    if !state.password.is_empty() && state.password != state.password2 {
        col = col.push(
            text("Passwords don't match yet.")
                .size(metrics::BADGE_PX)
                .color(palette::color(palette::URGENT)),
        );
    }
    col.into()
}

fn your_phone_body(state: &Oobe) -> Element<'_, Msg> {
    Column::new()
        .spacing(12.0)
        .padding(pad(20.0, 8.0, 0.0, 8.0))
        .push(text("Phone number").size(metrics::UI_PX).color(dim()))
        .push(
            text_input("+1 555 0123", &state.phone)
                .on_input(Msg::Phone)
                .size(metrics::UI_PX)
                .padding(pad(6.0, 10.0, 6.0, 10.0))
                .width(Length::Fixed(260.0)),
        )
        .push(
            text("We'll send a link to install the companion app. Pairing finishes in the Your Phone app.")
                .size(metrics::BADGE_PX)
                .color(dim()),
        )
        .into()
}

fn view(state: &Oobe) -> Element<'_, Msg> {
    match state.stage {
        Stage::Region => frame(
            "Let's start with your region",
            "Is this the right country or region?",
            picker(
                REGIONS.iter().enumerate().map(|(i, (name, _))| (i, *name)),
                state.region,
                Msg::PickRegion,
            ),
            actions(false, "Yes", Msg::Next),
        ),
        Stage::Keyboard => frame(
            "Is this the right keyboard layout?",
            "If you also use another keyboard layout, you can add one later.",
            picker(
                crate::keyboard::LAYOUTS
                    .iter()
                    .enumerate()
                    .map(|(i, (_, name))| (i, *name)),
                state.layout,
                Msg::PickLayout,
            ),
            actions(true, "Yes", Msg::Next),
        ),
        Stage::SecondKeyboard => frame(
            "Want to add a second keyboard layout?",
            "Pick another layout to switch between, or skip — you can add one later in Settings.",
            picker(
                crate::keyboard::LAYOUTS
                    .iter()
                    .enumerate()
                    .map(|(i, (_, name))| (i, *name)),
                state.second_layout.unwrap_or(usize::MAX),
                Msg::PickSecond,
            ),
            actions_full(true, Some("Skip"), "Add layout", Msg::Next),
        ),
        Stage::Network => frame(
            "Let's connect you to a network",
            "Pick a Wi-Fi network to get updates and finish setup, or skip for now.",
            network_body(state),
            actions_full(true, Some("Skip for now"), "Connect", Msg::Next),
        ),
        Stage::Account => frame(
            "Who's going to use this PC?",
            "Create a local account. You'll use this name and password to sign in.",
            account_body(state),
            actions(true, "Next", Msg::Next),
        ),
        Stage::Pin => {
            let body = Column::new()
                .spacing(10.0)
                .padding(pad(20.0, 0.0, 0.0, 0.0))
                .push(
                    text("A PIN is a quick, secure way to sign in to your device.")
                        .size(metrics::UI_PX)
                        .color(white()),
                )
                .push(
                    text("You can set this up later in Settings ▸ Accounts ▸ Sign-in options.")
                        .size(metrics::BADGE_PX)
                        .color(dim()),
                );
            frame(
                "Create a PIN",
                "Windows Hello sign-in.",
                body.into(),
                actions_full(true, Some("Skip for now"), "Next", Msg::Next),
            )
        }
        Stage::YourPhone => frame(
            "Link your phone and PC",
            "Get your phone's photos, messages and notifications on your PC. Enter your number, or do this later.",
            your_phone_body(state),
            actions_full(true, Some("Do it later"), "Next", Msg::Next),
        ),
        Stage::Privacy => frame(
            "Choose privacy settings for your device",
            "You're in control. Turn off anything you'd rather not share; you can change these later in Settings.",
            privacy_body(state),
            actions(true, "Accept", Msg::Next),
        ),
        Stage::Personalize => frame(
            "Now personalize your device",
            "Pick an accent color and a light or dark look. You can change this any time.",
            personalize_body(state),
            actions(true, "Next", Msg::Next),
        ),
        Stage::Finalize => {
            let body = Column::new()
                .spacing(10.0)
                .padding(pad(20.0, 0.0, 0.0, 0.0))
                .push(
                    text("This might take a few minutes.")
                        .size(metrics::UI_PX)
                        .color(dim()),
                )
                .push(
                    text(format!(
                        "Region: {}\nKeyboard: {}",
                        REGIONS[state.region].0,
                        crate::keyboard::LAYOUTS[state.layout].1
                    ))
                    .size(metrics::UI_PX)
                    .color(white()),
                );
            frame(
                "Hi. We're getting everything ready for you.",
                "Almost there — your MackesDE desktop is nearly set up.",
                body.into(),
                actions(true, "Finish", Msg::Finish),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_flow_is_linear_and_terminates() {
        // The canonical FLOW: flow_next/flow_prev walk it and terminate cleanly.
        assert_eq!(flow_next(FLOW, Stage::Region), Some(Stage::Keyboard));
        assert_eq!(flow_prev(FLOW, Stage::Region), None);
        assert_eq!(flow_next(FLOW, *FLOW.last().unwrap()), None);
        // Round-trip every adjacent pair: prev(next(s)) == s.
        for w in FLOW.windows(2) {
            assert_eq!(flow_next(FLOW, w[0]), Some(w[1]));
            assert_eq!(flow_prev(FLOW, w[1]), Some(w[0]));
        }
    }

    #[test]
    fn wired_link_drops_the_network_stage() {
        // E11.4: the live flow excludes Network when wired; Account follows
        // SecondKeyboard directly.
        let flow: Vec<Stage> = FLOW
            .iter()
            .copied()
            .filter(|s| *s != Stage::Network)
            .collect();
        assert_eq!(
            flow_next(&flow, Stage::SecondKeyboard),
            Some(Stage::Account)
        );
        assert!(!flow.contains(&Stage::Network));
    }

    #[test]
    fn detected_region_falls_back_to_us() {
        // An unknown/empty LANG must not panic and lands on a valid index.
        assert!(detected_region() < REGIONS.len());
        assert!(detected_layout() < crate::keyboard::LAYOUTS.len());
    }

    // E11.2 — the interactive TUI picker's navigation logic (the part that's
    // verifiable without a TTY; the ratatui render + event loop need a real console).
    use super::interactive::Picker;

    #[test]
    fn picker_starts_at_detected_and_clamps_both_ends() {
        let mut p = Picker::new(REGIONS.len(), 3);
        assert_eq!(p.cursor, 3, "pre-highlighted to the detected index");
        p.move_cursor(-1);
        assert_eq!(p.cursor, 2);
        p.move_cursor(-10);
        assert_eq!(p.cursor, 0, "clamps at the top, no wrap");
        p.move_cursor(1000);
        assert_eq!(p.cursor, REGIONS.len() - 1, "clamps at the bottom, no wrap");
    }

    #[test]
    fn picker_start_index_clamps_into_range() {
        // A detected index past the end (e.g. a stale layout list) is clamped, not OOB.
        let p = Picker::new(3, 99);
        assert_eq!(p.cursor, 2);
    }

    #[test]
    fn picker_on_empty_list_is_a_safe_noop() {
        let mut p = Picker::new(0, 5);
        assert_eq!(p.cursor, 0);
        p.move_cursor(1);
        p.move_cursor(-1);
        assert_eq!(p.cursor, 0);
    }
}
