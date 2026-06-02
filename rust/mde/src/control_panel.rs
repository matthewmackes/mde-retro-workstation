//! Control Panel — Win2000-named mapping of Fedora system tools.
//!
//! Default (no args) opens the GUI: an Explorer-style window with the blue
//! "web view" info-pane on the left and a white, categorized tool area on the
//! right (matching the My Computer reference). Clicking a tool launches it
//! (CLI tools at 150%); clicking a missing tool installs it via `pkexec dnf`.
//!
//! Headless subcommands remain for scripting:
//!   mde control-panel --list            list tools + [installed]/[MISSING]
//!   mde control-panel --launch N        launch tool number N
//!   mde control-panel --install-missing pkexec dnf the missing ones

use std::process::ExitCode;

use iced::widget::{button, container, mouse_area, scrollable, text, Column, Row, Space, Stack};
use iced::{Background, Border, Element, Length, Padding, Shadow, Task};

use mde_ui::{frame, metrics, palette};

use crate::fedora;

pub fn run(args: &[String]) -> ExitCode {
    match args.first().map(String::as_str) {
        Some("--list") => {
            list();
            ExitCode::SUCCESS
        }
        Some("--launch") => launch_n(args.get(1)),
        Some("--install-missing") => install_missing(),
        _ => {
            // Per-era routing (E6.7): under the Windows 10 theme the classic
            // Control Panel is superseded by the modern Settings app, so the
            // bare `mde control-panel` (Start "Settings", desktop menus, panel)
            // opens Settings instead. The `--list`/`--launch`/`--install-missing`
            // CLI arms stay Control Panel (they're scripting hooks). Other eras
            // (Carbon default, Win2000) keep the Control Panel unchanged.
            if mde_ui::palette::is_windows10() {
                let mde = std::env::current_exe()
                    .ok()
                    .and_then(|p| p.to_str().map(String::from))
                    .unwrap_or_else(|| "mde".to_string());
                let _ = std::process::Command::new(mde).arg("settings").spawn();
                return ExitCode::SUCCESS;
            }
            match gui() {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("mde control-panel: {e}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}

// --- GUI -------------------------------------------------------------------

#[derive(Default)]
struct ControlPanel {
    selected: Option<usize>,
    /// Installed-state per tool, parallel to `fedora::TOOLS`. Computed once at
    /// startup — `is_installed` spawns subprocesses (`command -v` / `rpm -q`),
    /// so calling it from the view would fire ~80 of them on every redraw.
    installed: Vec<bool>,
    /// Index of the open menubar dropdown (File=0 … Help=5), if any.
    menu_open: Option<usize>,
    /// The Help ▸ About box is showing.
    about_open: bool,
    /// How the applet area is laid out (the View menu).
    view: CpView,
}

/// The Win2000 folder view modes the View menu switches between.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum CpView {
    #[default]
    LargeIcons,
    List,
    Details,
}

#[derive(Debug, Clone)]
enum Message {
    Activate(usize),
    OpenMenu(usize),
    CloseMenus,
    SetView(CpView),
    Refresh,
    About,
    CloseWindow,
    Noop,
}

fn gui() -> iced::Result {
    iced::application(
        |_: &ControlPanel| "Control Panel - mde".to_string(),
        update,
        view,
    )
    .theme(|_| iced::Theme::Light)
    .font(mde_ui::font::REGULAR_BYTES)
    .font(mde_ui::font::BOLD_BYTES)
    .font(mde_ui::font::PLEX_REGULAR_BYTES)
    .font(mde_ui::font::PLEX_BOLD_BYTES)
    .default_font(mde_ui::font::ui())
    .run_with(|| {
        let installed = fedora::TOOLS.iter().map(fedora::is_installed).collect();
        (
            ControlPanel {
                installed,
                ..ControlPanel::default()
            },
            Task::none(),
        )
    })
}

fn update(state: &mut ControlPanel, message: Message) -> Task<Message> {
    match message {
        Message::Activate(i) => {
            // Single click selects and opens (a missing tool installs, then opens).
            state.selected = Some(i);
            if let Some(tool) = fedora::TOOLS.get(i) {
                if state.installed.get(i).copied().unwrap_or(false) {
                    let _ = fedora::launch(tool);
                } else if matches!(fedora::install(&[tool.package]), Ok(s) if s.success()) {
                    // Install + open in one gesture, like Win2000 Add/Remove.
                    if let Some(flag) = state.installed.get_mut(i) {
                        *flag = true;
                    }
                    let _ = fedora::launch(tool);
                }
            }
        }
        // Clicking a menubar title toggles its dropdown open/closed.
        Message::OpenMenu(i) => state.menu_open = (state.menu_open != Some(i)).then_some(i),
        Message::SetView(v) => {
            state.view = v;
            state.menu_open = None;
        }
        Message::CloseMenus => {
            state.menu_open = None;
            state.about_open = false;
        }
        Message::Refresh => {
            state.installed = fedora::TOOLS.iter().map(fedora::is_installed).collect();
            state.menu_open = None;
        }
        Message::About => {
            state.about_open = true;
            state.menu_open = None;
        }
        Message::CloseWindow => std::process::exit(0),
        Message::Noop => {}
    }
    Task::none()
}

fn pad(top: f32, right: f32, bottom: f32, left: f32) -> Padding {
    Padding {
        top,
        right,
        bottom,
        left,
    }
}

fn flat(theme: &iced::Theme, status: button::Status) -> button::Style {
    item_style(false)(theme, status)
}

fn item_style(selected: bool) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let hot = selected || matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: hot.then(|| Background::Color(palette::color(palette::HIGHLIGHT))),
            text_color: if hot {
                palette::color(palette::HIGHLIGHT_TEXT)
            } else {
                palette::color(palette::WINDOW_TEXT)
            },
            border: Border::default(),
            shadow: Shadow::default(),
        }
    }
}

const MENU_TITLES: [&str; 6] = ["File", "Edit", "View", "Favorites", "Tools", "Help"];

/// The dropdown contents for menu `i`: (label, action, enabled). Items that
/// don't apply to a folder-style window are present-but-disabled (greyed), the
/// Win2000 convention — so the menus read complete without faking behaviour.
/// The View entries carry a leading bullet on the active mode (the radio mark).
fn menu_items(i: usize, view: CpView) -> Vec<(&'static str, Message, bool)> {
    let mark = |v: CpView, on: &'static str, off: &'static str| if view == v { on } else { off };
    match i {
        0 => vec![("Close", Message::CloseWindow, true)],
        1 => vec![
            ("Cut", Message::Noop, false),
            ("Copy", Message::Noop, false),
            ("Paste", Message::Noop, false),
        ],
        2 => vec![
            (
                mark(CpView::LargeIcons, "\u{2022} Large Icons", "  Large Icons"),
                Message::SetView(CpView::LargeIcons),
                true,
            ),
            (
                mark(CpView::List, "\u{2022} List", "  List"),
                Message::SetView(CpView::List),
                true,
            ),
            (
                mark(CpView::Details, "\u{2022} Details", "  Details"),
                Message::SetView(CpView::Details),
                true,
            ),
            ("Refresh", Message::Refresh, true),
        ],
        3 => vec![("Add to Favorites\u{2026}", Message::Noop, false)],
        4 => vec![("Folder Options\u{2026}", Message::Noop, false)],
        5 => vec![("About Control Panel", Message::About, true)],
        _ => vec![],
    }
}

/// Approximate left edge (px) of each menubar title, for placing its dropdown.
const MENU_LEFT: [f32; 6] = [4.0, 42.0, 80.0, 124.0, 196.0, 240.0];
const MENUBAR_H: f32 = 20.0;

fn menubar(open: Option<usize>) -> Element<'static, Message> {
    let mut bar = Row::new();
    for (i, label) in MENU_TITLES.iter().enumerate() {
        // The open title shows the navy highlight (item_style(true)); the rest
        // are flat until hovered.
        bar = bar.push(if open == Some(i) {
            button(text(*label).size(metrics::UI_PX))
                .on_press(Message::OpenMenu(i))
                .padding(pad(2.0, 8.0, 2.0, 8.0))
                .style(item_style(true))
        } else {
            button(text(*label).size(metrics::UI_PX))
                .on_press(Message::OpenMenu(i))
                .padding(pad(2.0, 8.0, 2.0, 8.0))
                .style(flat)
        });
    }
    container(bar)
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(palette::color(palette::MENU))),
            ..container::Style::default()
        })
        .into()
}

/// A menubar dropdown panel for menu `i`, positioned under its title.
fn dropdown(i: usize, view: CpView) -> Element<'static, Message> {
    let items = menu_items(i, view);
    let mut col = Column::new().spacing(0.0);
    for (label, msg, enabled) in &items {
        // Enabled rows highlight navy-on-hover (item_style handles their text
        // colour); disabled rows are flat grey and never react.
        col = col.push(if *enabled {
            button(text(*label).size(metrics::UI_PX))
                .on_press(msg.clone())
                .width(Length::Fill)
                .padding(pad(2.0, 24.0, 2.0, 12.0))
                .style(item_style(false))
        } else {
            button(
                text(*label)
                    .size(metrics::UI_PX)
                    .color(palette::color(palette::GRAY_TEXT)),
            )
            .width(Length::Fill)
            .padding(pad(2.0, 24.0, 2.0, 12.0))
            .style(disabled_item)
        });
    }
    let panel = container(iced::widget::stack![
        frame::raised().thickness(2),
        container(col).padding(2.0)
    ])
    .width(Length::Fixed(168.0))
    .height(Length::Fixed(items.len() as f32 * 20.0 + 6.0));
    // Push it to MENU_LEFT[i] / below the menubar via leading spacers.
    Column::new()
        .push(Space::with_height(Length::Fixed(MENUBAR_H)))
        .push(
            Row::new()
                .push(Space::with_width(Length::Fixed(MENU_LEFT[i])))
                .push(panel),
        )
        .into()
}

/// Greyed (disabled) dropdown row: never highlights.
fn disabled_item(_t: &iced::Theme, _s: button::Status) -> button::Style {
    button::Style {
        background: None,
        text_color: palette::color(palette::GRAY_TEXT),
        border: Border::default(),
        shadow: Shadow::default(),
    }
}

/// The Help ▸ About box.
fn about_box() -> Element<'static, Message> {
    let bold = mde_ui::font::ui_bold();
    let body = Column::new()
        .spacing(8.0)
        .align_x(iced::Alignment::Center)
        .padding(pad(16.0, 20.0, 12.0, 20.0))
        .push(
            text("Control Panel")
                .size(metrics::INFO_TITLE_PX)
                .font(bold),
        )
        .push(text("MDE-Retro — a Windows 2000 desktop for Fedora").size(metrics::UI_PX))
        .push(text("Native Rust shell (iced)").size(metrics::UI_PX))
        .push(Space::with_height(Length::Fixed(6.0)))
        .push(
            button(text("OK").size(metrics::UI_PX))
                .on_press(Message::CloseMenus)
                .padding(pad(2.0, 16.0, 2.0, 16.0)),
        );
    let panel = container(iced::widget::stack![frame::raised(), container(body)])
        .width(Length::Fixed(300.0))
        .height(Length::Fixed(150.0));
    container(panel)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
}

fn sidebar<'a>() -> Element<'a, Message> {
    let bold = mde_ui::font::ui_bold();
    let accent = mde_ui::infoband::accent();
    let col = Column::new()
        .spacing(8.0)
        .padding(pad(10.0, 12.0, 10.0, 12.0))
        .push(
            Row::new()
                .spacing(8.0)
                .align_y(iced::Alignment::Center)
                .push(crate::icons::icon_any(
                    &["preferences-system", "gnome-control-center", "computer"],
                    32,
                ))
                .push(
                    text("Control Panel")
                        .size(metrics::INFO_TITLE_PX)
                        .font(bold)
                        .color(accent),
                ),
        )
        .push(container(Space::new(Length::Fill, Length::Fixed(2.0))).style(mde_ui::infoband::rule))
        .push(text("Select an item to view its description.").size(metrics::UI_PX))
        .push(
            container(
                text("Configures your computer and adds or removes programs and devices.")
                    .size(metrics::UI_PX),
            )
            .style(mde_ui::infoband::tip)
            .padding(pad(4.0, 6.0, 4.0, 6.0))
            .width(Length::Fill),
        )
        .push(Space::new(Length::Fill, Length::Fixed(6.0)))
        .push(text("See also:").size(metrics::UI_PX))
        .push(
            text("Administrative Tools")
                .size(metrics::UI_PX)
                .color(accent),
        )
        .push(text("Windows Update").size(metrics::UI_PX).color(accent));

    container(col)
        .width(Length::Fixed(190.0))
        .height(Length::Fill)
        .style(mde_ui::infoband::band)
        .into()
}

/// The white applet well, laid out per the active View mode.
fn grid(state: &ControlPanel) -> Element<'_, Message> {
    let content = match state.view {
        CpView::LargeIcons => grid_large(state),
        CpView::List => grid_list(state),
        CpView::Details => grid_details(state),
    };
    iced::widget::stack![
        frame::sunken().face(palette::color(palette::WINDOW)),
        // Fill the well so the scrollbar sits at the right edge (not at the
        // content's right edge, which left it stranded mid-window).
        container(
            scrollable(content)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(mde_ui::scrollbar),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(2.0),
    ]
    .into()
}

/// A bold category section heading.
fn cat_header(category: &'static str) -> Element<'static, Message> {
    container(
        text(category)
            .size(metrics::UI_PX)
            .font(mde_ui::font::ui_bold()),
    )
    .padding(pad(5.0, 6.0, 1.0, 4.0))
    .into()
}

/// The list label, with a "(install)" hint for tools not yet present.
fn tool_label(state: &ControlPanel, i: usize, tool: &fedora::Tool) -> String {
    if state.installed.get(i).copied().unwrap_or(true) {
        tool.name.to_string()
    } else {
        format!("{}  (install)", tool.name)
    }
}

/// Grey a label when the tool isn't installed (it stays clickable → installs on
/// click via pkexec dnf), so present vs. installable reads at a glance.
fn dim_missing(
    state: &ControlPanel,
    i: usize,
    t: iced::widget::Text<'static>,
) -> iced::widget::Text<'static> {
    if state.installed.get(i).copied().unwrap_or(true) {
        t
    } else {
        t.color(palette::color(palette::GRAY_TEXT))
    }
}

/// List view: one row per applet, 16px icon + name, grouped by category.
fn grid_list(state: &ControlPanel) -> Element<'_, Message> {
    let mut col = Column::new()
        .spacing(0.0)
        .padding(pad(4.0, 4.0, 4.0, 6.0))
        .width(Length::Fill);
    for category in fedora::categories() {
        col = col.push(cat_header(category));
        for (i, tool) in fedora::TOOLS.iter().enumerate() {
            if tool.category != category {
                continue;
            }
            let row = Row::new()
                .spacing(5.0)
                .align_y(iced::Alignment::Center)
                .push(crate::icons::icon_any(tool.icons, 16))
                .push(dim_missing(
                    state,
                    i,
                    text(tool_label(state, i, tool)).size(metrics::UI_PX),
                ));
            col = col.push(
                button(row)
                    .on_press(Message::Activate(i))
                    .width(Length::Fill)
                    .padding(pad(2.0, 8.0, 2.0, 8.0))
                    .style(item_style(state.selected == Some(i))),
            );
        }
    }
    col.into()
}

/// Number of large-icon cells per row (the applet well is ~360px wide).
const LARGE_COLS: usize = 4;

/// Large Icons view: 32px icons with the name beneath, flowing in rows under
/// each category heading (the classic Control Panel default).
fn grid_large(state: &ControlPanel) -> Element<'_, Message> {
    let mut col = Column::new()
        .spacing(2.0)
        .padding(pad(4.0, 4.0, 4.0, 6.0))
        .width(Length::Fill);
    for category in fedora::categories() {
        col = col.push(cat_header(category));
        let items: Vec<(usize, &fedora::Tool)> = fedora::TOOLS
            .iter()
            .enumerate()
            .filter(|(_, t)| t.category == category)
            .collect();
        for chunk in items.chunks(LARGE_COLS) {
            let mut row = Row::new().spacing(4.0);
            for (i, tool) in chunk {
                row = row.push(large_cell(state, *i, tool));
            }
            col = col.push(row);
        }
    }
    col.into()
}

/// One large-icon cell: a 32px icon over a centered, wrapping label.
fn large_cell<'a>(state: &ControlPanel, i: usize, tool: &'a fedora::Tool) -> Element<'a, Message> {
    let cell = Column::new()
        .align_x(iced::Alignment::Center)
        .spacing(3.0)
        .width(Length::Fixed(82.0))
        .push(crate::icons::icon_any(tool.icons, 32))
        .push(dim_missing(
            state,
            i,
            text(tool.name.to_string())
                .size(metrics::UI_PX)
                .align_x(iced::Alignment::Center)
                .width(Length::Fill),
        ));
    button(cell)
        .on_press(Message::Activate(i))
        .padding(pad(6.0, 2.0, 6.0, 2.0))
        .style(item_style(state.selected == Some(i)))
        .into()
}

/// Details view: a columnar list (Name · Category · Status) with a header row.
fn grid_details(state: &ControlPanel) -> Element<'_, Message> {
    let bold = mde_ui::font::ui_bold();
    let header = Row::new()
        .padding(pad(2.0, 6.0, 3.0, 6.0))
        .push(
            text("Name")
                .size(metrics::UI_PX)
                .font(bold)
                .width(Length::Fill),
        )
        .push(
            text("Category")
                .size(metrics::UI_PX)
                .font(bold)
                .width(Length::Fixed(150.0)),
        )
        .push(
            text("Status")
                .size(metrics::UI_PX)
                .font(bold)
                .width(Length::Fixed(96.0)),
        );
    let mut col = Column::new()
        .spacing(0.0)
        .padding(pad(2.0, 4.0, 4.0, 4.0))
        .width(Length::Fill)
        .push(header);
    for (i, tool) in fedora::TOOLS.iter().enumerate() {
        let installed = state.installed.get(i).copied().unwrap_or(true);
        let status = if installed {
            "Installed"
        } else {
            "Not installed"
        };
        let name = Row::new()
            .spacing(5.0)
            .align_y(iced::Alignment::Center)
            .width(Length::Fill)
            .push(crate::icons::icon_any(tool.icons, 16))
            .push(text(tool.name.to_string()).size(metrics::UI_PX));
        let row = Row::new()
            .align_y(iced::Alignment::Center)
            .push(name)
            .push(
                text(tool.category.to_string())
                    .size(metrics::UI_PX)
                    .width(Length::Fixed(150.0)),
            )
            .push(
                text(status.to_string())
                    .size(metrics::UI_PX)
                    .width(Length::Fixed(96.0)),
            );
        col = col.push(
            button(row)
                .on_press(Message::Activate(i))
                .width(Length::Fill)
                .padding(pad(1.0, 6.0, 1.0, 6.0))
                .style(item_style(state.selected == Some(i))),
        );
    }
    col.into()
}

fn status_bar(state: &ControlPanel) -> Element<'_, Message> {
    let total = fedora::TOOLS.len();
    let missing = state.installed.iter().filter(|&&i| !i).count();
    container(iced::widget::stack![
        frame::sunken().thickness(1),
        container(text(format!("{total} items, {missing} not installed")).size(metrics::UI_PX))
            .padding(pad(1.0, 6.0, 1.0, 6.0)),
    ])
    .width(Length::Fill)
    .height(Length::Fixed(18.0))
    .into()
}

fn view(state: &ControlPanel) -> Element<'_, Message> {
    let body = Row::new().push(sidebar()).push(
        container(grid(state))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(2.0),
    );

    let content = Column::new()
        .push(menubar(state.menu_open))
        .push(container(body).width(Length::Fill).height(Length::Fill))
        .push(status_bar(state));

    let window = container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(palette::color(palette::MENU))),
            ..container::Style::default()
        });

    let mut layers = Stack::new().push(window);
    // An open dropdown (or the About box) gets a full-window click-catcher behind
    // it so a click anywhere else dismisses it, then the floating panel on top.
    if let Some(i) = state.menu_open {
        layers = layers
            .push(mouse_area(Space::new(Length::Fill, Length::Fill)).on_press(Message::CloseMenus))
            .push(dropdown(i, state.view));
    }
    if state.about_open {
        layers = layers
            .push(mouse_area(Space::new(Length::Fill, Length::Fill)).on_press(Message::CloseMenus))
            .push(about_box());
    }
    layers.into()
}

// --- headless backend ------------------------------------------------------

fn list() {
    println!("Control Panel — Fedora system tools\n");
    let mut n = 0;
    for category in fedora::categories() {
        println!("{category}");
        for tool in fedora::TOOLS.iter().filter(|t| t.category == category) {
            n += 1;
            let status = if fedora::is_installed(tool) {
                "installed"
            } else {
                "MISSING  "
            };
            println!(
                "  {:>2}. [{}]  {:<32}  ({})",
                n,
                status,
                tool.name,
                fedora::binary(tool.command)
            );
        }
        println!();
    }
    let missing = fedora::missing_packages();
    if missing.is_empty() {
        println!("All backing tools are installed.");
    } else {
        println!("{} missing. Packages: {}", missing.len(), missing.join(" "));
    }
}

fn launch_n(arg: Option<&String>) -> ExitCode {
    match arg
        .and_then(|s| s.parse::<usize>().ok())
        .and_then(|n| fedora::TOOLS.get(n.saturating_sub(1)))
    {
        Some(tool) => match fedora::launch(tool) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("launch failed: {e}");
                ExitCode::FAILURE
            }
        },
        None => {
            eprintln!("--launch needs a valid tool number");
            ExitCode::from(2)
        }
    }
}

fn install_missing() -> ExitCode {
    let packages = fedora::missing_packages();
    if packages.is_empty() {
        println!("Nothing to install.");
        return ExitCode::SUCCESS;
    }
    println!("Installing: {}", packages.join(" "));
    match fedora::install(&packages) {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        Ok(s) => {
            eprintln!("dnf exited with {s}");
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("pkexec dnf failed: {e}");
            ExitCode::FAILURE
        }
    }
}
