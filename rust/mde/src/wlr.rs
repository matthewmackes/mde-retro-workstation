//! Window management via wlr-foreign-toplevel-management.
//!
//! The hard-cut replacement for the old sway-IPC window list/control: a
//! `wayland-client` listener for `zwlr_foreign_toplevel_manager_v1` runs on a
//! background thread (like [`crate::tray`]), keeping a shared snapshot of the
//! open toplevels the taskbar reads each tick, plus a map of their handles so
//! any thread can activate / close / minimize / maximize them. Works on labwc
//! and any wlroots compositor (sway included, which is handy for testing).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;

use wayland_client::backend::ObjectId;
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::protocol::wl_seat::{self, WlSeat};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_handle_v1::{
    self as ftl, ZwlrForeignToplevelHandleV1,
};
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_manager_v1::{
    self as mgr, ZwlrForeignToplevelManagerV1,
};

// foreign-toplevel state enum values (wire protocol): maximized, minimized,
// activated, fullscreen — in that order.
const STATE_MAXIMIZED: u32 = 0;
const STATE_MINIMIZED: u32 = 1;
const STATE_ACTIVATED: u32 = 2;

/// One open toplevel window, as the taskbar sees it.
#[derive(Clone, Debug)]
pub struct Window {
    pub id: u64,
    pub title: String,
    pub app_id: String,
    pub focused: bool,
    pub minimized: bool,
    pub maximized: bool,
}

/// A handle the panel keeps: read the window list, and drive window actions.
#[derive(Clone)]
pub struct Wm {
    windows: Arc<Mutex<Vec<Window>>>,
    handles: Arc<Mutex<HashMap<u64, ZwlrForeignToplevelHandleV1>>>,
    seat: Arc<Mutex<Option<WlSeat>>>,
    conn: Connection,
}

impl Wm {
    /// The current toplevel snapshot (id-ordered).
    pub fn windows(&self) -> Vec<Window> {
        self.windows.lock().map(|v| v.clone()).unwrap_or_default()
    }

    fn handle(&self, id: u64) -> Option<ZwlrForeignToplevelHandleV1> {
        self.handles.lock().ok().and_then(|m| m.get(&id).cloned())
    }

    fn flush(&self) {
        let _ = self.conn.flush();
    }

    /// Focus/raise a window (un-minimises it too).
    pub fn focus(&self, id: u64) {
        let seat = self.seat.lock().ok().and_then(|s| s.clone());
        if let (Some(h), Some(seat)) = (self.handle(id), seat) {
            h.unset_minimized();
            h.activate(&seat);
            self.flush();
        }
    }

    /// Close a window (used by the labwc titlebar path / scripting).
    #[allow(dead_code)]
    pub fn close(&self, id: u64) {
        if let Some(h) = self.handle(id) {
            h.close();
            self.flush();
        }
    }

    pub fn set_minimized(&self, id: u64, on: bool) {
        if let Some(h) = self.handle(id) {
            if on {
                h.set_minimized();
            } else {
                h.unset_minimized();
            }
            self.flush();
        }
    }

    #[allow(dead_code)]
    pub fn set_maximized(&self, id: u64, on: bool) {
        if let Some(h) = self.handle(id) {
            if on {
                h.set_maximized();
            } else {
                h.unset_maximized();
            }
            self.flush();
        }
    }
}

/// Start the background foreign-toplevel listener; `None` if there's no Wayland
/// display or no compositor support (the taskbar then shows no window buttons).
pub fn start() -> Option<Wm> {
    let conn = Connection::connect_to_env().ok()?;
    let windows = Arc::new(Mutex::new(Vec::new()));
    let handles = Arc::new(Mutex::new(HashMap::new()));
    let seat = Arc::new(Mutex::new(None));
    let wm = Wm {
        windows: windows.clone(),
        handles: handles.clone(),
        seat: seat.clone(),
        conn: conn.clone(),
    };
    thread::spawn(move || {
        if let Err(e) = serve(conn, windows, handles, seat) {
            eprintln!("mde wlr: {e}");
        }
    });
    Some(wm)
}

/// Internal per-toplevel record: committed fields + the pending fields built up
/// between `done` events (foreign-toplevel is double-buffered like that).
struct Item {
    id: u64,
    title: String,
    app_id: String,
    focused: bool,
    minimized: bool,
    maximized: bool,
    p_title: String,
    p_app_id: String,
    p_focused: bool,
    p_minimized: bool,
    p_maximized: bool,
}

impl Item {
    fn new(id: u64) -> Self {
        Item {
            id,
            title: String::new(),
            app_id: String::new(),
            focused: false,
            minimized: false,
            maximized: false,
            p_title: String::new(),
            p_app_id: String::new(),
            p_focused: false,
            p_minimized: false,
            p_maximized: false,
        }
    }
    fn commit(&mut self) {
        self.title = self.p_title.clone();
        self.app_id = self.p_app_id.clone();
        self.focused = self.p_focused;
        self.minimized = self.p_minimized;
        self.maximized = self.p_maximized;
    }
}

struct AppState {
    next_id: u64,
    items: HashMap<ObjectId, Item>,
    out: Arc<Mutex<Vec<Window>>>,
    handles: Arc<Mutex<HashMap<u64, ZwlrForeignToplevelHandleV1>>>,
    seat_shared: Arc<Mutex<Option<WlSeat>>>,
}

impl AppState {
    /// Republish the public snapshot from the internal records.
    fn rebuild(&self) {
        let mut v: Vec<Window> = self
            .items
            .values()
            .map(|it| Window {
                id: it.id,
                title: it.title.clone(),
                app_id: it.app_id.clone(),
                focused: it.focused,
                minimized: it.minimized,
                maximized: it.maximized,
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
    windows: Arc<Mutex<Vec<Window>>>,
    handles: Arc<Mutex<HashMap<u64, ZwlrForeignToplevelHandleV1>>>,
    seat: Arc<Mutex<Option<WlSeat>>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut queue = conn.new_event_queue::<AppState>();
    let qh = queue.handle();
    conn.display().get_registry(&qh, ());
    let mut state = AppState {
        next_id: 1,
        items: HashMap::new(),
        out: windows,
        handles,
        seat_shared: seat,
    };
    // Two roundtrips: the first binds the globals, the second drains the initial
    // burst of toplevel + state events so the first snapshot is populated.
    queue.roundtrip(&mut state)?;
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
        if let wl_registry::Event::Global { name, interface, version } = event {
            match interface.as_str() {
                "zwlr_foreign_toplevel_manager_v1" => {
                    registry.bind::<ZwlrForeignToplevelManagerV1, _, _>(name, version.min(3), qh, ());
                }
                "wl_seat" => {
                    let s = registry.bind::<WlSeat, _, _>(name, version.min(1), qh, ());
                    if let Ok(mut shared) = state.seat_shared.lock() {
                        *shared = Some(s);
                    }
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<WlSeat, ()> for AppState {
    fn event(_: &mut Self, _: &WlSeat, _: wl_seat::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ZwlrForeignToplevelManagerV1, ()> for AppState {
    fn event(
        state: &mut Self,
        _: &ZwlrForeignToplevelManagerV1,
        event: mgr::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let mgr::Event::Toplevel { toplevel } = event {
            let id = state.next_id;
            state.next_id += 1;
            if let Ok(mut h) = state.handles.lock() {
                h.insert(id, toplevel.clone());
            }
            state.items.insert(toplevel.id(), Item::new(id));
        }
    }

    // The manager's `toplevel` event (opcode 0) hands us a new handle object;
    // this override (it must live inside the Dispatch impl) tells wayland-client
    // how to construct that child.
    wayland_client::event_created_child!(AppState, ZwlrForeignToplevelManagerV1, [
        0 => (ZwlrForeignToplevelHandleV1, ()),
    ]);
}

impl Dispatch<ZwlrForeignToplevelHandleV1, ()> for AppState {
    fn event(
        state: &mut Self,
        handle: &ZwlrForeignToplevelHandleV1,
        event: ftl::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let oid = handle.id();
        match event {
            ftl::Event::Title { title } => {
                if let Some(it) = state.items.get_mut(&oid) {
                    it.p_title = title;
                }
            }
            ftl::Event::AppId { app_id } => {
                if let Some(it) = state.items.get_mut(&oid) {
                    it.p_app_id = app_id;
                }
            }
            ftl::Event::State { state: bytes } => {
                if let Some(it) = state.items.get_mut(&oid) {
                    it.p_focused = false;
                    it.p_minimized = false;
                    it.p_maximized = false;
                    for v in bytes.chunks_exact(4).map(|c| u32::from_ne_bytes([c[0], c[1], c[2], c[3]])) {
                        match v {
                            STATE_MAXIMIZED => it.p_maximized = true,
                            STATE_MINIMIZED => it.p_minimized = true,
                            STATE_ACTIVATED => it.p_focused = true,
                            _ => {}
                        }
                    }
                }
            }
            ftl::Event::Done => {
                if let Some(it) = state.items.get_mut(&oid) {
                    it.commit();
                }
                state.rebuild();
            }
            ftl::Event::Closed => {
                if let Some(it) = state.items.remove(&oid) {
                    if let Ok(mut h) = state.handles.lock() {
                        h.remove(&it.id);
                    }
                }
                state.rebuild();
            }
            _ => {}
        }
    }
}

/// `mde __wlr-list` — headless: list the open toplevels and exit (a smoke test
/// for the foreign-toplevel client against the running compositor).
pub fn debug_list() {
    match start() {
        Some(wm) => {
            thread::sleep(std::time::Duration::from_millis(500));
            for w in wm.windows() {
                println!(
                    "[{}] {:<40} app_id={:<20} focused={} min={} max={}",
                    w.id, w.title, w.app_id, w.focused, w.minimized, w.maximized
                );
            }
        }
        None => eprintln!("no wayland display / no foreign-toplevel support"),
    }
}
