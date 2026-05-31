//! The four-pane Windows flag for the Start button.
//!
//! Windows 2000's Start button carries the "flying windows" logo: a 2×2 grid of
//! red / green / blue / yellow panes split by a thin white cross. We draw it as
//! four colored quads (the same `fill` primitive the bevels use) rather than a
//! font glyph or bitmap — the bundled UI font (Droid Sans) has no flag/dingbat
//! glyph, so a text mark renders as tofu; quads render identically everywhere
//! and stay palette-disciplined. Childless and draw-only, like [`BevelFrame`].

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{Tree, Widget};
use iced::mouse;
use iced::{Element, Length, Rectangle, Size};

use crate::palette;
use crate::widget::fill;

/// The Start-button flag. Fixed-size (defaults to the 16×13 the 18px button
/// wants); set [`Flag::size`] to scale it.
pub struct Flag {
    width: f32,
    height: f32,
}

/// Construct the flag at its default size.
pub fn flag() -> Flag {
    Flag { width: 16.0, height: 13.0 }
}

impl Flag {
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer> for Flag
where
    Renderer: renderer::Renderer,
{
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(self.width), Length::Fixed(self.height))
    }

    fn layout(&self, _tree: &mut Tree, _renderer: &Renderer, _limits: &layout::Limits) -> layout::Node {
        layout::Node::new(Size::new(self.width, self.height))
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let b = layout.bounds();
        let gap = 1.0_f32; // the white cross between the four panes
        let pw = ((b.width - gap) / 2.0).max(1.0);
        let ph = ((b.height - gap) / 2.0).max(1.0);
        let rx = b.x + pw + gap; // right column x
        let by = b.y + ph + gap; // bottom row y
        // White cross underneath (shows through the 1px gaps between panes).
        fill(renderer, b.x, b.y, b.width, b.height, palette::color(palette::WINDOW));
        // Four panes: red TL, green TR, blue BL, yellow BR — the flag's layout.
        fill(renderer, b.x, b.y, pw, ph, palette::color(palette::LOGO_RED));
        fill(renderer, rx, b.y, pw, ph, palette::color(palette::LOGO_GREEN));
        fill(renderer, b.x, by, pw, ph, palette::color(palette::LOGO_BLUE));
        fill(renderer, rx, by, pw, ph, palette::color(palette::LOGO_YELLOW));
    }
}

impl<'a, Message, Theme, Renderer> From<Flag> for Element<'a, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer + 'a,
    Message: 'a,
    Theme: 'a,
{
    fn from(f: Flag) -> Self {
        Self::new(f)
    }
}
