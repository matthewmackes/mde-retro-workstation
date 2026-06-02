//! Persisted shell state at `~/.config/mde/menu.json` — the store behind
//! Start-menu pinned items (and, as they land, Quick Launch, renames, hidden
//! entries, custom icons). Plain serde over serde_json (already a dependency);
//! no iced, so it is unit-tested directly. Loads tolerantly (missing/garbage →
//! defaults) and saves atomically (temp file + rename).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// One item pinned to the top of the Start menu.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PinnedItem {
    pub name: String,
    pub command: String,
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
        }
    }
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
                },
                PinnedItem {
                    name: "Terminal".into(),
                    command: "foot".into(),
                },
            ],
            start_small_icons: true,
            icon_set: "haiku".into(),
            theme: "win2000".into(),
            theme_mode: "light".into(),
            icon_color: "blue".into(),
        };
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(parse(&json), s);
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
                command: "x".into()
            }]
        );
    }
}
