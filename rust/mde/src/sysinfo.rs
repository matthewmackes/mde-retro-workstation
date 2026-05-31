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
    cmd_line("sh", &["-c", &format!("command -v {bin}")]).map(|s| !s.is_empty()).unwrap_or(false)
}

/// Collect the Advanced/Updates/Remote/Restore facts (best-effort).
pub fn advanced() -> Advanced {
    let read = |p: &str| std::fs::read_to_string(p).unwrap_or_default().trim().to_string();
    let grub = std::fs::read_to_string("/etc/default/grub").unwrap_or_default();
    let grub_val = |key: &str| {
        grub.lines()
            .find_map(|l| l.trim().strip_prefix(key).map(|v| v.trim_matches(['=', '"', ' ']).to_string()))
            .unwrap_or_default()
    };
    let zram = cmd_line("sh", &["-c", "zramctl --noheadings --output DISKSIZE 2>/dev/null | head -1"])
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "none".to_string());
    let timer_enabled = cmd_line("systemctl", &["is-enabled", "dnf-automatic.timer"])
        .map(|s| s.trim() == "enabled")
        .unwrap_or(false);
    let applies = std::fs::read_to_string("/etc/dnf/automatic.conf")
        .unwrap_or_default()
        .lines()
        .any(|l| l.replace(' ', "").starts_with("apply_updates=yes"));
    let auto_updates = match (timer_enabled, applies) {
        (false, _) => AutoMode::Off,
        (true, false) => AutoMode::DownloadOnly,
        (true, true) => AutoMode::Install,
    };
    Advanced {
        swappiness: read("/proc/sys/vm/swappiness"),
        zram,
        grub_default: grub_val("GRUB_DEFAULT"),
        grub_timeout: grub_val("GRUB_TIMEOUT"),
        env_count: std::env::vars().count(),
        auto_updates,
        timeshift_installed: on_path("timeshift") || on_path("timeshift-launcher"),
        remote_available: on_path("wayvnc"),
        remote_running: cmd_line("pgrep", &["-x", "wayvnc"]).map(|s| !s.trim().is_empty()).unwrap_or(false),
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
    s.lines().filter(|l| l.split_once(':').map(|(k, _)| k.trim() == "processor").unwrap_or(false)).count()
}

/// `MemTotal` in kB from /proc/meminfo.
pub fn parse_meminfo_total(s: &str) -> u64 {
    s.lines()
        .find_map(|l| l.strip_prefix("MemTotal:"))
        .and_then(|v| v.trim().split_whitespace().next())
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
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
        .map(|s| s.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
        .unwrap_or_default()
}

/// `lspci` device descriptions whose class line contains `class` (e.g. "VGA").
fn lspci_class(class: &str) -> Vec<String> {
    lines_of("lspci", &[])
        .into_iter()
        .filter(|l| l.contains(class))
        .map(|l| l.splitn(2, ' ').nth(1).unwrap_or(&l).to_string())
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

    const OS_RELEASE: &str = "NAME=\"Fedora Linux\"\nVERSION=\"44 (Workstation Edition)\"\nID=fedora\n";
    const CPUINFO: &str = "processor\t: 0\nmodel name\t: AMD Ryzen 7 5800X\ncores\t: 8\n\nprocessor\t: 1\nmodel name\t: AMD Ryzen 7 5800X\n";
    const MEMINFO: &str = "MemTotal:       16307712 kB\nMemFree:         1234567 kB\n";

    #[test]
    fn os_release_name_and_version() {
        assert_eq!(
            parse_os_release(OS_RELEASE),
            ("Fedora Linux".to_string(), "44 (Workstation Edition)".to_string())
        );
    }

    #[test]
    fn cpu_model_and_core_count() {
        assert_eq!(parse_cpu_model(CPUINFO).as_deref(), Some("AMD Ryzen 7 5800X"));
        assert_eq!(count_cores(CPUINFO), 2);
    }

    #[test]
    fn meminfo_total_parsed() {
        assert_eq!(parse_meminfo_total(MEMINFO), 16_307_712);
        let g = General { mem_total_kb: 16_307_712, ..Default::default() };
        assert_eq!(g.mem_human(), "15.6 GB");
    }

    #[test]
    fn missing_fields_dont_panic() {
        assert_eq!(parse_os_release(""), (String::new(), String::new()));
        assert_eq!(parse_cpu_model(""), None);
        assert_eq!(count_cores(""), 0);
        assert_eq!(parse_meminfo_total(""), 0);
    }
}
