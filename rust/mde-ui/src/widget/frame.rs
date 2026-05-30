//! A childless 3D bevel frame widget for iced.
//!
//! Fills its bounds with the Win2000 face color and draws the two-line
//! raised/sunken/pressed bevel from [`crate::widget::bevel`]. Being childless
//! it is trivially correct and composes as a background in a `stack!` or as a
//! separator / group-box / clock-well frame.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{Tree, Widget};
use iced::mouse;
use iced::{Border, Color, Element, Length, Rectangle, Shadow, Size};

use crate::palette;
use crate::widget::bevel::Bevel;

/// A 3D bevel frame. See [`raised`], [`sunken`], [`pressed`].
pub struct BevelFrame {
    bevel: Bevel,
    face: Option<Color>,
    width: Length,
    height: Length,
    /// 2 = full two-line edge (default), 1 = a single thin line.
    thickness: u16,
}

/// A raised frame: panels, the taskbar, buttons at rest.
pub fn raised() -> BevelFrame {
    BevelFrame::new(Bevel::raised())
}
/// A sunken frame: text fields, list/tree views, the clock well.
pub fn sunken() -> BevelFrame {
    BevelFrame::new(Bevel::sunken())
}
/// A pressed frame: a depressed button.
pub fn pressed() -> BevelFrame {
    BevelFrame::new(Bevel::pressed())
}

impl BevelFrame {
    fn new(bevel: Bevel) -> Self {
        Self {
            bevel,
            face: Some(palette::color(palette::BUTTON_FACE)),
            width: Length::Fill,
            height: Length::Fill,
            thickness: 2,
        }
    }

    pub fn face(mut self, color: Color) -> Self {
        self.face = Some(color);
        self
    }
    pub fn no_face(mut self) -> Self {
        self.face = None;
        self
    }
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }
    pub fn thickness(mut self, thickness: u16) -> Self {
        self.thickness = thickness;
        self
    }
}

fn fill<Renderer: renderer::Renderer>(
    renderer: &mut Renderer,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: Color,
) {
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    renderer.fill_quad(
        renderer::Quad {
            bounds: Rectangle {
                x,
                y,
                width: w,
                height: h,
            },
            border: Border::default(),
            shadow: Shadow::default(),
        },
        color,
    );
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer> for BevelFrame
where
    Renderer: renderer::Renderer,
{
    fn size(&self) -> Size<Length> {
        Size::new(self.width, self.height)
    }

    fn layout(
        &self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let size = limits.resolve(self.width, self.height, Size::ZERO);
        layout::Node::new(size)
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
        let (x, y, w, h) = (b.x, b.y, b.width, b.height);

        if let Some(face) = self.face {
            fill(renderer, x, y, w, h, face);
        }
        // Outer edge (top/left vs bottom/right).
        fill(renderer, x, y, w, 1.0, palette::color(self.bevel.outer_tl));
        fill(renderer, x, y, 1.0, h, palette::color(self.bevel.outer_tl));
        fill(renderer, x, y + h - 1.0, w, 1.0, palette::color(self.bevel.outer_br));
        fill(renderer, x + w - 1.0, y, 1.0, h, palette::color(self.bevel.outer_br));

        if self.thickness >= 2 {
            // Inner edge.
            fill(renderer, x + 1.0, y + 1.0, w - 2.0, 1.0, palette::color(self.bevel.inner_tl));
            fill(renderer, x + 1.0, y + 1.0, 1.0, h - 2.0, palette::color(self.bevel.inner_tl));
            fill(renderer, x + 1.0, y + h - 2.0, w - 2.0, 1.0, palette::color(self.bevel.inner_br));
            fill(renderer, x + w - 2.0, y + 1.0, 1.0, h - 2.0, palette::color(self.bevel.inner_br));
        }
    }
}

impl<'a, Message, Theme, Renderer> From<BevelFrame> for Element<'a, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer + 'a,
    Message: 'a,
    Theme: 'a,
{
    fn from(frame: BevelFrame) -> Self {
        Self::new(frame)
    }
}
