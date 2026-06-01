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
// (0xff,0xff,0xfe) is a SENTINEL — visually pure white in Win2000/BeOS, but a
// distinct key so the Carbon remap can tell "white text on a colored/dark
// surface" (must stay light) apart from WINDOW (a white *surface* that must
// darken in dark mode). Win2000 ground truth is still white. See `carbon()`.
pub const TITLE_TEXT: Rgb = (0xff, 0xff, 0xfe);
pub const INACTIVE_TITLE_TEXT: Rgb = (0xd4, 0xd0, 0xc8);

pub const MENU: Rgb = (0xd4, 0xd0, 0xc8);
pub const MENU_TEXT: Rgb = (0x00, 0x00, 0x00);
pub const WINDOW: Rgb = (0xff, 0xff, 0xff);
pub const WINDOW_TEXT: Rgb = (0x00, 0x00, 0x00);
// (0x00,0x00,0x01) is a SENTINEL — visually pure black in Win2000/BeOS, but a
// distinct key so the Carbon remap can tell a window/border FRAME apart from
// black TEXT (WINDOW_TEXT etc.), which must lighten in dark mode while frames
// become a subtle border gray. Win2000 ground truth is still black.
pub const WINDOW_FRAME: Rgb = (0x00, 0x00, 0x01);

// 3D button/face bevel ramp (light -> dark).
pub const BUTTON_FACE: Rgb = (0xd4, 0xd0, 0xc8);
pub const BUTTON_HILIGHT: Rgb = (0xff, 0xff, 0xff); // brightest bevel
pub const BUTTON_LIGHT: Rgb = (0xdf, 0xdf, 0xdf);
pub const BUTTON_SHADOW: Rgb = (0x80, 0x80, 0x80);
pub const BUTTON_DK_SHADOW: Rgb = (0x40, 0x40, 0x40); // darkest bevel
pub const BUTTON_TEXT: Rgb = (0x00, 0x00, 0x00);

pub const HIGHLIGHT: Rgb = (0x0a, 0x24, 0x6a); // selection
// SENTINEL white text (see TITLE_TEXT) — selection text stays white on the
// accent fill in both Carbon light and dark.
pub const HIGHLIGHT_TEXT: Rgb = (0xff, 0xff, 0xfe);
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

/// The shell bar / UI Shell header surface. Identity value is the Win2000 silver
/// taskbar face (a distinct key from `BUTTON_FACE` so the Carbon remap can paint
/// the header its own Gray 100 / white). Under Carbon it becomes the flat header
/// strip; under Win2000/BeOS it reads as the classic silver bar.
pub const SHELL_HEADER: Rgb = (0xd4, 0xd0, 0xc7);

// --- Runtime theme switch --------------------------------------------------
// The palette constants above are the canonical Win2000 role keys. Alternate
// themes are applied by remapping those role colors at the `color()` edge — so
// no call site changes and every surface retints together. The active shell
// binary selects the theme at startup from persisted state (see mde state.rs /
// main.rs). Three themes exist: Windows 2000 (identity), BeOS, and IBM Carbon
// (with a light/dark mode and a selectable accent hue).
use std::sync::atomic::{AtomicU8, Ordering};

/// The active shell theme.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Theme {
    Win2000,
    Beos,
    Carbon,
}

// Packed active theme + Carbon mode/accent in plain atomics (read on the draw
// hot path, so lock-free). THEME: 0=Win2000 1=Beos 2=Carbon. DARK: Carbon mode.
static THEME: AtomicU8 = AtomicU8::new(0);
static DARK: AtomicU8 = AtomicU8::new(1); // Carbon default mode = dark
static ACCENT: AtomicU8 = AtomicU8::new(0); // 0=blue 1=orange 2=red 3=neutral (icon accent)

/// Select the active theme.
pub fn set_theme(theme: Theme) {
    THEME.store(theme as u8, Ordering::Relaxed);
}

/// The active theme.
pub fn theme() -> Theme {
    match THEME.load(Ordering::Relaxed) {
        1 => Theme::Beos,
        2 => Theme::Carbon,
        _ => Theme::Win2000,
    }
}

/// Set Carbon mode: dark (true) or light (false). No effect outside Carbon.
pub fn set_dark(on: bool) {
    DARK.store(on as u8, Ordering::Relaxed);
}

/// Whether Carbon is in dark mode.
pub fn is_dark() -> bool {
    DARK.load(Ordering::Relaxed) != 0
}

/// Set the icon accent hue (0=blue 1=orange 2=red 3=neutral). Consumed by the
/// shell's icon tinting; the UI accent itself is always Carbon Blue 60.
pub fn set_accent(idx: u8) {
    ACCENT.store(idx, Ordering::Relaxed);
}

/// The icon accent index (0=blue 1=orange 2=red 3=neutral).
pub fn accent_idx() -> u8 {
    ACCENT.load(Ordering::Relaxed)
}

/// Whether the BeOS theme is active.
pub fn is_beos() -> bool {
    theme() == Theme::Beos
}

/// Whether the Carbon theme is active.
pub fn is_carbon() -> bool {
    theme() == Theme::Carbon
}

/// Map a Win2000 role color to its BeOS equivalent (light-gray panels, softer
/// bevels, a blue selection; white/black pass through). Roles that share a
/// Win2000 value share the BeOS value — they're the same surface concept.
fn beos(rgb: Rgb) -> Rgb {
    match rgb {
        (0xd4, 0xd0, 0xc8) => (0xd8, 0xd8, 0xd8), // panel / menu / button face
        (0xdf, 0xdf, 0xdf) => (0xec, 0xec, 0xec), // inner bevel light
        (0x80, 0x80, 0x80) => (0x8c, 0x8c, 0x8c), // bevel shadow / disabled / inactive
        (0x40, 0x40, 0x40) => (0x55, 0x55, 0x55), // dark bevel
        (0x0a, 0x24, 0x6a) => (0x33, 0x55, 0x9c), // selection / accent (BeOS blue)
        (0x3a, 0x6e, 0xa5) => (0x33, 0x66, 0x98), // desktop
        (0x1d, 0x5c, 0xa8) => (0x46, 0x7a, 0xbe), // web-view info band
        other => other,                            // white, black, brand art, etc.
    }
}

/// The Carbon Blue 60 interactive accent for the active mode (UI accent — drives
/// selection, focus, primary buttons, links). Always blue regardless of the
/// separate *icon* accent hue.
pub fn carbon_accent() -> Rgb {
    if is_dark() {
        (0x45, 0x89, 0xff) // Blue 50/60 on dark
    } else {
        (0x0f, 0x62, 0xfe) // Blue 60 on light
    }
}

/// Map a Win2000 role color to its IBM Carbon token, per the active light/dark
/// mode. Tuple keys are the canonical Win2000 role values (note the white/black
/// text vs surface SENTINELS above, which let text stay legible after surfaces
/// invert in dark mode). Tokens follow Carbon Gray 10 (light) / Gray 90 (dark).
fn carbon(rgb: Rgb) -> Rgb {
    let dark = is_dark();
    let accent = carbon_accent();
    match rgb {
        // Selection / title / accent roles -> Carbon Blue 60.
        (0x0a, 0x24, 0x6a) => accent, // HIGHLIGHT + ACTIVE_TITLE
        (0xa6, 0xca, 0xf0) => accent, // ACTIVE_TITLE_GRADIENT
        (0x1d, 0x5c, 0xa8) => accent, // INFO_BAND (web-view accent/links)
        // White TEXT on a colored/dark surface (sentinel) -> stays white.
        (0xff, 0xff, 0xfe) => (0xff, 0xff, 0xff), // TITLE_TEXT + HIGHLIGHT_TEXT
        // Window-frame / border (sentinel black) -> subtle border gray.
        (0x00, 0x00, 0x01) => if dark { (0x52, 0x52, 0x52) } else { (0x8d, 0x8d, 0x8d) },
        // Black text roles -> text-primary (invert in dark).
        (0x00, 0x00, 0x00) => if dark { (0xf4, 0xf4, 0xf4) } else { (0x16, 0x16, 0x16) },
        // White surfaces (WINDOW / BUTTON_HILIGHT) -> field / layer-01.
        (0xff, 0xff, 0xff) => if dark { (0x39, 0x39, 0x39) } else { (0xff, 0xff, 0xff) },
        // Silver panel / menu / button face / inactive title text -> layer.
        (0xd4, 0xd0, 0xc8) => if dark { (0x39, 0x39, 0x39) } else { (0xf4, 0xf4, 0xf4) },
        // Shell/UI-Shell header -> Gray 100 (dark) / white (light).
        (0xd4, 0xd0, 0xc7) => if dark { (0x16, 0x16, 0x16) } else { (0xff, 0xff, 0xff) },
        // Inner bevel light -> hover layer (mostly unused once flattened).
        (0xdf, 0xdf, 0xdf) => if dark { (0x47, 0x47, 0x47) } else { (0xe8, 0xe8, 0xe8) },
        // Bevel shadow / disabled / inactive -> text-secondary / border-strong.
        (0x80, 0x80, 0x80) => if dark { (0x6f, 0x6f, 0x6f) } else { (0x8d, 0x8d, 0x8d) },
        // Dark bevel -> border-subtle.
        (0x40, 0x40, 0x40) => if dark { (0x52, 0x52, 0x52) } else { (0x6f, 0x6f, 0x6f) },
        // Desktop background -> deepest gray (dark) / light gray (light).
        (0x3a, 0x6e, 0xa5) => if dark { (0x16, 0x16, 0x16) } else { (0xd0, 0xd0, 0xd0) },
        // Tooltip background -> layer.
        (0xff, 0xff, 0xe1) => if dark { (0x39, 0x39, 0x39) } else { (0xff, 0xff, 0xff) },
        // Urgent / error -> Carbon danger red.
        (0x80, 0x00, 0x00) => if dark { (0xfa, 0x4d, 0x56) } else { (0xda, 0x1e, 0x28) },
        // Setup-wizard blues -> accent family.
        (0x1c, 0x4a, 0x8f) => if dark { (0x00, 0x43, 0xce) } else { (0x0f, 0x62, 0xfe) },
        (0x08, 0x16, 0x40) => if dark { (0x00, 0x11, 0x41) } else { (0x00, 0x2d, 0x9c) },
        (0x16, 0x3a, 0xa8) => accent,
        (0x9e, 0xb2, 0xdb) => if dark { (0xa6, 0xc8, 0xff) } else { (0x52, 0x52, 0x52) },
        // Brand flag art and anything else -> unchanged.
        other => other,
    }
}

/// Convert a palette [`Rgb`] into an `iced::Color`, applying the active theme.
pub fn color(rgb: Rgb) -> iced::Color {
    let rgb = match theme() {
        Theme::Win2000 => rgb,
        Theme::Beos => beos(rgb),
        Theme::Carbon => carbon(rgb),
    };
    iced::Color::from_rgb8(rgb.0, rgb.1, rgb.2)
}

/// The UI accent as an `iced::Color` (Carbon Blue 60 under Carbon; the Win2000
/// navy HIGHLIGHT otherwise). Convenience for accent underlines/focus rings.
pub fn accent() -> iced::Color {
    color(HIGHLIGHT)
}
