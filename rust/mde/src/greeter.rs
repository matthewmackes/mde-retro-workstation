//! LightDM-gtk-greeter "Windows 10" theme generator (E10.9).
//!
//! Emits the greeter's GTK CSS and its `lightdm-gtk-greeter.conf` with every
//! colour sourced from the live palette via [`palette::hex`] — no hand-written hex
//! (§2.1). The greeter is always the Windows 10 look (which shares Carbon's dark
//! colouring after the MackesDE rebrand — `palette::win10` was retired), so the
//! generator forces `Theme::Windows10` before emitting, independent of the user's
//! desktop theme.
//!
//! `tests/stage-greeter-assets.sh` runs this to write `assets/greeter/win10.css`
//! and `assets/greeter/lightdm-gtk-greeter.conf`; E10.10 installs them. A grim of
//! the live greeter (user list + ~/.face + clock) needs a real LightDM session and
//! is a manual/integration check, not harness-observable.
//!
//!   mde greeter --css            print the GTK theme CSS
//!   mde greeter --conf [BG]      print the greeter conf (optional wallpaper path)

use std::process::ExitCode;

use mde_ui::palette::{self, Theme};

/// The GTK-3 CSS for the `win10` greeter theme, palette-sourced. The colours go
/// through `@define-color` so the rule block reads them by name and the staging
/// test can assert each definition equals `palette::hex(role)`.
pub fn css() -> String {
    let bg = palette::hex(palette::WINDOW);
    let fg = palette::hex(palette::WINDOW_TEXT);
    let accent = palette::hex(palette::HIGHLIGHT);
    let accent_fg = palette::hex(palette::HIGHLIGHT_TEXT);
    let field = palette::hex(palette::BUTTON_FACE);
    let frame = palette::hex(palette::WINDOW_FRAME);
    let dim = palette::hex(palette::GRAY_TEXT);
    format!(
        "/* MDE-Retro Windows 10 greeter theme — generated (E10.9). Do not hand-edit;\n\
         regenerate with tests/stage-greeter-assets.sh. Colours are palette-sourced. */\n\
         @define-color mde_bg {bg};\n\
         @define-color mde_fg {fg};\n\
         @define-color mde_accent {accent};\n\
         @define-color mde_accent_fg {accent_fg};\n\
         @define-color mde_field {field};\n\
         @define-color mde_frame {frame};\n\
         @define-color mde_dim {dim};\n\
         \n\
         window, #panel_window, #login_window, #content_frame {{\n\
         \tbackground-color: @mde_bg;\n\
         \tcolor: @mde_fg;\n\
         }}\n\
         label {{ color: @mde_fg; }}\n\
         label.gtk-tooltip, .prompt {{ color: @mde_dim; }}\n\
         #clock_label {{ color: @mde_fg; font-size: 32px; font-weight: 300; }}\n\
         entry {{\n\
         \tbackground-color: @mde_field;\n\
         \tcolor: @mde_fg;\n\
         \tborder: 1px solid @mde_frame;\n\
         \tborder-radius: 2px;\n\
         \tpadding: 6px 8px;\n\
         }}\n\
         entry:focus {{ border-color: @mde_accent; }}\n\
         button {{\n\
         \tbackground-color: @mde_field;\n\
         \tcolor: @mde_fg;\n\
         \tborder: 1px solid @mde_frame;\n\
         \tborder-radius: 2px;\n\
         \tpadding: 4px 12px;\n\
         }}\n\
         button:hover, button:focus {{ background-color: @mde_accent; color: @mde_accent_fg; }}\n\
         #panel_window {{ border-bottom: 1px solid @mde_frame; }}\n"
    )
}

/// The `lightdm-gtk-greeter.conf` `[greeter]` block selecting the `win10` theme,
/// IBM Plex Sans, a 12-hour clock, and (optionally) a background image.
pub fn conf(wallpaper: &str) -> String {
    let bg = if wallpaper.is_empty() {
        String::new()
    } else {
        format!("background = {wallpaper}\n")
    };
    format!(
        "# MDE-Retro Windows 10 greeter — generated (E10.9). Regenerate with\n\
         # tests/stage-greeter-assets.sh; installed by the RPM %post (E10.10).\n\
         [greeter]\n\
         theme-name = win10\n\
         icon-theme-name = Adwaita\n\
         font-name = IBM Plex Sans 11\n\
         xft-antialias = true\n\
         xft-hintstyle = slight\n\
         clock-format = %l:%M %p\n\
         indicators = ~host;~spacer;~clock;~spacer;~session;~power\n\
         position = 50%,center 55%,center\n\
         {bg}"
    )
}

/// Headless entry for `mde greeter …`.
pub fn run(args: &[String]) -> ExitCode {
    // The greeter is always the Windows 10 look, regardless of the desktop theme.
    palette::set_theme(Theme::Windows10);
    if args.iter().any(|a| a == "--css") {
        print!("{}", css());
        return ExitCode::SUCCESS;
    }
    if let Some(i) = args.iter().position(|a| a == "--conf") {
        let wp = args.get(i + 1).filter(|s| !s.starts_with("--")).cloned();
        print!("{}", conf(wp.as_deref().unwrap_or("")));
        return ExitCode::SUCCESS;
    }
    eprintln!("usage: mde greeter --css | --conf [wallpaper]");
    ExitCode::FAILURE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn css_colours_are_palette_sourced() {
        // Win10 == Carbon colours after the rebrand, so this can't perturb a
        // parallel colour-reading test (both themes resolve identically).
        palette::set_theme(Theme::Windows10);
        let css = css();
        // Every @define-color must equal palette::hex(role) — the "diff CSS vars
        // against win10() outputs" bench, proving the colours are palette-sourced.
        let known: std::collections::HashSet<String> = [
            palette::WINDOW,
            palette::WINDOW_TEXT,
            palette::HIGHLIGHT,
            palette::HIGHLIGHT_TEXT,
            palette::BUTTON_FACE,
            palette::WINDOW_FRAME,
            palette::GRAY_TEXT,
        ]
        .iter()
        .map(|r| palette::hex(*r))
        .collect();
        for h in &known {
            assert!(
                css.contains(h.as_str()),
                "greeter CSS must use the palette hex {h}"
            );
        }
        // Conversely, every `#rrggbb` literal must be a palette colour (no hand-hex,
        // §2.1). GTK `#id` selectors are followed by letters, so the 7-char
        // `#` + 6-hex-digit shape only matches real colour literals.
        let bytes = css.as_bytes();
        for (i, _) in css.match_indices('#') {
            if i + 7 <= css.len() && bytes[i + 1..i + 7].iter().all(u8::is_ascii_hexdigit) {
                let lit = &css[i..i + 7];
                assert!(
                    known.contains(lit),
                    "stray non-palette hex in greeter CSS: {lit}"
                );
            }
        }
        palette::set_theme(Theme::Carbon); // restore default
    }

    #[test]
    fn conf_selects_the_theme_and_optional_background() {
        let c = conf("/usr/share/mde/greeter/win10-bg.png");
        assert!(c.contains("theme-name = win10"));
        assert!(c.contains("background = /usr/share/mde/greeter/win10-bg.png"));
        // No background line when none is given.
        assert!(!conf("").contains("background ="));
    }
}
