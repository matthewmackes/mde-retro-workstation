//! MDE-Retro installer (`mde setup`) — styled after the Windows 2000 GUI Setup
//! screen: deep-blue gradient background, a "Choose Components" picker, and a
//! bottom status strip, all in Tahoma/white. The GUI *collects* the component
//! selection; the real install runs through the hardened TUI engine (it relaunches
//! `pkexec mde setup --tui --packages=…`), so there is exactly one install path.

use std::process::{exit, ExitCode};

use iced::widget::{container, mouse_area, scrollable, text, Column, Row, Space};
use iced::{
    gradient::Linear, Background, Color, Element, Gradient, Length, Padding, Radians, Task,
};

use mde_ui::{font, metrics, palette};

/// Setup chrome text colors, sourced from the palette (white on the blue, and
/// the dimmed light-blue for locked/unavailable/subtitle text).
fn white() -> Color {
    palette::color(palette::WINDOW)
}
fn dim() -> Color {
    palette::color(palette::SETUP_SUBTITLE)
}

struct Setup {
    cat: Vec<crate::catalogue::Component>,
    checked: Vec<bool>,
    /// mandatory || already-installed — shown checked and not toggleable.
    locked: Vec<bool>,
    /// offered by an enabled repo (else dimmed, can't be selected).
    avail: Vec<bool>,
}

#[derive(Debug, Clone)]
enum Msg {
    Toggle(usize),
    /// 0 = Minimal, 1 = Standard, 2 = Everything.
    Preset(u8),
    Install,
    Cancel,
}

/// Route `mde setup`:
///   --gui            the themed iced component picker (collects, then hands the
///                    selection to the TUI engine to install)
///   --tui / headless the real text-mode installer (verified engine)
///   in-session       launch that same real TUI installer, privileged, in a
///                    Win2000-styled terminal.
pub fn dispatch(args: &[String]) -> ExitCode {
    let tui = args.iter().any(|a| a == "--tui");
    let gui = args.iter().any(|a| a == "--gui");
    let dry = args.iter().any(|a| a == "--dry-run");
    // `--packages=pkg1,pkg2` — an explicit set (e.g. handed over from the GUI
    // component picker). When absent, the TUI shows its own Choose-Components
    // screen and falls back to the curated catalogue default.
    let packages = args.iter().find_map(|a| a.strip_prefix("--packages=")).map(|s| {
        s.split(',').filter(|p| !p.is_empty()).map(str::to_string).collect::<Vec<String>>()
    });
    let headless = std::env::var_os("WAYLAND_DISPLAY").is_none();
    if gui {
        run(args) // themed component picker (explicit opt-in)
    } else if tui || headless {
        crate::tui_setup::run(dry, packages)
    } else {
        launch_tui_terminal(dry, packages)
    }
}

/// In-session: open a Win2000-blue `foot` window running the real TUI installer
/// as root (`pkexec mde setup --tui`). The verified engine does the work; the
/// terminal is the graphical face. Dry runs skip privilege (they install nothing).
fn launch_tui_terminal(dry: bool, packages: Option<Vec<String>>) -> ExitCode {
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_else(|| "mde".to_string());
    // Forward a GUI-collected component set to the real engine, if any.
    let pkgs_arg = match packages {
        Some(p) if !p.is_empty() => format!(" --packages='{}'", p.join(",")),
        _ => String::new(),
    };
    let inner = if dry {
        format!("'{exe}' setup --tui --dry-run{pkgs_arg}; printf '\\nPress Enter to close… '; read _")
    } else {
        format!("pkexec '{exe}' setup --tui{pkgs_arg}")
    };
    let status = std::process::Command::new("foot")
        .args(["--title", "MDE-Retro Setup", "-o", "colors.background=0a246a"])
        .arg("sh")
        .arg("-c")
        .arg(inner)
        .status();
    match status {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("mde setup: could not launch terminal: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Hand the chosen component set to the privileged TUI engine in its own
/// Win2000-blue terminal, detached, then let the GUI exit (Q10: GUI collects →
/// TUI installs — one engine).
fn handoff(packages: &[String]) {
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_else(|| "mde".to_string());
    let inner = format!("pkexec '{exe}' setup --tui --packages='{}'", packages.join(","));
    let _ = std::process::Command::new("foot")
        .args(["--title", "MDE-Retro Setup", "-o", "colors.background=0a246a"])
        .arg("sh")
        .arg("-c")
        .arg(inner)
        .spawn();
}

pub fn run(_args: &[String]) -> ExitCode {
    // Build the catalogue + checked/locked/available state up front (the dnf
    // availability probe runs once here, before the window appears).
    let cat = crate::catalogue::catalogue();
    let n = cat.len();
    let pkgs: Vec<&str> = cat.iter().map(|c| c.package).collect();
    let available = crate::catalogue::available(&pkgs);
    let mut checked = vec![false; n];
    let mut locked = vec![false; n];
    let mut avail = vec![true; n];
    for (i, c) in cat.iter().enumerate() {
        let installed = crate::catalogue::is_installed(c.package);
        locked[i] = c.mandatory || installed;
        avail[i] = available.contains(c.package);
        checked[i] = c.mandatory || installed || (c.default_on && avail[i]);
    }

    let r = iced::application(|_: &Setup| "MDE-Retro Setup".to_string(), update, view)
        .window_size(iced::Size::new(640.0, 480.0))
        .resizable(false)
        .font(font::REGULAR_BYTES)
        .font(font::BOLD_BYTES)
        .font(font::PLEX_REGULAR_BYTES)
        .font(font::PLEX_BOLD_BYTES)
        .default_font(font::ui())
        .run_with(move || (Setup { cat, checked, locked, avail }, Task::none()));
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}

fn update(state: &mut Setup, msg: Msg) -> Task<Msg> {
    match msg {
        Msg::Toggle(i) => {
            if state.avail[i] && !state.locked[i] {
                state.checked[i] = !state.checked[i];
            }
        }
        Msg::Preset(p) => {
            let everything = p == 2;
            let standard = p == 1;
            for i in 0..state.cat.len() {
                state.checked[i] = state.locked[i]
                    || (state.avail[i] && (everything || (standard && state.cat[i].default_on)));
            }
        }
        Msg::Install => {
            let pkgs: Vec<String> = state
                .cat
                .iter()
                .enumerate()
                .filter(|(i, _)| state.checked[*i])
                .map(|(_, c)| c.package.to_string())
                .collect();
            handoff(&pkgs);
            exit(0);
        }
        Msg::Cancel => exit(0),
    }
    Task::none()
}

fn pad(t: f32, r: f32, b: f32, l: f32) -> Padding {
    Padding { top: t, right: r, bottom: b, left: l }
}

fn bg_gradient() -> Background {
    Background::Gradient(Gradient::Linear(
        Linear::new(Radians(std::f32::consts::PI))
            .add_stop(0.0, palette::color(palette::SETUP_GRADIENT_TOP))
            .add_stop(1.0, palette::color(palette::SETUP_GRADIENT_BOTTOM)),
    ))
}

fn status_bar<'a>() -> Element<'a, Msg> {
    container(text("MDE-Retro Professional Setup").size(10.0).color(dim()))
        .width(Length::Fill)
        .padding(pad(2.0, 8.0, 2.0, 8.0))
        .style(|_| container::Style {
            background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.35))),
            ..container::Style::default()
        })
        .into()
}

/// The scrollable category tree of component rows.
fn tree(state: &Setup) -> Element<'_, Msg> {
    let mut col = Column::new().spacing(2.0).padding(pad(0.0, 24.0, 0.0, 24.0));
    let mut last_cat = "";
    for (i, c) in state.cat.iter().enumerate() {
        if c.category != last_cat {
            last_cat = c.category;
            col = col.push(Space::new(Length::Fixed(1.0), Length::Fixed(6.0)));
            col = col.push(
                text(c.category).size(metrics::UI_PX).font(font::ui_bold()).color(white()),
            );
        }
        let (label, color, toggleable) = if !state.avail[i] {
            (format!("    [ ] {}   (unavailable)", c.name), dim(), false)
        } else if state.locked[i] {
            let tag = if c.mandatory { "required" } else { "installed" };
            (format!("    [x] {}   ({tag})", c.name), dim(), false)
        } else {
            let mark = if state.checked[i] { "[x]" } else { "[ ]" };
            (format!("    {mark} {}", c.name), white(), true)
        };
        let row = text(label).size(metrics::UI_PX).color(color);
        if toggleable {
            col = col.push(mouse_area(row).on_press(Msg::Toggle(i)));
        } else {
            col = col.push(row);
        }
    }
    scrollable(col).height(Length::Fill).style(mde_ui::scrollbar).into()
}

fn view(state: &Setup) -> Element<'_, Msg> {
    let mut header = Column::new().spacing(4.0).padding(pad(16.0, 24.0, 8.0, 24.0));
    header = header.push(
        text("Choose Components").size(15.0).font(font::ui_bold()).color(white()),
    );
    header = header.push(
        text("Select the software to install. Installed items are locked; unavailable ones are dimmed.")
            .size(metrics::UI_PX)
            .color(dim()),
    );

    let presets = Row::new()
        .spacing(8.0)
        .push(mde_ui::button(text("Minimal").size(metrics::UI_PX)).on_press(Msg::Preset(0)))
        .push(mde_ui::button(text("Standard").size(metrics::UI_PX)).on_press(Msg::Preset(1)))
        .push(mde_ui::button(text("Everything").size(metrics::UI_PX)).on_press(Msg::Preset(2)));

    let buttons = Row::new()
        .spacing(8.0)
        .push(Space::with_width(Length::Fill))
        .push(
            mde_ui::button(text("Install").size(metrics::UI_PX))
                .on_press(Msg::Install)
                .default(true)
                .width(Length::Fixed(96.0)),
        )
        .push(
            mde_ui::button(text("Cancel").size(metrics::UI_PX))
                .on_press(Msg::Cancel)
                .width(Length::Fixed(84.0)),
        );

    let body = Column::new()
        .push(header)
        .push(container(presets).padding(pad(0.0, 24.0, 8.0, 24.0)))
        .push(tree(state))
        .push(container(buttons).padding(pad(8.0, 24.0, 12.0, 24.0)));

    let screen = Column::new()
        .push(container(body).width(Length::Fill).height(Length::Fill))
        .push(status_bar());

    container(screen)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(bg_gradient()),
            ..container::Style::default()
        })
        .into()
}
