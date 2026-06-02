//! Shared building blocks for the two Start surfaces — the Carbon switcher
//! (`menu.rs`) and the Windows 10 tiled Start (`start_win10.rs`). Extracted so
//! the flat tile widget, the launch helpers, and the single-instance guard each
//! have ONE implementation instead of being copied per surface.

use std::process::Command;

use iced::alignment::Horizontal;
use iced::widget::{button, container, mouse_area, text, Column};
use iced::{Background, Border, Color, Element, Length, Padding, Shadow};

use mde_ui::{metrics, palette};

/// Launch a shell command, optionally inside a `foot` terminal (for CLI tools).
pub fn launch_cmd(cmd: &str, terminal: bool) {
    if terminal {
        let _ = Command::new("foot")
            .arg("-o")
            .arg(format!(
                "font=monospace:size={}",
                crate::fedora::CLI_FONT_SIZE
            ))
            .arg("sh")
            .arg("-c")
            .arg(cmd)
            .spawn();
    } else {
        let _ = Command::new("sh").arg("-c").arg(cmd).spawn();
    }
}

/// Re-exec this binary with a subcommand (e.g. `mde shutdown`).
pub fn mde_self(sub: &str) {
    if let Ok(exe) = std::env::current_exe() {
        let _ = Command::new(exe).arg(sub).spawn();
    }
}

/// The Start subcommand for the active era: the Win10 tiled Start under the
/// Windows 10 theme, else the Carbon/Win2000 cascade menu. The ONE place the
/// per-era choice lives, so the panel Start button and the `mde start` keybind
/// dispatcher always agree (D1: Carbon/Win2000 keep `mde menu`).
pub fn active_start_cmd() -> &'static str {
    start_cmd_for(palette::is_windows10())
}

fn start_cmd_for(is_windows10: bool) -> &'static str {
    if is_windows10 {
        "start-win10"
    } else {
        "menu"
    }
}

/// Single-instance guard via a pid file `<XDG_RUNTIME_DIR>/<basename>.pid`: if it
/// names a still-live process (`/proc/<pid>`), the slot is taken. A stale file
/// (the previous surface exited via `exit(0)`, which skips cleanup) is harmless —
/// its pid is gone, so we reclaim it. The `basename` lets each Start surface
/// guard its own slot (the Carbon menu on `mde-menu`, the Win10 Start on
/// `mde-start-win10`). Linux-only liveness check, no extra dependency.
pub fn acquire_singleton(basename: &str) -> bool {
    let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    let path = format!("{dir}/{basename}.pid");
    if let Ok(s) = std::fs::read_to_string(&path) {
        if let Ok(pid) = s.trim().parse::<u32>() {
            if std::path::Path::new(&format!("/proc/{pid}")).exists() {
                return false;
            }
        }
    }
    std::fs::write(&path, std::process::id().to_string()).is_ok()
}

/// One flat Start tile: a vertical button (icon over a centered, wrapped label)
/// with an accent-tinted hover. `right`, when set, wires a right-click message.
/// Generic over the surface's `Message` so both Start surfaces share it; the
/// Carbon switcher passes its fixed 104×88, the Win10 Start its per-size spans.
pub fn tile<'a, M: Clone + 'a>(
    icon: Element<'a, M>,
    label: &'a str,
    press: M,
    right: Option<M>,
    width: f32,
    height: f32,
) -> Element<'a, M> {
    let content = Column::new()
        .spacing(4.0)
        .align_x(Horizontal::Center)
        .width(Length::Fill)
        .push(container(icon).center_x(Length::Fill))
        .push(
            text(label)
                .size(metrics::UI_PX)
                .align_x(Horizontal::Center)
                .width(Length::Fill),
        );
    let btn = button(content)
        .on_press(press)
        .width(Length::Fixed(width))
        .height(Length::Fixed(height))
        .padding(Padding {
            top: 8.0,
            right: 4.0,
            bottom: 6.0,
            left: 4.0,
        })
        .style(tile_style());
    match right {
        Some(r) => mouse_area(btn).on_right_press(r).into(),
        None => btn.into(),
    }
}

/// Flat tile style: transparent at rest, accent-tinted on hover/press, 2px radius,
/// label in menu text. Shared by both Start surfaces (flat under Carbon/Win10).
pub fn tile_style() -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    |_theme, status| {
        let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let mut a = palette::accent();
        a.a = 0.18;
        button::Style {
            background: hot.then_some(Background::Color(a)),
            text_color: palette::color(palette::MENU_TEXT),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 2.0.into(),
            },
            shadow: Shadow::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_cmd_routes_per_era() {
        // E1.11: Windows 10 era → the tiled Start; everything else → the cascade menu.
        assert_eq!(start_cmd_for(true), "start-win10");
        assert_eq!(start_cmd_for(false), "menu");
    }
}
