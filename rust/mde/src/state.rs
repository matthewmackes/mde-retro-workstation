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

/// The persisted menu/shell state. `#[serde(default)]` on every field keeps old
/// files loadable as new fields are added (pinned today; renames/hidden next).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MenuState {
    #[serde(default)]
    pub pinned: Vec<PinnedItem>,
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
                PinnedItem { name: "Files".into(), command: "mde files".into() },
                PinnedItem { name: "Terminal".into(), command: "foot".into() },
            ],
        };
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(parse(&json), s);
    }

    #[test]
    fn missing_and_garbage_fall_back_to_default() {
        assert_eq!(parse(""), MenuState::default());
        assert_eq!(parse("not json"), MenuState::default());
        assert_eq!(parse("{}"), MenuState::default()); // empty object → empty pinned
    }

    #[test]
    fn unknown_and_absent_fields_are_tolerated() {
        // Forward-compat: an old file without `pinned`, or a future file with
        // extra keys, both load cleanly.
        assert_eq!(parse(r#"{"renames":{"a":"b"}}"#).pinned.len(), 0);
        let s = parse(r#"{"pinned":[{"name":"X","command":"x"}],"future":true}"#);
        assert_eq!(s.pinned, vec![PinnedItem { name: "X".into(), command: "x".into() }]);
    }
}
