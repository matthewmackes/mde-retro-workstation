//! Unified, selectable software catalogue for the Setup "Choose Components"
//! screen. Folds together the bare session, the applications the shell launches,
//! the Control Panel tools (`fedora::TOOLS`), and virtualization guest tools —
//! each tagged with a category, a default-on flag, and whether it is mandatory.
//!
//! Selection model (see the 10-Q spec):
//!   * mandatory  -> always installed, shown checked + locked (bare session).
//!   * installed  -> checked + locked (purely additive; Setup never removes).
//!   * default_on -> the curated "Standard" preset; desktop-referenced menus/apps on, heavyweight/niche off.
//!
//! Checked state is otherwise derived from `rpm -q`, never persisted.

use std::collections::BTreeSet;
use std::process::{Command, Stdio};

pub struct Component {
    pub package: &'static str,
    pub category: &'static str,
    pub name: &'static str,
    /// Always installed; shown checked + locked (the minimum to boot the shell).
    pub mandatory: bool,
    /// Curated default (Standard preset). `mandatory` implies on.
    pub default_on: bool,
}

// --- the bare session: minimum to boot into the MDE shell (mandatory) --------
const BASE: &[(&str, &str)] = &[
    ("labwc", "Window manager (labwc)"),
    ("foot", "Terminal"),
    ("swaybg", "Wallpaper"),
    ("NetworkManager", "Networking"),
    ("pipewire", "Audio (PipeWire)"),
    ("wireplumber", "Audio session manager"),
    ("polkit", "Authorization (polkit)"),
    ("xkeyboard-config", "Keyboard layouts"),
    ("google-droid-sans-fonts", "UI font"),
    ("lightdm", "Login manager (LightDM)"),
    ("lightdm-gtk-greeter", "Login greeter"),
    ("python3", "Asset installer runtime"),
    ("git", "Asset fetcher"),
];

// --- session utilities the shell/scripts use (curated default-on) ------------
const SESSION: &[(&str, &str)] = &[
    ("wlr-randr", "Display geometry"),
    ("wlopm", "Monitor power"),
    ("swayidle", "Idle/screensaver"),
    ("swaylock", "Screen lock"),
    ("brightnessctl", "Brightness keys"),
    ("grim", "Screenshots"),
    ("xdg-utils", "xdg-open / MIME"),
    ("nm-connection-editor", "Network config GUI"),
];

// --- applications the desktop launches / the menus reference (default-on) ----
const APPS: &[(&str, &str)] = &[
    ("firefox", "Web Browser (Firefox)"),
    ("terminator", "Terminal (Terminator)"),
    ("playerctl", "Media key control"),
    ("swayosd", "On-screen volume/brightness OSD"),
];

// --- virtualization guest tools (Xen / XCP-ng). Default-on only when this
//     machine is actually a Xen guest; otherwise offered but unchecked.
//
// Per docs.xcp-ng.org/vms and github.com/xenserver/xe-guest-utilities: on a
// Fedora/RHEL-family guest the only package needed is `xe-guest-utilities-latest`
// (the management agent: xe-daemon + xenstore client; reports IP/memory, clean
// shutdown/reboot, time sync) — its service is `xe-linux-distribution` (enabled
// after install, see enable_guest_agent). The Xen PV drivers (xen-blkfront,
// xen-netfront, pvclock) are built into the Linux kernel — no package required —
// and qemu-guest-agent is NOT used for XCP-ng guests. The Mesa software
// renderers are MDE-specific: the iced shell needs a Vulkan/GL backend, and a
// GPU-less HVM guest must fall back to lavapipe/llvmpipe. -----------------------
pub const VIRT_CATEGORY: &str = "Virtualization (Xen/XCP-ng guest)";
/// Package providing the guest agent (used to gate service enablement).
pub const GUEST_AGENT_PKG: &str = "xe-guest-utilities-latest";
const VIRT: &[(&str, &str)] = &[
    (
        "xe-guest-utilities-latest",
        "XCP-ng / XenServer guest agent — IP & memory report, clean shutdown/reboot, time sync",
    ),
    (
        "mesa-dri-drivers",
        "Software OpenGL (llvmpipe) — renders the desktop in a GPU-less VM",
    ),
    (
        "mesa-vulkan-drivers",
        "Software Vulkan (lavapipe) — the iced shell's wgpu backend in a VM",
    ),
];

/// Packages that are part of `fedora::TOOLS` but are base/always-present, so we
/// don't surface them as toggleable extras.
const TOOLS_SKIP: &[&str] = &["mde", "systemd", "systemd-resolved"];

/// Extras left UNCHECKED by default (heavyweight or niche).
const DEFAULT_OFF: &[&str] = &[
    "cockpit",
    "wireshark",
    "timeshift",
    "setroubleshoot-server",
    "mediawriter",
    "stacer",
    "blivet-gui",
    "nvtop",
];

/// True if running as a Xen guest (XCP-ng/XenServer), so the guest tools are
/// pre-selected. Checks systemd-detect-virt, then /sys/hypervisor/type.
pub fn is_xen_guest() -> bool {
    if let Ok(o) = Command::new("systemd-detect-virt").output() {
        if String::from_utf8_lossy(&o.stdout).trim() == "xen" {
            return true;
        }
    }
    std::fs::read_to_string("/sys/hypervisor/type")
        .map(|s| s.trim() == "xen")
        .unwrap_or(false)
}

/// The full selectable catalogue, in display order (categories grouped).
pub fn catalogue() -> Vec<Component> {
    let xen = is_xen_guest();
    let mut out: Vec<Component> = Vec::new();
    let mut seen: BTreeSet<&'static str> = BTreeSet::new();

    for &(package, name) in BASE {
        if seen.insert(package) {
            out.push(Component {
                package,
                category: "Base System",
                name,
                mandatory: true,
                default_on: true,
            });
        }
    }
    for &(package, name) in SESSION {
        if seen.insert(package) {
            out.push(Component {
                package,
                category: "Session",
                name,
                mandatory: false,
                default_on: true,
            });
        }
    }
    for &(package, name) in APPS {
        if seen.insert(package) {
            out.push(Component {
                package,
                category: "Applications",
                name,
                mandatory: false,
                default_on: true,
            });
        }
    }
    // Control Panel tools, in their existing category order; default-on unless
    // heavyweight/niche. Skip base/duplicate packages.
    for t in crate::fedora::TOOLS {
        if TOOLS_SKIP.contains(&t.package) {
            continue;
        }
        if seen.insert(t.package) {
            out.push(Component {
                package: t.package,
                category: t.category,
                name: t.name,
                mandatory: false,
                default_on: !DEFAULT_OFF.contains(&t.package),
            });
        }
    }
    for &(package, name) in VIRT {
        if seen.insert(package) {
            out.push(Component {
                package,
                category: VIRT_CATEGORY,
                name,
                mandatory: false,
                default_on: xen,
            });
        }
    }
    out
}

/// Ordered, de-duplicated categories as they appear in the catalogue.
pub fn categories(cat: &[Component]) -> Vec<&'static str> {
    let mut cats: Vec<&'static str> = Vec::new();
    for c in cat {
        if !cats.contains(&c.category) {
            cats.push(c.category);
        }
    }
    cats
}

/// rpm-installed check (the system is the source of truth for "checked").
pub fn is_installed(package: &str) -> bool {
    Command::new("rpm")
        .args(["-q", package])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Which of `packages` are offered by an enabled repo (for greying out the
/// rest). Best-effort: a single `dnf repoquery`; on any failure assume all are
/// available so we never hide something spuriously. Installed packages are
/// always considered available.
pub fn available(packages: &[&str]) -> BTreeSet<String> {
    let mut set: BTreeSet<String> = packages
        .iter()
        .filter(|p| is_installed(p))
        .map(|p| p.to_string())
        .collect();
    let out = Command::new("dnf")
        .args(["-q", "repoquery", "--qf", "%{name}"])
        .args(packages)
        .output();
    match out {
        Ok(o) if o.status.success() => {
            for line in String::from_utf8_lossy(&o.stdout).lines() {
                let name = line.trim();
                if !name.is_empty() {
                    set.insert(name.to_string());
                }
            }
        }
        // Query failed (offline, dnf busy): don't hide anything.
        _ => {
            for p in packages {
                set.insert(p.to_string());
            }
        }
    }
    set
}

/// If the XCP-ng/XenServer guest agent got installed, enable its service so the
/// host sees the guest (IP/memory, clean shutdown). The systemd unit is
/// `xe-linux-distribution`; older builds use `xe-daemon` — try both, best-effort.
pub fn enable_guest_agent() {
    if !is_installed(GUEST_AGENT_PKG) {
        return;
    }
    for unit in ["xe-linux-distribution", "xe-daemon"] {
        let _ = Command::new("systemctl")
            .args(["enable", "--now", unit])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}
