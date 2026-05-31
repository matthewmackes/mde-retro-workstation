//! Windows 2000 "Classic" system palette.
//!
//! These values are the ground truth transcribed from
//! `assets/reference/win2000-classic-colors.ini`. They are kept as plain
//! `(u8, u8, u8)` tuples so this module has no dependency on any GUI toolkit;
//! use [`color`] to convert to an `iced::Color` at the edges.

/// An sRGB 8-bit-per-channel color, `(r, g, b)`.
pub type Rgb = (u8, u8, u8);

// --- Core Win2000 Classic colors (COLOR_* / GetSysColor defaults) ----------
pub const BACKGROUND: Rgb = (0x3a, 0x6e, 0xa5); // desktop
pub const ACTIVE_TITLE: Rgb = (0x0a, 0x24, 0x6a); // focused title bar / Highlight
// Recorded ground truth, but NOT rendered by mde: sway draws title bars as a
// flat `client.focused` color, so the navy→blue gradient caption is the known
// casualty of the mde↔sway boundary (see ACCURACY.md §0). Kept so the value is
// transcribed; it only returns if mde ever draws client-side title rows.
pub const ACTIVE_TITLE_GRADIENT: Rgb = (0xa6, 0xca, 0xf0); // title gradient end (sway-owned)
pub const INACTIVE_TITLE: Rgb = (0x80, 0x80, 0x80);
pub const TITLE_TEXT: Rgb = (0xff, 0xff, 0xff);
pub const INACTIVE_TITLE_TEXT: Rgb = (0xd4, 0xd0, 0xc8);

pub const MENU: Rgb = (0xd4, 0xd0, 0xc8);
pub const MENU_TEXT: Rgb = (0x00, 0x00, 0x00);
pub const WINDOW: Rgb = (0xff, 0xff, 0xff);
pub const WINDOW_TEXT: Rgb = (0x00, 0x00, 0x00);
pub const WINDOW_FRAME: Rgb = (0x00, 0x00, 0x00);

// 3D button/face bevel ramp (light -> dark).
pub const BUTTON_FACE: Rgb = (0xd4, 0xd0, 0xc8);
pub const BUTTON_HILIGHT: Rgb = (0xff, 0xff, 0xff); // brightest bevel
pub const BUTTON_LIGHT: Rgb = (0xdf, 0xdf, 0xdf);
pub const BUTTON_SHADOW: Rgb = (0x80, 0x80, 0x80);
pub const BUTTON_DK_SHADOW: Rgb = (0x40, 0x40, 0x40); // darkest bevel
pub const BUTTON_TEXT: Rgb = (0x00, 0x00, 0x00);

pub const HIGHLIGHT: Rgb = (0x0a, 0x24, 0x6a); // selection
pub const HIGHLIGHT_TEXT: Rgb = (0xff, 0xff, 0xff);
pub const GRAY_TEXT: Rgb = (0x80, 0x80, 0x80); // disabled

pub const INFO_TEXT: Rgb = (0x00, 0x00, 0x00); // tooltip
pub const INFO_WINDOW: Rgb = (0xff, 0xff, 0xe1);
pub const URGENT: Rgb = (0x80, 0x00, 0x00); // MDE-Retro: urgent window (maroon)

// --- MDE-Retro app chrome (NOT GetSysColor) --------------------------------
// Colors for surfaces Windows 2000 drew with bespoke art rather than a system
// color: the Explorer / Control-Panel "web view" info band and the Setup
// wizard's blue. They live here, separated from the system table above, so that
// NOTHING outside this module names a raw hex value.
/// The Explorer / Control Panel web-view info band (left blue pane).
pub const INFO_BAND: Rgb = (0x1d, 0x5c, 0xa8);
/// GUI Setup background gradient (top → bottom).
pub const SETUP_GRADIENT_TOP: Rgb = (0x1c, 0x4a, 0x8f);
pub const SETUP_GRADIENT_BOTTOM: Rgb = (0x08, 0x16, 0x40);
/// GUI Setup progress-bar fill, and the dimmed (pending/subtitle) text on it.
pub const SETUP_PROGRESS: Rgb = (0x16, 0x3a, 0xa8);
pub const SETUP_SUBTITLE: Rgb = (0x9e, 0xb2, 0xdb);

/// The Start-button "flying windows" flag panes (red/green/blue/yellow). Brand
/// art, not a GetSysColor value — drawn as quads because the UI font has no
/// flag glyph (see `widget::flag`).
pub const LOGO_RED: Rgb = (0xe8, 0x44, 0x32);
pub const LOGO_GREEN: Rgb = (0x6f, 0xb1, 0x2e);
pub const LOGO_BLUE: Rgb = (0x2a, 0x7d, 0xe1);
pub const LOGO_YELLOW: Rgb = (0xf2, 0xc4, 0x1d);

/// Convert a palette [`Rgb`] into an `iced::Color`.
pub fn color(rgb: Rgb) -> iced::Color {
    iced::Color::from_rgb8(rgb.0, rgb.1, rgb.2)
}
