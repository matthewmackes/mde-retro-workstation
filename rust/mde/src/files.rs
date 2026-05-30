//! File manager — an Explorer-style window (stock iced xdg toplevel; sway draws
//! the navy title bar via our window theming).
//!
//! Client area, top to bottom: menubar (File/Edit/View/Favorites/Tools/Help),
//! a raised toolbar (Back/Forward/Up/Refresh/Home), an editable Address bar,
//! the sunken details list (Name/Size/Type, navigates on click), and a status
//! bar ("N object(s)"). Directory reads use std::fs; files open via xdg-open.

use std::path::PathBuf;
use std::process::{Command, ExitCode};

use iced::widget::{button, container, scrollable, text, text_input, Column, Row, Space};
use iced::{Background, Border, Element, Length, Padding, Shadow, Task};

use mde_ui::{frame, palette};

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
}

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
        if let Ok(rd) = std::fs::read_dir(&self.cwd) {
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
                    } else {
                        let _ = Command::new("xdg-open").arg(&entry.path).spawn();
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
        Message::AddressChanged(s) => state.address = s,
        Message::GoAddress => {
            let p = PathBuf::from(&state.address);
            if p.is_dir() {
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

fn menubar<'a>() -> Element<'a, Message> {
    let mut bar = Row::new().spacing(0.0);
    for label in ["File", "Edit", "View", "Favorites", "Tools", "Help"] {
        bar = bar.push(
            button(text(label).size(11.0))
                .on_press(Message::Noop)
                .padding(pad(2.0, 8.0, 2.0, 8.0))
                .style(flat),
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

fn tool<'a>(label: &'a str, msg: Message) -> Element<'a, Message> {
    button(text(label).size(11.0))
        .on_press(msg)
        .padding(pad(2.0, 8.0, 2.0, 8.0))
        .style(flat)
        .into()
}

fn toolbar(state: &Files) -> Element<'_, Message> {
    let back = state.hpos > 0;
    let fwd = state.hpos + 1 < state.history.len();
    let row = Row::new()
        .spacing(2.0)
        .padding(2.0)
        .push(tool("Back", if back { Message::Back } else { Message::Noop }))
        .push(tool("Forward", if fwd { Message::Forward } else { Message::Noop }))
        .push(tool("Up", Message::Up))
        .push(tool("Home", Message::Home))
        .push(tool("Refresh", Message::Refresh));
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
        .push(text("Address").size(11.0))
        .push(
            text_input("path", &state.address)
                .on_input(Message::AddressChanged)
                .on_submit(Message::GoAddress)
                .size(11.0)
                .width(Length::Fill),
        )
        .push(tool("Go", Message::GoAddress))
        .into()
}

fn header_cell<'a>(label: &'a str, width: Length) -> Element<'a, Message> {
    iced::widget::stack![
        frame::raised().thickness(1),
        container(text(label).size(11.0))
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
            .push(text(e.name.clone()).size(11.0).width(name_w))
            .push(
                text(if e.is_dir { String::new() } else { human(e.size) })
                    .size(11.0)
                    .width(size_w),
            )
            .push(text(kind(e)).size(11.0).width(type_w));
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
        .push(scrollable(rows).width(Length::Fill).height(Length::Fill));

    iced::widget::stack![
        frame::sunken().face(palette::color(palette::WINDOW)),
        container(inner).padding(2.0),
    ]
    .into()
}

fn status_bar(state: &Files) -> Element<'_, Message> {
    container(iced::widget::stack![
        frame::sunken().thickness(1),
        container(text(format!("{} object(s)", state.entries.len())).size(11.0))
            .padding(pad(1.0, 6.0, 1.0, 6.0)),
    ])
    .width(Length::Fill)
    .height(Length::Fixed(18.0))
    .into()
}

fn view(state: &Files) -> Element<'_, Message> {
    let content = Column::new()
        .push(menubar())
        .push(toolbar(state))
        .push(address_bar(state))
        .push(container(list(state)).width(Length::Fill).height(Length::Fill).padding(2.0))
        .push(status_bar(state));

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(palette::color(palette::MENU))),
            ..container::Style::default()
        })
        .into()
}
