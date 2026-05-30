//! Win2000 Classic widgets for iced.
//!
//! The bevel model ([`bevel`]) is implemented and unit-tested. The iced
//! `Widget`/style wiring (3D button, sunken field, title bar, menubar, tree,
//! column list) lands as the components are built — see tasks for mde-ui.

pub mod bevel;
pub mod button;
pub mod frame;

pub use bevel::Bevel;
pub use button::{button, Button};
pub use frame::BevelFrame;

use iced::advanced::renderer;
use iced::widget::text_input;
use iced::{Background, Border, Color, Rectangle, Shadow};

use crate::palette;

/// The Win2000 sunken-white text field: `COLOR_WINDOW` fill with a recessed 1px
/// edge. Pass to `text_input(...).style(mde_ui::sunken_field)` so form fields
/// obey the rule for their kind instead of shipping the iced default.
pub fn sunken_field(_theme: &iced::Theme, _status: text_input::Status) -> text_input::Style {
    text_input::Style {
        background: Background::Color(palette::color(palette::WINDOW)),
        border: Border {
            color: palette::color(palette::BUTTON_SHADOW),
            width: 1.0,
            radius: 0.0.into(),
        },
        icon: palette::color(palette::WINDOW_TEXT),
        placeholder: palette::color(palette::GRAY_TEXT),
        value: palette::color(palette::WINDOW_TEXT),
        selection: palette::color(palette::HIGHLIGHT),
    }
}

/// Fill an axis-aligned rectangle with a solid color (skips degenerate rects).
/// The one quad primitive every Win2000 edge is built from.
pub(crate) fn fill<R: renderer::Renderer>(r: &mut R, x: f32, y: f32, w: f32, h: f32, c: Color) {
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    r.fill_quad(
        renderer::Quad {
            bounds: Rectangle { x, y, width: w, height: h },
            border: Border::default(),
            shadow: Shadow::default(),
        },
        c,
    );
}

/// The Win2000 `DrawEdge`: optionally fill `face`, then lay the bevel's outer
/// (and, when `thickness >= 2`, inner) 1px lines around `rect`. This is the
/// single place a 1px edge can be wrong — [`Button`] and [`BevelFrame`] both
/// call it, so the raised/sunken/pressed look is identical everywhere.
pub(crate) fn draw_edge<R: renderer::Renderer>(
    r: &mut R,
    rect: Rectangle,
    bevel: Bevel,
    thickness: u16,
    face: Option<Color>,
) {
    let (x, y, w, h) = (rect.x, rect.y, rect.width, rect.height);
    if let Some(face) = face {
        fill(r, x, y, w, h, face);
    }
    // Outer edge: top + left vs bottom + right.
    fill(r, x, y, w, 1.0, palette::color(bevel.outer_tl));
    fill(r, x, y, 1.0, h, palette::color(bevel.outer_tl));
    fill(r, x, y + h - 1.0, w, 1.0, palette::color(bevel.outer_br));
    fill(r, x + w - 1.0, y, 1.0, h, palette::color(bevel.outer_br));
    if thickness >= 2 {
        // Inner edge.
        fill(r, x + 1.0, y + 1.0, w - 2.0, 1.0, palette::color(bevel.inner_tl));
        fill(r, x + 1.0, y + 1.0, 1.0, h - 2.0, palette::color(bevel.inner_tl));
        fill(r, x + 1.0, y + h - 2.0, w - 2.0, 1.0, palette::color(bevel.inner_br));
        fill(r, x + w - 2.0, y + 1.0, 1.0, h - 2.0, palette::color(bevel.inner_br));
    }
}
