//! Installed-application scanner for the Programs ▶ submenu.
//!
//! Reads .desktop files from the standard application directories and groups
//! them into Win2000-ish Programs folders (Accessories, Internet, Office, ...)
//! by their freedesktop Categories.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct App {
    pub name: String,
    pub exec: String,
    pub terminal: bool,
}

/// Programs grouped by folder, in menu order.
pub fn programs() -> Vec<(String, Vec<App>)> {
    let mut by_cat: BTreeMap<String, Vec<App>> = BTreeMap::new();
    for dir in app_dirs() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("desktop") {
                continue;
            }
            if let Some((app, cat)) = parse(&path) {
                by_cat.entry(cat).or_default().push(app);
            }
        }
    }

    let order = [
        "Accessories",
        "Internet",
        "Office",
        "Graphics",
        "Multimedia",
        "Programming",
        "Games",
        "Other",
    ];
    let mut out: Vec<(String, Vec<App>)> = by_cat
        .into_iter()
        .map(|(cat, mut apps)| {
            apps.sort_by_key(|a| a.name.to_lowercase());
            apps.dedup_by(|a, b| a.name == b.name);
            (cat, apps)
        })
        .collect();
    out.sort_by_key(|(c, _)| order.iter().position(|o| o == c).unwrap_or(usize::MAX));
    out
}

fn app_dirs() -> Vec<PathBuf> {
    let mut v = vec![
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        v.push(PathBuf::from(home).join(".local/share/applications"));
    }
    v
}

fn parse(path: &PathBuf) -> Option<(App, String)> {
    let text = fs::read_to_string(path).ok()?;
    let (mut name, mut exec, mut typ): (Option<String>, Option<String>, Option<String>) =
        (None, None, None);
    let mut cats = String::new();
    let (mut terminal, mut nodisplay) = (false, false);
    let mut in_entry = false;

    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_entry = l == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        if let Some(v) = l.strip_prefix("Name=") {
            name.get_or_insert_with(|| v.to_string());
        } else if let Some(v) = l.strip_prefix("Exec=") {
            exec.get_or_insert_with(|| v.to_string());
        } else if let Some(v) = l.strip_prefix("Categories=") {
            cats = v.to_string();
        } else if let Some(v) = l.strip_prefix("Terminal=") {
            terminal = v.eq_ignore_ascii_case("true");
        } else if let Some(v) = l.strip_prefix("NoDisplay=") {
            nodisplay |= v.eq_ignore_ascii_case("true");
        } else if let Some(v) = l.strip_prefix("Hidden=") {
            nodisplay |= v.eq_ignore_ascii_case("true");
        } else if let Some(v) = l.strip_prefix("Type=") {
            typ.get_or_insert_with(|| v.to_string());
        }
    }

    if nodisplay || typ.as_deref() != Some("Application") {
        return None;
    }
    let name = name?;
    let exec = exec?
        .split_whitespace()
        .filter(|t| !t.starts_with('%'))
        .collect::<Vec<_>>()
        .join(" ");
    if exec.is_empty() {
        return None;
    }
    Some((
        App {
            name,
            exec,
            terminal,
        },
        category(&cats),
    ))
}

fn category(cats: &str) -> String {
    let has = |k: &str| cats.split(';').any(|x| x == k);
    if has("Network") || has("WebBrowser") || has("Email") {
        "Internet"
    } else if has("AudioVideo") || has("Audio") || has("Video") {
        "Multimedia"
    } else if has("Development") {
        "Programming"
    } else if has("Graphics") {
        "Graphics"
    } else if has("Office") {
        "Office"
    } else if has("Game") {
        "Games"
    } else if has("Utility") || has("Accessories") || has("System") || has("Settings") {
        "Accessories"
    } else {
        "Other"
    }
    .to_string()
}
