//! The MDE-Retro UI font.
//!
//! Windows 2000's UI font is Tahoma, which isn't freely distributable. Like the
//! rest of the platform (see `fontconfig/fonts.conf`), we substitute Droid Sans
//! (Apache-2.0), a humanist sans close to Tahoma's proportions. The TTFs are
//! bundled so the look is reproducible without relying on system font discovery.

use iced::font::{Family, Weight};
use iced::Font;

/// Regular face, registered at startup via the app builder's `.font(...)`.
pub const REGULAR_BYTES: &[u8] = include_bytes!("../fonts/DroidSans.ttf");
/// Bold face.
pub const BOLD_BYTES: &[u8] = include_bytes!("../fonts/DroidSans-Bold.ttf");

/// The Windows 2000 UI font (Tahoma stand-in).
pub const FAMILY: &str = "Droid Sans";

/// IBM Plex Sans — the BeOS-theme UI font (OFL), bundled and registered too.
pub const PLEX_REGULAR_BYTES: &[u8] = include_bytes!("../fonts/IBMPlexSans-Regular.ttf");
pub const PLEX_BOLD_BYTES: &[u8] = include_bytes!("../fonts/IBMPlexSans-Bold.ttf");
pub const PLEX_FAMILY: &str = "IBM Plex Sans";

/// The active UI font family — IBM Plex Sans under the BeOS, Carbon, and Windows
/// 10 themes (Win10's Segoe UI target is unshippable, so it reuses the already-
/// bundled Plex face per §2.4 — see `metrics::UI_FONT_TARGET_WIN10`), else Droid
/// Sans. Both are registered at startup, so switching is just the family name.
fn family() -> &'static str {
    if crate::palette::is_beos() || crate::palette::is_carbon() || crate::palette::is_windows10() {
        PLEX_FAMILY
    } else {
        FAMILY
    }
}

/// The default UI font.
pub fn ui() -> Font {
    Font {
        family: Family::Name(family()),
        ..Font::DEFAULT
    }
}

/// Bold UI font (title bars, menu section headers).
pub fn ui_bold() -> Font {
    Font {
        family: Family::Name(family()),
        weight: Weight::Bold,
        ..Font::DEFAULT
    }
}

/// The Nerd Font family used for notification-area glyph icons (volume,
/// network, battery, and SNI tray items). Hack Nerd Font ships the Font Awesome +
/// Material Design Icon glyph ranges; it's loaded from the system at startup
/// (see `panel::nerd_font_bytes`) and referenced here by family name.
pub const NERD_FAMILY: &str = "Hack Nerd Font";

/// The Nerd Font as an iced [`Font`].
pub const NERD: Font = Font {
    family: Family::Name(NERD_FAMILY),
    ..Font::DEFAULT
};
