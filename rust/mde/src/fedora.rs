//! Fedora system-tool wiring for the Win2000 Control Panel and the right-click
//! System Tools menu. Each entry maps a Windows 2000 / descriptive name to a
//! real Fedora tool, the dnf package that provides it, its category (used to
//! group the menu/Control Panel), and whether it runs in a terminal. Tools that
//! aren't installed can be installed via `pkexec dnf` from inside the desktop.

use std::process::{Command, Stdio};

#[allow(dead_code)]
pub struct Tool {
    /// Grouping (Control Panel section / right-click submenu).
    pub category: &'static str,
    /// Display name.
    pub name: &'static str,
    /// Shell command that launches it.
    pub command: &'static str,
    /// Run inside a terminal (CLI tools).
    pub terminal: bool,
    /// dnf package that provides it (for install-if-missing).
    pub package: &'static str,
    /// Specific binary to test on `$PATH`; empty ⇒ detect via `rpm -q package`
    /// (for package-only/service tools like cockpit or dnf-plugins-core).
    pub detect_bin: &'static str,
    /// Freedesktop icon-name candidates, best first.
    pub icons: &'static [&'static str],
}

/// The full tool table. Categories order = menu/Control Panel order.
pub const TOOLS: &[Tool] = &[
    // --- Control Panel (classic applets) -----------------------------------
    // Add/Remove Programs is mde's own dnf-backed surface (B) — `mde add-remove`,
    // launched via the running binary (see control_panel Activate). Automatic
    // Updates runs `dnf upgrade` in a terminal. Both replaced dnfdragora, which
    // hung on launch and couldn't be killed. `package`/`detect_bin` = "mde" so the
    // applet is always present (never offered for install).
    Tool { category: "Control Panel", name: "Add/Remove Programs", command: "mde add-remove", terminal: false, package: "mde", detect_bin: "mde", icons: &["system-software-install"] },
    Tool { category: "Control Panel", name: "Automatic Updates", command: "sudo dnf upgrade", terminal: true, package: "dnf", detect_bin: "dnf", icons: &["system-software-update"] },
    Tool { category: "Control Panel", name: "MackesDE Firewall", command: "firewall-config", terminal: false, package: "firewall-config", detect_bin: "firewall-config", icons: &["security-high"] },
    Tool { category: "Control Panel", name: "Network and Dial-up Connections", command: "nm-connection-editor", terminal: false, package: "nm-connection-editor", detect_bin: "nm-connection-editor", icons: &["network-wired"] },
    Tool { category: "Control Panel", name: "Sounds and Multimedia", command: "pavucontrol", terminal: false, package: "pavucontrol", detect_bin: "pavucontrol", icons: &["multimedia-volume-control"] },
    Tool { category: "Control Panel", name: "Disk Management", command: "gnome-disks", terminal: false, package: "gnome-disk-utility", detect_bin: "gnome-disks", icons: &["drive-harddisk"] },
    Tool { category: "Control Panel", name: "Partition Manager", command: "gparted", terminal: false, package: "gparted", detect_bin: "gparted", icons: &["gparted"] },
    Tool { category: "Control Panel", name: "Storage Spaces", command: "blivet-gui", terminal: false, package: "blivet-gui", detect_bin: "blivet-gui", icons: &["drive-multidisk"] },
    Tool { category: "Control Panel", name: "Users and Passwords", command: "seahorse", terminal: false, package: "seahorse", detect_bin: "seahorse", icons: &["system-users"] },
    Tool { category: "Control Panel", name: "Regional Options", command: "system-config-language", terminal: false, package: "system-config-language", detect_bin: "system-config-language", icons: &["preferences-desktop-locale"] },
    Tool { category: "Control Panel", name: "Keyboard", command: "im-chooser", terminal: false, package: "im-chooser", detect_bin: "im-chooser", icons: &["input-keyboard"] },
    Tool { category: "Control Panel", name: "Event Viewer", command: "gnome-abrt", terminal: false, package: "gnome-abrt", detect_bin: "gnome-abrt", icons: &["logviewer"] },
    Tool { category: "Control Panel", name: "Security Center", command: "sealert -b", terminal: false, package: "setroubleshoot-server", detect_bin: "sealert", icons: &["security-high"] },
    Tool { category: "Control Panel", name: "Create Installation Media", command: "mediawriter", terminal: false, package: "mediawriter", detect_bin: "mediawriter", icons: &["media-optical"] },
    Tool { category: "Control Panel", name: "System", command: "mde system-properties", terminal: false, package: "mde", detect_bin: "mde", icons: &["computer"] },
    Tool { category: "Control Panel", name: "Mobile Devices", command: "mde phone", terminal: false, package: "mde", detect_bin: "mde", icons: &["smartphone", "phone", "preferences-system-network"] },
    Tool { category: "Control Panel", name: "Display", command: "mde display", terminal: false, package: "mde", detect_bin: "mde", icons: &["preferences-desktop-display", "video-display"] },
    Tool { category: "Control Panel", name: "Date and Time", command: "timedatectl; echo; read -p 'Press Enter to close '", terminal: true, package: "systemd", detect_bin: "timedatectl", icons: &["preferences-system-time"] },
    Tool { category: "Control Panel", name: "Printers", command: "system-config-printer", terminal: false, package: "system-config-printer", detect_bin: "system-config-printer", icons: &["printer"] },

    // --- Administrative Tools (systemd) -------------------------------------
    Tool { category: "Administrative Tools", name: "Services", command: "systemctl --no-pager list-units --type=service; echo; read -p 'Press Enter to close '", terminal: true, package: "systemd", detect_bin: "systemctl", icons: &["preferences-system"] },
    Tool { category: "Administrative Tools", name: "Boot Performance", command: "systemd-analyze; echo; systemd-analyze blame | head -n 30; echo; read -p 'Press Enter to close '", terminal: true, package: "systemd", detect_bin: "systemd-analyze", icons: &["utilities-system-monitor"] },
    Tool { category: "Administrative Tools", name: "Name Resolution (DNS)", command: "resolvectl status; echo; read -p 'Press Enter to close '", terminal: true, package: "systemd-resolved", detect_bin: "resolvectl", icons: &["network-wired"] },
    Tool { category: "Administrative Tools", name: "Temporary Files", command: "systemd-tmpfiles --cat-config | less", terminal: true, package: "systemd", detect_bin: "systemd-tmpfiles", icons: &["folder-temp"] },

    // --- All-in-One Dashboard -----------------------------------------------
    Tool { category: "All-in-One Dashboard", name: "Cockpit (Web Console)", command: "xdg-open https://localhost:9090", terminal: false, package: "cockpit", detect_bin: "", icons: &["network-server"] },
    Tool { category: "All-in-One Dashboard", name: "Stacer", command: "stacer", terminal: false, package: "stacer", detect_bin: "stacer", icons: &["stacer", "system-run"] },

    // --- Resource Monitoring -------------------------------------------------
    Tool { category: "Resource Monitoring", name: "btop", command: "btop", terminal: true, package: "btop", detect_bin: "btop", icons: &["utilities-system-monitor"] },
    Tool { category: "Resource Monitoring", name: "htop", command: "htop", terminal: true, package: "htop", detect_bin: "htop", icons: &["utilities-system-monitor"] },
    Tool { category: "Resource Monitoring", name: "Glances", command: "glances", terminal: true, package: "glances", detect_bin: "glances", icons: &["utilities-system-monitor"] },
    Tool { category: "Resource Monitoring", name: "iotop (Disk I/O)", command: "iotop -o", terminal: true, package: "iotop", detect_bin: "iotop", icons: &["drive-harddisk"] },
    Tool { category: "Resource Monitoring", name: "nvtop (GPU)", command: "nvtop", terminal: true, package: "nvtop", detect_bin: "nvtop", icons: &["video-display"] },

    // --- Storage & Disk ------------------------------------------------------
    Tool { category: "Storage & Disk", name: "ncdu (Disk Usage)", command: "ncdu /", terminal: true, package: "ncdu", detect_bin: "ncdu", icons: &["drive-harddisk"] },
    Tool { category: "Storage & Disk", name: "Drive Health (SMART)", command: "smartctl --scan; echo; echo 'Detail: smartctl -a /dev/sdX'; read -p 'Press Enter to close '", terminal: true, package: "smartmontools", detect_bin: "smartctl", icons: &["drive-harddisk"] },

    // --- Backup & Recovery ---------------------------------------------------
    Tool { category: "Backup & Recovery", name: "Timeshift (Snapshots)", command: "timeshift-launcher", terminal: false, package: "timeshift", detect_bin: "timeshift-launcher", icons: &["document-save"] },

    // --- Package Management --------------------------------------------------
    Tool { category: "Package Management", name: "DNF Plugins", command: "dnf config-manager --help 2>&1 | less", terminal: true, package: "dnf-plugins-core", detect_bin: "", icons: &["package-x-generic"] },
    Tool { category: "Package Management", name: "Flatpak", command: "flatpak list; echo; read -p 'Press Enter to close '", terminal: true, package: "flatpak", detect_bin: "flatpak", icons: &["package-x-generic"] },

    // --- Network Tools -------------------------------------------------------
    Tool { category: "Network Tools", name: "Nmap", command: "echo 'Usage: nmap <target>'; nmap --version; echo; read -p 'Press Enter to close '", terminal: true, package: "nmap", detect_bin: "nmap", icons: &["network-workgroup"] },
    Tool { category: "Network Tools", name: "iPerf3", command: "echo 'Server: iperf3 -s   Client: iperf3 -c <host>'; iperf3 --version; echo; read -p 'Press Enter to close '", terminal: true, package: "iperf3", detect_bin: "iperf3", icons: &["network-wired"] },
    Tool { category: "Network Tools", name: "Wireshark", command: "wireshark", terminal: false, package: "wireshark", detect_bin: "wireshark", icons: &["wireshark", "network-wired"] },

    // --- Power Management ----------------------------------------------------
    Tool { category: "Power Management", name: "PowerTOP", command: "echo 'Tip: pkexec powertop'; powertop --version; echo; read -p 'Press Enter to close '", terminal: true, package: "powertop", detect_bin: "powertop", icons: &["battery"] },

    // --- Hardware Info -------------------------------------------------------
    Tool { category: "Hardware Info", name: "fastfetch", command: "fastfetch; echo; read -p 'Press Enter to close '", terminal: true, package: "fastfetch", detect_bin: "fastfetch", icons: &["computer"] },
    Tool { category: "Hardware Info", name: "Sensors (lm_sensors)", command: "sensors; echo; read -p 'Press Enter to close '", terminal: true, package: "lm_sensors", detect_bin: "sensors", icons: &["temperature"] },

    // --- System Security -----------------------------------------------------
    Tool { category: "System Security", name: "Lynis (Audit)", command: "echo 'Full audit: pkexec lynis audit system'; lynis show version; echo; read -p 'Press Enter to close '", terminal: true, package: "lynis", detect_bin: "lynis", icons: &["security-high"] },
];

/// The leading binary of a command (`sealert -b` -> `sealert`).
pub fn binary(command: &str) -> &str {
    command.split_whitespace().next().unwrap_or(command)
}

fn on_path(bin: &str) -> bool {
    !bin.is_empty()
        && Command::new("sh")
            .arg("-c")
            .arg(format!("command -v {bin}"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
}

fn rpm_installed(package: &str) -> bool {
    Command::new("rpm")
        .args(["-q", package])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Whether the tool is available: a specific binary on `$PATH`, else its rpm.
pub fn is_installed(tool: &Tool) -> bool {
    if on_path(tool.detect_bin) {
        return true;
    }
    rpm_installed(tool.package)
}

/// Terminal font size for CLI tools: 150% of foot's 8pt default.
pub const CLI_FONT_SIZE: u32 = 12;

/// Launch a tool. CLI tools open in `foot` zoomed to 150% (12pt).
pub fn launch(tool: &Tool) -> std::io::Result<()> {
    if tool.terminal {
        Command::new("foot")
            .arg("-o")
            .arg(format!("font=monospace:size={CLI_FONT_SIZE}"))
            .arg("sh")
            .arg("-c")
            .arg(tool.command)
            .spawn()?;
    } else {
        Command::new("sh").arg("-c").arg(tool.command).spawn()?;
    }
    Ok(())
}

/// Ordered, de-duplicated list of categories.
pub fn categories() -> Vec<&'static str> {
    let mut cats = Vec::new();
    for t in TOOLS {
        if !cats.contains(&t.category) {
            cats.push(t.category);
        }
    }
    cats
}

/// Tools whose package/binary is not currently present.
pub fn missing() -> Vec<&'static Tool> {
    TOOLS.iter().filter(|t| !is_installed(t)).collect()
}

/// Unique dnf packages needed to satisfy all missing tools.
pub fn missing_packages() -> Vec<&'static str> {
    let mut pkgs: Vec<&str> = missing().iter().map(|t| t.package).collect();
    pkgs.sort_unstable();
    pkgs.dedup();
    pkgs
}

/// Install the given packages via a single graphical `pkexec dnf` prompt.
pub fn install(packages: &[&str]) -> std::io::Result<std::process::ExitStatus> {
    Command::new("pkexec")
        .arg("dnf")
        .arg("install")
        .arg("-y")
        .args(packages)
        .status()
}

// --- installed-package listing (Settings ▸ Storage ▸ Apps & features, E17.4) ---

/// One installed package and its on-disk size (bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    pub size: u64,
}

/// Parse `rpm -qa --qf '%{NAME} %{SIZE}\n'` into packages, **largest first**. Pure
/// (unit-tested). Malformed/sizeless lines are skipped.
pub fn parse_packages(out: &str) -> Vec<Package> {
    let mut pkgs: Vec<Package> = out
        .lines()
        .filter_map(|l| {
            let (name, size) = l.trim().rsplit_once(' ')?;
            let size: u64 = size.trim().parse().ok()?;
            (!name.is_empty()).then(|| Package {
                name: name.trim().to_string(),
                size,
            })
        })
        .collect();
    pkgs.sort_by(|a, b| b.size.cmp(&a.size).then(a.name.cmp(&b.name)));
    pkgs
}

/// The installed packages, largest first (`rpm -qa`).
pub fn installed_packages() -> Vec<Package> {
    let out = Command::new("rpm")
        .args(["-qa", "--qf", "%{NAME} %{SIZE}\n"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    parse_packages(&out)
}

/// `dnf remove -y <pkg>` — the uninstall argv (run via `pkexec`). Pure.
pub fn dnf_remove_cmd(pkg: &str) -> Vec<String> {
    vec!["dnf".into(), "remove".into(), "-y".into(), pkg.into()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packages_parse_and_sort_largest_first() {
        let out = "\
firefox 250000000
bash 8000000
kernel-core 90000000
junkline
nosize abc
";
        let p = parse_packages(out);
        assert_eq!(p.len(), 3); // junk + sizeless dropped
        assert_eq!(p[0].name, "firefox"); // largest first
        assert_eq!(p[0].size, 250_000_000);
        assert_eq!(p[1].name, "kernel-core");
        assert_eq!(p[2].name, "bash");
    }

    #[test]
    fn remove_cmd_is_dnf_remove() {
        assert_eq!(dnf_remove_cmd("htop"), vec!["dnf", "remove", "-y", "htop"]);
    }
}
