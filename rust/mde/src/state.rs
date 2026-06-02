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
fn def_quick_actions() -> Vec<String> {
    ["wifi", "bluetooth", "airplane", "mute", "nightlight"]
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
