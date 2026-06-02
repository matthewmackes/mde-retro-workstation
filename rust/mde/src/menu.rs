//! Start menu — authentic Windows 2000 classic column with cascading submenus.
//!
//! A full-screen transparent layer-shell surface: a click anywhere outside the
//! menu (the overlay) closes it, as does Esc or launching an item. The menu
//! sits bottom-left above the taskbar. Submenus open on click and cascade to
//! the right (Programs ▶, Settings ▶, Search ▶, System Tools ▶).

use std::process::{exit, Command, ExitCode};

use iced::alignment::{Horizontal, Vertical};
use iced::widget::{button, container, mouse_area, scrollable, text, Column, Row, Space};
use iced::{
    event, keyboard, Background, Border, Color, Element, Event, Length, Padding, Shadow, Task,
};
use iced_layershell::build_pattern::{application, MainSettings};
use iced_layershell::reexport::{Anchor, KeyboardInteractivity};
use iced_layershell::settings::LayerShellSettings;
use iced_layershell::{to_layer_message, Appearance};

use mde_ui::{frame, metrics, palette};

use crate::{apps, fedora, start_common};

/// A node in the menu tree.
enum Node {
    Sep,
    Leaf(String, Act),
    Sub(String, Vec<Node>),
}

/// What a leaf does when activated.
#[derive(Clone)]
enum Act {
    Tool(usize),       // fedora tool index
    Cmd(String, bool), // shell command, run-in-terminal
    Mde(&'static str), // re-exec this binary with a subcommand
    Run,
    Help,
    LogOff,
    ShutDown,
}

struct Menu {
    root: Vec<Node>,
    /// Indices of the currently-open submenu chain (column 0 selects column 1…).
    open: Vec<usize>,
    /// Keyboard selection within the active (deepest-open) column. `None` until
    /// the first arrow key, matching Win2000 (no highlight until you navigate).
    cursor: Option<usize>,
    /// The (column, index) of the right-clicked item showing a context menu.
    context: Option<(usize, usize)>,
    /// "Show small icons in Start menu" — when false (the Win2000 default) the
    /// root column uses large 32px icons; submenus always use small icons.
    small_icons: bool,
    /// Names of currently-pinned items, so the right-click menu can offer Pin
    /// or Unpin appropriately.
    pinned: Vec<String>,
}

#[to_layer_message]
#[derive(Debug, Clone)]
enum Message {
    Click(usize, usize), // (column, index)
    RightClick(usize, usize),
    CtxOpen,
    CtxPin,
    CtxUnpin,
    CtxProperties,
    Close,
    Event(Event),
}

pub fn run(args: &[String]) -> ExitCode {
    // Headless pinned-items management (the GUI pin action is the right-click
    // context menu; this is the write path it — and scripts — drive).
    match args.first().map(String::as_str) {
        Some("--pin") => return pin(&args[1..]),
        Some("--unpin") => return unpin(&args[1..]),
        Some("--list-pinned") => return list_pinned(),
        _ => {}
    }
    // Singleton: if a Start menu is already open, this launch is a duplicate
    // (e.g. Ctrl+Esc while one is up, or a click during the first instance's
    // start-up). Exit quietly rather than stacking another full-screen overlay
    // — stacked overlays are what made the menu "take several clicks" to open.
    if !start_common::acquire_singleton("mde-menu") {
        return ExitCode::SUCCESS;
    }
    match launch() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mde menu: {e}");
            ExitCode::FAILURE
        }
    }
}

/// `mde menu --pin <Name> <command...>` — pin an item to the Start menu top.
fn pin(args: &[String]) -> ExitCode {
    if args.len() < 2 {
        eprintln!("usage: mde menu --pin <Name> <command...>");
        return ExitCode::from(2);
    }
    let name = args[0].clone();
    let command = args[1..].join(" ");
    let mut state = crate::state::load();
    if !state.pinned.iter().any(|p| p.name == name) {
        state.pinned.push(crate::state::PinnedItem {
            name,
            command,
            ..Default::default()
        });
        if let Err(e) = crate::state::save(&state) {
            eprintln!("mde menu: could not save pins: {e}");
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}

/// `mde menu --unpin <Name>` — remove a pinned item by name.
fn unpin(args: &[String]) -> ExitCode {
    let Some(name) = args.first() else {
        eprintln!("usage: mde menu --unpin <Name>");
        return ExitCode::from(2);
    };
    let mut state = crate::state::load();
    state.pinned.retain(|p| &p.name != name);
    if let Err(e) = crate::state::save(&state) {
        eprintln!("mde menu: could not save pins: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn list_pinned() -> ExitCode {
    for p in crate::state::load().pinned {
        println!("{}\t{}", p.name, p.command);
    }
    ExitCode::SUCCESS
}

fn launch() -> Result<(), iced_layershell::Error> {
    application(namespace, update, view)
        .style(style)
        .subscription(subscription)
        .font(mde_ui::font::REGULAR_BYTES)
        .font(mde_ui::font::BOLD_BYTES)
        .font(mde_ui::font::PLEX_REGULAR_BYTES)
        .font(mde_ui::font::PLEX_BOLD_BYTES)
        .default_font(mde_ui::font::ui())
        .settings(MainSettings {
            layer_settings: LayerShellSettings {
                // Full-screen overlay so clicks outside the menu close it.
                anchor: Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right,
                exclusive_zone: 0,
                // Exclusive (not OnDemand): a freshly-mapped OnDemand layer
                // surface on labwc receives pointer motion (hover) but its first
                // click is consumed to focus it rather than delivered — so items
                // never fire. Exclusive focuses the surface on map, so clicks
                // (and keyboard nav) work immediately. The menu is modal/transient.
                keyboard_interactivity: KeyboardInteractivity::Exclusive,
                ..Default::default()
            },
            ..Default::default()
        })
        .run_with(|| {
            let st = crate::state::load();
            let pinned = st.pinned.iter().map(|p| p.name.clone()).collect();
            (
                Menu {
                    root: build_root(),
                    open: Vec::new(),
                    cursor: None,
                    context: None,
                    small_icons: st.start_small_icons,
                    pinned,
                },
                Task::none(),
            )
        })
}

fn namespace(_: &Menu) -> String {
    "mde-menu".to_string()
}

fn style(_: &Menu, _: &iced::Theme) -> Appearance {
    Appearance {
        background_color: Color::TRANSPARENT,
        text_color: palette::color(palette::MENU_TEXT),
    }
}

fn subscription(_: &Menu) -> iced::Subscription<Message> {
    // The menu only acts on keyboard events (clicks go through the widget tree).
    // Subscribing to *all* events rebuilt the whole menu on every mouse motion;
    // filter to keyboard so motion over the menu doesn't churn update + view.
    event::listen_with(|event, _status, _window| match event {
        Event::Keyboard(_) => Some(Message::Event(event)),
        _ => None,
    })
}

// --- menu tree -------------------------------------------------------------

fn build_root() -> Vec<Node> {
    let mut root = Vec::new();
    // Pinned items sit at the very top, above their own separator (Win2000's
    // [Pinned items] band), loaded from ~/.config/mde/menu.json.
    let pinned = crate::state::load().pinned;
    if !pinned.is_empty() {
        for item in &pinned {
            root.push(Node::Leaf(
                item.name.clone(),
                Act::Cmd(item.command.clone(), false),
            ));
        }
        root.push(Node::Sep);
    }
    root.extend([
        // Quick-launch staples at the top of the menu, above MackesDE Update.
        Node::Leaf("File Explorer".into(), Act::Mde("files")),
        Node::Leaf(
            "Terminal (Terminator)".into(),
            Act::Cmd("terminator".into(), false),
        ),
        Node::Leaf("Firefox".into(), Act::Cmd("firefox".into(), false)),
        Node::Leaf(
            "MackesDE Update".into(),
            Act::Cmd("sudo dnf upgrade".into(), true),
        ),
        Node::Sep,
        Node::Sub("Programs".into(), programs_tree()),
        Node::Sub("Settings".into(), settings_tree()),
        Node::Sub("Search".into(), search_tree()),
        Node::Sub("System Tools".into(), system_tools_tree()),
        Node::Leaf("Help".into(), Act::Help),
        Node::Leaf("About MDE Retro Workstation".into(), Act::Mde("about")),
        Node::Leaf("Run...".into(), Act::Run),
        Node::Sep,
        Node::Leaf("Log Off...".into(), Act::LogOff),
        Node::Leaf("Shut Down...".into(), Act::ShutDown),
    ]);
    root
}

fn programs_tree() -> Vec<Node> {
    let mut nodes = Vec::new();
    for (folder, apps) in apps::programs() {
        let children = apps
            .into_iter()
            .map(|a| Node::Leaf(a.name, Act::Cmd(a.exec, a.terminal)))
            .collect();
        nodes.push(Node::Sub(folder, children));
    }
    if nodes.is_empty() {
        nodes.push(Node::Leaf("(no applications found)".into(), Act::Help));
    }
    nodes
}

fn system_tools_tree() -> Vec<Node> {
    fedora::categories()
        .into_iter()
        .map(|cat| {
            let children = fedora::TOOLS
                .iter()
                .enumerate()
                .filter(|(_, t)| t.category == cat)
                .map(|(i, t)| Node::Leaf(t.name.to_string(), Act::Tool(i)))
                .collect();
            Node::Sub(cat.to_string(), children)
        })
        .collect()
}

fn settings_tree() -> Vec<Node> {
    vec![
        Node::Leaf("Control Panel".into(), Act::Mde("control-panel")),
        Node::Leaf("Display".into(), Act::Mde("display")),
        Node::Leaf(
            "Network and Dial-up Connections".into(),
            Act::Cmd("nm-connection-editor".into(), false),
        ),
        Node::Leaf(
            "Printers".into(),
            Act::Cmd("system-config-printer".into(), false),
        ),
        Node::Leaf(
            "Taskbar & Start Menu".into(),
            Act::Mde("taskbar-properties"),
        ),
    ]
}

fn search_tree() -> Vec<Node> {
    vec![
        Node::Leaf("For Files or Folders...".into(), Act::Mde("files")),
        Node::Leaf(
            "On the Internet...".into(),
            Act::Cmd("xdg-open https://duckduckgo.com".into(), false),
        ),
    ]
}

// --- update ----------------------------------------------------------------

/// The visible columns for the current open-path: column 0 is the root, each
/// subsequent column is the opened submenu's children.
fn columns(menu: &Menu) -> Vec<&[Node]> {
    let mut cols: Vec<&[Node]> = vec![&menu.root];
    let mut cur: &[Node] = &menu.root;
    for &i in &menu.open {
        match cur.get(i) {
            Some(Node::Sub(_, children)) => {
                cols.push(children);
                cur = children;
            }
            _ => break,
        }
    }
    cols
}

/// Index of the active (deepest-open) column in [`columns`].
fn active_col(menu: &Menu) -> usize {
    menu.open.len()
}

/// Step the selection by `delta` within `nodes`, skipping separators and
/// wrapping. `from == None` lands on the first (delta>0) or last (delta<0) item.
fn step(nodes: &[Node], from: Option<usize>, delta: isize) -> Option<usize> {
    let len = nodes.len() as isize;
    if len == 0 {
        return None;
    }
    let mut i = match from {
        Some(i) => i as isize,
        None => {
            if delta > 0 {
                -1
            } else {
                len
            }
        }
    };
    for _ in 0..len {
        i = (i + delta).rem_euclid(len);
        if !matches!(nodes[i as usize], Node::Sep) {
            return Some(i as usize);
        }
    }
    None
}

/// Accelerator jump: the next item after `from` (wrapping) whose label starts
/// with `ch` (case-insensitive), skipping separators.
fn jump(nodes: &[Node], from: Option<usize>, ch: char) -> Option<usize> {
    let len = nodes.len();
    if len == 0 {
        return None;
    }
    let start = from.map(|i| i + 1).unwrap_or(0);
    for k in 0..len {
        let i = (start + k) % len;
        let label = match &nodes[i] {
            Node::Leaf(l, _) | Node::Sub(l, _) => l,
            Node::Sep => continue,
        };
        if label.chars().next().map(|c| c.to_ascii_lowercase()) == Some(ch) {
            return Some(i);
        }
    }
    None
}

/// Open the submenu the cursor is on (if it is a `Sub`), selecting its first item.
fn open_cursor_sub(menu: &mut Menu) {
    let is_sub = {
        let cols = columns(menu);
        menu.cursor
            .and_then(|c| cols[active_col(menu)].get(c))
            .map(|n| matches!(n, Node::Sub(_, _)))
            == Some(true)
    };
    if let (true, Some(c)) = (is_sub, menu.cursor) {
        menu.open.push(c);
        let first = {
            let cols = columns(menu);
            step(cols[active_col(menu)], None, 1)
        };
        menu.cursor = first;
    }
}

/// (display name, launch command) for a leaf, if it has a runnable command —
/// used by the right-click context menu's Pin / Properties.
fn node_meta(menu: &Menu, col: usize, idx: usize) -> Option<(String, String)> {
    let cols = columns(menu);
    match cols.get(col)?.get(idx)? {
        Node::Leaf(label, act) => command_for(act).map(|c| (label.clone(), c)),
        _ => None,
    }
}

/// Rebuild the root column after a pin change so it shows immediately; reset
/// navigation since the pinned band (and therefore indices) shifted.
fn refresh_pins(menu: &mut Menu) {
    menu.root = build_root();
    menu.open.clear();
    menu.cursor = None;
}

fn command_for(act: &Act) -> Option<String> {
    match act {
        Act::Cmd(c, _) => Some(c.clone()),
        Act::Mde(s) => Some(format!("mde {s}")),
        Act::Tool(i) => fedora::TOOLS.get(*i).map(|t| t.command.to_string()),
        _ => None,
    }
}

fn mde_self_args(sub: &str, args: &[String]) {
    if let Ok(exe) = std::env::current_exe() {
        let _ = Command::new(exe).arg(sub).args(args).spawn();
    }
}

fn update(menu: &mut Menu, message: Message) -> Task<Message> {
    match message {
        Message::Click(col, idx) => {
            menu.context = None;
            let node = columns(menu).get(col).and_then(|c| c.get(idx));
            match node {
                Some(Node::Sub(_, _)) => {
                    if menu.open.get(col) == Some(&idx) {
                        menu.open.truncate(col); // toggle closed
                    } else {
                        menu.open.truncate(col);
                        menu.open.push(idx);
                    }
                }
                Some(Node::Leaf(_, act)) => {
                    run_act(act);
                    exit(0);
                }
                _ => {}
            }
        }
        // A click on the backdrop closes the context menu first, else the menu.
        Message::Close => {
            if menu.context.take().is_none() {
                exit(0);
            }
        }
        Message::RightClick(col, idx) => menu.context = Some((col, idx)),
        Message::CtxOpen => {
            let act = menu.context.and_then(|(c, i)| {
                let cols = columns(menu);
                match cols.get(c).and_then(|col| col.get(i)) {
                    Some(Node::Leaf(_, a)) => Some(a.clone()),
                    _ => None,
                }
            });
            if let Some(a) = act {
                run_act(&a);
                exit(0);
            }
            menu.context = None;
        }
        Message::CtxPin => {
            if let Some((c, i)) = menu.context {
                if let Some((name, command)) = node_meta(menu, c, i) {
                    let mut st = crate::state::load();
                    if !st.pinned.iter().any(|p| p.name == name) {
                        st.pinned.push(crate::state::PinnedItem {
                            name: name.clone(),
                            command,
                            ..Default::default()
                        });
                        let _ = crate::state::save(&st);
                        menu.pinned.push(name);
                        refresh_pins(menu);
                    }
                }
            }
            menu.context = None;
        }
        Message::CtxUnpin => {
            if let Some((c, i)) = menu.context {
                if let Some((name, _)) = node_meta(menu, c, i) {
                    let mut st = crate::state::load();
                    st.pinned.retain(|p| p.name != name);
                    let _ = crate::state::save(&st);
                    menu.pinned.retain(|n| n != &name);
                    refresh_pins(menu);
                }
            }
            menu.context = None;
        }
        Message::CtxProperties => {
            if let Some((c, i)) = menu.context {
                if let Some((name, command)) = node_meta(menu, c, i) {
                    mde_self_args("properties", &[name, command]);
                }
            }
            exit(0);
        }
        Message::Event(Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(named),
            ..
        })) => {
            use keyboard::key::Named as N;
            match named {
                // Esc: back out one submenu level, or close at the root.
                N::Escape => {
                    if menu.open.pop().is_none() {
                        exit(0);
                    }
                    menu.cursor = None;
                }
                N::ArrowDown | N::ArrowUp => {
                    let delta = if named == N::ArrowDown { 1 } else { -1 };
                    let next = {
                        let cols = columns(menu);
                        step(cols[active_col(menu)], menu.cursor, delta)
                    };
                    menu.cursor = next;
                }
                // Right opens the highlighted submenu.
                N::ArrowRight => open_cursor_sub(menu),
                // Left collapses the current column, reselecting its parent item.
                N::ArrowLeft => {
                    if let Some(parent) = menu.open.pop() {
                        menu.cursor = Some(parent);
                    }
                }
                N::Enter => {
                    let act = {
                        let cols = columns(menu);
                        menu.cursor
                            .and_then(|c| cols[active_col(menu)].get(c))
                            .and_then(|n| match n {
                                Node::Leaf(_, a) => Some(a.clone()),
                                _ => None,
                            })
                    };
                    match act {
                        Some(a) => {
                            run_act(&a);
                            exit(0);
                        }
                        None => open_cursor_sub(menu), // Enter on a submenu opens it
                    }
                }
                _ => {}
            }
        }
        // Accelerator letters: jump to the next item starting with the key.
        Message::Event(Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Character(s),
            ..
        })) => {
            if let Some(ch) = s.chars().next().map(|c| c.to_ascii_lowercase()) {
                let next = {
                    let cols = columns(menu);
                    jump(cols[active_col(menu)], menu.cursor, ch)
                };
                if next.is_some() {
                    menu.cursor = next;
                }
            }
        }
        _ => {}
    }
    Task::none()
}

fn run_act(act: &Act) {
    match act {
        Act::Tool(i) => {
            if let Some(t) = fedora::TOOLS.get(*i) {
                let _ = fedora::launch(t);
            }
        }
        Act::Cmd(cmd, terminal) => start_common::launch_cmd(cmd, *terminal),
        Act::Mde(sub) => start_common::mde_self(sub),
        Act::Run => start_common::mde_self("run"),
        Act::Help => start_common::launch_cmd(
            "echo 'MDE-Retro — Start=Super  Run=Super+R  Close=Alt+F4  Switch=Alt+Tab  My Computer=Super+E'; read -p 'Press Enter to close '",
            true,
        ),
        Act::LogOff => start_common::mde_self("logoff"),
        Act::ShutDown => start_common::mde_self("shutdown"),
    }
}

// --- view ------------------------------------------------------------------

fn pad(top: f32, right: f32, bottom: f32, left: f32) -> Padding {
    Padding {
        top,
        right,
        bottom,
        left,
    }
}

fn item_style(selected: bool) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_t, status| {
        let hot = selected || matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: hot.then(|| Background::Color(palette::color(palette::HIGHLIGHT))),
            text_color: if hot {
                palette::color(palette::HIGHLIGHT_TEXT)
            } else {
                palette::color(palette::MENU_TEXT)
            },
            border: Border::default(),
            shadow: Shadow::default(),
        }
    }
}

// The Start menu uses taller rows than a menu *bar* (SM_CYMENU, 18px): the
// classic Win2000 Start list gives each item room around its 16px icon. 18px
// crowded the icon edge-to-edge, so the list reads as one dense block.
const ITEM_H: f32 = 22.0;
/// Large-icon row height for the root column (32px icon + breathing room) —
/// the Win2000 default Start menu, taller than the small-icon list.
const ITEM_H_LARGE: f32 = 34.0;
const SEP_H: f32 = 7.0;
const MAX_COL_H: f32 = 680.0;

fn col_content_height(nodes: &[Node], row_h: f32) -> f32 {
    nodes
        .iter()
        .map(|n| if matches!(n, Node::Sep) { SEP_H } else { row_h })
        .sum()
}

/// Freedesktop icon-name candidates for a Start-menu entry, by label keyword.
/// The resolver falls back to blank space, so an unmatched app simply shows no
/// icon (rather than tofu). Well-known shell entries get their classic icon;
/// program entries fall back to a generic executable.
fn menu_icon(label: &str) -> &'static [&'static str] {
    let l = label.to_ascii_lowercase();
    let has = |k: &str| l.contains(k);
    if has("program") {
        &[
            "applications-other",
            "folder-applications",
            "applications-all",
        ]
    } else if has("document") {
        &["folder-documents", "folder"]
    } else if has("display") {
        &["preferences-desktop-display", "video-display"]
    } else if has("setting") || has("control panel") || has("taskbar") {
        &["preferences-system", "gnome-control-center"]
    } else if has("search") || has("files or folders") {
        &["system-search", "edit-find", "search"]
    } else if has("firefox") {
        &["firefox", "firefox-esr", "web-browser"]
    } else if has("on the internet") || has("internet") {
        &[
            "applications-internet",
            "web-browser",
            "internet-web-browser",
        ]
    } else if has("help") {
        &["help-browser", "system-help", "help-contents"]
    } else if has("run") {
        &["system-run", "gnome-run"]
    } else if has("log off") {
        &["system-log-out"]
    } else if has("shut down") {
        &["system-shutdown"]
    } else if has("update") {
        &["system-software-update"]
    } else if has("system tools") || has("administrative") {
        &["applications-system", "preferences-system"]
    } else if has("network") || has("dial-up") {
        &["network-workgroup", "network-wired"]
    } else if has("printer") {
        &["printer"]
    } else if has("terminal") || has("command prompt") {
        &["utilities-terminal", "terminal"]
    } else if has("file manager") || has("explorer") {
        &["system-file-manager", "folder"]
    } else {
        &["application-x-executable", "applications-other"]
    }
}

fn render_item<'a>(
    node: &'a Node,
    col: usize,
    idx: usize,
    selected: bool,
    icon_px: u16,
    row_h: f32,
) -> Element<'a, Message> {
    match node {
        // Etched (engraved) menu separator: a 1px shadow line over a 1px
        // highlight line — the Win2000 sunken edge, not a single gray rule.
        Node::Sep => {
            let line = |rgb| {
                container(Space::new(Length::Fill, Length::Fixed(1.0)))
                    .width(Length::Fill)
                    .style(move |_| container::Style {
                        background: Some(Background::Color(palette::color(rgb))),
                        ..container::Style::default()
                    })
            };
            container(
                Column::new()
                    .push(line(palette::BUTTON_SHADOW))
                    .push(line(palette::BUTTON_HILIGHT)),
            )
            .height(Length::Fixed(SEP_H))
            .padding(pad(3.0, 6.0, 3.0, 6.0))
            .into()
        }
        Node::Leaf(label, _) => mouse_area(
            button(
                Row::new()
                    .spacing(6.0)
                    .align_y(iced::Alignment::Center)
                    .push(crate::icons::icon_any(menu_icon(label), icon_px))
                    .push(text(label).size(metrics::UI_PX)),
            )
            .on_press(Message::Click(col, idx))
            .width(Length::Fill)
            .height(Length::Fixed(row_h))
            .padding(pad(0.0, 16.0, 0.0, 8.0))
            .style(item_style(selected)),
        )
        .on_right_press(Message::RightClick(col, idx))
        .into(),
        Node::Sub(label, _) => button(
            Row::new()
                .spacing(6.0)
                .align_y(iced::Alignment::Center)
                .push(crate::icons::icon_any(menu_icon(label), icon_px))
                .push(text(label).size(metrics::UI_PX).width(Length::Fill))
                .push(text(">").size(metrics::UI_PX)),
        )
        .on_press(Message::Click(col, idx))
        .width(Length::Fill)
        .height(Length::Fixed(row_h))
        .padding(pad(0.0, 8.0, 0.0, 8.0))
        .style(item_style(selected))
        .into(),
    }
}

fn item_list<'a>(
    nodes: &'a [Node],
    col: usize,
    open: Option<usize>,
    icon_px: u16,
    row_h: f32,
) -> Column<'a, Message> {
    let mut list = Column::new().spacing(0.0);
    for (idx, node) in nodes.iter().enumerate() {
        list = list.push(render_item(
            node,
            col,
            idx,
            open == Some(idx),
            icon_px,
            row_h,
        ));
    }
    list
}

/// Width of the Start-menu side banner.
const BANNER_W: f32 = 28.0;

fn banner<'a>(height: f32) -> Element<'a, Message> {
    // The classic Windows side banner (à la Windows Me): a black strip with a
    // blue glow at the foot and the product name rotated 90°, reading
    // bottom-to-top — "MDE Retro" white, "Workstation" light blue, bold italic.
    // Rendered as an SVG because iced has no text rotation but its svg widget
    // rasterises <text> with the system fonts (Droid Sans).
    let w = BANNER_W;
    let h = height.max(1.0);
    let ty = h - 10.0;
    // Fixed brand colors sourced from the palette so no raw hex lives here (§2.1);
    // `hex_fixed` deliberately bypasses the per-theme remap — a logo reads the
    // same in every era (the constants are `LOGO_*` in palette.rs).
    let glow = palette::hex_fixed(palette::LOGO_BANNER_GLOW);
    let fade = palette::hex_fixed(palette::LOGO_BANNER_GLOW_FADE);
    let bg = palette::hex_fixed(palette::LOGO_BANNER_BG);
    let name = palette::hex_fixed(palette::LOGO_TEXT);
    let accent = palette::hex_fixed(palette::LOGO_TEXT_ACCENT);
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">
<defs><linearGradient id="g" x1="0" y1="1" x2="0" y2="0">
<stop offset="0" stop-color="{glow}" stop-opacity="1"/>
<stop offset="0.22" stop-color="{fade}" stop-opacity="0"/>
</linearGradient></defs>
<rect width="{w}" height="{h}" fill="{bg}"/>
<rect width="{w}" height="{h}" fill="url(#g)"/>
<text transform="translate(20,{ty}) rotate(-90)" font-family="Droid Sans" font-style="italic" font-weight="bold" text-anchor="start">
<tspan font-size="15" fill="{name}">MDE Retro </tspan><tspan font-size="12" fill="{accent}">Workstation</tspan>
</text></svg>"##
    );
    iced::widget::svg(iced::widget::svg::Handle::from_memory(svg.into_bytes()))
        .width(Length::Fixed(w))
        .height(Length::Fixed(h))
        .into()
}

fn render_column<'a>(
    nodes: &'a [Node],
    col: usize,
    open: Option<usize>,
    width: f32,
) -> Element<'a, Message> {
    // Submenu columns always use the small-icon list.
    let h = (col_content_height(nodes, ITEM_H).min(MAX_COL_H)) + 4.0;
    let panel = iced::widget::stack![
        frame::raised().thickness(2),
        container(scrollable(item_list(nodes, col, open, 16, ITEM_H)).style(mde_ui::scrollbar))
            .padding(2.0),
    ];
    container(panel)
        .width(Length::Fixed(width))
        .height(Length::Fixed(h))
        .into()
}

fn render_root_column<'a>(
    nodes: &'a [Node],
    open: Option<usize>,
    large: bool,
) -> Element<'a, Message> {
    // The root column is the only one that grows large icons (the Win2000
    // default); "Show small icons in Start menu" collapses it back to 16px.
    let (icon_px, row_h, inner_w, total_w) = if large {
        (32, ITEM_H_LARGE, 200.0, 232.0)
    } else {
        (16, ITEM_H, 186.0, 218.0)
    };
    let h = (col_content_height(nodes, row_h).min(MAX_COL_H)) + 4.0;
    let inner = Row::new().push(banner(h)).push(
        container(scrollable(item_list(nodes, 0, open, icon_px, row_h)).style(mde_ui::scrollbar))
            .width(Length::Fixed(inner_w))
            .padding(2.0),
    );
    container(iced::widget::stack![frame::raised().thickness(2), inner])
        .width(Length::Fixed(total_w))
        .height(Length::Fixed(h))
        .into()
}

/// Which item to highlight in column `col`: the keyboard cursor in the active
/// column, otherwise the open-submenu item that leads to the next column.
fn highlight_for(menu: &Menu, col: usize) -> Option<usize> {
    if col == active_col(menu) {
        menu.cursor
    } else {
        menu.open.get(col).copied()
    }
}

fn view(menu: &Menu) -> Element<'_, Message> {
    if palette::is_carbon() {
        return view_carbon(menu);
    }
    let cols = columns(menu);
    let large = !menu.small_icons;
    let root_row_h = if large { ITEM_H_LARGE } else { ITEM_H };
    let root_w = if large { 232.0 } else { 218.0 };
    const SUB_W: f32 = 200.0;
    // Item lists start this far below a column's top (the 2px container pad over
    // the raised frame); used to line a submenu up with its parent item.
    const TOP_PAD: f32 = 2.0;

    // Per-column geometry. A submenu must open *attached to the parent item that
    // spawned it* — its top aligned with that item — rather than all columns
    // resting on the taskbar. `top[c]` is column c's top measured down from the
    // root column's top; `left[c]` is its x. The root column stays anchored to
    // the taskbar (bottom edge), and each column is then lifted by `bottom_y` so
    // its bottom lands at the right height.
    let row_h_of = |c: usize| if c == 0 { root_row_h } else { ITEM_H };
    let col_h = |c: usize| col_content_height(cols[c], row_h_of(c)).min(MAX_COL_H) + 4.0;
    let h0 = col_h(0);

    let mut top = vec![0.0f32; cols.len()];
    let mut left = vec![0.0f32; cols.len()];
    for c in 1..cols.len() {
        let parent_row_h = row_h_of(c - 1);
        let parent_idx = menu.open[c - 1];
        // y of the parent item's top within column c-1.
        let item_top: f32 = TOP_PAD
            + cols[c - 1][..parent_idx]
                .iter()
                .map(|n| {
                    if matches!(n, Node::Sep) {
                        SEP_H
                    } else {
                        parent_row_h
                    }
                })
                .sum::<f32>();
        top[c] = top[c - 1] + item_top;
        left[c] = left[c - 1] + if c - 1 == 0 { root_w } else { SUB_W };
    }

    // Behind everything: a full-screen click catcher that closes the menu.
    let mut layers = iced::widget::stack![
        mouse_area(Space::new(Length::Fill, Length::Fill)).on_press(Message::Close)
    ];
    // Each column placed on its own full-screen layer, bottom-left anchored, then
    // offset (left = x, bottom = lift above the taskbar) to attach to its parent.
    for c in 0..cols.len() {
        let elem = if c == 0 {
            render_root_column(cols[0], highlight_for(menu, 0), large)
        } else {
            render_column(cols[c], c, highlight_for(menu, c), SUB_W)
        };
        let bottom_y = (h0 - top[c] - col_h(c)).max(0.0);
        layers = layers.push(
            container(elem)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Horizontal::Left)
                .align_y(Vertical::Bottom)
                .padding(Padding {
                    top: 0.0,
                    right: 0.0,
                    bottom: bottom_y,
                    left: left[c],
                }),
        );
    }
    // A right-clicked launcher shows a context menu (Open / Pin / Properties)
    // anchored bottom-left above the taskbar. Exact cursor-following is a
    // grim-tuning refinement; the commands are wired.
    if let Some((c, i)) = menu.context {
        if let Some((name, _)) = node_meta(menu, c, i) {
            let pinned = menu.pinned.contains(&name);
            layers = layers.push(
                container(context_menu(pinned))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(Horizontal::Left)
                    .align_y(Vertical::Bottom)
                    .padding(pad(0.0, 0.0, 16.0, 226.0)),
            );
        }
    }
    layers.into()
}

/// The Carbon product-switcher Start menu: a flat grid of app tiles dropping
/// from the top-left ≡ button. Folder entries (Programs/Settings/…) drill into a
/// sub-grid with a Back tile; leaves launch. Reuses the cascade `open`/`Click`
/// plumbing — `open.len()` is the current depth and the deepest column's nodes
/// are the tiles. Right-click still raises the Open/Pin/Properties context menu.
fn view_carbon(menu: &Menu) -> Element<'_, Message> {
    const COLS: usize = 4;
    const TILE_W: f32 = 104.0;
    const TILE_H: f32 = 88.0;
    const BAR_H: f32 = 32.0; // mirrors panel::CARBON_BAR_H

    let cols = columns(menu);
    let depth = menu.open.len();
    let nodes = cols[depth];

    // Build the tile list: a Back tile when drilled in, then every non-separator
    // node (keeping its ORIGINAL index so Click() indexes the column correctly).
    let mut tiles: Vec<Element<Message>> = Vec::new();
    if depth > 0 {
        // Back = toggle the parent submenu closed (Click on the open parent).
        let parent_idx = menu.open[depth - 1];
        tiles.push(start_common::tile(
            crate::icons::icon_any(&["go-previous", "back", "arrow-left"], 32),
            "Back",
            Message::Click(depth - 1, parent_idx),
            None,
            104.0,
            88.0,
        ));
    }
    for (i, node) in nodes.iter().enumerate() {
        match node {
            Node::Sep => {}
            Node::Leaf(label, _) | Node::Sub(label, _) => {
                tiles.push(start_common::tile(
                    crate::icons::icon_any(menu_icon(label), 32),
                    label,
                    Message::Click(depth, i),
                    Some(Message::RightClick(depth, i)),
                    104.0,
                    88.0,
                ));
            }
        }
    }

    // Arrange tiles into rows of COLS.
    let mut grid = Column::new().spacing(2.0);
    let mut row = Row::new().spacing(2.0);
    let mut n = 0;
    for tile in tiles {
        row = row.push(tile);
        n += 1;
        if n % COLS == 0 {
            grid = grid.push(row);
            row = Row::new().spacing(2.0);
        }
    }
    if n % COLS != 0 {
        // Pad the last row so it stays left-aligned at the tile width.
        for _ in 0..(COLS - (n % COLS)) {
            row = row.push(Space::new(Length::Fixed(TILE_W), Length::Fixed(TILE_H)));
        }
        grid = grid.push(row);
    }

    // The flat panel: a Carbon layer surface, 1px border, 2px radius, soft
    // overlay shadow. Width fits COLS tiles; height caps and scrolls.
    let panel_w = COLS as f32 * TILE_W + (COLS as f32 - 1.0) * 2.0 + 16.0;
    let panel = container(scrollable(container(grid).padding(8.0)).style(mde_ui::scrollbar))
        .width(Length::Fixed(panel_w))
        .max_height(MAX_COL_H + 8.0)
        .style(|_| container::Style {
            background: Some(Background::Color(palette::color(palette::MENU))),
            border: Border {
                color: palette::color(palette::WINDOW_FRAME),
                width: 1.0,
                radius: 2.0.into(),
            },
            shadow: Shadow {
                color: Color {
                    a: 0.35,
                    ..Color::BLACK
                },
                offset: iced::Vector::new(0.0, 2.0),
                blur_radius: 12.0,
            },
            ..container::Style::default()
        });

    // Backdrop click-catcher closes the menu; the panel drops from the top-left.
    let mut layers = iced::widget::stack![
        mouse_area(Space::new(Length::Fill, Length::Fill)).on_press(Message::Close),
        container(panel)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Horizontal::Left)
            .align_y(Vertical::Top)
            .padding(Padding {
                top: BAR_H + 2.0,
                right: 0.0,
                bottom: 0.0,
                left: 4.0
            }),
    ];

    // Right-click context menu (Open / Pin / Properties), dropped near the top.
    if let Some((c, i)) = menu.context {
        if node_meta(menu, c, i).is_some() {
            let pinned = node_meta(menu, c, i)
                .map(|(n, _)| menu.pinned.contains(&n))
                .unwrap_or(false);
            layers = layers.push(
                container(context_menu(pinned))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(Horizontal::Left)
                    .align_y(Vertical::Top)
                    .padding(pad(BAR_H + 40.0, 0.0, 0.0, 40.0)),
            );
        }
    }
    layers.into()
}

/// The launcher right-click menu panel. `pinned` swaps Pin for Unpin. Sized to
/// its three rows — a Fill `frame::raised()` base would balloon it full-screen.
fn context_menu(pinned: bool) -> Element<'static, Message> {
    let item = |label: &'static str, msg: Message| {
        button(text(label).size(metrics::UI_PX))
            .on_press(msg)
            .width(Length::Fill)
            .height(Length::Fixed(ITEM_H))
            .padding(pad(4.0, 16.0, 0.0, 12.0))
            .style(item_style(false))
    };
    let (pin_label, pin_msg) = if pinned {
        ("Unpin from Start menu", Message::CtxUnpin)
    } else {
        ("Pin to Start menu", Message::CtxPin)
    };
    let col = Column::new()
        .push(item("Open", Message::CtxOpen))
        .push(item(pin_label, pin_msg))
        .push(item("Properties", Message::CtxProperties));
    container(iced::widget::stack![
        frame::raised().thickness(2),
        container(col).padding(2.0)
    ])
    .width(Length::Fixed(168.0))
    .height(Length::Fixed(3.0 * ITEM_H + 6.0))
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(s: &str) -> Node {
        Node::Leaf(s.into(), Act::Help)
    }

    #[test]
    fn accelerator_jump_finds_next_match_wrapping() {
        let nodes = vec![leaf("Apple"), Node::Sep, leaf("Banana"), leaf("Avocado")];
        assert_eq!(jump(&nodes, None, 'a'), Some(0)); // first 'a' = Apple
        assert_eq!(jump(&nodes, Some(0), 'a'), Some(3)); // next 'a' = Avocado
        assert_eq!(jump(&nodes, Some(3), 'a'), Some(0)); // wraps back to Apple
        assert_eq!(jump(&nodes, None, 'b'), Some(2)); // Banana
        assert_eq!(jump(&nodes, None, 'z'), None); // no match
    }

    #[test]
    fn step_skips_separators_and_wraps() {
        let nodes = vec![leaf("A"), Node::Sep, leaf("B")];
        assert_eq!(step(&nodes, None, 1), Some(0)); // first selectable
        assert_eq!(step(&nodes, Some(0), 1), Some(2)); // skips the separator
        assert_eq!(step(&nodes, Some(2), 1), Some(0)); // wraps
        assert_eq!(step(&nodes, Some(0), -1), Some(2)); // up wraps to last
    }
}
