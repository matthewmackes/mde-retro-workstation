//! `mde-ui` — the Windows 2000 Classic look for the MDE-Retro shell.
//!
//! Three layers:
//! - [`palette`] — the system color table (ground truth, toolkit-agnostic).
//! - [`metrics`] — default UI metrics at 96 DPI (the accuracy targets).
//! - [`widget`] — the 3D bevel model and (incrementally) the iced widgets.
//!
//! Accuracy is job 1: `palette` and `metrics` are transcribed from
//! `assets/reference/win2000-classic-colors.ini` and the classic `SM_*`
//! metrics, and the screenshot-diff harness checks the rendered result
//! against them.

pub mod font;
pub mod metrics;
pub mod palette;
pub mod widget;

pub use palette::{color, Rgb};
pub use widget::{
    button, flag, frame, infoband, scrollbar, sunken_field, sunken_picklist, Bevel, BevelFrame,
    Button,
};
