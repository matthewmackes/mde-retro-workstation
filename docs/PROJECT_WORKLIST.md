# MDE-Retro — Project Worklist

The **single** durable tracker (see `.claude/CLAUDE.md` §5). In-session Task tools
are a scratchpad; this file wins on any divergence. `rust/SPEC-*.md` are design
docs, not a parallel worklist — lift actionable items into here.

**Legend (locked):** `[ ]` Open · `[>]` In Progress · `[✓]` Done · `[!]` Blocked.
No `[~]` deferred, no `[x]`. Flip to `[>]` before substantive edits; `[✓]` only when
the §3 Definition of Done holds (reachable + observably works + verified).

---

## Compliance (from the first `/audit` sweep — see `docs/COMPLIANCE.md`)

Fixed in the reorg pass (✓ below = done & verified): doc drift across all 6 docs;
removed 5 dead surfaces (flag widget + LOGO_*, progress_style, use_beos,
frame::pressed, _gauge); routed 3 raw-hex leaks through palette
(SHELL_HEADER/GRAY_TEXT/BACKGROUND); pinned 3 ground-truth constants
(WINDOW_FRAME, TITLE_TEXT, INFO_TITLE_PX); added the 3 missing `%post` symlinks.

Remaining (FINISH unless noted):

- [✓] **§2.1** Carbon icon-accent table moved to `palette::icon_accent()`;
  `icons.rs` calls it. (No raw hex at the icon site.)
- [✓] **§2.3** `.size()` literals replaced with named `metrics` constants
  (`IDENTIFY_PX`, `WIZARD_HEADING_PX`, `WIZARD_STATUS_PX`) at display.rs/installer.rs.
- [✓] **§2.2** `TASKBAR_BUTTON_MIN`, `IDENTIFY_PX`, `WIZARD_*` pinned in `checklist.rs`.
- [✓] **§3 mockup** `display.rs` Effects tab — greyed (no on_toggle); dead
  `fx_*` fields/messages/handlers removed.
- [✓] **§3 mockup** `taskbar_properties.rs` "Show clock" + "Use Personalized Menus"
  — greyed; dead `show_clock`/`personalized` state + messages removed.
- [✓] **§4 packaging** Add `assets/licenses/DroidSans-Apache-2.0.txt` to the asset
  list + a Droid Sans entry to `NOTICE.md` (the font is embedded in the shipped
  binary; IBM Plex — embedded the same way — is already covered).
- [✓] **§3 decision** `wlr.rs` `Wm::close`/`Wm::set_maximized` REMOVED (per-window
  close/maximize is labwc's by the compositor boundary, so the taskbar never calls
  them); `outputs.rs` `Output::make` FINISHED — folded into `Output::label()` so the
  Display dropdown shows the full EDID identity ("Dell U2419H (DP-1)"), unit-pinned.
- [✓] **§1 docs** Deeper prose pass on `PREVIEW.md`/`ACCURACY.md`: fixed the live
  boundary (mde↔labwc, themerc-sourced titlebar colors; "live sway IPC" →
  wlr-foreign-toplevel), retired stale "gaps" now shipped (tray, taskbar/Start
  popups, checkbox/radio/tab/group-box widgets), and reframed the session cutover
  as labwc/done. Legitimate harness `sway` references (headless wlroots driver) kept.

## Backlog

- [ ] **Mobile Devices (native KDE Connect)** — 15-Q spec in [[mde-kdeconnect]].
  Shared crate `matthewmackes/MDE-KDECnt-Rust` (public) stands up the protocol core
  (✓ Phase 1: extracted + 181 tests). Remaining: [ ] generalize the host (Transport
  trait + event stream) + complete the LAN transport (UDP 1716 + rustls) in the
  shared crate; [ ] rewire MDE + MDE-Retro to depend on it; [ ] `mde connect`
  systemd user daemon + pairing modal/tray; [ ] capability surfaces (notifications
  bidirectional + a freedesktop notify daemon, clipboard, battery, file transfer via
  Explorer "Send to", MPRIS, run-commands, SMS); [ ] "Mobile Devices" Control Panel
  applet. LAN-only; remote-input deferred. Config at ~/.config/mde/connect/.
- [ ] Carbon polish (from the theme survey): primary-blue / ghost button variants
  (current Carbon buttons are flat secondary); explicit accent-tinted labwc
  titlebar buttons; popup.rs context menus still bottom-anchored under the top bar.
