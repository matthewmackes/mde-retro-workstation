//! Wayland display state for the Display Properties applet.
//!
//! The "display manager" data layer: query the compositor's outputs
//! (`wlr-randr --json`), and apply / persist resolution, refresh rate,
//! orientation, scale, position, and wallpaper. Live changes go through
//! `wlr-randr` (and `swaybg` for wallpaper); persistence is a generated
//! `~/.config/mde/display.sh` the labwc autostart replays at login.
//!
//! Toolkit-agnostic and mostly pure, so the parsing/formatting is unit-tested
//! without a compositor (see the tests at the bottom). The GUI lives in
//! `display.rs`; this module never touches iced.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

// --- the model -------------------------------------------------------------

/// One video mode a monitor supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mode {
    pub width: i32,
    pub height: i32,
    /// Refresh in mHz, as sway reports it (60_000 = 60.000 Hz).
    pub refresh_mhz: i32,
}

impl Mode {
    /// Hz as a display string, e.g. "60 Hz" (drops a trailing ".000").
    pub fn refresh_label(&self) -> String {
        let hz = self.refresh_mhz as f64 / 1000.0;
        if (hz.round() - hz).abs() < 0.005 {
            format!("{} Hz", hz.round() as i64)
        } else {
            format!("{hz:.3} Hz")
        }
    }

    /// The `--mode` argument wlr-randr expects, e.g. "1920x1080@60.000Hz".
    pub fn mode_arg(&self) -> String {
        format!("{}x{}@{:.3}Hz", self.width, self.height, self.refresh_mhz as f64 / 1000.0)
    }

    /// "1920 x 1080" — the resolution without refresh, for the area slider.
    pub fn res_label(&self) -> String {
        format!("{} x {}", self.width, self.height)
    }
}

/// A connected output (monitor) and its current + available state.
#[derive(Debug, Clone)]
pub struct Output {
    pub name: String,
    /// Manufacturer string from EDID (transcribed; the label prefers model+name).
    #[allow(dead_code)]
    pub make: String,
    pub model: String,
    /// Whether the output is currently enabled.
    pub active: bool,
    /// Whether it currently has input focus (the "primary" proxy).
    pub focused: bool,
    pub scale: f64,
    /// sway transform: "normal" | "90" | "180" | "270" (+ flipped variants).
    pub transform: String,
    /// Logical position of the output's top-left in the layout.
    pub x: i32,
    pub y: i32,
    /// Current logical size (after scale/transform).
    pub rect_w: i32,
    pub rect_h: i32,
    pub current: Option<Mode>,
    pub modes: Vec<Mode>,
}

impl Output {
    pub fn label(&self) -> String {
        let title = if self.model.is_empty() {
            self.name.clone()
        } else {
            format!("{} ({})", self.model, self.name)
        };
        title
    }

    /// Distinct resolutions (W×H), highest-area first — the "Screen area" stops.
    pub fn resolutions(&self) -> Vec<(i32, i32)> {
        let mut res: Vec<(i32, i32)> = self.modes.iter().map(|m| (m.width, m.height)).collect();
        res.sort_unstable_by_key(|&(w, h)| std::cmp::Reverse(w as i64 * h as i64));
        res.dedup();
        res
    }

    /// Refresh rates available at a given resolution, highest first.
    pub fn refreshes_at(&self, w: i32, h: i32) -> Vec<Mode> {
        let mut ms: Vec<Mode> =
            self.modes.iter().copied().filter(|m| m.width == w && m.height == h).collect();
        ms.sort_unstable_by_key(|m| std::cmp::Reverse(m.refresh_mhz));
        ms.dedup_by_key(|m| m.refresh_mhz);
        ms
    }
}

// --- wlr-randr --json parsing ----------------------------------------------
// labwc (and any wlroots compositor) is queried via `wlr-randr --json`, whose
// schema differs from sway's get_outputs: `enabled` (not active), refresh in Hz
// (not mHz), the current mode flagged `current`, and a `position` object.

#[derive(Debug, Deserialize)]
struct RawMode {
    width: i32,
    height: i32,
    #[serde(default)]
    refresh: f64, // Hz
    #[serde(default)]
    current: bool,
}

#[derive(Debug, Deserialize)]
struct RawPos {
    #[serde(default)]
    x: i32,
    #[serde(default)]
    y: i32,
}

#[derive(Debug, Deserialize)]
struct RawOutput {
    name: String,
    #[serde(default)]
    make: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    enabled: bool,
    #[serde(default = "one")]
    scale: f64,
    #[serde(default)]
    transform: Option<String>,
    #[serde(default)]
    position: Option<RawPos>,
    #[serde(default)]
    modes: Vec<RawMode>,
}

fn one() -> f64 {
    1.0
}

fn to_mode(m: &RawMode) -> Mode {
    Mode { width: m.width, height: m.height, refresh_mhz: (m.refresh * 1000.0).round() as i32 }
}

fn convert(raw: RawOutput, primary: bool) -> Output {
    let pos = raw.position.unwrap_or(RawPos { x: 0, y: 0 });
    let current = raw.modes.iter().find(|m| m.current).map(to_mode);
    let (rect_w, rect_h) = current.map(|m| (m.width, m.height)).unwrap_or((0, 0));
    Output {
        name: raw.name,
        make: raw.make,
        model: raw.model,
        active: raw.enabled,
        // wlr-randr reports no "focused"; the first output is treated as primary.
        focused: primary,
        scale: raw.scale,
        transform: raw.transform.unwrap_or_else(|| "normal".to_string()),
        x: pos.x,
        y: pos.y,
        rect_w,
        rect_h,
        current,
        modes: raw.modes.iter().map(to_mode).collect(),
    }
}

/// Parse the JSON array `wlr-randr --json` produces.
pub fn parse(json: &str) -> Vec<Output> {
    serde_json::from_str::<Vec<RawOutput>>(json)
        .map(|v| v.into_iter().enumerate().map(|(i, o)| convert(o, i == 0)).collect())
        .unwrap_or_default()
}

/// Query the compositor for the live output list via wlr-randr (empty on any
/// failure — the GUI shows a "no displays detected" state rather than crashing).
pub fn query() -> Vec<Output> {
    Command::new("wlr-randr")
        .arg("--json")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| parse(&s))
        .unwrap_or_default()
}

// --- desired state (what the dialog edits) ---------------------------------

/// The full set of display settings the applet edits and persists. Built from
/// the live outputs, mutated by the tabs, then applied/persisted as a unit.
#[derive(Debug, Clone, Default)]
pub struct Desired {
    pub outputs: Vec<DesiredOutput>,
    pub wallpaper: Option<Wallpaper>,
    pub screensaver: Option<ScreenSaver>,
    /// Appearance scheme key (e.g. "win2k-standard"); None ⇒ leave colors alone.
    pub scheme: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DesiredOutput {
    pub name: String,
    pub width: i32,
    pub height: i32,
    pub refresh_mhz: i32,
    pub scale: f64,
    pub transform: String,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone)]
pub struct Wallpaper {
    pub path: String,
    /// swaybg mode: center | tile | stretch | fit | fill.
    pub mode: String,
}

#[derive(Debug, Clone)]
pub struct ScreenSaver {
    /// Idle minutes before the saver/lock arms (0 ⇒ disabled).
    pub minutes: u32,
    /// Lock (swaylock) on resume.
    pub lock: bool,
}

impl DesiredOutput {
    fn from(o: &Output) -> Self {
        let cur = o.current.unwrap_or(Mode { width: o.rect_w, height: o.rect_h, refresh_mhz: 60_000 });
        DesiredOutput {
            name: o.name.clone(),
            width: cur.width,
            height: cur.height,
            refresh_mhz: cur.refresh_mhz,
            scale: o.scale,
            transform: o.transform.clone(),
            x: o.x,
            y: o.y,
        }
    }
}

/// Seed a `Desired` from the live outputs.
pub fn desired_from(outputs: &[Output]) -> Vec<DesiredOutput> {
    outputs.iter().map(DesiredOutput::from).collect()
}

// --- applying (live, via wlr-randr / swaybg) -------------------------------

/// The `wlr-randr` arguments that realise one output's desired state.
pub fn output_args(d: &DesiredOutput) -> Vec<String> {
    let m = Mode { width: d.width, height: d.height, refresh_mhz: d.refresh_mhz };
    vec![
        "--output".into(),
        d.name.clone(),
        "--mode".into(),
        m.mode_arg(),
        "--transform".into(),
        d.transform.clone(),
        "--scale".into(),
        fmt_scale(d.scale),
        "--pos".into(),
        format!("{},{}", d.x, d.y),
    ]
}

fn fmt_scale(s: f64) -> String {
    if (s.round() - s).abs() < 0.001 {
        format!("{}", s.round() as i64)
    } else {
        format!("{s:.2}")
    }
}

/// Run wlr-randr with the given args (returns true on success).
fn run_wlr(args: &[String]) -> bool {
    Command::new("wlr-randr").args(args).status().map(|s| s.success()).unwrap_or(false)
}

/// (Re)launch swaybg for the current wallpaper.
fn apply_wallpaper(w: &Wallpaper) {
    let _ = Command::new("pkill").args(["-x", "swaybg"]).status();
    let _ = Command::new("swaybg").args(["-i", &w.path, "-m", &w.mode]).spawn();
}

/// Apply the whole desired state live (no persistence). Best-effort: each
/// output is independent so a single bad mode doesn't abort the rest.
pub fn apply_live(d: &Desired) {
    for o in &d.outputs {
        let _ = run_wlr(&output_args(o));
    }
    if let Some(w) = &d.wallpaper {
        apply_wallpaper(w);
    }
    if let Some(s) = &d.scheme {
        apply_scheme(s);
    }
}

/// Re-apply a previously-captured live state (the 15-second revert path).
pub fn revert_to(prev: &Desired) {
    for o in &prev.outputs {
        let _ = run_wlr(&output_args(o));
    }
}

/// Ask labwc to re-read its config + theme.
pub fn reconfigure() {
    let _ = Command::new("labwc").arg("--reconfigure").status();
}

// --- appearance schemes ----------------------------------------------------

/// (focused border, focused bg, focused text) for a scheme key — the sway
/// `client.focused` triple plus our shell window background.
pub fn scheme_colors(key: &str) -> (&'static str, &'static str, &'static str) {
    match key {
        // Windows Standard: navy active caption, silver face.
        "win2k-standard" => ("#0a246a", "#0a246a", "#ffffff"),
        // High Contrast Black.
        "high-contrast-black" => ("#000000", "#000000", "#ffffff"),
        // Brick / Desert-ish warm scheme.
        "win2k-brick" => ("#6b3f2b", "#6b3f2b", "#ffffff"),
        // Spruce green.
        "win2k-spruce" => ("#2b5b3f", "#2b5b3f", "#ffffff"),
        _ => ("#0a246a", "#0a246a", "#ffffff"),
    }
}

/// Apply an Appearance scheme by rewriting the Win2000-MDE Openbox theme's
/// active-caption colours and asking labwc to reconfigure (labwc takes window
/// colours from the theme, not a live IPC, so this is how a scheme lands).
pub fn apply_scheme(key: &str) {
    let (_border, bg, text) = scheme_colors(key);
    if let Some(p) = theme_path() {
        if let Ok(content) = std::fs::read_to_string(&p) {
            let new: String = content
                .lines()
                .map(|l| {
                    let t = l.trim_start();
                    if t.starts_with("window.active.title.bg.color:") {
                        format!("window.active.title.bg.color: {bg}")
                    } else if t.starts_with("window.active.label.text.color:") {
                        format!("window.active.label.text.color: {text}")
                    } else {
                        l.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            let _ = std::fs::write(&p, format!("{new}\n"));
        }
    }
    reconfigure();
}

fn theme_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(base.join("themes/Win2000-MDE/openbox-3/themerc"))
}

// --- persistence (labwc autostart-sourced scripts) -------------------------

/// `~/.config/mde/` — where the generated session scripts live (sourced by the
/// labwc autostart).
fn mde_config_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("mde"))
}

/// The generated `display.sh` (the output geometry, replayed at login by the
/// labwc autostart). Public for tests.
pub fn persist_text(d: &Desired) -> String {
    let mut s = String::from("#!/bin/sh\n# Generated by `mde display` — output geometry, replayed at login.\n");
    for o in &d.outputs {
        s.push_str(&format!("wlr-randr {}\n", output_args(o).join(" ")));
    }
    s
}

/// The generated `wallpaper.sh`.
fn wallpaper_text(w: &Wallpaper) -> String {
    format!(
        "#!/bin/sh\npkill -x swaybg\nswaybg -i {} -m {} &\n",
        shell_quote(&w.path),
        w.mode
    )
}

/// The generated `idle.sh` (screen saver via swayidle + wlopm/swaylock).
fn idle_text(sv: &ScreenSaver) -> String {
    if sv.minutes == 0 {
        return "#!/bin/sh\n# screen saver disabled\npkill swayidle\n".to_string();
    }
    let secs = sv.minutes * 60;
    let action = if sv.lock {
        "swaylock -f -c 000000".to_string()
    } else {
        "wlopm --off '*'".to_string()
    };
    format!(
        "#!/bin/sh\npkill swayidle\nswayidle -w timeout {secs} '{action}' resume 'wlopm --on \"*\"' &\n"
    )
}

/// Write the generated session scripts and (re)apply them live. labwc's
/// autostart sources these at next login; we also apply now via [`apply_live`].
pub fn persist(d: &Desired) -> std::io::Result<()> {
    let Some(dir) = mde_config_dir() else {
        return Ok(());
    };
    std::fs::create_dir_all(&dir)?;
    write_script(&dir.join("display.sh"), &persist_text(d))?;
    if let Some(w) = &d.wallpaper {
        write_script(&dir.join("wallpaper.sh"), &wallpaper_text(w))?;
    }
    if let Some(sv) = &d.screensaver {
        write_script(&dir.join("idle.sh"), &idle_text(sv))?;
    }
    if let Some(scheme) = &d.scheme {
        apply_scheme(scheme);
    }
    Ok(())
}

/// Write an executable script atomically.
fn write_script(path: &Path, body: &str) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, body)?;
    let mut perms = std::fs::metadata(&tmp)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&tmp, perms)?;
    std::fs::rename(&tmp, path)
}

/// Minimal shell-quote for a path embedded in a generated script.
fn shell_quote(s: &str) -> String {
    if s.is_empty() || s.contains(|c: char| c.is_whitespace() || c == '\'') {
        format!("\"{}\"", s.replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

// --- dependency probing ----------------------------------------------------

/// Whether a backend binary is on `$PATH`.
pub fn have(bin: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {bin}"))
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// The packages backing the optional features, for install-if-missing.
pub const BACKENDS: &[(&str, &str)] = &[
    ("wlr-randr", "wlr-randr"),
    ("swaybg", "swaybg"),
    ("swayidle", "swayidle"),
    ("swaylock", "swaylock"),
    ("wlopm", "wlopm"),
];

/// Packages whose binary is missing, for a single `pkexec dnf install`.
pub fn missing_backends() -> Vec<&'static str> {
    BACKENDS.iter().filter(|(bin, _)| !have(bin)).map(|(_, pkg)| *pkg).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // wlr-randr --json shape: refresh in Hz, current-mode flag, position object.
    const SAMPLE: &str = r#"[
      {"name":"DP-1","make":"Dell","model":"U2419H","enabled":true,
       "scale":1.0,"transform":"normal","position":{"x":0,"y":0},
       "modes":[{"width":1920,"height":1080,"refresh":60.000,"current":true},
                {"width":1920,"height":1080,"refresh":59.951,"current":false},
                {"width":1280,"height":720,"refresh":60.000,"current":false}]},
      {"name":"HDMI-A-1","make":"LG","model":"","enabled":false,
       "scale":2.0,"transform":"90","position":{"x":1920,"y":0},
       "modes":[]}
    ]"#;

    #[test]
    fn parses_outputs() {
        let outs = parse(SAMPLE);
        assert_eq!(outs.len(), 2);
        assert_eq!(outs[0].name, "DP-1");
        assert!(outs[0].focused); // first output = primary proxy
        assert_eq!(outs[0].current.unwrap().width, 1920);
        assert_eq!(outs[1].scale, 2.0);
        assert_eq!(outs[1].transform, "90");
        assert!(!outs[1].active);
    }

    #[test]
    fn resolutions_are_sorted_and_deduped() {
        let outs = parse(SAMPLE);
        let res = outs[0].resolutions();
        assert_eq!(res, vec![(1920, 1080), (1280, 720)]);
    }

    #[test]
    fn refreshes_at_resolution_dedup_by_rate() {
        let outs = parse(SAMPLE);
        let r = outs[0].refreshes_at(1920, 1080);
        assert_eq!(r.len(), 2); // 60.000 and 59.951
        assert_eq!(r[0].refresh_mhz, 60000); // highest first
    }

    #[test]
    fn mode_tokens_and_labels() {
        let m = Mode { width: 1920, height: 1080, refresh_mhz: 60000 };
        assert_eq!(m.mode_arg(), "1920x1080@60.000Hz");
        assert_eq!(m.refresh_label(), "60 Hz");
        assert_eq!(m.res_label(), "1920 x 1080");
        let odd = Mode { width: 2560, height: 1440, refresh_mhz: 59951 };
        assert_eq!(odd.refresh_label(), "59.951 Hz");
    }

    #[test]
    fn output_args_cover_all_axes() {
        let d = DesiredOutput {
            name: "DP-1".into(),
            width: 1920,
            height: 1080,
            refresh_mhz: 60000,
            scale: 1.5,
            transform: "90".into(),
            x: 100,
            y: 0,
        };
        let a = output_args(&d).join(" ");
        assert_eq!(a, "--output DP-1 --mode 1920x1080@60.000Hz --transform 90 --scale 1.50 --pos 100,0");
    }

    #[test]
    fn persist_text_is_an_executable_wlr_randr_script() {
        let d = Desired {
            outputs: vec![DesiredOutput {
                name: "DP-1".into(),
                width: 1280,
                height: 720,
                refresh_mhz: 60000,
                scale: 1.0,
                transform: "normal".into(),
                x: 0,
                y: 0,
            }],
            wallpaper: None,
            screensaver: None,
            scheme: None,
        };
        let text = persist_text(&d);
        assert!(text.starts_with("#!/bin/sh"));
        assert!(text.contains("wlr-randr --output DP-1 --mode 1280x720@60.000Hz --transform normal --scale 1 --pos 0,0"));
    }

    #[test]
    fn idle_script_uses_wlopm_or_swaylock() {
        let off = idle_text(&ScreenSaver { minutes: 10, lock: false });
        assert!(off.contains("swayidle -w timeout 600"));
        assert!(off.contains("wlopm --off"));
        let lock = idle_text(&ScreenSaver { minutes: 5, lock: true });
        assert!(lock.contains("swayidle -w timeout 300"));
        assert!(lock.contains("swaylock"));
    }

    #[test]
    fn shell_quote_wraps_spaces() {
        assert_eq!(shell_quote("/a/b.png"), "/a/b.png");
        assert_eq!(shell_quote("/a b/c.png"), "\"/a b/c.png\"");
    }
}
