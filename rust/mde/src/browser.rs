//! Default-web-browser surfacing for the Windows 10 era (E18). Reads the system
//! default browser via `xdg-settings` and maps its `.desktop` id to a display
//! name + an icon key, and builds the commands to make Firefox the default. The
//! shell ships Firefox, so an unknown/empty default falls back to Firefox.
//!
//!   mde browser-default              print the default browser's name
//!   mde browser-default --icon       print its icon key (for icon_any)
//!   mde browser-default --set-default  make Firefox the default browser
//!   mde browser-default <url>        open the url in the default browser

use std::process::{Command, ExitCode};

/// The resolved default browser: a display name + an icon key for `icons::icon_any`.
pub struct Browser {
    pub name: String,
    pub icon: String,
}

/// Map a `.desktop` id (e.g. `firefox.desktop`) to a display name + icon key.
/// Empty/unknown ids fall back to Firefox (the shipped browser) so the surface is
/// never blank; a recognized-but-unbundled browser keeps its own name + a generic
/// `web-browser` icon.
fn id_to_browser(desktop_id: &str) -> Browser {
    let b = |name: &str, icon: &str| Browser {
        name: name.to_string(),
        icon: icon.to_string(),
    };
    match desktop_id.trim().trim_end_matches(".desktop") {
        "" | "firefox" | "org.mozilla.firefox" | "firefox-esr" => b("Firefox", "firefox"),
        "google-chrome" | "google-chrome-stable" => b("Google Chrome", "google-chrome"),
        "chromium" | "chromium-browser" => b("Chromium", "chromium"),
        "brave-browser" => b("Brave", "brave-browser"),
        "microsoft-edge" => b("Microsoft Edge", "microsoft-edge"),
        other => b(other, "web-browser"),
    }
}

/// The current default web browser (`xdg-settings get default-web-browser`),
/// mapped to a name + icon. Falls back to Firefox when the tool/setting is absent.
pub fn default_browser() -> Browser {
    let id = Command::new("xdg-settings")
        .args(["get", "default-web-browser"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    id_to_browser(&id)
}

/// Open a URL in the system default browser (fire-and-forget, like the rest of the
/// shell's launchers). `xdg-open` honours whatever the default is.
pub fn launch_url(url: &str) {
    let _ = Command::new("xdg-open").arg(url).spawn();
}

/// The shell command that opens `url` (what [`launch_url`] runs) — the popup runs
/// jump-list item commands through `sh -c`, so the URL is single-quote-escaped
/// (E18.4).
pub fn open_url_cmd(url: &str) -> String {
    format!("xdg-open '{}'", url.replace('\'', "'\\''"))
}

/// The Firefox `places.sqlite` to read history from. `MDE_PLACES_DB` overrides it
/// (a capture/test seam); otherwise the `default`-named profile under
/// `~/.mozilla/firefox/`, else any profile that has one.
fn places_db() -> Option<std::path::PathBuf> {
    if let Some(p) = std::env::var_os("MDE_PLACES_DB") {
        return Some(std::path::PathBuf::from(p));
    }
    let home = std::env::var_os("HOME")?;
    let ff = std::path::PathBuf::from(home).join(".mozilla/firefox");
    let mut fallback = None;
    for e in std::fs::read_dir(&ff).ok()?.flatten() {
        let db = e.path().join("places.sqlite");
        if !db.is_file() {
            continue;
        }
        if e.file_name()
            .to_string_lossy()
            .to_lowercase()
            .contains("default")
        {
            return Some(db);
        }
        fallback = Some(db);
    }
    fallback
}

/// The most-recently-visited sites as `(title, url)`, newest first, capped at 8
/// (E18.4). Returns empty on any failure (no profile, unreadable DB, …) so the
/// jump list's Recent section omits itself cleanly. Reads read-only from a copy of
/// `places.sqlite` opened `?immutable=1`, since Firefox holds a write lock on the
/// live file.
pub fn recent_sites() -> Vec<(String, String)> {
    let Some(db) = places_db() else {
        return Vec::new();
    };
    let tmp = std::env::temp_dir().join("mde-places-copy.sqlite");
    if std::fs::copy(&db, &tmp).is_err() {
        return Vec::new();
    }
    let out = read_recent(&tmp);
    let _ = std::fs::remove_file(&tmp);
    out
}

/// Headless helper for `mde __places-recent [--seed <path>]` (E18.4): with `--seed`
/// it writes a small fixture `places.sqlite` (so captures don't need a real Firefox
/// profile or the `sqlite3` CLI); otherwise it prints what `recent_sites` reads.
pub fn debug_recent(args: &[String]) {
    if let Some(i) = args.iter().position(|a| a == "--seed") {
        if let Some(path) = args.get(i + 1) {
            if let Ok(conn) = rusqlite::Connection::open(path) {
                let _ = conn.execute_batch(
                    "CREATE TABLE IF NOT EXISTS moz_places(id INTEGER PRIMARY KEY, url TEXT, title TEXT, last_visit_date INTEGER);\n\
                     DELETE FROM moz_places;\n\
                     INSERT INTO moz_places(url,title,last_visit_date) VALUES\n\
                      ('https://github.com/matthewmackes/mde-retro-workstation','MDE-Retro on GitHub',1780000000000000),\n\
                      ('https://www.rust-lang.org/','Rust Programming Language',1779000000000000),\n\
                      ('https://docs.rs/iced','iced — Rust GUI',1778000000000000),\n\
                      ('https://en.wikipedia.org/wiki/Windows_2000','Windows 2000 - Wikipedia',1777000000000000);",
                );
                println!("seeded {path}");
            }
            return;
        }
    }
    for (title, url) in recent_sites() {
        println!("{title}  <{url}>");
    }
}

fn read_recent(path: &std::path::Path) -> Vec<(String, String)> {
    use rusqlite::OpenFlags;
    let uri = format!("file:{}?immutable=1", path.display());
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI;
    let Ok(conn) = rusqlite::Connection::open_with_flags(uri, flags) else {
        return Vec::new();
    };
    let sql = "SELECT url, title FROM moz_places \
               WHERE last_visit_date IS NOT NULL AND url LIKE 'http%' \
               ORDER BY last_visit_date DESC LIMIT 8";
    let Ok(mut stmt) = conn.prepare(sql) else {
        return Vec::new();
    };
    let rows = stmt.query_map([], |r| {
        let url: String = r.get(0)?;
        let title: Option<String> = r.get(1)?;
        let label = title
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| url.clone());
        Ok((label, url))
    });
    match rows {
        Ok(it) => it.flatten().collect(),
        Err(_) => Vec::new(),
    }
}

/// The commands that make Firefox the default browser + http(s)/html handler,
/// as (program, args) pairs — returned (not run) so a test can assert them exactly
/// without mutating the session's real default.
fn set_default_cmds() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        (
            "xdg-settings",
            vec!["set", "default-web-browser", "firefox.desktop"],
        ),
        (
            "xdg-mime",
            vec!["default", "firefox.desktop", "x-scheme-handler/http"],
        ),
        (
            "xdg-mime",
            vec!["default", "firefox.desktop", "x-scheme-handler/https"],
        ),
        ("xdg-mime", vec!["default", "firefox.desktop", "text/html"]),
    ]
}

/// Make Firefox the default browser (runs `set_default_cmds`). Best-effort.
pub fn set_default() {
    for (prog, args) in set_default_cmds() {
        let _ = Command::new(prog).args(args).status();
    }
}

pub fn run(args: &[String]) -> ExitCode {
    if args.iter().any(|a| a == "--set-default") {
        set_default();
        println!("Set Firefox as the default web browser.");
    } else if args.iter().any(|a| a == "--icon") {
        println!("{}", default_browser().icon);
    } else if let Some(url) = args.iter().find(|a| !a.starts_with("--")) {
        launch_url(url);
    } else {
        println!("{}", default_browser().name);
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_url_cmd_shell_quotes() {
        assert_eq!(
            open_url_cmd("https://x.com/a"),
            "xdg-open 'https://x.com/a'"
        );
        // A single quote in the URL is escaped, so `sh -c` can't break out.
        assert_eq!(
            open_url_cmd("https://x.com/it's"),
            "xdg-open 'https://x.com/it'\\''s'"
        );
    }

    #[test]
    fn desktop_id_maps_to_name_and_icon() {
        // The shipped browser + its fallbacks.
        let f = id_to_browser("firefox.desktop");
        assert_eq!(f.name, "Firefox");
        assert_eq!(f.icon, "firefox");
        // Empty / unknown id → Firefox fallback (never blank).
        assert_eq!(id_to_browser("").name, "Firefox");
        // A recognized non-bundled browser keeps its name.
        assert_eq!(id_to_browser("google-chrome.desktop").name, "Google Chrome");
        // An unrecognized id keeps its stem + the generic web-browser icon.
        let u = id_to_browser("falkon.desktop");
        assert_eq!(u.name, "falkon");
        assert_eq!(u.icon, "web-browser");
    }

    #[test]
    fn set_default_builds_the_exact_xdg_commands() {
        // Pin the argv so a test covers the side effect without mutating the
        // session default (E18.2). Web-browser handler + http/https/html mime.
        let cmds = set_default_cmds();
        assert_eq!(
            cmds,
            vec![
                (
                    "xdg-settings",
                    vec!["set", "default-web-browser", "firefox.desktop"]
                ),
                (
                    "xdg-mime",
                    vec!["default", "firefox.desktop", "x-scheme-handler/http"]
                ),
                (
                    "xdg-mime",
                    vec!["default", "firefox.desktop", "x-scheme-handler/https"]
                ),
                ("xdg-mime", vec!["default", "firefox.desktop", "text/html"]),
            ]
        );
    }
}
