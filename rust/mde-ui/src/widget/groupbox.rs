//! The Windows 2000 group box: an etched rectangle framing related controls,
//! with a caption that sits on the top border line.
//!
//! iced has no group-box widget, so this is a small builder: a 1px-bordered
//! container (the engraved frame) under a caption chip painted in the dialog
//! face color, which masks the border behind it so the line appears to break
//! for the title — exactly the Win2000 look. Composes any content element.

use iced::widget::{container, text, Column, Row, Space, Stack};
use iced::{Background, Border, Element, Length, Padding};

use crate::palette;

/// Frame `content` in a captioned group box. `face` is the surrounding dialog
/// color the caption chip is painted in (usually `palette::MENU` silver).
pub fn group_box<'a, Message>(
    title: impl text::IntoFragment<'a>,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message>
where
    Message: 'a,
{
    // The engraved frame: a 1px shadow border, content inset (top inset leaves
    // room for the caption to overlap the line).
    let framed = container(content)
        .padding(Padding { top: 12.0, right: 10.0, bottom: 10.0, left: 10.0 })
        .width(Length::Fill)
        .style(|_: &iced::Theme| container::Style {
            background: None,
            border: Border {
                color: palette::color(palette::BUTTON_SHADOW),
                width: 1.0,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        });

    // The caption chip, painted in the dialog face so it masks the border line.
    let caption = container(
        text(title)
            .size(crate::metrics::UI_PX)
            .color(palette::color(palette::WINDOW_TEXT)),
    )
    .padding(Padding { top: 0.0, right: 4.0, bottom: 0.0, left: 4.0 })
    .style(|_: &iced::Theme| container::Style {
        background: Some(Background::Color(palette::color(palette::MENU))),
        ..container::Style::default()
    });

    // Float the caption at the top-left, slightly indented, over the frame.
    let overlay = Column::new()
        .push(Row::new().push(Space::with_width(Length::Fixed(8.0))).push(caption))
        .push(Space::with_height(Length::Fill));

    Stack::new().push(framed).push(overlay).into()
}
