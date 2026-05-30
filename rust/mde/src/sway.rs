//! Minimal sway IPC via the `swaymsg` CLI.
//!
//! The taskbar needs the list of open windows (for window buttons), the focused
//! one (to draw it pressed), and the ability to focus/close on click. Shelling
//! out to `swaymsg -t get_tree` keeps this dependency-free and robust; a first
//! cut polls, which is plenty for a taskbar.

use std::process::Command;

use serde::Deserialize;

/// One toplevel application window in the sway tree.
#[derive(Debug, Clone)]
pub struct Window {
    pub id: i64,
    pub title: String,
    pub app_id: String,
    pub focused: bool,
    pub workspace: String,
}

#[derive(Debug, Deserialize)]
struct Node {
    id: i64,
    name: Option<String>,
    #[serde(default)]
    app_id: Option<String>,
    #[serde(default)]
    window_properties: Option<WindowProperties>,
    #[serde(default)]
    focused: bool,
    #[serde(rename = "type", default)]
    node_type: String,
    #[serde(default)]
    nodes: Vec<Node>,
    #[serde(default)]
    floating_nodes: Vec<Node>,
}

#[derive(Debug, Deserialize)]
struct WindowProperties {
    class: Option<String>,
}

/// Return all application windows (X11 + Wayland) currently open.
pub fn windows() -> anyhow::Result<Vec<Window>> {
    let out = Command::new("swaymsg").args(["-t", "get_tree"]).output()?;
    if !out.status.success() {
        anyhow::bail!("swaymsg -t get_tree failed");
    }
    let root: Node = serde_json::from_slice(&out.stdout)?;
    let mut acc = Vec::new();
    walk(&root, "", &mut acc);
    Ok(acc)
}

fn walk(node: &Node, workspace: &str, acc: &mut Vec<Window>) {
    // Track the current workspace name as we descend.
    let ws = if node.node_type == "workspace" {
        node.name.as_deref().unwrap_or(workspace)
    } else {
        workspace
    };

    let is_window = node.node_type == "con" || node.node_type == "floating_con";
    let app_id = node
        .app_id
        .clone()
        .or_else(|| node.window_properties.as_ref().and_then(|p| p.class.clone()));

    if is_window {
        if let Some(app_id) = app_id {
            // A real toplevel has a name; containers without one are layout nodes.
            if let Some(title) = node.name.clone() {
                acc.push(Window {
                    id: node.id,
                    title,
                    app_id,
                    focused: node.focused,
                    workspace: ws.to_string(),
                });
            }
        }
    }

    for child in node.nodes.iter().chain(node.floating_nodes.iter()) {
        walk(child, ws, acc);
    }
}

/// Focus a window by its sway container id (and switch to its workspace).
pub fn focus(id: i64) -> anyhow::Result<()> {
    run(&format!("[con_id={id}] focus"))
}

/// Close a window by its sway container id.
pub fn close(id: i64) -> anyhow::Result<()> {
    run(&format!("[con_id={id}] kill"))
}

fn run(cmd: &str) -> anyhow::Result<()> {
    let status = Command::new("swaymsg").arg(cmd).status()?;
    if !status.success() {
        anyhow::bail!("swaymsg '{cmd}' failed");
    }
    Ok(())
}
