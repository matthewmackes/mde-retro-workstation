//! The Windows 2000 "web view" info band — the description panel that fills the
//! left of an Explorer (or Control Panel) folder before the file list: a white
//! field carrying a colored folder title, a fading divider rule, body text, a
//! yellow description tip, and "See also" hyperlinks.
//!
//! This module owns the *look* (toolkit-agnostic styles + the band accent
//! color); the apps supply the per-folder content. Keeping it here means the
//! Explorer band and any future Control-Panel band obey one rule, and no app
//! names a raw color — every value still routes through [`palette`].

use iced::widget::container;
use iced::{Background, Border, Color, Gradient, Shadow};

use crate::palette;

/// The band background: a plain white (`COLOR_WINDOW`) field. The web view has
/// no 3D edge against the icon list — it is a flat extension of the window.
pub fn band(_theme: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(palette::color(palette::WINDOW))),
        text_color: Some(palette::color(palette::WINDOW_TEXT)),
        ..container::Style::default()
    }
}

/// The yellow description tip box (`COLOR_INFOBK`), edged 1px in the band blue —
/// the little callout that explains the selected item.
pub fn tip(_theme: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(palette::color(palette::INFO_WINDOW))),
        text_color: Some(palette::color(palette::INFO_TEXT)),
        border: Border {
            color: palette::color(palette::INFO_BAND),
            width: 1.0,
            radius: 0.0.into(),
        },
        shadow: Shadow::default(),
    }
}

/// The divider rule under the folder title: the band blue fading left→right to
/// white, exactly the gradient hairline Win2000's web view drew. Give the
/// container a small fixed height (≈2px) and `Length::Fill` width.
pub fn rule(_theme: &iced::Theme) -> container::Style {
    let gradient = Gradient::Linear(
        iced::gradient::Linear::new(iced::Radians(std::f32::consts::FRAC_PI_2))
            .add_stop(0.0, palette::color(palette::INFO_BAND))
            .add_stop(1.0, palette::color(palette::WINDOW)),
    );
    container::Style {
        background: Some(Background::Gradient(gradient)),
        ..container::Style::default()
    }
}

/// The band's title / hyperlink color (the Win2000 web-view blue, `INFO_BAND`).
/// The folder title and the "See also" links are drawn in this.
pub fn accent() -> Color {
    palette::color(palette::INFO_BAND)
}
