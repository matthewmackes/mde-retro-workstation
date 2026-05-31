//! File manager — an Explorer-style window (stock iced xdg toplevel; sway draws
//! the navy title bar via our window theming).
//!
//! Client area, top to bottom: menubar (File/Edit/View/Favorites/Tools/Help),
//! a raised toolbar (Back/Forward/Up/Refresh/Home), an editable Address bar,
//! the sunken details list (Name/Size/Type, navigates on click), and a status
//! bar ("N object(s)"). Directory reads use std::fs; files open via xdg-open.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use iced::widget::{button, container, scrollable, text, text_input, Column, Row, Space};
use iced::{Background, Border, Element, Length, Padding, Shadow, Task};

use mde_ui::{frame, metrics, palette};

#[derive(Debug, Clone)]
struct Entry {
    name: String,
    path: PathBuf,
    is_dir: bool,
    size: u64,
}

struct Files {
    cwd: PathBuf,
    address: String,
    entries: Vec<Entry>,
    history: Vec<PathBuf>,
    hpos: usize,
    selected: Option<usize>,
    last_click: Option<(usize, std::time::Instant)>,
    /// Last navigation/IO problem, shown in the status bar instead of leaving
    /// the user staring at an unchanged or empty list with no explanation.
    error: Option<String>,
    /// Expanded directories in the left tree pane.
    tree_expanded: HashSet<PathBuf>,
    /// Which menubar menu is open (index into MENUS), if any.
    open_menu: Option<usize>,
}

/// The menubar titles (indices used by `open_menu` / ToggleMenu).
const MENUS: [&str; 6] = ["File", "Edit", "View", "Favorites", "Tools", "Help"];

#[derive(Debug, Clone)]
enum Message {
    Open(usize),
    Up,
    Back,
    Forward,
    Home,
    Refresh,
    AddressChanged(String),
    GoAddress,
    TreeToggle(PathBuf),
    TreeNav(PathBuf),
    ToggleMenu(usize),
    CloseMenu,
    NewFolder,
    CloseWindow,
    About,
    Noop,
}

pub fn run(args: &[String]) -> ExitCode {
    let start = args
        .first()
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .or_else(home)
        .unwrap_or_else(|| PathBuf::from("/"));
    match launch(start) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mde files: {e}");
            ExitCode::FAILURE
        }
    }
}

fn launch(start: PathBuf) -> iced::Result {
    iced::application(title, update, view)
        .theme(|_| iced::Theme::Light)
        .font(mde_ui::font::REGULAR_BYTES)
        .font(mde_ui::font::BOLD_BYTES)
        .default_font(mde_ui::font::UI)
        .run_with(move || {
            let mut f = Files {
                cwd: start.clone(),
                address: start.display().to_string(),
                entries: Vec::new(),
                history: vec![start.clone()],
                hpos: 0,
                selected: None,
                last_click: None,
                error: None,
                tree_expanded: home().into_iter().collect(),
                open_menu: None,
            };
            f.load();
            (f, Task::none())
        })
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn title(state: &Files) -> String {
    let name = state
        .cwd
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| state.cwd.display().to_string());
    format!("{name} - mde files")
}

impl Files {
    fn load(&mut self) {
        let mut entries = Vec::new();
        match std::fs::read_dir(&self.cwd) {
            Ok(rd) => {
                for e in rd.flatten() {
                    let path = e.path();
                    let md = e.metadata().ok();
                    let is_dir = md.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                    let size = md.as_ref().map(|m| m.len()).unwrap_or(0);
                    entries.push(Entry {
                        name: e.file_name().to_string_lossy().to_string(),
                        path,
                        is_dir,
                        size,
                    });
                }
                self.error = None;
            }
            Err(e) => {
                self.error = Some(match e.kind() {
                    std::io::ErrorKind::PermissionDenied => "Access denied.".to_string(),
                    std::io::ErrorKind::NotFound => "Folder not found.".to_string(),
                    _ => "Cannot read this folder.".to_string(),
                });
            }
        }
        // Folders first, then files; alphabetical, case-insensitive.
        entries.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        self.entries = entries;
        self.address = self.cwd.display().to_string();
        self.selected = None;
        self.last_click = None;
    }

    fn navigate(&mut self, path: PathBuf) {
        self.cwd = path.clone();
        self.load();
        self.history.truncate(self.hpos + 1);
        self.history.push(path);
        self.hpos = self.history.len() - 1;
    }
}

fn update(state: &mut Files, message: Message) -> Task<Message> {
    match message {
        Message::Open(i) => {
            // Single-click selects; double-click (within 400ms) opens — the
            // classic Windows shell activation model.
            let now = std::time::Instant::now();
            let is_double = state
                .last_click
                .map(|(li, lt)| li == i && now.duration_since(lt) < std::time::Duration::from_millis(400))
                .unwrap_or(false);
            if is_double {
                state.last_click = None;
                if let Some(entry) = state.entries.get(i) {
                    if entry.is_dir {
                        let p = entry.path.clone();
                        state.navigate(p);
                    } else if Command::new("xdg-open").arg(&entry.path).spawn().is_err() {
                        state.error = Some("Could not open this file.".to_string());
                    }
                }
            } else {
                state.selected = Some(i);
                state.last_click = Some((i, now));
            }
        }
        Message::Up => {
            if let Some(parent) = state.cwd.parent() {
                let p = parent.to_path_buf();
                state.navigate(p);
            }
        }
        Message::Back => {
            if state.hpos > 0 {
                state.hpos -= 1;
                state.cwd = state.history[state.hpos].clone();
                state.load();
            }
        }
        Message::Forward => {
            if state.hpos + 1 < state.history.len() {
                state.hpos += 1;
                state.cwd = state.history[state.hpos].clone();
                state.load();
            }
        }
        Message::Home => {
            if let Some(h) = home() {
                state.navigate(h);
            }
        }
        Message::Refresh => state.load(),
        Message::AddressChanged(s) => {
            state.address = s;
            state.error = None;
        }
        Message::GoAddress => {
            let p = PathBuf::from(&state.address);
            if p.is_dir() {
                state.navigate(p);
            } else {
                state.error = Some(format!("Cannot find '{}'.", state.address));
            }
        }
        Message::ToggleMenu(i) => {
            state.open_menu = if state.open_menu == Some(i) { None } else { Some(i) };
        }
        Message::CloseMenu => state.open_menu = None,
        Message::NewFolder => {
            state.open_menu = None;
            // Create "New Folder" (then " (2)", " (3)"… on conflict), like the shell.
            let mut target = state.cwd.join("New Folder");
            let mut n = 2;
            while target.exists() {
                target = state.cwd.join(format!("New Folder ({n})"));
                n += 1;
            }
            match std::fs::create_dir(&target) {
                Ok(()) => state.load(),
                Err(e) => state.error = Some(format!("Could not create folder: {e}")),
            }
        }
        Message::CloseWindow => std::process::exit(0),
        Message::About => {
            state.open_menu = None;
            state.error = Some("MDE-Retro file manager".to_string());
        }
        Message::TreeToggle(p) => {
            if !state.tree_expanded.remove(&p) {
                state.tree_expanded.insert(p);
            }
        }
        Message::TreeNav(p) => {
            if p.is_dir() {
                state.tree_expanded.insert(p.clone());
                state.navigate(p);
            }
        }
        Message::Noop => {}
    }
    Task::none()
}

// --- styling helpers -------------------------------------------------------

fn pad(top: f32, right: f32, bottom: f32, left: f32) -> Padding {
    Padding { top, right, bottom, left }
}

/// Flat item that highlights navy on hover (menubar entries).
fn flat(_theme: &iced::Theme, status: button::Status) -> button::Style {
    row_style(false)(_theme, status)
}

/// List-row style: navy when selected or hovered (white text), else plain.
fn row_style(selected: bool) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
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

fn human(size: u64) -> String {
    if size >= 1 << 20 {
        format!("{:.1} MB", size as f64 / (1u64 << 20) as f64)
    } else if size >= 1 << 10 {
        format!("{} KB", size / (1 << 10))
    } else {
        format!("{size} B")
    }
}

fn kind(e: &Entry) -> String {
    if e.is_dir {
        "File Folder".to_string()
    } else if let Some(ext) = e.path.extension() {
        format!("{} File", ext.to_string_lossy().to_uppercase())
    } else {
        "File".to_string()
    }
}

fn menubar(state: &Files) -> Element<'_, Message> {
    let mut bar = Row::new().spacing(0.0);
    for (i, label) in MENUS.iter().enumerate() {
        bar = bar.push(
            button(text(*label).size(metrics::UI_PX))
                .on_press(Message::ToggleMenu(i))
                .padding(pad(2.0, 8.0, 2.0, 8.0))
                .style(row_style(state.open_menu == Some(i))),
        );
    }
    container(bar)
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(palette::color(palette::MENU))),
            ..container::Style::default()
        })
        .into()
}

/// The commands in menubar menu `i`: (label, message, enabled). Disabled items
/// (not yet wired) render grayed, the Win2000 way, rather than being absent.
fn menu_items(i: usize) -> Vec<(&'static str, Message, bool)> {
    match i {
        0 => vec![("New Folder", Message::NewFolder, true), ("Close", Message::CloseWindow, true)],
        1 => vec![
            ("Cut", Message::Noop, false),
            ("Copy", Message::Noop, false),
            ("Paste", Message::Noop, false),
        ],
        2 => vec![("Refresh", Message::Refresh, true)],
        3 => vec![("Add to Favorites", Message::Noop, false)],
        4 => vec![("Folder Options...", Message::Noop, false)],
        5 => vec![("About MDE-Retro", Message::About, true)],
        _ => vec![],
    }
}

/// Estimated x-offset of menubar item `i` (label width + padding). Exact pixel
/// alignment is a grim-tuning refinement; this places the dropdown under its title.
fn menu_x(i: usize) -> f32 {
    MENUS[..i].iter().map(|l| l.len() as f32 * 6.0 + 16.0).sum()
}

/// The dropdown panel for menu `i`: a raised frame over the command list.
fn dropdown(i: usize) -> Element<'static, Message> {
    let mut col = Column::new().spacing(0.0);
    for (label, msg, enabled) in menu_items(i) {
        let color = if enabled {
            palette::color(palette::MENU_TEXT)
        } else {
            palette::color(palette::GRAY_TEXT)
        };
        col = col.push(
            button(text(label).size(metrics::UI_PX).color(color))
                .on_press(if enabled { msg } else { Message::Noop })
                .width(Length::Fill)
                .padding(pad(2.0, 16.0, 2.0, 8.0))
                .style(flat),
        );
    }
    iced::widget::stack![frame::raised(), container(col).padding(2.0)].into()
}

/// A toolbar button: raised 3D chrome (Win2000 hot-track / classic), pressed on
/// click — NOT the navy menu-highlight style. `None` = unavailable, drawn
/// disabled (gray) via the mde-ui button's no-action state.
fn tool<'a>(label: &'a str, msg: Option<Message>) -> Element<'a, Message> {
    let mut b = mde_ui::button(text(label).size(metrics::UI_PX)).padding(pad(2.0, 8.0, 2.0, 8.0));
    if let Some(m) = msg {
        b = b.on_press(m);
    }
    b.into()
}

fn toolbar(state: &Files) -> Element<'_, Message> {
    let back = state.hpos > 0;
    let fwd = state.hpos + 1 < state.history.len();
    let row = Row::new()
        .spacing(2.0)
        .padding(2.0)
        .push(tool("Back", back.then_some(Message::Back)))
        .push(tool("Forward", fwd.then_some(Message::Forward)))
        .push(tool("Up", Some(Message::Up)))
        .push(tool("Home", Some(Message::Home)))
        .push(tool("Refresh", Some(Message::Refresh)));
    container(iced::widget::stack![
        frame::raised().thickness(1),
        container(row).width(Length::Fill)
    ])
    .width(Length::Fill)
    .height(Length::Fixed(26.0))
    .into()
}

fn address_bar(state: &Files) -> Element<'_, Message> {
    Row::new()
        .spacing(6.0)
        .padding(pad(2.0, 4.0, 2.0, 4.0))
        .align_y(iced::Alignment::Center)
        .push(text("Address").size(metrics::UI_PX))
        .push(
            text_input("path", &state.address)
                .on_input(Message::AddressChanged)
                .on_submit(Message::GoAddress)
                .size(metrics::UI_PX)
                .width(Length::Fill)
                .style(mde_ui::sunken_field),
        )
        .push(tool("Go", Some(Message::GoAddress)))
        .into()
}

fn header_cell<'a>(label: &'a str, width: Length) -> Element<'a, Message> {
    // A column header is a raised button-like cell (full 2-line edge), the way
    // Win2000's list-view header draws — not a thin hairline.
    iced::widget::stack![
        frame::raised(),
        container(text(label).size(metrics::UI_PX))
            .padding(pad(1.0, 6.0, 1.0, 6.0))
            .width(width),
    ]
    .into()
}

fn list(state: &Files) -> Element<'_, Message> {
    let name_w = Length::FillPortion(6);
    let size_w = Length::Fixed(90.0);
    let type_w = Length::Fixed(120.0);

    let header = Row::new()
        .height(Length::Fixed(18.0))
        .push(header_cell("Name", name_w))
        .push(header_cell("Size", size_w))
        .push(header_cell("Type", type_w));

    let mut rows = Column::new().spacing(0.0);
    for (i, e) in state.entries.iter().enumerate() {
        let row = Row::new()
            .push(text(e.name.clone()).size(metrics::UI_PX).width(name_w))
            .push(
                text(if e.is_dir { String::new() } else { human(e.size) })
                    .size(metrics::UI_PX)
                    .width(size_w),
            )
            .push(text(kind(e)).size(metrics::UI_PX).width(type_w));
        rows = rows.push(
            button(row)
                .on_press(Message::Open(i))
                .padding(pad(1.0, 4.0, 1.0, 4.0))
                .width(Length::Fill)
                .style(row_style(state.selected == Some(i))),
        );
    }

    let inner = Column::new()
        .push(header)
        .push(scrollable(rows).width(Length::Fill).height(Length::Fill).style(mde_ui::scrollbar));

    iced::widget::stack![
        frame::sunken().face(palette::color(palette::WINDOW)),
        container(inner).padding(2.0),
    ]
    .into()
}

fn status_bar(state: &Files) -> Element<'_, Message> {
    let msg = match &state.error {
        Some(e) => e.clone(),
        None => format!("{} object(s)", state.entries.len()),
    };
    container(iced::widget::stack![
        frame::sunken(),
        container(text(msg).size(metrics::UI_PX)).padding(pad(1.0, 6.0, 1.0, 6.0)),
    ])
    .width(Length::Fill)
    .height(Length::Fixed(18.0))
    .into()
}

/// Immediate subdirectories of `path`, sorted (case-insensitive).
fn subdirs(path: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    v.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    v
}

fn tree_label(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

/// Recursive tree rows for `path` at `depth`; expands children when in the set.
fn tree_rows(state: &Files, path: &Path, label: String, depth: u16) -> Vec<Element<'static, Message>> {
    let expanded = state.tree_expanded.contains(path);
    let marker = if expanded { "\u{25bc}" } else { "\u{25b6}" }; // ▼ / ▶
    let row = Row::new()
        .push(Space::with_width(Length::Fixed(depth as f32 * 12.0)))
        .push(
            button(text(marker).size(metrics::UI_PX))
                .on_press(Message::TreeToggle(path.to_path_buf()))
                .padding(pad(0.0, 3.0, 0.0, 3.0))
                .style(flat),
        )
        .push(
            button(text(label).size(metrics::UI_PX))
                .on_press(Message::TreeNav(path.to_path_buf()))
                .width(Length::Fill)
                .padding(pad(0.0, 4.0, 0.0, 2.0))
                .style(row_style(path == state.cwd)),
        );
    let mut rows: Vec<Element<'static, Message>> = vec![row.into()];
    if expanded {
        for kid in subdirs(path) {
            let l = tree_label(&kid);
            rows.extend(tree_rows(state, &kid, l, depth + 1));
        }
    }
    rows
}

fn tree_pane(state: &Files) -> Element<'_, Message> {
    let mut col = Column::new().spacing(0.0);
    // Roots: the user's home, then the filesystem root ("My Computer").
    if let Some(h) = home() {
        for row in tree_rows(state, &h, "Home".to_string(), 0) {
            col = col.push(row);
        }
    }
    for row in tree_rows(state, Path::new("/"), "Filesystem".to_string(), 0) {
        col = col.push(row);
    }
    iced::widget::stack![
        frame::sunken().face(palette::color(palette::WINDOW)),
        container(scrollable(col).style(mde_ui::scrollbar)).padding(2.0),
    ]
    .into()
}

fn view(state: &Files) -> Element<'_, Message> {
    let body = Row::new()
        .push(
            container(tree_pane(state))
                .width(Length::Fixed(180.0))
                .height(Length::Fill)
                .padding(pad(2.0, 1.0, 2.0, 2.0)),
        )
        .push(container(list(state)).width(Length::Fill).height(Length::Fill).padding(2.0));

    let content = Column::new()
        .push(menubar(state))
        .push(toolbar(state))
        .push(address_bar(state))
        .push(container(body).width(Length::Fill).height(Length::Fill))
        .push(status_bar(state));

    let base = container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(palette::color(palette::MENU))),
            ..container::Style::default()
        });

    // An open menubar menu overlays a dropdown (positioned under its title) plus
    // a transparent full-window catcher so a click anywhere else closes it.
    match state.open_menu {
        Some(i) => {
            let catcher = iced::widget::mouse_area(Space::new(Length::Fill, Length::Fill))
                .on_press(Message::CloseMenu);
            let positioned = Column::new()
                .push(Space::with_height(Length::Fixed(20.0)))
                .push(
                    Row::new()
                        .push(Space::with_width(Length::Fixed(menu_x(i))))
                        .push(container(dropdown(i)).width(Length::Fixed(170.0))),
                );
            iced::widget::stack![base, catcher, positioned].into()
        }
        None => base.into(),
    }
}
