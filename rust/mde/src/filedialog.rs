//! Common File Dialog — the Windows 2000 Open / Save / Browse interface.
//!
//! A standalone, reusable dialog: it prints the chosen path to stdout and exits
//! 0, or exits non-zero on Cancel — so any part of the shell (and scripts) can
//! drive it, exactly the drop-in pattern the wallpaper Browse button uses. The
//! caller spawns it and awaits the result, which blocks that flow like a modal.
//!
//! Layout (per the classic common dialog, not a modern file picker):
//!   ┌ Look in: [folder ▾]            [Up] [New] [Views] ┐
//!   │ ┌Places┐ ┌ file / folder list ───────────────────┐│
//!   │ │ … bar│ │ folders first, then files (filtered)   ││
//!   │ └──────┘ └────────────────────────────────────────┘│
//!   │ File name:    [____________________]      [ Open ]  │
//!   │ Files of type:[ Images (*.png…) ▾  ]      [Cancel]  │
//!   └────────────────────────────────────────────────────┘
//! Filesystem navigation (no breadcrumb bar, no search-first, no preview pane).
//!
//!   mde filedialog [--save] [--title T] [--dir D] [--filename F]
//!                  [--filter "Images:png,jpg;All Files:*"]

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use iced::widget::{container, scrollable, text, text_input, Column, Row};
use iced::{Background, Border, Element, Length, Padding, Shadow, Task};

use mde_ui::{button, frame, metrics, palette};

// --- model -----------------------------------------------------------------

struct Entry {
    name: String,
    path: PathBuf,
    is_dir: bool,
    size: u64,
}

#[derive(Clone)]
struct Filter {
    label: String,
    /// Lowercased extensions, or a single "*" meaning all files.
    exts: Vec<String>,
}

impl Filter {
    fn accepts(&self, name: &str) -> bool {
        if self.exts.iter().any(|e| e == "*") {
            return true;
        }
        let ext = Path::new(name)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        self.exts.contains(&ext)
    }

    fn pattern(&self) -> String {
        if self.exts.iter().any(|e| e == "*") {
            "*.*".to_string()
        } else {
            self.exts
                .iter()
                .map(|e| format!("*.{e}"))
                .collect::<Vec<_>>()
                .join(";")
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PathChoice(PathBuf);
impl std::fmt::Display for PathChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = self.0.file_name().and_then(|s| s.to_str());
        match name {
            Some(n) => f.write_str(n),
            None => f.write_str("/ (Computer)"),
        }
    }
}

#[derive(Debug, Clone)]
struct FilterChoice {
    idx: usize,
    label: String,
}
impl PartialEq for FilterChoice {
    fn eq(&self, other: &Self) -> bool {
        self.idx == other.idx
    }
}
impl std::fmt::Display for FilterChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

struct FileDialog {
    save: bool,
    current: PathBuf,
    entries: Vec<Entry>,
    selected: Option<usize>,
    last_click: Option<(usize, Instant)>,
    filename: String,
    filters: Vec<Filter>,
    filter_idx: usize,
    details: bool,
}

#[derive(Debug, Clone)]
enum Message {
    LookIn(PathChoice),
    Place(usize),
    Up,
    NewFolder,
    ToggleView,
    ClickEntry(usize),
    FilenameChanged(String),
    SetFilter(FilterChoice),
    Accept,
    Cancel,
}

// --- CLI dispatch -----------------------------------------------------------

pub fn run(args: &[String]) -> ExitCode {
    let mut save = false;
    let mut title = String::new();
    let mut dir: Option<PathBuf> = None;
    let mut filename = String::new();
    let mut filter_spec = String::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--save" => save = true,
            "--title" => title = it.next().cloned().unwrap_or_default(),
            "--dir" => dir = it.next().map(PathBuf::from),
            "--filename" => filename = it.next().cloned().unwrap_or_default(),
            "--filter" => filter_spec = it.next().cloned().unwrap_or_default(),
            _ => {}
        }
    }
    let filters = parse_filters(&filter_spec);
    let current = dir
        .filter(|d| d.is_dir())
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("/"));
    let title = if title.is_empty() {
        if save {
            "Save As".to_string()
        } else {
            "Open".to_string()
        }
    } else {
        title
    };
    match gui(save, title, current, filename, filters) {
        // Selection is printed inside the GUI (then it exits 0); reaching here
        // without a selection means the window closed / Cancel.
        Ok(()) => ExitCode::from(1),
        Err(e) => {
            eprintln!("mde filedialog: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Parse `"Images:png,jpg;All Files:*"` into filters; default to All Files.
fn parse_filters(spec: &str) -> Vec<Filter> {
    let mut out = Vec::new();
    for group in spec.split(';').filter(|s| !s.trim().is_empty()) {
        let (label, exts) = group.split_once(':').unwrap_or((group, "*"));
        let exts: Vec<String> = exts
            .split(',')
            .map(|e| e.trim().trim_start_matches('.').to_ascii_lowercase())
            .filter(|e| !e.is_empty())
            .collect();
        let exts = if exts.is_empty() {
            vec!["*".to_string()]
        } else {
            exts
        };
        let f = Filter {
            label: String::new(),
            exts,
        };
        // Build the "Label (*.ext;…)" display string.
        out.push(Filter {
            label: format!("{} ({})", label.trim(), f.pattern()),
            exts: f.exts,
        });
    }
    if out.is_empty() {
        out.push(Filter {
            label: "All Files (*.*)".to_string(),
            exts: vec!["*".to_string()],
        });
    }
    out
}

fn gui(
    save: bool,
    title: String,
    current: PathBuf,
    filename: String,
    filters: Vec<Filter>,
) -> iced::Result {
    iced::application(move |_: &FileDialog| title.clone(), update, view)
        .window_size(iced::Size::new(540.0, 380.0))
        .theme(|_| mde_ui::palette::iced_theme())
        .font(mde_ui::font::REGULAR_BYTES)
        .font(mde_ui::font::BOLD_BYTES)
        .font(mde_ui::font::PLEX_REGULAR_BYTES)
        .font(mde_ui::font::PLEX_BOLD_BYTES)
        .default_font(mde_ui::font::ui())
        .run_with(move || {
            let entries = read_entries(&current, &filters[0]);
            (
                FileDialog {
                    save,
                    current,
                    entries,
                    selected: None,
                    last_click: None,
                    filename,
                    filters,
                    filter_idx: 0,
                    details: false,
                },
                Task::none(),
            )
        })
}

// --- filesystem -------------------------------------------------------------

/// Folders first (always shown), then files matching the active filter; hidden
/// dotfiles are skipped, matching the classic dialog's default.
fn read_entries(dir: &Path, filter: &Filter) -> Vec<Entry> {
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let path = e.path();
            let md = e.metadata().ok();
            let is_dir = md.as_ref().map(|m| m.is_dir()).unwrap_or(false);
            let size = md.as_ref().map(|m| m.len()).unwrap_or(0);
            if is_dir {
                dirs.push(Entry {
                    name,
                    path,
                    is_dir,
                    size,
                });
            } else if filter.accepts(&name) {
                files.push(Entry {
                    name,
                    path,
                    is_dir,
                    size,
                });
            }
        }
    }
    dirs.sort_by_key(|a| a.name.to_lowercase());
    files.sort_by_key(|a| a.name.to_lowercase());
    dirs.extend(files);
    dirs
}

fn navigate(state: &mut FileDialog, dir: PathBuf) {
    state.current = dir;
    state.entries = read_entries(&state.current, &state.filters[state.filter_idx]);
    state.selected = None;
    state.filename.clear();
}

/// Look-in dropdown options: the current folder, then each ancestor up to root.
fn ancestors(dir: &Path) -> Vec<PathChoice> {
    dir.ancestors()
        .map(|p| PathChoice(p.to_path_buf()))
        .collect()
}

/// The places-bar destinations that exist, as (label, freedesktop-icon, path).
fn places() -> Vec<(&'static str, &'static str, PathBuf)> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"));
    let candidates = [
        ("History", "document-open-recent", home.clone()),
        ("Desktop", "user-desktop", home.join("Desktop")),
        ("My Documents", "folder-documents", home.join("Documents")),
        ("My Computer", "computer", PathBuf::from("/")),
        (
            "My Network Places",
            "network-workgroup",
            PathBuf::from("/run/media"),
        ),
    ];
    candidates
        .into_iter()
        .map(|(l, i, p)| {
            let p = if p.is_dir() { p } else { home.clone() };
            (l, i, p)
        })
        .collect()
}

fn unique_new_folder(dir: &Path) -> PathBuf {
    let base = dir.join("New Folder");
    if !base.exists() {
        return base;
    }
    for n in 2..1000 {
        let p = dir.join(format!("New Folder ({n})"));
        if !p.exists() {
            return p;
        }
    }
    base
}

/// Finish: print the chosen path and exit (the contract with the caller).
fn accept_path(p: &Path) -> ! {
    println!("{}", p.display());
    std::process::exit(0)
}

// --- update ----------------------------------------------------------------

fn update(state: &mut FileDialog, message: Message) -> Task<Message> {
    match message {
        Message::LookIn(PathChoice(p)) => navigate(state, p),
        Message::Place(i) => {
            if let Some((_, _, p)) = places().into_iter().nth(i) {
                navigate(state, p);
            }
        }
        Message::Up => {
            if let Some(parent) = state.current.parent().map(Path::to_path_buf) {
                navigate(state, parent);
            }
        }
        Message::NewFolder => {
            let p = unique_new_folder(&state.current);
            if std::fs::create_dir(&p).is_ok() {
                state.entries = read_entries(&state.current, &state.filters[state.filter_idx]);
                state.selected = state.entries.iter().position(|e| e.path == p);
            }
        }
        Message::ToggleView => state.details = !state.details,
        Message::ClickEntry(i) => {
            let now = Instant::now();
            let dbl = state
                .last_click
                .map(|(li, lt)| li == i && now.duration_since(lt) < Duration::from_millis(400))
                .unwrap_or(false);
            state.selected = Some(i);
            if let Some(e) = state.entries.get(i) {
                state.filename = e.name.clone();
                if dbl {
                    if e.is_dir {
                        let path = e.path.clone();
                        navigate(state, path);
                        return Task::none();
                    } else {
                        accept_path(&e.path);
                    }
                }
            }
            state.last_click = Some((i, now));
        }
        Message::FilenameChanged(s) => state.filename = s,
        Message::SetFilter(FilterChoice { idx, .. }) => {
            state.filter_idx = idx.min(state.filters.len().saturating_sub(1));
            state.entries = read_entries(&state.current, &state.filters[state.filter_idx]);
            state.selected = None;
        }
        Message::Accept => {
            let name = state.filename.trim();
            let target = if name.is_empty() {
                state
                    .selected
                    .and_then(|i| state.entries.get(i))
                    .map(|e| e.path.clone())
            } else if Path::new(name).is_absolute() {
                Some(PathBuf::from(name))
            } else {
                Some(state.current.join(name))
            };
            if let Some(t) = target {
                if t.is_dir() {
                    navigate(state, t); // a folder: drill in rather than choose
                } else {
                    accept_path(&t); // a file (existing, or to-create on Save)
                }
            }
        }
        Message::Cancel => std::process::exit(1),
    }
    Task::none()
}

// --- view -------------------------------------------------------------------

fn pad(t: f32, r: f32, b: f32, l: f32) -> Padding {
    Padding {
        top: t,
        right: r,
        bottom: b,
        left: l,
    }
}

fn row_style(
    selected: bool,
) -> impl Fn(&iced::Theme, iced::widget::button::Status) -> iced::widget::button::Style {
    move |_t, status| {
        let hot = selected
            || matches!(
                status,
                iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed
            );
        iced::widget::button::Style {
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

fn human_size(bytes: u64) -> String {
    let kb = bytes.div_ceil(1024);
    format!("{kb} KB")
}

fn toolbar() -> Element<'static, Message> {
    let btn = |icons: &'static [&'static str], msg: Message| {
        button(crate::icons::icon_any(icons, 16))
            .on_press(msg)
            .padding(pad(2.0, 4.0, 2.0, 4.0))
    };
    Row::new()
        .spacing(2.0)
        .push(btn(&["go-up", "up"], Message::Up))
        .push(btn(
            &["folder-new", "folder-new-symbolic"],
            Message::NewFolder,
        ))
        .push(btn(
            &["view-list-details", "view-list", "view-more"],
            Message::ToggleView,
        ))
        .into()
}

fn places_bar() -> Element<'static, Message> {
    let mut col = Column::new().spacing(2.0).padding(4.0);
    for (i, (label, icon, _)) in places().into_iter().enumerate() {
        col = col.push(
            iced::widget::button(
                Column::new()
                    .align_x(iced::Alignment::Center)
                    .spacing(2.0)
                    .push(crate::icons::icon_any(&[icon], 24))
                    .push(text(label.to_string()).size(metrics::UI_PX - 1.0)),
            )
            .on_press(Message::Place(i))
            .width(Length::Fill)
            .padding(pad(4.0, 2.0, 4.0, 2.0))
            .style(row_style(false)),
        );
    }
    // The navy/sunken places strip down the left of the dialog.
    container(iced::widget::stack![
        frame::sunken().face(palette::color(palette::BUTTON_LIGHT)),
        col
    ])
    .width(Length::Fixed(88.0))
    .height(Length::Fill)
    .into()
}

fn entry_row<'a>(state: &FileDialog, i: usize, e: &'a Entry) -> Element<'a, Message> {
    let icon = if e.is_dir {
        &["folder"][..]
    } else if is_image(&e.name) {
        &["image-x-generic", "text-x-generic"][..]
    } else {
        &["text-x-generic", "application-x-executable"][..]
    };
    let mut row = Row::new()
        .spacing(5.0)
        .align_y(iced::Alignment::Center)
        .push(crate::icons::icon_any(icon, 16))
        .push(
            text(e.name.clone())
                .size(metrics::UI_PX)
                .width(Length::Fill),
        );
    if state.details {
        let meta = if e.is_dir {
            "File Folder".to_string()
        } else {
            human_size(e.size)
        };
        row = row.push(text(meta).size(metrics::UI_PX).width(Length::Fixed(90.0)));
    }
    iced::widget::button(row)
        .on_press(Message::ClickEntry(i))
        .width(Length::Fill)
        .padding(pad(1.0, 6.0, 1.0, 4.0))
        .style(row_style(state.selected == Some(i)))
        .into()
}

fn is_image(name: &str) -> bool {
    let ext = Path::new(name)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "bmp" | "webp" | "gif"
    )
}

fn view(state: &FileDialog) -> Element<'_, Message> {
    // Look in: dropdown + toolbar.
    let look_in = Row::new()
        .spacing(6.0)
        .align_y(iced::Alignment::Center)
        .push(text("Look in:").size(metrics::UI_PX))
        .push(
            iced::widget::pick_list(
                ancestors(&state.current),
                Some(PathChoice(state.current.clone())),
                Message::LookIn,
            )
            .style(mde_ui::sunken_picklist)
            .text_size(metrics::UI_PX)
            .width(Length::Fill),
        )
        .push(toolbar());

    // File list well.
    let mut list = Column::new().spacing(0.0);
    if state.entries.is_empty() {
        list = list
            .push(container(text("(empty)").size(metrics::UI_PX)).padding(pad(2.0, 4.0, 2.0, 4.0)));
    }
    for (i, e) in state.entries.iter().enumerate() {
        list = list.push(entry_row(state, i, e));
    }
    let well = container(iced::widget::stack![
        frame::sunken().face(palette::color(palette::WINDOW)),
        container(scrollable(list).style(mde_ui::scrollbar)).padding(2.0),
    ])
    .width(Length::Fill)
    .height(Length::Fill);

    let middle = Row::new().spacing(6.0).push(places_bar()).push(well);

    // Filename row + Open/Save button.
    let accept_label = if state.save { "Save" } else { "Open" };
    let name_row = Row::new()
        .spacing(6.0)
        .align_y(iced::Alignment::Center)
        .push(
            text("File name:")
                .size(metrics::UI_PX)
                .width(Length::Fixed(74.0)),
        )
        .push(
            text_input("", &state.filename)
                .on_input(Message::FilenameChanged)
                .on_submit(Message::Accept)
                .size(metrics::UI_PX)
                .style(mde_ui::sunken_field)
                .width(Length::Fill),
        )
        .push(
            button(text(accept_label).size(metrics::UI_PX))
                .on_press(Message::Accept)
                .width(Length::Fixed(80.0)),
        );

    // Files of type row + Cancel button.
    let cur_filter = FilterChoice {
        idx: state.filter_idx,
        label: state.filters[state.filter_idx].label.clone(),
    };
    let filter_opts: Vec<FilterChoice> = state
        .filters
        .iter()
        .enumerate()
        .map(|(i, f)| FilterChoice {
            idx: i,
            label: f.label.clone(),
        })
        .collect();
    let type_row = Row::new()
        .spacing(6.0)
        .align_y(iced::Alignment::Center)
        .push(
            text("Files of type:")
                .size(metrics::UI_PX)
                .width(Length::Fixed(74.0)),
        )
        .push(
            iced::widget::pick_list(filter_opts, Some(cur_filter), Message::SetFilter)
                .style(mde_ui::sunken_picklist)
                .text_size(metrics::UI_PX)
                .width(Length::Fill),
        )
        .push(
            button(text("Cancel").size(metrics::UI_PX))
                .on_press(Message::Cancel)
                .width(Length::Fixed(80.0)),
        );

    let body = Column::new()
        .spacing(8.0)
        .padding(10.0)
        .push(look_in)
        .push(container(middle).height(Length::Fill))
        .push(name_row)
        .push(type_row);

    container(iced::widget::stack![frame::raised(), body])
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
