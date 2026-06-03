//! Windows 10 "Settings" app — a flat category-nav window that replaces the
//! Control Panel in the Win10 era (Control Panel still serves the classic eras;
//! see the per-era routing in `control_panel::run`).
//!
//!   mde settings           the category grid (Home), drilling into categories
//!   mde settings --list    print the page → backend map (headless)
//!
//! Home is an icon-tile grid of the Win10 setting categories. A tile drills into
//! a category: a left page-rail + a right content pane, with a back-arrow home.
//! Pages are one of:
//!   - **Colors** — a native Light/Dark + accent page (re-skins live, persists).
//!   - **Spawn** — opens an existing backend (mde's own surfaces, or a
//!     `fedora::TOOLS` entry that installs-if-missing like the Control Panel).
//!   - **Deferred** — a greyed rail entry whose pane says it's a later milestone;
//!     never a fake working page (§3).

use std::collections::HashMap;
use std::process::{Command, ExitCode};

use iced::widget::{
    button, checkbox, column, container, image, mouse_area, pick_list, scrollable, text,
    text_input, Column, Row, Space,
};
use iced::{Background, Border, Color, Element, Length, Padding, Task};

use mde_ui::{metrics, palette};

use crate::wallpaper::{self, BgMode};
use crate::{fedora, outputs};

/// The Background page's source mode (E7.4): a picture, a solid color, or a
/// slideshow cycling the scanned pictures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BgSource {
    Picture,
    Solid,
    Slideshow,
}
impl BgSource {
    const ALL: [BgSource; 3] = [BgSource::Picture, BgSource::Solid, BgSource::Slideshow];
}

/// Taskbar location (E7.9): the two horizontal edges the Win10 bar supports
/// (left/right need a vertical bar — E7.9a).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaskbarLoc {
    Bottom,
    Top,
}
impl TaskbarLoc {
    const ALL: [TaskbarLoc; 2] = [TaskbarLoc::Bottom, TaskbarLoc::Top];
    fn key(self) -> &'static str {
        match self {
            TaskbarLoc::Bottom => "bottom",
            TaskbarLoc::Top => "top",
        }
    }
    fn from_key(k: &str) -> Self {
        if k == "top" {
            TaskbarLoc::Top
        } else {
            TaskbarLoc::Bottom
        }
    }
}
impl std::fmt::Display for TaskbarLoc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            TaskbarLoc::Bottom => "Bottom",
            TaskbarLoc::Top => "Top",
        })
    }
}

/// Win10 taskbar search affordance (E7.9a): a magnifier button, a wider "Search"
/// pill, or nothing — persisted as `win10_search_mode`, already consumed by
/// `panel.rs` (`win10_search_affordance`, E2.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchMode {
    Button,
    Box,
    Hidden,
}
impl SearchMode {
    const ALL: [SearchMode; 3] = [SearchMode::Button, SearchMode::Box, SearchMode::Hidden];
    fn key(self) -> &'static str {
        match self {
            SearchMode::Button => "button",
            SearchMode::Box => "box",
            SearchMode::Hidden => "hidden",
        }
    }
    fn from_key(k: &str) -> Self {
        match k {
            "box" => SearchMode::Box,
            "hidden" => SearchMode::Hidden,
            _ => SearchMode::Button,
        }
    }
}
impl std::fmt::Display for SearchMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            SearchMode::Button => "Search button",
            SearchMode::Box => "Search box",
            SearchMode::Hidden => "Hidden",
        })
    }
}

/// An hour of the day, for the active-hours pick-lists (E13.5) — renders `HH:00`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Hour(u8);
impl std::fmt::Display for Hour {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:02}:00", self.0)
    }
}

/// Proxy mode for the Proxy page picker (E15.9) — the GNOME `mode` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProxyMode {
    Off,
    Manual,
    Auto,
}
impl ProxyMode {
    const ALL: [ProxyMode; 3] = [ProxyMode::Off, ProxyMode::Manual, ProxyMode::Auto];
    fn key(self) -> &'static str {
        match self {
            ProxyMode::Off => "none",
            ProxyMode::Manual => "manual",
            ProxyMode::Auto => "auto",
        }
    }
    fn from_key(k: &str) -> Self {
        match k {
            "manual" => ProxyMode::Manual,
            "auto" => ProxyMode::Auto,
            _ => ProxyMode::Off,
        }
    }
}
impl std::fmt::Display for ProxyMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ProxyMode::Off => "Off",
            ProxyMode::Manual => "Manual",
            ProxyMode::Auto => "Automatic",
        })
    }
}
impl std::fmt::Display for BgSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            BgSource::Picture => "Picture",
            BgSource::Solid => "Solid color",
            BgSource::Slideshow => "Slideshow",
        })
    }
}

/// Devices ▸ Mouse (E12.6): which physical button is "primary". Right ⇒ labwc
/// `<leftHanded>yes</leftHanded>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrimaryButton {
    Left,
    Right,
}
impl PrimaryButton {
    const ALL: [PrimaryButton; 2] = [PrimaryButton::Left, PrimaryButton::Right];
}
impl std::fmt::Display for PrimaryButton {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            PrimaryButton::Left => "Left",
            PrimaryButton::Right => "Right",
        })
    }
}

/// A "lines to scroll" pick_list item (1–10) that renders as "N lines" (E12.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Lines(u8);
impl std::fmt::Display for Lines {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} lines", self.0)
    }
}

/// Devices ▸ Typing (E12.8): key-repeat rate as a 3-level pick_list → labwc
/// `<repeatRate>` (chars/sec).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RepeatRate {
    Slow,
    Medium,
    Fast,
}
impl RepeatRate {
    const ALL: [RepeatRate; 3] = [RepeatRate::Slow, RepeatRate::Medium, RepeatRate::Fast];
    fn rate(self) -> u32 {
        match self {
            RepeatRate::Slow => 10,
            RepeatRate::Medium => 25,
            RepeatRate::Fast => 40,
        }
    }
    fn from_rate(r: u32) -> Self {
        if r <= 15 {
            RepeatRate::Slow
        } else if r <= 32 {
            RepeatRate::Medium
        } else {
            RepeatRate::Fast
        }
    }
}
impl std::fmt::Display for RepeatRate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            RepeatRate::Slow => "Slow",
            RepeatRate::Medium => "Medium",
            RepeatRate::Fast => "Fast",
        })
    }
}

/// Key-repeat delay as a 3-level pick_list → labwc `<repeatDelay>` (ms).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RepeatDelay {
    Short,
    Medium,
    Long,
}
impl RepeatDelay {
    const ALL: [RepeatDelay; 3] = [RepeatDelay::Short, RepeatDelay::Medium, RepeatDelay::Long];
    fn delay(self) -> u32 {
        match self {
            RepeatDelay::Short => 300,
            RepeatDelay::Medium => 600,
            RepeatDelay::Long => 1000,
        }
    }
    fn from_delay(d: u32) -> Self {
        if d <= 450 {
            RepeatDelay::Short
        } else if d <= 800 {
            RepeatDelay::Medium
        } else {
            RepeatDelay::Long
        }
    }
}
impl std::fmt::Display for RepeatDelay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            RepeatDelay::Short => "Short",
            RepeatDelay::Medium => "Medium",
            RepeatDelay::Long => "Long",
        })
    }
}

/// Devices ▸ AutoPlay (E12.9): the per-media-type action pick_list, mapping to the
/// `crate::autoplay::Action` keys persisted in menu.json.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AutoAction {
    Open,
    Ask,
    Nothing,
}
impl AutoAction {
    const ALL: [AutoAction; 3] = [AutoAction::Open, AutoAction::Ask, AutoAction::Nothing];
    fn key(self) -> &'static str {
        match self {
            AutoAction::Open => "open",
            AutoAction::Ask => "ask",
            AutoAction::Nothing => "nothing",
        }
    }
    fn from_key(s: &str) -> AutoAction {
        match s {
            "ask" => AutoAction::Ask,
            "nothing" => AutoAction::Nothing,
            _ => AutoAction::Open,
        }
    }
}
impl std::fmt::Display for AutoAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            AutoAction::Open => "Open folder to view files",
            AutoAction::Ask => "Ask me what to do",
            AutoAction::Nothing => "Take no action",
        })
    }
}

/// Update & Security ▸ Backup ▸ More options (E17.7): how often to snapshot →
/// systemd `OnCalendar=`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Schedule {
    Hourly,
    Daily,
    Weekly,
    Monthly,
}
impl Schedule {
    const ALL: [Schedule; 4] = [
        Schedule::Hourly,
        Schedule::Daily,
        Schedule::Weekly,
        Schedule::Monthly,
    ];
    fn key(self) -> &'static str {
        match self {
            Schedule::Hourly => "hourly",
            Schedule::Daily => "daily",
            Schedule::Weekly => "weekly",
            Schedule::Monthly => "monthly",
        }
    }
    fn from_key(s: &str) -> Schedule {
        match s {
            "daily" => Schedule::Daily,
            "weekly" => Schedule::Weekly,
            "monthly" => Schedule::Monthly,
            _ => Schedule::Hourly,
        }
    }
}
impl std::fmt::Display for Schedule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Schedule::Hourly => "Hourly",
            Schedule::Daily => "Daily",
            Schedule::Weekly => "Weekly",
            Schedule::Monthly => "Monthly",
        })
    }
}

/// Backup ▸ More options: how many snapshots to keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Retention {
    Forever,
    Last10,
    Last30Days,
}
impl Retention {
    const ALL: [Retention; 3] = [Retention::Forever, Retention::Last10, Retention::Last30Days];
    fn key(self) -> &'static str {
        match self {
            Retention::Forever => "forever",
            Retention::Last10 => "last10",
            Retention::Last30Days => "days30",
        }
    }
    fn from_key(s: &str) -> Retention {
        match s {
            "last10" => Retention::Last10,
            "days30" => Retention::Last30Days,
            _ => Retention::Forever,
        }
    }
}
impl std::fmt::Display for Retention {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Retention::Forever => "Keep forever",
            Retention::Last10 => "Last 10 snapshots",
            Retention::Last30Days => "Last 30 days",
        })
    }
}

/// A keyboard-layout pick_list item — renders the friendly name, keyed by xkb code
/// (E12.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Layout {
    code: &'static str,
    name: &'static str,
}
impl Layout {
    fn all() -> Vec<Layout> {
        crate::keyboard::LAYOUTS
            .iter()
            .map(|(code, name)| Layout { code, name })
            .collect()
    }
    /// The `Layout` for a stored code, falling back to the first entry (US).
    fn for_code(code: &str) -> Layout {
        crate::keyboard::LAYOUTS
            .iter()
            .find(|(c, _)| *c == code)
            .or_else(|| crate::keyboard::LAYOUTS.first())
            .map(|(code, name)| Layout { code, name })
            .unwrap_or(Layout {
                code: "us",
                name: "English (US)",
            })
    }
}
impl std::fmt::Display for Layout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name)
    }
}

const COLS: usize = 4;

/// What a settings page does when opened.
#[derive(Clone, Copy)]
enum Kind {
    /// Greyed rail entry; the pane states it's a later milestone.
    Deferred,
    /// The native Personalization ▸ Colors page (E6.4).
    Colors,
    /// The native Personalization ▸ Background page (E7.4).
    Background,
    /// The native Personalization ▸ Themes page (E7.7).
    Themes,
    /// The native Personalization ▸ Lock screen page (E7.6).
    LockScreen,
    /// The native Personalization ▸ Start page (E7.8).
    Start,
    /// The native Personalization ▸ Taskbar page (E7.9).
    Taskbar,
    /// The native Update & Security ▸ Update page (E13.2): status card + Check.
    Update,
    /// The native Update & Security ▸ Update history page (E13.6).
    UpdateHistory,
    /// The native Update & Security ▸ Advanced options page (E13.7).
    UpdateAdvanced,
    /// The native Network & Internet ▸ Status page (E15.5).
    NetworkStatus,
    /// The native Network & Internet ▸ Wi-Fi page (E15.6).
    Wifi,
    /// The native Network & Internet ▸ Ethernet page (E15.7).
    Ethernet,
    /// The native Network & Internet ▸ VPN page (E15.7).
    Vpn,
    /// The native Network & Internet ▸ Mobile hotspot page (E15.8).
    Hotspot,
    /// The native Network & Internet ▸ Proxy page (E15.9).
    Proxy,
    /// The native Network & Internet ▸ Airplane mode page (E15.10).
    Airplane,
    /// The native Network & Internet ▸ Data usage page (E15.11).
    DataUsage,
    /// The native Network & Internet ▸ Cellular page (E15.12) — greyed advisory.
    Cellular,
    /// The native Accounts ▸ Your info page (E10.3).
    AccountInfo,
    /// The native Accounts ▸ Family & other users page (E10.4): the real account list.
    FamilyUsers,
    /// The native Accounts ▸ Sign-in options page (E10.6): PIN + password + Hello.
    SignIn,
    /// The native Devices ▸ Bluetooth page (E12.2): BlueZ adapter + device list.
    Bluetooth,
    /// The native Devices ▸ Printers & scanners page (E12.4): CUPS queue list.
    Printers,
    /// The native Devices ▸ Mouse page (E12.6): labwc libinput in rc.xml.
    Mouse,
    /// The native Devices ▸ Touchpad page (E12.7): conditional on a touchpad.
    Touchpad,
    /// The native Devices ▸ Typing page (E12.8): labwc keyboard repeat + layout.
    Typing,
    /// The native Devices ▸ AutoPlay page (E12.9): removable-media defaults.
    AutoPlay,
    /// The native System ▸ Storage page (E17.4): usage bars + Storage Sense + apps.
    Storage,
    /// The native Update & Security ▸ Backup page (E17.6): Timeshift drive picker.
    Backup,
    /// Spawn one of mde's own subcommands (`mde <sub>`).
    Mde(&'static str),
    /// Launch a `fedora::TOOLS` entry by its command (install-if-missing).
    Tool(&'static str),
    /// A raw command (in a terminal when `bool`).
    Cmd(&'static str, bool),
}

struct Page {
    title: &'static str,
    kind: Kind,
}

struct Category {
    title: &'static str,
    caption: &'static str,
    icons: &'static [&'static str],
    pages: &'static [Page],
}

/// The Win10 Settings categories (no Gaming, no Search — see the worklist). Each
/// lists its live pages first, then the deferred ones.
const CATEGORIES: &[Category] = &[
    Category {
        title: "System",
        caption: "Display, notifications, power",
        icons: &["preferences-system", "computer"],
        pages: &[
            Page {
                title: "Display",
                kind: Kind::Mde("display"),
            },
            Page {
                title: "About",
                kind: Kind::Mde("system-properties"),
            },
            Page {
                title: "Notifications & actions",
                kind: Kind::Deferred,
            },
            Page {
                title: "Power & sleep",
                kind: Kind::Deferred,
            },
            Page {
                title: "Storage",
                kind: Kind::Storage,
            },
        ],
    },
    Category {
        title: "Devices",
        caption: "Bluetooth, printers, mouse",
        icons: &["preferences-desktop-peripherals", "input-mouse"],
        pages: &[
            Page {
                title: "Printers & scanners",
                kind: Kind::Printers,
            },
            Page {
                title: "Bluetooth & other devices",
                kind: Kind::Bluetooth,
            },
            Page {
                title: "Mouse",
                kind: Kind::Mouse,
            },
            Page {
                title: "Touchpad",
                kind: Kind::Touchpad,
            },
            Page {
                title: "Typing",
                kind: Kind::Typing,
            },
            Page {
                title: "AutoPlay",
                kind: Kind::AutoPlay,
            },
        ],
    },
    Category {
        title: "Phone",
        caption: "Link your Android, iPhone",
        icons: &["phone", "smartphone"],
        pages: &[Page {
            title: "Your Phone",
            kind: Kind::Deferred,
        }],
    },
    Category {
        title: "Network & Internet",
        caption: "Wi-Fi, airplane mode, VPN",
        icons: &["preferences-system-network", "network-wired"],
        pages: &[
            Page {
                title: "Connections",
                kind: Kind::NetworkStatus,
            },
            Page {
                title: "Wi-Fi",
                kind: Kind::Wifi,
            },
            Page {
                title: "Ethernet",
                kind: Kind::Ethernet,
            },
            Page {
                title: "VPN",
                kind: Kind::Vpn,
            },
            Page {
                title: "Airplane mode",
                kind: Kind::Airplane,
            },
            Page {
                title: "Mobile hotspot",
                kind: Kind::Hotspot,
            },
            Page {
                title: "Proxy",
                kind: Kind::Proxy,
            },
            Page {
                title: "Cellular",
                kind: Kind::Cellular,
            },
            Page {
                title: "Data usage",
                kind: Kind::DataUsage,
            },
        ],
    },
    Category {
        title: "Personalization",
        caption: "Background, lock screen, colors",
        icons: &["preferences-desktop-wallpaper", "preferences-desktop-theme"],
        pages: &[
            Page {
                title: "Background",
                kind: Kind::Background,
            },
            Page {
                title: "Colors",
                kind: Kind::Colors,
            },
            Page {
                title: "Lock screen",
                kind: Kind::LockScreen,
            },
            Page {
                title: "Themes",
                kind: Kind::Themes,
            },
            Page {
                title: "Start",
                kind: Kind::Start,
            },
            Page {
                title: "Taskbar",
                kind: Kind::Taskbar,
            },
        ],
    },
    Category {
        title: "Apps",
        caption: "Uninstall, defaults, optional features",
        icons: &[
            "preferences-desktop-applications",
            "system-software-install",
        ],
        pages: &[
            Page {
                title: "Apps & features",
                kind: Kind::Mde("add-remove"),
            },
            Page {
                title: "Default apps",
                kind: Kind::Deferred,
            },
            Page {
                title: "Startup",
                kind: Kind::Deferred,
            },
        ],
    },
    Category {
        title: "Accounts",
        caption: "Your accounts, sign-in, sync",
        icons: &["system-users", "avatar-default"],
        pages: &[
            Page {
                title: "Your info",
                kind: Kind::AccountInfo,
            },
            Page {
                title: "Family & other users",
                kind: Kind::FamilyUsers,
            },
            Page {
                title: "Sign-in options",
                kind: Kind::SignIn,
            },
            Page {
                title: "Sync your settings",
                kind: Kind::Deferred,
            },
        ],
    },
    Category {
        title: "Time & Language",
        caption: "Speech, region, date",
        icons: &["preferences-system-time", "clock"],
        pages: &[
            Page {
                title: "Date & time",
                kind: Kind::Cmd("timedatectl", true),
            },
            Page {
                title: "Region",
                kind: Kind::Deferred,
            },
            Page {
                title: "Language",
                kind: Kind::Deferred,
            },
        ],
    },
    Category {
        title: "Ease of Access",
        caption: "Narrator, magnifier, high contrast",
        icons: &["preferences-desktop-accessibility"],
        pages: &[Page {
            title: "Display",
            kind: Kind::Deferred,
        }],
    },
    Category {
        title: "Privacy",
        caption: "Location, camera, microphone",
        icons: &["preferences-system-privacy", "security-medium"],
        pages: &[Page {
            title: "General",
            kind: Kind::Deferred,
        }],
    },
    Category {
        title: "Update & Security",
        caption: "MackesDE Update, recovery, backup",
        icons: &["system-software-update", "security-high"],
        pages: &[
            Page {
                title: "MackesDE Update",
                kind: Kind::Update,
            },
            Page {
                title: "Update history",
                kind: Kind::UpdateHistory,
            },
            Page {
                title: "Advanced options",
                kind: Kind::UpdateAdvanced,
            },
            Page {
                title: "MackesDE Security",
                kind: Kind::Tool("firewall-config"),
            },
            Page {
                title: "Backup",
                kind: Kind::Backup,
            },
            Page {
                title: "Recovery",
                kind: Kind::Deferred,
            },
        ],
    },
];

#[derive(Debug, Clone, Copy, PartialEq)]
enum View {
    Home,
    Category(usize),
}

struct Settings {
    view: View,
    page: usize, // selected rail page within the current category
    dark: bool,
    /// Windows 10 UI accent index (E7.5) — into `palette::WIN10_ACCENTS`; drives
    /// selection/highlight via the `win10()` slot, persisted as `win10_accent`.
    win10_accent: u8,
    /// E7.5a: "show accent color on Start & taskbar" — persisted as
    /// `win10_accent_on_taskbar`, consumed by `palette::chrome_accent` in panel.rs.
    accent_on_taskbar: bool,
    /// Home-screen search query (E6.6): filters the flat (category, page) list.
    search: String,
    /// Background page (E7.4): source mode, scanned pictures, selection, fit.
    bg_source: BgSource,
    bg_wallpapers: Vec<String>,
    bg_selected: Option<usize>,
    bg_mode: BgMode,
    /// Saved theme bundles (E7.7).
    themes: Vec<crate::state::SavedTheme>,
    /// Start page toggles (E7.8), consumed by `start_win10`.
    start_more_tiles: bool,
    /// Start ▸ "Use Start full screen" (E7.8b).
    start_full_screen: bool,
    start_show_recent: bool,
    start_show_suggested: bool,
    /// Start rail system folders (E7.8a): the chosen `start_win10::START_FOLDERS` keys.
    start_folders: Vec<String>,
    /// Taskbar location (E7.9), consumed by `panel.rs`'s Win10 anchor.
    taskbar_loc: TaskbarLoc,
    /// Win10 taskbar search affordance + Task-View button (E7.9a), consumed by panel.rs.
    search_mode: SearchMode,
    show_taskview: bool,
    /// Win10 "automatically hide the taskbar" (E2.9a), consumed by panel.rs.
    autohide: bool,
    /// Win10 "use small taskbar buttons" (E7.9a), consumed by panel.rs.
    small_buttons: bool,
    /// Lock screen (greeter) picture selection (E7.6).
    lock_selected: Option<usize>,
    /// Update page (E13.2): a `dnf check-update` is in flight.
    update_checking: bool,
    /// Update page: the last check — None = not checked this session, Ok(list) =
    /// the pending updates (empty = up to date), Err = the check failed (E13.2/3).
    update_status: Option<Result<Vec<crate::packages::Update>, String>>,
    /// Update page (E13.3): a `pkexec dnf upgrade` install is in flight.
    update_installing: bool,
    /// Update page (E13.4): paused-until Unix seconds (0 = not paused), mirrors
    /// `state.update_paused_until`.
    update_paused_until: u64,
    /// Update page (E13.5): active-hours window (hours 0–23), mirrors state.
    active_start: u8,
    active_end: u8,
    /// Update history page (E13.6): the `dnf history list` transactions (None =
    /// not loaded), whether a load is in flight, and the selected transaction id.
    history: Option<Result<Vec<HistoryEntry>, String>>,
    history_loading: bool,
    history_selected: Option<u32>,
    /// Advanced options (E13.7): restart ASAP (writes automatic.conf) + notify on
    /// restart-required. Mirror `state`.
    restart_asap: bool,
    restart_notify: bool,
    /// Automatic-updates posture (E13.8), read live from the timer/config — the
    /// same source the System Properties radios use, so the two surfaces agree.
    auto_mode: crate::sysinfo::AutoMode,
    /// Network status page (E15.5): the active connections + the active one's
    /// firewalld zone (Private/Public), read live from nmcli.
    net_conns: Vec<crate::nm::Conn>,
    net_zone: String,
    /// Wi-Fi page (E15.6): the scanned networks (None = not yet scanned), whether a
    /// scan is in flight, and the "auto-connect to known networks" state.
    wifis: Option<Vec<crate::nm::Wifi>>,
    wifi_scanning: bool,
    wifi_autoconnect: bool,
    /// Saved Wi-Fi connection names (E15.6a) — a scanned SSID that matches shows a
    /// Forget button instead of Connect.
    wifi_saved: Vec<String>,
    /// VPN/WireGuard connections (E15.7), read at settings start.
    vpns: Vec<crate::nm::Conn>,
    /// Mobile hotspot (E15.8): the AP SSID + key, mirroring `state`.
    hotspot_name: String,
    hotspot_password: String,
    /// Proxy page (E15.9): the GNOME proxy mode + manual HTTP host/port, read live.
    proxy_mode: ProxyMode,
    proxy_host: String,
    proxy_port: String,
    /// Airplane page (E15.10): airplane-mode + Wi-Fi-radio state, read on show.
    airplane: bool,
    wifi_radio: bool,
    /// Data-usage page (E15.11): per-device (name, rx, tx) read on show, + the
    /// editable monthly limit in MB (mirrors `state.data_limit_mb`).
    usage: Vec<(String, u64, u64)>,
    data_limit: String,
    /// Accounts ▸ Your info (E10.3): friendly name + avatar path, mirroring `state`.
    display_name: String,
    account_picture: String,
    /// Devices ▸ Bluetooth (E12.2): the BlueZ adapter + device snapshot, loaded
    /// off-thread on first visit and after each action.
    bt: Option<crate::bluez::BtState>,
    /// Devices ▸ Printers (E12.4): the CUPS queue list + last discovery scan,
    /// loaded off-thread; `manage_default` mirrors `win10_manage_default_printer`.
    printers: Option<crate::cups::CupsState>,
    manage_default: bool,
    /// Set once "+ Add a printer" has run, so an empty scan shows "no devices
    /// found" rather than looking like a no-op (E12.4).
    printers_scanned: bool,
    /// Devices ▸ Mouse (E12.6): mirror of the menu.json mouse prefs. Changes to the
    /// first three rewrite rc.xml; `scroll_inactive` is advisory (menu.json only).
    mouse_left_handed: bool,
    mouse_natural_scroll: bool,
    mouse_scroll_lines: u8,
    scroll_inactive: bool,
    /// Devices ▸ Touchpad (E12.7): probed once at init; the rail hides the Touchpad
    /// page when false. The five settings mirror menu.json and (re)write rc.xml's
    /// `touchpad` device whenever any libinput control changes.
    touchpad_present: bool,
    touchpad_enabled: bool,
    touchpad_speed: u8,
    touchpad_tap: bool,
    touchpad_two_finger: bool,
    touchpad_natural_scroll: bool,
    /// Devices ▸ Typing (E12.8): mirror of the menu.json keyboard prefs. Rate/delay
    /// rewrite rc.xml's `<keyboard>`; layout writes the labwc `environment` file;
    /// the two typing toggles are advisory (menu.json only).
    kb_repeat_rate: u32,
    kb_repeat_delay: u32,
    kb_layout: String,
    typing_autocorrect: bool,
    typing_suggestions: bool,
    /// Devices ▸ AutoPlay (E12.9): master toggle + per-type actions. Persisted to
    /// menu.json; `mde devices-monitor` re-reads them on each mount event.
    autoplay_enabled: bool,
    autoplay_removable: AutoAction,
    autoplay_memcard: AutoAction,
    /// System ▸ Storage (E17.4): the breakdown (loaded off-thread on first visit),
    /// the Storage Sense toggle (mirrors menu.json), and the Apps & features
    /// drill-in (its package list + a pending uninstall confirm).
    storage: Option<crate::sysinfo::StorageUsage>,
    storage_sense: bool,
    storage_apps: bool,
    packages: Vec<crate::fedora::Package>,
    confirm_uninstall: Option<String>,
    /// System ▸ Storage ▸ Configure Storage Sense / Clean now (E17.5): the sub-view
    /// flag, a pending clean-now confirm, and the last run's freed bytes.
    storage_clean: bool,
    confirm_clean: bool,
    last_freed: Option<u64>,
    /// Update & Security ▸ Backup (E17.6): the Timeshift snapshot device + the
    /// automatic-backup toggle, mirroring menu.json.
    backup_drive: String,
    auto_backup: bool,
    /// Backup ▸ More options (E17.7): the sub-view flag, a pending back-up-now
    /// confirm, the schedule/retention pick_list values, and the included folders.
    backup_more: bool,
    confirm_backup: bool,
    backup_schedule: Schedule,
    backup_retention: Retention,
    backup_includes: Vec<String>,
    /// Accounts ▸ Family & other users (E10.4): the enumerated local accounts,
    /// (re)read on each visit to that page.
    accounts: Vec<crate::sysinfo::Account>,
    /// E10.5: the "add a user" name field + the login pending a remove-confirm.
    new_user: String,
    confirm_remove: Option<String>,
    /// Accounts ▸ Sign-in options (E10.6): whether a PIN is enrolled, the two PIN
    /// entry fields, and a transient status/error line.
    pin_set: bool,
    pin1: String,
    pin2: String,
    pin_msg: String,
    /// Cached install state for the `fedora::TOOLS` command of a viewed Tool
    /// page (computed lazily — `is_installed` spawns subprocesses).
    installed: HashMap<&'static str, bool>,
}

#[derive(Debug, Clone)]
enum Message {
    OpenCategory(usize),
    SelectPage(usize),
    Open, // activate the current page's backend
    Back,
    SetDark(bool),
    SetAccentOnTaskbar(bool),
    Search(String),
    Jump(usize, usize), // (category, page) from a search result
    // Background page (E7.4).
    BgSource(BgSource),
    BgSelect(usize),
    BgMode(BgMode),
    BgBrowse,
    BgBrowsed(Option<String>),
    BgApply,
    // Themes page (E7.7).
    SaveTheme,
    ApplyTheme(usize),
    // Start page (E7.8).
    SetStartMore(bool),
    /// Start ▸ "Use Start full screen" (E7.8b).
    SetStartFullScreen(bool),
    SetStartRecent(bool),
    SetStartSuggested(bool),
    /// Toggle a system folder's presence in the Start rail (E7.8a).
    ToggleStartFolder(String, bool),
    // Taskbar page (E7.9).
    SetTaskbarLoc(TaskbarLoc),
    SetSearchMode(SearchMode),
    SetShowTaskview(bool),
    SetAutohide(bool),
    SetSmallButtons(bool),
    // Lock screen page (E7.6).
    LockSelect(usize),
    LockBrowse,
    LockBrowsed(Option<String>),
    LockApply,
    // Update page (E13.2 / E13.3).
    CheckUpdates,
    UpdatesChecked(Result<Vec<crate::packages::Update>, String>),
    InstallUpdates,
    UpdatesInstalled(bool),
    PauseUpdates,
    ResumeUpdates,
    SetActiveStart(u8),
    SetActiveEnd(u8),
    SaveActiveHours,
    // Update history page (E13.6).
    HistoryFetched(Result<Vec<HistoryEntry>, String>),
    SelectHistory(u32),
    UninstallHistory,
    OpenRecovery,
    // Advanced options (E13.7).
    SetRestartAsap(bool),
    SetRestartNotify(bool),
    // Automatic-updates posture (E13.8).
    SetAutoMode(crate::sysinfo::AutoMode),
    // Network status page (E15.5).
    SetNetPrivate(bool),
    OpenNetEditor,
    // Wi-Fi page (E15.6).
    WifiScanned(Vec<crate::nm::Wifi>),
    ConnectSsid(String),
    ForgetSsid(String),
    SetWifiAutoconnect(bool),
    // VPN page (E15.7).
    VpnToggle(String, bool),
    AddVpn,
    // Mobile hotspot page (E15.8).
    SetHotspotOn(bool),
    HotspotName(String),
    HotspotPassword(String),
    // Proxy page (E15.9).
    SetProxyMode(ProxyMode),
    ProxyHost(String),
    ProxyPort(String),
    ApplyProxy,
    // Airplane page (E15.10).
    SetAirplane(bool),
    SetWifiRadio(bool),
    // Data usage page (E15.11).
    SetDataLimit(String),
    // Accounts ▸ Your info (E10.3).
    DisplayName(String),
    BrowseAvatar,
    AvatarBrowsed(Option<String>),
    // Accounts ▸ Family & other users (E10.5).
    NewUser(String),
    AddUser,
    ToggleAdmin(String),
    AskRemove(String),
    CancelRemove,
    ConfirmRemove(String),
    AccountsReloaded,
    // Accounts ▸ Sign-in options (E10.6).
    Pin1(String),
    Pin2(String),
    SavePin,
    RemovePin,
    ChangePassword,
    // Devices ▸ Bluetooth (E12.2).
    BtLoaded(Box<crate::bluez::BtState>),
    BtPowered(bool),
    BtDiscover,
    BtPair(String),
    BtConnectToggle(String, bool), // (path, currently-connected)
    BtRemove(String),
    // Devices ▸ Printers (E12.4).
    PrintersLoaded(Box<crate::cups::CupsState>),
    PrintersDiscover,
    PrintersAdd(String, String), // (queue name, device uri)
    PrintersSetDefault(String),
    PrintersTest(String),
    PrintersRemove(String),
    PrintersSetupPdf,
    SetManageDefaultPrinter(bool),
    // Devices ▸ Mouse (E12.6).
    SetPrimaryButton(PrimaryButton),
    SetMouseNatural(bool),
    SetScrollLines(Lines),
    SetScrollInactive(bool),
    // Devices ▸ Touchpad (E12.7).
    SetTouchpadEnabled(bool),
    SetTouchpadSpeed(u8),
    SetTouchpadTap(bool),
    SetTouchpadTwoFinger(bool),
    SetTouchpadNatural(bool),
    // Devices ▸ Typing (E12.8).
    SetRepeatRate(RepeatRate),
    SetRepeatDelay(RepeatDelay),
    SetKbLayout(Layout),
    SetAutocorrect(bool),
    SetSuggestions(bool),
    // Devices ▸ AutoPlay (E12.9).
    SetAutoplayEnabled(bool),
    SetAutoplayRemovable(AutoAction),
    SetAutoplayMemcard(AutoAction),
    /// A do-nothing completion (for fire-and-forget off-thread side effects).
    Noop,
    // System ▸ Storage (E17.4).
    StorageLoaded(Box<crate::sysinfo::StorageUsage>),
    SetStorageSense(bool),
    ShowApps,
    BackFromApps,
    PackagesLoaded(Vec<crate::fedora::Package>),
    AskUninstall(String),
    CancelUninstall,
    ConfirmUninstall(String),
    ShowClean,
    BackFromClean,
    AskClean,
    CancelClean,
    ConfirmClean,
    Cleaned(u64),
    // Update & Security ▸ Backup (E17.6).
    SetBackupDrive(String),
    RemoveBackupDrive,
    SetAutoBackup(bool),
    // Backup ▸ More options (E17.7).
    ShowBackupMore,
    BackFromMore,
    AskBackupNow,
    CancelBackupNow,
    ConfirmBackupNow,
    BackedUp,
    SetSchedule(Schedule),
    SetRetention(Retention),
    AddInclude,
    IncludeAdded(Option<String>),
    RemoveInclude(String),
}

pub fn run(args: &[String]) -> ExitCode {
    // `mde settings storage …` — headless debug paths (E17.3/E17.4), checked
    // before the generic `--list` (category map) below.
    if args.iter().any(|a| a == "storage") {
        if args.iter().any(|a| a == "--list") {
            crate::sysinfo::print_storage_list();
            return ExitCode::SUCCESS;
        }
        if args.iter().any(|a| a == "--apps") {
            for p in crate::fedora::installed_packages() {
                println!("{:>12}  {}", crate::sysinfo::human_bytes(p.size), p.name);
            }
            return ExitCode::SUCCESS;
        }
        // `--remove <pkg>` prints the exact privileged uninstall command (dry-run).
        if let Some(i) = args.iter().position(|a| a == "--remove") {
            if let Some(pkg) = args.get(i + 1) {
                let cmd = crate::fedora::dnf_remove_cmd(pkg);
                println!("pkexec {}", cmd.join(" "));
                return ExitCode::SUCCESS;
            }
        }
        // `--clean-now [--dry-run]` runs the cleanup (E17.5). `--dry-run` deletes
        // nothing and needs no root — it reports the would-free estimate.
        if args.iter().any(|a| a == "--clean-now") {
            let dry = args.iter().any(|a| a == "--dry-run");
            let freed = crate::sysinfo::clean_now(dry);
            let verb = if dry { "would free" } else { "freed" };
            println!("{verb} {}", crate::sysinfo::human_bytes(freed));
            return ExitCode::SUCCESS;
        }
    }
    // `mde settings backup --add-drive <dev>` (E17.6): persist the Timeshift backup
    // device to menu.json and print the privileged config command (CI-safe; the GUI
    // runs the pkexec).
    if args.iter().any(|a| a == "backup") {
        if let Some(i) = args.iter().position(|a| a == "--add-drive") {
            if let Some(dev) = args.get(i + 1) {
                let mut st = crate::state::load();
                st.backup_drive = dev.clone();
                let _ = crate::state::save(&st);
                println!(
                    "backup drive set to {dev}\npkexec {}",
                    crate::sysinfo::timeshift_device_cmd(dev).join(" ")
                );
                return ExitCode::SUCCESS;
            }
        }
        // `--restore` opens the File-History snapshot restore browser (E17.8).
        if args.iter().any(|a| a == "--restore") {
            return crate::restore::run(args);
        }
        // `--backup-now [--dry-run]` — create a Timeshift snapshot (E17.7). Dry-run
        // prints the exact privileged command without running it (CI-safe).
        if args.iter().any(|a| a == "--backup-now") {
            let cmd = crate::sysinfo::timeshift_create_cmd();
            if args.iter().any(|a| a == "--dry-run") {
                println!("pkexec {}", cmd.join(" "));
            } else {
                let ok = std::process::Command::new("pkexec")
                    .args(&cmd)
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);
                println!(
                    "{}",
                    if ok {
                        "snapshot created"
                    } else {
                        "snapshot failed"
                    }
                );
            }
            return ExitCode::SUCCESS;
        }
    }
    if args.iter().any(|a| a == "--list") {
        return list();
    }
    // Dry-run: print the LightDM-greeter write script (E7.6) so its logic can be
    // verified without root — point MDE_LOCK_CONF at a temp file and run it.
    if args.first().map(String::as_str) == Some("--lock-script") {
        print!("{}", lock_script());
        return ExitCode::SUCCESS;
    }
    // Era gate (E10.7): the Settings app is the Windows 10 modern config surface;
    // the classic eras (Win2000 / Carbon) use the Control Panel. Under any non-Win10
    // theme, exit without drawing rather than render a modern app in a classic shell.
    // (main.rs has already set the palette from menu.json by now; the headless
    // --list/--lock-script debug paths above stay theme-independent for tests.)
    if !palette::is_windows10() {
        eprintln!(
            "mde settings: the Settings app is a Windows 10-era surface — use the Control Panel in this theme."
        );
        return ExitCode::SUCCESS;
    }
    // Parse a positional category name plus an optional `--page <name>` deep-link
    // (E7.3): `mde settings personalization --page taskbar`. A positional that
    // isn't a category pre-fills the Home search box instead (`mde settings
    // display`).
    let mut cat_arg = String::new();
    let mut page_arg: Option<String> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--page" {
            page_arg = it.next().cloned();
        } else if !a.starts_with("--") {
            if !cat_arg.is_empty() {
                cat_arg.push(' ');
            }
            cat_arg.push_str(a);
        }
    }
    let cat_arg = cat_arg.trim().to_string();
    // `mde settings storage`/`backup` are shortcuts straight to their pages (E17.4/
    // E17.6) — those are pages, not categories, so without this they'd fall through
    // to a Home search. (`recovery` joins as it lands.)
    let page_shortcut = match cat_arg.to_lowercase().as_str() {
        "storage" => Some(("system", "storage")),
        "backup" => Some(("update", "backup")),
        _ => None,
    };
    let (initial_cat, initial_page) = if let Some((cat, page)) = page_shortcut {
        let c = category_index(cat);
        let p = c.and_then(|c| page_index(c, page)).unwrap_or(0);
        (c, p)
    } else {
        let cat = category_index(&cat_arg);
        let page = match (cat, &page_arg) {
            (Some(c), Some(name)) => page_index(c, name).unwrap_or(0),
            _ => 0,
        };
        (cat, page)
    };
    let initial_search = if initial_cat.is_none() {
        cat_arg
    } else {
        String::new()
    };
    match gui(initial_cat, initial_page, initial_search) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mde settings: {e}");
            ExitCode::FAILURE
        }
    }
}

/// First category whose title contains `name` (case-insensitive).
fn category_index(name: &str) -> Option<usize> {
    let n = name.to_lowercase();
    CATEGORIES
        .iter()
        .position(|c| c.title.to_lowercase().contains(&n))
}

/// First page in `cat` whose title contains `name` (case-insensitive) — the
/// `--page` deep-link target.
fn page_index(cat: usize, name: &str) -> Option<usize> {
    let n = name.to_lowercase();
    CATEGORIES
        .get(cat)?
        .pages
        .iter()
        .position(|p| p.title.to_lowercase().contains(&n))
}

/// `mde settings --list` — print the page → backend map for every live page.
fn list() -> ExitCode {
    for cat in CATEGORIES {
        for p in cat.pages {
            let backend = match p.kind {
                Kind::Deferred => continue,
                Kind::Colors => "(native: Colors)".to_string(),
                Kind::Background => "(native: Background)".to_string(),
                Kind::Themes => "(native: Themes)".to_string(),
                Kind::Start => "(native: Start)".to_string(),
                Kind::Taskbar => "(native: Taskbar)".to_string(),
                Kind::Update => "(native: Update)".to_string(),
                Kind::UpdateHistory => "(native: Update history)".to_string(),
                Kind::UpdateAdvanced => "(native: Advanced options)".to_string(),
                Kind::NetworkStatus => "(native: Network status)".to_string(),
                Kind::Wifi => "(native: Wi-Fi)".to_string(),
                Kind::Ethernet => "(native: Ethernet)".to_string(),
                Kind::Vpn => "(native: VPN)".to_string(),
                Kind::Hotspot => "(native: Mobile hotspot)".to_string(),
                Kind::Proxy => "(native: Proxy)".to_string(),
                Kind::Airplane => "(native: Airplane mode)".to_string(),
                Kind::DataUsage => "(native: Data usage)".to_string(),
                Kind::Cellular => "(native: Cellular)".to_string(),
                Kind::AccountInfo => "(native: Your info)".to_string(),
                Kind::FamilyUsers => "(native: Family & other users)".to_string(),
                Kind::SignIn => "(native: Sign-in options)".to_string(),
                Kind::Bluetooth => "(native: Bluetooth)".to_string(),
                Kind::Printers => "(native: Printers & scanners)".to_string(),
                Kind::Mouse => "(native: Mouse)".to_string(),
                Kind::Touchpad => "(native: Touchpad)".to_string(),
                Kind::Typing => "(native: Typing)".to_string(),
                Kind::AutoPlay => "(native: AutoPlay)".to_string(),
                Kind::Storage => "(native: Storage)".to_string(),
                Kind::Backup => "(native: Backup)".to_string(),
                Kind::LockScreen => "(native: Lock screen)".to_string(),
                Kind::Mde(s) => format!("mde {s}"),
                Kind::Tool(c) => format!("tool: {c}"),
                Kind::Cmd(c, _) => format!("cmd: {c}"),
            };
            println!("{} \u{25b8} {} -> {}", cat.title, p.title, backend);
        }
    }
    ExitCode::SUCCESS
}

fn gui(initial: Option<usize>, initial_page: usize, initial_search: String) -> iced::Result {
    iced::application(|_: &Settings| "Settings - mde".to_string(), update, view)
        .theme(|_| mde_ui::palette::iced_theme())
        .window_size(iced::Size::new(940.0, 640.0))
        .font(mde_ui::font::REGULAR_BYTES)
        .font(mde_ui::font::BOLD_BYTES)
        .font(mde_ui::font::PLEX_REGULAR_BYTES)
        .font(mde_ui::font::PLEX_BOLD_BYTES)
        .default_font(mde_ui::font::ui())
        .run_with(move || {
            let st = crate::state::load();
            // Network status (E15.5): the active connections + the active one's zone.
            let net_conns = crate::nm::active_connections();
            let net_zone = net_conns
                .iter()
                .find(|c| c.state == "activated")
                .map(|c| crate::nm::connection_zone(&c.name))
                .unwrap_or_default();
            let mut s = Settings {
                view: initial.map(View::Category).unwrap_or(View::Home),
                page: initial_page,
                dark: st.theme_mode != "light",
                win10_accent: st.win10_accent,
                accent_on_taskbar: st.win10_accent_on_taskbar,
                search: initial_search,
                bg_source: BgSource::Picture,
                bg_wallpapers: wallpaper::scan(),
                bg_selected: None,
                bg_mode: BgMode::Fill,
                themes: st.themes.clone(),
                start_more_tiles: st.start_more_tiles,
                start_full_screen: st.start_full_screen,
                start_show_recent: st.start_show_recent,
                start_show_suggested: st.start_show_suggested,
                start_folders: st.start_folders.clone(),
                taskbar_loc: TaskbarLoc::from_key(&st.taskbar_location),
                search_mode: SearchMode::from_key(&st.win10_search_mode),
                show_taskview: st.win10_show_taskview,
                autohide: st.win10_autohide,
                small_buttons: st.win10_small_buttons,
                lock_selected: None,
                update_checking: false,
                update_status: None,
                update_installing: false,
                update_paused_until: st.update_paused_until,
                active_start: st.update_active_start,
                active_end: st.update_active_end,
                history: None,
                history_loading: false,
                history_selected: None,
                restart_asap: st.update_restart_asap,
                restart_notify: st.update_restart_notify,
                auto_mode: crate::sysinfo::auto_mode(),
                net_conns,
                net_zone,
                wifis: None,
                wifi_scanning: false,
                wifi_autoconnect: true,
                wifi_saved: Vec::new(),
                vpns: crate::nm::vpn_list(),
                hotspot_name: st.hotspot_name.clone(),
                hotspot_password: st.hotspot_password.clone(),
                proxy_mode: ProxyMode::from_key(&crate::nm::proxy_mode()),
                proxy_host: {
                    let (h, _) = crate::nm::proxy_http();
                    h
                },
                proxy_port: crate::nm::proxy_http().1,
                airplane: false,
                wifi_radio: false,
                display_name: st.display_name.clone(),
                account_picture: st.account_picture.clone(),
                bt: None,
                printers: None,
                manage_default: st.win10_manage_default_printer,
                printers_scanned: false,
                mouse_left_handed: st.mouse_left_handed,
                mouse_natural_scroll: st.mouse_natural_scroll,
                mouse_scroll_lines: st.mouse_scroll_lines,
                scroll_inactive: st.mouse_scroll_inactive,
                touchpad_present: crate::mouse::has_touchpad(),
                touchpad_enabled: st.touchpad_enabled,
                touchpad_speed: st.touchpad_speed,
                touchpad_tap: st.touchpad_tap,
                touchpad_two_finger: st.touchpad_two_finger,
                touchpad_natural_scroll: st.touchpad_natural_scroll,
                kb_repeat_rate: st.kb_repeat_rate,
                kb_repeat_delay: st.kb_repeat_delay,
                kb_layout: st.kb_layout.clone(),
                typing_autocorrect: st.typing_autocorrect,
                typing_suggestions: st.typing_suggestions,
                autoplay_enabled: st.autoplay_enabled,
                autoplay_removable: AutoAction::from_key(&st.autoplay_removable),
                autoplay_memcard: AutoAction::from_key(&st.autoplay_memcard),
                storage: None,
                storage_sense: st.storage_sense,
                // Gallery/bench seam: start in the Apps drill-in / Clean-now sub-view.
                storage_apps: std::env::var("MDE_STORAGE_VIEW").as_deref() == Ok("apps"),
                packages: Vec::new(),
                confirm_uninstall: None,
                storage_clean: std::env::var("MDE_STORAGE_VIEW").as_deref() == Ok("clean"),
                confirm_clean: false,
                last_freed: None,
                backup_drive: st.backup_drive.clone(),
                auto_backup: st.auto_backup,
                backup_more: std::env::var("MDE_BACKUP_VIEW").as_deref() == Ok("more"),
                confirm_backup: false,
                backup_schedule: Schedule::from_key(&st.backup_schedule),
                backup_retention: Retention::from_key(&st.backup_retention),
                backup_includes: if st.backup_includes.is_empty() {
                    seed_backup_includes()
                } else {
                    st.backup_includes.clone()
                },
                accounts: Vec::new(),
                new_user: String::new(),
                confirm_remove: None,
                pin_set: false,
                pin1: String::new(),
                pin2: String::new(),
                pin_msg: String::new(),
                usage: Vec::new(),
                data_limit: if st.data_limit_mb > 0 {
                    st.data_limit_mb.to_string()
                } else {
                    String::new()
                },
                installed: HashMap::new(),
            };
            cache_install(&mut s);
            // Auto-check when we land directly on the Update page (E13.3).
            let init = maybe_load(&mut s);
            (s, init)
        })
}

fn current_page(state: &Settings) -> Option<&'static Page> {
    match state.view {
        View::Category(c) => CATEGORIES.get(c).and_then(|cat| cat.pages.get(state.page)),
        View::Home => None,
    }
}

fn update(state: &mut Settings, message: Message) -> Task<Message> {
    match message {
        Message::OpenCategory(i) => {
            state.view = View::Category(i);
            state.page = 0;
            cache_install(state);
            return maybe_load(state);
        }
        Message::SelectPage(i) => {
            state.page = i;
            cache_install(state);
            return maybe_load(state);
        }
        Message::Back => {
            state.view = View::Home;
            state.page = 0;
        }
        Message::Search(q) => state.search = q,
        Message::Jump(c, p) => {
            state.view = View::Category(c);
            state.page = p;
            cache_install(state);
            return maybe_load(state);
        }
        Message::Open => open_current(state),
        Message::SetDark(d) => {
            state.dark = d;
            palette::set_dark(d);
            persist(state);
        }
        Message::SetAccentOnTaskbar(on) => {
            state.accent_on_taskbar = on;
            palette::set_accent_on_chrome(on);
            persist(state);
        }
        Message::BgSource(s) => state.bg_source = s,
        Message::BgSelect(i) => state.bg_selected = Some(i),
        Message::BgMode(m) => state.bg_mode = m,
        Message::BgBrowse => {
            return Task::perform(async { wallpaper::browse() }, Message::BgBrowsed);
        }
        Message::BgBrowsed(Some(p)) => {
            state.bg_wallpapers.push(p);
            state.bg_selected = Some(state.bg_wallpapers.len() - 1);
        }
        Message::BgBrowsed(None) => {}
        Message::BgApply => apply_background(state),
        Message::SaveTheme => save_theme(state),
        Message::ApplyTheme(i) => apply_theme(state, i),
        Message::SetStartMore(v) => {
            state.start_more_tiles = v;
            persist(state);
        }
        Message::SetStartFullScreen(v) => {
            state.start_full_screen = v;
            persist(state);
        }
        Message::SetStartRecent(v) => {
            state.start_show_recent = v;
            persist(state);
        }
        Message::SetStartSuggested(v) => {
            state.start_show_suggested = v;
            persist(state);
        }
        Message::ToggleStartFolder(key, on) => {
            state.start_folders.retain(|k| k != &key);
            if on {
                state.start_folders.push(key);
                // Keep the chosen set in the canonical START_FOLDERS order, so the
                // rail order is stable regardless of toggle sequence.
                state.start_folders.sort_by_key(|k| {
                    crate::start_win10::START_FOLDERS
                        .iter()
                        .position(|f| f.0 == k.as_str())
                        .unwrap_or(usize::MAX)
                });
            }
            persist(state);
        }
        Message::SetTaskbarLoc(loc) => {
            state.taskbar_loc = loc;
            persist(state);
        }
        Message::SetSearchMode(m) => {
            state.search_mode = m;
            persist(state);
        }
        Message::SetShowTaskview(v) => {
            state.show_taskview = v;
            persist(state);
        }
        Message::SetAutohide(v) => {
            state.autohide = v;
            persist(state);
        }
        Message::SetSmallButtons(v) => {
            state.small_buttons = v;
            persist(state);
        }
        Message::LockSelect(i) => state.lock_selected = Some(i),
        Message::LockBrowse => {
            return Task::perform(async { wallpaper::browse() }, Message::LockBrowsed);
        }
        Message::LockBrowsed(Some(p)) => {
            state.bg_wallpapers.push(p);
            state.lock_selected = Some(state.bg_wallpapers.len() - 1);
        }
        Message::LockBrowsed(None) => {}
        Message::LockApply => {
            if let Some(p) = state.lock_selected.and_then(|i| state.bg_wallpapers.get(i)) {
                set_lock_background(p);
            }
        }
        Message::CheckUpdates => {
            if !state.update_checking && !state.update_installing {
                state.update_checking = true;
                return check_updates_task();
            }
        }
        Message::UpdatesChecked(r) => {
            state.update_checking = false;
            // Restart-required toast (E13.7): if enabled and the pending set brings
            // a new kernel, notify (the E3 daemon renders it).
            if state.restart_notify {
                if let Ok(v) = &r {
                    if v.iter().any(|u| u.package.starts_with("kernel")) {
                        let _ = Command::new("notify-send")
                            .args([
                                "Restart required",
                                "A kernel update needs a restart to finish installing.",
                            ])
                            .spawn();
                    }
                }
            }
            state.update_status = Some(r);
        }
        Message::InstallUpdates => {
            let has = matches!(&state.update_status, Some(Ok(v)) if !v.is_empty());
            if has && !state.update_installing && !state.update_checking {
                state.update_installing = true;
                return install_updates_task();
            }
        }
        Message::UpdatesInstalled(ok) => {
            state.update_installing = false;
            if ok {
                // Re-check so the list reflects the post-upgrade state (→ empty).
                state.update_status = None;
                state.update_checking = true;
                return check_updates_task();
            }
        }
        Message::PauseUpdates => {
            // Step 7 days (cap 35) and mask the timer (E13.4). Optimistic + persist,
            // like the other privileged spawns in this module.
            state.update_paused_until = next_pause(state.update_paused_until, now_secs());
            let _ = Command::new("pkexec")
                .args(["systemctl", "mask", "--now", "dnf-automatic.timer"])
                .spawn();
            persist(state);
        }
        Message::ResumeUpdates => {
            state.update_paused_until = 0;
            let _ = Command::new("pkexec")
                .args(["systemctl", "unmask", "dnf-automatic.timer"])
                .spawn();
            persist(state);
        }
        Message::SetActiveStart(h) => {
            state.active_start = h;
            persist(state);
        }
        Message::SetActiveEnd(h) => {
            state.active_end = h;
            persist(state);
        }
        Message::SaveActiveHours => {
            // Override the dnf-automatic timer to run at the end of active hours.
            let _ = Command::new("pkexec")
                .args(["sh", "-c", &active_hours_override(state.active_end)])
                .spawn();
        }
        Message::HistoryFetched(r) => {
            state.history_loading = false;
            state.history = Some(r);
        }
        Message::SelectHistory(id) => state.history_selected = Some(id),
        Message::UninstallHistory => {
            if let Some(id) = state.history_selected {
                let _ = Command::new("pkexec")
                    .args(["dnf", "history", "undo", &id.to_string(), "-y"])
                    .spawn();
            }
        }
        Message::OpenRecovery => {
            // The existing System Restore / Timeshift path (best-effort launch).
            let _ = Command::new("timeshift-launcher").spawn();
        }
        Message::SetRestartAsap(v) => {
            state.restart_asap = v;
            // Write the dnf-automatic reboot policy (when-needed vs never).
            let _ = Command::new("pkexec")
                .args(["sh", "-c", &reboot_command(v)])
                .spawn();
            persist(state);
        }
        Message::SetRestartNotify(v) => {
            state.restart_notify = v;
            persist(state);
        }
        Message::SetAutoMode(m) => {
            // Write the posture to the timer + automatic.conf via the SAME helper
            // System Properties uses (E13.8), then re-read so both surfaces agree.
            let _ = Command::new("sh")
                .args(["-c", &crate::sysinfo::set_auto_command(m)])
                .spawn();
            state.auto_mode = m;
        }
        Message::SetNetPrivate(private) => {
            // Private = a trusted firewalld zone (home), Public = public (E15.5).
            let zone = if private { "home" } else { "public" };
            if let Some(c) = state.net_conns.iter().find(|c| c.state == "activated") {
                crate::nm::set_zone(&c.name, zone);
                state.net_zone = crate::nm::connection_zone(&c.name);
            }
        }
        Message::OpenNetEditor => {
            let _ = Command::new("nm-connection-editor").spawn();
        }
        Message::WifiScanned(list) => {
            state.wifi_scanning = false;
            state.wifis = Some(list);
        }
        Message::ConnectSsid(ssid) => {
            // Hand the connect (key prompt) to the flyout's flow, pre-selected.
            let exe = std::env::current_exe().unwrap_or_else(|_| "mde".into());
            let _ = Command::new(exe)
                .args(["net-flyout", "--select", &ssid])
                .spawn();
        }
        Message::ForgetSsid(ssid) => {
            crate::nm::forget_wifi(&ssid);
            state.wifi_saved = crate::nm::saved_wifi();
            state.wifis = None; // re-scan so the row flips Forget→Connect
            state.wifi_scanning = true;
            return wifi_scan_task();
        }
        Message::SetWifiAutoconnect(on) => {
            crate::nm::set_wifi_autoconnect(on);
            state.wifi_autoconnect = on;
        }
        Message::VpnToggle(name, up) => {
            crate::nm::vpn_up_down(&name, up);
            state.vpns = crate::nm::vpn_list(); // refresh the active state
        }
        Message::AddVpn => {
            let _ = Command::new("nm-connection-editor").spawn();
        }
        Message::SetHotspotOn(on) => {
            crate::nm::set_hotspot(on, &state.hotspot_name, &state.hotspot_password);
            state.net_conns = crate::nm::active_connections(); // reflect Hotspot
        }
        Message::HotspotName(n) => {
            state.hotspot_name = n;
            persist(state);
        }
        Message::HotspotPassword(p) => {
            state.hotspot_password = p;
            persist(state);
        }
        Message::SetProxyMode(m) => {
            crate::nm::set_proxy_mode(m.key());
            state.proxy_mode = m;
        }
        Message::ProxyHost(h) => state.proxy_host = h,
        Message::ProxyPort(p) => {
            state.proxy_port = p.chars().filter(|c| c.is_ascii_digit()).collect()
        }
        Message::ApplyProxy => {
            crate::nm::set_proxy_http(&state.proxy_host, &state.proxy_port);
        }
        Message::SetAirplane(on) => {
            crate::nm::set_airplane(on);
            state.airplane = crate::nm::airplane_on();
            state.wifi_radio = crate::nm::wifi_enabled();
        }
        Message::SetWifiRadio(on) => {
            crate::nm::radio_wifi(on);
            state.wifi_radio = crate::nm::wifi_enabled();
        }
        Message::SetDataLimit(s) => {
            state.data_limit = s.chars().filter(|c| c.is_ascii_digit()).collect();
            persist(state);
            // Notify if the current usage already exceeds the new limit (E15.11).
            let limit: u64 = state.data_limit.parse().unwrap_or(0);
            let used_mb = state.usage.iter().map(|(_, rx, tx)| rx + tx).sum::<u64>() / 1_000_000;
            if limit > 0 && used_mb > limit {
                let _ = Command::new("notify-send")
                    .args([
                        "Data limit reached",
                        &format!("You've used {used_mb} MB of your {limit} MB limit."),
                    ])
                    .spawn();
            }
        }
        Message::DisplayName(n) => {
            state.display_name = n;
            persist(state);
        }
        Message::BrowseAvatar => {
            return Task::perform(async { wallpaper::browse() }, Message::AvatarBrowsed);
        }
        Message::AvatarBrowsed(Some(src)) => {
            // Copy the chosen image to ~/.face (the standard avatar) and record it.
            if let Some(face) = std::env::var_os("HOME").map(|h| {
                let mut p = std::path::PathBuf::from(h);
                p.push(".face");
                p
            }) {
                if std::fs::copy(&src, &face).is_ok() {
                    state.account_picture = face.display().to_string();
                    persist(state);
                }
            }
        }
        Message::AvatarBrowsed(None) => {}
        Message::NewUser(n) => state.new_user = n,
        Message::AddUser => {
            let name = crate::sysinfo::sanitize_login(&state.new_user);
            if name.is_empty() {
                return Task::none();
            }
            state.new_user.clear();
            return pkexec_then_reload(crate::sysinfo::useradd_cmd(&name));
        }
        Message::ToggleAdmin(name) => {
            let is_admin = state.accounts.iter().any(|a| a.name == name && a.admin);
            // Guard: never demote the last administrator.
            if is_admin && crate::sysinfo::admin_count(&state.accounts) <= 1 {
                return Task::none();
            }
            return pkexec_then_reload(crate::sysinfo::set_admin_cmd(&name, !is_admin));
        }
        Message::AskRemove(name) => state.confirm_remove = Some(name),
        Message::CancelRemove => state.confirm_remove = None,
        Message::ConfirmRemove(name) => {
            state.confirm_remove = None;
            // Guard: never delete the signed-in account.
            if name == std::env::var("USER").unwrap_or_default() {
                return Task::none();
            }
            return pkexec_then_reload(crate::sysinfo::userdel_cmd(&name));
        }
        Message::AccountsReloaded => state.accounts = crate::sysinfo::accounts(),
        // --- Devices ▸ Bluetooth (E12.2) — every action runs off-thread, then
        // re-reads the adapter so the list reflects what actually changed.
        Message::BtLoaded(s) => state.bt = Some(*s),
        Message::BtPowered(on) => {
            return bt_action_task(move || crate::bluez::set_powered(on));
        }
        Message::BtDiscover => {
            return bt_action_task(|| crate::bluez::set_discovery(true));
        }
        Message::BtPair(path) => {
            return bt_action_task(move || crate::bluez::pair(&path));
        }
        Message::BtConnectToggle(path, connected) => {
            return bt_action_task(move || {
                if connected {
                    crate::bluez::disconnect(&path)
                } else {
                    crate::bluez::connect(&path)
                }
            });
        }
        Message::BtRemove(path) => {
            return bt_action_task(move || crate::bluez::remove(&path));
        }
        // --- Devices ▸ Printers (E12.4). Set-default + test page run as the user;
        // add/remove change the system queue config and go through pkexec. Each
        // re-reads the CUPS list so the page reflects what actually changed.
        Message::PrintersLoaded(s) => state.printers = Some(*s),
        Message::PrintersDiscover => {
            state.printers_scanned = true;
            return printers_discover_task();
        }
        Message::PrintersSetDefault(name) => {
            return printers_action_task(move || crate::cups::set_default(&name));
        }
        Message::PrintersTest(name) => {
            return printers_action_task(move || crate::cups::print_test_page(&name));
        }
        Message::PrintersAdd(name, uri) => {
            return printers_pkexec_task(crate::cups::add_cmd(&name, &uri));
        }
        Message::PrintersRemove(name) => {
            return printers_pkexec_task(crate::cups::remove_cmd(&name));
        }
        Message::PrintersSetupPdf => {
            // Install cups-pdf + create the queue in one pkexec, then re-read (E12.5).
            return printers_pkexec_task(crate::cups::ensure_pdf_cmd());
        }
        Message::SetManageDefaultPrinter(on) => {
            state.manage_default = on;
            persist(state);
        }
        // Devices ▸ Mouse (E12.6) — the first three rewrite rc.xml + reconfigure;
        // "scroll inactive windows" is advisory (persisted, never in rc.xml).
        Message::SetPrimaryButton(b) => {
            state.mouse_left_handed = b == PrimaryButton::Right;
            apply_libinput(state);
        }
        Message::SetMouseNatural(on) => {
            state.mouse_natural_scroll = on;
            apply_libinput(state);
        }
        Message::SetScrollLines(Lines(n)) => {
            state.mouse_scroll_lines = n;
            apply_libinput(state);
        }
        Message::SetScrollInactive(on) => {
            state.scroll_inactive = on;
            persist(state);
        }
        // Devices ▸ Touchpad (E12.7) — each rewrites rc.xml's touchpad device.
        Message::SetTouchpadEnabled(on) => {
            state.touchpad_enabled = on;
            apply_libinput(state);
        }
        Message::SetTouchpadSpeed(n) => {
            state.touchpad_speed = n;
            apply_libinput(state);
        }
        Message::SetTouchpadTap(on) => {
            state.touchpad_tap = on;
            apply_libinput(state);
        }
        Message::SetTouchpadTwoFinger(on) => {
            state.touchpad_two_finger = on;
            apply_libinput(state);
        }
        Message::SetTouchpadNatural(on) => {
            state.touchpad_natural_scroll = on;
            apply_libinput(state);
        }
        // Devices ▸ Typing (E12.8) — rate/delay rewrite rc.xml's <keyboard>; layout
        // writes the labwc environment file (next sign-in); the two toggles are
        // advisory (persisted only).
        Message::SetRepeatRate(r) => {
            state.kb_repeat_rate = r.rate();
            apply_keyboard(state);
        }
        Message::SetRepeatDelay(d) => {
            state.kb_repeat_delay = d.delay();
            apply_keyboard(state);
        }
        Message::SetKbLayout(l) => {
            state.kb_layout = l.code.to_string();
            persist(state);
            let _ = crate::keyboard::apply_layout(l.code);
        }
        Message::SetAutocorrect(on) => {
            state.typing_autocorrect = on;
            persist(state);
        }
        Message::SetSuggestions(on) => {
            state.typing_suggestions = on;
            persist(state);
        }
        // Devices ▸ AutoPlay (E12.9) — persist only; `mde devices-monitor` re-reads
        // menu.json on each mount event, so no live action is needed here.
        Message::SetAutoplayEnabled(on) => {
            state.autoplay_enabled = on;
            persist(state);
        }
        Message::SetAutoplayRemovable(a) => {
            state.autoplay_removable = a;
            persist(state);
        }
        Message::SetAutoplayMemcard(a) => {
            state.autoplay_memcard = a;
            persist(state);
        }
        Message::Noop => {}
        // System ▸ Storage (E17.4).
        Message::StorageLoaded(s) => state.storage = Some(*s),
        Message::SetStorageSense(on) => {
            state.storage_sense = on;
            persist(state);
            // Write + (en/dis)able the --user timer off-thread (systemctl can block).
            return Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || crate::sysinfo::apply_storage_sense(on))
                        .await
                        .ok();
                },
                |_| Message::Noop,
            );
        }
        Message::ShowApps => {
            state.storage_apps = true;
            if state.packages.is_empty() {
                return packages_load_task();
            }
        }
        Message::BackFromApps => {
            state.storage_apps = false;
            state.confirm_uninstall = None;
        }
        Message::PackagesLoaded(p) => state.packages = p,
        Message::AskUninstall(name) => state.confirm_uninstall = Some(name),
        Message::CancelUninstall => state.confirm_uninstall = None,
        Message::ConfirmUninstall(name) => {
            state.confirm_uninstall = None;
            // pkexec dnf remove, then re-read the package list.
            let args = crate::fedora::dnf_remove_cmd(&name);
            return Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        let _ = std::process::Command::new("pkexec").args(&args).status();
                        crate::fedora::installed_packages()
                    })
                    .await
                    .unwrap_or_default()
                },
                Message::PackagesLoaded,
            );
        }
        Message::ShowClean => state.storage_clean = true,
        Message::BackFromClean => {
            state.storage_clean = false;
            state.confirm_clean = false;
        }
        Message::AskClean => state.confirm_clean = true,
        Message::CancelClean => state.confirm_clean = false,
        Message::ConfirmClean => {
            state.confirm_clean = false;
            // The cleanup shells pkexec; run it off the UI thread (E17.5).
            return Task::perform(
                async {
                    tokio::task::spawn_blocking(|| crate::sysinfo::clean_now(false))
                        .await
                        .unwrap_or(0)
                },
                Message::Cleaned,
            );
        }
        Message::Cleaned(freed) => {
            state.last_freed = Some(freed);
            // Re-read the breakdown so the bars reflect the reclaimed space.
            state.storage = None;
            let _ = std::process::Command::new("notify-send")
                .args([
                    "Storage cleaned",
                    &format!("Freed {}", crate::sysinfo::human_bytes(freed)),
                ])
                .spawn();
            return storage_load_task();
        }
        // Update & Security ▸ Backup (E17.6).
        Message::SetBackupDrive(dev) => {
            state.backup_drive = dev.clone();
            persist(state);
            // Point Timeshift at the chosen device (root-level config), off-thread.
            let args = crate::sysinfo::timeshift_device_cmd(&dev);
            return Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        let _ = std::process::Command::new("pkexec").args(&args).status();
                    })
                    .await
                    .ok();
                },
                |_| Message::Noop,
            );
        }
        Message::RemoveBackupDrive => {
            state.backup_drive.clear();
            persist(state);
        }
        Message::SetAutoBackup(on) => {
            state.auto_backup = on;
            persist(state);
        }
        // Backup ▸ More options (E17.7).
        Message::ShowBackupMore => state.backup_more = true,
        Message::BackFromMore => {
            state.backup_more = false;
            state.confirm_backup = false;
        }
        Message::AskBackupNow => state.confirm_backup = true,
        Message::CancelBackupNow => state.confirm_backup = false,
        Message::ConfirmBackupNow => {
            state.confirm_backup = false;
            let cmd = crate::sysinfo::timeshift_create_cmd();
            return Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        let _ = std::process::Command::new("pkexec").args(&cmd).status();
                    })
                    .await
                    .ok();
                },
                |_| Message::BackedUp,
            );
        }
        Message::BackedUp => {
            let _ = std::process::Command::new("notify-send")
                .args(["Backup", "Snapshot created"])
                .spawn();
        }
        Message::SetSchedule(s) => {
            state.backup_schedule = s;
            persist(state);
            // Write + enable the --user backup timer with the new OnCalendar.
            let cal = s.key().to_string();
            return Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        crate::sysinfo::apply_backup_schedule(&cal)
                    })
                    .await
                    .ok();
                },
                |_| Message::Noop,
            );
        }
        Message::SetRetention(r) => {
            state.backup_retention = r;
            persist(state);
        }
        Message::AddInclude => return pick_folder_task(),
        Message::IncludeAdded(Some(p)) => {
            if !state.backup_includes.contains(&p) {
                state.backup_includes.push(p);
                persist(state);
            }
        }
        Message::IncludeAdded(None) => {}
        Message::RemoveInclude(p) => {
            state.backup_includes.retain(|x| x != &p);
            persist(state);
        }
        // Keep PIN entry to digits (a Windows-style numeric PIN), capped at 8.
        Message::Pin1(p) => {
            state.pin1 = p.chars().filter(char::is_ascii_digit).take(8).collect();
            state.pin_msg.clear();
        }
        Message::Pin2(p) => {
            state.pin2 = p.chars().filter(char::is_ascii_digit).take(8).collect();
            state.pin_msg.clear();
        }
        Message::SavePin => {
            if state.pin1.len() < 4 {
                state.pin_msg = "PIN must be at least 4 digits.".into();
            } else if state.pin1 != state.pin2 {
                state.pin_msg = "The PINs don't match.".into();
            } else {
                match crate::pin::set_pin(&state.pin1) {
                    Ok(()) => {
                        state.pin_set = true;
                        state.pin_msg = "PIN saved.".into();
                    }
                    Err(e) => state.pin_msg = format!("Couldn't save PIN: {e}"),
                }
                state.pin1.clear();
                state.pin2.clear();
            }
        }
        Message::RemovePin => {
            let _ = crate::pin::clear();
            state.pin_set = false;
            state.pin1.clear();
            state.pin2.clear();
            state.pin_msg = "PIN removed.".into();
        }
        Message::ChangePassword => {
            let user = std::env::var("USER").unwrap_or_default();
            let inner =
                format!("pkexec passwd '{user}'; printf '\\nPress Enter to close… '; read _");
            crate::installer::spawn_terminal(
                "Change password",
                &palette::hex(palette::HIGHLIGHT),
                &inner,
            );
        }
    }
    Task::none()
}

/// Run a privileged user-management command via `pkexec` (raising the polkit
/// prompt) off the UI thread, then refresh the account list once it finishes so
/// the page reflects what actually changed (E10.5).
fn pkexec_then_reload(args: Vec<String>) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let _ = std::process::Command::new("pkexec").args(&args).status();
            })
            .await
            .ok();
        },
        |_| Message::AccountsReloaded,
    )
}

/// The privileged script (`pkexec sh -c`) that sets the dnf-automatic `reboot`
/// policy (E13.7): `when-needed` when restart-ASAP is on, else `never`. Pure for
/// testing; the `sed` matches the same whitespace-tolerant key form as
/// [`active_hours_override`].
fn reboot_command(asap: bool) -> String {
    let val = if asap { "when-needed" } else { "never" };
    format!(
        "sed -i -E 's/^[[:space:]]*reboot[[:space:]]*=.*/reboot = {val}/' /etc/dnf/automatic.conf"
    )
}

/// One `dnf history list` transaction (E13.6): the id + command + date + action +
/// the count of packages altered.
#[derive(Debug, Clone, PartialEq, Eq)]
struct HistoryEntry {
    id: u32,
    command: String,
    date: String,
    action: String,
    altered: String,
}

/// Whether `t` is a `YYYY-MM-DD` date token (the anchor for column-splitting).
fn is_date(t: &str) -> bool {
    t.len() == 10
        && t.bytes().enumerate().all(|(i, b)| {
            if i == 4 || i == 7 {
                b == b'-'
            } else {
                b.is_ascii_digit()
            }
        })
}

/// Parse `dnf history list` output into transactions (E13.6). dnf5 prints
/// whitespace-aligned columns `ID  Command line  Date and time  Action(s)
/// Altered` where the command + action hold spaces, so split on the
/// `YYYY-MM-DD HH:MM:SS` date: everything before is the command, the trailing
/// number is Altered, the middle is Action(s). The "ID" header has no numeric id.
fn parse_history(out: &str) -> Vec<HistoryEntry> {
    let mut v = Vec::new();
    for line in out.lines() {
        let t: Vec<&str> = line.split_whitespace().collect();
        if t.len() < 4 {
            continue;
        }
        let Ok(id) = t[0].parse::<u32>() else {
            continue; // skips the "ID …" header
        };
        let Some(d) = t.iter().position(|x| is_date(x)) else {
            continue;
        };
        if d < 1 || d + 1 >= t.len() {
            continue;
        }
        let date = format!("{} {}", t[d], t[d + 1]);
        let (action, altered) = match t[(d + 2).min(t.len())..].split_last() {
            Some((last, head)) => (head.join(" "), last.to_string()),
            None => (String::new(), String::new()),
        };
        v.push(HistoryEntry {
            id,
            command: t[1..d].join(" "),
            date,
            action,
            altered,
        });
    }
    v
}

/// Run `dnf history list` off the UI thread (E13.6) and parse the transactions.
fn fetch_history_task() -> Task<Message> {
    Task::perform(
        async {
            tokio::task::spawn_blocking(|| {
                match std::process::Command::new("dnf")
                    .args(["history", "list"])
                    .output()
                {
                    Ok(o) => Ok(parse_history(&String::from_utf8_lossy(&o.stdout))),
                    Err(e) => Err(format!("dnf is not available: {e}")),
                }
            })
            .await
            .unwrap_or_else(|e| Err(format!("history task failed: {e}")))
        },
        Message::HistoryFetched,
    )
}

/// The privileged script (run via `pkexec sh -c`) that overrides the dnf-automatic
/// timer to fire at the end of active hours (E13.5): a drop-in whose empty
/// `OnCalendar=` clears the packaged schedule, then the next sets ours, followed by
/// a daemon-reload. Pure, so the schedule string is unit-tested without root.
fn active_hours_override(end: u8) -> String {
    format!(
        "mkdir -p /etc/systemd/system/dnf-automatic.timer.d && \
         printf '[Timer]\\nOnCalendar=\\nOnCalendar=*-*-* {end:02}:00:00\\n' \
         > /etc/systemd/system/dnf-automatic.timer.d/override.conf && systemctl daemon-reload"
    )
}

/// Now in Unix seconds (0 if the clock is before the epoch, which never happens).
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Next "paused until" time (E13.4): step 7 days from the later of `now` / the
/// current pause end, capped at 35 days from `now`. Pure, so the stepping + cap
/// are unit-tested without touching the clock.
fn next_pause(current_until: u64, now: u64) -> u64 {
    const DAY: u64 = 86_400;
    (current_until.max(now) + 7 * DAY).min(now + 35 * DAY)
}

/// Auto-load a page's data when it first becomes visible (E13.3/E13.6) — a
/// `dnf check-update` for the Update page, a `dnf history list` for the Update
/// history page — so opening it shows current state like Windows 10. A no-op on
/// any other page, or once the load is in flight/done.
fn maybe_load(state: &mut Settings) -> Task<Message> {
    match current_page(state).map(|p| p.kind) {
        Some(Kind::Update) if state.update_status.is_none() && !state.update_checking => {
            state.update_checking = true;
            check_updates_task()
        }
        Some(Kind::UpdateHistory) if state.history.is_none() && !state.history_loading => {
            state.history_loading = true;
            fetch_history_task()
        }
        Some(Kind::Wifi) if state.wifis.is_none() && !state.wifi_scanning => {
            state.wifi_scanning = true;
            state.wifi_autoconnect = crate::nm::wifi_autoconnect();
            state.wifi_saved = crate::nm::saved_wifi();
            wifi_scan_task()
        }
        Some(Kind::Airplane) => {
            // Cheap sync reads; refresh on each visit so the toggles match reality.
            state.airplane = crate::nm::airplane_on();
            state.wifi_radio = crate::nm::wifi_enabled();
            Task::none()
        }
        Some(Kind::DataUsage) => {
            state.usage = crate::nm::all_device_bytes();
            Task::none()
        }
        Some(Kind::FamilyUsers) => {
            // Cheap synchronous /etc read; refresh on each visit (E10.5 mutates it).
            state.accounts = crate::sysinfo::accounts();
            Task::none()
        }
        Some(Kind::SignIn) => {
            state.pin_set = crate::pin::is_set();
            Task::none()
        }
        Some(Kind::Bluetooth) if state.bt.is_none() => bt_load_task(),
        Some(Kind::Printers) if state.printers.is_none() => printers_load_task(),
        Some(Kind::Storage) => {
            let mut tasks = Vec::new();
            if state.storage.is_none() {
                tasks.push(storage_load_task());
            }
            if state.storage_apps && state.packages.is_empty() {
                tasks.push(packages_load_task());
            }
            Task::batch(tasks)
        }
        // Backup reuses the storage breakdown for its candidate-drive list (E17.6).
        Some(Kind::Backup) if state.storage.is_none() => storage_load_task(),
        _ => Task::none(),
    }
}

/// Read the BlueZ adapter + device list off the UI thread (E12.2) — the zbus
/// calls block, so they run on tokio's blocking pool and deliver `Message::BtLoaded`.
fn bt_load_task() -> Task<Message> {
    Task::perform(
        async {
            tokio::task::spawn_blocking(crate::bluez::state)
                .await
                .unwrap_or_default()
        },
        |s| Message::BtLoaded(Box::new(s)),
    )
}

/// Run a BlueZ mutation off-thread, then re-read the adapter so the page reflects
/// the change (E12.2).
fn bt_action_task<F: FnOnce() + Send + 'static>(action: F) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                action();
                crate::bluez::state()
            })
            .await
            .unwrap_or_default()
        },
        |s| Message::BtLoaded(Box::new(s)),
    )
}

/// Seed the backup include list from the XDG user dirs that exist (E17.7).
fn seed_backup_includes() -> Vec<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    ["Documents", "Pictures", "Videos", "Music", "Downloads"]
        .iter()
        .map(|d| format!("{home}/{d}"))
        .filter(|p| std::path::Path::new(p).is_dir())
        .collect()
}

/// Pop `mde filedialog` to pick a folder, returning the chosen path (E17.7).
fn pick_folder_task() -> Task<Message> {
    Task::perform(
        async {
            tokio::task::spawn_blocking(|| {
                let exe = std::env::current_exe().ok()?;
                let out = std::process::Command::new(exe)
                    .args(["filedialog", "--title", "Add a folder to back up"])
                    .output()
                    .ok()?;
                let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
                (!p.is_empty()).then_some(p)
            })
            .await
            .unwrap_or(None)
        },
        Message::IncludeAdded,
    )
}

/// Read the storage breakdown off the UI thread (E17.4) — `df`/`du`/`rpm` are slow,
/// so they run on tokio's blocking pool and deliver `Message::StorageLoaded`.
fn storage_load_task() -> Task<Message> {
    Task::perform(
        async {
            tokio::task::spawn_blocking(crate::sysinfo::storage_usage)
                .await
                .unwrap_or_default()
        },
        |s| Message::StorageLoaded(Box::new(s)),
    )
}

/// Read the installed-package list off the UI thread for the Apps drill-in (E17.4).
fn packages_load_task() -> Task<Message> {
    Task::perform(
        async {
            tokio::task::spawn_blocking(crate::fedora::installed_packages)
                .await
                .unwrap_or_default()
        },
        Message::PackagesLoaded,
    )
}

/// Read the CUPS queue list off the UI thread (E12.4) — `lpstat` shells out, so it
/// runs on tokio's blocking pool and delivers `Message::PrintersLoaded`.
fn printers_load_task() -> Task<Message> {
    Task::perform(
        async {
            tokio::task::spawn_blocking(crate::cups::state)
                .await
                .unwrap_or_default()
        },
        |s| Message::PrintersLoaded(Box::new(s)),
    )
}

/// Discovery scan for "+ Add a printer": re-read the queues *and* probe for new
/// devices (`lpinfo`, slow), merging both into one snapshot (E12.4).
fn printers_discover_task() -> Task<Message> {
    Task::perform(
        async {
            tokio::task::spawn_blocking(|| {
                let mut st = crate::cups::state();
                st.discovered = crate::cups::discover();
                st
            })
            .await
            .unwrap_or_default()
        },
        |s| Message::PrintersLoaded(Box::new(s)),
    )
}

/// Run a per-user CUPS action (set-default / test page) off-thread, then re-read
/// the queue list (E12.4).
fn printers_action_task<F: FnOnce() + Send + 'static>(action: F) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                action();
                crate::cups::state()
            })
            .await
            .unwrap_or_default()
        },
        |s| Message::PrintersLoaded(Box::new(s)),
    )
}

/// Run an `lpadmin` add/remove through `pkexec` (system queue change), then
/// re-read the list (E12.4) — the same privilege path Accounts uses.
fn printers_pkexec_task(args: Vec<String>) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let _ = std::process::Command::new("pkexec").args(&args).status();
                crate::cups::state()
            })
            .await
            .unwrap_or_default()
        },
        |s| Message::PrintersLoaded(Box::new(s)),
    )
}

/// Scan Wi-Fi off the UI thread (E15.6) — `nmcli dev wifi` can take seconds, so it
/// runs on tokio's blocking pool and delivers via `Message::WifiScanned`.
fn wifi_scan_task() -> Task<Message> {
    Task::perform(
        async {
            tokio::task::spawn_blocking(crate::nm::wifi_list)
                .await
                .unwrap_or_default()
        },
        Message::WifiScanned,
    )
}

/// Run `pkexec dnf upgrade -y` off the UI thread (E13.3 Install). Best-effort; the
/// result drives a re-check so the list reflects what actually installed.
fn install_updates_task() -> Task<Message> {
    Task::perform(
        async {
            tokio::task::spawn_blocking(|| {
                std::process::Command::new("pkexec")
                    .args(["dnf", "upgrade", "-y"])
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false)
            })
            .await
            .unwrap_or(false)
        },
        Message::UpdatesInstalled,
    )
}

/// Run `dnf check-update` off the UI thread for the Update page (E13.2), reusing
/// `packages::parse_check_update`. dnf exits 0 (up to date), 100 (updates), or
/// other (error) — so "up to date" is distinct from "couldn't check".
fn check_updates_task() -> Task<Message> {
    Task::perform(
        async {
            tokio::task::spawn_blocking(|| {
                let out = match std::process::Command::new("dnf")
                    .args(["check-update", "-q"])
                    .output()
                {
                    Ok(o) => o,
                    Err(e) => return Err(format!("dnf is not available: {e}")),
                };
                match out.status.code() {
                    Some(0) => Ok(Vec::new()),
                    Some(100) => Ok(crate::packages::parse_check_update(
                        &String::from_utf8_lossy(&out.stdout),
                    )),
                    _ => {
                        let err = String::from_utf8_lossy(&out.stderr);
                        Err(if err.trim().is_empty() {
                            "dnf check-update failed".to_string()
                        } else {
                            err.trim().to_string()
                        })
                    }
                }
            })
            .await
            .unwrap_or_else(|e| Err(format!("check task failed: {e}")))
        },
        Message::UpdatesChecked,
    )
}

/// Capture the current background + accent + mode as a new saved theme bundle.
fn save_theme(state: &mut Settings) {
    let wallpaper = state
        .bg_selected
        .and_then(|i| state.bg_wallpapers.get(i))
        .cloned()
        .unwrap_or_default();
    let theme = crate::state::SavedTheme {
        name: format!("Custom {}", state.themes.len() + 1),
        wallpaper,
        accent: state.win10_accent,
        dark: state.dark,
    };
    state.themes.push(theme);
    let mut st = crate::state::load();
    st.themes = state.themes.clone();
    let _ = crate::state::save(&st);
}

/// Apply a saved theme bundle: wallpaper (swaybg) + mode. (The accent is no
/// longer per-theme — Carbon Blue is fixed across eras after the rebrand.)
fn apply_theme(state: &mut Settings, i: usize) {
    let Some(t) = state.themes.get(i).cloned() else {
        return;
    };
    if !t.wallpaper.is_empty() {
        let _ = outputs::set_wallpaper(&t.wallpaper, state.bg_mode.swaybg());
    }
    state.dark = t.dark;
    palette::set_dark(t.dark);
    persist(state);
}

/// The sh script that sets the LightDM greeter `background=` to `$1`, updating
/// the value inside `[greeter]` without clobbering the rest of the conf. The
/// path arrives as `$1` (never embedded in the program text) and the value is
/// passed to awk via `-v`, so neither can inject. The conf path honours
/// `$MDE_LOCK_CONF` so the logic is testable without root (E7.6).
fn lock_script() -> String {
    "set -e\n\
     f=\"${MDE_LOCK_CONF:-/etc/lightdm/lightdm-gtk-greeter.conf}\"\n\
     bg=\"$1\"\n\
     mkdir -p \"$(dirname \"$f\")\"\n\
     [ -f \"$f\" ] || printf '[greeter]\\n' > \"$f\"\n\
     grep -q '^\\[greeter\\]' \"$f\" || printf '\\n[greeter]\\n' >> \"$f\"\n\
     awk -v v=\"$bg\" '\n\
     /^\\[greeter\\]/ { print; print \"background=\" v; ingreeter=1; next }\n\
     /^\\[/ { ingreeter=0 }\n\
     ingreeter && /^[[:space:]]*background=/ { next }\n\
     { print }\n\
     ' \"$f\" > \"$f.tmp\" && mv \"$f.tmp\" \"$f\"\n"
        .to_string()
}

/// Set the LightDM greeter (lock-screen) background to `path` via pkexec (the
/// only way to write `/etc`; needs an interactive auth agent — E7.6).
fn set_lock_background(path: &str) {
    let _ = Command::new("pkexec")
        .arg("sh")
        .arg("-c")
        .arg(lock_script())
        .arg("mde-lock")
        .arg(path)
        .status();
}

/// Apply (and persist) the Background page's current selection via swaybg.
fn apply_background(state: &Settings) {
    let mode = state.bg_mode.swaybg();
    match state.bg_source {
        BgSource::Picture => {
            if let Some(p) = state.bg_selected.and_then(|i| state.bg_wallpapers.get(i)) {
                let _ = outputs::set_wallpaper(p, mode);
            }
        }
        BgSource::Solid => {
            // "Solid" = the themed desktop color (no separate color picker yet).
            let _ = outputs::set_solid(&palette::hex(palette::BACKGROUND));
        }
        BgSource::Slideshow => {
            let _ = outputs::set_slideshow(&state.bg_wallpapers, mode, 300);
        }
    }
}

/// Compute (once) whether the current Tool page's command is installed.
fn cache_install(state: &mut Settings) {
    if let Some(Page {
        kind: Kind::Tool(cmd),
        ..
    }) = current_page(state)
    {
        if !state.installed.contains_key(cmd) {
            let present = tool_by_cmd(cmd).map(fedora::is_installed).unwrap_or(false);
            state.installed.insert(cmd, present);
        }
    }
}

fn tool_by_cmd(cmd: &str) -> Option<&'static fedora::Tool> {
    fedora::TOOLS.iter().find(|t| t.command == cmd)
}

/// Open the current page's backend. Tool pages install-if-missing (like the
/// Control Panel); everything else spawns directly.
fn open_current(state: &mut Settings) {
    let Some(page) = current_page(state) else {
        return;
    };
    match page.kind {
        Kind::Deferred
        | Kind::Colors
        | Kind::Background
        | Kind::Themes
        | Kind::Start
        | Kind::Taskbar
        | Kind::Update
        | Kind::UpdateHistory
        | Kind::UpdateAdvanced
        | Kind::NetworkStatus
        | Kind::Wifi
        | Kind::Ethernet
        | Kind::Vpn
        | Kind::Hotspot
        | Kind::Proxy
        | Kind::Airplane
        | Kind::DataUsage
        | Kind::Cellular
        | Kind::AccountInfo
        | Kind::FamilyUsers
        | Kind::SignIn
        | Kind::Bluetooth
        | Kind::Printers
        | Kind::Mouse
        | Kind::Touchpad
        | Kind::Typing
        | Kind::AutoPlay
        | Kind::Storage
        | Kind::Backup
        | Kind::LockScreen => {}
        Kind::Mde(sub) => {
            let mde = mde_path();
            let _ = Command::new(mde).arg(sub).spawn();
        }
        Kind::Tool(cmd) => {
            if let Some(tool) = tool_by_cmd(cmd) {
                let present = state.installed.get(cmd).copied().unwrap_or(false);
                if present {
                    let _ = fedora::launch(tool);
                } else if matches!(fedora::install(&[tool.package]), Ok(s) if s.success()) {
                    state.installed.insert(cmd, true);
                    let _ = fedora::launch(tool);
                }
            }
        }
        Kind::Cmd(cmd, terminal) => {
            if terminal {
                let _ = Command::new("foot")
                    .arg("-e")
                    .arg("sh")
                    .arg("-c")
                    .arg(cmd)
                    .spawn();
            } else {
                let _ = Command::new("sh").arg("-c").arg(cmd).spawn();
            }
        }
    }
}

fn mde_path() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_else(|| "mde".to_string())
}

fn persist(state: &Settings) {
    let mut st = crate::state::load();
    st.theme_mode = if state.dark { "dark" } else { "light" }.to_string();
    st.win10_accent = state.win10_accent;
    st.win10_accent_on_taskbar = state.accent_on_taskbar;
    st.start_more_tiles = state.start_more_tiles;
    st.start_full_screen = state.start_full_screen;
    st.start_show_recent = state.start_show_recent;
    st.start_show_suggested = state.start_show_suggested;
    st.start_folders = state.start_folders.clone();
    st.taskbar_location = state.taskbar_loc.key().to_string();
    st.win10_search_mode = state.search_mode.key().to_string();
    st.win10_show_taskview = state.show_taskview;
    st.win10_autohide = state.autohide;
    st.win10_small_buttons = state.small_buttons;
    st.win10_manage_default_printer = state.manage_default;
    st.mouse_left_handed = state.mouse_left_handed;
    st.mouse_natural_scroll = state.mouse_natural_scroll;
    st.mouse_scroll_lines = state.mouse_scroll_lines;
    st.mouse_scroll_inactive = state.scroll_inactive;
    st.touchpad_enabled = state.touchpad_enabled;
    st.touchpad_speed = state.touchpad_speed;
    st.touchpad_tap = state.touchpad_tap;
    st.touchpad_two_finger = state.touchpad_two_finger;
    st.touchpad_natural_scroll = state.touchpad_natural_scroll;
    st.kb_repeat_rate = state.kb_repeat_rate;
    st.kb_repeat_delay = state.kb_repeat_delay;
    st.kb_layout = state.kb_layout.clone();
    st.typing_autocorrect = state.typing_autocorrect;
    st.typing_suggestions = state.typing_suggestions;
    st.autoplay_enabled = state.autoplay_enabled;
    st.autoplay_removable = state.autoplay_removable.key().to_string();
    st.autoplay_memcard = state.autoplay_memcard.key().to_string();
    st.storage_sense = state.storage_sense;
    st.backup_drive = state.backup_drive.clone();
    st.auto_backup = state.auto_backup;
    st.backup_schedule = state.backup_schedule.key().to_string();
    st.backup_retention = state.backup_retention.key().to_string();
    st.backup_includes = state.backup_includes.clone();
    st.update_paused_until = state.update_paused_until;
    st.update_active_start = state.active_start;
    st.update_active_end = state.active_end;
    st.update_restart_asap = state.restart_asap;
    st.update_restart_notify = state.restart_notify;
    st.hotspot_name = state.hotspot_name.clone();
    st.hotspot_password = state.hotspot_password.clone();
    st.data_limit_mb = state.data_limit.parse().unwrap_or(0);
    st.display_name = state.display_name.clone();
    st.account_picture = state.account_picture.clone();
    let _ = crate::state::save(&st);
}

/// Persist the pointer prefs, then push the rc.xml-backed controls into labwc's
/// `<libinput>` block and reconfigure (E12.6/E12.7). Writes the `touchpad` device
/// too when one is present, so a change on either page produces a complete block.
/// The advisory `scroll_inactive` rides along in `persist` but is never in rc.xml.
fn apply_libinput(state: &Settings) {
    persist(state);
    let tp = state.touchpad_present.then(|| crate::mouse::Touchpad {
        enabled: state.touchpad_enabled,
        pointer_speed: crate::mouse::pointer_speed(state.touchpad_speed),
        tap: state.touchpad_tap,
        two_finger: state.touchpad_two_finger,
        natural_scroll: state.touchpad_natural_scroll,
    });
    let _ = crate::mouse::apply(
        state.mouse_left_handed,
        state.mouse_natural_scroll,
        state.mouse_scroll_lines,
        tp.as_ref(),
    );
}

/// Persist the keyboard prefs, then push the repeat rate/delay into labwc's
/// `<keyboard>` block and reconfigure (E12.8). Layout is applied separately (it
/// goes to the environment file, not rc.xml).
fn apply_keyboard(state: &Settings) {
    persist(state);
    let _ = crate::keyboard::apply_repeat(state.kb_repeat_rate, state.kb_repeat_delay);
}

// --- view ------------------------------------------------------------------

fn bg(_t: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(palette::color(palette::WINDOW))),
        text_color: Some(palette::color(palette::WINDOW_TEXT)),
        ..container::Style::default()
    }
}

fn view(state: &Settings) -> Element<'_, Message> {
    let body = match state.view {
        View::Home => home(state),
        View::Category(c) => category(state, c),
    };
    container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(16.0)
        .style(bg)
        .into()
}

fn home(state: &Settings) -> Element<'_, Message> {
    let title = text("Settings")
        .size(metrics::INFO_TITLE_PX)
        .color(palette::color(palette::WINDOW_TEXT));
    let find = text_input("Find a setting", &state.search)
        .on_input(Message::Search)
        .padding(8.0)
        .size(metrics::UI_PX)
        .width(Length::Fixed(360.0));

    // A non-empty query filters the flat (category, page) list; otherwise the
    // category tile grid shows.
    let q = state.search.trim().to_lowercase();
    let content: Element<Message> = if q.is_empty() {
        let mut grid = Column::new().spacing(12.0);
        let mut r = Row::new().spacing(12.0);
        for (i, cat) in CATEGORIES.iter().enumerate() {
            r = r.push(home_tile(i, cat));
            if (i + 1) % COLS == 0 {
                grid = grid.push(r);
                r = Row::new().spacing(12.0);
            }
        }
        grid = grid.push(r);
        scrollable(grid).style(mde_ui::scrollbar).into()
    } else {
        search_results(&q)
    };

    Column::new()
        .spacing(16.0)
        .push(title)
        .push(find)
        .push(content)
        .into()
}

/// In-memory filter of every (category, page) by title; clicking a row jumps to
/// that page. No indexer — just a flat scan of the static model (E6.6).
fn search_results(q: &str) -> Element<'static, Message> {
    let mut col = Column::new().spacing(1.0);
    let mut any = false;
    for (ci, cat) in CATEGORIES.iter().enumerate() {
        for (pi, p) in cat.pages.iter().enumerate() {
            let hay = format!("{} {}", cat.title, p.title).to_lowercase();
            if !hay.contains(q) {
                continue;
            }
            any = true;
            // ASCII separator: the bundled UI font lacks ▸ (§2.7 never tofu);
            // `--list` keeps ▸ since it prints to a terminal font.
            let label = format!("{}  >  {}", cat.title, p.title);
            col = col.push(
                button(text(label).size(metrics::UI_PX))
                    .on_press(Message::Jump(ci, pi))
                    .width(Length::Fill)
                    .padding(Padding::from([6.0, 10.0]))
                    .style(|_t, status| {
                        let hot = matches!(status, button::Status::Hovered);
                        button::Style {
                            background: hot
                                .then(|| Background::Color(palette::color(palette::MENU))),
                            text_color: palette::color(palette::MENU_TEXT),
                            border: Border::default(),
                            ..button::Style::default()
                        }
                    }),
            );
        }
    }
    if !any {
        col = col.push(
            text("No matching settings")
                .size(metrics::UI_PX)
                .color(palette::color(palette::GRAY_TEXT)),
        );
    }
    scrollable(col).style(mde_ui::scrollbar).into()
}

fn home_tile(i: usize, cat: &'static Category) -> Element<'static, Message> {
    let inner = Row::new()
        .spacing(10.0)
        .align_y(iced::alignment::Vertical::Center)
        .push(crate::icons::icon_any(cat.icons, 32))
        .push(
            Column::new()
                .spacing(2.0)
                .push(
                    text(cat.title)
                        .size(metrics::UI_PX)
                        .color(palette::color(palette::WINDOW_TEXT)),
                )
                .push(
                    text(cat.caption)
                        .size(metrics::UI_PX)
                        .color(palette::color(palette::GRAY_TEXT)),
                ),
        );
    button(inner)
        .on_press(Message::OpenCategory(i))
        .width(Length::Fixed(210.0))
        .padding(12.0)
        .style(tile_style)
        .into()
}

fn tile_style(_t: &iced::Theme, status: button::Status) -> button::Style {
    let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: Some(Background::Color(palette::color(if hot {
            palette::MENU
        } else {
            palette::WINDOW
        }))),
        text_color: palette::color(palette::WINDOW_TEXT),
        border: Border {
            color: palette::color(palette::WINDOW_FRAME),
            width: 1.0,
            radius: 2.0.into(),
        },
        ..button::Style::default()
    }
}

fn category(state: &Settings, c: usize) -> Element<'_, Message> {
    let cat = &CATEGORIES[c];
    // Header: back arrow + category title.
    let back = button(text("\u{2190}").size(metrics::INFO_TITLE_PX))
        .on_press(Message::Back)
        .padding(Padding::from([2.0, 10.0]))
        .style(tile_style);
    let header = Row::new()
        .spacing(12.0)
        .align_y(iced::alignment::Vertical::Center)
        .push(back)
        .push(
            text(cat.title)
                .size(metrics::INFO_TITLE_PX)
                .color(palette::color(palette::WINDOW_TEXT)),
        );

    // Left rail: one entry per page (deferred greyed). The Touchpad page is
    // conditional — hidden from the rail when no touchpad is attached (E12.7);
    // its page index stays stable for the others since we only skip the push.
    let mut rail = Column::new().spacing(1.0).width(Length::Fixed(220.0));
    for (i, p) in cat.pages.iter().enumerate() {
        if matches!(p.kind, Kind::Touchpad) && !state.touchpad_present {
            continue;
        }
        rail = rail.push(rail_entry(i, p, i == state.page));
    }

    let pane = container(content_pane(state, cat))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(16.0);

    let cols = Row::new()
        .spacing(12.0)
        .push(scrollable(rail).style(mde_ui::scrollbar))
        .push(pane);

    Column::new().spacing(16.0).push(header).push(cols).into()
}

fn rail_entry(i: usize, p: &Page, selected: bool) -> Element<'static, Message> {
    let deferred = matches!(p.kind, Kind::Deferred);
    let fg = if deferred {
        palette::color(palette::GRAY_TEXT)
    } else {
        palette::color(palette::MENU_TEXT)
    };
    let label = text(p.title).size(metrics::UI_PX).color(fg);
    button(label)
        .on_press(Message::SelectPage(i))
        .width(Length::Fill)
        .padding(Padding::from([6.0, 10.0]))
        .style(move |_t, status| {
            let hot = selected || matches!(status, button::Status::Hovered);
            button::Style {
                background: hot.then(|| Background::Color(palette::color(palette::MENU))),
                text_color: fg,
                border: Border {
                    color: if selected {
                        palette::accent()
                    } else {
                        Color::TRANSPARENT
                    },
                    width: if selected { 2.0 } else { 0.0 },
                    radius: 0.0.into(),
                },
                ..button::Style::default()
            }
        })
        .into()
}

fn content_pane<'a>(state: &'a Settings, cat: &'static Category) -> Element<'a, Message> {
    let Some(page) = cat.pages.get(state.page) else {
        return Space::new(Length::Fill, Length::Fill).into();
    };
    let heading = text(page.title)
        .size(metrics::INFO_TITLE_PX)
        .color(palette::color(palette::WINDOW_TEXT));
    let inner: Element<Message> = match page.kind {
        Kind::Colors => colors_page(state),
        Kind::Background => background_page(state),
        Kind::Themes => themes_page(state),
        Kind::Start => start_page(state),
        Kind::Taskbar => taskbar_page(state),
        Kind::Update => update_page(state),
        Kind::UpdateHistory => update_history_page(state),
        Kind::UpdateAdvanced => update_advanced_page(state),
        Kind::NetworkStatus => network_status_page(state),
        Kind::Wifi => wifi_page(state),
        Kind::Ethernet => ethernet_page(state),
        Kind::Vpn => vpn_page(state),
        Kind::Hotspot => hotspot_page(state),
        Kind::Proxy => proxy_page(state),
        Kind::Airplane => airplane_page(state),
        Kind::DataUsage => data_usage_page(state),
        Kind::Cellular => cellular_page(),
        Kind::AccountInfo => account_info_page(state),
        Kind::FamilyUsers => family_users_page(state),
        Kind::SignIn => sign_in_page(state),
        Kind::Bluetooth => bluetooth_page(state),
        Kind::Printers => printers_page(state),
        Kind::Mouse => mouse_page(state),
        Kind::Touchpad => touchpad_page(state),
        Kind::Typing => typing_page(state),
        Kind::AutoPlay => autoplay_page(state),
        Kind::Storage => storage_page(state),
        Kind::Backup => backup_page(state),
        Kind::LockScreen => lock_page(state),
        Kind::Deferred => text("This page is part of a later milestone.")
            .size(metrics::UI_PX)
            .color(palette::color(palette::GRAY_TEXT))
            .into(),
        Kind::Mde(_) | Kind::Cmd(_, _) => open_button(page.title, true),
        Kind::Tool(cmd) => {
            let present = state.installed.get(cmd).copied().unwrap_or(true);
            open_button(page.title, present)
        }
    };
    Column::new().spacing(16.0).push(heading).push(inner).into()
}

/// A page that just launches an external backend: a single Open button (or
/// "Install & Open" when the backing tool is missing).
fn open_button<'a>(title: &str, present: bool) -> Element<'a, Message> {
    let label = if present {
        format!("Open {title}")
    } else {
        format!("Install & Open {title}")
    };
    Column::new()
        .spacing(10.0)
        .push(
            text(format!("Opens {title} in its own window."))
                .size(metrics::UI_PX)
                .color(palette::color(palette::WINDOW_TEXT)),
        )
        .push(
            button(text(label).size(metrics::UI_PX))
                .on_press(Message::Open)
                .padding(Padding::from([6.0, 16.0]))
                .style(tile_style),
        )
        .into()
}

/// Update & Security ▸ Update (E13.2): a status card (glyph + headline + sub-line)
/// over a "Check for updates" button that runs `dnf check-update` off-thread. The
/// page is era-neutral — settings.rs renders in the active theme, so it shows the
/// same in every era rather than gating to Win10 (best-choice §6, recorded). The
/// pending-update list + Install lands in E13.3.
fn update_page(state: &Settings) -> Element<'_, Message> {
    let pending: &[crate::packages::Update] = match &state.update_status {
        Some(Ok(v)) => v,
        _ => &[],
    };
    let busy = state.update_checking || state.update_installing;
    let (glyph, headline, sub) = if state.update_installing {
        (
            "\u{f021}", // fa-sync
            "Installing updates…".to_string(),
            "MackesDE Update is applying the updates.".to_string(),
        )
    } else if state.update_checking {
        (
            "\u{f021}",
            "Checking for updates…".to_string(),
            String::new(),
        )
    } else {
        match &state.update_status {
            None => (
                "\u{f021}",
                "Check for updates".to_string(),
                "Updates are installed by MackesDE Update.".to_string(),
            ),
            Some(Ok(v)) if v.is_empty() => (
                "\u{f00c}", // fa-check
                "You're up to date".to_string(),
                "Last checked: just now".to_string(),
            ),
            Some(Ok(v)) => (
                "\u{f0f3}", // fa-bell
                format!(
                    "{} update{} available",
                    v.len(),
                    if v.len() == 1 { "" } else { "s" }
                ),
                "Last checked: just now".to_string(),
            ),
            Some(Err(e)) => (
                "\u{f071}", // fa-warning
                "Couldn't check for updates".to_string(),
                e.clone(),
            ),
        }
    };
    let mut lines = Column::new().spacing(2.0).push(
        text(headline)
            .size(metrics::INFO_TITLE_PX)
            .color(palette::color(palette::WINDOW_TEXT)),
    );
    if !sub.is_empty() {
        lines = lines.push(
            text(sub)
                .size(metrics::UI_PX)
                .color(palette::color(palette::GRAY_TEXT)),
        );
    }
    let card = Row::new()
        .spacing(12.0)
        .align_y(iced::alignment::Vertical::Center)
        .push(
            text(glyph)
                .size(metrics::TILE_GLYPH_PX)
                .font(mde_ui::font::NERD)
                .color(palette::color(palette::WINDOW_TEXT)),
        )
        .push(lines);
    let mut buttons = Row::new().spacing(8.0).push(
        button(text("Check for updates").size(metrics::UI_PX))
            .on_press_maybe((!busy).then_some(Message::CheckUpdates))
            .padding(Padding::from([6.0, 16.0]))
            .style(tile_style),
    );
    if !pending.is_empty() {
        buttons = buttons.push(
            button(text("Install updates").size(metrics::UI_PX))
                .on_press_maybe((!busy).then_some(Message::InstallUpdates))
                .padding(Padding::from([6.0, 16.0]))
                .style(tile_style),
        );
    }
    let mut col = Column::new().spacing(16.0).push(card).push(buttons);
    // Pause / Resume updates (E13.4): a flat accent link. While paused the
    // dnf-automatic timer is masked; show the days remaining + a Resume flip.
    let now = now_secs();
    let pause: Element<Message> = if state.update_paused_until > now {
        let days = (state.update_paused_until - now).div_ceil(86_400);
        Row::new()
            .spacing(8.0)
            .align_y(iced::alignment::Vertical::Center)
            .push(
                text(format!(
                    "Updates paused — {days} day{} remaining",
                    if days == 1 { "" } else { "s" }
                ))
                .size(metrics::UI_PX)
                .color(palette::color(palette::GRAY_TEXT)),
            )
            .push(
                mouse_area(
                    text("Resume updates")
                        .size(metrics::UI_PX)
                        .color(palette::accent()),
                )
                .on_press(Message::ResumeUpdates),
            )
            .into()
    } else {
        mouse_area(
            text("Pause updates for 7 days")
                .size(metrics::UI_PX)
                .color(palette::accent()),
        )
        .on_press(Message::PauseUpdates)
        .into()
    };
    col = col.push(pause);
    // Active hours (E13.5): the window updates avoid. Start/End hour pick-lists +
    // Save, which writes the dnf-automatic timer OnCalendar override.
    let hours: Vec<Hour> = (0..24u8).map(Hour).collect();
    let active = Row::new()
        .spacing(8.0)
        .align_y(iced::alignment::Vertical::Center)
        .push(
            text("Active hours")
                .size(metrics::UI_PX)
                .color(palette::color(palette::WINDOW_TEXT)),
        )
        .push(
            pick_list(hours.clone(), Some(Hour(state.active_start)), |h| {
                Message::SetActiveStart(h.0)
            })
            .text_size(metrics::UI_PX),
        )
        .push(
            text("to")
                .size(metrics::UI_PX)
                .color(palette::color(palette::GRAY_TEXT)),
        )
        .push(
            pick_list(hours, Some(Hour(state.active_end)), |h| {
                Message::SetActiveEnd(h.0)
            })
            .text_size(metrics::UI_PX),
        )
        .push(
            button(text("Save").size(metrics::UI_PX))
                .on_press(Message::SaveActiveHours)
                .padding(Padding::from([4.0, 12.0]))
                .style(tile_style),
        );
    col = col.push(active);
    // The pending-update list (E13.3): package + candidate version, scrollable.
    if !pending.is_empty() {
        let mut list = Column::new().spacing(0.0);
        for u in pending {
            list = list.push(
                Row::new()
                    .spacing(8.0)
                    .push(
                        text(u.package.clone())
                            .size(metrics::UI_PX)
                            .width(Length::FillPortion(3))
                            .color(palette::color(palette::WINDOW_TEXT)),
                    )
                    .push(
                        text(u.version.clone())
                            .size(metrics::UI_PX)
                            .width(Length::FillPortion(2))
                            .color(palette::color(palette::GRAY_TEXT)),
                    )
                    .padding(Padding::from([2.0, 4.0])),
            );
        }
        col = col.push(container(scrollable(list).style(mde_ui::scrollbar)).height(Length::Fill));
    }
    col.into()
}

/// Update & Security ▸ Update history (E13.6): the `dnf history list` transactions,
/// each selectable (accent-tinted), with **Uninstall** (`dnf history undo`) for the
/// selection + a **Recovery options** link to Timeshift. Auto-loaded on show
/// (`maybe_load`). Feature/Quality grouping is deferred (E13.3a) — listed flat.
fn update_history_page(state: &Settings) -> Element<'_, Message> {
    let note = |s: String| {
        text(s)
            .size(metrics::UI_PX)
            .color(palette::color(palette::GRAY_TEXT))
    };
    let body: Element<Message> = if state.history_loading {
        note("Loading update history…".to_string()).into()
    } else {
        match &state.history {
            None => note("Loading update history…".to_string()).into(),
            Some(Err(e)) => note(format!("Couldn't read history: {e}")).into(),
            Some(Ok(v)) if v.is_empty() => note("No update history.".to_string()).into(),
            Some(Ok(v)) => {
                let mut list = Column::new().spacing(0.0);
                for h in v {
                    let sel = state.history_selected == Some(h.id);
                    let c = if sel {
                        palette::accent()
                    } else {
                        palette::color(palette::WINDOW_TEXT)
                    };
                    let cell = |s: String, p: u16, gray: bool| {
                        text(s)
                            .size(metrics::UI_PX)
                            .width(Length::FillPortion(p))
                            .color(if gray && !sel {
                                palette::color(palette::GRAY_TEXT)
                            } else {
                                c
                            })
                    };
                    list = list.push(
                        mouse_area(
                            Row::new()
                                .spacing(8.0)
                                .push(cell(h.id.to_string(), 1, false))
                                .push(cell(h.command.clone(), 4, false))
                                .push(cell(h.date.clone(), 3, true))
                                .push(cell(h.action.clone(), 2, true))
                                .push(cell(h.altered.clone(), 1, true))
                                .padding(Padding::from([2.0, 4.0])),
                        )
                        .on_press(Message::SelectHistory(h.id)),
                    );
                }
                container(scrollable(list).style(mde_ui::scrollbar))
                    .height(Length::Fill)
                    .into()
            }
        }
    };
    let buttons = Row::new()
        .spacing(8.0)
        .push(
            button(text("Uninstall selected").size(metrics::UI_PX))
                .on_press_maybe(state.history_selected.map(|_| Message::UninstallHistory))
                .padding(Padding::from([6.0, 16.0]))
                .style(tile_style),
        )
        .push(
            button(text("Recovery options").size(metrics::UI_PX))
                .on_press(Message::OpenRecovery)
                .padding(Padding::from([6.0, 16.0]))
                .style(tile_style),
        );
    Column::new()
        .spacing(12.0)
        .push(buttons)
        .push(container(body).height(Length::Fill))
        .into()
}

/// Update & Security ▸ Advanced options (E13.7): restart-ASAP (writes the
/// dnf-automatic reboot policy) + notify-on-restart-required (drives a toast on the
/// next check that finds a kernel). The two Win10 toggles with no Linux backend —
/// metered connections, other-products — are shown greyed/advisory per §3.
fn update_advanced_page(state: &Settings) -> Element<'_, Message> {
    let live = |label: &'static str, checked: bool, msg: fn(bool) -> Message| {
        checkbox(label, checked)
            .on_toggle(msg)
            .size(metrics::UI_PX)
            .text_size(metrics::UI_PX)
            .spacing(8.0)
            .style(mde_ui::checkbox_style)
    };
    let greyed = |label: &'static str| {
        checkbox(label, false)
            .size(metrics::UI_PX)
            .text_size(metrics::UI_PX)
            .spacing(8.0)
            .style(mde_ui::checkbox_style)
    };
    // Automatic-updates posture (E13.8) — reads/writes the same dnf-automatic
    // timer + config the System Properties radios use, so the two surfaces agree.
    let posture = Row::new()
        .spacing(8.0)
        .align_y(iced::alignment::Vertical::Center)
        .push(
            text("Automatic updates")
                .size(metrics::UI_PX)
                .color(palette::color(palette::WINDOW_TEXT)),
        )
        .push(
            pick_list(
                crate::sysinfo::AutoMode::ALL.to_vec(),
                Some(state.auto_mode),
                Message::SetAutoMode,
            )
            .text_size(metrics::UI_PX),
        );
    Column::new()
        .spacing(10.0)
        .push(posture)
        .push(greyed("Download updates over metered connections"))
        .push(live(
            "Restart this device as soon as possible when a restart is required",
            state.restart_asap,
            Message::SetRestartAsap,
        ))
        .push(live(
            "Show a notification when a restart is required to finish updating",
            state.restart_notify,
            Message::SetRestartNotify,
        ))
        .push(greyed("Receive updates for other MackesDE products"))
        .push(
            text(
                "Metered-connection and other-products options have no Linux backend — \
                 shown for fidelity, not enforced.",
            )
            .size(metrics::UI_PX)
            .color(palette::color(palette::GRAY_TEXT)),
        )
        .into()
}

/// `~/.face`, the standard per-user avatar path.
fn home_face() -> String {
    std::env::var_os("HOME")
        .map(|h| {
            let mut p = std::path::PathBuf::from(h);
            p.push(".face");
            p.display().to_string()
        })
        .unwrap_or_default()
}

/// Accounts ▸ Your info (E10.3): the account avatar (from `account_picture` / `~/.face`,
/// with a Browse → copy-to-`~/.face`) + an editable display name (empty falls back to
/// the system user). The avatar is square-bordered — iced has no circular image clip,
/// so the "round" picture is approximated (recorded).
fn account_info_page(state: &Settings) -> Element<'_, Message> {
    let path = if state.account_picture.is_empty() {
        home_face()
    } else {
        state.account_picture.clone()
    };
    let avatar: Element<Message> = if std::path::Path::new(&path).is_file() {
        image(path)
            .width(Length::Fixed(96.0))
            .height(Length::Fixed(96.0))
            .into()
    } else {
        container(
            text("\u{f007}") // nf-fa-user
                .size(metrics::IDENTIFY_PX)
                .font(mde_ui::font::NERD)
                .color(palette::color(palette::GRAY_TEXT)),
        )
        .width(Length::Fixed(96.0))
        .height(Length::Fixed(96.0))
        .center_x(Length::Fixed(96.0))
        .center_y(Length::Fixed(96.0))
        .style(|_| container::Style {
            border: Border {
                color: palette::color(palette::WINDOW_FRAME),
                width: 1.0,
                radius: 4.0.into(),
            },
            ..container::Style::default()
        })
        .into()
    };
    let user = std::env::var("USER").unwrap_or_else(|_| "User".into());
    Column::new()
        .spacing(14.0)
        .push(avatar)
        .push(
            button(text("Browse for one…").size(metrics::UI_PX))
                .on_press(Message::BrowseAvatar)
                .padding(Padding::from([4.0, 12.0]))
                .style(tile_style),
        )
        .push(
            Row::new()
                .spacing(8.0)
                .align_y(iced::alignment::Vertical::Center)
                .push(
                    text("Display name")
                        .size(metrics::UI_PX)
                        .width(Length::Fixed(110.0))
                        .color(palette::color(palette::WINDOW_TEXT)),
                )
                .push(
                    text_input(&user, &state.display_name)
                        .on_input(Message::DisplayName)
                        .size(metrics::UI_PX)
                        .width(Length::Fixed(220.0)),
                ),
        )
        .into()
}

/// Accounts ▸ Family & other users (E10.4 list + E10.5 management): the real
/// local-account list (`/etc/passwd`, UID ≥ 1000) with an Administrator/Standard
/// badge from `wheel`. Other accounts gain **Make admin/standard** + **Remove**
/// buttons driving `pkexec usermod`/`gpasswd`/`userdel`; the signed-in account and
/// the last administrator are guarded (no self-delete, no last-admin demote). An
/// **Add a user** field runs `pkexec useradd`. Remove asks an inline confirm first.
fn family_users_page(state: &Settings) -> Element<'_, Message> {
    let me = std::env::var("USER").unwrap_or_default();
    let admins = crate::sysinfo::admin_count(&state.accounts);
    let mut list = Column::new().spacing(4.0);
    for a in &state.accounts {
        let (badge, badge_col) = if a.admin {
            ("Administrator", palette::accent())
        } else {
            ("Standard user", palette::color(palette::GRAY_TEXT))
        };
        let is_me = a.name == me;
        let sub = if is_me {
            format!("{} — signed in", a.name)
        } else {
            a.name.clone()
        };
        let mut row = Row::new()
            .spacing(10.0)
            .align_y(iced::alignment::Vertical::Center)
            .push(
                text("\u{f007}") // nf-fa-user
                    .size(metrics::TILE_GLYPH_PX)
                    .font(mde_ui::font::NERD)
                    .color(palette::color(palette::WINDOW_TEXT)),
            )
            .push(
                Column::new()
                    .width(Length::Fill)
                    .push(
                        text(a.full.clone())
                            .size(metrics::UI_PX)
                            .color(palette::color(palette::WINDOW_TEXT)),
                    )
                    .push(
                        text(sub)
                            .size(metrics::BADGE_PX)
                            .color(palette::color(palette::GRAY_TEXT)),
                    ),
            )
            .push(
                container(text(badge).size(metrics::BADGE_PX).color(badge_col))
                    .padding(Padding::from([2.0, 8.0]))
                    .style(|_| container::Style {
                        border: Border {
                            color: palette::color(palette::WINDOW_FRAME),
                            width: 1.0,
                            radius: 2.0.into(),
                        },
                        ..container::Style::default()
                    }),
            );
        // Management actions on *other* accounts only (never the signed-in user).
        if !is_me {
            // Demoting the last admin is blocked — render the button inert.
            let demote_locked = a.admin && admins <= 1;
            let toggle_label = if a.admin {
                "Make standard"
            } else {
                "Make admin"
            };
            let mut toggle = button(text(toggle_label).size(metrics::UI_PX))
                .padding(Padding::from([3.0, 10.0]))
                .style(tile_style);
            if !demote_locked {
                toggle = toggle.on_press(Message::ToggleAdmin(a.name.clone()));
            }
            row = row.push(toggle).push(
                button(text("Remove").size(metrics::UI_PX))
                    .on_press(Message::AskRemove(a.name.clone()))
                    .padding(Padding::from([3.0, 10.0]))
                    .style(tile_style),
            );
        }
        list = list.push(container(row).padding(Padding::from([6.0, 4.0])));
    }
    let body: Element<Message> = if state.accounts.is_empty() {
        text("No accounts found on this PC.")
            .size(metrics::UI_PX)
            .color(palette::color(palette::GRAY_TEXT))
            .into()
    } else {
        scrollable(list).style(mde_ui::scrollbar).into()
    };

    let mut col = Column::new().spacing(10.0).push(
        text("People who use this PC")
            .size(metrics::UI_PX)
            .color(palette::color(palette::WINDOW_TEXT)),
    );

    // Inline remove-confirm banner (in place of a separate dialogs.rs window — keeps
    // the flow inside the Settings surface and headless-capturable; §6 best-choice).
    if let Some(name) = &state.confirm_remove {
        col = col.push(
            container(
                Column::new()
                    .spacing(8.0)
                    .push(
                        text(format!("Delete {name} and all of their data?"))
                            .size(metrics::UI_PX)
                            .color(palette::color(palette::WINDOW_TEXT)),
                    )
                    .push(
                        Row::new()
                            .spacing(8.0)
                            .push(
                                button(text("Delete account").size(metrics::UI_PX))
                                    .on_press(Message::ConfirmRemove(name.clone()))
                                    .padding(Padding::from([4.0, 12.0]))
                                    .style(tile_style),
                            )
                            .push(
                                button(text("Cancel").size(metrics::UI_PX))
                                    .on_press(Message::CancelRemove)
                                    .padding(Padding::from([4.0, 12.0]))
                                    .style(mde_ui::button_ghost),
                            ),
                    ),
            )
            .padding(10.0)
            .style(|_| container::Style {
                border: Border {
                    color: palette::accent(),
                    width: 1.0,
                    radius: 2.0.into(),
                },
                ..container::Style::default()
            }),
        );
    }

    col.push(container(body).height(Length::Fill))
        .push(
            Row::new()
                .spacing(8.0)
                .align_y(iced::alignment::Vertical::Center)
                .push(
                    text_input("Add a user…", &state.new_user)
                        .on_input(Message::NewUser)
                        .on_submit(Message::AddUser)
                        .size(metrics::UI_PX)
                        .width(Length::Fixed(220.0)),
                )
                .push(
                    button(text("Add account").size(metrics::UI_PX))
                        .on_press(Message::AddUser)
                        .padding(Padding::from([4.0, 12.0]))
                        .style(mde_ui::button_primary),
                ),
        )
        .into()
}

/// Accounts ▸ Sign-in options (E10.6): Windows-Hello Face/Fingerprint shown as
/// advisory unavailable rows (§3 — no fake toggles), a working **PIN** enrol/change
/// (argon2-hashed `pin.hash` via `pin::set_pin`), and **Password** → `pkexec passwd`
/// in a Win10-blue `foot` terminal.
fn sign_in_page(state: &Settings) -> Element<'_, Message> {
    // A dimmed "unavailable" Windows Hello row.
    let hello = |title: &str| -> Element<Message> {
        Column::new()
            .spacing(1.0)
            .push(
                text(title.to_string())
                    .size(metrics::UI_PX)
                    .color(palette::color(palette::GRAY_TEXT)),
            )
            .push(
                text("This option is currently unavailable.")
                    .size(metrics::BADGE_PX)
                    .color(palette::color(palette::GRAY_TEXT)),
            )
            .into()
    };

    // PIN section: status + two secure fields + Save/Change (+ Remove when set).
    let pin_status = if state.pin_set {
        "A PIN is set."
    } else {
        "No PIN is set."
    };
    let mut pin_buttons = Row::new().spacing(8.0).push(
        button(
            text(if state.pin_set {
                "Change PIN"
            } else {
                "Set PIN"
            })
            .size(metrics::UI_PX),
        )
        .on_press(Message::SavePin)
        .padding(Padding::from([4.0, 12.0]))
        .style(mde_ui::button_primary),
    );
    if state.pin_set {
        pin_buttons = pin_buttons.push(
            button(text("Remove").size(metrics::UI_PX))
                .on_press(Message::RemovePin)
                .padding(Padding::from([4.0, 12.0]))
                .style(mde_ui::button_ghost),
        );
    }
    let mut pin_section = Column::new()
        .spacing(8.0)
        .push(
            text("PIN")
                .size(metrics::UI_PX)
                .color(palette::color(palette::WINDOW_TEXT)),
        )
        .push(
            text(pin_status)
                .size(metrics::BADGE_PX)
                .color(palette::color(palette::GRAY_TEXT)),
        )
        .push(
            text_input("Enter PIN", &state.pin1)
                .on_input(Message::Pin1)
                .secure(true)
                .size(metrics::UI_PX)
                .width(Length::Fixed(180.0)),
        )
        .push(
            text_input("Confirm PIN", &state.pin2)
                .on_input(Message::Pin2)
                .on_submit(Message::SavePin)
                .secure(true)
                .size(metrics::UI_PX)
                .width(Length::Fixed(180.0)),
        )
        .push(pin_buttons);
    if !state.pin_msg.is_empty() {
        pin_section = pin_section.push(
            text(state.pin_msg.clone())
                .size(metrics::BADGE_PX)
                .color(palette::accent()),
        );
    }

    let password_section = Column::new()
        .spacing(8.0)
        .push(
            text("Password")
                .size(metrics::UI_PX)
                .color(palette::color(palette::WINDOW_TEXT)),
        )
        .push(
            text("Change your account password.")
                .size(metrics::BADGE_PX)
                .color(palette::color(palette::GRAY_TEXT)),
        )
        .push(
            button(text("Change").size(metrics::UI_PX))
                .on_press(Message::ChangePassword)
                .padding(Padding::from([4.0, 12.0]))
                .style(tile_style),
        );

    Column::new()
        .spacing(18.0)
        .push(hello("Windows Hello Face"))
        .push(hello("Windows Hello Fingerprint"))
        .push(pin_section)
        .push(password_section)
        .into()
}

/// Devices ▸ Bluetooth (E12.2): the BlueZ adapter Powered toggle, an "Add a device"
/// discovery action, and the device list (paired → Connect/Disconnect + Remove;
/// discovered → Pair). All actions run off-thread and re-read the adapter.
fn bluetooth_page(state: &Settings) -> Element<'_, Message> {
    let Some(bt) = &state.bt else {
        return text("Checking Bluetooth…")
            .size(metrics::UI_PX)
            .color(palette::color(palette::GRAY_TEXT))
            .into();
    };
    if !bt.present {
        return text("No Bluetooth adapter was found on this PC.")
            .size(metrics::UI_PX)
            .color(palette::color(palette::GRAY_TEXT))
            .into();
    }

    let mut col = Column::new().spacing(12.0).push(
        checkbox("Bluetooth", bt.powered)
            .on_toggle(Message::BtPowered)
            .size(metrics::UI_PX)
            .text_size(metrics::UI_PX)
            .spacing(8.0)
            .style(mde_ui::checkbox_style),
    );

    if !bt.powered {
        return col
            .push(
                text("Bluetooth is off. Turn it on to connect devices.")
                    .size(metrics::BADGE_PX)
                    .color(palette::color(palette::GRAY_TEXT)),
            )
            .into();
    }

    col = col.push(
        button(
            text(if bt.discovering {
                "Searching for devices…"
            } else {
                "+ Add a device"
            })
            .size(metrics::UI_PX),
        )
        .on_press(Message::BtDiscover)
        .padding(Padding::from([4.0, 12.0]))
        .style(mde_ui::button_primary),
    );

    if bt.devices.is_empty() {
        return col
            .push(
                text("No devices yet — select \"Add a device\" to find nearby ones.")
                    .size(metrics::UI_PX)
                    .color(palette::color(palette::GRAY_TEXT)),
            )
            .into();
    }

    let mut list = Column::new().spacing(4.0);
    for d in &bt.devices {
        let status = if d.connected {
            "Connected"
        } else if d.paired {
            "Paired"
        } else {
            "Available"
        };
        let mut row = Row::new()
            .spacing(10.0)
            .align_y(iced::alignment::Vertical::Center)
            .push(
                text("\u{f293}") // nf-fa-bluetooth
                    .font(mde_ui::font::NERD)
                    .size(metrics::TILE_GLYPH_PX)
                    .color(palette::color(if d.connected {
                        palette::WINDOW_TEXT
                    } else {
                        palette::GRAY_TEXT
                    })),
            )
            .push(
                Column::new()
                    .width(Length::Fill)
                    .push(
                        text(d.name.clone())
                            .size(metrics::UI_PX)
                            .color(palette::color(palette::WINDOW_TEXT)),
                    )
                    .push(
                        text(status)
                            .size(metrics::BADGE_PX)
                            .color(palette::color(palette::GRAY_TEXT)),
                    ),
            );
        if d.paired {
            row = row
                .push(
                    button(
                        text(if d.connected { "Disconnect" } else { "Connect" })
                            .size(metrics::UI_PX),
                    )
                    .on_press(Message::BtConnectToggle(d.path.clone(), d.connected))
                    .padding(Padding::from([3.0, 10.0]))
                    .style(mde_ui::button_ghost),
                )
                .push(
                    button(text("Remove").size(metrics::UI_PX))
                        .on_press(Message::BtRemove(d.path.clone()))
                        .padding(Padding::from([3.0, 10.0]))
                        .style(mde_ui::button_ghost),
                );
        } else {
            row = row.push(
                button(text("Pair").size(metrics::UI_PX))
                    .on_press(Message::BtPair(d.path.clone()))
                    .padding(Padding::from([3.0, 10.0]))
                    .style(mde_ui::button_primary),
            );
        }
        list = list.push(container(row).padding(Padding::from([4.0, 4.0])));
    }
    col.push(container(scrollable(list).style(mde_ui::scrollbar)).height(Length::Fill))
        .into()
}

/// Devices ▸ Printers & scanners (E12.4): the CUPS queue list (per-printer Set as
/// default / Print test page / Remove), a "+ Add a printer or scanner" discovery
/// scan that installs a driverless queue, and the "Let Windows manage my default
/// printer" toggle that hides Set-as-default when on (Win10 behaviour). Set-default
/// and the test page run as the user; add/remove go through pkexec.
fn printers_page(state: &Settings) -> Element<'_, Message> {
    let Some(cups) = &state.printers else {
        return text("Checking printers…")
            .size(metrics::UI_PX)
            .color(palette::color(palette::GRAY_TEXT))
            .into();
    };
    if !cups.present {
        return text("CUPS printing is not installed on this PC.")
            .size(metrics::UI_PX)
            .color(palette::color(palette::GRAY_TEXT))
            .into();
    }

    let mut col = Column::new().spacing(12.0).push(
        button(
            text(if cups.discovered.is_empty() && !state.printers_scanned {
                "+ Add a printer or scanner"
            } else {
                "+ Search again"
            })
            .size(metrics::UI_PX),
        )
        .on_press(Message::PrintersDiscover)
        .padding(Padding::from([4.0, 12.0]))
        .style(mde_ui::button_primary),
    );

    // Discovered devices (after a scan) — each addable as a driverless queue.
    if !cups.discovered.is_empty() {
        let mut found = Column::new().spacing(4.0).push(
            text("Found these printers")
                .size(metrics::BADGE_PX)
                .color(palette::color(palette::GRAY_TEXT)),
        );
        for d in &cups.discovered {
            let name = crate::cups::sanitize_queue_name(&d.info);
            let uri = d.uri.clone();
            let row = Row::new()
                .spacing(10.0)
                .align_y(iced::alignment::Vertical::Center)
                .push(
                    Column::new()
                        .width(Length::Fill)
                        .push(
                            text(d.info.clone())
                                .size(metrics::UI_PX)
                                .color(palette::color(palette::WINDOW_TEXT)),
                        )
                        .push(
                            text(d.uri.clone())
                                .size(metrics::BADGE_PX)
                                .color(palette::color(palette::GRAY_TEXT)),
                        ),
                )
                .push(
                    button(text("Add device").size(metrics::UI_PX))
                        .on_press(Message::PrintersAdd(name, uri))
                        .padding(Padding::from([3.0, 10.0]))
                        .style(mde_ui::button_primary),
                );
            found = found.push(container(row).padding(Padding::from([4.0, 4.0])));
        }
        col = col.push(found);
    } else if state.printers_scanned {
        col = col.push(
            text("No new printers were found. Make sure the printer is on and connected.")
                .size(metrics::BADGE_PX)
                .color(palette::color(palette::GRAY_TEXT)),
        );
    }

    // The installed queues, led by the permanent "Print to PDF" row (E12.5) — it
    // is always shown (like Win10's Microsoft Print to PDF), whether or not the
    // cups-pdf queue is actually set up yet.
    let mut list = Column::new()
        .spacing(4.0)
        .push(print_to_pdf_row(state, cups));
    let real: Vec<&crate::cups::Printer> = cups.printers.iter().filter(|p| !p.is_pdf).collect();
    if real.is_empty() {
        list = list.push(
            text("No other printers are installed yet.")
                .size(metrics::UI_PX)
                .color(palette::color(palette::GRAY_TEXT)),
        );
    }
    for p in real {
        let mut info_col = Column::new().width(Length::Fill).push(
            text(p.name.clone())
                .size(metrics::UI_PX)
                .color(palette::color(palette::WINDOW_TEXT)),
        );
        let status = if p.is_default {
            format!("Default · {}", p.state.label())
        } else {
            p.state.label().to_string()
        };
        info_col = info_col.push(
            text(status)
                .size(metrics::BADGE_PX)
                .color(palette::color(palette::GRAY_TEXT)),
        );
        let mut row = Row::new()
            .spacing(10.0)
            .align_y(iced::alignment::Vertical::Center)
            .push(
                text("\u{f02f}") // nf-fa-print
                    .font(mde_ui::font::NERD)
                    .size(metrics::TILE_GLYPH_PX)
                    .color(palette::color(if p.is_default {
                        palette::WINDOW_TEXT
                    } else {
                        palette::GRAY_TEXT
                    })),
            )
            .push(info_col);
        // "Set as default" is hidden when Windows manages the default, or when this
        // queue already is the default.
        if !state.manage_default && !p.is_default {
            let n = p.name.clone();
            row = row.push(
                button(text("Set as default").size(metrics::UI_PX))
                    .on_press(Message::PrintersSetDefault(n))
                    .padding(Padding::from([3.0, 10.0]))
                    .style(mde_ui::button_ghost),
            );
        }
        let test_name = p.name.clone();
        let rm_name = p.name.clone();
        row = row
            .push(
                button(text("Print test page").size(metrics::UI_PX))
                    .on_press(Message::PrintersTest(test_name))
                    .padding(Padding::from([3.0, 10.0]))
                    .style(mde_ui::button_ghost),
            )
            .push(
                button(text("Remove").size(metrics::UI_PX))
                    .on_press(Message::PrintersRemove(rm_name))
                    .padding(Padding::from([3.0, 10.0]))
                    .style(mde_ui::button_ghost),
            );
        list = list.push(container(row).padding(Padding::from([4.0, 4.0])));
    }

    col = col.push(container(scrollable(list).style(mde_ui::scrollbar)).height(Length::Fill));

    // The Win10 "let Windows manage my default printer" toggle, below the list.
    col.push(
        checkbox(
            "Let Windows manage my default printer",
            state.manage_default,
        )
        .on_toggle(Message::SetManageDefaultPrinter)
        .size(metrics::UI_PX)
        .text_size(metrics::UI_PX)
        .spacing(8.0)
        .style(mde_ui::checkbox_style),
    )
    .into()
}

/// The permanent "Print to PDF" row (E12.5) — the Win10 Microsoft-Print-to-PDF
/// equivalent backed by cups-pdf. Always present: when the queue is installed it
/// offers a test page (+ Set as default, subject to the manage-default toggle) and
/// has no Remove (it is permanent); when absent, a single "Set up" action installs
/// cups-pdf and registers the queue.
fn print_to_pdf_row<'a>(
    state: &Settings,
    cups: &'a crate::cups::CupsState,
) -> Element<'a, Message> {
    let pdf = cups.printers.iter().find(|p| p.is_pdf);
    let status = match pdf {
        Some(p) if p.is_default => format!("Default · {}", p.state.label()),
        Some(p) => p.state.label().to_string(),
        None => "Not set up".to_string(),
    };
    let info_col = Column::new()
        .width(Length::Fill)
        .push(
            text("Print to PDF")
                .size(metrics::UI_PX)
                .color(palette::color(palette::WINDOW_TEXT)),
        )
        .push(
            text(status)
                .size(metrics::BADGE_PX)
                .color(palette::color(palette::GRAY_TEXT)),
        );
    let mut row = Row::new()
        .spacing(10.0)
        .align_y(iced::alignment::Vertical::Center)
        .push(
            text("\u{f1c1}") // nf-fa-file_pdf_o
                .font(mde_ui::font::NERD)
                .size(metrics::TILE_GLYPH_PX)
                .color(palette::color(palette::WINDOW_TEXT)),
        )
        .push(info_col);
    match pdf {
        Some(p) => {
            if !state.manage_default && !p.is_default {
                let n = p.name.clone();
                row = row.push(
                    button(text("Set as default").size(metrics::UI_PX))
                        .on_press(Message::PrintersSetDefault(n))
                        .padding(Padding::from([3.0, 10.0]))
                        .style(mde_ui::button_ghost),
                );
            }
            let test = p.name.clone();
            row = row.push(
                button(text("Print test page").size(metrics::UI_PX))
                    .on_press(Message::PrintersTest(test))
                    .padding(Padding::from([3.0, 10.0]))
                    .style(mde_ui::button_ghost),
            );
        }
        None => {
            row = row.push(
                button(text("Set up").size(metrics::UI_PX))
                    .on_press(Message::PrintersSetupPdf)
                    .padding(Padding::from([3.0, 10.0]))
                    .style(mde_ui::button_primary),
            );
        }
    }
    container(row).padding(Padding::from([4.0, 4.0])).into()
}

/// Devices ▸ Mouse (E12.6): primary button, scroll lines, natural-scroll, and the
/// advisory "scroll inactive windows" toggle. The first three rewrite labwc's
/// `<libinput>` block live (apply on change); the advisory persists to menu.json
/// only. Mirrors the Win10 Mouse page controls.
fn mouse_page(state: &Settings) -> Element<'_, Message> {
    let primary = if state.mouse_left_handed {
        PrimaryButton::Right
    } else {
        PrimaryButton::Left
    };
    let label = |t: &'static str| {
        text(t)
            .size(metrics::UI_PX)
            .width(Length::Fixed(230.0))
            .color(palette::color(palette::WINDOW_TEXT))
    };

    let primary_row = Row::new()
        .spacing(8.0)
        .align_y(iced::alignment::Vertical::Center)
        .push(label("Select your primary button"))
        .push(
            pick_list(
                PrimaryButton::ALL.to_vec(),
                Some(primary),
                Message::SetPrimaryButton,
            )
            .text_size(metrics::UI_PX),
        );

    let line_items: Vec<Lines> = (1u8..=10).map(Lines).collect();
    let scroll_row = Row::new()
        .spacing(8.0)
        .align_y(iced::alignment::Vertical::Center)
        .push(label("Lines to scroll each time"))
        .push(
            pick_list(
                line_items,
                Some(Lines(state.mouse_scroll_lines)),
                Message::SetScrollLines,
            )
            .text_size(metrics::UI_PX),
        );

    let natural = checkbox("Reverse scrolling direction", state.mouse_natural_scroll)
        .on_toggle(Message::SetMouseNatural)
        .size(metrics::UI_PX)
        .text_size(metrics::UI_PX)
        .spacing(8.0)
        .style(mde_ui::checkbox_style);

    let inactive = Column::new()
        .spacing(2.0)
        .push(
            checkbox(
                "Scroll inactive windows when I hover over them",
                state.scroll_inactive,
            )
            .on_toggle(Message::SetScrollInactive)
            .size(metrics::UI_PX)
            .text_size(metrics::UI_PX)
            .spacing(8.0)
            .style(mde_ui::checkbox_style),
        )
        .push(
            text(
                "Saved preference — applied per-application; labwc has no global setting for this.",
            )
            .size(metrics::BADGE_PX)
            .color(palette::color(palette::GRAY_TEXT)),
        );

    Column::new()
        .spacing(14.0)
        .push(primary_row)
        .push(scroll_row)
        .push(natural)
        .push(inactive)
        .into()
}

/// Devices ▸ Touchpad (E12.7): On/Off, cursor speed, tap, two-finger scroll, and
/// reverse-scroll — each rewriting labwc's `touchpad` libinput device live. Only
/// reached when a touchpad is present (the rail hides the page otherwise).
fn touchpad_page(state: &Settings) -> Element<'_, Message> {
    let cb = |lbl: &'static str, checked: bool, msg: fn(bool) -> Message| {
        checkbox(lbl, checked)
            .on_toggle(msg)
            .size(metrics::UI_PX)
            .text_size(metrics::UI_PX)
            .spacing(8.0)
            .style(mde_ui::checkbox_style)
    };

    let speed_items: Vec<u8> = (1u8..=10).collect();
    let speed_row = Row::new()
        .spacing(8.0)
        .align_y(iced::alignment::Vertical::Center)
        .push(
            text("Cursor speed")
                .size(metrics::UI_PX)
                .width(Length::Fixed(230.0))
                .color(palette::color(palette::WINDOW_TEXT)),
        )
        .push(
            pick_list(
                speed_items,
                Some(state.touchpad_speed),
                Message::SetTouchpadSpeed,
            )
            .text_size(metrics::UI_PX),
        );

    Column::new()
        .spacing(14.0)
        .push(cb(
            "Touchpad",
            state.touchpad_enabled,
            Message::SetTouchpadEnabled,
        ))
        .push(speed_row)
        .push(cb(
            "Tap with a single finger to single-click",
            state.touchpad_tap,
            Message::SetTouchpadTap,
        ))
        .push(cb(
            "Drag two fingers to scroll",
            state.touchpad_two_finger,
            Message::SetTouchpadTwoFinger,
        ))
        .push(cb(
            "Reverse scrolling direction",
            state.touchpad_natural_scroll,
            Message::SetTouchpadNatural,
        ))
        .into()
}

/// Devices ▸ Typing (E12.8): keyboard layout (→ labwc `environment`, next sign-in),
/// key-repeat rate + delay (→ `<keyboard>` live), and advisory autocorrect /
/// suggestion toggles (menu.json only — no IME backend in this shell).
fn typing_page(state: &Settings) -> Element<'_, Message> {
    let label = |t: &'static str| {
        text(t)
            .size(metrics::UI_PX)
            .width(Length::Fixed(150.0))
            .color(palette::color(palette::WINDOW_TEXT))
    };
    let row = |lbl: &'static str, control: Element<'static, Message>| {
        Row::new()
            .spacing(8.0)
            .align_y(iced::alignment::Vertical::Center)
            .push(label(lbl))
            .push(control)
    };

    let layout_row = Row::new()
        .spacing(8.0)
        .align_y(iced::alignment::Vertical::Center)
        .push(label("Keyboard layout"))
        .push(
            pick_list(
                Layout::all(),
                Some(Layout::for_code(&state.kb_layout)),
                Message::SetKbLayout,
            )
            .text_size(metrics::UI_PX),
        )
        .push(
            text("Takes effect at next sign-in")
                .size(metrics::BADGE_PX)
                .color(palette::color(palette::GRAY_TEXT)),
        );

    let rate = pick_list(
        RepeatRate::ALL.to_vec(),
        Some(RepeatRate::from_rate(state.kb_repeat_rate)),
        Message::SetRepeatRate,
    )
    .text_size(metrics::UI_PX);
    let delay = pick_list(
        RepeatDelay::ALL.to_vec(),
        Some(RepeatDelay::from_delay(state.kb_repeat_delay)),
        Message::SetRepeatDelay,
    )
    .text_size(metrics::UI_PX);

    let adv = |lbl: &'static str, checked: bool, msg: fn(bool) -> Message| {
        checkbox(lbl, checked)
            .on_toggle(msg)
            .size(metrics::UI_PX)
            .text_size(metrics::UI_PX)
            .spacing(8.0)
            .style(mde_ui::checkbox_style)
    };

    Column::new()
        .spacing(14.0)
        .push(layout_row)
        .push(row("Repeat rate", rate.into()))
        .push(row("Repeat delay", delay.into()))
        .push(
            text("Spelling")
                .size(metrics::UI_PX)
                .color(palette::color(palette::GRAY_TEXT)),
        )
        .push(adv(
            "Autocorrect misspelled words",
            state.typing_autocorrect,
            Message::SetAutocorrect,
        ))
        .push(adv(
            "Show text suggestions as I type",
            state.typing_suggestions,
            Message::SetSuggestions,
        ))
        .push(
            text("Spelling preferences are saved for apps that support them.")
                .size(metrics::BADGE_PX)
                .color(palette::color(palette::GRAY_TEXT)),
        )
        .into()
}

/// Devices ▸ AutoPlay (E12.9): the master toggle + per-type default-action
/// pick_lists. The settings drive `mde devices-monitor`, which opens removable
/// media in Files (or notifies) on mount.
fn autoplay_page(state: &Settings) -> Element<'_, Message> {
    let master = checkbox(
        "Use AutoPlay for all media and devices",
        state.autoplay_enabled,
    )
    .on_toggle(Message::SetAutoplayEnabled)
    .size(metrics::UI_PX)
    .text_size(metrics::UI_PX)
    .spacing(8.0)
    .style(mde_ui::checkbox_style);

    let pick = |label_text: &'static str,
                value: AutoAction,
                msg: fn(AutoAction) -> Message|
     -> Element<'_, Message> {
        Row::new()
            .spacing(8.0)
            .align_y(iced::alignment::Vertical::Center)
            .push(
                text(label_text)
                    .size(metrics::UI_PX)
                    .width(Length::Fixed(160.0))
                    .color(palette::color(palette::WINDOW_TEXT)),
            )
            .push(pick_list(AutoAction::ALL.to_vec(), Some(value), msg).text_size(metrics::UI_PX))
            .into()
    };

    Column::new()
        .spacing(14.0)
        .push(master)
        .push(
            text("Choose AutoPlay defaults")
                .size(metrics::UI_PX)
                .color(palette::color(palette::GRAY_TEXT)),
        )
        .push(pick(
            "Removable drive",
            state.autoplay_removable,
            Message::SetAutoplayRemovable,
        ))
        .push(pick(
            "Memory card",
            state.autoplay_memcard,
            Message::SetAutoplayMemcard,
        ))
        .into()
}

/// A proportional usage bar (accent fill on a muted track), E17.4.
fn usage_bar(frac: f32) -> Element<'static, Message> {
    const BAR_W: f32 = 360.0;
    const BAR_H: f32 = 8.0;
    let fill = (BAR_W * frac.clamp(0.0, 1.0)).max(0.0);
    let cell = |w: Length, color: iced::Color| {
        container(text(""))
            .width(w)
            .height(Length::Fixed(BAR_H))
            .style(move |_: &iced::Theme| container::Style {
                background: Some(color.into()),
                ..Default::default()
            })
    };
    Row::new()
        .width(Length::Fixed(BAR_W))
        .push(cell(Length::Fixed(fill), palette::accent()))
        .push(cell(Length::Fill, palette::color(palette::WINDOW_FRAME)))
        .into()
}

/// System ▸ Storage (E17.4): "This PC" usage summary + per-category bars, the
/// Storage Sense toggle, the other drives, and the Apps & features drill-in.
fn storage_page(state: &Settings) -> Element<'_, Message> {
    let Some(u) = &state.storage else {
        return text("Reading storage…")
            .size(metrics::UI_PX)
            .color(palette::color(palette::GRAY_TEXT))
            .into();
    };
    if state.storage_apps {
        return apps_drill_in(state);
    }
    if state.storage_clean {
        return clean_drill_in(state);
    }

    let root = u.mounts.iter().find(|m| m.target == "/");
    // Bars show each category's share of the *used* space (so they compose what's
    // in use — Win10 shows the breakdown of used storage, not of empty capacity).
    let root_used = root.map(|m| m.used).unwrap_or(1).max(1);

    let mut col = Column::new().spacing(12.0);
    if let Some(r) = root {
        col = col.push(
            text(format!(
                "This PC — {} of {} used",
                crate::sysinfo::human_bytes(r.used),
                crate::sysinfo::human_bytes(r.total)
            ))
            .size(metrics::UI_PX)
            .color(palette::color(palette::WINDOW_TEXT)),
        );
    }

    // Per-category bars.
    let mut bars = Column::new().spacing(8.0);
    for (cat, bytes) in &u.categories {
        let frac = *bytes as f32 / root_used as f32;
        bars = bars.push(
            Column::new()
                .spacing(3.0)
                .push(
                    Row::new()
                        .push(
                            text(cat.label())
                                .size(metrics::UI_PX)
                                .width(Length::Fill)
                                .color(palette::color(palette::WINDOW_TEXT)),
                        )
                        .push(
                            text(crate::sysinfo::human_bytes(*bytes))
                                .size(metrics::BADGE_PX)
                                .color(palette::color(palette::GRAY_TEXT)),
                        ),
                )
                .push(usage_bar(frac)),
        );
    }
    col = col.push(bars);

    // Apps & features drill-in.
    col = col.push(
        button(text("Apps & features").size(metrics::UI_PX))
            .on_press(Message::ShowApps)
            .padding(Padding::from([4.0, 12.0]))
            .style(mde_ui::button_ghost),
    );

    // Storage Sense.
    col = col
        .push(
            checkbox("Storage Sense", state.storage_sense)
                .on_toggle(Message::SetStorageSense)
                .size(metrics::UI_PX)
                .text_size(metrics::UI_PX)
                .spacing(8.0)
                .style(mde_ui::checkbox_style),
        )
        .push(
            text("Free up space automatically by deleting temporary files and emptying the Trash on a schedule.")
                .size(metrics::BADGE_PX)
                .color(palette::color(palette::GRAY_TEXT)),
        )
        .push(
            button(text("Configure Storage Sense or run it now").size(metrics::UI_PX))
                .on_press(Message::ShowClean)
                .padding(Padding::from([4.0, 12.0]))
                .style(mde_ui::button_ghost),
        );

    // Other drives.
    let others: Vec<&crate::sysinfo::Mount> = u.mounts.iter().filter(|m| m.target != "/").collect();
    if !others.is_empty() {
        col = col.push(
            text("Storage usage on other drives")
                .size(metrics::UI_PX)
                .color(palette::color(palette::GRAY_TEXT)),
        );
        for m in others {
            col = col.push(
                text(format!(
                    "{} — {} of {} used",
                    m.target,
                    crate::sysinfo::human_bytes(m.used),
                    crate::sysinfo::human_bytes(m.total)
                ))
                .size(metrics::BADGE_PX)
                .color(palette::color(palette::WINDOW_TEXT)),
            );
        }
    }

    scrollable(col).style(mde_ui::scrollbar).into()
}

/// The Apps & features drill-in (E17.4): the largest installed packages, each with
/// a (greyed, no-backend) Move and an Uninstall behind a confirm → `pkexec dnf remove`.
fn apps_drill_in(state: &Settings) -> Element<'_, Message> {
    let back = button(text("\u{2190} Apps & features").size(metrics::UI_PX))
        .on_press(Message::BackFromApps)
        .padding(Padding::from([3.0, 10.0]))
        .style(mde_ui::button_ghost);

    let mut list = Column::new().spacing(2.0);
    if state.packages.is_empty() {
        list = list.push(
            text("Reading installed apps…")
                .size(metrics::UI_PX)
                .color(palette::color(palette::GRAY_TEXT)),
        );
    }
    // Top-by-size (the full set is thousands of packages); 60 is plenty to scroll.
    for p in state.packages.iter().take(60) {
        let row = Row::new()
            .spacing(10.0)
            .align_y(iced::alignment::Vertical::Center)
            .push(
                Column::new()
                    .width(Length::Fill)
                    .push(
                        text(p.name.clone())
                            .size(metrics::UI_PX)
                            .color(palette::color(palette::WINDOW_TEXT)),
                    )
                    .push(
                        text(crate::sysinfo::human_bytes(p.size))
                            .size(metrics::BADGE_PX)
                            .color(palette::color(palette::GRAY_TEXT)),
                    ),
            )
            // Move has no backend on Linux — shown disabled (no on_press), greyed.
            .push(
                button(
                    text("Move")
                        .size(metrics::UI_PX)
                        .color(palette::color(palette::GRAY_TEXT)),
                )
                .padding(Padding::from([3.0, 10.0]))
                .style(mde_ui::button_ghost),
            )
            .push(
                button(text("Uninstall").size(metrics::UI_PX))
                    .on_press(Message::AskUninstall(p.name.clone()))
                    .padding(Padding::from([3.0, 10.0]))
                    .style(mde_ui::button_ghost),
            );
        list = list.push(container(row).padding(Padding::from([4.0, 4.0])));
    }

    let mut col = Column::new()
        .spacing(10.0)
        .push(back)
        .push(container(scrollable(list).style(mde_ui::scrollbar)).height(Length::Fill));

    // Inline uninstall confirm (the accounts-page pattern).
    if let Some(name) = &state.confirm_uninstall {
        col = col.push(
            Column::new()
                .spacing(6.0)
                .push(
                    text(format!("Uninstall {name}?"))
                        .size(metrics::UI_PX)
                        .color(palette::color(palette::WINDOW_TEXT)),
                )
                .push(
                    Row::new()
                        .spacing(8.0)
                        .push(
                            button(text("Uninstall").size(metrics::UI_PX))
                                .on_press(Message::ConfirmUninstall(name.clone()))
                                .padding(Padding::from([3.0, 12.0]))
                                .style(mde_ui::button_primary),
                        )
                        .push(
                            button(text("Cancel").size(metrics::UI_PX))
                                .on_press(Message::CancelUninstall)
                                .padding(Padding::from([3.0, 12.0]))
                                .style(mde_ui::button_ghost),
                        ),
                ),
        );
    }
    col.into()
}

/// Configure Storage Sense / Clean now (E17.5): a Clean now button that purges the
/// thumbnail cache + Trash and reclaims package/journal space (via pkexec), behind
/// a confirm, reporting the freed bytes.
fn clean_drill_in(state: &Settings) -> Element<'_, Message> {
    let back = button(text("\u{2190} Storage").size(metrics::UI_PX))
        .on_press(Message::BackFromClean)
        .padding(Padding::from([3.0, 10.0]))
        .style(mde_ui::button_ghost);

    let mut col = Column::new()
        .spacing(12.0)
        .push(back)
        .push(
            text("Free up space now")
                .size(metrics::INFO_TITLE_PX)
                .color(palette::color(palette::WINDOW_TEXT)),
        )
        .push(
            text("Empties the thumbnail cache and Trash, clears the package cache (dnf clean all), and vacuums old system logs.")
                .size(metrics::UI_PX)
                .color(palette::color(palette::GRAY_TEXT)),
        );

    if let Some(freed) = state.last_freed {
        col = col.push(
            text(format!("Freed {}.", crate::sysinfo::human_bytes(freed)))
                .size(metrics::UI_PX)
                .color(palette::color(palette::STATUS_OK)),
        );
    }

    if state.confirm_clean {
        col = col.push(
            Row::new()
                .spacing(8.0)
                .push(
                    text("Clean now? This empties the Trash.")
                        .size(metrics::UI_PX)
                        .color(palette::color(palette::WINDOW_TEXT)),
                )
                .push(
                    button(text("Clean now").size(metrics::UI_PX))
                        .on_press(Message::ConfirmClean)
                        .padding(Padding::from([3.0, 12.0]))
                        .style(mde_ui::button_primary),
                )
                .push(
                    button(text("Cancel").size(metrics::UI_PX))
                        .on_press(Message::CancelClean)
                        .padding(Padding::from([3.0, 12.0]))
                        .style(mde_ui::button_ghost),
                ),
        );
    } else {
        col = col.push(
            button(text("Clean now").size(metrics::UI_PX))
                .on_press(Message::AskClean)
                .padding(Padding::from([4.0, 14.0]))
                .style(mde_ui::button_primary),
        );
    }
    col.into()
}

/// Update & Security ▸ Backup (E17.6): pick a Timeshift snapshot drive from the
/// live filesystems, the automatic-backup toggle, and the current target.
fn backup_page(state: &Settings) -> Element<'_, Message> {
    let Some(u) = &state.storage else {
        return text("Reading drives…")
            .size(metrics::UI_PX)
            .color(palette::color(palette::GRAY_TEXT))
            .into();
    };
    if state.backup_more {
        return backup_more_page(state);
    }

    let mut col = Column::new()
        .spacing(12.0)
        .push(
            text("Back up using Timeshift")
                .size(metrics::INFO_TITLE_PX)
                .color(palette::color(palette::WINDOW_TEXT)),
        )
        .push(
            text("Pick a drive to hold restore points; Timeshift snapshots the system to it.")
                .size(metrics::UI_PX)
                .color(palette::color(palette::GRAY_TEXT)),
        );

    if !state.backup_drive.is_empty() {
        col = col
            .push(
                text(format!("Backing up to {}", state.backup_drive))
                    .size(metrics::UI_PX)
                    .color(palette::color(palette::STATUS_OK)),
            )
            .push(
                checkbox("Automatically back up on a schedule", state.auto_backup)
                    .on_toggle(Message::SetAutoBackup)
                    .size(metrics::UI_PX)
                    .text_size(metrics::UI_PX)
                    .spacing(8.0)
                    .style(mde_ui::checkbox_style),
            )
            .push(
                Row::new()
                    .spacing(8.0)
                    .push(
                        button(text("More options").size(metrics::UI_PX))
                            .on_press(Message::ShowBackupMore)
                            .padding(Padding::from([3.0, 12.0]))
                            .style(mde_ui::button_primary),
                    )
                    .push(
                        button(text("Remove drive").size(metrics::UI_PX))
                            .on_press(Message::RemoveBackupDrive)
                            .padding(Padding::from([3.0, 12.0]))
                            .style(mde_ui::button_ghost),
                    ),
            );
    } else {
        col = col.push(
            text("Add a drive")
                .size(metrics::UI_PX)
                .color(palette::color(palette::GRAY_TEXT)),
        );
        // One row per distinct backing device (several mounts can share a disk).
        let mut seen: Vec<String> = Vec::new();
        for m in &u.mounts {
            if seen.contains(&m.source) {
                continue;
            }
            seen.push(m.source.clone());
            let dev = m.source.clone();
            let row = Row::new()
                .spacing(10.0)
                .align_y(iced::alignment::Vertical::Center)
                .push(
                    Column::new()
                        .width(Length::Fill)
                        .push(
                            text(m.source.clone())
                                .size(metrics::UI_PX)
                                .color(palette::color(palette::WINDOW_TEXT)),
                        )
                        .push(
                            text(format!(
                                "{} free of {} · mounted at {}",
                                crate::sysinfo::human_bytes(m.avail),
                                crate::sysinfo::human_bytes(m.total),
                                m.target
                            ))
                            .size(metrics::BADGE_PX)
                            .color(palette::color(palette::GRAY_TEXT)),
                        ),
                )
                .push(
                    button(text("Use this drive").size(metrics::UI_PX))
                        .on_press(Message::SetBackupDrive(dev))
                        .padding(Padding::from([3.0, 10.0]))
                        .style(mde_ui::button_primary),
                );
            col = col.push(container(row).padding(Padding::from([4.0, 4.0])));
        }
    }
    scrollable(col).style(mde_ui::scrollbar).into()
}

/// Backup ▸ More options (E17.7): Back up now, the schedule + retention pick_lists,
/// and the included-folders add/remove list.
fn backup_more_page(state: &Settings) -> Element<'_, Message> {
    let back = button(text("\u{2190} Backup").size(metrics::UI_PX))
        .on_press(Message::BackFromMore)
        .padding(Padding::from([3.0, 10.0]))
        .style(mde_ui::button_ghost);

    let row = |lbl: &'static str, control: Element<'static, Message>| {
        Row::new()
            .spacing(8.0)
            .align_y(iced::alignment::Vertical::Center)
            .push(
                text(lbl)
                    .size(metrics::UI_PX)
                    .width(Length::Fixed(170.0))
                    .color(palette::color(palette::WINDOW_TEXT)),
            )
            .push(control)
    };

    // Back up now (confirm → pkexec timeshift --create).
    let backup_now: Element<'_, Message> = if state.confirm_backup {
        Row::new()
            .spacing(8.0)
            .push(
                text("Create a snapshot now?")
                    .size(metrics::UI_PX)
                    .color(palette::color(palette::WINDOW_TEXT)),
            )
            .push(
                button(text("Back up now").size(metrics::UI_PX))
                    .on_press(Message::ConfirmBackupNow)
                    .padding(Padding::from([3.0, 12.0]))
                    .style(mde_ui::button_primary),
            )
            .push(
                button(text("Cancel").size(metrics::UI_PX))
                    .on_press(Message::CancelBackupNow)
                    .padding(Padding::from([3.0, 12.0]))
                    .style(mde_ui::button_ghost),
            )
            .into()
    } else {
        button(text("Back up now").size(metrics::UI_PX))
            .on_press(Message::AskBackupNow)
            .padding(Padding::from([4.0, 14.0]))
            .style(mde_ui::button_primary)
            .into()
    };

    let mut col = Column::new()
        .spacing(12.0)
        .push(back)
        .push(
            text("More options")
                .size(metrics::INFO_TITLE_PX)
                .color(palette::color(palette::WINDOW_TEXT)),
        )
        .push(backup_now)
        .push(row(
            "Back up my files",
            pick_list(
                Schedule::ALL.to_vec(),
                Some(state.backup_schedule),
                Message::SetSchedule,
            )
            .text_size(metrics::UI_PX)
            .into(),
        ))
        .push(row(
            "Keep my backups",
            pick_list(
                Retention::ALL.to_vec(),
                Some(state.backup_retention),
                Message::SetRetention,
            )
            .text_size(metrics::UI_PX)
            .into(),
        ))
        .push(
            text("Back up these folders")
                .size(metrics::UI_PX)
                .color(palette::color(palette::GRAY_TEXT)),
        );

    for inc in &state.backup_includes {
        col = col.push(
            Row::new()
                .spacing(10.0)
                .align_y(iced::alignment::Vertical::Center)
                .push(
                    text(inc.clone())
                        .size(metrics::UI_PX)
                        .width(Length::Fill)
                        .color(palette::color(palette::WINDOW_TEXT)),
                )
                .push(
                    button(text("Remove").size(metrics::UI_PX))
                        .on_press(Message::RemoveInclude(inc.clone()))
                        .padding(Padding::from([3.0, 10.0]))
                        .style(mde_ui::button_ghost),
                ),
        );
    }
    col = col.push(
        button(text("+ Add a folder").size(metrics::UI_PX))
            .on_press(Message::AddInclude)
            .padding(Padding::from([4.0, 12.0]))
            .style(mde_ui::button_ghost),
    );

    scrollable(col).style(mde_ui::scrollbar).into()
}

/// Network & Internet ▸ Cellular (E15.12): a greyed, disabled "Cellular" toggle +
/// a "No cellular hardware detected" advisory — Win10-fidelity, not a working
/// surface (§3: shown disabled, never an inert mockup).
fn cellular_page() -> Element<'static, Message> {
    Column::new()
        .spacing(10.0)
        .push(
            // No on_toggle → disabled, like the labwc-managed taskbar toggles.
            checkbox("Cellular", false)
                .size(metrics::UI_PX)
                .text_size(metrics::UI_PX)
                .spacing(8.0)
                .style(mde_ui::checkbox_style),
        )
        .push(
            text("No cellular hardware detected.")
                .size(metrics::UI_PX)
                .color(palette::color(palette::GRAY_TEXT)),
        )
        .into()
}

/// Human-readable byte count (decimal) for the Data-usage page (E15.11).
fn human_bytes(b: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1000.0 && i < U.len() - 1 {
        v /= 1000.0;
        i += 1;
    }
    if i == 0 {
        format!("{b} B")
    } else {
        format!("{v:.1} {}", U[i])
    }
}

/// Network & Internet ▸ Data usage (E15.11): per-device rx+tx with a proportional
/// bar (vs the busiest device), a total, and an editable monthly limit (MB) that
/// notifies when the current usage already exceeds it. Read live on show.
fn data_usage_page(state: &Settings) -> Element<'_, Message> {
    let max = state
        .usage
        .iter()
        .map(|(_, rx, tx)| rx + tx)
        .max()
        .unwrap_or(1)
        .max(1);
    let total: u64 = state.usage.iter().map(|(_, rx, tx)| rx + tx).sum();
    let mut list = Column::new().spacing(6.0);
    for (name, rx, tx) in &state.usage {
        let used = rx + tx;
        let filled = ((used * 100 / max) as u16).max(1);
        let bar = Row::new()
            .height(Length::Fixed(8.0))
            .push(
                container(Space::new(Length::Fill, Length::Fill))
                    .width(Length::FillPortion(filled))
                    .style(|_| container::Style {
                        background: Some(Background::Color(palette::accent())),
                        ..container::Style::default()
                    }),
            )
            .push(Space::new(
                Length::FillPortion(100 - filled + 1),
                Length::Shrink,
            ));
        list = list.push(
            Column::new()
                .spacing(2.0)
                .push(
                    Row::new()
                        .push(
                            text(name.clone())
                                .size(metrics::UI_PX)
                                .width(Length::Fill)
                                .color(palette::color(palette::WINDOW_TEXT)),
                        )
                        .push(
                            text(human_bytes(used))
                                .size(metrics::UI_PX)
                                .color(palette::color(palette::GRAY_TEXT)),
                        ),
                )
                .push(bar),
        );
    }
    let limit = Row::new()
        .spacing(8.0)
        .align_y(iced::alignment::Vertical::Center)
        .push(
            text("Monthly limit (MB)")
                .size(metrics::UI_PX)
                .width(Length::Fixed(150.0))
                .color(palette::color(palette::WINDOW_TEXT)),
        )
        .push(
            text_input("none", &state.data_limit)
                .on_input(Message::SetDataLimit)
                .size(metrics::UI_PX)
                .width(Length::Fixed(100.0)),
        );
    Column::new()
        .spacing(14.0)
        .push(
            text(format!("Total since boot: {}", human_bytes(total)))
                .size(metrics::UI_PX)
                .color(palette::color(palette::WINDOW_TEXT)),
        )
        .push(container(scrollable(list).style(mde_ui::scrollbar)).height(Length::Fill))
        .push(limit)
        .into()
}

/// Network & Internet ▸ Airplane mode (E15.10): a master Airplane toggle
/// (`rfkill block/unblock all`) over a per-radio Wi-Fi toggle (`nmcli radio wifi`),
/// disabled while airplane mode is on. State read live on show.
fn airplane_page(state: &Settings) -> Element<'_, Message> {
    let cb = |label: &'static str, on: bool| {
        checkbox(label, on)
            .size(metrics::UI_PX)
            .text_size(metrics::UI_PX)
            .spacing(8.0)
            .style(mde_ui::checkbox_style)
    };
    // Wi-Fi reads off while airplane mode is on, and is disabled then (no on_toggle).
    let mut wifi = cb("Wi-Fi", state.wifi_radio && !state.airplane);
    if !state.airplane {
        wifi = wifi.on_toggle(Message::SetWifiRadio);
    }
    Column::new()
        .spacing(12.0)
        .push(cb("Airplane mode", state.airplane).on_toggle(Message::SetAirplane))
        .push(
            text("Wireless devices")
                .size(metrics::UI_PX)
                .font(mde_ui::font::ui_bold())
                .color(palette::color(palette::WINDOW_TEXT)),
        )
        .push(wifi)
        .into()
}

/// Network & Internet ▸ Proxy (E15.9): an Off/Manual/Automatic mode picker; under
/// Manual, an address + port + Apply that writes `org.gnome.system.proxy*` via
/// gsettings. Read live at settings start.
fn proxy_page(state: &Settings) -> Element<'_, Message> {
    let mode = Row::new()
        .spacing(8.0)
        .align_y(iced::alignment::Vertical::Center)
        .push(
            text("Use a proxy server")
                .size(metrics::UI_PX)
                .width(Length::Fixed(130.0))
                .color(palette::color(palette::WINDOW_TEXT)),
        )
        .push(
            pick_list(
                ProxyMode::ALL.to_vec(),
                Some(state.proxy_mode),
                Message::SetProxyMode,
            )
            .text_size(metrics::UI_PX),
        );
    let mut col = Column::new().spacing(12.0).push(mode);
    if state.proxy_mode == ProxyMode::Manual {
        let field = |label: &'static str, value: &str, msg: fn(String) -> Message, w: f32| {
            Row::new()
                .spacing(8.0)
                .align_y(iced::alignment::Vertical::Center)
                .push(
                    text(label)
                        .size(metrics::UI_PX)
                        .width(Length::Fixed(130.0))
                        .color(palette::color(palette::WINDOW_TEXT)),
                )
                .push(
                    text_input(label, value)
                        .on_input(msg)
                        .on_submit(Message::ApplyProxy)
                        .size(metrics::UI_PX)
                        .width(Length::Fixed(w)),
                )
        };
        col = col
            .push(field(
                "Address",
                &state.proxy_host,
                Message::ProxyHost,
                220.0,
            ))
            .push(field("Port", &state.proxy_port, Message::ProxyPort, 80.0))
            .push(
                button(text("Apply").size(metrics::UI_PX))
                    .on_press(Message::ApplyProxy)
                    .padding(Padding::from([6.0, 16.0]))
                    .style(tile_style),
            );
    }
    col.into()
}

/// Network & Internet ▸ Mobile hotspot (E15.8): a "Share my Internet connection"
/// toggle (drives `nmcli device wifi hotspot`) over the network name + key fields
/// (persisted to state — `nmcli` reads them on enable). The toggle's on-state is
/// the live "Hotspot" connection. (No modal — settings.rs is a flat shell; the
/// name/key edit is inline. The same `nm::set_hotspot` is callable from the AC tile.)
fn hotspot_page(state: &Settings) -> Element<'_, Message> {
    let on = state
        .net_conns
        .iter()
        .any(|c| c.name == "Hotspot" && c.state == "activated");
    let field = |label: &'static str, value: &str, msg: fn(String) -> Message, secure: bool| {
        Row::new()
            .spacing(8.0)
            .align_y(iced::alignment::Vertical::Center)
            .push(
                text(label)
                    .size(metrics::UI_PX)
                    .width(Length::Fixed(130.0))
                    .color(palette::color(palette::WINDOW_TEXT)),
            )
            .push(
                text_input(label, value)
                    .on_input(msg)
                    .secure(secure)
                    .size(metrics::UI_PX)
                    .width(Length::Fixed(220.0)),
            )
    };
    Column::new()
        .spacing(12.0)
        .push(
            checkbox("Share my Internet connection with other devices", on)
                .on_toggle(Message::SetHotspotOn)
                .size(metrics::UI_PX)
                .text_size(metrics::UI_PX)
                .spacing(8.0)
                .style(mde_ui::checkbox_style),
        )
        .push(field(
            "Network name",
            &state.hotspot_name,
            Message::HotspotName,
            false,
        ))
        .push(field(
            "Network password",
            &state.hotspot_password,
            Message::HotspotPassword,
            true,
        ))
        .into()
}

/// Network & Internet ▸ Ethernet (E15.7): the wired connection summary + a
/// Private/Public network-profile toggle (reuses the active connection's zone via
/// `SetNetPrivate`, E15.5). Shows "not connected" when no wired link is active.
fn ethernet_page(state: &Settings) -> Element<'_, Message> {
    let wired = state
        .net_conns
        .iter()
        .find(|c| c.kind.contains("ethernet") && c.state == "activated");
    let Some(c) = wired else {
        return text("Not connected via Ethernet.")
            .size(metrics::UI_PX)
            .color(palette::color(palette::GRAY_TEXT))
            .into();
    };
    let card = Column::new()
        .spacing(2.0)
        .push(
            text(format!("Connected — {}", c.name))
                .size(metrics::INFO_TITLE_PX)
                .color(palette::color(palette::WINDOW_TEXT)),
        )
        .push(
            text(format!("Wired · {}", c.device))
                .size(metrics::UI_PX)
                .color(palette::color(palette::GRAY_TEXT)),
        );
    let is_private = state.net_zone != "public";
    Column::new()
        .spacing(16.0)
        .push(card)
        .push(
            checkbox("Make this a private (trusted) network", is_private)
                .on_toggle(Message::SetNetPrivate)
                .size(metrics::UI_PX)
                .text_size(metrics::UI_PX)
                .spacing(8.0)
                .style(mde_ui::checkbox_style),
        )
        .into()
}

/// Network & Internet ▸ VPN (E15.7): the configured VPN/WireGuard connections with
/// per-row Connect/Disconnect (`nmcli connection up/down`) + an "Add VPN" button
/// that opens nm-connection-editor.
fn vpn_page(state: &Settings) -> Element<'_, Message> {
    let add = button(text("Add VPN connection").size(metrics::UI_PX))
        .on_press(Message::AddVpn)
        .padding(Padding::from([6.0, 16.0]))
        .style(tile_style);
    let body: Element<Message> = if state.vpns.is_empty() {
        text("No VPN connections configured.")
            .size(metrics::UI_PX)
            .color(palette::color(palette::GRAY_TEXT))
            .into()
    } else {
        let mut col = Column::new().spacing(4.0);
        for v in &state.vpns {
            let active = v.state == "activated";
            col = col.push(
                Row::new()
                    .spacing(8.0)
                    .align_y(iced::alignment::Vertical::Center)
                    .push(
                        text(v.name.clone())
                            .size(metrics::UI_PX)
                            .width(Length::Fill)
                            .color(palette::color(palette::WINDOW_TEXT)),
                    )
                    .push(
                        button(
                            text(if active { "Disconnect" } else { "Connect" })
                                .size(metrics::UI_PX),
                        )
                        .on_press(Message::VpnToggle(v.name.clone(), !active))
                        .padding(Padding::from([2.0, 10.0]))
                        .style(tile_style),
                    )
                    .padding(Padding::from([2.0, 4.0])),
            );
        }
        container(scrollable(col).style(mde_ui::scrollbar))
            .height(Length::Fill)
            .into()
    };
    Column::new()
        .spacing(12.0)
        .push(add)
        .push(container(body).height(Length::Fill))
        .into()
}

/// Network & Internet ▸ Wi-Fi (E15.6): an "auto-connect to known networks" toggle
/// over the scanned SSID list (ssid + lock + signal % + Connect — the connect key
/// prompt is handed to the flyout, E15.4). Scanned async on show.
fn wifi_page(state: &Settings) -> Element<'_, Message> {
    let toggle = checkbox(
        "Automatically connect to known networks",
        state.wifi_autoconnect,
    )
    .on_toggle(Message::SetWifiAutoconnect)
    .size(metrics::UI_PX)
    .text_size(metrics::UI_PX)
    .spacing(8.0)
    .style(mde_ui::checkbox_style);
    let body: Element<Message> = if state.wifi_scanning || state.wifis.is_none() {
        text("Scanning for networks…")
            .size(metrics::UI_PX)
            .color(palette::color(palette::GRAY_TEXT))
            .into()
    } else {
        match &state.wifis {
            Some(list) if !list.is_empty() => {
                let mut col = Column::new().spacing(0.0);
                for w in list {
                    // A saved network shows Forget (delete); an unsaved one, Connect.
                    let saved = state.wifi_saved.iter().any(|n| n == &w.ssid);
                    let action = if saved {
                        button(text("Forget").size(metrics::UI_PX))
                            .on_press(Message::ForgetSsid(w.ssid.clone()))
                    } else {
                        button(text("Connect").size(metrics::UI_PX))
                            .on_press(Message::ConnectSsid(w.ssid.clone()))
                    };
                    col = col.push(
                        Row::new()
                            .spacing(8.0)
                            .align_y(iced::alignment::Vertical::Center)
                            .push(
                                text(w.ssid.clone())
                                    .size(metrics::UI_PX)
                                    .width(Length::Fill)
                                    .color(palette::color(palette::WINDOW_TEXT)),
                            )
                            .push(
                                text(if w.secured { "\u{f023}" } else { " " })
                                    .size(metrics::PANEL_GLYPH_PX)
                                    .font(mde_ui::font::NERD)
                                    .color(palette::color(palette::GRAY_TEXT)),
                            )
                            .push(
                                text(format!("{}%", w.signal))
                                    .size(metrics::UI_PX)
                                    .color(palette::color(palette::GRAY_TEXT)),
                            )
                            .push(action.padding(Padding::from([2.0, 10.0])).style(tile_style))
                            .padding(Padding::from([3.0, 6.0])),
                    );
                }
                container(scrollable(col).style(mde_ui::scrollbar))
                    .height(Length::Fill)
                    .into()
            }
            _ => text("No networks found.")
                .size(metrics::UI_PX)
                .color(palette::color(palette::GRAY_TEXT))
                .into(),
        }
    };
    Column::new()
        .spacing(12.0)
        .push(toggle)
        .push(container(body).height(Length::Fill))
        .into()
}

/// Network & Internet ▸ Status (E15.5): the active connection summary, a
/// Private/Public network-profile toggle (firewalld zone), and an Advanced handoff
/// to nm-connection-editor. Reads live from nmcli (loaded at settings start).
fn network_status_page(state: &Settings) -> Element<'_, Message> {
    let active = state.net_conns.iter().find(|c| c.state == "activated");
    let (glyph, head, sub) = match active {
        Some(c) => (
            "\u{f1eb}", // nf-fa-wifi
            format!("Connected — {}", c.name),
            format!("{} on {}", c.kind, c.device),
        ),
        None => ("\u{f071}", "Not connected".to_string(), String::new()), // nf-fa-warning
    };
    let mut lines = Column::new().spacing(2.0).push(
        text(head)
            .size(metrics::INFO_TITLE_PX)
            .color(palette::color(palette::WINDOW_TEXT)),
    );
    if !sub.is_empty() {
        lines = lines.push(
            text(sub)
                .size(metrics::UI_PX)
                .color(palette::color(palette::GRAY_TEXT)),
        );
    }
    let card = Row::new()
        .spacing(12.0)
        .align_y(iced::alignment::Vertical::Center)
        .push(
            text(glyph)
                .size(metrics::TILE_GLYPH_PX)
                .font(mde_ui::font::NERD)
                .color(palette::color(palette::WINDOW_TEXT)),
        )
        .push(lines);
    let mut col = Column::new().spacing(16.0).push(card);
    if active.is_some() {
        let is_private = state.net_zone != "public";
        col = col.push(
            checkbox("Make this a private (trusted) network", is_private)
                .on_toggle(Message::SetNetPrivate)
                .size(metrics::UI_PX)
                .text_size(metrics::UI_PX)
                .spacing(8.0)
                .style(mde_ui::checkbox_style),
        );
    }
    col.push(
        button(text("Change advanced sharing options").size(metrics::UI_PX))
            .on_press(Message::OpenNetEditor)
            .padding(Padding::from([6.0, 16.0]))
            .style(tile_style),
    )
    .into()
}

/// Personalization ▸ Colors (E6.4): the Light/Dark choice (re-skins live + persists).
/// The accent picker was retired in the MackesDE rebrand — the accent is now
/// Carbon Blue for every era, not a per-user pick.
fn colors_page(state: &Settings) -> Element<'_, Message> {
    let mode_label = text("Choose your color")
        .size(metrics::UI_PX)
        .color(palette::color(palette::WINDOW_TEXT));
    let light = mode_button("Light", !state.dark, Message::SetDark(false));
    let dark = mode_button("Dark", state.dark, Message::SetDark(true));
    let modes = Row::new().spacing(8.0).push(light).push(dark);

    // E7.5a: "Show accent color on Start & taskbar" — gates the panel chrome's
    // accent tint (palette::chrome_accent). The companion "on title bars" option is
    // superseded by the MackesDE rebrand (the Win10 titlebar matches Carbon, no
    // accent tint), so it's intentionally absent.
    let accent_chrome = checkbox(
        "Show accent color on Start and taskbar",
        state.accent_on_taskbar,
    )
    .on_toggle(Message::SetAccentOnTaskbar)
    .size(metrics::UI_PX)
    .text_size(metrics::UI_PX)
    .spacing(8.0)
    .style(mde_ui::checkbox_style);

    column![mode_label, modes, accent_chrome]
        .spacing(10.0)
        .into()
}

/// Personalization ▸ Background (E7.4): a live preview, a Picture/Solid/
/// Slideshow source dropdown, a thumbnail strip + Browse + fit dropdown for
/// Picture, and Apply — driving swaybg through the shared `crate::wallpaper`
/// + `outputs` helpers.
fn background_page(state: &Settings) -> Element<'_, Message> {
    let sel = state
        .bg_selected
        .and_then(|i| state.bg_wallpapers.get(i))
        .map(String::as_str);
    // Solid previews the themed desktop color (no picture).
    let preview_sel = if state.bg_source == BgSource::Solid {
        None
    } else {
        sel
    };
    let preview = container(wallpaper::preview::<Message>(preview_sel))
        .width(Length::Fixed(320.0))
        .height(Length::Fixed(180.0))
        .style(|_| container::Style {
            border: Border {
                color: palette::color(palette::WINDOW_FRAME),
                width: 1.0,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        });

    let source = Row::new()
        .spacing(8.0)
        .align_y(iced::alignment::Vertical::Center)
        .push(
            text("Background")
                .size(metrics::UI_PX)
                .color(palette::color(palette::WINDOW_TEXT)),
        )
        .push(
            pick_list(
                BgSource::ALL.to_vec(),
                Some(state.bg_source),
                Message::BgSource,
            )
            .text_size(metrics::UI_PX),
        );

    let mut col = Column::new().spacing(12.0).push(preview).push(source);

    match state.bg_source {
        BgSource::Picture => {
            let mut strip = Row::new().spacing(8.0);
            for (i, wp) in state.bg_wallpapers.iter().enumerate().take(24) {
                strip = strip.push(thumb(
                    wp,
                    state.bg_selected == Some(i),
                    Message::BgSelect(i),
                ));
            }
            col = col
                .push(
                    text("Choose your picture")
                        .size(metrics::UI_PX)
                        .color(palette::color(palette::WINDOW_TEXT)),
                )
                .push(scrollable(strip).style(mde_ui::scrollbar))
                .push(fit_row("Browse", state.bg_mode, true));
        }
        BgSource::Solid => {
            col = col.push(
                text("Uses the themed desktop color.")
                    .size(metrics::UI_PX)
                    .color(palette::color(palette::GRAY_TEXT)),
            );
        }
        BgSource::Slideshow => {
            col = col
                .push(
                    text(format!(
                        "Cycles {} pictures every 5 minutes.",
                        state.bg_wallpapers.len()
                    ))
                    .size(metrics::UI_PX)
                    .color(palette::color(palette::GRAY_TEXT)),
                )
                .push(fit_row("", state.bg_mode, false));
        }
    }

    col.push(
        button(text("Apply").size(metrics::UI_PX))
            .on_press(Message::BgApply)
            .padding(Padding::from([6.0, 18.0]))
            .style(tile_style),
    )
    .into()
}

/// The "Browse … + fit dropdown" row shared by the Picture/Slideshow sources.
/// `browse` controls whether the Browse button shows.
fn fit_row<'a>(browse_label: &str, mode: BgMode, browse: bool) -> Element<'a, Message> {
    let mut row = Row::new()
        .spacing(8.0)
        .align_y(iced::alignment::Vertical::Center);
    if browse {
        row = row.push(
            button(text(browse_label.to_string()).size(metrics::UI_PX))
                .on_press(Message::BgBrowse)
                .padding(Padding::from([4.0, 12.0]))
                .style(tile_style),
        );
    }
    row.push(
        text("Choose a fit")
            .size(metrics::UI_PX)
            .color(palette::color(palette::WINDOW_TEXT)),
    )
    .push(pick_list(BgMode::ALL.to_vec(), Some(mode), Message::BgMode).text_size(metrics::UI_PX))
    .into()
}

/// A clickable wallpaper thumbnail (accent border when selected).
fn thumb(path: &str, selected: bool, on_press: Message) -> Element<'static, Message> {
    let img = image(image::Handle::from_path(path))
        .width(Length::Fixed(96.0))
        .height(Length::Fixed(60.0))
        .content_fit(iced::ContentFit::Cover);
    let border_c = if selected {
        palette::accent()
    } else {
        palette::color(palette::WINDOW_FRAME)
    };
    mouse_area(container(img).style(move |_| container::Style {
        border: Border {
            color: border_c,
            width: if selected { 2.0 } else { 1.0 },
            radius: 0.0.into(),
        },
        ..container::Style::default()
    }))
    .on_press(on_press)
    .into()
}

/// Personalization ▸ Themes (E7.7): a gallery of saved {background, accent,
/// mode} bundles + a "Save theme" button. Selecting a tile re-applies the whole
/// bundle (swaybg + accent + mode) in one action.
fn themes_page(state: &Settings) -> Element<'_, Message> {
    let gallery: Element<Message> = if state.themes.is_empty() {
        text("No saved themes yet. Save your current background, accent and mode as a theme.")
            .size(metrics::UI_PX)
            .color(palette::color(palette::GRAY_TEXT))
            .into()
    } else {
        let mut row = Row::new().spacing(12.0);
        for (i, t) in state.themes.iter().enumerate() {
            row = row.push(theme_tile(i, t));
        }
        scrollable(row).style(mde_ui::scrollbar).into()
    };
    let save = button(text("Save theme").size(metrics::UI_PX))
        .on_press(Message::SaveTheme)
        .padding(Padding::from([6.0, 18.0]))
        .style(tile_style);
    Column::new().spacing(16.0).push(gallery).push(save).into()
}

/// One theme tile: the saved wallpaper thumbnail (or the accent swatch when the
/// bundle keeps the current background) over its name. Click re-applies it.
fn theme_tile(i: usize, t: &crate::state::SavedTheme) -> Element<'static, Message> {
    let preview: Element<Message> = if t.wallpaper.is_empty() {
        // No per-theme accent anymore (rebrand) — preview the fixed Carbon accent.
        let c = palette::accent();
        container(Space::new(Length::Fixed(120.0), Length::Fixed(70.0)))
            .style(move |_| container::Style {
                background: Some(Background::Color(c)),
                border: Border {
                    color: palette::color(palette::WINDOW_FRAME),
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..container::Style::default()
            })
            .into()
    } else {
        container(
            image(image::Handle::from_path(&t.wallpaper))
                .width(Length::Fixed(120.0))
                .height(Length::Fixed(70.0))
                .content_fit(iced::ContentFit::Cover),
        )
        .style(|_| container::Style {
            border: Border {
                color: palette::color(palette::WINDOW_FRAME),
                width: 1.0,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        })
        .into()
    };
    let card = Column::new()
        .spacing(4.0)
        .align_x(iced::alignment::Horizontal::Center)
        .push(preview)
        .push(
            text(t.name.clone())
                .size(metrics::UI_PX)
                .color(palette::color(palette::WINDOW_TEXT)),
        );
    mouse_area(card).on_press(Message::ApplyTheme(i)).into()
}

/// Personalization ▸ Start (E7.8): the toggles the Win10 tiled Start consumes.
fn start_page(state: &Settings) -> Element<'_, Message> {
    let row = |label: &'static str, checked: bool, msg: fn(bool) -> Message| {
        checkbox(label, checked)
            .on_toggle(msg)
            .size(metrics::UI_PX)
            .text_size(metrics::UI_PX)
            .spacing(8.0)
            .style(mde_ui::checkbox_style)
    };
    let mut col = Column::new()
        .spacing(10.0)
        .push(row(
            "Show more tiles",
            state.start_more_tiles,
            Message::SetStartMore,
        ))
        .push(row(
            "Use Start full screen",
            state.start_full_screen,
            Message::SetStartFullScreen,
        ))
        .push(row(
            "Show recently added apps",
            state.start_show_recent,
            Message::SetStartRecent,
        ))
        .push(row(
            "Show most used apps",
            state.start_show_suggested,
            Message::SetStartSuggested,
        ))
        .push(
            text("Choose which folders appear on Start")
                .size(metrics::UI_PX)
                .font(mde_ui::font::ui_bold()),
        );
    // One checkbox per selectable system folder (E7.8a); toggling persists and the
    // Start rail reflects it on next open. Closures capture the key, so these can't
    // use the `fn(bool)`-pointer `row` helper above.
    for entry in crate::start_win10::START_FOLDERS {
        let key = entry.0.to_string();
        let checked = state.start_folders.iter().any(|k| k == &key);
        col = col.push(
            checkbox(entry.1, checked)
                .on_toggle(move |on| Message::ToggleStartFolder(key.clone(), on))
                .size(metrics::UI_PX)
                .text_size(metrics::UI_PX)
                .spacing(8.0)
                .style(mde_ui::checkbox_style),
        );
    }
    col.into()
}

/// Personalization ▸ Taskbar (E7.9): the location dropdown drives the panel
/// anchor; lock / auto-hide are labwc-managed (greyed, present for fidelity).
fn taskbar_page(state: &Settings) -> Element<'_, Message> {
    let location = Row::new()
        .spacing(8.0)
        .align_y(iced::alignment::Vertical::Center)
        .push(
            text("Taskbar location on screen")
                .size(metrics::UI_PX)
                .color(palette::color(palette::WINDOW_TEXT)),
        )
        .push(
            pick_list(
                TaskbarLoc::ALL.to_vec(),
                Some(state.taskbar_loc),
                Message::SetTaskbarLoc,
            )
            .text_size(metrics::UI_PX),
        );
    // Search affordance picker (E7.9a) — drives panel.rs's win10_search_affordance.
    let search = Row::new()
        .spacing(8.0)
        .align_y(iced::alignment::Vertical::Center)
        .push(
            text("Search on the taskbar")
                .size(metrics::UI_PX)
                .color(palette::color(palette::WINDOW_TEXT)),
        )
        .push(
            pick_list(
                SearchMode::ALL.to_vec(),
                Some(state.search_mode),
                Message::SetSearchMode,
            )
            .text_size(metrics::UI_PX),
        );
    // Greyed: labwc owns "Lock the taskbar", present for fidelity but not enforced
    // here — like taskbar_properties.rs.
    let greyed = |label: &'static str| {
        checkbox(label, false)
            .size(metrics::UI_PX)
            .text_size(metrics::UI_PX)
            .spacing(8.0)
            .style(mde_ui::checkbox_style)
    };
    Column::new()
        .spacing(10.0)
        .push(location)
        .push(search)
        // Real toggle: panel.rs hides the Task View button when off (E2.9).
        .push(
            checkbox("Show the Task View button", state.show_taskview)
                .on_toggle(Message::SetShowTaskview)
                .size(metrics::UI_PX)
                .text_size(metrics::UI_PX)
                .spacing(8.0)
                .style(mde_ui::checkbox_style),
        )
        // Real toggle (E2.9a): panel.rs starts as a 1px reveal strip when on.
        .push(
            checkbox("Automatically hide the taskbar", state.autohide)
                .on_toggle(Message::SetAutohide)
                .size(metrics::UI_PX)
                .text_size(metrics::UI_PX)
                .spacing(8.0)
                .style(mde_ui::checkbox_style),
        )
        // Real toggle (E7.9a): panel.rs renders a compact 30px bar when on.
        .push(
            checkbox("Use small taskbar buttons", state.small_buttons)
                .on_toggle(Message::SetSmallButtons)
                .size(metrics::UI_PX)
                .text_size(metrics::UI_PX)
                .spacing(8.0)
                .style(mde_ui::checkbox_style),
        )
        .push(greyed("Lock the taskbar"))
        .push(
            text(
                "Lock the taskbar is labwc-managed. Left/right (vertical) location is a \
                 later milestone.",
            )
            .size(metrics::UI_PX)
            .color(palette::color(palette::GRAY_TEXT)),
        )
        .into()
}

/// Personalization ▸ Lock screen (E7.6): pick a Picture for the LightDM greeter
/// background. Apply writes it via pkexec. (Spotlight rotation / Slideshow don't
/// map to LightDM's single static greeter background, so only Picture ships.)
fn lock_page(state: &Settings) -> Element<'_, Message> {
    let sel = state
        .lock_selected
        .and_then(|i| state.bg_wallpapers.get(i))
        .map(String::as_str);
    let preview = container(wallpaper::preview::<Message>(sel))
        .width(Length::Fixed(320.0))
        .height(Length::Fixed(180.0))
        .style(|_| container::Style {
            border: Border {
                color: palette::color(palette::WINDOW_FRAME),
                width: 1.0,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        });
    let mut strip = Row::new().spacing(8.0);
    for (i, wp) in state.bg_wallpapers.iter().enumerate().take(24) {
        strip = strip.push(thumb(
            wp,
            state.lock_selected == Some(i),
            Message::LockSelect(i),
        ));
    }
    let greyed = |label: &'static str| {
        checkbox(label, true)
            .size(metrics::UI_PX)
            .text_size(metrics::UI_PX)
            .spacing(8.0)
            .style(mde_ui::checkbox_style)
    };
    Column::new()
        .spacing(12.0)
        .push(preview)
        .push(
            text("Background")
                .size(metrics::UI_PX)
                .color(palette::color(palette::WINDOW_TEXT)),
        )
        .push(scrollable(strip).style(mde_ui::scrollbar))
        .push(
            Row::new()
                .spacing(8.0)
                .push(
                    button(text("Browse").size(metrics::UI_PX))
                        .on_press(Message::LockBrowse)
                        .padding(Padding::from([4.0, 12.0]))
                        .style(tile_style),
                )
                .push(
                    button(text("Apply").size(metrics::UI_PX))
                        .on_press(Message::LockApply)
                        .padding(Padding::from([4.0, 16.0]))
                        .style(tile_style),
                ),
        )
        .push(greyed(
            "Show the lock screen background picture on the sign-in screen",
        ))
        .push(
            text(
                "Spotlight (rotating) and Slideshow aren't supported by the LightDM greeter \
                 (single static background); the sign-in toggle is greeter-managed. Apply writes \
                 the greeter background via pkexec (asks for your password).",
            )
            .size(metrics::UI_PX)
            .color(palette::color(palette::GRAY_TEXT)),
        )
        .into()
}

fn mode_button<'a>(label: &'a str, selected: bool, msg: Message) -> Element<'a, Message> {
    button(text(label).size(metrics::UI_PX))
        .on_press(msg)
        .padding(Padding::from([6.0, 18.0]))
        .style(move |_t, status| {
            let hot = selected || matches!(status, button::Status::Hovered);
            button::Style {
                background: Some(Background::Color(palette::color(if hot {
                    palette::HIGHLIGHT
                } else {
                    palette::MENU
                }))),
                text_color: if hot {
                    palette::color(palette::HIGHLIGHT_TEXT)
                } else {
                    palette::color(palette::MENU_TEXT)
                },
                border: Border {
                    color: if selected {
                        palette::accent()
                    } else {
                        palette::color(palette::WINDOW_FRAME)
                    },
                    width: if selected { 2.0 } else { 1.0 },
                    radius: 2.0.into(),
                },
                ..button::Style::default()
            }
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_history_extracts_transactions() {
        // dnf5's whitespace-aligned format: command + action hold spaces; split on
        // the YYYY-MM-DD HH:MM:SS date. Action(s) is often empty.
        let out = "\
ID Command line                 Date and time       Action(s) Altered
28 dnf install mc               2026-06-02 20:26:06                 1
27 dnf upgrade -y               2026-06-01 21:46:17 Upgrade         6
";
        let h = parse_history(out);
        assert_eq!(h.len(), 2, "header skipped, two rows parsed");
        assert_eq!(h[0].id, 28);
        assert_eq!(h[0].command, "dnf install mc");
        assert_eq!(h[0].date, "2026-06-02 20:26:06");
        assert_eq!(h[0].action, ""); // no Action(s) column value
        assert_eq!(h[0].altered, "1");
        assert_eq!(h[1].id, 27);
        assert_eq!(h[1].command, "dnf upgrade -y");
        assert_eq!(h[1].action, "Upgrade");
        assert_eq!(h[1].altered, "6");
        assert!(parse_history("").is_empty());
    }

    #[test]
    fn reboot_command_toggles_the_policy() {
        assert!(reboot_command(true).contains("reboot = when-needed"));
        assert!(reboot_command(false).contains("reboot = never"));
        assert!(reboot_command(true).contains("/etc/dnf/automatic.conf"));
    }

    #[test]
    fn active_hours_override_sets_oncalendar_and_reloads() {
        let s = active_hours_override(17);
        assert!(s.contains("OnCalendar=*-*-* 17:00:00"));
        assert!(s.contains("dnf-automatic.timer.d/override.conf"));
        assert!(s.contains("systemctl daemon-reload"));
        // Zero-padded hour.
        assert!(active_hours_override(8).contains("08:00:00"));
    }

    #[test]
    fn pause_steps_seven_days_capped_at_thirty_five() {
        const DAY: u64 = 86_400;
        let now = 1_000_000;
        // First pause → 7 days from now.
        assert_eq!(next_pause(0, now), now + 7 * DAY);
        // Already paused → step another 7 (14 total).
        assert_eq!(next_pause(now + 7 * DAY, now), now + 14 * DAY);
        // Capped at 35 days from now, never further.
        assert_eq!(next_pause(now + 35 * DAY, now), now + 35 * DAY);
        assert_eq!(next_pause(now + 30 * DAY, now), now + 35 * DAY);
    }

    #[test]
    fn live_pages_map_to_real_backends() {
        // Every non-deferred page resolves to a real backend: an mde subcommand,
        // a fedora::TOOLS command that exists, a shell command, or Colors.
        for cat in CATEGORIES {
            for p in cat.pages {
                if let Kind::Tool(cmd) = p.kind {
                    assert!(
                        tool_by_cmd(cmd).is_some(),
                        "{} has no fedora::TOOLS entry",
                        cmd
                    );
                }
            }
        }
    }

    #[test]
    fn home_has_eleven_categories_no_gaming_no_search() {
        assert_eq!(CATEGORIES.len(), 11);
        assert!(!CATEGORIES
            .iter()
            .any(|c| c.title == "Gaming" || c.title == "Search"));
    }
}
