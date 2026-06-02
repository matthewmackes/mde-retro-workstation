//! Shared wallpaper helpers — the picture scan, the `swaybg` fit modes, the
//! Browse-via-filedialog hop, and the monitor-preview render.
//!
//! Extracted from `display.rs` (the Win2000 Display Properties ▸ Background tab)
//! so the Windows 10 Settings ▸ Personalization ▸ Background page (E7.4) can
//! reuse the same logic instead of re-deriving it. Pure of any one surface's
//! state: callers pass the selected path in and drive `swaybg` through
//! `outputs::persist` themselves.

use iced::widget::{container, image, Space};
use iced::{Background, Element, Length};

use mde_ui::palette;

/// How a wallpaper fills the screen — the five `swaybg` modes (no invented
/// "Span"). The `Display` impl is the picker label; [`swaybg`] is the CLI token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BgMode {
    Center,
    Tile,
    Stretch,
    Fit,
    Fill,
}

impl BgMode {
    pub const ALL: [BgMode; 5] = [
        BgMode::Center,
        BgMode::Tile,
        BgMode::Stretch,
        BgMode::Fit,
        BgMode::Fill,
    ];
    /// The `swaybg --mode` token.
    pub fn swaybg(self) -> &'static str {
        match self {
            BgMode::Center => "center",
            BgMode::Tile => "tile",
            BgMode::Stretch => "stretch",
            BgMode::Fit => "fit",
            BgMode::Fill => "fill",
        }
    }
}

impl std::fmt::Display for BgMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            BgMode::Center => "Center",
            BgMode::Tile => "Tile",
            BgMode::Stretch => "Stretch",
            BgMode::Fit => "Fit",
            BgMode::Fill => "Fill",
        })
    }
}

/// Scan the standard picture directories for usable wallpaper images, sorted and
/// de-duplicated. Looks in `~/Pictures`, `~/.local/share/backgrounds`, the
/// bundled `~/.local/share/mde/wallpapers`, and `/usr/share/backgrounds`.
pub fn scan() -> Vec<String> {
    let mut out = Vec::new();
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let mut dirs: Vec<std::path::PathBuf> = Vec::new();
    if let Some(h) = &home {
        dirs.push(h.join("Pictures"));
        dirs.push(h.join(".local/share/backgrounds"));
        dirs.push(h.join(".local/share/mde/wallpapers")); // bundled look-alike set
    }
    dirs.push("/usr/share/backgrounds".into());
    for dir in dirs {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for e in entries.flatten() {
                let p = e.path();
                let ext = p
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "bmp" | "webp") {
                    if let Some(s) = p.to_str() {
                        out.push(s.to_string());
                    }
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Open the shell's own Common File Dialog (`mde filedialog`) filtered to images
/// and return the chosen path (or `None` if cancelled).
pub fn browse() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let o = std::process::Command::new(exe)
        .args([
            "filedialog",
            "--title",
            "Browse",
            "--filter",
            "Images:png,jpg,jpeg,bmp,webp;All Files:*",
        ])
        .output()
        .ok()?;
    if o.status.success() {
        let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
        if !s.is_empty() {
            return Some(s);
        }
    }
    None
}

/// The screen-fill content for a monitor preview: the selected image
/// cover-fitted, or the themed desktop background when nothing is picked (via
/// the palette edge, never a raw literal — §2.1).
pub fn preview<M: 'static>(selected: Option<&str>) -> Element<'static, M> {
    if let Some(path) = selected {
        return image(image::Handle::from_path(path))
            .width(Length::Fill)
            .height(Length::Fill)
            .content_fit(iced::ContentFit::Cover)
            .into();
    }
    container(Space::new(Length::Fill, Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(palette::color(palette::BACKGROUND))),
            ..container::Style::default()
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bgmode_swaybg_tokens() {
        assert_eq!(BgMode::Fill.swaybg(), "fill");
        assert_eq!(BgMode::Center.swaybg(), "center");
        assert_eq!(BgMode::ALL.len(), 5);
    }
}
