//! BlueZ data layer for the Settings ▸ Devices ▸ Bluetooth page (E12.2).
//!
//! Talks to `org.bluez` on the **system** bus via the blocking zbus API (the same
//! stack the tray uses, on the caller's thread / a `spawn_blocking` task — never the
//! iced UI thread). No iced dependency. Headless entry: `mde __bt-list`.

use std::collections::HashMap;

use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

const SVC: &str = "org.bluez";
const ADAPTER: &str = "org.bluez.Adapter1";
const DEVICE: &str = "org.bluez.Device1";
const OBJMGR: &str = "org.freedesktop.DBus.ObjectManager";

type Props = HashMap<String, OwnedValue>;
type Managed = HashMap<OwnedObjectPath, HashMap<String, Props>>;

/// One Bluetooth device (paired or just discovered).
#[derive(Debug, Clone, Default)]
pub struct BtDevice {
    pub name: String,
    pub address: String,
    pub path: String,
    pub paired: bool,
    pub connected: bool,
}

/// Snapshot of the first adapter + its known devices.
#[derive(Debug, Clone, Default)]
pub struct BtState {
    pub present: bool, // an adapter exists at all
    pub powered: bool,
    pub discovering: bool,
    pub devices: Vec<BtDevice>,
}

fn conn() -> Option<Connection> {
    Connection::system().ok()
}

fn managed(c: &Connection) -> Option<Managed> {
    Proxy::new(c, SVC, "/", OBJMGR)
        .ok()?
        .call("GetManagedObjects", &())
        .ok()
}

fn prop_str(p: &Props, key: &str) -> String {
    p.get(key)
        .and_then(|v| String::try_from(v.try_clone().ok()?).ok())
        .unwrap_or_default()
}

fn prop_bool(p: &Props, key: &str) -> bool {
    p.get(key)
        .and_then(|v| bool::try_from(v.try_clone().ok()?).ok())
        .unwrap_or(false)
}

/// Read the adapter + device list. Empty/default if BlueZ or an adapter is absent.
pub fn state() -> BtState {
    let Some(c) = conn() else {
        return BtState::default();
    };
    let Some(m) = managed(&c) else {
        return BtState::default();
    };
    let mut st = BtState::default();
    for ifaces in m.values() {
        if let Some(a) = ifaces.get(ADAPTER) {
            st.present = true;
            st.powered = prop_bool(a, "Powered");
            st.discovering = prop_bool(a, "Discovering");
        }
    }
    for (path, ifaces) in &m {
        if let Some(d) = ifaces.get(DEVICE) {
            let name = {
                let n = prop_str(d, "Name");
                if n.is_empty() {
                    prop_str(d, "Address")
                } else {
                    n
                }
            };
            st.devices.push(BtDevice {
                name,
                address: prop_str(d, "Address"),
                path: path.to_string(),
                paired: prop_bool(d, "Paired"),
                connected: prop_bool(d, "Connected"),
            });
        }
    }
    // Paired (then connected) first, then alphabetical.
    st.devices.sort_by(|a, b| {
        b.paired
            .cmp(&a.paired)
            .then(b.connected.cmp(&a.connected))
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    st
}

fn adapter_path() -> Option<String> {
    let c = conn()?;
    managed(&c)?
        .into_iter()
        .find(|(_, i)| i.contains_key(ADAPTER))
        .map(|(p, _)| p.to_string())
}

fn adapter_proxy(c: &Connection) -> Option<Proxy<'static>> {
    let p = adapter_path()?;
    Proxy::new(c, SVC, p, ADAPTER).ok()
}

/// Power the adapter on/off (toggles `Adapter1.Powered`).
pub fn set_powered(on: bool) {
    if let Some(c) = conn() {
        if let Some(a) = adapter_proxy(&c) {
            let _ = a.set_property("Powered", on);
        }
    }
}

/// Start/stop device discovery (the "+ Add a device" scan).
pub fn set_discovery(on: bool) {
    if let Some(c) = conn() {
        if let Some(a) = adapter_proxy(&c) {
            let method = if on {
                "StartDiscovery"
            } else {
                "StopDiscovery"
            };
            let _ = a.call::<_, _, ()>(method, &());
        }
    }
}

fn device_call(path: &str, method: &str) {
    if let Some(c) = conn() {
        if let Ok(d) = Proxy::new(&c, SVC, path, DEVICE) {
            let _ = d.call::<_, _, ()>(method, &());
        }
    }
}

pub fn pair(path: &str) {
    device_call(path, "Pair");
}
pub fn connect(path: &str) {
    device_call(path, "Connect");
}
pub fn disconnect(path: &str) {
    device_call(path, "Disconnect");
}

/// Forget a device (`Adapter1.RemoveDevice` with the device object path).
pub fn remove(path: &str) {
    if let Some(c) = conn() {
        if let Some(a) = adapter_proxy(&c) {
            if let Ok(op) = zbus::zvariant::ObjectPath::try_from(path) {
                let _ = a.call::<_, _, ()>("RemoveDevice", &op);
            }
        }
    }
}

/// Headless dump for `mde __bt-list`.
pub fn debug_list() {
    let st = state();
    println!(
        "adapter present={} powered={} discovering={}",
        st.present, st.powered, st.discovering
    );
    for d in st.devices {
        println!(
            "  {} [{}] paired={} connected={}",
            d.name, d.address, d.paired, d.connected
        );
    }
}
