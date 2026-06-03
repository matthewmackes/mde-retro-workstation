//! AutoPlay removable-storage monitor for Settings ▸ Devices ▸ AutoPlay (E12.9).
//!
//! `mde devices-monitor` watches **udisks2** (system bus) for newly-appearing
//! removable filesystems and, per the AutoPlay config in `menu.json`, opens them
//! in `mde files` (or sends a notification, or does nothing). The session's
//! gvfs-udisks2 volume monitor usually mounts the media itself; this only reacts
//! to the mount and acts on it — and falls back to mounting via udisks2 itself if
//! nothing else did. Autostarted from the labwc autostart (like the clipboard
//! daemon).
//!
//! The classification (removable drive vs memory card) and the act/skip decision
//! are pure and unit-tested; the D-Bus plumbing around them is intentionally thin.
//! `mde devices-monitor --dry-run <removable|card> <mountpoint>` prints what would
//! happen for a synthetic event without touching D-Bus (the bench seam).

use std::collections::HashMap;
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::OwnedValue;

const UDISKS: &str = "org.freedesktop.UDisks2";
const OBJMGR_PATH: &str = "/org/freedesktop/UDisks2";
const OBJMGR: &str = "org.freedesktop.DBus.ObjectManager";
const BLOCK: &str = "org.freedesktop.UDisks2.Block";
const FILESYSTEM: &str = "org.freedesktop.UDisks2.Filesystem";
const DRIVE: &str = "org.freedesktop.UDisks2.Drive";
const PROPS: &str = "org.freedesktop.DBus.Properties";

/// What AutoPlay does for a media type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Open the mounted volume in `mde files`.
    Open,
    /// Notify the user it's ready (a degraded "ask me what to do").
    Ask,
    /// Do nothing.
    Nothing,
}

impl Action {
    pub fn from_key(s: &str) -> Action {
        match s {
            "ask" => Action::Ask,
            "nothing" => Action::Nothing,
            _ => Action::Open,
        }
    }
}

/// The kind of removable media that appeared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    RemovableDrive,
    MemoryCard,
}

/// AutoPlay settings, read fresh from `menu.json` on each event.
#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub enabled: bool,
    pub removable: Action,
    pub memory_card: Action,
}

impl Config {
    pub fn load() -> Config {
        let st = crate::state::load();
        Config {
            enabled: st.autoplay_enabled,
            removable: Action::from_key(&st.autoplay_removable),
            memory_card: Action::from_key(&st.autoplay_memcard),
        }
    }
    fn action_for(&self, kind: DeviceKind) -> Action {
        match kind {
            DeviceKind::RemovableDrive => self.removable,
            DeviceKind::MemoryCard => self.memory_card,
        }
    }
}

/// Classify a drive by its udisks2 `Media`/`ConnectionBus`. SD/MMC/CF/MS flash and
/// the SDIO bus are memory cards; everything else removable is a drive. Pure.
pub fn classify(media: &str, connection_bus: &str) -> DeviceKind {
    let m = media.to_lowercase();
    let card = m.contains("flash_sd")
        || m.contains("flash_mmc")
        || m.contains("flash_cf")
        || m.contains("flash_ms")
        || m.contains("flash_sm")
        || connection_bus.eq_ignore_ascii_case("sdio");
    if card {
        DeviceKind::MemoryCard
    } else {
        DeviceKind::RemovableDrive
    }
}

/// What to do for this media, given the config. AutoPlay off ⇒ nothing. Pure.
pub fn decide(cfg: &Config, kind: DeviceKind) -> Action {
    if !cfg.enabled {
        return Action::Nothing;
    }
    cfg.action_for(kind)
}

/// `mde devices-monitor [--dry-run KIND MOUNTPOINT]`.
pub fn run(args: &[String]) -> ExitCode {
    if args.first().map(String::as_str) == Some("--dry-run") {
        let kind = match args.get(1).map(String::as_str) {
            Some("card") => DeviceKind::MemoryCard,
            _ => DeviceKind::RemovableDrive,
        };
        let mp = args
            .get(2)
            .cloned()
            .unwrap_or_else(|| "/run/media/test".into());
        let cfg = Config::load();
        match decide(&cfg, kind) {
            Action::Open => println!("autoplay: would open `mde files {mp}`"),
            Action::Ask => println!("autoplay: would notify about {mp}"),
            Action::Nothing => println!("autoplay: no action"),
        }
        return ExitCode::SUCCESS;
    }
    match watch() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mde devices-monitor: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Block on the udisks2 `InterfacesAdded` signal; act on each new filesystem.
fn watch() -> zbus::Result<()> {
    let conn = Connection::system()?;
    let om = Proxy::new(&conn, UDISKS, OBJMGR_PATH, OBJMGR)?;
    let signals = om.receive_signal("InterfacesAdded")?;
    for msg in signals {
        let Ok((path, ifaces)) = msg.body().deserialize::<(
            zbus::zvariant::OwnedObjectPath,
            HashMap<String, HashMap<String, OwnedValue>>,
        )>() else {
            continue;
        };
        // Only filesystems can be opened.
        if !ifaces.contains_key(FILESYSTEM) {
            continue;
        }
        handle(&conn, path.as_str(), ifaces.get(BLOCK));
    }
    Ok(())
}

fn prop_str(p: &HashMap<String, OwnedValue>, key: &str) -> String {
    p.get(key)
        .and_then(|v| v.try_clone().ok())
        .and_then(|v| String::try_from(v).ok())
        .unwrap_or_default()
}

fn prop_bool(p: &HashMap<String, OwnedValue>, key: &str) -> bool {
    p.get(key)
        .and_then(|v| v.try_clone().ok())
        .and_then(|v| bool::try_from(v).ok())
        .unwrap_or(false)
}

fn get_all(conn: &Connection, path: &str, iface: &str) -> Option<HashMap<String, OwnedValue>> {
    Proxy::new(conn, UDISKS, path, PROPS)
        .ok()?
        .call::<_, _, HashMap<String, OwnedValue>>("GetAll", &(iface))
        .ok()
}

/// Act on a freshly-appeared filesystem block device.
fn handle(conn: &Connection, fs_path: &str, block: Option<&HashMap<String, OwnedValue>>) {
    let cfg = Config::load();
    if !cfg.enabled {
        return;
    }
    // Block props: skip internal/system devices, find the backing Drive. Use the
    // props from the InterfacesAdded payload if present, else fetch them.
    let fetched;
    let block = match block {
        Some(b) => b,
        None => match get_all(conn, fs_path, BLOCK) {
            Some(b) => {
                fetched = b;
                &fetched
            }
            None => return,
        },
    };
    if prop_bool(block, "HintIgnore") || prop_bool(block, "HintSystem") {
        return;
    }
    let drive_path = prop_str(block, "Drive");
    if drive_path.is_empty() || drive_path == "/" {
        return;
    }
    let Some(drive) = get_all(conn, &drive_path, DRIVE) else {
        return;
    };
    // Only act on removable media (never internal disks).
    if !prop_bool(&drive, "Removable") {
        return;
    }
    let kind = classify(
        &prop_str(&drive, "Media"),
        &prop_str(&drive, "ConnectionBus"),
    );
    let action = decide(&cfg, kind);
    if action == Action::Nothing {
        return;
    }
    // Wait for the session to mount it; mount ourselves if nobody did.
    let Some(mp) = ensure_mounted(conn, fs_path) else {
        return;
    };
    match action {
        Action::Open => {
            let exe = std::env::current_exe().unwrap_or_else(|_| "mde".into());
            let _ = std::process::Command::new(exe)
                .arg("files")
                .arg(&mp)
                .spawn();
        }
        Action::Ask => {
            let label = match kind {
                DeviceKind::MemoryCard => "Memory card",
                DeviceKind::RemovableDrive => "Removable drive",
            };
            let _ = std::process::Command::new("notify-send")
                .arg("-a")
                .arg("AutoPlay")
                .arg(format!("{label} ready"))
                .arg(format!("Mounted at {mp} — open it in Files?"))
                .spawn();
        }
        Action::Nothing => {}
    }
}

/// First mountpoint of the filesystem, parsed from udisks2 `MountPoints` (`aay`).
fn current_mountpoint(conn: &Connection, fs_path: &str) -> Option<String> {
    let props = get_all(conn, fs_path, FILESYSTEM)?;
    let mounts: Vec<Vec<u8>> = props
        .get("MountPoints")
        .and_then(|v| v.try_clone().ok())
        .and_then(|v| Vec::<Vec<u8>>::try_from(v).ok())?;
    let first = mounts.into_iter().next()?;
    let s = String::from_utf8_lossy(&first);
    let trimmed = s.trim_end_matches('\0').trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Poll for an existing mount (~4s), then fall back to mounting via udisks2.
fn ensure_mounted(conn: &Connection, fs_path: &str) -> Option<String> {
    for _ in 0..8 {
        if let Some(mp) = current_mountpoint(conn, fs_path) {
            return Some(mp);
        }
        thread::sleep(Duration::from_millis(500));
    }
    Proxy::new(conn, UDISKS, fs_path, FILESYSTEM)
        .ok()?
        .call::<_, _, String>("Mount", &(HashMap::<&str, zbus::zvariant::Value>::new()))
        .ok()
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_cards_vs_drives() {
        assert_eq!(classify("flash_sd", ""), DeviceKind::MemoryCard);
        assert_eq!(classify("flash_mmc", "sdio"), DeviceKind::MemoryCard);
        assert_eq!(classify("", "sdio"), DeviceKind::MemoryCard);
        assert_eq!(classify("thumb", "usb"), DeviceKind::RemovableDrive);
        assert_eq!(classify("", "usb"), DeviceKind::RemovableDrive);
    }

    #[test]
    fn decide_respects_master_and_per_type() {
        let on = Config {
            enabled: true,
            removable: Action::Open,
            memory_card: Action::Ask,
        };
        assert_eq!(decide(&on, DeviceKind::RemovableDrive), Action::Open);
        assert_eq!(decide(&on, DeviceKind::MemoryCard), Action::Ask);
        // Master off ⇒ nothing, whatever the per-type setting.
        let off = Config {
            enabled: false,
            ..on
        };
        assert_eq!(decide(&off, DeviceKind::RemovableDrive), Action::Nothing);
        assert_eq!(decide(&off, DeviceKind::MemoryCard), Action::Nothing);
    }

    #[test]
    fn action_keys_parse() {
        assert_eq!(Action::from_key("open"), Action::Open);
        assert_eq!(Action::from_key("ask"), Action::Ask);
        assert_eq!(Action::from_key("nothing"), Action::Nothing);
        assert_eq!(Action::from_key("garbage"), Action::Open); // default
    }
}
