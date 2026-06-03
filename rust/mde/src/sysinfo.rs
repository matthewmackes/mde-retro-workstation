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

// --- storage usage (Settings ▸ System ▸ Storage, E17.3) --------------------

/// One mounted filesystem's space, in bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    pub source: String, // device node
    pub target: String, // mountpoint
    pub total: u64,
    pub used: u64,
    pub avail: u64,
}

/// A Win10-style storage breakdown category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Apps,
    Documents,
    Pictures,
    Videos,
    Temporary,
    System,
}

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Category::Apps => "Apps & features",
            Category::Documents => "Documents",
            Category::Pictures => "Pictures",
            Category::Videos => "Videos",
            Category::Temporary => "Temporary files",
            Category::System => "System & reserved",
        }
    }
}

/// The full breakdown: real-device mounts + per-category bytes on the root device.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StorageUsage {
    pub mounts: Vec<Mount>,
    pub categories: Vec<(Category, u64)>,
}

/// Parse `df -B1 --output=source,size,used,avail,target` (a header line then one
/// row per filesystem). Pure — unit-tested. The mountpoint is the rest of the line,
/// so paths with spaces survive.
pub fn parse_df(out: &str) -> Vec<Mount> {
    out.lines()
        .skip(1) // header row
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let source = it.next()?.to_string();
            let total: u64 = it.next()?.parse().ok()?;
            let used: u64 = it.next()?.parse().ok()?;
            let avail: u64 = it.next()?.parse().ok()?;
            let target = it.collect::<Vec<_>>().join(" ");
            (!target.is_empty()).then_some(Mount {
                source,
                target,
                total,
                used,
                avail,
            })
        })
        .collect()
}

/// Sum the byte sizes from `rpm -qa --qf '%{SIZE}\n'`. Pure — unit-tested.
pub fn parse_rpm_sizes(out: &str) -> u64 {
    out.lines()
        .filter_map(|l| l.trim().parse::<u64>().ok())
        .sum()
}

/// Real block-device mounts (drop pseudo/virtual filesystems by keeping only
/// `/dev/*` sources), via `df`.
pub fn mounts() -> Vec<Mount> {
    let out =
        cmd_line("df", &["-B1", "--output=source,size,used,avail,target"]).unwrap_or_default();
    parse_df(&out)
        .into_iter()
        .filter(|m| m.source.starts_with("/dev/"))
        .collect()
}

/// `du -sb <path>` in bytes; 0 when the directory is missing or unreadable.
fn dir_bytes(path: &str) -> u64 {
    cmd_line("du", &["-sb", path])
        .and_then(|s| s.split_whitespace().next()?.parse().ok())
        .unwrap_or(0)
}

/// Total installed-package footprint, via `rpm`.
fn apps_bytes() -> u64 {
    parse_rpm_sizes(&cmd_line("rpm", &["-qa", "--qf", "%{SIZE}\n"]).unwrap_or_default())
}

fn home_dir(sub: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    format!("{home}/{sub}")
}

/// Per-category bytes on the root device. `System` is the remainder of the root
/// `used` after the measurable categories (never negative).
pub fn category_bytes(root_used: u64) -> Vec<(Category, u64)> {
    let apps = apps_bytes();
    let docs = dir_bytes(&home_dir("Documents"));
    let pics = dir_bytes(&home_dir("Pictures"));
    let vids = dir_bytes(&home_dir("Videos"));
    let temp = dir_bytes(&home_dir(".cache"));
    let counted = apps
        .saturating_add(docs)
        .saturating_add(pics)
        .saturating_add(vids)
        .saturating_add(temp);
    let system = root_used.saturating_sub(counted);
    vec![
        (Category::Apps, apps),
        (Category::Documents, docs),
        (Category::Pictures, pics),
        (Category::Videos, vids),
        (Category::Temporary, temp),
        (Category::System, system),
    ]
}

/// The live storage breakdown for the root device.
pub fn storage_usage() -> StorageUsage {
    let mounts = mounts();
    let root_used = mounts
        .iter()
        .find(|m| m.target == "/")
        .map(|m| m.used)
        .unwrap_or(0);
    StorageUsage {
        categories: category_bytes(root_used),
        mounts,
    }
}

/// Human-readable bytes (binary units), e.g. "1.5 GB".
pub fn human_bytes(b: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{b} B")
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}

/// `mde settings storage --list` — print the live breakdown.
pub fn print_storage_list() {
    let u = storage_usage();
    println!("Drives:");
    for m in &u.mounts {
        let pct = if m.total > 0 {
            (m.used as f64 / m.total as f64 * 100.0).round() as u64
        } else {
            0
        };
        println!(
            "  {:<14} {} used / {} total ({pct}% full) on {}",
            m.source,
            human_bytes(m.used),
            human_bytes(m.total),
            m.target,
        );
    }
    println!("Categories (root device):");
    for (cat, bytes) in &u.categories {
        println!("  {:<18} {}", cat.label(), human_bytes(*bytes));
    }
}

/// The Storage Sense `--user` units (service, timer) as file contents (E17.4). The
/// cleanup is inline in the service (purge the thumbnail cache + Trash), so the
/// timer is self-contained — it does not depend on E17.5's richer `--clean-now`.
/// Pure (unit-tested). `exe` is the absolute `mde` path (unused now, reserved).
pub fn storage_sense_units() -> (String, String) {
    let service = "[Unit]\n\
        Description=MackesDE Storage Sense cleanup\n\n\
        [Service]\n\
        Type=oneshot\n\
        ExecStart=/bin/sh -c 'rm -rf \"$HOME\"/.cache/thumbnails/* \"$HOME\"/.local/share/Trash/files/* \"$HOME\"/.local/share/Trash/info/* 2>/dev/null; true'\n"
        .to_string();
    let timer = "[Unit]\n\
        Description=MackesDE Storage Sense schedule\n\n\
        [Timer]\n\
        OnCalendar=weekly\n\
        Persistent=true\n\n\
        [Install]\n\
        WantedBy=timers.target\n"
        .to_string();
    (service, timer)
}

/// Write the Storage Sense units and enable/disable the `--user` timer (E17.4).
/// Best-effort (a session without `systemctl --user` just persists the preference).
pub fn apply_storage_sense(on: bool) {
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let dir = std::path::PathBuf::from(home).join(".config/systemd/user");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let (service, timer) = storage_sense_units();
    let _ = std::fs::write(dir.join("mde-storage-sense.service"), service);
    let _ = std::fs::write(dir.join("mde-storage-sense.timer"), timer);
    let _ = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status();
    let verb = if on { "enable" } else { "disable" };
    let _ = Command::new("systemctl")
        .args(["--user", verb, "--now", "mde-storage-sense.timer"])
        .status();
}

/// Root-device used bytes (for measuring freed space).
fn root_used() -> u64 {
    mounts()
        .iter()
        .find(|m| m.target == "/")
        .map(|m| m.used)
        .unwrap_or(0)
}

/// Remove the *contents* of a directory, keeping the directory itself.
fn purge_dir_contents(path: &str) {
    if let Ok(rd) = std::fs::read_dir(path) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                let _ = std::fs::remove_dir_all(&p);
            } else {
                let _ = std::fs::remove_file(&p);
            }
        }
    }
}

/// Free up space (E17.5): purge the thumbnail cache + Trash (user-level, no root),
/// then `dnf clean all` + `journalctl --vacuum-time` via `pkexec`. Returns the
/// bytes freed (root-device df delta, floored at the user-level purge estimate).
///
/// With `dry == true` it deletes nothing and runs **no** privileged step — it just
/// returns the would-free estimate, so the CI/bench path needs no root.
pub fn clean_now(dry: bool) -> u64 {
    let thumbs = home_dir(".cache/thumbnails");
    let trash = home_dir(".local/share/Trash");
    let estimate = dir_bytes(&thumbs) + dir_bytes(&trash);
    if dry {
        return estimate;
    }
    let before = root_used();
    // User-level purge (no root needed).
    let _ = std::fs::remove_dir_all(&thumbs);
    purge_dir_contents(&format!("{trash}/files"));
    purge_dir_contents(&format!("{trash}/info"));
    // System-level reclaim (root) — one pkexec prompt, best-effort.
    let _ = Command::new("pkexec")
        .args(["sh", "-c", "dnf clean all; journalctl --vacuum-time=7d"])
        .status();
    let after = root_used();
    before.saturating_sub(after).max(estimate)
}

/// `timeshift --snapshot-device <dev>` — set the Timeshift backup device (E17.6).
/// Returns the argv (run via `pkexec`). Pure.
pub fn timeshift_device_cmd(dev: &str) -> Vec<String> {
    vec!["timeshift".into(), "--snapshot-device".into(), dev.into()]
}

/// `timeshift --create` — make a snapshot now (E17.7). Returns the argv (`pkexec`).
pub fn timeshift_create_cmd() -> Vec<String> {
    vec!["timeshift".into(), "--create".into()]
}

/// One Timeshift snapshot (a restore point), E17.8.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub name: String, // the `YYYY-MM-DD_HH-MM-SS` id
    pub desc: String, // optional comment
}

/// Parse `timeshift --list`'s snapshot rows: `Num > YYYY-MM-DD_HH-MM-SS Tags Desc`.
/// Pure — unit-tested. Header/separator lines are skipped.
pub fn parse_snapshots(out: &str) -> Vec<Snapshot> {
    out.lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            // A snapshot row starts with the index number.
            it.next().filter(|n| n.parse::<u32>().is_ok())?;
            let mut rest: Vec<&str> = it.collect();
            if rest.first() == Some(&">") {
                rest.remove(0);
            }
            let name = rest.first()?.to_string();
            // The id is a timestamp; skip anything else (e.g. stray lines).
            if !name.contains('-') || !name.contains('_') {
                return None;
            }
            // rest[0]=name, rest[1]=tags, rest[2..]=description.
            let desc = rest.get(2..).map(|s| s.join(" ")).unwrap_or_default();
            Some(Snapshot { name, desc })
        })
        .collect()
}

/// `timeshift --restore --snapshot <name> --yes` — restore a snapshot to the
/// original location (E17.8). Returns the argv (run via `pkexec`).
pub fn timeshift_restore_cmd(name: &str) -> Vec<String> {
    vec![
        "timeshift".into(),
        "--restore".into(),
        "--snapshot".into(),
        name.into(),
        "--yes".into(),
    ]
}

/// Restore a snapshot to a **different** target device — `timeshift --restore
/// --snapshot <name> --target-device <dev> --yes` (E17.8a). Returns the `pkexec`
/// argv.
pub fn timeshift_restore_to_cmd(name: &str, dev: &str) -> Vec<String> {
    vec![
        "timeshift".into(),
        "--restore".into(),
        "--snapshot".into(),
        name.into(),
        "--target-device".into(),
        dev.into(),
        "--yes".into(),
    ]
}

/// The Timeshift snapshots, newest first. `MDE_TIMESHIFT_FIXTURE` (a file of
/// `timeshift --list` output) overrides for deterministic captures; otherwise
/// `timeshift --list` is run (it needs root, so unprivileged sessions get an empty
/// list — the browser then offers a privileged refresh).
pub fn snapshots() -> Vec<Snapshot> {
    if let Ok(path) = std::env::var("MDE_TIMESHIFT_FIXTURE") {
        if let Ok(s) = std::fs::read_to_string(path) {
            let mut v = parse_snapshots(&s);
            v.reverse();
            return v;
        }
    }
    let out = Command::new("timeshift")
        .arg("--list")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    let mut v = parse_snapshots(&out);
    v.reverse();
    v
}

/// The backup-schedule `--user` units (service, timer) for an `OnCalendar=` value
/// (E17.7). The service runs `mde settings backup --backup-now` (which pkexecs
/// timeshift). Pure (unit-tested). Unattended runs need a polkit rule for
/// timeshift; the manual "Back up now" works with the password prompt.
pub fn backup_schedule_units(oncalendar: &str) -> (String, String) {
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_else(|| "mde".into());
    let service = format!(
        "[Unit]\nDescription=MackesDE scheduled backup\n\n[Service]\nType=oneshot\nExecStart={exe} settings backup --backup-now\n"
    );
    let timer = format!(
        "[Unit]\nDescription=MackesDE backup schedule\n\n[Timer]\nOnCalendar={oncalendar}\nPersistent=true\n\n[Install]\nWantedBy=timers.target\n"
    );
    (service, timer)
}

/// Write the backup-schedule units and enable the `--user` timer (E17.7).
pub fn apply_backup_schedule(oncalendar: &str) {
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let dir = std::path::PathBuf::from(home).join(".config/systemd/user");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let (service, timer) = backup_schedule_units(oncalendar);
    let _ = std::fs::write(dir.join("mde-backup.service"), service);
    let _ = std::fs::write(dir.join("mde-backup.timer"), timer);
    let _ = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status();
    let _ = Command::new("systemctl")
        .args(["--user", "enable", "--now", "mde-backup.timer"])
        .status();
}

// --- recovery (Settings ▸ Update & Security ▸ Recovery, E17.9) --------------

/// "Reset this PC — keep my files": resync every package to the release (repairs a
/// broken system without touching `/home`). Returns the `pkexec` argv. Pure.
pub fn reset_keep_cmd() -> Vec<String> {
    vec!["dnf".into(), "distro-sync".into(), "-y".into()]
}

/// "Restart now" into the firmware/UEFI setup — `systemctl reboot
/// --firmware-setup`. Returns the `pkexec` argv. Pure.
pub fn restart_firmware_cmd() -> Vec<String> {
    vec![
        "systemctl".into(),
        "reboot".into(),
        "--firmware-setup".into(),
    ]
}

/// "Uninstall updates" — roll back the last dnf transaction (`dnf history undo
/// last`). Returns the `pkexec` argv. Pure.
pub fn uninstall_updates_cmd() -> Vec<String> {
    vec![
        "dnf".into(),
        "history".into(),
        "undo".into(),
        "last".into(),
        "-y".into(),
    ]
}

/// The shipped recovery/rescue image written to a USB drive (E17.10).
pub const RECOVERY_ISO: &str = "/usr/share/mde/recovery.iso";

/// "Create a recovery drive" — image the rescue ISO onto `device` with `dd` (E17.10).
/// Returns the `pkexec` argv. Pure. (`dd` is always present; `livecd-iso-to-disk`
/// isn't, so the raw image write is the portable choice.)
pub fn recovery_drive_cmd(device: &str) -> Vec<String> {
    vec![
        "dd".into(),
        format!("if={RECOVERY_ISO}"),
        format!("of={device}"),
        "bs=4M".into(),
        "status=progress".into(),
        "oflag=sync".into(),
    ]
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
    fn df_parses_rows_and_keeps_spaced_mountpoints() {
        let out = "\
Filesystem        1B-blocks         Used        Avail Mounted on
/dev/nvme0n1p3  500000000000 200000000000 300000000000 /
/dev/nvme0n1p1     500000000     50000000    450000000 /boot/efi
tmpfs             16000000000            0  16000000000 /run/user/1000
/dev/sda1       1000000000000 400000000000 600000000000 /mnt/My Backup
";
        let m = parse_df(out);
        assert_eq!(m.len(), 4);
        assert_eq!(m[0].source, "/dev/nvme0n1p3");
        assert_eq!(m[0].target, "/");
        assert_eq!(m[0].total, 500_000_000_000);
        assert_eq!(m[0].used, 200_000_000_000);
        // A mountpoint with a space survives.
        assert_eq!(m[3].target, "/mnt/My Backup");
    }

    #[test]
    fn rpm_sizes_sum_and_ignore_junk() {
        assert_eq!(parse_rpm_sizes("100\n200\n\n4096\n"), 4396);
        assert_eq!(parse_rpm_sizes("(none)\nabc\n50\n"), 50);
        assert_eq!(parse_rpm_sizes(""), 0);
    }

    #[test]
    fn human_bytes_scales() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1536), "1.5 KB");
        assert_eq!(human_bytes(5_368_709_120), "5.0 GB");
    }

    #[test]
    fn recovery_commands_shaped() {
        assert_eq!(reset_keep_cmd(), vec!["dnf", "distro-sync", "-y"]);
        assert_eq!(
            restart_firmware_cmd(),
            vec!["systemctl", "reboot", "--firmware-setup"]
        );
        assert_eq!(
            uninstall_updates_cmd(),
            vec!["dnf", "history", "undo", "last", "-y"]
        );
        assert_eq!(
            recovery_drive_cmd("/dev/sdb"),
            vec![
                "dd",
                "if=/usr/share/mde/recovery.iso",
                "of=/dev/sdb",
                "bs=4M",
                "status=progress",
                "oflag=sync"
            ]
        );
    }

    #[test]
    fn timeshift_commands_and_schedule_units() {
        assert_eq!(
            timeshift_device_cmd("/dev/sdb1"),
            vec!["timeshift", "--snapshot-device", "/dev/sdb1"]
        );
        assert_eq!(timeshift_create_cmd(), vec!["timeshift", "--create"]);
        let (service, timer) = backup_schedule_units("hourly");
        assert!(service.contains("settings backup --backup-now"));
        assert!(timer.contains("OnCalendar=hourly"));
        assert!(timer.contains("WantedBy=timers.target"));
    }

    #[test]
    fn snapshots_parse_and_restore_command() {
        let out = "\
Device : /dev/sda3
Num     Name                 Tags  Description
------------------------------------------------------------------------------
0    >  2024-01-15_10-00-01  O
1    >  2024-01-16_09-30-00  O     Before kernel update
";
        let s = parse_snapshots(out);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].name, "2024-01-15_10-00-01");
        assert_eq!(s[0].desc, "");
        assert_eq!(s[1].name, "2024-01-16_09-30-00");
        assert_eq!(s[1].desc, "Before kernel update");
        assert_eq!(
            timeshift_restore_cmd("2024-01-15_10-00-01"),
            vec![
                "timeshift",
                "--restore",
                "--snapshot",
                "2024-01-15_10-00-01",
                "--yes"
            ]
        );
        assert_eq!(
            timeshift_restore_to_cmd("2024-01-15_10-00-01", "/dev/sdb1"),
            vec![
                "timeshift",
                "--restore",
                "--snapshot",
                "2024-01-15_10-00-01",
                "--target-device",
                "/dev/sdb1",
                "--yes"
            ]
        );
    }

    #[test]
    fn storage_sense_units_are_self_contained() {
        let (service, timer) = storage_sense_units();
        // Service purges thumbnails + Trash inline (no dependency on --clean-now).
        assert!(service.contains("ExecStart="));
        assert!(service.contains(".cache/thumbnails"));
        assert!(service.contains(".local/share/Trash"));
        assert!(!service.contains("--clean-now"));
        // Timer is a weekly, install-able systemd timer.
        assert!(timer.contains("OnCalendar=weekly"));
        assert!(timer.contains("WantedBy=timers.target"));
    }

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
