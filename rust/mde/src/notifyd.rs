//! Freedesktop notification daemon — `org.freedesktop.Notifications` (E3).
//!
//! The single producer of toasts and the Action Center history store. Hosted in
//! the long-lived `panel` process under the Windows 10 era ONLY (started next to
//! `tray::start()`). Best-effort name claim like `tray.rs`: if another daemon
//! (gnome-shell, mako, …) already owns the name, we log and no-op so the panel
//! still runs. The store is mirrored to `~/.config/mde/notifications.json` so the
//! short-lived `action-center` / `toast` processes can read it.
//!
//! This is the SHARED daemon (D5/D7): any app's `notify-send`, and the KDE
//! Connect bridge, deliver here.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use zbus::blocking::connection;
use zbus::zvariant::OwnedValue;

/// One notification record (the Action Center history unit).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notif {
    pub id: u32,
    pub app_name: String,
    #[serde(default)]
    pub app_icon: String,
    pub summary: String,
    #[serde(default)]
    pub body: String,
    /// (action_key, label) pairs from the freedesktop `actions` array.
    #[serde(default)]
    pub actions: Vec<(String, String)>,
    #[serde(default)]
    pub hint_urgency: u8,
    pub timestamp: SystemTime,
    /// Transient (e.g. volume OSD) — collected in history but never toasted twice.
    #[serde(default)]
    pub transient: bool,
}

/// The in-process store the panel reads each tick.
pub type Store = Arc<Mutex<Vec<Notif>>>;

/// The on-disk mirror, read by the `action-center` and `toast` processes. Carries
/// a `last_read` marker so the panel can compute the unread badge cross-process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotifFile {
    #[serde(default)]
    pub notifications: Vec<Notif>,
    #[serde(default = "epoch")]
    pub last_read: SystemTime,
}

impl Default for NotifFile {
    fn default() -> Self {
        NotifFile {
            notifications: Vec::new(),
            last_read: epoch(),
        }
    }
}

fn epoch() -> SystemTime {
    SystemTime::UNIX_EPOCH
}

/// `~/.config/mde/notifications.json` (honouring `$XDG_CONFIG_HOME`, like state.rs).
pub fn file_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("mde").join("notifications.json"))
}

/// Load the mirror, tolerating absence/garbage (§2.6).
pub fn load_file() -> NotifFile {
    file_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Write the mirror atomically (temp + rename, like state.rs::save).
pub fn save_file(f: &NotifFile) {
    let Some(path) = file_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string_pretty(f) {
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

/// Mirror the store to disk, preserving the on-disk `last_read` marker.
fn mirror(store: &Store) {
    let mut f = load_file();
    f.notifications = store.lock().map(|s| s.clone()).unwrap_or_default();
    save_file(&f);
}

struct Notifyd {
    store: Store,
    next_id: AtomicU32,
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Notifyd {
    /// Post (or, with `replaces_id`, update) a notification; returns its id.
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        _expire_timeout: i32,
    ) -> u32 {
        let urgency = hints
            .get("urgency")
            .and_then(|v| u8::try_from(v).ok())
            .unwrap_or(1);
        let transient = hints
            .get("transient")
            .and_then(|v| bool::try_from(v).ok())
            .unwrap_or(false);
        let id = if replaces_id != 0 {
            replaces_id
        } else {
            self.next_id.fetch_add(1, Ordering::Relaxed)
        };
        let notif = Notif {
            id,
            app_name,
            app_icon,
            summary,
            body,
            actions: pair_actions(&actions),
            hint_urgency: urgency,
            timestamp: SystemTime::now(),
            transient,
        };
        if let Ok(mut s) = self.store.lock() {
            s.retain(|n| n.id != id);
            s.push(notif);
        }
        mirror(&self.store);
        // Pop a toast for the new notification (E3.8). Transient hints (e.g. a
        // volume OSD) collect in history but don't toast.
        if !transient {
            spawn_toast(id);
        }
        id
    }

    /// Dismiss a notification by id.
    fn close_notification(&self, id: u32) {
        if let Ok(mut s) = self.store.lock() {
            s.retain(|n| n.id != id);
        }
        mirror(&self.store);
    }

    fn get_capabilities(&self) -> Vec<String> {
        vec![
            "body".into(),
            "actions".into(),
            "icon-static".into(),
            "body-markup".into(),
            "persistence".into(),
        ]
    }

    fn get_server_information(&self) -> (String, String, String, String) {
        (
            "MDE Action Center".into(),
            "MDE-Retro".into(),
            env!("CARGO_PKG_VERSION").into(),
            "1.2".into(),
        )
    }

    /// Emitted when a notification is closed (by the user, by `CloseNotification`,
    /// or on expiry). Consumers subscribe to learn a toast went away.
    #[zbus(signal)]
    async fn notification_closed(
        ctxt: &zbus::object_server::SignalContext<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;

    /// Emitted when the user invokes a notification action (from the center).
    #[zbus(signal)]
    async fn action_invoked(
        ctxt: &zbus::object_server::SignalContext<'_>,
        id: u32,
        action_key: &str,
    ) -> zbus::Result<()>;
}

/// Spawn `mde toast <id>` to pop a toast for a new notification (E3.8).
fn spawn_toast(id: u32) {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe)
            .arg("toast")
            .arg(id.to_string())
            .spawn();
    }
}

/// The freedesktop `actions` array is a flat [key, label, key, label, …] list.
fn pair_actions(flat: &[String]) -> Vec<(String, String)> {
    flat.chunks_exact(2)
        .map(|c| (c[0].clone(), c[1].clone()))
        .collect()
}

/// Start the notification daemon on a background thread; returns the shared store
/// the panel reads. No-ops (empty store) if the bus/name is unavailable.
pub fn start() -> Store {
    let store: Store = Arc::new(Mutex::new(load_file().notifications));
    let store2 = store.clone();
    thread::spawn(move || {
        if let Err(e) = serve(store2) {
            eprintln!("mde notifyd: {e}");
        }
    });
    store
}

fn serve(store: Store) -> zbus::Result<()> {
    // Seed next_id past anything already persisted so reused ids don't clash.
    let next = store
        .lock()
        .map(|s| s.iter().map(|n| n.id).max().unwrap_or(0) + 1)
        .unwrap_or(1);
    let daemon = Notifyd {
        store: store.clone(),
        next_id: AtomicU32::new(next),
    };
    let conn = connection::Builder::session()?
        .serve_at("/org/freedesktop/Notifications", daemon)?
        .build()?;

    // Best-effort name claim: another daemon may own it (and may be shutting
    // down), so retry a few times before giving up — non-fatal either way.
    let mut claimed = false;
    for _ in 0..6 {
        if conn.request_name("org.freedesktop.Notifications").is_ok() {
            claimed = true;
            break;
        }
        thread::sleep(Duration::from_millis(500));
    }
    if !claimed {
        return Err(zbus::Error::Failure(
            "org.freedesktop.Notifications already owned".into(),
        ));
    }

    // Keep the connection (and thus the served interface) alive. Incoming method
    // calls are dispatched by zbus's own task; we just park.
    loop {
        thread::sleep(Duration::from_secs(60));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_pair_up_and_drop_odd() {
        let flat = vec![
            "default".into(),
            "Open".into(),
            "reply".into(),
            "Reply".into(),
        ];
        assert_eq!(
            pair_actions(&flat),
            vec![
                ("default".into(), "Open".into()),
                ("reply".into(), "Reply".into())
            ]
        );
        // A trailing key with no label is dropped (chunks_exact).
        assert_eq!(
            pair_actions(&["lonely".into()]),
            Vec::<(String, String)>::new()
        );
    }

    #[test]
    fn notif_file_round_trips() {
        let f = NotifFile {
            notifications: vec![Notif {
                id: 7,
                app_name: "Files".into(),
                app_icon: "folder".into(),
                summary: "Copied".into(),
                body: "3 items".into(),
                actions: vec![("default".into(), "Open".into())],
                hint_urgency: 1,
                timestamp: SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000),
                transient: false,
            }],
            last_read: SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_500),
        };
        let json = serde_json::to_string(&f).unwrap();
        assert_eq!(serde_json::from_str::<NotifFile>(&json).unwrap(), f);
    }

    #[test]
    fn missing_or_garbage_file_is_default() {
        assert_eq!(
            serde_json::from_str::<NotifFile>("not json").unwrap_or_default(),
            NotifFile::default()
        );
        // A partial record fills icon/body/actions/urgency/transient defaults.
        let f: NotifFile = serde_json::from_str(
            r#"{"notifications":[{"id":1,"app_name":"X","summary":"S","timestamp":{"secs_since_epoch":0,"nanos_since_epoch":0}}]}"#,
        )
        .unwrap();
        assert_eq!(f.notifications[0].body, "");
        assert_eq!(f.notifications[0].hint_urgency, 0);
    }
}
