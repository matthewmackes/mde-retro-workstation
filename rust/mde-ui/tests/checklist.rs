//! Static accuracy checklist (layer 1 of `rust/ACCURACY.md`).
//!
//! These tests pin the Windows 2000 Classic ground truth — the exact palette
//! values and UI metrics transcribed from
//! `assets/reference/win2000-classic-colors.ini` and the classic `SM_*`
//! system metrics. They have no Wayland or GUI dependency, so they gate every
//! build: any accidental drift in a color or metric fails CI immediately.
//!
//! The dynamic screenshot spot-check (layer 2) lives in the `mde` crate and
//! validates that the *rendered* output actually paints these values.

use mde_ui::metrics;
use mde_ui::palette::{self, Rgb};

// --- Palette ---------------------------------------------------------------

#[test]
fn desktop_background_is_win2000_blue() {
    assert_eq!(palette::BACKGROUND, (0x3a, 0x6e, 0xa5));
}

#[test]
fn active_title_is_navy_with_blue_gradient_end() {
    assert_eq!(palette::ACTIVE_TITLE, (0x0a, 0x24, 0x6a));
    assert_eq!(palette::ACTIVE_TITLE_GRADIENT, (0xa6, 0xca, 0xf0));
}

#[test]
fn inactive_title_is_gray() {
    assert_eq!(palette::INACTIVE_TITLE, (0x80, 0x80, 0x80));
}

#[test]
fn selection_highlight_is_navy_on_white() {
    assert_eq!(palette::HIGHLIGHT, (0x0a, 0x24, 0x6a));
    // Sentinel white (0xff,0xff,0xfe) — renders pure white in Win2000; the 1-LSB
    // marker lets the Carbon dark remap keep selection text light. See palette.rs.
    assert_eq!(palette::HIGHLIGHT_TEXT, (0xff, 0xff, 0xfe));
}

#[test]
fn window_and_frame_silver() {
    assert_eq!(palette::WINDOW, (0xff, 0xff, 0xff));
    assert_eq!(palette::MENU, (0xd4, 0xd0, 0xc8));
    assert_eq!(palette::BUTTON_FACE, (0xd4, 0xd0, 0xc8));
}

/// The Carbon sentinels are load-bearing (§2.2): a "fix" to pure white/black would
/// break the dark-mode text/surface split. Pin both by name so they can't drift,
/// plus the SHELL_HEADER role's silver identity (distinct key from BUTTON_FACE).
#[test]
fn carbon_sentinels_and_header_are_pinned() {
    assert_eq!(palette::TITLE_TEXT, (0xff, 0xff, 0xfe)); // white text, distinct from WINDOW surface
    assert_eq!(palette::HIGHLIGHT_TEXT, (0xff, 0xff, 0xfe));
    assert_eq!(palette::WINDOW_FRAME, (0x00, 0x00, 0x01)); // frame, distinct from black TEXT
    assert_eq!(palette::SHELL_HEADER, (0xd4, 0xd0, 0xc7)); // ≠ BUTTON_FACE (…c8)
}

/// The 3D ramp must run strictly light -> dark so bevels read correctly.
#[test]
fn bevel_ramp_is_monotonically_darker() {
    let lum = |c: Rgb| c.0 as u32 + c.1 as u32 + c.2 as u32;
    assert!(lum(palette::BUTTON_HILIGHT) > lum(palette::BUTTON_LIGHT));
    assert!(lum(palette::BUTTON_LIGHT) > lum(palette::BUTTON_FACE));
    assert!(lum(palette::BUTTON_FACE) > lum(palette::BUTTON_SHADOW));
    assert!(lum(palette::BUTTON_SHADOW) > lum(palette::BUTTON_DK_SHADOW));
}

#[test]
fn bevel_endpoints_match_checklist() {
    // raised = white/#dfdfdf (TL) over #808080/#404040 (BR)
    assert_eq!(palette::BUTTON_HILIGHT, (0xff, 0xff, 0xff));
    assert_eq!(palette::BUTTON_LIGHT, (0xdf, 0xdf, 0xdf));
    assert_eq!(palette::BUTTON_SHADOW, (0x80, 0x80, 0x80));
    assert_eq!(palette::BUTTON_DK_SHADOW, (0x40, 0x40, 0x40));
}

/// `color()` must round-trip an 8-bit channel exactly (no gamma surprises).
#[test]
fn color_conversion_is_exact_8bit() {
    let c = palette::color(palette::BACKGROUND);
    assert_eq!((c.r * 255.0).round() as u8, 0x3a);
    assert_eq!((c.g * 255.0).round() as u8, 0x6e);
    assert_eq!((c.b * 255.0).round() as u8, 0xa5);
}

/// App-chrome colors live in the palette too, so nothing outside it names a
/// raw hex; pin them so a future hand-tuned literal fails here instead.
#[test]
fn app_chrome_colors_are_pinned() {
    assert_eq!(palette::INFO_BAND, (0x1d, 0x5c, 0xa8));
    assert_eq!(palette::SETUP_GRADIENT_TOP, (0x1c, 0x4a, 0x8f));
    assert_eq!(palette::SETUP_GRADIENT_BOTTOM, (0x08, 0x16, 0x40));
    assert_eq!(palette::SETUP_PROGRESS, (0x16, 0x3a, 0xa8));
}

// --- Metrics ---------------------------------------------------------------

#[test]
fn title_bar_is_18px() {
    assert_eq!(metrics::TITLE_BAR_HEIGHT, 18);
}

#[test]
fn frames_match_win2000() {
    assert_eq!(metrics::SIZE_FRAME, 3);
    assert_eq!(metrics::FIXED_FRAME, 1);
    assert_eq!(metrics::BEVEL_LINE, 1);
}

#[test]
fn taskbar_is_28px() {
    assert_eq!(metrics::TASKBAR_HEIGHT, 28);
}

#[test]
fn scrollbar_and_menu_rows() {
    assert_eq!(metrics::SCROLLBAR, 16);
    assert_eq!(metrics::MENU_HEIGHT, 18);
}

/// Pin what the renderer ACTUALLY ships, not the unattainable target. Win2000's
/// Tahoma isn't freely distributable, so the shell renders Droid Sans; a green
/// "accuracy" test must never launder that approximation by asserting "Tahoma".
/// The target is recorded separately so the gap stays named.
#[test]
fn ui_font_is_the_shipped_substitute() {
    assert_eq!(mde_ui::font::FAMILY, "Droid Sans"); // the family every renderer loads
    assert_eq!(metrics::UI_FONT_TARGET, "Tahoma"); // the documented ground truth
    assert!(metrics::TITLE_FONT_BOLD);
}

/// 8pt at 96 DPI is 10.67px → 11; UI_PX is the single size the renderer uses,
/// so "8pt everywhere" is one derived constant rather than 38 magic literals.
#[test]
fn ui_size_is_one_source_of_truth() {
    assert_eq!(metrics::UI_FONT_PT, 8.0);
    assert_eq!(metrics::UI_PX, (metrics::UI_FONT_PT * 96.0 / 72.0).round());
    // INFO_TITLE_PX is §2.3's one larger UI size (info-band/about/control-panel
    // headings); pin it so a silent drift fails CI like UI_PX does.
    assert_eq!(metrics::INFO_TITLE_PX, 16.0);
}
