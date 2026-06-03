//! Toolkit-agnostic system/hardware data layer for System Properties + the
//! (native iced) Device Manager — per the rank-25 decision: no GTK HardInfo2,
//! read the facts ourselves from `/proc`, `/sys`, `/etc/os-release` and the
//! standard CLI tools, parse them into plain structs, and let the GUI render.
//!
//! This module has NO iced dependency so the parsers are unit-tested directly
//! (the data is the ground truth; the dialog just displays it). Headless entry:
//!   mde system-properties --info     print the General-tab facts
//!   mde system-properties --devices  print the Device Manager category tree

use std::process::{Command, ExitCode};

/// The System Properties "General" tab facts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct General {
    pub product: String,   // os-release NAME
    pub version: String,   // os-release VERSION
    pub kernel: String,    // uname -r
    pub cpu: String,       // /proc/cpuinfo model name
    pub cores: usize,      // logical processors
    pub mem_total_kb: u64, // /proc/meminfo MemTotal
    pub hostname: String,
    pub user: String,
}

impl General {
    /// Human-readable RAM, e.g. "15.4 GB".
    pub fn mem_human(&self) -> String {
        let gb = self.mem_total_kb as f64 / (1024.0 * 1024.0);
        format!("{gb:.1} GB")
    }
}

/// Collect the General-tab facts from the live system.
pub fn general() -> General {
    let os = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    let (product, version) = parse_os_release(&os);
    let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let meminfo = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    General {
        product,
        version,
        kernel: cmd_line("uname", &["-r"]).unwrap_or_default(),
        cpu: parse_cpu_model(&cpuinfo).unwrap_or_else(|| "Unknown processor".into()),
        cores: count_cores(&cpuinfo),
        mem_total_kb: parse_meminfo_total(&meminfo),
        hostname: std::fs::read_to_string("/proc/sys/kernel/hostname")
            .unwrap_or_default()
            .trim()
            .to_string(),
        user: std::env::var("USER").unwrap_or_default(),
    }
}

/// Automatic-updates posture, read from the dnf-automatic timer + config.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AutoMode {
    #[default]
    Off,
    DownloadOnly,
    Install,
}

impl AutoMode {
    /// All postures, for a picker (E13.8).
    pub const ALL: [AutoMode; 3] = [AutoMode::Off, AutoMode::DownloadOnly, AutoMode::Install];
}
impl std::fmt::Display for AutoMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            AutoMode::Off => "Off",
            AutoMode::DownloadOnly => "Download only, notify before installing",
            AutoMode::Install => "Install automatically",
        })
    }
}

/// Read the live automatic-updates posture from the dnf-automatic timer +
/// `automatic.conf` (E13.8) — the authoritative source both the System Properties
/// radios and the Win10 Update page read, so they agree without a duplicated store.
pub fn auto_mode() -> AutoMode {
    let timer_enabled = cmd_line("systemctl", &["is-enabled", "dnf-automatic.timer"])
        .map(|s| s.trim() == "enabled")
        .unwrap_or(false);
    let applies = std::fs::read_to_string("/etc/dnf/automatic.conf")
        .unwrap_or_default()
        .lines()
        .any(|l| l.replace(' ', "").starts_with("apply_updates=yes"));
    match (timer_enabled, applies) {
        (false, _) => AutoMode::Off,
        (true, false) => AutoMode::DownloadOnly,
        (true, true) => AutoMode::Install,
    }
}

/// Live facts for the Advanced / System Restore / Automatic Updates / Remote
/// tabs. Read-only — the tabs display real system state (the Win2000 layout)
/// rather than pretending to change settings a preview shouldn't touch.
#[derive(Debug, Clone, Default)]
pub struct Advanced {
    pub swappiness: String,
    pub zram: String,
    pub grub_default: String,
    pub grub_timeout: String,
    pub env_count: usize,
    pub auto_updates: AutoMode,
    pub timeshift_installed: bool,
    pub remote_available: bool,
    pub remote_running: bool,
}

/// Whether a binary is resolvable on `$PATH`.
fn on_path(bin: &str) -> bool {
    cmd_line("sh", &["-c", &format!("command -v {bin}")])
        .map(|s| !s.is_empty())
        .unwrap_or(false)
}

/// Collect the Advanced/Updates/Remote/Restore facts (best-effort).
pub fn advanced() -> Advanced {
    let read = |p: &str| {
        std::fs::read_to_string(p)
            .unwrap_or_default()
            .trim()
            .to_string()
    };
    let grub = std::fs::read_to_string("/etc/default/grub").unwrap_or_default();
    let grub_val = |key: &str| {
        grub.lines()
            .find_map(|l| {
                l.trim()
                    .strip_prefix(key)
                    .map(|v| v.trim_matches(['=', '"', ' ']).to_string())
            })
            .unwrap_or_default()
    };
    let zram = cmd_line(
        "sh",
        &[
            "-c",
            "zramctl --noheadings --output DISKSIZE 2>/dev/null | head -1",
        ],
    )
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| "none".to_string());
    Advanced {
        swappiness: read("/proc/sys/vm/swappiness"),
        zram,
        grub_default: grub_val("GRUB_DEFAULT"),
        grub_timeout: grub_val("GRUB_TIMEOUT"),
        env_count: std::env::vars().count(),
        auto_updates: auto_mode(),
        timeshift_installed: on_path("timeshift") || on_path("timeshift-launcher"),
        remote_available: on_path("wayvnc"),
        remote_running: cmd_line("pgrep", &["-x", "wayvnc"])
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false),
    }
}

/// The privileged command that applies an automatic-updates posture (Apply on
/// System Properties ▸ Updates, and the Win10 Update page, E13). It sets BOTH
/// halves that [`advanced`] reads back — the dnf-automatic **timer** and
/// `automatic.conf`'s **`apply_updates`** — so Download-only (`no`) and Install
/// (`yes`) genuinely differ; toggling the timer alone can't distinguish them (the
/// prior Apply produced the same command for both). One `pkexec`; the `sed` form
/// matches the whitespace-stripped key `advanced` checks (`apply_updates=yes`).
pub fn set_auto_command(mode: AutoMode) -> String {
    match mode {
        AutoMode::Off => "pkexec systemctl disable --now dnf-automatic.timer".to_string(),
        AutoMode::DownloadOnly | AutoMode::Install => {
            let apply = if matches!(mode, AutoMode::Install) {
                "yes"
            } else {
                "no"
            };
            format!(
                "pkexec sh -c 'systemctl enable --now dnf-automatic.timer && \
                 sed -i -E \"s/^[[:space:]]*apply_updates[[:space:]]*=.*/apply_updates = {apply}/\" \
                 /etc/dnf/automatic.conf'"
            )
        }
    }
}

/// A Device Manager category and the devices under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCategory {
    pub name: &'static str,
    pub devices: Vec<String>,
}

/// Build the Device Manager tree from the standard probes (best-effort; a probe
/// that isn't installed simply yields an empty category).
pub fn devices() -> Vec<DeviceCategory> {
    vec![
        DeviceCategory {
            name: "Processors",
            devices: {
                let info = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
                let model = parse_cpu_model(&info).unwrap_or_else(|| "Processor".into());
                vec![model; count_cores(&info).max(1)]
            },
        },
        DeviceCategory {
            name: "Display adapters",
            devices: lspci_class("VGA"),
        },
        DeviceCategory {
            name: "Disk drives",
            devices: lsblk_disks(),
        },
        DeviceCategory {
            name: "Universal Serial Bus controllers",
            devices: lines_of("lsusb", &[]),
        },
    ]
}

// --- parsers (pure, unit-tested) -------------------------------------------

/// Extract `NAME` and `VERSION` from os-release contents.
pub fn parse_os_release(s: &str) -> (String, String) {
    let val = |key: &str| {
        s.lines()
            .find_map(|l| l.strip_prefix(key))
            .map(|v| v.trim_matches('"').to_string())
            .unwrap_or_default()
    };
    (val("NAME="), val("VERSION="))
}

/// First `model name` line from /proc/cpuinfo.
pub fn parse_cpu_model(s: &str) -> Option<String> {
    s.lines()
        .find_map(|l| l.split_once(':').filter(|(k, _)| k.trim() == "model name"))
        .map(|(_, v)| v.trim().to_string())
}

/// Count logical processors (number of `processor` records).
pub fn count_cores(s: &str) -> usize {
    s.lines()
        .filter(|l| {
            l.split_once(':')
                .map(|(k, _)| k.trim() == "processor")
                .unwrap_or(false)
        })
        .count()
}

/// `MemTotal` in kB from /proc/meminfo.
pub fn parse_meminfo_total(s: &str) -> u64 {
    s.lines()
        .find_map(|l| l.strip_prefix("MemTotal:"))
        .and_then(|v| v.split_whitespace().next())
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

// --- local accounts (Accounts ▸ Family & other users, E10.4) ---------------

/// A local user account for the Accounts pages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub name: String, // login name (passwd field 1)
    pub uid: u32,     // numeric UID
    pub full: String, // GECOS full name, falling back to the login name
    pub admin: bool,  // member of the `wheel` group → Administrator
}

/// Member login names of the `wheel` line in `/etc/group` content
/// (`wheel:x:10:alice,bob` → `[alice, bob]`). The trailing colon-field anchors it,
/// so a group merely *starting* "wheel" (none in practice) can't false-match.
pub fn parse_wheel(group: &str) -> Vec<String> {
    group
        .lines()
        .find_map(|l| l.strip_prefix("wheel:"))
        .map(|rest| {
            rest.rsplit(':')
                .next()
                .unwrap_or("")
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Parse `/etc/passwd` content into the *human* accounts: UID ≥ 1000 and not the
/// `nobody` sentinel (65534). `wheel` membership marks the admin badge.
pub fn parse_passwd(passwd: &str, wheel: &[String]) -> Vec<Account> {
    passwd
        .lines()
        .filter_map(|l| {
            let mut f = l.split(':');
            let name = f.next()?;
            let _pw = f.next()?;
            let uid: u32 = f.next()?.parse().ok()?;
            let _gid = f.next()?;
            let gecos = f.next().unwrap_or("");
            if uid < 1000 || uid == 65534 {
                return None;
            }
            let full = gecos.split(',').next().unwrap_or("").trim();
            Some(Account {
                name: name.to_string(),
                uid,
                full: if full.is_empty() {
                    name.to_string()
                } else {
                    full.to_string()
                },
                admin: wheel.iter().any(|w| w == name),
            })
        })
        .collect()
}

/// Live enumeration of human accounts from `/etc/passwd` + `/etc/group` (matches
/// `getent passwd` for the local files). Sorted by login name for a stable list.
/// The two paths are overridable via `MDE_PASSWD_PATH`/`MDE_GROUP_PATH` so the
/// gallery/preview harness can render a deterministic multi-account list from
/// real passwd-format fixture files (still real parsing, no demo data).
pub fn accounts() -> Vec<Account> {
    let passwd = std::env::var("MDE_PASSWD_PATH").unwrap_or_else(|_| "/etc/passwd".into());
    let group = std::env::var("MDE_GROUP_PATH").unwrap_or_else(|_| "/etc/group".into());
    let group = std::fs::read_to_string(group).unwrap_or_default();
    let passwd = std::fs::read_to_string(passwd).unwrap_or_default();
    let mut a = parse_passwd(&passwd, &parse_wheel(&group));
    a.sort_by(|x, y| x.name.cmp(&y.name));
    a
}

/// How many of these accounts are administrators (`wheel`).
pub fn admin_count(accounts: &[Account]) -> usize {
    accounts.iter().filter(|a| a.admin).count()
}

/// Coerce arbitrary text into a valid Linux login (E10.5 Add user): lower-case,
/// keep `[a-z0-9_-]`, must begin with a letter, capped at 32 chars. Empty when
/// nothing usable remains — the caller refuses to run `useradd` on an empty name.
pub fn sanitize_login(s: &str) -> String {
    let mut out = String::new();
    for c in s.trim().to_lowercase().chars() {
        let ok = c.is_ascii_lowercase()
            || c == '_'
            || (!out.is_empty() && (c.is_ascii_digit() || c == '-'));
        if ok {
            out.push(c);
        }
        if out.len() >= 32 {
            break;
        }
    }
    out
}

/// `useradd -m <name>` argv (E10.5). `-m` creates the home dir; the password is
/// set separately on Sign-in options (E10.6), so the account starts locked.
pub fn useradd_cmd(name: &str) -> Vec<String> {
    vec!["useradd".into(), "-m".into(), name.into()]
}

/// argv to grant (`usermod -aG wheel`) or revoke (`gpasswd -d … wheel`) admin (E10.5).
pub fn set_admin_cmd(name: &str, admin: bool) -> Vec<String> {
    if admin {
        vec!["usermod".into(), "-aG".into(), "wheel".into(), name.into()]
    } else {
        vec!["gpasswd".into(), "-d".into(), name.into(), "wheel".into()]
    }
}

/// `userdel -r <name>` argv (E10.5) — `-r` also removes the home dir and mail spool.
pub fn userdel_cmd(name: &str) -> Vec<String> {
    vec!["userdel".into(), "-r".into(), name.into()]
}

// --- CLI probes ------------------------------------------------------------

fn cmd_line(bin: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(bin).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn lines_of(bin: &str, args: &[&str]) -> Vec<String> {
    cmd_line(bin, args)
        .map(|s| {
            s.lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// `lspci` device descriptions whose class line contains `class` (e.g. "VGA").
fn lspci_class(class: &str) -> Vec<String> {
    lines_of("lspci", &[])
        .into_iter()
        .filter(|l| l.contains(class))
        .map(|l| l.split_once(' ').map(|x| x.1).unwrap_or(&l).to_string())
        .collect()
}

fn lsblk_disks() -> Vec<String> {
    lines_of("lsblk", &["-dno", "NAME,SIZE,MODEL"])
}

// --- headless entry point --------------------------------------------------

pub fn run(args: &[String]) -> ExitCode {
    let devices_flag = args.iter().any(|a| a == "--devices");
    if devices_flag {
        for cat in devices() {
            println!("{}", cat.name);
            for d in cat.devices {
                println!("    {d}");
            }
        }
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "--users") {
        for a in accounts() {
            let role = if a.admin { "Administrator" } else { "Standard" };
            println!("{:<16} uid={:<6} {role:<13} {}", a.name, a.uid, a.full);
        }
        return ExitCode::SUCCESS;
    }
    // Default / --info: the General-tab facts. (The GUI dialog is the next step.)
    let g = general();
    println!("System:    {} {}", g.product, g.version);
    println!("Kernel:    {}", g.kernel);
    println!("Computer:  {}", g.hostname);
    println!("Processor: {} ({} logical)", g.cpu, g.cores);
    println!("Memory:    {}", g.mem_human());
    println!("Registered to: {}", g.user);
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_auto_command_sets_timer_and_apply_updates() {
        // Off disables the timer; Download/Install enable it AND set apply_updates
        // (no/yes) so they actually differ — the bug was that they didn't (E13.1).
        assert!(set_auto_command(AutoMode::Off).contains("disable --now dnf-automatic.timer"));
        let d = set_auto_command(AutoMode::DownloadOnly);
        let i = set_auto_command(AutoMode::Install);
        assert!(d.contains("enable --now dnf-automatic.timer"));
        assert!(i.contains("enable --now dnf-automatic.timer"));
        assert!(d.contains("apply_updates = no"));
        assert!(i.contains("apply_updates = yes"));
        assert_ne!(d, i, "Download and Install must produce different commands");
    }

    const OS_RELEASE: &str =
        "NAME=\"Fedora Linux\"\nVERSION=\"44 (Workstation Edition)\"\nID=fedora\n";
    const CPUINFO: &str = "processor\t: 0\nmodel name\t: AMD Ryzen 7 5800X\ncores\t: 8\n\nprocessor\t: 1\nmodel name\t: AMD Ryzen 7 5800X\n";
    const MEMINFO: &str = "MemTotal:       16307712 kB\nMemFree:         1234567 kB\n";

    #[test]
    fn os_release_name_and_version() {
        assert_eq!(
            parse_os_release(OS_RELEASE),
            (
                "Fedora Linux".to_string(),
                "44 (Workstation Edition)".to_string()
            )
        );
    }

    #[test]
    fn cpu_model_and_core_count() {
        assert_eq!(
            parse_cpu_model(CPUINFO).as_deref(),
            Some("AMD Ryzen 7 5800X")
        );
        assert_eq!(count_cores(CPUINFO), 2);
    }

    #[test]
    fn meminfo_total_parsed() {
        assert_eq!(parse_meminfo_total(MEMINFO), 16_307_712);
        let g = General {
            mem_total_kb: 16_307_712,
            ..Default::default()
        };
        assert_eq!(g.mem_human(), "15.6 GB");
    }

    #[test]
    fn missing_fields_dont_panic() {
        assert_eq!(parse_os_release(""), (String::new(), String::new()));
        assert_eq!(parse_cpu_model(""), None);
        assert_eq!(count_cores(""), 0);
        assert_eq!(parse_meminfo_total(""), 0);
    }

    const GROUP: &str = "root:x:0:\nwheel:x:10:ada,grace\nusers:x:100:\nnobody:x:65534:\n";
    const PASSWD: &str = "root:x:0:0:root:/root:/bin/bash\n\
        daemon:x:2:2:daemon:/sbin:/usr/sbin/nologin\n\
        ada:x:1000:1000:Ada Lovelace,,,:/home/ada:/bin/bash\n\
        grace:x:1001:1001::/home/grace:/bin/bash\n\
        guest:x:1002:1002:Guest User:/home/guest:/bin/bash\n\
        nobody:x:65534:65534:Nobody:/:/usr/sbin/nologin\n";

    #[test]
    fn wheel_members_parsed() {
        assert_eq!(parse_wheel(GROUP), vec!["ada", "grace"]);
        assert!(parse_wheel("root:x:0:\nwheel:x:10:\n").is_empty());
        assert!(parse_wheel("").is_empty());
    }

    #[test]
    fn passwd_keeps_humans_and_badges_admins() {
        let users = parse_passwd(PASSWD, &parse_wheel(GROUP));
        // root/daemon (UID < 1000) and nobody (65534) are filtered out.
        let names: Vec<_> = users.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["ada", "grace", "guest"]);
        // GECOS full name, falling back to login when the field is empty.
        let ada = &users[0];
        assert_eq!(ada.full, "Ada Lovelace");
        assert!(ada.admin); // in wheel
        let grace = &users[1];
        assert_eq!(grace.full, "grace"); // empty GECOS → login name
        assert!(grace.admin);
        let guest = &users[2];
        assert_eq!(guest.full, "Guest User");
        assert!(!guest.admin); // not in wheel → Standard
    }

    #[test]
    fn admin_count_counts_wheel() {
        let users = parse_passwd(PASSWD, &parse_wheel(GROUP));
        assert_eq!(admin_count(&users), 2); // ada + grace
        assert_eq!(admin_count(&[]), 0);
    }

    #[test]
    fn sanitize_login_makes_valid_names() {
        assert_eq!(sanitize_login("Ada Lovelace"), "adalovelace");
        assert_eq!(sanitize_login("  Bob_99  "), "bob_99");
        assert_eq!(sanitize_login("9lives"), "lives"); // can't start with a digit
        assert_eq!(sanitize_login("-dash"), "dash"); // can't start with a dash
        assert_eq!(sanitize_login("!!!"), ""); // nothing usable
    }

    #[test]
    fn account_command_argv() {
        assert_eq!(useradd_cmd("bob"), ["useradd", "-m", "bob"]);
        assert_eq!(
            set_admin_cmd("bob", true),
            ["usermod", "-aG", "wheel", "bob"]
        );
        assert_eq!(
            set_admin_cmd("bob", false),
            ["gpasswd", "-d", "bob", "wheel"]
        );
        assert_eq!(userdel_cmd("bob"), ["userdel", "-r", "bob"]);
    }
}
