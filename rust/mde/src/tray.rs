//! System tray (StatusNotifier notification area) for the taskbar.
//!
//! The panel becomes the `org.kde.StatusNotifierWatcher` and the host, so
//! StatusNotifier apps (nm-applet --indicator, blueman, etc.) show their icons
//! in the taskbar. A background thread runs the blocking zbus connection and
//! maintains a shared list of tray items the panel reads each tick; clicking an
//! icon calls the item's `Activate`.
//!
//! We deliberately keep this small: record registered items, advertise that a
//! host exists (apps gate their SNI on `IsStatusNotifierHostRegistered`), and
//! poll each item's `IconName` + tooltip. Pixmap-only items fall back to a
//! generic icon name.

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use zbus::blocking::{connection, Connection, Proxy};
use zbus::names::BusName;

/// A tray item the panel renders.
#[derive(Clone, Debug, Default)]
pub struct TrayItem {
    /// The DBus service hosting the item (bus name).
    pub service: String,
    /// Object path of the item (usually /StatusNotifierItem).
    pub path: String,
    pub icon_name: String,
}

/// The shared, panel-readable tray state.
pub type Tray = Arc<Mutex<Vec<TrayItem>>>;

/// Registered item references (service + object path), filled by the watcher
/// interface and read by the refresh loop.
type Registered = Arc<Mutex<Vec<(String, String)>>>;

/// The org.kde.StatusNotifierWatcher implementation: it records items and tells
/// callers a host is present. (We are the host; we read the list directly, so
/// we don't bother emitting the registration signals.)
struct Watcher {
    registered: Registered,
}

#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl Watcher {
    /// An item registers itself. `service` is either a bus name, or a
    /// `busname/objectpath`; per spec, a bare path means "the sender, at this
    /// path". Resolve to (service, path).
    fn register_status_notifier_item(
        &self,
        service: &str,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) {
        let sender = header.sender().map(|s| s.to_string()).unwrap_or_default();
        let (svc, path) = if service.starts_with('/') {
            (sender, service.to_string())
        } else if let Some((s, p)) = service.split_once('/') {
            (s.to_string(), format!("/{p}"))
        } else {
            (service.to_string(), "/StatusNotifierItem".to_string())
        };
        let mut reg = self.registered.lock().unwrap();
        if !reg.iter().any(|(s, p)| s == &svc && p == &path) {
            reg.push((svc, path));
        }
    }

    fn register_status_notifier_host(&self, _service: &str) {}

    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        self.registered.lock().unwrap().iter().map(|(s, p)| format!("{s}{p}")).collect()
    }

    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn protocol_version(&self) -> i32 {
        0
    }
}

/// Start the tray watcher/host on a background thread; returns the shared list
/// the panel reads. Silently does nothing if the bus/name is unavailable (e.g.
/// another watcher already owns it) — the panel just shows no tray.
pub fn start() -> Tray {
    let tray: Tray = Arc::new(Mutex::new(Vec::new()));
    let registered: Registered = Arc::new(Mutex::new(Vec::new()));
    let tray2 = tray.clone();
    thread::spawn(move || {
        if let Err(e) = serve(tray2, registered) {
            eprintln!("mde tray: {e}");
        }
    });
    tray
}

fn serve(tray: Tray, registered: Registered) -> zbus::Result<()> {
    let watcher = Watcher { registered: registered.clone() };
    let conn = connection::Builder::session()?
        .serve_at("/StatusNotifierWatcher", watcher)?
        .build()?;

    // Claim the well-known name. Another host (e.g. swaybar) may still own it for
    // a moment as it shuts down, so retry before giving up.
    let mut claimed = false;
    for _ in 0..20 {
        if conn.request_name("org.kde.StatusNotifierWatcher").is_ok() {
            claimed = true;
            break;
        }
        thread::sleep(Duration::from_millis(500));
    }
    if !claimed {
        return Err(zbus::Error::Failure("StatusNotifierWatcher name unavailable".into()));
    }

    // Refresh loop: prune dead services, then read each item's icon + tooltip.
    loop {
        refresh(&conn, &registered, &tray);
        thread::sleep(Duration::from_secs(2));
    }
}

/// Whether `service` still has an owner on the bus.
fn has_owner(conn: &Connection, service: &str) -> bool {
    let Ok(name) = BusName::try_from(service.to_string()) else { return false };
    Proxy::new(conn, "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus")
        .ok()
        .and_then(|p| p.call::<_, _, bool>("NameHasOwner", &(name)).ok())
        .unwrap_or(false)
}

fn refresh(conn: &Connection, registered: &Registered, tray: &Tray) {
    // Drop items whose service has disappeared.
    {
        let mut reg = registered.lock().unwrap();
        reg.retain(|(s, _)| has_owner(conn, s));
    }
    let items: Vec<(String, String)> = registered.lock().unwrap().clone();
    let mut out = Vec::new();
    for (service, path) in items {
        let proxy = match Proxy::new(conn, service.clone(), path.clone(), "org.kde.StatusNotifierItem") {
            Ok(p) => p,
            Err(_) => continue,
        };
        let icon_name: String = proxy.get_property("IconName").unwrap_or_default();
        let icon_name = if icon_name.is_empty() { "application-x-executable".to_string() } else { icon_name };
        out.push(TrayItem { service, path, icon_name });
    }
    *tray.lock().unwrap() = out;
}

/// Activate a tray item (left-click) by calling its `Activate(x, y)`.
pub fn activate(service: &str, path: &str) {
    let (service, path) = (service.to_string(), path.to_string());
    thread::spawn(move || {
        if let Ok(conn) = Connection::session() {
            if let Ok(proxy) = Proxy::new(&conn, service, path, "org.kde.StatusNotifierItem") {
                let _ = proxy.call::<_, _, ()>("Activate", &(0i32, 0i32));
            }
        }
    });
}
