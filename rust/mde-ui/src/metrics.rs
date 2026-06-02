//! Windows 2000 default UI metrics at 96 DPI.
//!
//! These are the target numbers the accuracy harness checks against (see
//! `rust/ACCURACY.md`). Values are the classic `SM_*` system metrics; adjust
//! only with a reference screenshot to back the change.

/// Title-bar height (SM_CYCAPTION), excluding the 3D frame.
pub const TITLE_BAR_HEIGHT: u16 = 18;
/// Sizing-frame thickness around a resizable window (SM_CXSIZEFRAME).
/// Sway-owned today: sway draws the window frame, so this is transcribed for
/// completeness, not applied by mde (see ACCURACY.md §0).
pub const SIZE_FRAME: u16 = 3;
/// Thin 3D frame thickness around a fixed/dialog window. Sway-owned (as above).
pub const FIXED_FRAME: u16 = 1;
/// Each bevel is two 1px lines.
pub const BEVEL_LINE: u16 = 1;
/// Scrollbar thickness (SM_CXVSCROLL / SM_CYHSCROLL).
pub const SCROLLBAR: u16 = 16;
/// Menu-bar item height (SM_CYMENU).
pub const MENU_HEIGHT: u16 = 18;
/// The taskbar height (one row of 28px-ish buttons + bevel).
pub const TASKBAR_HEIGHT: u16 = 28;
/// Default min width of a taskbar window button before it elides.
pub const TASKBAR_BUTTON_MIN: u16 = 160;

/// The Win2000 UI font — the ground-truth TARGET. Tahoma is not freely
/// distributable, so the shell renders a substitute (`mde_ui::font::FAMILY`);
/// this records the original so the gap is named, not hidden. The renderer
/// never requests this string — see `font.rs` for what actually ships.
pub const UI_FONT_TARGET: &str = "Tahoma";
/// The Windows 10 era's UI font TARGET — Segoe UI, likewise not redistributable.
/// Per §2.4 the gap is named, not laundered: the Win10 era ships the already-
/// bundled IBM Plex Sans (`font::PLEX_FAMILY`) as its humanist-sans substitute,
/// so no new TTF/licence is added. The renderer never requests this string.
pub const UI_FONT_TARGET_WIN10: &str = "Segoe UI";
/// UI font size in points (Tahoma 8pt) — the transcribed system value.
pub const UI_FONT_PT: f32 = 8.0;
/// `UI_FONT_PT` in device pixels at 96 DPI (8pt → 10.67px, rounded to 11): the
/// ONE size every UI `.size(...)` call must use, so the "8pt everywhere" rule
/// has a single source of truth instead of scattered literals.
pub const UI_PX: f32 = 11.0;
/// The web-view info-band folder title — the one larger display size in the
/// shell (Win2000 drew this caption well above body text). Single source so the
/// band title isn't a scattered literal either.
pub const INFO_TITLE_PX: f32 = 16.0;
/// Setup-wizard heading size (the "Choose Components" step title). Named so the
/// installer doesn't carry a scattered literal (§2.3).
pub const WIZARD_HEADING_PX: f32 = 15.0;
/// Setup-wizard status-bar caption size (smaller than UI text).
pub const WIZARD_STATUS_PX: f32 = 10.0;
/// The big monitor-number overlay drawn by Display ▸ Identify.
pub const IDENTIFY_PX: f32 = 48.0;
/// Title-bar font is the UI font, bold, at the same size.
pub const TITLE_FONT_BOLD: bool = true;
/// The Windows 10 Task View window-tile size (px) — a square-ish window card.
pub const TASKVIEW_TILE: f32 = 200.0;
