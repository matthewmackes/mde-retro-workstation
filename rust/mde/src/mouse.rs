//! labwc libinput (mouse) config for Settings ▸ Devices ▸ Mouse (E12.6).
//!
//! Rewrites the `<libinput>` block of the user's `~/.config/labwc/rc.xml` from the
//! Mouse page's settings, preserving the rest of the file — crucially the
//! `<mouse><default/>` block (§7: dropping `<default/>` makes every window
//! unmovable and the titlebar buttons dead). After a rewrite it asks labwc to
//! reload (`labwc --reconfigure`), the same way `panel.rs` does after writing
//! `menu.xml`. iced-free.
//!
//! The `MDE_LABWC_RC` env var overrides the target path — a dry-run / test seam
//! (like `MDE_LOCK_CONF`) that also suppresses the live reconfigure, so a bench
//! can verify the rewrite against a temp file without touching the real session.
//! Headless entry: `mde __mouse-rc`.
//!
//! The rc.xml path resolution + atomic-write-and-reconfigure plumbing ([`rc_path`],
//! [`write_rc`]) is shared with the keyboard config ([`crate::keyboard`], E12.8).

use std::path::{Path, PathBuf};

/// Touchpad (`category="touchpad"`) settings for the optional second `<device>`
/// in the `<libinput>` block (E12.7). Only emitted when a touchpad is present.
#[derive(Debug, Clone, Copy)]
pub struct Touchpad {
    pub enabled: bool,      // sendEventsMode yes|no (On/Off)
    pub pointer_speed: f32, // -1.0..1.0
    pub tap: bool,          // tap-to-click
    pub two_finger: bool,   // scrollMethod twofinger|none
    pub natural_scroll: bool,
}

/// Map the Win10-style "lines to scroll" (1–10, default 3) onto libinput's
/// `scrollFactor` multiplier (3 lines ⇒ 1.0, the neutral default).
fn scroll_factor(lines: u8) -> f32 {
    (lines.clamp(1, 10) as f32) / 3.0
}

/// Map a touchpad sensitivity level (1–10, default 5) onto libinput's
/// `pointerSpeed` (-1.0..1.0); level 5 ⇒ 0.0 (the neutral default), E12.7.
pub fn pointer_speed(level: u8) -> f32 {
    ((level.clamp(1, 10) as f32) - 5.0) / 5.0
}

/// The `<libinput>` block: a `default` device from the mouse settings, plus a
/// `touchpad` device when one is present. Indented for the top level of `rc.xml`,
/// no trailing newline. The "scroll inactive windows" mouse preference is
/// deliberately absent — labwc has no such knob, so it stays a menu.json advisory
/// (E12.6). The labwc element vocabulary is per `man 5 labwc-config` (0.9.6).
pub fn libinput_block(
    left_handed: bool,
    natural_scroll: bool,
    scroll_lines: u8,
    touchpad: Option<&Touchpad>,
) -> String {
    let yn = |b: bool| if b { "yes" } else { "no" };
    let mut s = format!(
        "  <libinput>
    <device category=\"default\">
      <naturalScroll>{nat}</naturalScroll>
      <leftHanded>{lh}</leftHanded>
      <scrollFactor>{sf:.2}</scrollFactor>
    </device>",
        nat = yn(natural_scroll),
        lh = yn(left_handed),
        sf = scroll_factor(scroll_lines),
    );
    if let Some(t) = touchpad {
        s.push_str(&format!(
            "
    <device category=\"touchpad\">
      <sendEventsMode>{ev}</sendEventsMode>
      <pointerSpeed>{ps:.2}</pointerSpeed>
      <naturalScroll>{nat}</naturalScroll>
      <tap>{tap}</tap>
      <scrollMethod>{sm}</scrollMethod>
    </device>",
            ev = yn(t.enabled),
            ps = t.pointer_speed.clamp(-1.0, 1.0),
            nat = yn(t.natural_scroll),
            tap = yn(t.tap),
            sm = if t.two_finger { "twofinger" } else { "none" },
        ));
    }
    s.push_str("\n  </libinput>");
    s
}

/// Whether a touchpad is attached. `MDE_TOUCHPAD` (1/0) forces the answer (a test
/// seam); otherwise scan `/proc/bus/input/devices` for a touch/track-pad (E12.7).
pub fn has_touchpad() -> bool {
    match std::env::var("MDE_TOUCHPAD").ok().as_deref() {
        Some("1") => return true,
        Some("0") => return false,
        _ => {}
    }
    std::fs::read_to_string("/proc/bus/input/devices")
        .map(|s| {
            s.lines().any(|l| {
                let l = l.to_lowercase();
                l.starts_with("n: name=") && (l.contains("touchpad") || l.contains("trackpad"))
            })
        })
        .unwrap_or(false)
}

/// Swap `<libinput>…</libinput>` in `xml` for `block`, preserving everything else
/// (including `<mouse><default/>`). When no `<libinput>` exists, insert `block`
/// just before `</labwc_config>`. Pure — unit-tested for swap/insert/idempotence.
pub fn rewrite_libinput(xml: &str, block: &str) -> String {
    let block = block.trim_matches('\n');
    match (xml.find("<libinput"), xml.find("</libinput>")) {
        (Some(start), Some(end)) => {
            let end = end + "</libinput>".len();
            // Back up to the start of the `<libinput` line so we replace its
            // indentation too, then keep whatever followed `</libinput>` verbatim.
            let line_start = xml[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
            format!("{}{}{}", &xml[..line_start], block, &xml[end..])
        }
        _ => match xml.rfind("</labwc_config>") {
            Some(pos) => format!("{}{}\n{}", &xml[..pos], block, &xml[pos..]),
            None => format!("{xml}\n{block}\n"),
        },
    }
}

/// The rc.xml path: `MDE_LABWC_RC` if set (test seam), else
/// `$XDG_CONFIG_HOME/labwc/rc.xml` (honouring `HOME` otherwise). Shared with
/// [`crate::keyboard`].
pub(crate) fn rc_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("MDE_LABWC_RC") {
        return Some(PathBuf::from(p));
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("labwc/rc.xml"))
}

/// Atomically write `content` to `path` (temp sibling + rename), then ask labwc to
/// reload — unless `MDE_LABWC_RC` is set (a test, no live labwc to signal). Shared
/// by the libinput (mouse/touchpad) and keyboard rewriters.
pub(crate) fn write_rc(path: &Path, content: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("xml.mde-tmp");
    std::fs::write(&tmp, content.as_bytes())?;
    std::fs::rename(&tmp, path)?;
    if std::env::var_os("MDE_LABWC_RC").is_none() {
        let _ = std::process::Command::new("labwc")
            .arg("--reconfigure")
            .status();
    }
    Ok(())
}

/// Write the mouse settings into rc.xml (atomic temp+rename), then reload labwc.
pub fn apply(
    left_handed: bool,
    natural_scroll: bool,
    scroll_lines: u8,
    touchpad: Option<&Touchpad>,
) -> std::io::Result<()> {
    let Some(path) = rc_path() else {
        return Ok(());
    };
    let xml = std::fs::read_to_string(&path)?;
    let block = libinput_block(left_handed, natural_scroll, scroll_lines, touchpad);
    let out = rewrite_libinput(&xml, &block);
    write_rc(&path, &out)
}

/// Headless exercise for `mde __mouse-rc`: apply the persisted mouse settings to
/// the rc.xml (honouring `MDE_LABWC_RC`) and print the result, so the rewrite can
/// be checked end-to-end without a live session.
pub fn debug_apply() {
    let st = crate::state::load();
    let tp = has_touchpad().then(|| Touchpad {
        enabled: st.touchpad_enabled,
        pointer_speed: pointer_speed(st.touchpad_speed),
        tap: st.touchpad_tap,
        two_finger: st.touchpad_two_finger,
        natural_scroll: st.touchpad_natural_scroll,
    });
    if let Err(e) = apply(
        st.mouse_left_handed,
        st.mouse_natural_scroll,
        st.mouse_scroll_lines,
        tp.as_ref(),
    ) {
        eprintln!("mde __mouse-rc: {e}");
        return;
    }
    if let Some(p) = rc_path() {
        if let Ok(s) = std::fs::read_to_string(&p) {
            print!("{s}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
<?xml version=\"1.0\"?>
<labwc_config>
  <libinput>
    <device category=\"default\">
      <naturalScroll>no</naturalScroll>
      <leftHanded>no</leftHanded>
      <scrollFactor>1.00</scrollFactor>
    </device>
  </libinput>
  <keyboard>
    <keybind key=\"W-l\"><action name=\"Execute\"><command>mde lock</command></action></keybind>
  </keyboard>
  <mouse>
    <default/>
    <context name=\"Root\">
      <mousebind button=\"Right\" action=\"Press\"><action name=\"ShowMenu\"><menu>root-menu</menu></action></mousebind>
    </context>
  </mouse>
</labwc_config>
";

    #[test]
    fn scroll_factor_maps() {
        assert!((scroll_factor(3) - 1.0).abs() < 1e-6);
        assert!((scroll_factor(6) - 2.0).abs() < 1e-6);
        assert!((scroll_factor(0) - scroll_factor(1)).abs() < 1e-6); // clamped
        assert!((scroll_factor(99) - scroll_factor(10)).abs() < 1e-6);
    }

    #[test]
    fn pointer_speed_maps() {
        assert!((pointer_speed(5) - 0.0).abs() < 1e-6);
        assert!((pointer_speed(10) - 1.0).abs() < 1e-6);
        assert!((pointer_speed(1) - (-0.8)).abs() < 1e-6);
        assert!((pointer_speed(0) - pointer_speed(1)).abs() < 1e-6); // clamped
    }

    #[test]
    fn block_omits_the_advisory_and_touchpad_when_none() {
        let b = libinput_block(true, true, 6, None);
        assert!(b.contains("<leftHanded>yes</leftHanded>"));
        assert!(b.contains("<naturalScroll>yes</naturalScroll>"));
        assert!(b.contains("<scrollFactor>2.00</scrollFactor>"));
        // No touchpad present ⇒ only the default device.
        assert!(!b.contains("category=\"touchpad\""));
        // The "scroll inactive windows" advisory must never reach rc.xml.
        assert!(!b.to_lowercase().contains("inactive"));
    }

    #[test]
    fn block_emits_touchpad_device_when_present() {
        let tp = Touchpad {
            enabled: false,
            pointer_speed: pointer_speed(10),
            tap: false,
            two_finger: false,
            natural_scroll: true,
        };
        let b = libinput_block(false, false, 3, Some(&tp));
        // Both device profiles, in order.
        assert!(b.contains("category=\"default\""));
        assert!(b.contains("category=\"touchpad\""));
        assert!(b.find("category=\"default\"").unwrap() < b.find("category=\"touchpad\"").unwrap());
        // On/Off off ⇒ sendEventsMode no; two-finger off ⇒ scrollMethod none.
        assert!(b.contains("<sendEventsMode>no</sendEventsMode>"));
        assert!(b.contains("<scrollMethod>none</scrollMethod>"));
        assert!(b.contains("<pointerSpeed>1.00</pointerSpeed>"));
        assert!(b.contains("<tap>no</tap>"));
        // Exactly one libinput wrapper around the two devices.
        assert_eq!(b.matches("<libinput>").count(), 1);
    }

    #[test]
    fn rewrite_swaps_block_and_keeps_mouse_default() {
        let block = libinput_block(true, false, 3, None);
        let out = rewrite_libinput(SAMPLE, &block);
        // New value in, old value gone.
        assert!(out.contains("<leftHanded>yes</leftHanded>"));
        assert!(!out.contains("<leftHanded>no</leftHanded>"));
        // Exactly one libinput block (no duplication).
        assert_eq!(out.matches("<libinput>").count(), 1);
        // The load-bearing bits survive untouched (§7).
        assert!(out.contains("<mouse>"));
        assert!(out.contains("<default/>"));
        assert!(out.contains("root-menu"));
        assert!(out.contains("mde lock"));
    }

    #[test]
    fn rewrite_inserts_when_absent_keeping_mouse() {
        let no_li = "<labwc_config>\n  <mouse>\n    <default/>\n  </mouse>\n</labwc_config>\n";
        let block = libinput_block(false, true, 5, None);
        let out = rewrite_libinput(no_li, &block);
        assert_eq!(out.matches("<libinput>").count(), 1);
        assert!(out.contains("<naturalScroll>yes</naturalScroll>"));
        assert!(out.contains("<default/>"));
        // Inserted before the closing tag.
        assert!(out.find("<libinput>").unwrap() < out.find("</labwc_config>").unwrap());
    }

    #[test]
    fn rewrite_is_idempotent_with_touchpad() {
        let tp = Touchpad {
            enabled: true,
            pointer_speed: 0.2,
            tap: true,
            two_finger: true,
            natural_scroll: false,
        };
        let block = libinput_block(true, true, 7, Some(&tp));
        let once = rewrite_libinput(SAMPLE, &block);
        let twice = rewrite_libinput(&once, &block);
        assert_eq!(once, twice);
        assert_eq!(twice.matches("category=\"touchpad\"").count(), 1);
    }
}
