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

use iced::widget::{
    button, container, mouse_area, scrollable, text, text_input, Column, Row, Space,
};
use iced::{event, Background, Border, Element, Event, Length, Padding, Point, Shadow, Task};

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
    /// Left pane mode: `false` = the web-view info band (Win2000 default),
    /// `true` = the folder tree. The "Folders" toolbar button toggles it.
    show_tree: bool,
    /// Last known cursor position (tracked so the right-click context menu can
    /// open at the pointer, the Win2000 way).
    cursor: Point,
    /// The entry index whose right-click context menu is open, if any.
    ctx: Option<usize>,
    /// Cut/copy clipboard: the source path and whether it was cut (move) vs
    /// copied. Paste acts on the current folder.
    clipboard: Option<(PathBuf, bool)>,
    /// Details-list sort: column (0 = Name, 1 = Size, 2 = Type) and direction.
    sort_col: usize,
    sort_desc: bool,
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
    ToggleFolders,
    HeaderClick(usize),
    ToggleMenu(usize),
    CloseMenu,
    NewFolder,
    CloseWindow,
    About,
    // Right-click context menu on a list row, and its actions.
    Event(Event),
    RowContext(usize),
    CloseCtx,
    CtxOpen,
    CtxCut,
    CtxCopy,
    CtxPaste,
    CtxDelete,
    CtxProperties,
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
        .subscription(|_: &Files| event::listen().map(Message::Event))
        .font(mde_ui::font::REGULAR_BYTES)
        .font(mde_ui::font::BOLD_BYTES)
        .font(mde_ui::font::PLEX_REGULAR_BYTES)
        .font(mde_ui::font::PLEX_BOLD_BYTES)
        .default_font(mde_ui::font::ui())
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
                show_tree: false,
                cursor: Point::ORIGIN,
                ctx: None,
                clipboard: None,
                sort_col: 0,
                sort_desc: false,
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
        self.entries = entries;
        self.sort_entries();
        self.address = self.cwd.display().to_string();
        self.selected = None;
        self.last_click = None;
    }

    /// Sort the current entries by the active column/direction. Folders always
    /// group before files (the Win2000 details view), and Name is the tiebreak.
    fn sort_entries(&mut self) {
        let (col, desc) = (self.sort_col, self.sort_desc);
        self.entries.sort_by(|a, b| {
            let primary = match col {
                1 => a.size.cmp(&b.size),
                2 => kind(a).to_lowercase().cmp(&kind(b).to_lowercase()),
                _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            };
            let primary = if desc { primary.reverse() } else { primary };
            // Folders first regardless of direction, then the column, then name.
            b.is_dir
                .cmp(&a.is_dir)
                .then(primary)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
    }

    fn navigate(&mut self, path: PathBuf) {
        self.cwd = path.clone();
        self.load();
        self.history.truncate(self.hpos + 1);
        self.history.push(path);
        self.hpos = self.history.len() - 1;
    }

    /// Open entry `i`: enter a folder, or hand a file to `xdg-open`.
    fn activate(&mut self, i: usize) {
        if let Some(entry) = self.entries.get(i) {
            if entry.is_dir {
                let p = entry.path.clone();
                self.navigate(p);
            } else if Command::new("xdg-open").arg(&entry.path).spawn().is_err() {
                self.error = Some("Could not open this file.".to_string());
            }
        }
    }

    /// Move entry `i` to the trash (the Recycle Bin) via `gio trash`.
    fn trash(&mut self, i: usize) {
        if let Some(entry) = self.entries.get(i) {
            let (path, name) = (entry.path.clone(), entry.name.clone());
            match Command::new("gio").arg("trash").arg(&path).status() {
                Ok(s) if s.success() => self.load(),
                Ok(_) | Err(_) => self.error = Some(format!("Could not delete '{name}'.")),
            }
        }
    }

    /// Paste the clipboard entry into the current folder (move if it was cut,
    /// else copy). Folder *copy* is not yet supported (surfaced, not silent).
    fn paste(&mut self) {
        let Some((src, cut)) = self.clipboard.clone() else {
            return;
        };
        let Some(fname) = src.file_name() else { return };
        let dst = self.cwd.join(fname);
        if dst == src {
            return;
        }
        let result = if cut {
            std::fs::rename(&src, &dst).map(|_| ())
        } else if src.is_dir() {
            self.error = Some("Copying folders isn't supported yet.".to_string());
            return;
        } else {
            std::fs::copy(&src, &dst).map(|_| ())
        };
        match result {
            Ok(()) => {
                if cut {
                    self.clipboard = None;
                }
                self.load();
            }
            Err(e) => self.error = Some(format!("Paste failed: {e}")),
        }
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
                .map(|(li, lt)| {
                    li == i && now.duration_since(lt) < std::time::Duration::from_millis(400)
                })
                .unwrap_or(false);
            if is_double {
                state.last_click = None;
                state.activate(i);
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
            state.open_menu = if state.open_menu == Some(i) {
                None
            } else {
                Some(i)
            };
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
        Message::ToggleFolders => state.show_tree = !state.show_tree,
        Message::HeaderClick(col) => {
            if state.sort_col == col {
                state.sort_desc = !state.sort_desc;
            } else {
                state.sort_col = col;
                state.sort_desc = false;
            }
            let sel = state
                .selected
                .and_then(|i| state.entries.get(i))
                .map(|e| e.path.clone());
            state.sort_entries();
            // Keep the selection on the same item after re-sorting.
            state.selected = sel.and_then(|p| state.entries.iter().position(|e| e.path == p));
        }
        // Track the pointer so the context menu opens at the cursor.
        Message::Event(Event::Mouse(iced::mouse::Event::CursorMoved { position })) => {
            state.cursor = position;
        }
        Message::Event(Event::Keyboard(iced::keyboard::Event::KeyPressed { key, .. })) => {
            use iced::keyboard::key::Named;
            use iced::keyboard::Key;
            // Shell navigation keys, active only when no menu/context is open.
            if state.ctx.is_none() && state.open_menu.is_none() {
                match key {
                    Key::Named(Named::Enter) => {
                        if let Some(i) = state.selected {
                            state.activate(i);
                        }
                    }
                    Key::Named(Named::Backspace) => {
                        if let Some(parent) = state.cwd.parent() {
                            let p = parent.to_path_buf();
                            state.navigate(p);
                        }
                    }
                    Key::Named(Named::Delete) => {
                        if let Some(i) = state.selected {
                            state.trash(i);
                        }
                    }
                    Key::Named(Named::F5) => state.load(),
                    Key::Named(Named::ArrowDown) => {
                        if !state.entries.is_empty() {
                            let n = state.entries.len();
                            state.selected = Some(state.selected.map_or(0, |i| (i + 1).min(n - 1)));
                        }
                    }
                    Key::Named(Named::ArrowUp) => {
                        if !state.entries.is_empty() {
                            state.selected =
                                Some(state.selected.map_or(0, |i| i.saturating_sub(1)));
                        }
                    }
                    Key::Named(Named::Escape) => state.selected = None,
                    _ => {}
                }
            }
        }
        Message::Event(_) => {}
        Message::RowContext(i) => {
            state.selected = Some(i);
            state.ctx = Some(i);
            state.open_menu = None;
        }
        Message::CloseCtx => state.ctx = None,
        Message::CtxOpen => {
            let target = state.ctx.or(state.selected);
            state.ctx = None;
            if let Some(i) = target {
                state.activate(i);
            }
        }
        Message::CtxCut => {
            let p = state
                .ctx
                .or(state.selected)
                .and_then(|i| state.entries.get(i))
                .map(|e| e.path.clone());
            if let Some(p) = p {
                state.clipboard = Some((p, true));
            }
            state.ctx = None;
        }
        Message::CtxCopy => {
            let p = state
                .ctx
                .or(state.selected)
                .and_then(|i| state.entries.get(i))
                .map(|e| e.path.clone());
            if let Some(p) = p {
                state.clipboard = Some((p, false));
            }
            state.ctx = None;
        }
        Message::CtxPaste => {
            state.ctx = None;
            state.paste();
        }
        Message::CtxDelete => {
            let target = state.ctx.or(state.selected);
            state.ctx = None;
            if let Some(i) = target {
                state.trash(i);
            }
        }
        Message::CtxProperties => {
            if let Some(e) = state
                .ctx
                .or(state.selected)
                .and_then(|i| state.entries.get(i))
            {
                let exe = std::env::current_exe()
                    .ok()
                    .and_then(|p| p.to_str().map(String::from))
                    .unwrap_or_else(|| "mde".to_string());
                let _ = Command::new(exe)
                    .arg("properties")
                    .arg(&e.name)
                    .arg(e.path.display().to_string())
                    .spawn();
            }
            state.ctx = None;
        }
        Message::Noop => {}
    }
    Task::none()
}

// --- styling helpers -------------------------------------------------------

fn pad(top: f32, right: f32, bottom: f32, left: f32) -> Padding {
    Padding {
        top,
        right,
        bottom,
        left,
    }
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

/// Candidate freedesktop icon names for an entry, by kind/extension (the icon
/// resolver tries each in turn and falls back to blank space if none resolve).
fn icon_names(e: &Entry) -> &'static [&'static str] {
    if e.is_dir {
        return &["folder"];
    }
    let ext = e
        .path
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "txt" | "md" | "log" | "rst" | "ini" | "conf" | "cfg" | "toml" | "yaml" | "yml" => {
            &["text-x-generic", "text-plain"]
        }
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "svg" | "webp" | "ico" => &["image-x-generic"],
        "mp3" | "flac" | "ogg" | "wav" | "m4a" => &["audio-x-generic"],
        "mp4" | "mkv" | "webm" | "avi" | "mov" => &["video-x-generic"],
        "pdf" => &["application-pdf", "text-x-generic"],
        "zip" | "gz" | "xz" | "bz2" | "tar" | "tgz" | "zst" | "7z" | "rpm" => &[
            "package-x-generic",
            "application-x-archive",
            "text-x-generic",
        ],
        "sh" | "bash" | "zsh" | "py" | "rs" | "c" | "cpp" | "h" | "js" | "ts" => {
            &["text-x-script", "text-x-generic"]
        }
        "html" | "htm" | "xml" | "json" => &["text-html", "text-x-generic"],
        "desktop" => &["application-x-executable", "text-x-generic"],
        _ => &["text-x-generic", "application-x-generic", "unknown"],
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
/// Edit reflects the live selection/clipboard so Cut/Copy/Paste light up.
fn menu_items(state: &Files, i: usize) -> Vec<(&'static str, Message, bool)> {
    let has_sel = state.selected.is_some();
    let has_clip = state.clipboard.is_some();
    match i {
        0 => vec![
            ("New Folder", Message::NewFolder, true),
            ("Close", Message::CloseWindow, true),
        ],
        1 => vec![
            ("Cut", Message::CtxCut, has_sel),
            ("Copy", Message::CtxCopy, has_sel),
            ("Paste", Message::CtxPaste, has_clip),
            ("Delete", Message::CtxDelete, has_sel),
            ("Properties", Message::CtxProperties, has_sel),
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

/// A raised dropdown panel over a list of (label, message, enabled) commands —
/// shared by the menubar dropdowns and the row context menu.
fn command_menu(items: Vec<(&'static str, Message, bool)>) -> Element<'static, Message> {
    // Bound the panel height to its content. Without this the `frame::raised()`
    // base (Fill height) stretches the dropdown down the whole screen.
    let h: f32 = items
        .iter()
        .map(|(l, _, _)| if l.is_empty() { 9.0 } else { 20.0 })
        .sum::<f32>()
        + 4.0;
    let mut col = Column::new().spacing(0.0);
    for (label, msg, enabled) in items {
        if label.is_empty() {
            // A separator entry.
            col = col.push(
                container(Space::new(Length::Fill, Length::Fixed(5.0)))
                    .padding(pad(2.0, 6.0, 2.0, 6.0)),
            );
            continue;
        }
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
    container(iced::widget::stack![
        frame::raised(),
        container(col).padding(2.0)
    ])
    .height(Length::Fixed(h))
    .into()
}

/// The dropdown panel for menubar menu `i`.
fn dropdown(state: &Files, i: usize) -> Element<'static, Message> {
    command_menu(menu_items(state, i))
}

/// The right-click context menu for a list row.
fn context_menu(state: &Files) -> Element<'static, Message> {
    let has_clip = state.clipboard.is_some();
    command_menu(vec![
        ("Open", Message::CtxOpen, true),
        ("", Message::Noop, false),
        ("Cut", Message::CtxCut, true),
        ("Copy", Message::CtxCopy, true),
        ("Paste", Message::CtxPaste, has_clip),
        ("", Message::Noop, false),
        ("Delete", Message::CtxDelete, true),
        ("", Message::Noop, false),
        ("Properties", Message::CtxProperties, true),
    ])
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
    // "Folders" stays pressed while the tree pane is showing — the Win2000
    // toggle that swaps the left pane between the web view and the tree.
    let folders = mde_ui::button(text("Folders").size(metrics::UI_PX))
        .padding(pad(2.0, 8.0, 2.0, 8.0))
        .active(state.show_tree)
        .on_press(Message::ToggleFolders);
    let row = Row::new()
        .spacing(2.0)
        .padding(2.0)
        .push(tool("Back", back.then_some(Message::Back)))
        .push(tool("Forward", fwd.then_some(Message::Forward)))
        .push(tool("Up", Some(Message::Up)))
        .push(folders)
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

/// The Windows 10 Explorer command bar (E8.1): a flat row of the file actions
/// that replaces the Win2000 menubar+toolbar under the Win10 theme. Selection-
/// dependent actions are disabled when nothing is selected; Paste needs a
/// clipboard. Each reuses the existing Ctx* messages (they target
/// `ctx.or(selected)`). No Rename — files.rs has no rename action yet.
fn command_bar(state: &Files) -> Element<'_, Message> {
    let sel = state.selected.is_some();
    let row = Row::new()
        .spacing(2.0)
        .padding(2.0)
        .push(tool("New folder", Some(Message::NewFolder)))
        .push(tool("Cut", sel.then_some(Message::CtxCut)))
        .push(tool("Copy", sel.then_some(Message::CtxCopy)))
        .push(tool(
            "Paste",
            state.clipboard.as_ref().map(|_| Message::CtxPaste),
        ))
        .push(tool("Delete", sel.then_some(Message::CtxDelete)))
        .push(tool("Properties", sel.then_some(Message::CtxProperties)));
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

fn header_cell<'a>(label: &'a str, width: Length, col: usize) -> Element<'a, Message> {
    // A column header is a raised 3D button (Win2000's list-view header) — and
    // clicking it sorts the list by that column.
    mde_ui::button(text(label).size(metrics::UI_PX))
        .on_press(Message::HeaderClick(col))
        .width(width)
        .padding(pad(1.0, 6.0, 1.0, 6.0))
        .into()
}

fn list(state: &Files) -> Element<'_, Message> {
    let name_w = Length::FillPortion(6);
    let size_w = Length::Fixed(90.0);
    let type_w = Length::Fixed(120.0);

    let header = Row::new()
        .height(Length::Fixed(18.0))
        .push(header_cell("Name", name_w, 0))
        .push(header_cell("Size", size_w, 1))
        .push(header_cell("Type", type_w, 2));

    let mut rows = Column::new().spacing(0.0);
    for (i, e) in state.entries.iter().enumerate() {
        // A 16px shell icon leads the name cell, like Win2000's list view.
        let name_cell = Row::new()
            .spacing(4.0)
            .align_y(iced::Alignment::Center)
            .push(crate::icons::icon_any(icon_names(e), 16))
            .push(text(e.name.clone()).size(metrics::UI_PX))
            .width(name_w);
        let row = Row::new()
            .push(name_cell)
            .push(
                text(if e.is_dir {
                    String::new()
                } else {
                    human(e.size)
                })
                .size(metrics::UI_PX)
                .width(size_w),
            )
            .push(text(kind(e)).size(metrics::UI_PX).width(type_w));
        rows = rows.push(
            mouse_area(
                button(row)
                    .on_press(Message::Open(i))
                    .padding(pad(1.0, 4.0, 1.0, 4.0))
                    .width(Length::Fill)
                    .style(row_style(state.selected == Some(i))),
            )
            .on_right_press(Message::RowContext(i)),
        );
    }

    let inner = Column::new().push(header).push(
        scrollable(rows)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(mde_ui::scrollbar),
    );

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
fn tree_rows(
    state: &Files,
    path: &Path,
    label: String,
    depth: u16,
) -> Vec<Element<'static, Message>> {
    let expanded = state.tree_expanded.contains(path);
    // "+"/"-" — the Win2000 tree control, and always-rendering (Droid Sans has
    // no ▶/▼ glyphs, which would show as tofu boxes).
    let marker = if expanded { "-" } else { "+" };
    let row = Row::new()
        .align_y(iced::Alignment::Center)
        .push(Space::with_width(Length::Fixed(depth as f32 * 12.0)))
        .push(
            button(text(marker).size(metrics::UI_PX))
                .on_press(Message::TreeToggle(path.to_path_buf()))
                .padding(pad(0.0, 3.0, 0.0, 3.0))
                .style(flat),
        )
        .push(crate::icons::icon("folder", 16))
        .push(
            button(
                Row::new()
                    .spacing(4.0)
                    .align_y(iced::Alignment::Center)
                    .push(text(label).size(metrics::UI_PX)),
            )
            .on_press(Message::TreeNav(path.to_path_buf()))
            .width(Length::Fill)
            .padding(pad(0.0, 4.0, 0.0, 4.0))
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

/// The display name of the current folder for the band title ("Home" at $HOME,
/// "Filesystem" at /, else the folder name).
fn band_title(state: &Files) -> String {
    if home().as_deref() == Some(state.cwd.as_path()) {
        "Home".to_string()
    } else if state.cwd == Path::new("/") {
        "Filesystem".to_string()
    } else {
        state
            .cwd
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| state.cwd.display().to_string())
    }
}

/// A "See also" hyperlink: blue, underlined-by-convention text that navigates.
fn see_also(label: &str, target: PathBuf) -> Element<'_, Message> {
    button(
        text(label.to_string())
            .size(metrics::UI_PX)
            .color(mde_ui::infoband::accent()),
    )
    .on_press(Message::TreeNav(target))
    .padding(pad(0.0, 0.0, 0.0, 0.0))
    .style(|_t, _s| iced::widget::button::Style {
        background: None,
        text_color: mde_ui::infoband::accent(),
        border: Border::default(),
        shadow: Shadow::default(),
    })
    .into()
}

/// The Win2000 "web view" info band: the folder title over a fading rule, the
/// description prompt, a yellow tip describing the selection (or the folder),
/// and the "See also" links. Shown in the left pane unless Folders is toggled.
fn info_band(state: &Files) -> Element<'_, Message> {
    // The tip text: the selected item's description, else the folder's own.
    let (prompt, tip_text) = match state.selected.and_then(|i| state.entries.get(i)) {
        Some(e) => (
            format!("{}:", e.name),
            if e.is_dir {
                format!("{} \u{2014} opens this folder.", kind(e))
            } else {
                format!("{} \u{2014} {}", kind(e), human(e.size))
            },
        ),
        None => (
            "Select an item to view its description.".to_string(),
            format!("Displays the files and folders in {}.", band_title(state)),
        ),
    };

    // A large shell icon watermarks the band title (the "My Computer" graphic
    // in the reference): home/filesystem/folder by what we're viewing.
    let band_icon: &[&str] = if band_title(state) == "Home" {
        &["user-home", "folder-home", "folder"]
    } else if band_title(state) == "Filesystem" {
        &["drive-harddisk", "computer", "folder"]
    } else {
        &["folder"]
    };
    let mut col = Column::new()
        .spacing(8.0)
        .push(
            Row::new()
                .spacing(8.0)
                .align_y(iced::Alignment::Center)
                .push(crate::icons::icon_any(band_icon, 32))
                .push(
                    text(band_title(state))
                        .size(metrics::INFO_TITLE_PX)
                        .font(mde_ui::font::ui_bold())
                        .color(mde_ui::infoband::accent()),
                ),
        )
        .push(container(Space::new(Length::Fill, Length::Fixed(2.0))).style(mde_ui::infoband::rule))
        .push(text(prompt).size(metrics::UI_PX))
        .push(
            container(text(tip_text).size(metrics::UI_PX))
                .style(mde_ui::infoband::tip)
                .padding(pad(4.0, 6.0, 4.0, 6.0))
                .width(Length::Fill),
        )
        .push(Space::with_height(Length::Fixed(6.0)))
        .push(text("See also:").size(metrics::UI_PX));

    if let Some(h) = home() {
        col = col.push(see_also("My Documents", h));
    }
    col = col.push(see_also("My Computer", PathBuf::from("/")));

    container(col)
        .style(mde_ui::infoband::band)
        .padding(pad(10.0, 10.0, 10.0, 10.0))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn view(state: &Files) -> Element<'_, Message> {
    let left: Element<'_, Message> = if state.show_tree {
        tree_pane(state)
    } else {
        info_band(state)
    };
    let body = Row::new()
        .push(
            container(left)
                .width(Length::Fixed(180.0))
                .height(Length::Fill)
                .padding(pad(2.0, 1.0, 2.0, 2.0)),
        )
        .push(
            container(list(state))
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(2.0),
        );

    // Win10 era: a flat command bar replaces the Win2000 menubar+toolbar (E8.1).
    // Other eras render the classic chrome unchanged.
    let body = container(body).width(Length::Fill).height(Length::Fill);
    let content = if palette::is_windows10() {
        Column::new()
            .push(command_bar(state))
            .push(address_bar(state))
            .push(body)
            .push(status_bar(state))
    } else {
        Column::new()
            .push(menubar(state))
            .push(toolbar(state))
            .push(address_bar(state))
            .push(body)
            .push(status_bar(state))
    };

    let base = container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(palette::color(palette::MENU))),
            ..container::Style::default()
        });

    // A right-click row context menu takes precedence; otherwise an open menubar
    // menu overlays its dropdown. Each adds a transparent full-window catcher so
    // a click (or right-click) anywhere else dismisses it.
    if state.ctx.is_some() {
        let catcher = mouse_area(Space::new(Length::Fill, Length::Fill))
            .on_press(Message::CloseCtx)
            .on_right_press(Message::CloseCtx);
        let positioned = Column::new()
            .push(Space::with_height(Length::Fixed(state.cursor.y)))
            .push(
                Row::new()
                    .push(Space::with_width(Length::Fixed(state.cursor.x)))
                    .push(container(context_menu(state)).width(Length::Fixed(150.0))),
            );
        iced::widget::stack![base, catcher, positioned].into()
    } else if let Some(i) = state.open_menu {
        let catcher =
            mouse_area(Space::new(Length::Fill, Length::Fill)).on_press(Message::CloseMenu);
        let positioned = Column::new()
            .push(Space::with_height(Length::Fixed(20.0)))
            .push(
                Row::new()
                    .push(Space::with_width(Length::Fixed(menu_x(i))))
                    .push(container(dropdown(state, i)).width(Length::Fixed(170.0))),
            );
        iced::widget::stack![base, catcher, positioned].into()
    } else {
        base.into()
    }
}
