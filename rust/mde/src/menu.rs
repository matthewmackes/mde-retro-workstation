//! Start menu — authentic Windows 2000 classic column with cascading submenus.
//!
//! A full-screen transparent layer-shell surface: a click anywhere outside the
//! menu (the overlay) closes it, as does Esc or launching an item. The menu
//! sits bottom-left above the taskbar. Submenus open on click and cascade to
//! the right (Programs ▶, Settings ▶, Search ▶, System Tools ▶).

use std::process::{exit, Command, ExitCode};

use iced::alignment::{Horizontal, Vertical};
use iced::widget::{button, container, mouse_area, scrollable, text, Column, Row, Space};
use iced::{event, keyboard, Background, Border, Color, Element, Event, Length, Padding, Shadow, Task};
use iced_layershell::build_pattern::{application, MainSettings};
use iced_layershell::reexport::{Anchor, KeyboardInteractivity};
use iced_layershell::settings::LayerShellSettings;
use iced_layershell::{to_layer_message, Appearance};

use mde_ui::{frame, metrics, palette};

use crate::{apps, fedora};

/// A node in the menu tree.
enum Node {
    Sep,
    Leaf(String, Act),
    Sub(String, Vec<Node>),
}

/// What a leaf does when activated.
#[derive(Clone)]
enum Act {
    Tool(usize),         // fedora tool index
    Cmd(String, bool),   // shell command, run-in-terminal
    Mde(&'static str),   // re-exec this binary with a subcommand
    Run,
    Help,
    LogOff,
    ShutDown,
}

struct Menu {
    root: Vec<Node>,
    /// Indices of the currently-open submenu chain (column 0 selects column 1…).
    open: Vec<usize>,
}

#[to_layer_message]
#[derive(Debug, Clone)]
enum Message {
    Click(usize, usize), // (column, index)
    Close,
    Event(Event),
}

pub fn run(_args: &[String]) -> ExitCode {
    match launch() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mde menu: {e}");
            ExitCode::FAILURE
        }
    }
}

fn launch() -> Result<(), iced_layershell::Error> {
    application(namespace, update, view)
        .style(style)
        .subscription(subscription)
        .font(mde_ui::font::REGULAR_BYTES)
        .font(mde_ui::font::BOLD_BYTES)
        .default_font(mde_ui::font::UI)
        .settings(MainSettings {
            layer_settings: LayerShellSettings {
                // Full-screen overlay so clicks outside the menu close it.
                anchor: Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right,
                exclusive_zone: 0,
                keyboard_interactivity: KeyboardInteractivity::OnDemand,
                ..Default::default()
            },
            ..Default::default()
        })
        .run_with(|| (Menu { root: build_root(), open: Vec::new() }, Task::none()))
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
    event::listen().map(Message::Event)
}

// --- menu tree -------------------------------------------------------------

fn build_root() -> Vec<Node> {
    vec![
        Node::Leaf("Windows Update".into(), Act::Cmd("dnfdragora-updater".into(), false)),
        Node::Sep,
        Node::Sub("Programs".into(), programs_tree()),
        Node::Sub("Settings".into(), settings_tree()),
        Node::Sub("Search".into(), search_tree()),
        Node::Sub("System Tools".into(), system_tools_tree()),
        Node::Leaf("Help".into(), Act::Help),
        Node::Leaf("Run...".into(), Act::Run),
        Node::Sep,
        Node::Leaf("Log Off...".into(), Act::LogOff),
        Node::Leaf("Shut Down...".into(), Act::ShutDown),
    ]
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
        Node::Leaf("Network and Dial-up Connections".into(), Act::Cmd("nm-connection-editor".into(), false)),
        Node::Leaf("Printers".into(), Act::Cmd("system-config-printer".into(), false)),
        Node::Leaf("Taskbar & Start Menu".into(), Act::Mde("control-panel")),
    ]
}

fn search_tree() -> Vec<Node> {
    vec![
        Node::Leaf("For Files or Folders...".into(), Act::Mde("files")),
        Node::Leaf("On the Internet...".into(), Act::Cmd("xdg-open https://duckduckgo.com".into(), false)),
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

fn update(menu: &mut Menu, message: Message) -> Task<Message> {
    match message {
        Message::Click(col, idx) => {
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
        Message::Close => exit(0),
        Message::Event(Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::Escape),
            ..
        })) => {
            if menu.open.is_empty() {
                exit(0);
            } else {
                menu.open.pop(); // Esc backs out one level
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
        Act::Cmd(cmd, terminal) => launch_cmd(cmd, *terminal),
        Act::Mde(sub) => mde_self(sub),
        Act::Run => {
            let _ = Command::new("wofi").args(["--show", "run"]).spawn();
        }
        Act::Help => launch_cmd(
            "echo 'MDE-Retro — Start=Win  Run=Win+R  Close=Alt+F4  Switch=Alt+Tab  My Computer=Win+E'; read -p 'Press Enter to close '",
            true,
        ),
        Act::LogOff => mde_self("logoff"),
        Act::ShutDown => mde_self("shutdown"),
    }
}

fn mde_self(sub: &str) {
    if let Ok(exe) = std::env::current_exe() {
        let _ = Command::new(exe).arg(sub).spawn();
    }
}

fn launch_cmd(cmd: &str, terminal: bool) {
    if terminal {
        let _ = Command::new("foot")
            .arg("-o")
            .arg(format!("font=monospace:size={}", fedora::CLI_FONT_SIZE))
            .arg("sh")
            .arg("-c")
            .arg(cmd)
            .spawn();
    } else {
        let _ = Command::new("sh").arg("-c").arg(cmd).spawn();
    }
}

// --- view ------------------------------------------------------------------

fn pad(top: f32, right: f32, bottom: f32, left: f32) -> Padding {
    Padding { top, right, bottom, left }
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

const ITEM_H: f32 = 22.0;
const SEP_H: f32 = 7.0;
const MAX_COL_H: f32 = 680.0;

fn col_content_height(nodes: &[Node]) -> f32 {
    nodes
        .iter()
        .map(|n| if matches!(n, Node::Sep) { SEP_H } else { ITEM_H })
        .sum()
}

fn render_item<'a>(node: &'a Node, col: usize, idx: usize, selected: bool) -> Element<'a, Message> {
    match node {
        Node::Sep => container(
            container(Space::new(Length::Fill, Length::Fixed(1.0)))
                .width(Length::Fill)
                .style(|_| container::Style {
                    background: Some(Background::Color(palette::color(palette::BUTTON_SHADOW))),
                    ..container::Style::default()
                }),
        )
        .height(Length::Fixed(SEP_H))
        .padding(pad(3.0, 6.0, 3.0, 6.0))
        .into(),
        Node::Leaf(label, _) => button(text(label).size(11.0))
            .on_press(Message::Click(col, idx))
            .width(Length::Fill)
            .height(Length::Fixed(ITEM_H))
            .padding(pad(4.0, 16.0, 0.0, 12.0))
            .style(item_style(false))
            .into(),
        Node::Sub(label, _) => button(
            Row::new()
                .push(text(label).size(11.0).width(Length::Fill))
                .push(text(">").size(11.0)),
        )
        .on_press(Message::Click(col, idx))
        .width(Length::Fill)
        .height(Length::Fixed(ITEM_H))
        .padding(pad(4.0, 8.0, 0.0, 12.0))
        .style(item_style(selected))
        .into(),
    }
}

fn item_list<'a>(nodes: &'a [Node], col: usize, open: Option<usize>) -> Column<'a, Message> {
    let mut list = Column::new().spacing(0.0);
    for (idx, node) in nodes.iter().enumerate() {
        list = list.push(render_item(node, col, idx, open == Some(idx)));
    }
    list
}

fn banner<'a>(height: f32) -> Element<'a, Message> {
    // Navy strip (rotated "MDE-Retro" text is a later refinement).
    container(Space::new(Length::Fixed(24.0), Length::Fixed(height)))
        .style(|_| container::Style {
            background: Some(Background::Color(palette::color(palette::ACTIVE_TITLE))),
            ..container::Style::default()
        })
        .into()
}

fn render_column<'a>(nodes: &'a [Node], col: usize, open: Option<usize>, width: f32) -> Element<'a, Message> {
    let h = (col_content_height(nodes).min(MAX_COL_H)) + 4.0;
    let panel = iced::widget::stack![
        frame::raised().thickness(2),
        container(scrollable(item_list(nodes, col, open))).padding(2.0),
    ];
    container(panel)
        .width(Length::Fixed(width))
        .height(Length::Fixed(h))
        .into()
}

fn render_root_column<'a>(nodes: &'a [Node], open: Option<usize>) -> Element<'a, Message> {
    let h = (col_content_height(nodes).min(MAX_COL_H)) + 4.0;
    let inner = Row::new().push(banner(h)).push(
        container(scrollable(item_list(nodes, 0, open)))
            .width(Length::Fixed(186.0))
            .padding(2.0),
    );
    container(iced::widget::stack![frame::raised().thickness(2), inner])
        .width(Length::Fixed(214.0))
        .height(Length::Fixed(h))
        .into()
}

fn view(menu: &Menu) -> Element<'_, Message> {
    let cols = columns(menu);
    let mut row = Row::new().align_y(Vertical::Top);
    row = row.push(render_root_column(cols[0], menu.open.first().copied()));
    for c in 1..cols.len() {
        row = row.push(render_column(cols[c], c, menu.open.get(c).copied(), 200.0));
    }

    let menu_panel = container(row)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Horizontal::Left)
        .align_y(Vertical::Bottom)
        .padding(pad(0.0, 0.0, metrics::TASKBAR_HEIGHT as f32, 0.0));

    // Behind everything: a full-screen click catcher that closes the menu.
    let overlay = mouse_area(Space::new(Length::Fill, Length::Fill)).on_press(Message::Close);

    iced::widget::stack![overlay, menu_panel].into()
}
