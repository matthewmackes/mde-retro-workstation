//! Parse-only NetworkManager / rfkill / proxy helpers (E15) — the data layer the
//! Win10 Network flyout (E15.2) and Settings ▸ Network pages (E15.5+) read.
//!
//! This module ships the **readers** (Wi-Fi list, active connections, VPN list,
//! per-device byte counters, proxy mode), each a thin `nmcli`/sysfs/`gsettings`
//! call over a **pure parser** (unit-tested on fixtures). The *action* helpers
//! (connect, airplane, hotspot, vpn up/down, proxy set) land with their UI
//! consumers (E15.4/7/8/9/10) so nothing ships unreachable (§3).
//!
//! `mde __nm-list` prints the readers (the bench hook + reachability).

use std::process::Command;

/// One scanned Wi-Fi network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wifi {
    pub ssid: String,
    pub signal: u8,
    pub secured: bool,
}

/// One NetworkManager connection (active list / VPN list).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conn {
    pub name: String,
    pub kind: String,
    pub device: String,
    pub state: String,
}

/// Split one `nmcli -t` (terse) line into fields: `:`-separated, with literal
/// colons escaped as `\:` and backslashes as `\\` (nmcli's terse escaping).
fn split_terse(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut it = line.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\\' => {
                if let Some(n) = it.next() {
                    cur.push(n); // un-escape \: and \\
                }
            }
            ':' => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// Parse `nmcli -t -f SSID,SIGNAL,SECURITY dev wifi` into networks. Blank SSIDs
/// (hidden networks) are dropped; `--`/empty SECURITY means open.
pub fn parse_wifi(out: &str) -> Vec<Wifi> {
    let mut v = Vec::new();
    for line in out.lines() {
        let f = split_terse(line);
        if f.len() < 3 || f[0].is_empty() {
            continue;
        }
        v.push(Wifi {
            ssid: f[0].clone(),
            signal: f[1].trim().parse().unwrap_or(0),
            secured: !f[2].is_empty() && f[2] != "--",
        });
    }
    v
}

/// Parse `nmcli -t -f NAME,TYPE,DEVICE,STATE connection show [--active]`.
pub fn parse_connections(out: &str) -> Vec<Conn> {
    let mut v = Vec::new();
    for line in out.lines() {
        let f = split_terse(line);
        if f.len() < 4 || f[0].is_empty() {
            continue;
        }
        v.push(Conn {
            name: f[0].clone(),
            kind: f[1].clone(),
            device: f[2].clone(),
            state: f[3].clone(),
        });
    }
    v
}

/// Whether a connection type is a VPN-ish tunnel (vpn / wireguard).
fn is_vpn(kind: &str) -> bool {
    let k = kind.to_lowercase();
    k.contains("vpn") || k.contains("wireguard")
}

fn nmcli(args: &[&str]) -> String {
    Command::new("nmcli")
        .args(args)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default()
}

/// Scanned Wi-Fi networks (`nmcli dev wifi`).
pub fn wifi_list() -> Vec<Wifi> {
    parse_wifi(&nmcli(&["-t", "-f", "SSID,SIGNAL,SECURITY", "dev", "wifi"]))
}

/// Active connections (`nmcli connection show --active`).
pub fn active_connections() -> Vec<Conn> {
    parse_connections(&nmcli(&[
        "-t",
        "-f",
        "NAME,TYPE,DEVICE,STATE",
        "connection",
        "show",
        "--active",
    ]))
}

/// All VPN/WireGuard connections (active or not), for the VPN page (E15.7).
pub fn vpn_list() -> Vec<Conn> {
    parse_connections(&nmcli(&[
        "-t",
        "-f",
        "NAME,TYPE,DEVICE,STATE",
        "connection",
        "show",
    ]))
    .into_iter()
    .filter(|c| is_vpn(&c.kind))
    .collect()
}

/// Per-interface (rx, tx) byte counters from sysfs — the Data-usage page (E15.11).
pub fn device_bytes(dev: &str) -> (u64, u64) {
    let read = |kind: &str| {
        std::fs::read_to_string(format!("/sys/class/net/{dev}/statistics/{kind}"))
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0)
    };
    (read("rx_bytes"), read("tx_bytes"))
}

/// Parse saved Wi-Fi connection names from `nmcli -t -f NAME,TYPE connection show`
/// (TYPE is a wireless type). The Wi-Fi page's auto-connect toggle targets these.
pub fn parse_saved_wifi(out: &str) -> Vec<String> {
    out.lines()
        .filter_map(|l| {
            let f = split_terse(l);
            (f.len() >= 2 && !f[0].is_empty() && f[1].contains("wireless")).then(|| f[0].clone())
        })
        .collect()
}

/// Saved Wi-Fi connection names (`nmcli connection show`).
pub fn saved_wifi() -> Vec<String> {
    parse_saved_wifi(&nmcli(&["-t", "-f", "NAME,TYPE", "connection", "show"]))
}

/// Whether saved Wi-Fi auto-connects (reads the first saved network's
/// `connection.autoconnect`; default on when none are saved) — the Wi-Fi page (E15.6).
pub fn wifi_autoconnect() -> bool {
    saved_wifi()
        .first()
        .map(|n| nmcli(&["-g", "connection.autoconnect", "connection", "show", n]).trim() == "yes")
        .unwrap_or(true)
}

/// Forget (delete) a saved connection by name (`nmcli connection delete`), for the
/// Wi-Fi page's Forget action (E15.6a). Best-effort.
pub fn forget_wifi(name: &str) -> bool {
    Command::new("nmcli")
        .args(["connection", "delete", name])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Set `connection.autoconnect` on every saved Wi-Fi network (E15.6). Best-effort.
pub fn set_wifi_autoconnect(on: bool) {
    let v = if on { "yes" } else { "no" };
    for n in saved_wifi() {
        let _ = Command::new("nmcli")
            .args(["connection", "modify", &n, "connection.autoconnect", v])
            .status();
    }
}

/// A connection's firewalld zone (`nmcli -g connection.zone connection show <name>`)
/// — the Network status page's Private/Public profile (E15.5).
pub fn connection_zone(name: &str) -> String {
    nmcli(&["-g", "connection.zone", "connection", "show", name])
        .trim()
        .to_string()
}

/// Set a connection's firewalld zone (Private↔Public toggle, E15.5). Best-effort.
pub fn set_zone(name: &str, zone: &str) -> bool {
    Command::new("nmcli")
        .args(["connection", "modify", name, "connection.zone", zone])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn gget(schema: &str, key: &str) -> String {
    Command::new("gsettings")
        .args(["get", schema, key])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().trim_matches('\'').to_string())
        .unwrap_or_default()
}

fn gset(schema: &str, key: &str, value: &str) -> bool {
    Command::new("gsettings")
        .args(["set", schema, key, value])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Per-interface `(name, rx, tx)` byte counters for every non-loopback device, for
/// the Data-usage page (E15.11). Sorted by name.
pub fn all_device_bytes() -> Vec<(String, u64, u64)> {
    let mut v = Vec::new();
    if let Ok(rd) = std::fs::read_dir("/sys/class/net") {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name == "lo" {
                continue;
            }
            let (rx, tx) = device_bytes(&name);
            v.push((name, rx, tx));
        }
    }
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

/// The GNOME proxy mode (`none`/`manual`/`auto`) — the Proxy page (E15.9) reads it.
pub fn proxy_mode() -> String {
    let m = gget("org.gnome.system.proxy", "mode");
    if m.is_empty() {
        "none".to_string()
    } else {
        m
    }
}

/// The manual HTTP proxy host + port (`gsettings`), for the Proxy page (E15.9).
pub fn proxy_http() -> (String, String) {
    (
        gget("org.gnome.system.proxy.http", "host"),
        gget("org.gnome.system.proxy.http", "port"),
    )
}

/// Set the GNOME proxy mode (`none`/`manual`/`auto`) — E15.9. Best-effort.
pub fn set_proxy_mode(mode: &str) -> bool {
    gset("org.gnome.system.proxy", "mode", mode)
}

/// Set the manual HTTP proxy host + port (E15.9). Best-effort.
pub fn set_proxy_http(host: &str, port: &str) -> bool {
    let p = if port.is_empty() { "0" } else { port };
    gset("org.gnome.system.proxy.http", "host", host)
        && gset("org.gnome.system.proxy.http", "port", p)
}

/// Whether the Wi-Fi radio is on (`nmcli radio wifi`).
pub fn wifi_enabled() -> bool {
    nmcli(&["-t", "radio", "wifi"]).trim() == "enabled"
}

/// Parse `rfkill list` — airplane mode ≈ every radio soft-blocked.
fn parse_airplane(out: &str) -> bool {
    let soft: Vec<bool> = out
        .lines()
        .filter_map(|l| {
            l.trim()
                .strip_prefix("Soft blocked:")
                .map(|v| v.trim() == "yes")
        })
        .collect();
    !soft.is_empty() && soft.iter().all(|&b| b)
}

/// Whether airplane mode (all radios soft-blocked) is on (`rfkill list`).
pub fn airplane_on() -> bool {
    parse_airplane(
        &Command::new("rfkill")
            .arg("list")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_default(),
    )
}

// --- action setters (consumed by the Network flyout, E15.2) -----------------

/// Connect to a Wi-Fi network (E15.4): `nmcli device wifi connect <ssid>
/// [password <key>]`. An empty key connects to an open network. Best-effort.
pub fn wifi_connect(ssid: &str, password: &str) -> bool {
    let mut args = vec!["device", "wifi", "connect", ssid];
    if !password.is_empty() {
        args.push("password");
        args.push(password);
    }
    Command::new("nmcli")
        .args(&args)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Bring a VPN/connection up or down (`nmcli connection up|down <name>`), for the
/// VPN page (E15.7). Best-effort.
pub fn vpn_up_down(name: &str, up: bool) -> bool {
    Command::new("nmcli")
        .args(["connection", if up { "up" } else { "down" }, name])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Turn the Wi-Fi radio on/off (`nmcli radio wifi on|off`). Best-effort.
pub fn radio_wifi(on: bool) -> bool {
    Command::new("nmcli")
        .args(["radio", "wifi", if on { "on" } else { "off" }])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Start/stop a Wi-Fi hotspot (E15.8): `nmcli device wifi hotspot [ssid <n>
/// password <p>]` to share the connection, or `nmcli connection down Hotspot` to
/// stop it. Best-effort.
pub fn set_hotspot(on: bool, ssid: &str, password: &str) -> bool {
    if on {
        let mut args = vec!["device", "wifi", "hotspot"];
        if !ssid.is_empty() {
            args.push("ssid");
            args.push(ssid);
        }
        if !password.is_empty() {
            args.push("password");
            args.push(password);
        }
        Command::new("nmcli")
            .args(&args)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    } else {
        Command::new("nmcli")
            .args(["connection", "down", "Hotspot"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

/// Block/unblock all radios — airplane mode (`rfkill block|unblock all`).
pub fn set_airplane(on: bool) -> bool {
    Command::new("rfkill")
        .args([if on { "block" } else { "unblock" }, "all"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// `mde __nm-list` — print the readers (the E15.1 bench hook + reachability).
pub fn debug_list() {
    println!("Wi-Fi networks:");
    for w in wifi_list() {
        let lock = if w.secured { "🔒" } else { "  " };
        println!("  {lock} {:3}%  {}", w.signal, w.ssid);
    }
    println!("Active connections:");
    for c in active_connections() {
        let (rx, tx) = device_bytes(&c.device);
        println!(
            "  {} [{}] on {} — {} (rx {rx} / tx {tx} B)",
            c.name, c.kind, c.device, c.state
        );
    }
    println!("VPNs:");
    for c in vpn_list() {
        println!("  {} [{}] {}", c.name, c.kind, c.state);
    }
    println!("Proxy mode: {}", proxy_mode());
    println!(
        "Wi-Fi radio on: {}  ·  Airplane: {}",
        wifi_enabled(),
        airplane_on()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_terse_unescapes_colons_and_backslashes() {
        assert_eq!(split_terse("a:b:c"), vec!["a", "b", "c"]);
        // An SSID with a literal colon (escaped \:) stays one field.
        assert_eq!(
            split_terse(r"My\:Net:72:WPA2"),
            vec!["My:Net", "72", "WPA2"]
        );
        assert_eq!(split_terse(r"a\\b:c"), vec![r"a\b", "c"]);
    }

    #[test]
    fn parse_wifi_reads_ssid_signal_security() {
        let out = "HomeNet:88:WPA2\nGuest:40:\nCafe\\:Wifi:65:WPA1 WPA2\n:0:WPA2\n";
        let w = parse_wifi(out);
        assert_eq!(w.len(), 3, "the blank-SSID hidden net is dropped");
        assert_eq!(
            w[0],
            Wifi {
                ssid: "HomeNet".into(),
                signal: 88,
                secured: true
            }
        );
        assert!(!w[1].secured, "empty SECURITY = open");
        assert_eq!(w[2].ssid, "Cafe:Wifi"); // escaped colon preserved
        assert_eq!(w[2].signal, 65);
    }

    #[test]
    fn parse_saved_wifi_picks_wireless_connections() {
        let out = "Wired connection 1:802-3-ethernet\nHomeNet:802-11-wireless\n\
                   MyVPN:vpn\nCafe:802-11-wireless\n";
        assert_eq!(parse_saved_wifi(out), vec!["HomeNet", "Cafe"]);
        assert!(parse_saved_wifi("").is_empty());
    }

    #[test]
    fn parse_airplane_needs_all_radios_blocked() {
        let all_blocked = "0: phy0: Wireless LAN\n\tSoft blocked: yes\n\tHard blocked: no\n\
                           1: hci0: Bluetooth\n\tSoft blocked: yes\n";
        let one_on = "0: phy0: Wireless LAN\n\tSoft blocked: no\n\
                      1: hci0: Bluetooth\n\tSoft blocked: yes\n";
        assert!(parse_airplane(all_blocked));
        assert!(!parse_airplane(one_on));
        assert!(!parse_airplane(""), "no radios = not airplane");
    }

    #[test]
    fn parse_connections_reads_four_fields() {
        let out = "Wired connection 1:802-3-ethernet:enp0s3:activated\n\
                   MyVPN:vpn:tun0:activated\n";
        let c = parse_connections(out);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].name, "Wired connection 1");
        assert_eq!(c[0].device, "enp0s3");
        assert!(is_vpn(&c[1].kind));
    }
}
