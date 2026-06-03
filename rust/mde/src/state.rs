//! Persisted shell state at `~/.config/mde/menu.json` — the store behind
//! Start-menu pinned items (and, as they land, Quick Launch, renames, hidden
//! entries, custom icons). Plain serde over serde_json (already a dependency);
//! no iced, so it is unit-tested directly. Loads tolerantly (missing/garbage →
//! defaults) and saves atomically (temp file + rename).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// One item pinned to the top of the Start menu.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PinnedItem {
    pub name: String,
    pub command: String,
    /// How many times this pin has been launched — the Win10 Start "Suggested"
    /// ranking. `#[serde(default)]` so old menu.json files load (count 0).
    #[serde(default)]
    pub launch_count: u32,
}

/// A Windows 10 Start tile size (the right tile area). Each maps to a grid span
/// in base small-tile cells; see `metrics::TILE_*_PX`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TileSize {
    Small,
    #[default]
    Medium,
    Wide,
    Large,
}

impl TileSize {
    /// (cols, rows) span in base small-tile cells.
    pub fn span(self) -> (u16, u16) {
        match self {
            TileSize::Small => (1, 1),
            TileSize::Medium => (2, 2),
            TileSize::Wide => (4, 2),
            TileSize::Large => (4, 4),
        }
    }
    /// The lowercase token (round-trips with [`TileSize::from_token`]).
    pub fn token(self) -> &'static str {
        match self {
            TileSize::Small => "small",
            TileSize::Medium => "medium",
            TileSize::Wide => "wide",
            TileSize::Large => "large",
        }
    }
    /// Parse a size token; anything unrecognized falls back to `Medium`.
    pub fn from_token(s: &str) -> TileSize {
        match s.to_ascii_lowercase().as_str() {
            "small" => TileSize::Small,
            "wide" => TileSize::Wide,
            "large" => TileSize::Large,
            _ => TileSize::Medium,
        }
    }
}

/// One Windows 10 Start tile. Optional fields tolerate missing/garbage to sane
/// defaults (§2.6).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StartTile {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub size: TileSize,
    #[serde(default)]
    pub group: String,
}

/// A saved Windows 10 theme bundle (Settings ▸ Personalization ▸ Themes, E7.7):
/// a wallpaper + UI accent + light/dark, applied together on select.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SavedTheme {
    pub name: String,
    /// Wallpaper path; empty ⇒ keep the current background.
    #[serde(default)]
    pub wallpaper: String,
    /// `palette::WIN10_ACCENTS` index.
    #[serde(default)]
    pub accent: u8,
    #[serde(default)]
    pub dark: bool,
}

fn def_theme() -> String {
    "carbon".into()
}
fn def_theme_mode() -> String {
    "dark".into()
}
fn def_icon_color() -> String {
    "neutral".into()
}
/// The Win10 Action Center quick-action tiles, in order. The first four show
/// collapsed; the rest appear on Expand (E3.5).
/// Default virtual-desktop count for the Task View fallback strip (E4.5).
fn def_virtual_desktops() -> u32 {
    4
}
fn def_quick_actions() -> Vec<String> {
    [
        "wifi",
        "bluetooth",
        "airplane",
        "mute",
        "focus",
        "nightlight",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// The Start-rail system folders shown by default (the pre-E7.8a hardcoded set).
fn def_start_folders() -> Vec<String> {
    ["documents", "pictures", "downloads"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// The persisted menu/shell state. `#[serde(default)]` on every field keeps old
/// files loadable as new fields are added. The appearance fields default to the
/// Carbon theme (dark, neutral icons) — see SPEC-carbon-theme.md — so explicit
/// default fns are required (bare String default is "", which is wrong here);
/// the manual `Default` impl below must agree so `parse("{}") == default()`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MenuState {
    #[serde(default)]
    pub pinned: Vec<PinnedItem>,
    /// "Show small icons in Start menu" (Taskbar & Start Menu Properties).
    /// Default false ⇒ the large-icon Start menu, the Win2000 default.
    #[serde(default)]
    pub start_small_icons: bool,
    /// Icon set key (Display ▸ Appearance). "" / "win2k" ⇒ the Windows 2000
    /// classic icons; "haiku" ⇒ the Haiku OS icon theme. Distinct from `theme`.
    #[serde(default)]
    pub icon_set: String,
    /// Look-and-feel theme: "carbon" (default), "win2000", or "windows10"
    /// (BeOS is "win2000" + the Haiku `icon_set`, which `main.rs` maps to
    /// `Theme::Beos`). Free-form; `main.rs` falls back to Carbon for anything
    /// unrecognized.
    #[serde(default = "def_theme")]
    pub theme: String,
    /// Carbon light/dark mode: "dark" (default) or "light".
    #[serde(default = "def_theme_mode")]
    pub theme_mode: String,
    /// Icon accent hue: "neutral" (default), "blue", "orange", or "red".
    #[serde(default = "def_icon_color")]
    pub icon_color: String,
    /// Windows 10 Start tiles (the right tile area). Empty on a fresh config;
    /// the Win10 Start seeds it from `pinned` (see [`seed_start_tiles`]) so the
    /// area is never blank. Garbage → empty (§2.6).
    #[serde(default)]
    pub start_tiles: Vec<StartTile>,
    /// Win10 Action Center quick-action tile order (E3.5).
    #[serde(default = "def_quick_actions")]
    pub quick_actions: Vec<String>,
    /// Win10 Focus assist (Do Not Disturb): while true, notifyd collects history
    /// but suppresses toasts (E3.7).
    #[serde(default)]
    pub focus_assist: bool,
    /// Number of virtual desktops to show in Task View's **fallback** strip when
    /// the compositor doesn't advertise ext-workspace-v1 (E4.5). When the live
    /// protocol is present (labwc), the band reflects the real workspaces and
    /// this is ignored. Default 4; a value ≤ 1 means a single desktop (no band).
    #[serde(default = "def_virtual_desktops")]
    pub virtual_desktops: u32,
    /// Windows 10 UI accent index (E7.1/E7.5) — into `palette::WIN10_ACCENTS`.
    /// Drives selection/highlight/active-title under the Win10 theme (distinct
    /// from `icon_color`, which only tints icons). 0 = the stock blue.
    #[serde(default)]
    pub win10_accent: u8,
    /// Saved Win10 theme bundles (Personalization ▸ Themes, E7.7).
    #[serde(default)]
    pub themes: Vec<SavedTheme>,
    /// Win10 Start ▸ "Show more tiles" — widens the tile grid (E7.8).
    #[serde(default)]
    pub start_more_tiles: bool,
    /// Win10 Start ▸ "Use Start full screen" — the opaque full-window Start layout
    /// instead of the compact bottom-left panel (E7.8b).
    #[serde(default)]
    pub start_full_screen: bool,
    /// Win10 Start ▸ "Show recently added apps" (E7.8). Default on.
    #[serde(default = "def_true")]
    pub start_show_recent: bool,
    /// Win10 Start ▸ "Show most used apps" (the Suggested band, E7.8). Default on.
    #[serde(default = "def_true")]
    pub start_show_suggested: bool,
    /// Win10 Start ▸ "Choose which folders appear on Start" (E7.8a): the system-
    /// folder keys shown in the Start rail, in order (see `start_win10::START_FOLDERS`).
    /// Unknown keys are ignored by the rail. Default = Documents, Pictures, Downloads.
    #[serde(default = "def_start_folders")]
    pub start_folders: Vec<String>,
    /// Win10 taskbar location: "bottom" (default) or "top" — drives the
    /// `panel.rs` layer anchor (E7.9). ("left"/"right" need a vertical bar, E7.9a.)
    #[serde(default = "def_taskbar_location")]
    pub taskbar_location: String,
    /// Win10 taskbar: show the Task View button (E2.9). Default on.
    #[serde(default = "def_true")]
    pub win10_show_taskview: bool,
    /// Win10 "show accent color on Start & taskbar" (E7.5a). Default on; when off,
    /// the panel chrome highlights use a neutral grey (`palette::chrome_accent`).
    #[serde(default = "def_true")]
    pub win10_accent_on_taskbar: bool,
    /// Win10 "automatically hide the taskbar" (E2.9a). When on, the panel reserves
    /// no space and sits as a 1px reveal strip until the edge is hovered.
    #[serde(default)]
    pub win10_autohide: bool,
    /// Win10 "use small taskbar buttons" (E7.9a) — a compact 30px bar instead of 40px.
    #[serde(default)]
    pub win10_small_buttons: bool,
    /// Win10 Devices ▸ Printers "Let Windows manage my default printer" (E12.4).
    /// Default on (matches Win10); when on, the per-printer "Set as default" action
    /// is hidden — Windows defers the default to the last-used queue.
    #[serde(default = "def_true")]
    pub win10_manage_default_printer: bool,
    /// Devices ▸ Mouse (E12.6): primary button on the right (left-handed) → labwc
    /// `<leftHanded>`. The Mouse page mirrors these into `rc.xml`.
    #[serde(default)]
    pub mouse_left_handed: bool,
    /// Devices ▸ Mouse: reverse wheel direction → labwc `<naturalScroll>`.
    #[serde(default)]
    pub mouse_natural_scroll: bool,
    /// Devices ▸ Mouse: lines per wheel notch (1–10, default 3) → `<scrollFactor>`.
    #[serde(default = "def_scroll_lines")]
    pub mouse_scroll_lines: u8,
    /// Devices ▸ Mouse: "scroll inactive windows on hover" — an advisory toggle
    /// only (labwc/wlroots has no such knob); persisted here, never written to
    /// rc.xml. Default on (matches Win10).
    #[serde(default = "def_true")]
    pub mouse_scroll_inactive: bool,
    /// Devices ▸ Touchpad (E12.7): on/off → `<sendEventsMode>`. Default on. Only
    /// surfaced (and written to rc.xml's `touchpad` device) when a touchpad exists.
    #[serde(default = "def_true")]
    pub touchpad_enabled: bool,
    /// Touchpad cursor speed level (1–10, default 5 = neutral) → `<pointerSpeed>`.
    #[serde(default = "def_touchpad_speed")]
    pub touchpad_speed: u8,
    /// Touchpad tap-to-click → `<tap>`. Default on.
    #[serde(default = "def_true")]
    pub touchpad_tap: bool,
    /// Touchpad two-finger scrolling → `<scrollMethod>twofinger|none</scrollMethod>`.
    /// Default on.
    #[serde(default = "def_true")]
    pub touchpad_two_finger: bool,
    /// Touchpad reverse (natural) scroll direction → `<naturalScroll>`. Default on
    /// (matches the Win10 touchpad default).
    #[serde(default = "def_true")]
    pub touchpad_natural_scroll: bool,
    /// Devices ▸ Typing (E12.8): key-repeat rate (chars/sec) → labwc
    /// `<keyboard><repeatRate>`. Default 25 (labwc's own default).
    #[serde(default = "def_kb_repeat_rate")]
    pub kb_repeat_rate: u32,
    /// Key-repeat delay before repeat starts (ms) → `<keyboard><repeatDelay>`.
    /// Default 600.
    #[serde(default = "def_kb_repeat_delay")]
    pub kb_repeat_delay: u32,
    /// Keyboard layout (xkb code, e.g. "us") → `XKB_DEFAULT_LAYOUT` in labwc's
    /// `environment` file; takes effect at next sign-in. Default "us".
    #[serde(default = "def_kb_layout")]
    pub kb_layout: String,
    /// Typing advisory toggles (no labwc/Wayland backend in a non-IME shell) —
    /// persisted, clearly labelled advisory. Both default on (match Win10).
    #[serde(default = "def_true")]
    pub typing_autocorrect: bool,
    #[serde(default = "def_true")]
    pub typing_suggestions: bool,
    /// Win10 taskbar search affordance: "button" (default), "box", or "hidden"
    /// (E2.9). All open `mde search`; "box" is a wider labelled pill.
    #[serde(default = "def_search_mode")]
    pub win10_search_mode: String,
    /// Settings ▸ Update "Pause updates" until this Unix-seconds time (E13.4);
    /// 0 = not paused. While in the future the dnf-automatic timer is masked.
    #[serde(default)]
    pub update_paused_until: u64,
    /// Settings ▸ Update active hours (E13.5): the window updates avoid; the
    /// dnf-automatic timer is overridden to run at `update_active_end`. Hours 0–23.
    #[serde(default = "def_active_start")]
    pub update_active_start: u8,
    #[serde(default = "def_active_end")]
    pub update_active_end: u8,
    /// Settings ▸ Update ▸ Advanced (E13.7): restart ASAP after updates (writes
    /// the dnf-automatic `reboot` setting) + notify when a restart is required.
    #[serde(default)]
    pub update_restart_asap: bool,
    #[serde(default)]
    pub update_restart_notify: bool,
    /// Settings ▸ Network ▸ Mobile hotspot (E15.8): the AP SSID + key.
    #[serde(default = "def_hotspot_name")]
    pub hotspot_name: String,
    #[serde(default)]
    pub hotspot_password: String,
    /// Settings ▸ Network ▸ Data usage (E15.11): monthly limit in MB; 0 = no limit.
    #[serde(default)]
    pub data_limit_mb: u64,
    /// Settings ▸ Accounts ▸ Your info (E10.1): friendly display name + avatar path
    /// (both empty → fall back to the system user / `~/.face`).
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub account_picture: String,
    /// Win10 Explorer ▸ Quick access user-pinned folders (E8.3): appended to the
    /// auto-pinned standard folders in the Frequent-folders list.
    #[serde(default)]
    pub explorer_pins: Vec<PathBuf>,
    /// Win10 Explorer default landing when launched with no path: "quick" (Quick
    /// access, the default), "thispc" (This PC), "network" (Network), or "cloud"
    /// (paired devices). Read by `files::run` (E8.4, E8.5, E8.7).
    #[serde(default = "def_explorer_landing")]
    pub explorer_landing: String,
}

fn def_scroll_lines() -> u8 {
    3
}

fn def_touchpad_speed() -> u8 {
    5
}

fn def_kb_repeat_rate() -> u32 {
    25
}

fn def_kb_repeat_delay() -> u32 {
    600
}

fn def_kb_layout() -> String {
    "us".to_string()
}

fn def_true() -> bool {
    true
}
fn def_taskbar_location() -> String {
    "bottom".into()
}
fn def_search_mode() -> String {
    "button".into()
}
/// Default mobile-hotspot SSID (E15.8).
fn def_hotspot_name() -> String {
    "MackesDE".into()
}
/// Default active-hours window (E13.5): 08:00–17:00, the Win10 default.
fn def_active_start() -> u8 {
    8
}
fn def_active_end() -> u8 {
    17
}
fn def_explorer_landing() -> String {
    "quick".into()
}

impl Default for MenuState {
    fn default() -> Self {
        MenuState {
            pinned: Vec::new(),
            start_small_icons: false,
            icon_set: String::new(),
            theme: def_theme(),
            theme_mode: def_theme_mode(),
            icon_color: def_icon_color(),
            start_tiles: Vec::new(),
            quick_actions: def_quick_actions(),
            focus_assist: false,
            virtual_desktops: def_virtual_desktops(),
            win10_accent: 0,
            themes: Vec::new(),
            start_more_tiles: false,
            start_full_screen: false,
            start_show_recent: true,
            start_show_suggested: true,
            start_folders: def_start_folders(),
            taskbar_location: def_taskbar_location(),
            win10_show_taskview: true,
            win10_accent_on_taskbar: true,
            win10_autohide: false,
            win10_small_buttons: false,
            win10_manage_default_printer: true,
            mouse_left_handed: false,
            mouse_natural_scroll: false,
            mouse_scroll_lines: def_scroll_lines(),
            mouse_scroll_inactive: true,
            touchpad_enabled: true,
            touchpad_speed: def_touchpad_speed(),
            touchpad_tap: true,
            touchpad_two_finger: true,
            touchpad_natural_scroll: true,
            kb_repeat_rate: def_kb_repeat_rate(),
            kb_repeat_delay: def_kb_repeat_delay(),
            kb_layout: def_kb_layout(),
            typing_autocorrect: true,
            typing_suggestions: true,
            win10_search_mode: def_search_mode(),
            update_paused_until: 0,
            update_active_start: def_active_start(),
            update_active_end: def_active_end(),
            update_restart_asap: false,
            update_restart_notify: false,
            hotspot_name: def_hotspot_name(),
            hotspot_password: String::new(),
            data_limit_mb: 0,
            display_name: String::new(),
            account_picture: String::new(),
            explorer_pins: Vec::new(),
            explorer_landing: def_explorer_landing(),
        }
    }
}

/// The Start tiles to show: the persisted `start_tiles` if any, else seeded from
/// the pinned items (first-run) so the Win10 Start tile area is never blank.
/// Pure — the caller persists if it wants the seed to stick.
pub fn seed_start_tiles(state: &MenuState) -> Vec<StartTile> {
    if !state.start_tiles.is_empty() {
        return state.start_tiles.clone();
    }
    state
        .pinned
        .iter()
        .map(|p| StartTile {
            name: p.name.clone(),
            command: p.command.clone(),
            icon: String::new(),
            size: TileSize::Medium,
            group: String::new(),
        })
        .collect()
}

/// `~/.config/mde/menu.json` (honouring `$XDG_CONFIG_HOME`).
pub fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("mde").join("menu.json"))
}

/// Load the state, falling back to defaults on any problem (absent file,
/// unreadable, or malformed JSON) — the shell must always start.
pub fn load() -> MenuState {
    config_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| parse(&s))
        .unwrap_or_default()
}

/// Parse menu.json contents, tolerating garbage.
pub fn parse(s: &str) -> MenuState {
    serde_json::from_str(s).unwrap_or_default()
}

/// Save atomically: write a sibling temp file, then rename over the target.
pub fn save(state: &MenuState) -> std::io::Result<()> {
    let Some(path) = config_path() else {
        return Ok(());
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(state)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, &path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_through_json() {
        let s = MenuState {
            pinned: vec![
                PinnedItem {
                    name: "Files".into(),
                    command: "mde files".into(),
                    launch_count: 7,
                },
                PinnedItem {
                    name: "Terminal".into(),
                    command: "foot".into(),
                    launch_count: 0,
                },
            ],
            start_small_icons: true,
            icon_set: "haiku".into(),
            theme: "win2000".into(),
            theme_mode: "light".into(),
            icon_color: "blue".into(),
            start_tiles: vec![StartTile {
                name: "Firefox".into(),
                command: "firefox".into(),
                icon: "firefox".into(),
                size: TileSize::Wide,
                group: "Web".into(),
            }],
            quick_actions: vec!["wifi".into(), "mute".into()],
            focus_assist: true,
            virtual_desktops: 6,
            win10_accent: 4,
            themes: vec![SavedTheme {
                name: "Sunset".into(),
                wallpaper: "/usr/share/backgrounds/sunset.jpg".into(),
                accent: 3,
                dark: true,
            }],
            start_more_tiles: true,
            start_full_screen: true,
            start_show_recent: false,
            start_show_suggested: true,
            start_folders: vec!["documents".into(), "music".into()],
            taskbar_location: "top".into(),
            win10_show_taskview: false,
            win10_accent_on_taskbar: false,
            win10_autohide: true,
            win10_small_buttons: true,
            win10_manage_default_printer: false,
            mouse_left_handed: true,
            mouse_natural_scroll: true,
            mouse_scroll_lines: 7,
            mouse_scroll_inactive: false,
            touchpad_enabled: false,
            touchpad_speed: 8,
            touchpad_tap: false,
            touchpad_two_finger: false,
            touchpad_natural_scroll: false,
            kb_repeat_rate: 40,
            kb_repeat_delay: 300,
            kb_layout: "gb".into(),
            typing_autocorrect: false,
            typing_suggestions: false,
            win10_search_mode: "box".into(),
            update_paused_until: 1_900_000_000,
            update_active_start: 9,
            update_active_end: 18,
            update_restart_asap: true,
            update_restart_notify: true,
            hotspot_name: "MyHotspot".into(),
            hotspot_password: "s3cret".into(),
            data_limit_mb: 2048,
            display_name: "Ada Lovelace".into(),
            account_picture: "/home/me/.face".into(),
            explorer_pins: vec![PathBuf::from("/home/me/Projects")],
            explorer_landing: "thispc".into(),
        };
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(parse(&json), s);
    }

    #[test]
    fn tile_size_token_round_trips() {
        for sz in [
            TileSize::Small,
            TileSize::Medium,
            TileSize::Wide,
            TileSize::Large,
        ] {
            assert_eq!(TileSize::from_token(sz.token()), sz);
        }
        assert_eq!(TileSize::from_token("nonsense"), TileSize::Medium); // §2.6 garbage → default
        assert_eq!(TileSize::default(), TileSize::Medium);
    }

    #[test]
    fn start_tiles_seed_from_pinned_when_empty() {
        // E1.7: a fresh config (no start_tiles) seeds the tile area from pinned;
        // once tiles exist, the seed is ignored.
        let mut st = MenuState {
            pinned: vec![PinnedItem {
                name: "Files".into(),
                command: "mde files".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let seeded = seed_start_tiles(&st);
        assert_eq!(seeded.len(), 1);
        assert_eq!(seeded[0].name, "Files");
        assert_eq!(seeded[0].size, TileSize::Medium);
        st.start_tiles = vec![StartTile {
            name: "Term".into(),
            command: "foot".into(),
            icon: String::new(),
            size: TileSize::Small,
            group: String::new(),
        }];
        assert_eq!(seed_start_tiles(&st), st.start_tiles); // non-empty → no seeding
    }

    #[test]
    fn tile_defaults_tolerate_partial_json() {
        // §2.6: a tile with only name+command fills icon/size/group with defaults.
        let st = parse(r#"{"start_tiles":[{"name":"X","command":"x"}]}"#);
        assert_eq!(st.start_tiles.len(), 1);
        assert_eq!(st.start_tiles[0].size, TileSize::Medium);
        assert_eq!(st.start_tiles[0].group, "");
    }

    #[test]
    fn appearance_defaults_are_carbon_dark_neutral() {
        // First run / empty file must yield the Carbon defaults (SPEC item 1/4/5).
        let d = parse("{}");
        assert_eq!(d.theme, "carbon");
        assert_eq!(d.theme_mode, "dark");
        assert_eq!(d.icon_color, "neutral");
        assert_eq!(d, MenuState::default());
    }

    #[test]
    fn missing_and_garbage_fall_back_to_default() {
        assert_eq!(parse(""), MenuState::default());
        assert_eq!(parse("not json"), MenuState::default());
        assert_eq!(parse("{}"), MenuState::default()); // empty object → empty pinned
    }

    #[test]
    fn windows10_theme_round_trips() {
        // E0.4: the Win10 era is selected by a free-form theme string; it must
        // round-trip, while an empty/garbage file still yields the Carbon default
        // (D1: Carbon stays default; main.rs maps unknown themes back to Carbon).
        assert_eq!(parse(r#"{"theme":"windows10"}"#).theme, "windows10");
        assert_eq!(parse("{}").theme, "carbon");
    }

    #[test]
    fn unknown_and_absent_fields_are_tolerated() {
        // Forward-compat: an old file without `pinned`, or a future file with
        // extra keys, both load cleanly.
        assert_eq!(parse(r#"{"renames":{"a":"b"}}"#).pinned.len(), 0);
        let s = parse(r#"{"pinned":[{"name":"X","command":"x"}],"future":true}"#);
        assert_eq!(
            s.pinned,
            vec![PinnedItem {
                name: "X".into(),
                command: "x".into(),
                ..Default::default()
            }]
        );
    }
}
