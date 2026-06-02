//! File manager — an Explorer-style window (stock iced xdg toplevel; labwc draws
//! the title bar + frame via its themerc, per our window theming).
//!
//! Client area, top to bottom: menubar (File/Edit/View/Favorites/Tools/Help),
//! a raised toolbar (Back/Forward/Up/Refresh/Home), an editable Address bar,
//! the sunken details list (Name/Size/Type, navigates on click), and a status
//! bar ("N object(s)"). Directory reads use std::fs; files open via xdg-open.
//!
//! Under the Win10 era the classic chrome is replaced by a flat command bar and a
//! left navigation pane (Quick access / This PC) beside the active view — the
//! Quick access landing, the This PC pane, or the details list (E8.1–E8.4).

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

/// Which view the main area shows. `Folder` is the directory listing (every
/// era's default). Under Win10 the no-path landing is configurable
/// (`explorer_landing`): `QuickAccess` (the default — Frequent folders + Recent
/// files), `ThisPc` (the known user folders + mounted drives), `Network`
/// (mounted remote locations), or `CloudDevice` (paired KDE Connect devices).
/// All are reachable any time from the nav pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pane {
    Folder,
    QuickAccess,
    ThisPc,
    Network,
    CloudDevice,
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
    /// Win10 search-in-folder query: filters the details list live, case-insensitive
    /// (E8.11). Empty = no filter; cleared on a folder change.
    search: String,
    /// Which landing the main area renders (Win10 Quick access vs a folder).
    pane: Pane,
    /// Win10 Quick access user-pinned folders (persisted `explorer_pins`, E8.3),
    /// appended to the auto-pinned standard folders.
    pins: Vec<PathBuf>,
    /// The Quick access folder whose right-click Pin/Unpin menu is open, if any.
    qctx: Option<PathBuf>,
    /// The selected Cloud device id (E8.7), when the CloudDevice pane is active.
    cloud_device: Option<String>,
    /// The Cloud device whose right-click offline menu is open, if any (E8.10).
    cloud_ctx: Option<String>,
    /// Network pane SMB browse (E8.5a): the server typed into the browse box, the
    /// Disk shares `smbclient -L` last returned for it, and whether a browse is in
    /// flight (the button then reads "Browsing…").
    net_host: String,
    net_shares: Vec<String>,
    net_busy: bool,
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
    SearchChanged(String),
    GoAddress,
    TreeToggle(PathBuf),
    TreeNav(PathBuf),
    /// Open a Quick access entry: enter the folder, or hand a file to xdg-open.
    OpenPath(PathBuf),
    /// Right-click a Quick access folder row: open its Pin/Unpin menu.
    QuickContext(PathBuf),
    /// Toggle a folder's Quick access pin (`explorer_pins`) and persist it.
    TogglePin(PathBuf),
    /// Switch the Win10 nav pane to Quick access / This PC / Network.
    ShowQuick,
    ShowThisPc,
    ShowNetwork,
    /// Network pane SMB browse (E8.5a): edit the server box, run the browse, and
    /// receive the parsed Disk shares (or an error). `OpenShare` mounts a listed
    /// share's `smb://host/share` via the shared off-thread mount flow.
    NetHostChanged(String),
    NetBrowse,
    NetBrowsed(Result<Vec<String>, String>),
    OpenShare(String),
    /// Show the Cloud devices pane (the paired-device list).
    ShowCloud,
    /// Mount a paired device over sftp and browse it (E8.8); on failure it selects
    /// the device and shows the error on the Cloud pane. The actual mount runs
    /// off-thread (E8.8a) and reports back via [`Message::MountDone`].
    MountCloud(String),
    /// An off-thread `mde mount` finished (E8.8a). `Ok(path)` → navigate into it;
    /// `Err` → show the message (and, for a cloud mount, reselect the device on the
    /// Cloud pane). `cloud_id` is `Some` for a paired-device mount, `None` for an
    /// address-bar `smb://`/`sftp://` connect.
    MountDone {
        result: Result<PathBuf, String>,
        cloud_id: Option<String>,
    },
    /// Right-click a Cloud device: open its offline menu (E8.10).
    CloudContext(String),
    /// Copy a device's files down into its local mirror (E8.10).
    MakeOffline(String),
    /// Delete a device's local mirror (E8.10).
    FreeUp(String),
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
    let st = crate::state::load();
    // An explicit directory argument always opens that folder. With no argument,
    // Win10 lands on its configured landing (Quick access or This PC, per
    // `explorer_landing`); other eras open the home folder.
    let explicit = args.first().map(PathBuf::from).filter(|p| p.is_dir());
    let start = explicit
        .clone()
        .or_else(home)
        .unwrap_or_else(|| PathBuf::from("/"));
    let pane = if explicit.is_some() || !palette::is_windows10() {
        Pane::Folder
    } else if st.explorer_landing == "thispc" {
        Pane::ThisPc
    } else if st.explorer_landing == "network" {
        Pane::Network
    } else if st.explorer_landing == "cloud" {
        Pane::CloudDevice
    } else {
        Pane::QuickAccess
    };
    match launch(start, pane, st.explorer_pins) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mde files: {e}");
            ExitCode::FAILURE
        }
    }
}

fn launch(start: PathBuf, pane: Pane, pins: Vec<PathBuf>) -> iced::Result {
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
                search: String::new(),
                pane,
                pins,
                qctx: None,
                cloud_device: None,
                cloud_ctx: None,
                net_host: String::new(),
                net_shares: Vec::new(),
                net_busy: false,
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
        self.pane = Pane::Folder;
        self.search.clear();
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
                state.pane = Pane::Folder;
                state.search.clear();
                state.load();
            }
        }
        Message::Forward => {
            if state.hpos + 1 < state.history.len() {
                state.hpos += 1;
                state.cwd = state.history[state.hpos].clone();
                state.pane = Pane::Folder;
                state.search.clear();
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
        Message::SearchChanged(s) => state.search = s,
        Message::GoAddress => {
            let addr = state.address.trim().to_string();
            if addr.contains("://") {
                // A remote URI (smb://, sftp://…): mount via `mde mount` (E8.6)
                // off-thread (E8.8a — an unreachable peer can block ~15s) and, on
                // MountDone, navigate into the path it prints or surface its error.
                state.error = Some(format!("Connecting to {addr}…"));
                return mount_task(addr, None);
            } else {
                let p = PathBuf::from(&addr);
                if p.is_dir() {
                    state.navigate(p);
                } else {
                    state.error = Some(format!("Cannot find '{addr}'."));
                }
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
        Message::OpenPath(p) => {
            if p.is_dir() {
                state.navigate(p);
            } else if Command::new("xdg-open").arg(&p).spawn().is_err() {
                state.error = Some("Could not open this file.".to_string());
            }
        }
        Message::QuickContext(p) => {
            state.qctx = Some(p);
            state.ctx = None;
            state.open_menu = None;
        }
        Message::TogglePin(p) => {
            state.ctx = None;
            state.qctx = None;
            if let Some(pos) = state.pins.iter().position(|x| x == &p) {
                state.pins.remove(pos);
            } else {
                state.pins.push(p);
            }
            // Persist via a fresh load+save so we never clobber fields edited
            // elsewhere; save() is atomic (§2.6).
            let mut st = crate::state::load();
            st.explorer_pins = state.pins.clone();
            let _ = crate::state::save(&st);
        }
        Message::ShowQuick => state.pane = Pane::QuickAccess,
        Message::ShowThisPc => state.pane = Pane::ThisPc,
        Message::ShowNetwork => state.pane = Pane::Network,
        Message::NetHostChanged(s) => state.net_host = s,
        Message::NetBrowse => {
            let host = state.net_host.trim().to_string();
            if !host.is_empty() {
                state.net_busy = true;
                state.net_shares.clear();
                state.error = Some(format!("Browsing \\\\{host}…"));
                return browse_task(host);
            }
        }
        Message::NetBrowsed(result) => {
            state.net_busy = false;
            match result {
                Ok(shares) => {
                    state.error = if shares.is_empty() {
                        Some(format!("No shares found on '{}'.", state.net_host.trim()))
                    } else {
                        None
                    };
                    state.net_shares = shares;
                }
                Err(e) => {
                    state.net_shares.clear();
                    state.error = Some(e);
                }
            }
        }
        Message::OpenShare(uri) => {
            // A discovered share carries its full smb:// URI; mount it off-thread.
            state.error = Some(format!("Connecting to {uri}…"));
            return mount_task(uri, None);
        }
        Message::ShowCloud => {
            state.cloud_device = None;
            state.pane = Pane::CloudDevice;
        }
        Message::MountCloud(id) => match cloud_devices().into_iter().find(|d| d.id == id) {
            // The mount itself runs off-thread (E8.8a); MountDone navigates or, on
            // error, reselects the device on the Cloud pane via `cloud_id`.
            Some(d) if !d.address.is_empty() => {
                state.cloud_device = Some(id.clone());
                state.pane = Pane::CloudDevice;
                state.error = Some(format!("Connecting to '{}'…", d.name));
                return mount_task(format!("sftp://{}", d.address), Some(id));
            }
            Some(d) => {
                state.cloud_device = Some(id);
                state.pane = Pane::CloudDevice;
                state.error = Some(format!("No address configured for '{}'.", d.name));
            }
            None => {}
        },
        Message::MountDone { result, cloud_id } => match result {
            Ok(path) => state.navigate(path),
            Err(e) => {
                // Surface the failure; for a cloud mount, land back on the Cloud
                // pane with the device reselected (the in-flight handler set those,
                // but an address-bar connect that errored leaves the pane as-is).
                if let Some(id) = cloud_id {
                    state.cloud_device = Some(id);
                    state.pane = Pane::CloudDevice;
                }
                state.error = Some(e);
            }
        },
        Message::CloudContext(id) => {
            state.cloud_ctx = Some(id);
            state.ctx = None;
            state.qctx = None;
            state.open_menu = None;
        }
        Message::MakeOffline(id) => {
            state.cloud_ctx = None;
            let dev = cloud_devices().into_iter().find(|d| d.id == id);
            match (dev, cloud_mirror(&id)) {
                (Some(d), Some(mirror)) if !d.address.is_empty() => {
                    match mount_uri(&format!("sftp://{}", d.address)) {
                        Ok(src) => match copy_tree(&src, &mirror) {
                            Ok(n) => {
                                state.error = Some(format!(
                                    "'{}' is now available offline ({n} file(s)).",
                                    d.name
                                ))
                            }
                            Err(e) => {
                                state.error =
                                    Some(format!("Could not copy '{}' offline: {e}", d.name))
                            }
                        },
                        Err(e) => state.error = Some(e),
                    }
                }
                (Some(d), _) => {
                    state.error = Some(format!("No address configured for '{}'.", d.name))
                }
                _ => {}
            }
        }
        Message::FreeUp(id) => {
            state.cloud_ctx = None;
            if let Some(mirror) = cloud_mirror(&id) {
                match std::fs::remove_dir_all(&mirror) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => state.error = Some(format!("Could not free up space: {e}")),
                }
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
        Message::CloseCtx => {
            state.ctx = None;
            state.qctx = None;
            state.cloud_ctx = None;
        }
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

/// The right-click context menu for a list row. Folders also offer Pin/Unpin to
/// Quick access (E8.3), toggling by whether the path is in `pins`.
fn context_menu(state: &Files) -> Element<'static, Message> {
    let has_clip = state.clipboard.is_some();
    let mut items = vec![
        ("Open", Message::CtxOpen, true),
        ("", Message::Noop, false),
        ("Cut", Message::CtxCut, true),
        ("Copy", Message::CtxCopy, true),
        ("Paste", Message::CtxPaste, has_clip),
        ("", Message::Noop, false),
        ("Delete", Message::CtxDelete, true),
    ];
    if let Some(e) = state.ctx.and_then(|i| state.entries.get(i)) {
        if e.is_dir {
            let pinned = state.pins.iter().any(|x| x == &e.path);
            let label = if pinned {
                "Unpin from Quick access"
            } else {
                "Pin to Quick access"
            };
            items.push(("", Message::Noop, false));
            items.push((label, Message::TogglePin(e.path.clone()), true));
        }
    }
    items.push(("", Message::Noop, false));
    items.push(("Properties", Message::CtxProperties, true));
    command_menu(items)
}

/// The right-click menu for a Quick access folder row: Open + Pin/Unpin (E8.3).
fn quick_context_menu(state: &Files) -> Element<'static, Message> {
    let p = state.qctx.clone().unwrap_or_default();
    let pinned = state.pins.iter().any(|x| x == &p);
    let pin_label = if pinned {
        "Unpin from Quick access"
    } else {
        "Pin to Quick access"
    };
    command_menu(vec![
        ("Open", Message::OpenPath(p.clone()), true),
        ("", Message::Noop, false),
        (pin_label, Message::TogglePin(p), true),
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
    let mut row = Row::new()
        .spacing(2.0)
        .padding(2.0)
        .align_y(iced::Alignment::Center)
        .push(tool("New folder", Some(Message::NewFolder)))
        .push(tool("Cut", sel.then_some(Message::CtxCut)))
        .push(tool("Copy", sel.then_some(Message::CtxCopy)))
        .push(tool(
            "Paste",
            state.clipboard.as_ref().map(|_| Message::CtxPaste),
        ))
        .push(tool("Delete", sel.then_some(Message::CtxDelete)))
        .push(tool("Properties", sel.then_some(Message::CtxProperties)));
    // Win10 search-in-folder box, only while browsing a folder (E8.11).
    if state.pane == Pane::Folder {
        row = row.push(Space::with_width(Length::Fill)).push(
            text_input("Search", &state.search)
                .on_input(Message::SearchChanged)
                .size(metrics::UI_PX)
                .width(Length::Fixed(180.0))
                .style(mde_ui::sunken_field),
        );
    }
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

/// Win10 search-in-folder match: case-insensitive substring of the name. An empty
/// query matches everything. `q_lower` is the already-lowercased query (E8.11).
fn name_matches(name: &str, q_lower: &str) -> bool {
    q_lower.is_empty() || name.to_lowercase().contains(q_lower)
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

    let q = state.search.to_lowercase();
    let mut rows = Column::new().spacing(0.0);
    for (i, e) in state.entries.iter().enumerate() {
        // Win10 search-in-folder: hide rows whose name doesn't contain the query
        // (case-insensitive), keeping the original index so Open(i) still maps right.
        if !name_matches(&e.name, &q) {
            continue;
        }
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
        None if state.pane == Pane::QuickAccess => "Quick access".to_string(),
        None if state.pane == Pane::ThisPc => "This PC".to_string(),
        None if state.pane == Pane::Network => {
            let n = network_locations().len();
            let shares = state.net_shares.len();
            if shares > 0 {
                format!("Network — {shares} share(s) on {}", state.net_host.trim())
            } else if n == 0 {
                "Network — browse a server, or type smb://host/share in the Address bar".to_string()
            } else {
                format!("Network — {n} location(s)")
            }
        }
        None if state.pane == Pane::CloudDevice => {
            let n = cloud_devices().len();
            if n == 0 {
                "No paired devices".to_string()
            } else {
                format!("{n} paired device(s)")
            }
        }
        None if !state.search.is_empty() => {
            let q = state.search.to_lowercase();
            let m = state
                .entries
                .iter()
                .filter(|e| name_matches(&e.name, &q))
                .count();
            format!("{m} of {} object(s)", state.entries.len())
        }
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

// --- Win10 Quick access landing (E8.2) -------------------------------------

/// A standard XDG user folder under $HOME, included only if it exists.
fn user_dir(name: &str) -> Option<PathBuf> {
    let p = home()?.join(name);
    p.is_dir().then_some(p)
}

/// Quick access "Frequent folders": the standard user folders that exist.
/// (E8.3 appends the user's persisted pins.)
fn frequent_folders(pins: &[PathBuf]) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = ["Desktop", "Documents", "Downloads", "Pictures"]
        .into_iter()
        .filter_map(user_dir)
        .collect();
    // The user's persisted pins after the standard folders, skipping any that no
    // longer exist or already appear (E8.3).
    for p in pins {
        if p.is_dir() && !v.contains(p) {
            v.push(p.clone());
        }
    }
    v
}

/// Quick access "Recent files": a live newest-first scan (by mtime) of the
/// standard user folders + $HOME, one level deep, hidden files skipped, capped.
/// Real mtimes rather than the recently-used registry, so it reflects actual
/// file activity regardless of which app touched the file.
fn recent_files(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = roots.to_vec();
    roots.extend(home());
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    for dir in roots {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let Ok(md) = e.metadata() else { continue };
            if md.is_file() {
                if let Ok(mtime) = md.modified() {
                    files.push((mtime, e.path()));
                }
            }
        }
    }
    files.sort_by_key(|e| std::cmp::Reverse(e.0));
    files.truncate(12);
    files.into_iter().map(|(_, p)| p).collect()
}

/// A Quick access section header. The one larger size (INFO_TITLE_PX) is reserved
/// for the web-view band, so headers use the standard UI size in bold.
fn section_header(label: &str) -> Element<'static, Message> {
    container(
        text(label.to_string())
            .size(metrics::UI_PX)
            .font(mde_ui::font::ui_bold()),
    )
    .padding(pad(6.0, 0.0, 2.0, 2.0))
    .into()
}

/// One Quick access row: a shell icon, the entry name, and a greyed path hint.
/// Clicking opens the folder (navigate) or the file (xdg-open) via `OpenPath`.
fn quick_row(p: &Path, is_folder: bool) -> Element<'static, Message> {
    let name = p
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| p.display().to_string());
    let hint = p
        .parent()
        .map(|d| d.display().to_string())
        .unwrap_or_default();
    let icon = if is_folder {
        crate::icons::icon("folder", 16)
    } else {
        let e = Entry {
            name: name.clone(),
            path: p.to_path_buf(),
            is_dir: false,
            size: 0,
        };
        crate::icons::icon_any(icon_names(&e), 16)
    };
    button(
        Row::new()
            .spacing(6.0)
            .align_y(iced::Alignment::Center)
            .push(icon)
            .push(
                text(name)
                    .size(metrics::UI_PX)
                    .width(Length::FillPortion(5)),
            )
            .push(
                text(hint)
                    .size(metrics::UI_PX)
                    .width(Length::FillPortion(6))
                    .color(palette::color(palette::GRAY_TEXT)),
            ),
    )
    .on_press(Message::OpenPath(p.to_path_buf()))
    .width(Length::Fill)
    .padding(pad(2.0, 6.0, 2.0, 6.0))
    .style(row_style(false))
    .into()
}

/// The Win10 Quick access landing: a Frequent-folders section over a Recent-files
/// section, each row navigable. Real folders + a live mtime scan, no mockups.
fn quick_access(state: &Files) -> Element<'_, Message> {
    let folders = frequent_folders(&state.pins);
    let recents = recent_files(&folders);
    let mut col = Column::new().spacing(0.0);

    col = col.push(section_header("Frequent folders"));
    if folders.is_empty() {
        col = col.push(
            container(text("No standard folders found.").size(metrics::UI_PX))
                .padding(pad(2.0, 6.0, 2.0, 6.0)),
        );
    } else {
        for p in &folders {
            // Folder rows carry a right-click Pin/Unpin menu (E8.3).
            col = col.push(
                mouse_area(quick_row(p, true)).on_right_press(Message::QuickContext(p.clone())),
            );
        }
    }

    col = col.push(Space::with_height(Length::Fixed(10.0)));

    col = col.push(section_header("Recent files"));
    if recents.is_empty() {
        col = col.push(
            container(text("No recent files.").size(metrics::UI_PX))
                .padding(pad(2.0, 6.0, 2.0, 6.0)),
        );
    } else {
        for p in &recents {
            col = col.push(quick_row(p, false));
        }
    }

    iced::widget::stack![
        frame::sunken().face(palette::color(palette::WINDOW)),
        container(scrollable(col).style(mde_ui::scrollbar))
            .padding(pad(2.0, 8.0, 2.0, 8.0))
            .width(Length::Fill)
            .height(Length::Fill),
    ]
    .into()
}

// --- Win10 navigation pane + This PC (E8.4) --------------------------------

/// A mounted volume from /proc/mounts (real block devices only).
struct Drive {
    mount: PathBuf,
    dev: String,
    fstype: String,
    removable: bool,
}

/// Decode the octal escapes /proc/mounts uses for special chars in paths.
fn unescape_mount(s: &str) -> String {
    s.replace("\\040", " ")
        .replace("\\011", "\t")
        .replace("\\012", "\n")
        .replace("\\134", "\\")
}

/// Real mounted volumes from /proc/mounts: block-device mounts (source under
/// `/dev/`), de-duped by mountpoint. Pseudo filesystems (proc/sys/tmpfs/…) are
/// skipped. Unreadable/empty → the caller falls back to the filesystem root.
fn drives() -> Vec<Drive> {
    let txt = std::fs::read_to_string("/proc/mounts").unwrap_or_default();
    let mut v: Vec<Drive> = Vec::new();
    for line in txt.lines() {
        let mut f = line.split_whitespace();
        let (Some(dev), Some(mp), Some(fstype)) = (f.next(), f.next(), f.next()) else {
            continue;
        };
        if !dev.starts_with("/dev/") {
            continue;
        }
        let mp = unescape_mount(mp);
        let removable =
            mp.starts_with("/run/media") || mp.starts_with("/media") || mp.starts_with("/mnt");
        v.push(Drive {
            mount: PathBuf::from(mp),
            dev: dev.to_string(),
            fstype: fstype.to_string(),
            removable,
        });
    }
    v.sort_by(|a, b| a.mount.cmp(&b.mount));
    v.dedup_by(|a, b| a.mount == b.mount);
    v
}

/// A "Devices and drives" row: a drive icon, the volume label, and a device·fs
/// hint. Clicking navigates into the mountpoint.
fn drive_row(d: &Drive) -> Element<'static, Message> {
    let label = if d.mount == Path::new("/") {
        "Local Disk (/)".to_string()
    } else {
        let base = d
            .mount
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| d.mount.display().to_string());
        format!("{base} ({})", d.mount.display())
    };
    let hint = if d.fstype.is_empty() {
        d.dev.clone()
    } else {
        format!("{} \u{00b7} {}", d.dev, d.fstype)
    };
    let icon = if d.removable {
        crate::icons::icon_any(&["drive-removable-media", "drive-harddisk", "drive"], 16)
    } else {
        crate::icons::icon_any(&["drive-harddisk", "drive"], 16)
    };
    button(
        Row::new()
            .spacing(6.0)
            .align_y(iced::Alignment::Center)
            .push(icon)
            .push(
                text(label)
                    .size(metrics::UI_PX)
                    .width(Length::FillPortion(5)),
            )
            .push(
                text(hint)
                    .size(metrics::UI_PX)
                    .width(Length::FillPortion(6))
                    .color(palette::color(palette::GRAY_TEXT)),
            ),
    )
    .on_press(Message::OpenPath(d.mount.clone()))
    .width(Length::Fill)
    .padding(pad(2.0, 6.0, 2.0, 6.0))
    .style(row_style(false))
    .into()
}

/// The This PC pane: the known user folders over the real mounted drives (E8.4).
fn this_pc() -> Element<'static, Message> {
    let mut col = Column::new().spacing(0.0);

    col = col.push(section_header("Folders"));
    let folders: Vec<PathBuf> = [
        "Desktop",
        "Documents",
        "Downloads",
        "Music",
        "Pictures",
        "Videos",
    ]
    .into_iter()
    .filter_map(user_dir)
    .collect();
    if folders.is_empty() {
        col = col.push(
            container(text("No user folders found.").size(metrics::UI_PX))
                .padding(pad(2.0, 6.0, 2.0, 6.0)),
        );
    } else {
        for p in &folders {
            col = col.push(quick_row(p, true));
        }
    }

    col = col.push(Space::with_height(Length::Fixed(10.0)));

    col = col.push(section_header("Devices and drives"));
    let ds = drives();
    if ds.is_empty() {
        // Fallback: the filesystem root is always navigable.
        col = col.push(drive_row(&Drive {
            mount: PathBuf::from("/"),
            dev: "rootfs".to_string(),
            fstype: String::new(),
            removable: false,
        }));
    } else {
        for d in &ds {
            col = col.push(drive_row(d));
        }
    }

    iced::widget::stack![
        frame::sunken().face(palette::color(palette::WINDOW)),
        container(scrollable(col).style(mde_ui::scrollbar))
            .padding(pad(2.0, 8.0, 2.0, 8.0))
            .width(Length::Fill)
            .height(Length::Fill),
    ]
    .into()
}

/// `$XDG_RUNTIME_DIR/gvfs` — where GVfs exposes mounted remote locations as FUSE
/// dirs (the same set `gio mount -l` reports).
fn runtime_gvfs() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").map(|r| PathBuf::from(r).join("gvfs"))
}

/// Currently-mounted network/remote locations, as (label, path), sorted. Empty
/// when nothing is mounted (or there is no runtime gvfs dir) — never panics.
fn network_locations() -> Vec<(String, PathBuf)> {
    let Some(dir) = runtime_gvfs() else {
        return Vec::new();
    };
    let mut v: Vec<(String, PathBuf)> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .map(|p| (network_label(&p), p))
        .collect();
    v.sort_by_key(|x| x.0.to_lowercase());
    v
}

/// A friendly label for a GVfs mount dir, decoding the common encodings
/// (`smb-share:server=…,share=…` → "share on server"; `sftp:host=…` → "host
/// (SFTP)"), else the raw entry name.
fn network_label(p: &Path) -> String {
    let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
    let field = |key: &str| {
        name.split([',', ':'])
            .find_map(|kv| kv.strip_prefix(key))
            .filter(|v| !v.is_empty())
    };
    if let (Some(server), Some(share)) = (field("server="), field("share=")) {
        format!("{share} on {server}")
    } else if let Some(host) = field("host=") {
        // sftp:host=… / dav:host=… — show the host plus its scheme.
        let scheme = name.split(':').next().unwrap_or("");
        if scheme.is_empty() || scheme == name {
            host.to_string()
        } else {
            format!("{host} ({})", scheme.to_uppercase())
        }
    } else if !name.is_empty() {
        name.to_string()
    } else {
        p.display().to_string()
    }
}

/// One Network row: a remote-folder icon, the location label, and its local path.
fn network_row(label: &str, path: &Path) -> Element<'static, Message> {
    button(
        Row::new()
            .spacing(6.0)
            .align_y(iced::Alignment::Center)
            .push(crate::icons::icon_any(
                &["folder-remote", "network-server", "network-workgroup"],
                16,
            ))
            .push(
                text(label.to_string())
                    .size(metrics::UI_PX)
                    .width(Length::FillPortion(5)),
            )
            .push(
                text(path.display().to_string())
                    .size(metrics::UI_PX)
                    .width(Length::FillPortion(6))
                    .color(palette::color(palette::GRAY_TEXT)),
            ),
    )
    .on_press(Message::OpenPath(path.to_path_buf()))
    .width(Length::Fill)
    .padding(pad(2.0, 6.0, 2.0, 6.0))
    .style(row_style(false))
    .into()
}

/// One discovered SMB share row (E8.5a): the share name + its `\\host\share` UNC
/// label; clicking it mounts the carried `smb://host/share` URI off-thread.
fn share_row(host: &str, share: &str) -> Element<'static, Message> {
    let unc = format!("\\\\{host}\\{share}");
    let uri = format!("smb://{host}/{share}");
    button(
        Row::new()
            .spacing(6.0)
            .align_y(iced::Alignment::Center)
            .push(crate::icons::icon_any(
                &["folder-remote", "network-server", "network-workgroup"],
                16,
            ))
            .push(
                text(share.to_string())
                    .size(metrics::UI_PX)
                    .width(Length::FillPortion(5)),
            )
            .push(
                text(unc)
                    .size(metrics::UI_PX)
                    .width(Length::FillPortion(6))
                    .color(palette::color(palette::GRAY_TEXT)),
            ),
    )
    .on_press(Message::OpenShare(uri))
    .width(Length::Fill)
    .padding(pad(2.0, 6.0, 2.0, 6.0))
    .style(row_style(false))
    .into()
}

/// The Network pane: a "browse a server" box (E8.5a) that lists an SMB host's
/// shares via `smbclient -L`, above the already-mounted remote locations (E8.5).
/// Typing a server + Browse lists its shares as rows that mount on click; the
/// Address bar `smb://…` route still works too. Empty mounts → an empty-state line.
fn network(state: &Files) -> Element<'_, Message> {
    let mut col = Column::new().spacing(0.0);

    // Browse box: enter a server name/IP, list its shares (E8.5a).
    col = col.push(section_header("Browse a server"));
    let browse_label = if state.net_busy {
        "Browsing…"
    } else {
        "Browse"
    };
    col = col.push(
        container(
            Row::new()
                .spacing(6.0)
                .align_y(iced::Alignment::Center)
                .push(
                    text_input("Server name or IP (e.g. fileserver)", &state.net_host)
                        .on_input(Message::NetHostChanged)
                        .on_submit(Message::NetBrowse)
                        .size(metrics::UI_PX)
                        .width(Length::FillPortion(5)),
                )
                .push(
                    button(text(browse_label).size(metrics::UI_PX))
                        .on_press(Message::NetBrowse)
                        .padding(pad(2.0, 10.0, 2.0, 10.0))
                        .style(row_style(false)),
                ),
        )
        .padding(pad(4.0, 6.0, 4.0, 6.0)),
    );

    // The shares the last browse returned, if any.
    if !state.net_shares.is_empty() {
        col = col.push(section_header(&format!(
            "Shares on {}",
            state.net_host.trim()
        )));
        for share in &state.net_shares {
            col = col.push(share_row(state.net_host.trim(), share));
        }
    }

    // The already-mounted remote locations (E8.5).
    col = col.push(section_header("Network locations"));
    let locs = network_locations();
    if locs.is_empty() {
        col = col.push(
            container(
                text("No network locations are mounted. Browse a server above, or type an address such as smb://host/share in the Address bar.")
                    .size(metrics::UI_PX),
            )
            .padding(pad(2.0, 6.0, 2.0, 6.0)),
        );
    } else {
        for (label, path) in &locs {
            col = col.push(network_row(label, path));
        }
    }
    iced::widget::stack![
        frame::sunken().face(palette::color(palette::WINDOW)),
        container(scrollable(col).style(mde_ui::scrollbar))
            .padding(pad(2.0, 8.0, 2.0, 8.0))
            .width(Length::Fill)
            .height(Length::Fill),
    ]
    .into()
}

/// Run [`mount_uri`] off the UI thread (E8.8a) and deliver its result as a
/// [`Message::MountDone`]. `mde mount` shells out to `gio`/`sshfs` with bounded
/// connect timeouts, so an unreachable peer can take ~15s — running it on tokio's
/// blocking pool keeps the file manager responsive while it connects. `cloud_id`
/// rides through so MountDone knows whether this was a paired-device mount.
fn mount_task(uri: String, cloud_id: Option<String>) -> Task<Message> {
    Task::perform(
        async move {
            let result = tokio::task::spawn_blocking(move || mount_uri(&uri))
                .await
                .unwrap_or_else(|e| Err(format!("mount task failed: {e}")));
            (result, cloud_id)
        },
        |(result, cloud_id)| Message::MountDone { result, cloud_id },
    )
}

/// Browse an SMB server's shares off the UI thread (E8.5a): runs `smbclient -L`
/// on tokio's blocking pool and delivers the parsed Disk shares (or a clean
/// error) as [`Message::NetBrowsed`], so a slow/unreachable server never freezes
/// the file manager.
fn browse_task(host: String) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || smb_shares(&host))
                .await
                .unwrap_or_else(|e| Err(format!("browse task failed: {e}")))
        },
        Message::NetBrowsed,
    )
}

/// List an SMB server's Disk shares with `smbclient -L <host> -N` (guest, no
/// prompt), bounded by `timeout` so an unreachable server fails in seconds.
/// Returns the share names, or a readable error when nothing could be listed.
fn smb_shares(host: &str) -> Result<Vec<String>, String> {
    let out = Command::new("timeout")
        .args(["12", "smbclient", "-L", host, "-N"])
        .output()
        .map_err(|e| format!("could not run smbclient: {e}"))?;
    // smbclient may exit non-zero yet still print a guest-listable share table, so
    // always parse stdout first and only synthesize an error when it's empty.
    let shares = parse_smb_shares(&String::from_utf8_lossy(&out.stdout));
    if !shares.is_empty() {
        return Ok(shares);
    }
    if out.status.code() == Some(124) {
        return Err(format!("Browsing '{host}' timed out."));
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let line = stderr.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    if out.status.code() == Some(127) || line.contains("not found") {
        Err("smbclient is not installed.".to_string())
    } else if line.trim().is_empty() {
        Err(format!("No shares found on '{host}'."))
    } else {
        Err(line.trim().to_string())
    }
}

/// Extract the Disk share names from `smbclient -L` output. The table is:
/// ```text
///         Sharename       Type      Comment
///         ---------       ----      -------
///         public          Disk      Public files
///         IPC$            IPC       IPC Service
/// ```
/// Take the first column of each row whose Type is `Disk`, dropping the `IPC$` /
/// `print$` administrative shares. Stops at the blank line that ends the table.
fn parse_smb_shares(output: &str) -> Vec<String> {
    let mut shares = Vec::new();
    let mut in_table = false;
    for line in output.lines() {
        let t = line.trim();
        if t.starts_with("Sharename") {
            in_table = true;
            continue;
        }
        if !in_table {
            continue;
        }
        if t.is_empty() {
            break; // the share table ends at the first blank line
        }
        if t.starts_with("---") {
            continue;
        }
        let mut cols = t.split_whitespace();
        let (Some(name), Some(kind)) = (cols.next(), cols.next()) else {
            continue;
        };
        if kind == "Disk" && name != "IPC$" && name != "print$" {
            shares.push(name.to_string());
        }
    }
    shares
}

/// Mount a remote URI by spawning `mde mount <uri>` (E8.6) and returning the local
/// path it prints, or a clean error message (its stderr, de-prefixed).
fn mount_uri(uri: &str) -> Result<PathBuf, String> {
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_else(|| "mde".to_string());
    let out = Command::new(exe)
        .arg("mount")
        .arg(uri)
        .output()
        .map_err(|e| format!("could not run mde mount: {e}"))?;
    if out.status.success() {
        let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if path.is_empty() {
            Err(format!("Mounted '{uri}' but got no path back."))
        } else {
            Ok(PathBuf::from(path))
        }
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let err = err.strip_prefix("mde mount: ").unwrap_or(&err).to_string();
        Err(if err.is_empty() {
            format!("Could not mount '{uri}'.")
        } else {
            err
        })
    }
}

/// A paired KDE Connect device from mde's connect store (E8.7).
#[derive(serde::Deserialize)]
struct CloudDevice {
    id: String,
    name: String,
    #[serde(default)]
    paired: bool,
    /// sftp target (host or user@host) used by E8.8 to mount + browse the device.
    #[serde(default)]
    address: String,
}

/// The connect pairing store: `~/.config/mde/connect/devices.json` (honouring
/// `$XDG_CONFIG_HOME`) — the direct-read fallback for the shared KDE Connect crate.
fn connect_store() -> Option<PathBuf> {
    crate::state::config_path()?
        .parent()
        .map(|d| d.join("connect").join("devices.json"))
}

/// Paired cloud devices, read from the connect store. Missing/garbage → none
/// (never panics); only `paired` devices are returned.
fn cloud_devices() -> Vec<CloudDevice> {
    let Some(path) = connect_store() else {
        return Vec::new();
    };
    let Ok(txt) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let list: Vec<CloudDevice> = serde_json::from_str(&txt).unwrap_or_default();
    list.into_iter().filter(|d| d.paired).collect()
}

/// The per-device offline-mirror dir: `$XDG_CACHE_HOME/mde/cloud/<id>` (honouring
/// the var, else `~/.cache`). E8.10 copies files down here; its presence drives the
/// Status column (E8.9).
fn cloud_mirror(id: &str) -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(base.join("mde").join("cloud").join(id))
}

/// A device's offline Status (E8.9), computed from local-mirror presence:
/// "Available offline" once its mirror holds files, else "Online-only". (The
/// transient "Syncing" state arrives with the E8.10 copy action.)
fn cloud_status(id: &str) -> &'static str {
    let has_mirror = cloud_mirror(id)
        .and_then(|p| std::fs::read_dir(p).ok())
        .map(|mut rd| rd.next().is_some())
        .unwrap_or(false);
    if has_mirror {
        "Available offline"
    } else {
        "Online-only"
    }
}

/// Recursively copy `src` into `dst` (creating `dst`) via std::fs — the offline
/// mirror copy-down (E8.10). Returns the number of files copied.
fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<usize> {
    std::fs::create_dir_all(dst)?;
    let mut n = 0;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            n += copy_tree(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), &to)?;
            n += 1;
        }
    }
    Ok(n)
}

/// One Cloud-device row: a phone glyph, the device name, its sftp address, and its
/// offline Status (E8.9). Clicking it mounts the device over sftp and browses it
/// (E8.8); a connect error re-selects the row and shows the reason in the status bar.
fn cloud_row(d: &CloudDevice, selected: bool) -> Element<'static, Message> {
    button(
        Row::new()
            .spacing(6.0)
            .align_y(iced::Alignment::Center)
            .push(crate::icons::icon_any(
                &["phone", "smartphone", "folder-remote"],
                16,
            ))
            .push(
                text(d.name.clone())
                    .size(metrics::UI_PX)
                    .width(Length::FillPortion(4)),
            )
            .push(
                text(if d.address.is_empty() {
                    "Paired (no address)".to_string()
                } else {
                    d.address.clone()
                })
                .size(metrics::UI_PX)
                .width(Length::FillPortion(4))
                .color(palette::color(palette::GRAY_TEXT)),
            )
            .push(
                text(cloud_status(&d.id))
                    .size(metrics::UI_PX)
                    .width(Length::FillPortion(3))
                    .color(palette::color(palette::GRAY_TEXT)),
            ),
    )
    .on_press(Message::MountCloud(d.id.clone()))
    .width(Length::Fill)
    .padding(pad(2.0, 6.0, 2.0, 6.0))
    .style(row_style(selected))
    .into()
}

/// The Cloud devices pane: the paired KDE Connect devices (E8.7); clicking one
/// mounts it over sftp and browses it (E8.8), a failed device staying highlighted
/// with its error. Empty → an empty-state line (the status bar echoes the count).
fn cloud_pane(state: &Files) -> Element<'_, Message> {
    let devices = cloud_devices();
    let mut col = Column::new().spacing(0.0);
    col = col.push(section_header("Paired devices"));
    if devices.is_empty() {
        col = col.push(
            container(
                text("No paired devices. Pair a phone in Mobile Devices (Your Phone) to see it here.")
                    .size(metrics::UI_PX),
            )
            .padding(pad(2.0, 6.0, 2.0, 6.0)),
        );
    } else {
        for d in &devices {
            let sel = state.cloud_device.as_deref() == Some(d.id.as_str());
            col = col.push(
                mouse_area(cloud_row(d, sel)).on_right_press(Message::CloudContext(d.id.clone())),
            );
        }
    }
    iced::widget::stack![
        frame::sunken().face(palette::color(palette::WINDOW)),
        container(scrollable(col).style(mde_ui::scrollbar))
            .padding(pad(2.0, 8.0, 2.0, 8.0))
            .width(Length::Fill)
            .height(Length::Fill),
    ]
    .into()
}

/// The right-click menu for a Cloud device row: Open, plus Make-available-offline
/// or Free-up-space depending on its current Status (E8.10).
fn cloud_context_menu(state: &Files) -> Element<'static, Message> {
    let id = state.cloud_ctx.clone().unwrap_or_default();
    let mut items = vec![
        ("Open", Message::MountCloud(id.clone()), true),
        ("", Message::Noop, false),
    ];
    if cloud_status(&id) == "Available offline" {
        items.push(("Free up space", Message::FreeUp(id), true));
    } else {
        items.push(("Make available offline", Message::MakeOffline(id), true));
    }
    command_menu(items)
}

/// An indented cloud-device child node under "Cloud Files": phone glyph + name.
fn cloud_nav_child(name: String, id: String, active: bool) -> Element<'static, Message> {
    button(
        Row::new()
            .spacing(4.0)
            .align_y(iced::Alignment::Center)
            .push(Space::with_width(Length::Fixed(14.0)))
            .push(crate::icons::icon_any(
                &["phone", "smartphone", "folder-remote"],
                16,
            ))
            .push(text(name).size(metrics::UI_PX)),
    )
    .on_press(Message::MountCloud(id))
    .width(Length::Fill)
    .padding(pad(1.0, 6.0, 1.0, 6.0))
    .style(row_style(active))
    .into()
}

/// A Win10 nav-pane root node (Quick access / This PC): icon + bold label,
/// accent-filled when it's the active pane.
fn nav_node(
    label: &'static str,
    icons: &'static [&'static str],
    active: bool,
    msg: Message,
) -> Element<'static, Message> {
    button(
        Row::new()
            .spacing(6.0)
            .align_y(iced::Alignment::Center)
            .push(crate::icons::icon_any(icons, 16))
            .push(
                text(label)
                    .size(metrics::UI_PX)
                    .font(mde_ui::font::ui_bold()),
            ),
    )
    .on_press(msg)
    .width(Length::Fill)
    .padding(pad(3.0, 6.0, 3.0, 6.0))
    .style(row_style(active))
    .into()
}

/// An indented child row under "Quick access": a frequent/pinned folder.
fn nav_child(p: &Path) -> Element<'static, Message> {
    let name = p
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| p.display().to_string());
    button(
        Row::new()
            .spacing(4.0)
            .align_y(iced::Alignment::Center)
            .push(Space::with_width(Length::Fixed(14.0)))
            .push(crate::icons::icon("folder", 16))
            .push(text(name).size(metrics::UI_PX)),
    )
    .on_press(Message::OpenPath(p.to_path_buf()))
    .width(Length::Fill)
    .padding(pad(1.0, 6.0, 1.0, 6.0))
    .style(row_style(false))
    .into()
}

/// The Win10 Explorer navigation pane: a Quick access node (with its frequent
/// folders as children) and a This PC node, each switching the active pane.
fn nav_pane(state: &Files) -> Element<'_, Message> {
    let mut col = Column::new().spacing(0.0);
    col = col.push(nav_node(
        "Quick access",
        &["bookmarks", "user-bookmarks", "folder"],
        state.pane == Pane::QuickAccess,
        Message::ShowQuick,
    ));
    for p in frequent_folders(&state.pins) {
        col = col.push(nav_child(&p));
    }
    col = col.push(Space::with_height(Length::Fixed(6.0)));
    col = col.push(nav_node(
        "This PC",
        &["computer", "drive-harddisk"],
        state.pane == Pane::ThisPc,
        Message::ShowThisPc,
    ));
    col = col.push(nav_node(
        "Network",
        &["network-workgroup", "network-server", "folder-remote"],
        state.pane == Pane::Network,
        Message::ShowNetwork,
    ));
    // Cloud Files: the root node lists all paired devices; each paired device is a
    // child node, the selected one highlighted (E8.7).
    col = col.push(nav_node(
        "Cloud Files",
        &["folder-cloud", "phone", "folder-remote"],
        state.pane == Pane::CloudDevice && state.cloud_device.is_none(),
        Message::ShowCloud,
    ));
    for d in cloud_devices() {
        let sel = state.pane == Pane::CloudDevice && state.cloud_device.as_deref() == Some(&d.id);
        col = col.push(cloud_nav_child(d.name, d.id, sel));
    }
    iced::widget::stack![
        frame::sunken().face(palette::color(palette::WINDOW)),
        container(scrollable(col).style(mde_ui::scrollbar)).padding(2.0),
    ]
    .into()
}

/// The folder body: the left pane (web-view info band or folder tree) beside the
/// details list. Non-Win10 eras use this; Win10 renders its own nav pane + view.
fn folder_body(state: &Files) -> Element<'_, Message> {
    let left: Element<'_, Message> = if state.show_tree {
        tree_pane(state)
    } else {
        info_band(state)
    };
    Row::new()
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
        )
        .into()
}

fn view(state: &Files) -> Element<'_, Message> {
    // Win10 era: a left navigation pane (Quick access / This PC) beside the active
    // pane's content — Quick access landing, This PC, or the folder list (E8.4).
    // Other eras keep the Win2000 folder body (web-view/tree + details list).
    let main: Element<'_, Message> = if palette::is_windows10() {
        let pane_content: Element<'_, Message> = match state.pane {
            Pane::QuickAccess => quick_access(state),
            Pane::ThisPc => this_pc(),
            Pane::Network => network(state),
            Pane::CloudDevice => cloud_pane(state),
            Pane::Folder => list(state),
        };
        Row::new()
            .push(
                container(nav_pane(state))
                    .width(Length::Fixed(180.0))
                    .height(Length::Fill)
                    .padding(pad(2.0, 1.0, 2.0, 2.0)),
            )
            .push(
                container(pane_content)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .padding(2.0),
            )
            .into()
    } else {
        folder_body(state)
    };
    let main = container(main).width(Length::Fill).height(Length::Fill);

    // Win10 era: a flat command bar replaces the Win2000 menubar+toolbar (E8.1).
    // Other eras render the classic chrome unchanged.
    let content = if palette::is_windows10() {
        Column::new()
            .push(command_bar(state))
            .push(address_bar(state))
            .push(main)
            .push(status_bar(state))
    } else {
        Column::new()
            .push(menubar(state))
            .push(toolbar(state))
            .push(address_bar(state))
            .push(main)
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
    if state.cloud_ctx.is_some() {
        let catcher = mouse_area(Space::new(Length::Fill, Length::Fill))
            .on_press(Message::CloseCtx)
            .on_right_press(Message::CloseCtx);
        let positioned = Column::new()
            .push(Space::with_height(Length::Fixed(state.cursor.y)))
            .push(
                Row::new()
                    .push(Space::with_width(Length::Fixed(state.cursor.x)))
                    .push(container(cloud_context_menu(state)).width(Length::Fixed(180.0))),
            );
        iced::widget::stack![base, catcher, positioned].into()
    } else if state.qctx.is_some() {
        let catcher = mouse_area(Space::new(Length::Fill, Length::Fill))
            .on_press(Message::CloseCtx)
            .on_right_press(Message::CloseCtx);
        let positioned = Column::new()
            .push(Space::with_height(Length::Fixed(state.cursor.y)))
            .push(
                Row::new()
                    .push(Space::with_width(Length::Fixed(state.cursor.x)))
                    .push(container(quick_context_menu(state)).width(Length::Fixed(170.0))),
            );
        iced::widget::stack![base, catcher, positioned].into()
    } else if state.ctx.is_some() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_smb_shares_takes_disk_shares_only() {
        // A representative `smbclient -L host -N` listing.
        let out = "\
Anonymous login successful

\tSharename       Type      Comment
\t---------       ----      -------
\tpublic          Disk      Public files
\tmedia           Disk      Movies & music
\tIPC$            IPC       IPC Service (server)
\tprint$          Disk      Printer Drivers

\tServer               Comment
\t---------            -------
\tFILESERVER           Samba
";
        let shares = parse_smb_shares(out);
        // Disk shares kept in order; IPC$/print$ admin shares dropped; the Server
        // section after the blank line is not mistaken for shares.
        assert_eq!(shares, vec!["public".to_string(), "media".to_string()]);
    }

    #[test]
    fn parse_smb_shares_handles_no_table() {
        assert!(parse_smb_shares("session setup failed: NT_STATUS_ACCESS_DENIED").is_empty());
        assert!(parse_smb_shares("").is_empty());
    }

    #[test]
    fn search_filter_is_case_insensitive_substring() {
        assert!(name_matches("Report.PDF", "report"));
        assert!(name_matches("Report.PDF", "pdf"));
        assert!(name_matches("Report.PDF", "")); // empty query matches all
        assert!(name_matches("budget.xlsx", "bud"));
        assert!(!name_matches("Report.PDF", "xyz"));
    }

    #[test]
    fn copy_tree_mirrors_a_directory() {
        let tmp = std::env::temp_dir().join(format!("mde-e810-{}", std::process::id()));
        let (src, dst) = (tmp.join("src"), tmp.join("dst"));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(src.join("sub")).unwrap();
        std::fs::write(src.join("a.txt"), b"a").unwrap();
        std::fs::write(src.join("sub/b.txt"), b"b").unwrap();
        let n = copy_tree(&src, &dst).unwrap();
        assert_eq!(n, 2);
        assert!(dst.join("a.txt").is_file());
        assert!(dst.join("sub/b.txt").is_file());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn network_label_decodes_gvfs_names() {
        assert_eq!(
            network_label(Path::new(
                "/run/user/1000/gvfs/smb-share:server=nas,share=media"
            )),
            "media on nas"
        );
        assert_eq!(
            network_label(Path::new(
                "/run/user/1000/gvfs/sftp:host=server.lan,user=me"
            )),
            "server.lan (SFTP)"
        );
        assert_eq!(
            network_label(Path::new("/run/user/1000/gvfs/dav:host=files.example")),
            "files.example (DAV)"
        );
        // Unknown encoding falls back to the raw entry name (never empty, no panic).
        assert_eq!(network_label(Path::new("/x/weird-mount")), "weird-mount");
    }
}
