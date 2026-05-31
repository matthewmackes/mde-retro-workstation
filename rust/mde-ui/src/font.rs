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

/// The family name the bundled TTFs register under.
pub const FAMILY: &str = "Droid Sans";

/// The default UI font (Tahoma stand-in).
pub const UI: Font = Font {
    family: Family::Name(FAMILY),
    ..Font::DEFAULT
};

/// Bold UI font (title bars, menu section headers).
pub const UI_BOLD: Font = Font {
    family: Family::Name(FAMILY),
    weight: Weight::Bold,
    ..Font::DEFAULT
};

/// The Nerd Font family used for notification-area glyph icons (volume,
/// network, battery, and SNI tray items). Hack Nerd Font ships the Font Awesome
/// + Material Design Icon glyph ranges; it's loaded from the system at startup
/// (see `panel::nerd_font_bytes`) and referenced here by family name.
pub const NERD_FAMILY: &str = "Hack Nerd Font";

/// The Nerd Font as an iced [`Font`].
pub const NERD: Font = Font {
    family: Family::Name(NERD_FAMILY),
    ..Font::DEFAULT
};
