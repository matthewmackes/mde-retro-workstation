# MDE-Retro 2.0 — "Windows 10 era" — Roadmap (draft)

> **Status: materialized.** The Ultraplan refinement is done — a 43-agent pass
> distilled the Windows 10 Field Guide (489pp) into the two deliverables it named:
> **`rust/SPEC-windows10.md`** (the 21-epic design SPEC, 78 coherence fixes against
> the 15 decisions) and the **`## 2.0 — Windows 10 era`** section of
> `docs/PROJECT_WORKLIST.md` (200 milestone-tiered stories). Those two are now the
> authority for 2.0 work; this file is the originating survey/decision record.

## Context

Incorporate the **Windows 10 Field Guide** (Thurrott/Rivera; 725-file `.odg`, ~540k
words — full ToC + per-area requirements extracted) into MDE-Retro's next interface
as a **2.0 release**. MDE-Retro today is a Rust/iced shell on labwc with three eras
(Win2000 / Carbon / BeOS); 2.0 adds a **Windows 10 era** plus the modern shell
surfaces it implies.

- **Out of scope** (per the request): Maps, Music/Groove, Store, Mail, Skype,
  Calendar, Photos/Video Editor, Movies & TV, Xbox/Games — and, from the survey,
  **OneNote, People** (Q12) and **all touch / tablet mode / Windows Ink** (Q9).
- **In scope:** the Win10 **shell + system experience** (everything else in the guide).

## Decisions (15-question survey)

| # | Decision |
|---|---|
| 1 | Win10 is a **new 4th era/theme + its shell features** (additive; Carbon stays default, Win10 opt-in) |
| 2 | Look = **Win10-flavored Carbon variant** (reuse flat Carbon widgets; shift palette/accent) |
| 3 | P0 pillars = **Action Center, Task View/virtual-desktops/Snap, Settings + Quick Access** (Search/Start promoted via Q6) |
| 4 | Modern **Settings app replaces the Control Panel in the Win10 era** (one config surface per era) |
| 5 | **Unify**: KDE Connect "Mobile Devices" *is* Win10 "Your Phone" |
| 6 | **Full tiled Start** in P0 (left rail + tiles + all-apps) |
| 7 | **Full Action Center**: freedesktop notify daemon + toasts + history + quick toggles |
| 8 | Multitasking **full, with graceful fallback** (Task View + virtual desktops + Snap) |
| 9 | **No touch/tablet/Ink** at all |
| 10 | "Cloud" = **your KDE Connect-paired devices** (remote-filesystem browse/sync), not a 3rd-party cloud |
| 11 | **Full Win10-style OOBE** (region → network → account → privacy → personalize → desktop) |
| 12 | **Edge → Firefox integration**; OneNote/People out |
| 13 | **Win10 lock + sign-in screen + Accounts settings** (extends LightDM-gtk-greeter; Hello = future) |
| 14 | **SPEC doc + milestone-tiered epics** in `docs/PROJECT_WORKLIST.md` |
| 15 | **2.0 = everything in-scope built** (milestones M1–M3 are internal sequencing) |

## Architecture approach (reuses existing patterns)

- **Win10 era as a 4th theme** — add `Theme::Windows10` to `mde-ui/src/palette.rs`
  and a `win10(rgb)` remap at the single `color()` edge (modeled on `carbon()`): a
  Win10-flavored Carbon variant (accent system + light/dark). Surface in `display.rs`
  Appearance; select at startup in `main.rs` from `state.rs`. **Zero call-site churn.**
- **New surfaces are drop-in `mde <subcommand>` layer-shell modules** following
  `popup.rs`/`menu.rs`: a `main.rs` match arm + `%post` symlink (CLAUDE.md §4), a
  module under `mde/src/`, a labwc `rc.xml` keybind. No compositor changes.
- **Per-era surface routing** off `palette::theme()` (Win10 → tiled Start + Settings
  app + top/auto-hide taskbar; Win2000/Carbon keep Start + Control Panel).
- **Risks:** (1) labwc must expose virtual desktops to a client (ext-workspace) —
  Task View degrades to window-switch + Snap otherwise. (2) Global hotkeys
  (Win+S/X/V/A, Win+arrows) are `rc.xml` binds — keep the `<mouse><default/>` rule
  (CLAUDE.md §7). (3) The notification daemon is shared by Action Center **and** KDE
  Connect — build once. (4) "Cloud = devices" depends on KDE Connect's
  remote-filesystem (sftp) capability in `MDE-KDECnt-Rust`.

## Deliverables (on approval)

1. **`rust/SPEC-windows10.md`** — per-area design/requirements distilled from the guide.
2. **`docs/PROJECT_WORKLIST.md` → "## 2.0 — Windows 10 era"** — epics → user-stories
   with screenshot-observable acceptance, tiered M1/M2/M3 (release cut = all of M1–M3).

## The 2.0 worklist — epics (milestone-tiered)

**Foundation & M1 (the Win10 era is usable):**
- **E0 Era foundation** — `Theme::Windows10` + `win10()` remap; era plumbing in
  `state.rs`/`main.rs`; per-era surface routing. *Files:* `mde-ui/src/palette.rs`,
  `font.rs`, `mde/src/{main,state,display}.rs`.
- **E1 Tiled Start** — left rail (account/places/power) + pinned/static tiles +
  all-apps. *New:* `mde/src/start_win10.rs`.
- **E2 Win10 taskbar** — jump lists, hover thumbnails, app badges, Task View button,
  search box, Action Center button. *Files:* `mde/src/panel.rs`.
- **E3 Action Center + notifications** — freedesktop notify daemon (zbus, shared with
  KDE Connect), toasts, slide-in center (history + quick toggles). *New:*
  `mde/src/action_center.rs` + `notifyd`.
- **E4 Multitasking** — Task View overlay (window grid via `wlr.rs`), virtual desktops
  (labwc workspaces + ext-workspace; fallback none), Snap assist + layouts. *New:*
  `mde/src/task_view.rs`; `rc.xml` binds.
- **E5 Search + Quick Access** — Search surface (Win+S; `mde/src/search.rs`); Win+X
  power menu (expand `popup.rs` + hotkey).
- **E6 Modern Settings app** — `mde settings`: flat category nav routing to existing
  backends; replaces Control Panel in the Win10 era. *New:* `mde/src/settings.rs`.
- **E7 Personalization** — wallpaper backend, lock screen, Start/taskbar config,
  accent (folded into Settings ▸ Personalization from `display.rs`).
- **E19 Power/session** — wire sleep + lock (stubs today in `dialogs.rs`).

**M2 (system + accounts + phone):**
- **E8 File Explorer + cloud-as-devices** — modernized Explorer + "Cloud Files" = KDE
  Connect device endpoints (remote sftp). *Files:* `mde/src/files.rs` + `MDE-KDECnt-Rust`.
- **E9 Your Phone** — unified KDE Connect surface with Win10 UX. Ties to `MDE-KDECnt-Rust`.
- **E10 Accounts / Lock / Sign-in** — Win10 lock + sign-in (LightDM-gtk-greeter) +
  Accounts in Settings.
- **E11 OOBE** — full first-run flow. *Files:* `mde/src/{installer,tui_setup}.rs`.
- **E12 Devices** — Settings ▸ Devices (mouse/keyboard/printers/second display/BT).
- **E13 Windows Update** — Settings ▸ Update over dnf (wires the System Properties stub).
- **E14 Security** — Windows-Security-style dashboard (firewalld, LUKS↔BitLocker,
  Find-my-device); unify existing tools.
- **E15 Networking** — Settings ▸ Network & Internet over NetworkManager.

**M3 (productivity + storage + polish):**
- **E16 Clipboard + Screenshots** — clipboard history (Win+V); Snip (Win+Shift+S) +
  PrintScreen. *New:* `mde/src/{clipboard,snip}.rs`.
- **E17 Storage / Backup / Recovery** — Storage Sense, Backup (Timeshift), Recovery
  (Reset this PC, recovery drive, advanced startup).
- **E18 Edge → Firefox integration** — default-browser surfacing + recent jump-list.
- **E20 Polish + accuracy** — per-surface gallery captures, keyboard-nav, era parity.

## Verification

- Per surface: `cargo build` + `cargo test` (accuracy gate) + `./preview.sh <component>`
  / `gallery` (the dynamic harness silently skips headless — CLAUDE.md §4). New
  `Theme::Windows10` pinned in `mde-ui/tests/checklist.rs`.
- Hotkeys verified live after `rc.xml` binds (keep `<mouse><default/>`).
- Notification daemon verified with `notify-send` + a KDE Connect round-trip.
- Release gate (`release` skill): all M1–M3 epics `[✓]`, build/clippy/fmt clean,
  gallery green, then the RPM cut.

## Pending threads

- One uncommitted change (`rust/mde/assets/licenses/NOTICE.md` — a partial DroidSans
  license entry from an interrupted `/ship` loop) — complete + commit before 2.0 work.
- The KDE Connect shared crate (`matthewmackes/MDE-KDECnt-Rust`, Phase 1 done) is a
  hard dependency of E8/E9.
