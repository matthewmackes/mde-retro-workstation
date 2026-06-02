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
