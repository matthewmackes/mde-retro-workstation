//! A Windows 2000 push button for iced.
//!
//! Raised 3D bevel at rest; when pressed it flips to a sunken bevel and the
//! label nudges 1px down-right — exactly like a classic Win2000 button. Used
//! for the Start button, taskbar window buttons, and dialog buttons.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{tree, Tree, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::{event, mouse};
use iced::{Border, Color, Element, Event, Length, Padding, Rectangle, Shadow, Size, Vector};

use crate::palette;
use crate::widget::bevel::Bevel;

#[derive(Default)]
struct State {
    is_pressed: bool,
}

/// A classic 3D push button wrapping arbitrary content (usually text).
pub struct Button<'a, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer,
{
    content: Element<'a, Message, Theme, Renderer>,
    on_press: Option<Message>,
    width: Length,
    height: Length,
    padding: Padding,
    face: Color,
    /// When true the button renders sunken even when not pressed (toggled on,
    /// e.g. the focused window's taskbar button).
    active: bool,
}

/// Construct a button around `content`.
pub fn button<'a, Message, Theme, Renderer>(
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
) -> Button<'a, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer,
{
    Button {
        content: content.into(),
        on_press: None,
        width: Length::Shrink,
        height: Length::Shrink,
        padding: Padding::from([2, 8]),
        face: palette::color(palette::BUTTON_FACE),
        active: false,
    }
}

impl<'a, Message, Theme, Renderer> Button<'a, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer,
{
    pub fn on_press(mut self, message: Message) -> Self {
        self.on_press = Some(message);
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
    pub fn padding(mut self, padding: impl Into<Padding>) -> Self {
        self.padding = padding.into();
        self
    }
    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }
}

fn fill<Renderer: renderer::Renderer>(r: &mut Renderer, x: f32, y: f32, w: f32, h: f32, c: Color) {
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

fn draw_bevel<Renderer: renderer::Renderer>(r: &mut Renderer, b: Rectangle, bevel: Bevel, face: Color) {
    let (x, y, w, h) = (b.x, b.y, b.width, b.height);
    fill(r, x, y, w, h, face);
    fill(r, x, y, w, 1.0, palette::color(bevel.outer_tl));
    fill(r, x, y, 1.0, h, palette::color(bevel.outer_tl));
    fill(r, x, y + h - 1.0, w, 1.0, palette::color(bevel.outer_br));
    fill(r, x + w - 1.0, y, 1.0, h, palette::color(bevel.outer_br));
    fill(r, x + 1.0, y + 1.0, w - 2.0, 1.0, palette::color(bevel.inner_tl));
    fill(r, x + 1.0, y + 1.0, 1.0, h - 2.0, palette::color(bevel.inner_tl));
    fill(r, x + 1.0, y + h - 2.0, w - 2.0, 1.0, palette::color(bevel.inner_br));
    fill(r, x + w - 2.0, y + 1.0, 1.0, h - 2.0, palette::color(bevel.inner_br));
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for Button<'a, Message, Theme, Renderer>
where
    Message: Clone,
    Renderer: renderer::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        Size::new(self.width, self.height)
    }

    fn layout(&self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
        layout::padded(limits, self.width, self.height, self.padding, |limits| {
            self.content
                .as_widget()
                .layout(&mut tree.children[0], renderer, limits)
        })
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let down = state.is_pressed || self.active;
        let bevel = if down { Bevel::sunken() } else { Bevel::raised() };
        draw_bevel(renderer, layout.bounds(), bevel, self.face);

        let content_layout = layout.children().next().expect("button has one child");
        let content_style = renderer::Style {
            text_color: palette::color(palette::BUTTON_TEXT),
        };
        let offset = if down { Vector::new(1.0, 1.0) } else { Vector::ZERO };
        renderer.with_translation(offset, |renderer| {
            self.content.as_widget().draw(
                &tree.children[0],
                renderer,
                theme,
                &content_style,
                content_layout,
                cursor,
                viewport,
            );
        });
        let _ = style;
    }

    fn on_event(
        &mut self,
        tree: &mut Tree,
        event: Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) -> event::Status {
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if cursor.is_over(layout.bounds()) {
                    state.is_pressed = true;
                    return event::Status::Captured;
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.is_pressed {
                    state.is_pressed = false;
                    if cursor.is_over(layout.bounds()) {
                        if let Some(message) = self.on_press.clone() {
                            shell.publish(message);
                        }
                        return event::Status::Captured;
                    }
                }
            }
            _ => {}
        }
        event::Status::Ignored
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
}

impl<'a, Message, Theme, Renderer> From<Button<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Theme: 'a,
    Renderer: renderer::Renderer + 'a,
{
    fn from(button: Button<'a, Message, Theme, Renderer>) -> Self {
        Self::new(button)
    }
}
