//! labwc keyboard config for Settings ▸ Devices ▸ Typing (E12.8).
//!
//! Two backends, both the way labwc actually wants them:
//!   - **Key-repeat rate / delay** are `<keyboard>` children in `rc.xml`. Unlike
//!     `<libinput>`, the `<keyboard>` block also holds every keybind, so the
//!     rewrite surgically updates *only* the `<repeatRate>`/`<repeatDelay>` lines
//!     and leaves the keybinds untouched. Applied live via `labwc --reconfigure`.
//!   - **Layout** is `XKB_DEFAULT_LAYOUT` in `~/.config/labwc/environment` (labwc
//!     reads XKB at startup, not from rc.xml), so a layout change takes effect at
//!     the next sign-in — the page says so.
//!
//! Shares rc.xml path + atomic-write plumbing with [`crate::mouse`]. The
//! `MDE_LABWC_RC` (rc.xml) and `MDE_LABWC_ENV` (environment) seams point the
//! writers at temp files for benches. Headless entry: `mde __kbd-rc`.

use std::path::PathBuf;

/// Common keyboard layouts surfaced in the Typing page — (xkb code, friendly
/// name). A focused set for a retro shell, not the full xkb registry.
pub const LAYOUTS: &[(&str, &str)] = &[
    ("us", "English (US)"),
    ("gb", "English (UK)"),
    ("de", "German"),
    ("fr", "French"),
    ("es", "Spanish"),
    ("it", "Italian"),
];

/// Surgically set `<repeatRate>`/`<repeatDelay>` inside the `<keyboard>` block,
/// preserving the keybinds (and everything else). Existing repeat lines anywhere
/// are dropped first (they live only in `<keyboard>`), then fresh ones are
/// inserted right after the `<keyboard …>` opening tag. Pure + idempotent.
pub fn rewrite_keyboard(xml: &str, rate: u32, delay: u32) -> String {
    // Drop any prior repeat lines.
    let mut stripped: String = xml
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with("<repeatRate>") && !t.starts_with("<repeatDelay>")
        })
        .collect::<Vec<_>>()
        .join("\n");
    if xml.ends_with('\n') {
        stripped.push('\n');
    }
    let insert =
        format!("\n    <repeatRate>{rate}</repeatRate>\n    <repeatDelay>{delay}</repeatDelay>");
    // Insert just after the close of the `<keyboard …>` opening tag.
    if let Some(kpos) = stripped.find("<keyboard") {
        if let Some(gt) = stripped[kpos..].find('>') {
            let at = kpos + gt + 1;
            return format!("{}{}{}", &stripped[..at], insert, &stripped[at..]);
        }
    }
    stripped
}

/// Write the repeat rate/delay into rc.xml (atomic) and reload labwc (E12.8).
pub fn apply_repeat(rate: u32, delay: u32) -> std::io::Result<()> {
    let Some(path) = crate::mouse::rc_path() else {
        return Ok(());
    };
    let xml = std::fs::read_to_string(&path)?;
    let out = rewrite_keyboard(&xml, rate, delay);
    crate::mouse::write_rc(&path, &out)
}

/// The labwc `environment` file path: `MDE_LABWC_ENV` if set (test seam), else
/// `$XDG_CONFIG_HOME/labwc/environment` (honouring `HOME`).
fn env_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("MDE_LABWC_ENV") {
        return Some(PathBuf::from(p));
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("labwc/environment"))
}

/// Upsert `KEY=value` in an `environment`-file body: replace the line if `KEY` is
/// present, else append it. Pure (unit-tested).
pub fn upsert_env(body: &str, key: &str, value: &str) -> String {
    let prefix = format!("{key}=");
    let mut found = false;
    let mut lines: Vec<String> = body
        .lines()
        .map(|l| {
            if l.trim_start().starts_with(&prefix) {
                found = true;
                format!("{key}={value}")
            } else {
                l.to_string()
            }
        })
        .collect();
    if !found {
        lines.push(format!("{key}={value}"));
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// Persist the keyboard layout to `~/.config/labwc/environment`
/// (`XKB_DEFAULT_LAYOUT`). No reconfigure — labwc reads XKB at startup, so this
/// lands at the next sign-in (E12.8).
pub fn apply_layout(code: &str) -> std::io::Result<()> {
    let Some(path) = env_path() else {
        return Ok(());
    };
    let body = std::fs::read_to_string(&path).unwrap_or_default();
    let out = upsert_env(&body, "XKB_DEFAULT_LAYOUT", code);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("mde-tmp");
    std::fs::write(&tmp, out.as_bytes())?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// Headless exercise for `mde __kbd-rc`: apply the persisted keyboard settings to
/// rc.xml + the environment file (honouring the seams) and print the rc.xml.
pub fn debug_apply() {
    let st = crate::state::load();
    if let Err(e) = apply_repeat(st.kb_repeat_rate, st.kb_repeat_delay) {
        eprintln!("mde __kbd-rc: {e}");
        return;
    }
    let _ = apply_layout(&st.kb_layout);
    if let Some(p) = crate::mouse::rc_path() {
        if let Ok(s) = std::fs::read_to_string(&p) {
            print!("{s}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KB: &str = "\
<?xml version=\"1.0\"?>
<labwc_config>
  <keyboard>
    <keybind key=\"W-l\"><action name=\"Execute\"><command>mde lock</command></action></keybind>
    <keybind key=\"A-F4\"><action name=\"Close\"/></keybind>
  </keyboard>
  <mouse>
    <default/>
  </mouse>
</labwc_config>
";

    #[test]
    fn inserts_repeat_and_keeps_keybinds() {
        let out = rewrite_keyboard(KB, 30, 400);
        assert!(out.contains("<repeatRate>30</repeatRate>"));
        assert!(out.contains("<repeatDelay>400</repeatDelay>"));
        // Keybinds and the mouse default survive.
        assert!(out.contains("mde lock"));
        assert!(out.contains("key=\"A-F4\""));
        assert!(out.contains("<default/>"));
        // Repeat lines sit inside <keyboard>, before its first keybind.
        let kb = out.find("<keyboard").unwrap();
        let rate = out.find("<repeatRate>").unwrap();
        let first_bind = out.find("<keybind").unwrap();
        assert!(kb < rate && rate < first_bind);
    }

    #[test]
    fn repeat_rewrite_is_idempotent() {
        let once = rewrite_keyboard(KB, 25, 600);
        let twice = rewrite_keyboard(&once, 25, 600);
        assert_eq!(once, twice);
        assert_eq!(twice.matches("<repeatRate>").count(), 1);
        assert_eq!(twice.matches("<repeatDelay>").count(), 1);
    }

    #[test]
    fn changing_rate_replaces_not_appends() {
        let first = rewrite_keyboard(KB, 25, 600);
        let second = rewrite_keyboard(&first, 40, 300);
        assert_eq!(second.matches("<repeatRate>").count(), 1);
        assert!(second.contains("<repeatRate>40</repeatRate>"));
        assert!(!second.contains("<repeatRate>25</repeatRate>"));
    }

    #[test]
    fn env_upsert_replaces_or_appends() {
        // Append into an empty/other body.
        let a = upsert_env("XKB_DEFAULT_MODEL=pc105\n", "XKB_DEFAULT_LAYOUT", "de");
        assert!(a.contains("XKB_DEFAULT_MODEL=pc105"));
        assert!(a.contains("XKB_DEFAULT_LAYOUT=de"));
        // Replace an existing value, not duplicate it.
        let b = upsert_env("XKB_DEFAULT_LAYOUT=us\n", "XKB_DEFAULT_LAYOUT", "fr");
        assert_eq!(b.matches("XKB_DEFAULT_LAYOUT=").count(), 1);
        assert!(b.contains("XKB_DEFAULT_LAYOUT=fr"));
        assert!(!b.contains("XKB_DEFAULT_LAYOUT=us"));
    }
}
