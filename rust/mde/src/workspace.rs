//! Virtual desktops via ext-workspace-v1.
//!
//! The Win10 Task View desktop band reads its workspaces from here. Like
//! [`crate::wlr`], a `wayland-client` listener for `ext_workspace_manager_v1`
//! runs on a background thread, keeping a shared snapshot of the workspaces
//! (name + active) the overlay reads each tick, plus the handles so any thread
//! can activate / create / remove one. labwc 0.9.x advertises the manager
//! global; on a compositor without it, [`start`] returns `None` and Task View
//! falls back to the fixed strip from `state.rs` (E4.5).
//!
//! mde never touches window geometry here — it drives the workspace manager's
//! own requests only, keeping the compositor boundary intact.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;

use wayland_client::backend::ObjectId;
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum};
use wayland_protocols::ext::workspace::v1::client::ext_workspace_group_handle_v1::{
    self as grp, ExtWorkspaceGroupHandleV1,
};
use wayland_protocols::ext::workspace::v1::client::ext_workspace_handle_v1::{
    self as wsh, ExtWorkspaceHandleV1,
};
use wayland_protocols::ext::workspace::v1::client::ext_workspace_manager_v1::{
    self as mgr, ExtWorkspaceManagerV1,
};

/// One virtual desktop, as the Task View band sees it.
#[derive(Clone, Debug)]
pub struct Workspace {
    pub id: u64,
    pub name: String,
    pub active: bool,
    /// The compositor advertised the `remove` capability for this workspace.
    /// labwc (static `<desktops>`) does not, so Task View hides the × there.
    pub removable: bool,
}

/// A handle Task View keeps: read the workspace snapshot and drive switching,
/// creation, and removal. Cloneable (the proxies are `Send`/`Sync`; requests
/// are serialised by the shared `Connection`, exactly as `wlr::Wm` does).
#[derive(Clone)]
pub struct Workspaces {
    list: Arc<Mutex<Vec<Workspace>>>,
    handles: Arc<Mutex<HashMap<u64, ExtWorkspaceHandleV1>>>,
    manager: Arc<Mutex<Option<ExtWorkspaceManagerV1>>>,
    group: Arc<Mutex<Option<ExtWorkspaceGroupHandleV1>>>,
    can_create: Arc<Mutex<bool>>,
    conn: Connection,
}

impl Workspaces {
    /// The current workspace snapshot (creation-ordered).
    pub fn list(&self) -> Vec<Workspace> {
        self.list.lock().map(|v| v.clone()).unwrap_or_default()
    }

    /// Whether the compositor's group advertised the `create_workspace`
    /// capability. labwc (static `<desktops>`) does not, so Task View hides the
    /// "+ New desktop" chip there rather than offering a dead button.
    pub fn can_create(&self) -> bool {
        self.can_create.lock().map(|b| *b).unwrap_or(false)
    }

    fn handle(&self, id: u64) -> Option<ExtWorkspaceHandleV1> {
        self.handles.lock().ok().and_then(|m| m.get(&id).cloned())
    }

    fn manager(&self) -> Option<ExtWorkspaceManagerV1> {
        self.manager.lock().ok().and_then(|m| m.clone())
    }

    /// ext-workspace requests are double-buffered: an `activate`/`remove`/
    /// `create_workspace` only takes effect after the manager's `commit`.
    fn commit(&self) {
        if let Some(m) = self.manager() {
            m.commit();
            let _ = self.conn.flush();
        }
    }

    /// Switch to a workspace.
    pub fn activate(&self, id: u64) {
        if let Some(h) = self.handle(id) {
            h.activate();
            self.commit();
        }
    }

    /// Create a new workspace (named) in the first group, if the compositor
    /// advertised the capability. A no-op otherwise (e.g. labwc).
    pub fn create(&self, name: &str) {
        if !self.can_create() {
            return;
        }
        let group = self.group.lock().ok().and_then(|g| g.clone());
        if let Some(g) = group {
            g.create_workspace(name.to_string());
            self.commit();
        }
    }

    /// Remove a workspace (if the compositor allows it).
    pub fn remove(&self, id: u64) {
        if let Some(h) = self.handle(id) {
            h.remove();
            self.commit();
        }
    }
}

/// Start the background ext-workspace listener; `None` if there's no Wayland
/// display or the compositor doesn't advertise `ext_workspace_manager_v1` (the
/// overlay then falls back to the `state.rs` fixed-desktop strip).
pub fn start() -> Option<Workspaces> {
    // Test/diagnostic escape hatch: force the fallback ladder (E4.5) without a
    // compositor. Lets a unit test exercise the no-ext-workspace path.
    if std::env::var_os("MDE_NO_EXT_WORKSPACE").is_some() {
        return None;
    }
    let conn = Connection::connect_to_env().ok()?;
    let list = Arc::new(Mutex::new(Vec::new()));
    let handles = Arc::new(Mutex::new(HashMap::new()));
    let manager = Arc::new(Mutex::new(None));
    let group = Arc::new(Mutex::new(None));
    let can_create = Arc::new(Mutex::new(false));
    let ws = Workspaces {
        list: list.clone(),
        handles: handles.clone(),
        manager: manager.clone(),
        group: group.clone(),
        can_create: can_create.clone(),
        conn: conn.clone(),
    };
    // Probe synchronously for the manager global so a missing protocol returns
    // `None` here (the fallback ladder needs that answer before it draws),
    // rather than spawning a thread that finds nothing.
    if !has_manager(&conn) {
        return None;
    }
    thread::spawn(move || {
        if let Err(e) = serve(conn, list, handles, manager, group, can_create) {
            eprintln!("mde workspace: {e}");
        }
    });
    Some(ws)
}

/// One roundtrip against a throwaway registry queue: does the compositor
/// advertise `ext_workspace_manager_v1` at all?
fn has_manager(conn: &Connection) -> bool {
    struct Probe {
        found: bool,
    }
    impl Dispatch<WlRegistry, ()> for Probe {
        fn event(
            state: &mut Self,
            _: &WlRegistry,
            event: wl_registry::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            if let wl_registry::Event::Global { interface, .. } = event {
                if interface == "ext_workspace_manager_v1" {
                    state.found = true;
                }
            }
        }
    }
    let mut queue = conn.new_event_queue::<Probe>();
    let qh = queue.handle();
    conn.display().get_registry(&qh, ());
    let mut probe = Probe { found: false };
    queue.roundtrip(&mut probe).ok();
    probe.found
}

/// Internal per-workspace record: committed fields + the pending fields built
/// up between the manager's `done` events (ext-workspace is double-buffered).
struct WsItem {
    id: u64,
    name: String,
    active: bool,
    removable: bool,
    p_name: String,
    p_id_str: String,
    p_active: bool,
    p_removable: bool,
}

impl WsItem {
    fn new(id: u64) -> Self {
        WsItem {
            id,
            name: String::new(),
            active: false,
            removable: false,
            p_name: String::new(),
            p_id_str: String::new(),
            p_active: false,
            p_removable: false,
        }
    }
    fn commit(&mut self) {
        // Prefer the human name; fall back to the protocol id string.
        self.name = if self.p_name.is_empty() {
            self.p_id_str.clone()
        } else {
            self.p_name.clone()
        };
        self.active = self.p_active;
        self.removable = self.p_removable;
    }
}

struct AppState {
    next_id: u64,
    items: HashMap<ObjectId, WsItem>,
    out: Arc<Mutex<Vec<Workspace>>>,
    handles: Arc<Mutex<HashMap<u64, ExtWorkspaceHandleV1>>>,
    group_shared: Arc<Mutex<Option<ExtWorkspaceGroupHandleV1>>>,
    can_create_shared: Arc<Mutex<bool>>,
    // The manager proxy, captured in the registry Dispatch and lifted into the
    // shared slot after the first roundtrip so request methods can `commit`.
    bound_manager: Option<ExtWorkspaceManagerV1>,
}

impl AppState {
    /// Republish the public snapshot from the internal records.
    fn rebuild(&self) {
        let mut v: Vec<Workspace> = self
            .items
            .values()
            .map(|it| Workspace {
                id: it.id,
                name: it.name.clone(),
                active: it.active,
                removable: it.removable,
            })
            .collect();
        v.sort_by_key(|w| w.id);
        if let Ok(mut out) = self.out.lock() {
            *out = v;
        }
    }
}

fn serve(
    conn: Connection,
    list: Arc<Mutex<Vec<Workspace>>>,
    handles: Arc<Mutex<HashMap<u64, ExtWorkspaceHandleV1>>>,
    manager: Arc<Mutex<Option<ExtWorkspaceManagerV1>>>,
    group: Arc<Mutex<Option<ExtWorkspaceGroupHandleV1>>>,
    can_create: Arc<Mutex<bool>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut queue = conn.new_event_queue::<AppState>();
    let qh = queue.handle();
    conn.display().get_registry(&qh, ());
    let mut state = AppState {
        next_id: 1,
        items: HashMap::new(),
        out: list,
        handles,
        group_shared: group,
        can_create_shared: can_create,
        bound_manager: None,
    };
    // First roundtrip binds the manager; stash it so request methods can commit.
    queue.roundtrip(&mut state)?;
    // The bind happens inside the registry Dispatch, which can't see `manager`;
    // recover the bound proxy from the queue's known objects is awkward, so the
    // registry handler stores it through this shared slot instead (set below).
    if let Ok(mut m) = manager.lock() {
        *m = state.bound_manager.clone();
    }
    // Second roundtrip drains the initial burst of group/workspace + state.
    queue.roundtrip(&mut state)?;
    loop {
        queue.blocking_dispatch(&mut state)?;
    }
}

impl Dispatch<WlRegistry, ()> for AppState {
    fn event(
        state: &mut Self,
        registry: &WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            if interface == "ext_workspace_manager_v1" {
                let m = registry.bind::<ExtWorkspaceManagerV1, _, _>(name, version.min(1), qh, ());
                state.bound_manager = Some(m);
            }
        }
    }
}

impl Dispatch<ExtWorkspaceManagerV1, ()> for AppState {
    fn event(
        state: &mut Self,
        _: &ExtWorkspaceManagerV1,
        event: mgr::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            mgr::Event::Workspace { workspace } => {
                let id = state.next_id;
                state.next_id += 1;
                if let Ok(mut h) = state.handles.lock() {
                    h.insert(id, workspace.clone());
                }
                state.items.insert(workspace.id(), WsItem::new(id));
            }
            mgr::Event::WorkspaceGroup { workspace_group } => {
                // Keep the first group as the create_workspace target.
                if let Ok(mut g) = state.group_shared.lock() {
                    if g.is_none() {
                        *g = Some(workspace_group);
                    }
                }
            }
            mgr::Event::Done => state.rebuild(),
            _ => {}
        }
    }

    // The manager's `workspace_group` (opcode 0) and `workspace` (opcode 1)
    // events each hand us a new child object; tell wayland-client how to build
    // them.
    wayland_client::event_created_child!(AppState, ExtWorkspaceManagerV1, [
        0 => (ExtWorkspaceGroupHandleV1, ()),
        1 => (ExtWorkspaceHandleV1, ()),
    ]);
}

impl Dispatch<ExtWorkspaceGroupHandleV1, ()> for AppState {
    fn event(
        state: &mut Self,
        _: &ExtWorkspaceGroupHandleV1,
        event: grp::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // The only group event the band cares about: whether clients may create
        // workspaces. (output / workspace-membership events don't affect the
        // flat snapshot.)
        if let grp::Event::Capabilities { capabilities } = event {
            let can = matches!(capabilities,
                WEnum::Value(c) if c.contains(grp::GroupCapabilities::CreateWorkspace));
            if let Ok(mut b) = state.can_create_shared.lock() {
                *b = can;
            }
        }
    }
}

impl Dispatch<ExtWorkspaceHandleV1, ()> for AppState {
    fn event(
        state: &mut Self,
        handle: &ExtWorkspaceHandleV1,
        event: wsh::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let oid = handle.id();
        match event {
            wsh::Event::Name { name } => {
                if let Some(it) = state.items.get_mut(&oid) {
                    it.p_name = name;
                }
            }
            wsh::Event::Id { id } => {
                if let Some(it) = state.items.get_mut(&oid) {
                    it.p_id_str = id;
                }
            }
            wsh::Event::State { state: bits } => {
                if let Some(it) = state.items.get_mut(&oid) {
                    it.p_active = matches!(bits, WEnum::Value(s) if s.contains(wsh::State::Active));
                }
            }
            wsh::Event::Capabilities { capabilities } => {
                if let Some(it) = state.items.get_mut(&oid) {
                    it.p_removable = matches!(capabilities,
                        WEnum::Value(c) if c.contains(wsh::WorkspaceCapabilities::Remove));
                }
            }
            wsh::Event::Removed => {
                if let Some(it) = state.items.remove(&oid) {
                    if let Ok(mut h) = state.handles.lock() {
                        h.remove(&it.id);
                    }
                }
                handle.destroy();
                state.rebuild();
            }
            _ => {}
        }
        // ext-workspace commits per-workspace pending fields on the manager's
        // `done`; fold them in eagerly so a `done` we already passed still shows.
        if let Some(it) = state.items.get_mut(&oid) {
            it.commit();
        }
    }
}

/// Briefly run the client so the initial workspace snapshot arrives, returning
/// the live handle (or `None` if the compositor has no ext-workspace).
fn settled() -> Option<Workspaces> {
    let ws = start()?;
    thread::sleep(std::time::Duration::from_millis(400));
    Some(ws)
}

/// `mde __ws-activate <id>` — switch to a workspace by its mde id and exit (a
/// smoke test that the ext-workspace `activate` request reaches the compositor;
/// the switch persists after this process exits).
pub fn debug_activate(id: u64) {
    if let Some(ws) = settled() {
        ws.activate(id);
        thread::sleep(std::time::Duration::from_millis(250));
    } else {
        eprintln!("mde __ws-activate: no ext-workspace-v1 support");
    }
}

/// `mde __ws-list` — headless: list the live workspaces and exit (a smoke test
/// for the ext-workspace client against the running compositor).
pub fn debug_list() {
    match start() {
        Some(ws) => {
            thread::sleep(std::time::Duration::from_millis(500));
            println!("can_create={}", ws.can_create());
            let list = ws.list();
            if list.is_empty() {
                println!("(no workspaces reported)");
            }
            for w in list {
                println!(
                    "[{}] {:<24} active={} removable={}",
                    w.id, w.name, w.active, w.removable
                );
            }
        }
        None => eprintln!("no wayland display / no ext-workspace-v1 support"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_guard_forces_fallback() {
        // E4.5: the guard makes start() return None before touching Wayland, so
        // the fallback ladder is exercisable headless.
        std::env::set_var("MDE_NO_EXT_WORKSPACE", "1");
        assert!(start().is_none());
        std::env::remove_var("MDE_NO_EXT_WORKSPACE");
    }
}
