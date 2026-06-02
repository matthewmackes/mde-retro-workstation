# MDE-Retro 2.0 — Windows 10 era — Design SPEC

> Distilled from the Windows 10 Field Guide (Thurrott/Rivera) against the 15 locked
> roadmap decisions in `docs/ROADMAP-2.0.md`. Authority ranks below Memory and
> `.claude/CLAUDE.md` (CLAUDE.md §0). This is a design doc — the actionable items
> live in the `## 2.0 — Windows 10 era` section of `docs/PROJECT_WORKLIST.md`.

## 2.0 — The Windows 10 Era

MDE-Retro 2.0 adds a **fourth era** to the shell — *Windows 10* — alongside the existing Windows 2000 Classic, BeOS, and (default) IBM Carbon looks. Per **D1/D15**, this is **purely additive**: Carbon stays the shipped default, Win10 is opt-in, and "2.0" means the full Win10 feature set is *built* — M1/M2/M3 are internal sequencing only, not a phased public rollout.

The era is not a from-scratch skin. Per **D2**, the Win10 look is a **Carbon variant**: it reuses the same flat Carbon widgets (`mde-ui/src/widget/`, where `draw_edge` already flattens to a 2px-radius 1px-bordered fill under Carbon) and differs only in palette, accent, and light/dark default. The P0 pillars (**D3**) are the five that define the Win10 desktop: **Action Center, Task View / virtual desktops / Snap, the modern Settings app + Quick Access, Search, and the tiled Start.**

### Theme / era architecture (the single edge)

Everything routes through the one existing theme edge. `palette.rs` holds the canonical Win2000 role colors as `Rgb=(u8,u8,u8)` and remaps them per active `Theme` inside `color(rgb) -> iced::Color` — so **no call site changes** when the era switches. The live edge today is:

```rust
let rgb = match theme() {
    Theme::Win2000 => rgb,
    Theme::Beos    => beos(rgb),
    Theme::Carbon  => carbon(rgb),
};
```

**E0 adds exactly one arm:** `Theme::Windows10 => win10(rgb)`, with `win10()` modeled on `carbon()` (same role-key match, same `is_dark()` branching, same sentinel handling for `TITLE_TEXT`=`0xff,0xff,0xfe` and `WINDOW_FRAME`=`0x00,0x00,0x01`). Win10 reuses the existing `set_dark`/`is_dark` light-dark machinery and the `set_accent` atom; its proposed identity is **accent `#0078d4`** (`#2899f5` in dark) over a **`#1f1f1f` dark neutral** — pinned in `checklist.rs` once approved (§2.2). The `Theme` enum gains a fourth variant, the packed `THEME` atomic gains value `3`, and `theme()` gains its match arm. **No raw hex escapes `palette.rs`** (§2.1) and every size still flows through `metrics::` (§2.3).

### Per-era routing

Surfaces branch off `palette::theme()` exactly as `panel.rs` already does for the layer anchor (Carbon→top 32px, Win2000→bottom, BeOS→left 115px). Win10 introduces a top-anchored taskbar (E2) and a family of new `mde <subcommand>` layer-shell surfaces (Start, Action Center, Task View, Search, Settings) that follow the `popup.rs`/`menu.rs` drop-in pattern: a `main.rs` dispatch arm + a `%post` symlink + a labwc `rc.xml` keybind. **No compositor patching.** Per-process theme load (§7) means each subcommand re-reads `menu.json` and selects the era at launch; an era switch is not live across already-running surfaces — they must be relaunched. Critically, **the modern Settings app replaces Control Panel in the Win10 era only (D4)** — Win2000/Carbon keep Control Panel; the router picks one config surface per era off `theme()`.

## Cross-cutting concerns

These cut across multiple epics; they are specified once here so no epic re-invents them.

### 1. The shared notification daemon (E3 owns, E9/E12/E13/E15/E16/E17 consume)

Per **D7**, E3 stands up **one** freedesktop `org.freedesktop.Notifications` daemon — the single producer of toasts and the Action Center history store. Every other surface that needs to notify is a *consumer*, never a second daemon:

- **E9 Your Phone** routes KDE Connect notifications (and reply/answer actions) **through E3's daemon** (per **D5**), so phone toasts share the same store and badge.
- **E12 Devices** (AutoPlay / Swift Pair), **E13 Windows Update** (restart-required), **E15 Networking** (data-limit warning), **E16 Clipboard** (Copied/snip toast), **E17 Storage** (Clean/Backup-complete) all emit via `notify-send`/zbus to the same bus name.
- **E2 taskbar** renders the Action Center button + unread badge in `panel.rs`, reading E3's persisted mirror — E3 supplies the daemon and store; E2 supplies the chrome.

**Open: daemon ownership.** Planned host is the long-lived `panel` process (short-lived center/toast surfaces read the persisted mirror + `CloseNotification` over D-Bus). The decoupled alternative is a supervised `mde notifyd` from labwc autostart, surviving panel restarts — recommended if panel restart churn proves to drop notifications. **Coexistence:** if mako/gnome-shell already owns the bus name, the plan is to yield and show only KDE-Connect/MDE-routed history; packaging may instead conflict-declare against mako so MDE wins under the Win10 era. **Toast anchor:** the Win10 era taskbar is **top** (Carbon-style) in MDE, so a Win10-faithful bottom-right toast could overlap nothing, but the Action Center slides from the right — anchor toasts to avoid the tray.

### 2. KDE Connect / MDE-KDECnt-Rust (E8 + E9 share one device surface)

Per **D5/D10**, "Your Phone" and "Cloud Files" are both views onto the **same KDE Connect-paired device list**. The hard dependency is the **MDE-KDECnt-Rust** crate (LAN UDP 1716 + rustls transport; notifications/telephony/SMS/sftp/contacts plugins) plus an `mde connect` user daemon and pairing store. **E8 and E9 must not duplicate the device-list/pairing-store access** — they coordinate on one `connect.rs` surface (shared sftp endpoints for both the E8 "Cloud Files" browse and E9 photo/file transfer). Both ship a **fallback**: if the host LAN-transport layer hasn't landed, read the on-disk pairing store directly (live device browse degrades to last-known-paired). E14 Find-my-device and E12 are downstream consumers of the same surface.

### 3. Virtual-desktop / ext-workspace risk + fallback (E4)

E4's Task View + virtual desktops lean on **labwc `<desktops>`** and the **`ext-workspace-v1`** protocol via `wayland-protocols`. **RISK:** the pinned `wayland-protocols` crate may not expose `ext-workspace-v1` bindings. **Fallback ladder (E4.5):** (a) prefer crate bindings; (b) if absent, hand-write the protocol XML + scanner exactly as `wlr.rs` already vendors the wlr-foreign-toplevel protocol; (c) if even that is impractical, fall back to a `state.rs` `virtual_desktops` field driving a labwc-IPC desktop strip. Window-grid tiles use **wlr-foreign-toplevel via `wlr.rs`** (already on a bg thread) — which exposes **no pixel buffer**, so Task View and Snap Assist show **icon+title cards**, not live thumbnails, in P0 (a wlr-screencopy thumbnail cache is a heavier, deferred M2/M3 option). Closed-desktop orphans: confirm whether mde must explicitly `SendToDesktop` before `remove()` or trust labwc's adjacent-desktop policy.

### 4. labwc `rc.xml` hotkeys + the `<mouse><default/>` gotcha

All Win10 keybinds are added as labwc `rc.xml` `<keybind>` entries dispatching `mde <subcommand>` — **no compositor patching**. The Win10 chord set (consistent across E0/E1/E2/E3/E4/E5/E6/E19/E10/E16): **Win** (Start), **Win+I** (Settings), **Win+A** (Action Center), **Win+S** (Search), **Win+D** (show desktop), **Win+Tab** (Task View), **Win+X** (Quick Access), **Win+Ctrl+D / +Ctrl+F4 / +Ctrl+←→** (virtual desktops), **Win+←/→/↑/↓** (Snap), **Win+L** (lock), **Win+V** (clipboard), **Win+Shift+S / PrintScreen family** (snip), **Win+E** (Explorer), **Win+P** (Project), **Win+Shift+S→** *(note collision: snip vs. Security; Security is gated `Theme::Windows10`-only)*. **HARD GOTCHA (§7):** any `<mouse>` block in `rc.xml` **MUST start with `<default/>`** — labwc treats `<mouse>` as a full replacement, so omitting it kills every mousebind (windows unmovable, titlebar buttons dead). **Decision needed:** ship **two skel `rc.xml` variants** (Carbon vs Win10 keymap, selected at session start) **or one file** with Win10 binds always present and each `mde <sub>` self-suppressing off `palette::theme()`. The latter is simpler to package; the former is cleaner per-era. E0/E2/E7 also write the labwc `themerc` `win10_accent()` so the accent shows on labwc-drawn titlebars (the one place the compositor boundary lets mde influence chrome).

### 5. Per-era Settings-vs-Control-Panel routing (D4)

The single most load-bearing era branch: **in the Win10 era, `mde settings` (E6) replaces Control Panel; in Win2000/Carbon, Control Panel stays.** Every "open config" entry point (Start rail gear, Search results, right-click Personalize, taskbar settings, Action Center deep-links) must route off `palette::theme()` — Win10 → `mde settings [--page X]`, else → `mde control-panel`/`display`/`system-properties`. **Interim contract:** until E6 lands, Win10 entry points dispatch the existing Control Panel surfaces (E1 rail Settings → `mde control-panel`; E5 Quick Access power options → `display`/`system-properties`), then re-point to `mde settings` once E6 ships. E6 reuses `control_panel.rs`/`display.rs`/`system_properties.rs`/`sysinfo.rs` structurally; only **Colors + the Display/Background home rows are native iced** in M1, everything else spawns an existing fedora tool. **Open:** does `settings.rs` expose a per-epic page-registration API (E12/E15 drop in submodules) or one monolithic match — this gates whether E12/E15/E13/E17 pages are drop-in modules or inline arms, and whether each page is its own short-lived process (grim-capturable, matches one-process-per-subcommand) or one long-lived KDE-Systemsettings-style host.

## Milestone sequencing (M1/M2/M3 are internal only — D15)

Per **D15**, all 2.0 scope ships; M1/M2/M3 are build-ordering, not a staged release. The ordering is driven by the dependency graph, not feature priority.

### M1 — the five P0 pillars + their foundation (E0–E7, E19)

**E0 is the universal gate.** Nothing renders Win10 until `Theme::Windows10` + `win10()` + the `state.rs` era field + `main.rs` startup select + per-era routing land. Every other epic carries a defensive "E0.1 adds it if absent" clause, but in practice E0 lands first.

After E0, the five pillars (**D3**) and Power/Session land together because they are the irreducible Win10 desktop and they cross-reference each other:
- **E2 (taskbar)** anchors the bar and owns the Start-button click + the Action Center badge — so E1/E3 plug into it.
- **E1 (tiled Start)** and **E5 (Search)** share one search surface (type-at-Start hands off to `search.rs`); E1 depends on E2 for the Start button and on E5 for type-to-search.
- **E3 (Action Center)** supplies the notify daemon that E2 renders the badge for.
- **E4 (Multitasking)** is mostly self-contained on `wlr.rs` + labwc but feeds a Task View button into E2's panel.
- **E6 (Settings)** + **E7 (Personalization)** close the loop: E6 is the D4 Control-Panel replacement that E1's rail, E3's config screen, and E7's Personalization rail all target; E7's Colors page drives the accent that E0 introduced.
- **E19 (Power/Session)** is small and rides E1 (rail power/user tiles) + E0 theme.

**Why pillars-together, not strictly serial:** they form a tight cycle (panel↔Start↔Search↔Settings↔Action Center). The interim contracts (E1 rail → `control-panel` until E6; E2 search → keybind-only until E5; E3 config-read until E6) let them land in any order within M1 without stubbing — each degrades to an existing surface, never to a `todo!()`.

### M2 — devices, identity, networking, the OOBE, second config surfaces (E8–E15, excl. E5/E6/E7 already in M1)

M2 builds on M1's Settings shell and notify daemon. **E8 (Explorer) + E9 (Your Phone)** land together because they share the KDE Connect device surface (must coordinate, not duplicate). **E10 (Accounts/Lock)**, **E12 (Devices)**, **E13 (Windows Update)**, **E14 (Security)**, **E15 (Networking)** are all **Settings categories** — they require E6's shell + nav rail (M1) and E3's daemon (M1) before they can mount. **E11 (OOBE)** lands here because it *hands off to* the finished M1 desktop (panel + taskbar) and *feeds* E10's greeter the account it creates — so the desktop it lands on must exist first.

### M3 — polish, productivity extras, parity (E16–E18, E20)

The deferrable layer: **E16 (Clipboard/Snip)**, **E17 (Storage/Backup)**, **E18 (Edge→Firefox)** are productivity surfaces that consume M1/M2 (notify daemon, Settings pages) but nothing depends on them. **E20 (Polish/accuracy)** is **last by construction** — it captures the gallery, pins `Theme::Windows10` accents in `checklist.rs`, and runs the four-era keyboard-nav parity sweep, which can only be meaningful once every surface it captures (Action Center, Settings, Start, Task View, Search) exists. E20 ships a carbon-aliased placeholder for `win10()` only if E0 somehow slips, which it should not.

### Dependency-ordering note (what must land before what)

1. **E0** (`Theme::Windows10` + `win10()` + era plumbing) — before *everything*.
2. **E3 notify daemon** — before E2's badge, E9/E12/E13/E15/E16/E17 toasts.
3. **E2 panel** — before E1's Start button, E3's badge chrome, E4's Task View button.
4. **E6 Settings shell + nav rail** — before E7/E10/E12/E13/E14/E15/E17 pages (all are Settings categories).
5. **MDE-KDECnt-Rust + `mde connect`** — before E8/E9 live device browse (both fall back to the on-disk pairing store if it slips).
6. **M1 desktop (panel+Start+taskbar)** — before E11 OOBE (it hands off to that desktop).
7. **All capturable surfaces** — before E20's gallery/parity sweep.

## Per-area design

### E0 Era foundation (Theme::Windows10 + win10() remap, era plumbing, per-era routing)

**Decisions covered:** D1 (Win10 is a new, opt-in 4th era; Carbon stays default), D2 (look = a Win10-flavored Carbon variant — reuse flat Carbon widgets, shift palette/accent; accent system + light/dark). Foundation for D3–D15 (this epic is what every other E# routes off of via `palette::theme()`).

#### What Windows 10 actually does (Field Guide, Get-to-Know + Personalize chapters)

Windows 10's visual identity, distilled from the guide's *Get to Know* and *Personalize* chapters:

- **Flat, chrome-light.** No 3D bevels, no title-bar gradients. Surfaces are flat fills separated by thin lines and elevation. Functionally identical to the Carbon look mde already ships (flat layers + 1px borders + 2px corners), which is exactly why D2 makes Win10 a *variant* of Carbon rather than a new widget set.
- **A user-chosen accent color** is the system's one chromatic element (Settings ▸ Personalization ▸ Colors). Win10's stock accent is a blue (`#0078D4`-family), deliberately *brighter/cyan-er* than Carbon Blue 60 (`#0f62fe`) — that hue shift is the whole visible difference between the Win10 era and Carbon. The accent drives selection, focus, Start tile highlights, and the Action Center toggles. **Scope note:** E0 only ships the *default* Win10 accent value (one source, `win10_accent()`); the user-selectable accent picker (Settings ▸ Personalization ▸ Colors) is a later epic — E0 just guarantees the value exists for everything else to route through, exactly as `carbon_accent()` is a fixed value today.
- **A light/dark "choose your color" mode** ("Light", "Dark", or "Custom"). The guide's Personalize chapter routes all of this through the Settings app (`WINKEY + I` ▸ Personalization ▸ Colors). Win10 light mode is a near-white surface set; dark mode is a deep neutral (`#1f1f1f`-family) — slightly *bluer/cooler* than Carbon Gray 90/10, which is the era's secondary tell. E0 reuses the existing `set_dark`/`is_dark` atomic for this (no new light/dark state).
- **"Show accent color on Start, taskbar, and action center"** and **"…on title bars and window borders"** are the two stock accent toggles. We honor the *behavior* (accent participates in shell chrome) but title bars are labwc's (compositor boundary), so accent-on-titlebar is realized via the labwc themerc, not by mde drawing caption rows.
- **Signature shortcuts** (the guide repeats these): `WINKEY` (Start), `WINKEY + I` (Settings), `WINKEY + A` (Action Center), `WINKEY + S` (Search), `WINKEY + D` (show desktop). E0 owns none of the *surfaces* behind these, but it owns the **era switch** that decides whether those surfaces exist at all (Win10 era) or are absent (Carbon/Win2000 keep Control Panel + classic Start).

Out of scope here and never a feature (D9, global out-of-scope list): touch, tablet mode, Windows Ink, Cortana, Spotlight ads, and all the apps listed out-of-scope (Maps, Groove/Music, Store, Mail, Skype, Calendar, Photos/Video, Movies & TV, Xbox, OneNote, People). E0 introduces no surface — only the theme/era plumbing.

#### MDE design — the one theme edge plus era plumbing

E0 is pure plumbing: it adds the 4th theme to the single `palette::color()` remap edge and the state/startup wiring, with **zero call-site churn** (every surface already routes color through `palette::color()` and font through `font::family()`). It does not add a subcommand of its own.

**1. `mde-ui/src/palette.rs` — `Theme::Windows10` + `win10()` remap (the only place hex lives, §2.1).**

- Extend `enum Theme { Win2000, Beos, Carbon, Windows10 }` (discriminants 0,1,2,3 — matches the existing `THEME` atomic encoding). The packed `THEME` atomic gets value `3`; update `theme()`'s match (`3 => Theme::Windows10`) and add `pub fn is_windows10() -> bool`.
- Add `fn win10(rgb: Rgb) -> Rgb` modeled byte-for-byte on `carbon()` (same `is_dark()` read, same sentinel handling for `TITLE_TEXT`/`HIGHLIGHT_TEXT`=`0xff,0xff,0xfe` and `WINDOW_FRAME`=`0x00,0x00,0x01`, same role-key tuples). The differences from `carbon()` are exactly two families:
  - **Accent family** (`HIGHLIGHT`/`ACTIVE_TITLE` `0x0a,0x24,0x6a`, `ACTIVE_TITLE_GRADIENT` `0xa6,0xca,0xf0`, `INFO_BAND` `0x1d,0x5c,0xa8`, `SETUP_PROGRESS` `0x16,0x3a,0xa8`, plus the setup-wizard blues) → the **Win10 accent** instead of Carbon Blue. Default Win10 accent = `#0078d4` (light) / `#2899f5` (dark). Provided by a new `pub fn win10_accent() -> Rgb` (mirrors `carbon_accent()`), so the accent is named in one spot.
  - **Neutral surfaces** → Win10's cooler neutrals: light `WINDOW` stays `#ffffff` but panels/menus (`0xd4,0xd0,0xc8`) map to `#f3f3f3`/`#2b2b2b`, desktop (`0x3a,0x6e,0xa5`) to `#1f1f1f` (dark) / `#e6e6e6` (light), shell header (`0xd4,0xd0,0xc7`) to `#1f1f1f`/`#ffffff`. Everything else (text-primary invert, danger red, border grays) reuses the Carbon token values verbatim — Win10 and Carbon share the same flat-neutral skeleton, only the accent hue and a couple of neutral temperatures shift.
- Add `win10()` to the `color()` match arm: `Theme::Windows10 => win10(rgb)`. The existing `set_dark`/`is_dark` atomics serve Win10's light/dark with no new state. The `set_accent`/`icon_accent` icon-tint hues stay shared (D2 reuses the accent *system*); the *UI* accent for Win10 comes from `win10_accent()`, parallel to how `carbon_accent()` is always-blue regardless of icon hue.

**2. `mde-ui/src/font.rs` — Win10 type family.** `family()` already returns Plex for Carbon/BeOS. Win10's face is Segoe UI (not redistributable). Per §2.4 ("don't launder the font gap") we keep the unattainable target named and ship a substitute: extend `family()` so `is_windows10()` also returns the bundled humanist sans. Reuse **IBM Plex Sans** (already embedded, OFL) rather than adding a new TTF — it is the closest bundled humanist face and avoids a new asset/licence entry; record `UI_FONT_TARGET_WIN10 = "Segoe UI"` as the documented target. No new `.font(...)` registration is needed (Plex is already registered at every app builder).

**3. `mde/src/state.rs` — era plumbing.** The `theme` field is a free-form string; add `"windows10"` as a recognized value. No schema change is required (every field is already `#[serde(default)]`, §2.6), but add a unit test asserting `parse(r#"{"theme":"windows10"}"#).theme == "windows10"` round-trips and that an unknown theme still falls back through `main.rs` to the Carbon default. The Carbon defaults in `def_theme()` are untouched (D1: Carbon stays default).

**4. `mde/src/main.rs` — startup era select + per-era surface routing.** The startup block already maps `st.theme` to a `palette::set_theme(...)`; add the arm `"windows10" => palette::set_theme(Theme::Windows10)` *before* the `_ => Carbon` fallback. `set_dark`/`set_accent` stay as-is (Win10 reuses the same light/dark + icon-hue state). This is the single point where the era becomes live for every subcommand process (§7: per-process theme load).

**5. Per-era surface routing branches off `palette::theme()`.** E0 establishes the pattern the later epics consume; it wires the two routing points that already exist:
   - `panel.rs` anchor block (currently `is_carbon()` / `is_beos()` / else): add an `is_windows10()` arm. The Win10 era taskbar is a **bottom** bar (Win10 ships bottom by default). Reuse the existing bottom-anchor `LayerShellSettings` branch (Win10 and Win2000 share bottom-edge geometry; E2 later layers Win10 content onto it). The existing Win2000 bottom branch uses `metrics::TASKBAR_HEIGHT` (28). If a taller Win10 bar is wanted, add a `WIN10_BAR_H` metric (e.g. 40); otherwise reuse `TASKBAR_HEIGHT` — best-choice at write time, recorded in the commit. Acceptance for E0 is only that the panel launches and anchors correctly under the new theme — Win10 taskbar *content* is E2.
   - `display.rs` Appearance tab: add "Windows 10" to the theme picker. **NOTE the local enum and the exhaustive themerc match.** `display.rs` carries its OWN local `Theme` enum (`Carbon, Win2000`) plus `const ALL`, `key()`, `from_key()`, `label()`, AND an **exhaustive** `match (state.theme, state.theme_mode)` that writes the labwc title colors. Adding the Win10 option means extending **all** of these, and in particular the exhaustive match **must** gain a `(Theme::Windows10, _)` arm or the build breaks. That arm writes a labwc themerc bg/fg pair (Win10 dark header `#1f1f1f`/`#ffffff`, accent on borders) — these are **labwc config strings, not iced colors**, so they fall under the same §2.1-exempt themerc precedent already present in this function (the existing Carbon/Win2000 hex there); source the accent value from `win10_accent()` rather than re-typing it. (D4's Settings-replaces-Control-Panel is E6; in E0 the Win10 era is still selectable from the existing Carbon-era Appearance UI so it can be turned on at all.) Selecting it writes `theme:"windows10"` via `state::save()`; relaunching surfaces renders Win10 (§7: theme change isn't live across running surfaces).

**6. `mde-ui/tests/checklist.rs` — pin the new theme (§2.2).** Add a `windows10_remap_pins` test: set `Theme::Windows10`, assert `win10_accent()` returns the pinned `#0078d4`/`#2899f5`, assert the sentinel passthrough (`color(TITLE_TEXT)` stays light, `color(WINDOW_FRAME)` becomes a border gray not black text), and assert a neutral (`color(WINDOW)` light = white). Mirrors the existing `carbon_sentinels_and_header_are_pinned` discipline. Restore the prior theme at test end (the atomics are process-global).

#### Linux backend mapping

E0 drives **no** Linux backend itself — it is the theme/era switch. It is the prerequisite that lets later epics' backend bindings (NetworkManager, dnf, firewalld, LightDM-gtk-greeter, KDE Connect, labwc ext-workspace, wlr-foreign-toplevel) be routed *per era*. The one compositor-adjacent note: accent-on-titlebar (Win10's "show accent on title bars") is realized by writing the accent into the **labwc themerc** at era switch, not by mde — consistent with the compositor boundary (mde never draws caption rows). That themerc write is E7/E2 polish; E0 only guarantees the accent *value* exists (`win10_accent()`) for them to read, and wires the minimal Win10 themerc arm needed for the display.rs match to compile.

#### Reused code (no duplication)

- `palette::color()` single edge, the `carbon()` function as the literal template for `win10()`, `carbon_accent()` as the template for `win10_accent()`, the `DARK`/`ACCENT`/`THEME` atomics and their setters — all reused, only extended.
- `font::family()` reuses the already-embedded Plex TTFs.
- `state.rs` reuses the tolerant `parse`/`save`/`load` path unchanged.
- `main.rs` reuses the existing startup theme-select block and the existing argv dispatch.
- `panel.rs` reuses the bottom-anchor `LayerShellSettings` branch; `display.rs` reuses the existing Appearance picker plumbing (local `Theme` enum + the labwc themerc rewriter), extended with a Win10 arm.

The whole epic is additive and §3-complete in one pass: after E0, `theme:"windows10"` in `menu.json` makes every existing surface (`panel`, `menu`, `files`, `display`, dialogs) render in the Win10 variant with zero new stubs.

### E1 Tiled Start (Windows 10 era)

**Depends on E0** (`Theme::Windows10` + `win10(rgb)` remap at the `palette::color()` edge; era plumbing in `state.rs`/`main.rs`). Target: NEW `mde/src/start_win10.rs`, reached via `mde start-win10` (new `main.rs` dispatch arm + `%post` symlink + an `rc.xml` Win-key bind for the Win10 era).

#### What Windows 10 actually does (from the Field Guide, pp. 3–7, 85–88)

Start is the familiar bottom-left Start button; the **Windows key** (or the button) shows/hides it. As a menu (the default — full-screen tablet mode is **out**, D9) it has **three columns**:

- **Left navigation pane** — a thin icon rail (account picture at top; then configurable system folders Documents / Pictures / Settings; **User account and Power are pinned and cannot be removed**, p. 87). On mouse-over the rail **expands** to a wider flyout showing each item's name.
- **Center column** — **Recently Added**, **Suggested**, then the **All Apps** alphabetical list (with **#** and **A–Z** group headers; clicking a header jumps to an alphabet index). Hidden only in full-screen mode (out of scope).
- **Right tile area** — **live/static tiles** in named **groups**. Tiles come in sizes **Small / Medium / Wide / Large** (Field Guide: default columns are 3 medium tiles wide; "Show more tiles" widens to 4). Tiles can be **arranged, grouped, resized, and named**; groups have editable header bands.

Signature interactions: open Start and **just start typing** to search (p. 7 — this hands off to E4 Search, not built here). **Right-click an app in the All-Apps list** → context menu with **Pin to Start**, **Unpin from Start**, **Resize ▸ (Small/Medium/Wide/Large)**, **More ▸ (Run as administrator / Open file location)**, **Uninstall** (pp. 84–88). **Win+X** opens the power-user Quick Access menu off the Start button (separate surface, E2). Power button → Sleep / Shut down / Restart submenu.

#### MDE design

**One new module, `mde/src/start_win10.rs`**, modeled structurally on `menu.rs::view_carbon` (which already proves a flat tile grid reusing layer-shell + click plumbing). It is a **full-screen transparent layer-shell overlay** (same `Anchor::Top|Bottom|Left|Right`, `exclusive_zone: 0`, `KeyboardInteractivity::Exclusive` settings as `menu.rs::launch`) so a backdrop click / Esc closes it. The panel is anchored **bottom-left above the Win10 taskbar** (the Win10 era panel anchors `Anchor::Bottom` — mirror of `panel.rs` Win2000 geometry; read `CARBON_BAR_H`-style era bar height).

**Layout (three regions in a `Row`):**
1. **Left rail** — a narrow (~48px) `Column` of icon buttons; reuses `crate::icons::icon_any` and `carbon_tile_style`-style accent-tinted hover. Items, top→bottom: account avatar, then state-driven system folders, then **Settings (gear)** and **Power**. Account + Power are always present (never removable), matching p. 87. The rail **expands on hover** into a labelled flyout via an iced `mouse_area`/hover `Message` that widens the rail and shows `text` labels (reuses the existing hover→`Message`→re-`view` pattern menu.rs uses).
2. **Center column** — a `scrollable` (styled `mde_ui::scrollbar`) with three sub-sections built from real data:
   - **Recently Added** + **Suggested**: top N of `apps::programs()` flattened, sorted by `.desktop` mtime (Recently Added is genuinely the newest-installed `.desktop` files via `std::fs::metadata` — no mockup/`demo_data`). Suggested = the user's most-launched pins from `state.rs` (launch-count field added to `PinnedItem`, `#[serde(default)]`).
   - **All Apps**: `apps::programs()` flattened across folders, deduped, sorted case-insensitively, rendered with **#/A–Z group headers** (a header row before each first-letter change). Each app is a row reusing `menu.rs`-style row buttons; clicking launches via the same `launch_cmd`/`Act::Cmd` mechanism (lift `run_act`/`launch_cmd` from `menu.rs` into a shared helper rather than duplicating).
3. **Right tile area** — a tile grid driven by a new `state.rs` field `start_tiles: Vec<StartTile>` (`#[serde(default)]`, garbage→empty per §2.6). `StartTile { name, command, icon, size: TileSize, group: String }`. Tiles render with the existing `carbon_tile_raw`/`carbon_tile_style` flat-tile look (extracted into a shared widget so both Carbon Start and Win10 Start use one implementation). `TileSize` maps to span counts: Small 1×1, Medium 2×2, Wide 4×2, Large 4×4, laid out left-to-right in a grid of named **groups** (each group a `Column` with an editable header `text_input`). On first run with no `start_tiles`, seed from the existing `state.pinned` so the area is never blank.

**Theme routing (per-era, off `palette::theme()`):** `start_win10.rs::run` asserts the Win10 era; the Win-key bind and panel Start button dispatch `mde start-win10` **only when `palette::theme() == Theme::Windows10`**, exactly as `panel.rs:134` / `menu.rs:836` branch today. Win2000/Carbon/BeOS keep `mde menu`. No call site changes — selection is one `match palette::theme()` arm in `panel.rs` (Start click) and one `main.rs` dispatch arm.

**`Theme::Windows10` styling:** all surfaces go through `palette::color()`; the new `win10(rgb)` remap (E0) is a Carbon-variant — flat panel = `palette::MENU`, 1px `palette::WINDOW_FRAME` border (like `view_carbon`'s panel), accent-tinted tile hover via `palette::accent()`/`carbon_accent()` shifted to the Win10 cobalt accent, light/dark via `palette::is_dark()`. **No raw hex** anywhere in `start_win10.rs` (§2.1); every `.size()` uses `metrics::UI_PX`/`metrics::INFO_TITLE_PX` (§2.3). Tile sizes are named `metrics::` constants (`TILE_SMALL_PX`, etc.), not scattered literals.

**Right-click tile/app actions:** reuse `menu.rs`'s `context: Option<(col,idx)>` + `RightClick`/`Ctx*` message pattern and the headless write paths. **Pin to Start** appends a `StartTile` (Medium default) to `start_tiles` and `state::save` (atomic, §2.6); **Unpin** removes it; **Resize ▸** rewrites the tile's `size`; **Open file location** opens the `.desktop`'s dir in `mde files`; **Uninstall** shells `dnfdragora --remove` (the Linux-backend mapping). Extend the existing `mde menu --pin/--unpin/--list-pinned` headless CLI with `mde start-win10 --pin/--unpin/--resize/--list-tiles` so the actions are **bench-testable without the GUI** (a dbus-free round-trip: write a tile, re-read it).

**Linux backends:** apps = `.desktop` scan via `apps.rs` (reused); launch = `sh -c` / `foot` terminal split (reused from `menu.rs::launch_cmd`); recently-added = `.desktop` mtime; the Settings rail item dispatches `mde settings` — the **Win10-era Settings route (E4/D4)** that **replaces** Control Panel in this era (one config surface per era; Win2000/Carbon keep `mde control-panel`, the Win10 era never reaches it). For E1 this same `mde settings` dispatch lands on E4's Settings entry point (stubbed by E4), recorded as an E4 dependency — it **must not** fall back to `mde control-panel`, which would drag the legacy config surface into the Win10 era and breach D4. Power dispatches the existing `mde shutdown`/`mde logoff` surfaces.

**Reused code:** `menu.rs` cascade/`Click`/`RightClick`/context plumbing and `launch_cmd`/`run_act` (extracted to a shared `start_common` helper, not duplicated); `apps::programs()`; `state.rs` (`PinnedItem`, atomic `save`, the singleton pid-file guard from `menu.rs::acquire_singleton` — promoted into `start_common` with a **parameterized pid-file basename** so Win10 Start guards on `mde-start-win10.pid`, not the Carbon menu's `mde-menu.pid`); `crate::icons::icon_any`; `mde_ui::{frame, metrics, palette, scrollbar}`; the `carbon_tile_raw`/`carbon_tile_style` flat-tile widget (promoted to a shared widget). **Compositor boundary respected** — Start is a layer-shell surface only; labwc owns frames/z-order (no client title rows).

**Win key:** the Win10-era `rc.xml` binds `W-` (Super) to `mde start-win10` (the keybind block must keep `<mouse><default/>` per §7). The module reuses the parameterized `acquire_singleton("mde-start-win10")` pid-file guard so a second Win press doesn't stack overlays.

### E2 Win10 taskbar (panel.rs · Theme::Windows10)

#### Win10 behavior (from the Field Guide, pp. 5-15, 88-92)

**Layout (bottom edge, left-to-right):** Start button · Search (box by default, button, or hidden) · Task View button · Quick-Launch pins + running-app buttons (icon-only by default, grouped/combined per app) · spacer · **notification area** (overflow chevron + system/app tray icons + native Network/Speaker/clock pop-ups) · clock+calendar (two lines: time over date) · **Action Center button** (far right) · a 1px "Peek at desktop" sliver at the very end.

**Behaviors/states:**
- Running app buttons show running apps + open windows automatically; the focused window's button is highlighted. Right-clicking a taskbar button opens a **jump list** (per-app: recent files / frequent locations / pinned tasks). Hovering a button shows **live thumbnail previews** of that app's windows.
- **Search**: a box to the right of Start, or a button, or hidden (default per the guide's advice). Tabs at the top of the Search pane filter (Apps/Documents/Web/Settings/etc.). `WINKEY` or "Start then type" reaches the same Search; the box is just a shortcut.
- **Task View button** (enabled by default): full-screen thumbnail grid of all open windows + a strip to switch/create virtual desktops. Reachable by `WINKEY + TAB`.
- **Action Center button** (far right): toggles a right-edge pane = notifications (top, scrollable history) + a grid of quick-action tiles (bottom; first 4 visible collapsed, "Expand" shows the rest). A **number badge** on the button counts missed notifications; it clears when the pane opens. `WINKEY + A` toggles it.
- **Quick Access power menu** on **right-click Start** (`WINKEY + X`): power-user/legacy links.
- **Notification-area pop-ups** (Network, Speakers, clock/calendar) are larger flyouts.
- **Taskbar settings** reached by right-clicking empty bar → "Taskbar settings": position (top/bottom), auto-hide, combine buttons, which Search/Task-View buttons show, which icons appear.
- Auto-hide: bar slides off-screen, returns on edge-hover.

#### MDE design

Win10 is the **4th era**, an additive Carbon variant (D1/D2). All taskbar work lands in **`mde/src/panel.rs`**, gated on a new `view_win10()` arm beside `view_carbon`/`view_horizontal`/`view_vertical`, plus a handful of new drop-in `mde <sub>` layer-shell surfaces that follow `menu.rs`/`popup.rs`.

**Theme edge (mde-ui/palette.rs).** Add `Theme::Windows10` to the `Theme` enum (variant index 3), extend `THEME` decode in `theme()` (`3 => Windows10`), and add `is_win10()`. Add a `fn win10(rgb: Rgb) -> Rgb` remap **modeled on `carbon()`** but with the Win10 flat-dark "Dark Mode" palette: bar/Action-Center surface = near-black `#1F1F1F`-class gray (mapped from `SHELL_HEADER` `0xd4d0c7`), Start/accent = the user's accent color (default Win10 cobalt `#0078D4`), light/dark switched via the existing `DARK` atomic and `is_dark()`. **Accent reuse (D2), corrected for reality:** the existing `carbon_accent()` is hardwired to Carbon Blue 60 and *ignores* the `ACCENT` atomic (which today only drives icon tinting via `carbon_icon_accent`). So Win10's *configurable UI accent* is wired by reusing the existing accent **storage** edge (`set_accent`/the `ACCENT` atomic) and adding a small `fn win10_accent() -> Rgb` that actually **reads** `ACCENT` and maps it to the Win10 accent token — i.e. extend the one accent system, do **not** add a parallel atomic and do **not** reuse `carbon_accent()`'s fixed-blue body. `color()`'s match gains `Theme::Windows10 => win10(rgb)`. **No raw hex outside palette.rs** (§2.1); every `.size()` keeps using `metrics::` constants (§2.3). Add `metrics::WIN10_BAR_H` (40) alongside `metrics::TASKBAR_HEIGHT` (and `panel.rs`'s existing `CARBON_BAR_H` const), and pin `WIN10_BAR_H` in `mde-ui/tests/checklist.rs` (§2.2).

**Panel routing & layout.** In `launch()`, add a `palette::is_win10()` branch to the `layer_settings` selector: anchor `Bottom | Left | Right` by default, height `WIN10_BAR_H`, exclusive zone `WIN10_BAR_H` — **but** read position+auto-hide from `state.rs` (see below) so the same branch can anchor `Top` and drop the exclusive zone to 0 for auto-hide. `style()` already routes the bar background through `SHELL_HEADER` via `palette::color()`, so the Win10 dark bar falls out of the new `win10()` remap with **zero `style()` changes**. `view()` gains `else if palette::is_win10() { view_win10(state) }`.

`view_win10()` reuses the existing flat-Carbon construction helpers (`start_button`, `carbon_task_button`'s flat-tab + accent-underline pattern, `tray_glyphs`, the clock `text`) but arranges them Win10-style: square Start tile (reuse `START_ICON` raster, tinted to accent), then Search, Task View, app buttons (icon-only via `icons::icon_any`, focused = accent underline — same code as `carbon_task_button`), spacer, tray glyphs (`tray_glyphs()` unchanged — SNI + native volume/net/battery), the two-line clock, then the Action Center button with its badge, then the 1px Peek sliver. Window-button click rules (focus/minimize/restore) and `wlr::Wm` wiring are **identical to Carbon** and reused verbatim.

**New surfaces (each a `main.rs` dispatch arm + `%post` symlink + labwc `rc.xml` keybind, no compositor patching):**

- **`mde search`** — layer-shell flyout anchored bottom-left above Search (like `menu.rs`). A text input + result tabs (Apps/Documents/Settings/Web). Apps come from the same `.desktop` scan `menu.rs` already does; Documents via `walkdir` over `$HOME` (reuse `files.rs` listing helpers); Settings rows deep-link `mde settings <pane>` (E-Settings); Web opens Firefox (D12). Launch with `WINKEY` and via the bar's Search box `on_press`. **Panel owns the child** like `menu` (toggle-close on re-press), reusing the existing `state.menu`/`spawn_child`/reap pattern.
- **`mde taskview`** — full-screen layer-shell overlay (keyboard-interactive, like `menu.rs`'s `Anchor::Top|Bottom|Left|Right`). A grid of running windows from `wlr::Wm.windows()` (icon + truncated title; click → `wm.focus(id)`, close → `wlr` close request) + a bottom strip of virtual desktops (list/switch/create). **wlr.rs work (NOT free reuse):** the existing `Wm` over wlr-foreign-toplevel-management exposes only `windows()`/`focus()`/`set_minimized()` and carries **no** workspace concept; virtual desktops require binding the **separate `ext-workspace-v1`** protocol. So this adds a new ext-workspace handler to `wlr.rs`, reusing only its existing background-thread + Wayland event-loop scaffolding, and exposes `workspaces()`/`activate_workspace()`/`create_workspace()`. Bound to `WINKEY+TAB`. (Live per-window thumbnails are a future item; P0 ships icon+title tiles since wlr-foreign-toplevel exposes no pixel buffer — recorded as an open question.)
- **`mde jumplist <app_id>`** — `popup`-style context flyout (extend `popup.rs`). Pinned tasks + recent items: for `mde files`/File-Explorer show pinned/recent folders from `state.rs`; for Firefox show "New window"/"New private window"; generic apps get "Open / Unpin from taskbar". Triggered by **right-click on a Win10 app button** (new `Message::JumpList(u64)` → `spawn_child(["jumplist", app_id])`), replacing the bare `MinimizeToggle` right-click for the Win10 arm.
- **`mde action-center`** — right-edge layer-shell pane (`Anchor::Top|Right|Bottom`, ~360px). Top: scrollable notification history; bottom: quick-action tile grid (Wi-Fi→NetworkManager, Bluetooth, Night light→`wlr-gamma`/`gammastep`, Brightness→the existing logind `SetBrightness` path lifted from `panel.rs::step_brightness`, Focus assist→DND toggle, All settings→`mde settings`). This is the **Action Center button** target and is shared with E7 (notification daemon owns the history store; the pane reads it). Bound to `WINKEY+A`. Cross-epic dependency on E7 for the toast/history backend (D7).

**Badge & auto-hide state.** Add `Message::ActionCenter`, `Message::TaskView`, `Message::Search`, `Message::JumpList(u64)` to `panel.rs`. The Action Center button badge count comes from the E7 daemon over D-Bus (unread toast count); `panel.rs` reads it on the existing 1-s `Tick` (cheap-read, like the tray snapshot) and draws a count chip when >0. **`state.rs` (§2.6)** gains `#[serde(default)]` fields `win10_taskbar_top: bool`, `win10_autohide: bool`, `win10_search_mode: enum{Box,Button,Hidden}`, `win10_show_taskview: bool`, each with a default fn, kept compatible per the manual-`Default`-agrees-with-`parse("{}")` test. Auto-hide is implemented by toggling the layer surface's exclusive zone to 0 and shrinking off-screen; an edge `mouse_area` reveals it.

**Per-era isolation:** Win2000/Carbon/BeOS arms are untouched — they keep their bars, their Control Panel, their context menus. Win10 surfaces only spawn when `palette::is_win10()`. Taskbar position/auto-hide config is edited from `mde settings > Personalization > Taskbar` (E-Settings, replacing the Win2000 `taskbar-properties` dialog in the Win10 era only, D4).

**Reused code:** `panel.rs` (bar shell, clock, `tray_glyphs`, `step_brightness`, child-reaping), `wlr.rs` (window list/focus/minimize + its bg-thread/event-loop scaffolding for the new ext-workspace binding), `tray.rs` (SNI), `icons.rs` (`icon_any`), `state.rs` (config), `menu.rs`/`popup.rs` (layer-shell flyout pattern), `mde-ui/widget` flat-Carbon styles, the `ACCENT`/`set_accent` accent-storage edge (extended with `win10_accent()`). New backends: **`ext-workspace-v1`** in `wlr.rs` (virtual desktops), Firefox (web search), the E7 freedesktop daemon (Action Center).

### E3 Action Center + notifications

#### Win10 source behavior (Field Guide, "Get to Know Windows 10", pp. 10-14)

**The Action Center button** lives in the taskbar notification area, to the right of the tray, before the clock. A **number badge** on it counts missed notifications; the badge clears when the center is opened. Win+A toggles it.

**The slide-in pane** appears anchored to the **right edge of the screen, full height**. It has two stacked regions:
- **Top (larger): notification history.** Missed/unactioned notifications, **grouped by source app** (app name as a group header). Per-notification: an icon, app/title text, body text, timestamp, a **Clear ("x")** at the top-right of each notification, and a **downward caret** on notifications with more content (expand/collapse). Each app group header has its own **Clear ("x")** that dismisses all of that app's notifications. The pane bottom has a **"Clear all"** link. Selecting a notification opens the owning app / the item that triggered it, and removes the notification from the center. Some notifications expose **inline actions** (e.g. an inline reply/action button carried in the freedesktop `actions` array).
- **Bottom: quick action tiles.** A grid of frequently-needed system settings (Wi-Fi, screen brightness, Airplane Mode, Focus assist, Nearby sharing, Night light, Bluetooth, etc.). Collapsed it shows the **first four** tiles; an **"Expand"/"Collapse"** link toggles the full grid. There are "over a dozen" tiles. Each tile is a flat square that toggles a system setting; brightness is a slider-style tile. The first four (collapsed-visible) are the most-used ones.

**Toasts** are the pop-up form: a notification first appears as a transient toast at the bottom-right; if not acted on it flows into the Action Center history. (The guide describes the same notifications surfacing as pop-ups and then collecting in the center.)

Settings (System > Notifications & actions) governs: master on/off for notifications, show-on-lock-screen, and per-sender toggles plus an "Edit your quick actions" mode that reorders tiles by drag. That config screen is **E6 Settings** scope; E3 only consumes the persisted values.

#### MDE-Retro design

E3 is **Win10-era only** (gated on `palette::theme() == Theme::Windows10`, added by E0). It is **two cooperating pieces**, both new:

**1. `notifyd` — a freedesktop notification daemon (shared with KDE Connect).**
A new module `mde/src/notifyd.rs`, modeled structurally on `tray.rs` (a background zbus thread + an `Arc<Mutex<…>>` the panel/center read each tick). It claims the well-known name **`org.freedesktop.Notifications`** at `/org/freedesktop/Notifications` and implements the standard interface:
- `Notify(app_name, replaces_id, app_icon, summary, body, actions, hints, expire_timeout) -> u32` — assigns/returns a monotonic id, appends/replaces a record in the shared store, returns the id.
- `CloseNotification(id)` — removes the record, emits `NotificationClosed(id, reason)`.
- `GetCapabilities() -> as` — returns `["body", "actions", "icon-static", "body-markup", "persistence"]`.
- `GetServerInformation() -> (ssss)` — `("MDE Action Center", "MDE-Retro", <crate version>, "1.2")`.
- Signals `NotificationClosed(u32 id, u32 reason)` and `ActionInvoked(u32 id, String action_key)`.

This is the SHARED daemon (D7/D10/D5): any app's `notify-send`, and the **MDE-KDECnt-Rust** crate's KDE Connect bridge (incoming-call / SMS / battery-low notifications), call this one daemon. **Claim is best-effort and non-fatal** (exactly like `tray.rs::serve` retry-then-give-up): if `gnome-shell`/`mako`/another daemon already owns `org.freedesktop.Notifications`, `notifyd::start()` logs and returns an empty store so the panel still runs. Because only one process may own the name, the daemon is **hosted in the long-lived `panel` process** (`panel.rs` calls `notifyd::start()` next to `tray::start()` when the era is Win10), and `action_center.rs` (a separate short-lived process) reads the store **over D-Bus** + the persisted mirror, not in-process — see piece 2.

Store shape: `pub struct Notif { id: u32, app_name: String, app_icon: String, summary: String, body: String, actions: Vec<(String,String)>, hint_urgency: u8, timestamp: SystemTime, transient: bool }` and `pub type Store = Arc<Mutex<Vec<Notif>>>`. Persistence: the store is mirrored to `~/.config/mde/notifications.json` (atomic write via the `state.rs::save` pattern) so history survives the toast process exiting and is readable by the center process; the `persistence` capability is honored by keeping non-`transient` records until cleared. The mirror file also carries a top-level `last_read: SystemTime` read-marker (see the cross-process badge mechanism below).

**Toasts:** when `Notify` lands a record, `notifyd` spawns `mde toast <id>` (a layer-shell surface, see below) unless the urgency hint says `transient`+silent or Focus assist is on. The toast process reads the record from `notifications.json` by id.

**2. `mde/src/action_center.rs` — the slide-in pane + the toast surface.**
A new layer-shell module following `popup.rs`/`menu.rs` exactly (full-screen transparent overlay, `keyboard_interactivity: Exclusive`, Esc/click-outside closes, items run shell commands). Two subcommands routed in `main.rs`:
- `mde action-center` — the right-anchored full-height pane. Layer settings: `anchor: Top | Bottom | Right`, `exclusive_zone: 0`, a fixed-width column (~360px) hugging the right edge inside a full-screen transparent catcher (mirror of `popup.rs`'s bottom-left positioner, flipped right). Built from the **flat Carbon widgets** (`mde_ui::widget::frame`, `groupbox`, `button`) which already render flat under non-Win2000 themes; Win10 styling comes purely from the palette remap (D2). On open it stamps `last_read = now` into `notifications.json` (atomic `save`) so the panel's badge clears (see cross-process mechanism below). Layout top-to-bottom:
  - **History region** (scrollable `iced::widget::scrollable`): notifications grouped by `app_name`. Each group = a header row (app icon via `icons::icon_any`, app name, a group Clear "x" button -> `CloseNotification` for each id in the group) then its notification cards. Each card: icon + summary (bold, `metrics::UI_PX`) + body (`metrics::UI_PX`, theme-secondary), a relative timestamp, a per-card Clear "x" (-> `CloseNotification(id)`), and a caret toggling body truncation when the body exceeds N lines. A card whose record carries `actions` renders those inline action buttons; pressing one invokes `ActionInvoked(id, <action_key>)`. Selecting a card body invokes the default action (`ActionInvoked(id, "default")`) and closes it. Empty state: a centered "No new notifications" label.
  - **"Clear all"** text button at the region's bottom -> closes every id.
  - **Quick-action tile grid** (bottom): a `Row`/`Column` grid of square toggle tiles. Collapsed = first four tiles + an **Expand** link; expanded = full grid + **Collapse**. Tile order + which four are pinned come from `state.rs` (new `quick_actions: Vec<String>` field, `#[serde(default)]`, §2.6). Each tile = an icon + label + on/off accent fill (accent fill = `palette::color(palette::HIGHLIGHT)` when on; flat layer when off), reusing the existing `widget::button` style path.
- `mde toast <id>` — a single transient toast. Layer settings: `anchor: Bottom | Right`, small fixed size (~360x80), `exclusive_zone: 0`, **`keyboard_interactivity: None`** (toasts must not steal focus). Reads the record by id from `notifications.json`, draws icon + summary + body + a Clear "x", auto-dismisses on an `iced::time` timer (`expire_timeout`, default 5s) or on click; clicking the body invokes the default action then exits. Multiple concurrent toasts stack upward (the Nth live toast offsets by height + gap).

**Quick-action tiles -> Linux backends (named, no hand-waving):**
- **Wi-Fi** -> NetworkManager: `nmcli radio wifi on|off` (state read via `nmcli -t radio wifi`).
- **Bluetooth** -> `rfkill block|unblock bluetooth` (state via `rfkill list bluetooth`).
- **Airplane Mode** -> `rfkill block|unblock all`.
- **Brightness** (slider tile) -> `brightnessctl set N%` (read `brightnessctl -m`).
- **Volume** -> `wpctl set-volume @DEFAULT_AUDIO_SINK@ N%` / mute toggle (read `wpctl get-volume`).
- **Night light** -> `wlsunset` (the panel owns the process; the tile flips a flag and (re)spawns/kills it).
- **Focus assist** -> sets the new `state.rs` `focus_assist: bool`; while on, `notifyd` suppresses toast spawning (history still collects). This is the Linux "Do Not Disturb."
- **Nearby sharing** -> launches the KDE Connect share UI (`mde phone` from E9 / the `MDE-KDECnt-Rust` crate) (D5).
- **All settings** tile -> `mde settings` (E6).
The state-read commands run once on view-build (cheap shell calls); toggles spawn the command then exit, exactly like `popup.rs::update` runs an item command.

**Per-era routing (D1, D3):**
- `panel.rs` (E2 owns the button, E3 owns the daemon): under `Theme::Windows10`, `panel::run` calls `notifyd::start()` (alongside `tray::start()`), and the Action Center taskbar button (drawn by E2) launches `mde action-center` and renders the **unread badge**. The badge is computed each tick from the shared store as the count of non-`transient` records whose `timestamp` is newer than the `last_read` marker in `notifications.json`; opening the center stamps `last_read = now`, so on the next tick the panel reads it and the badge falls to 0. Under Win2000/Carbon, `notifyd` is **not** started and no button appears — those eras keep their existing tray-only behavior (additive, D1).
- `main.rs` gains dispatch arms `"action-center" => action_center::run_center(rest)` and `"toast" => action_center::run_toast(rest)`, plus `pub mod notifyd; pub mod action_center;`. Each is reachable and observably works (§3) — no stub arms.

**Theme routing (E0 dependency):** all colors flow through `palette::color()`; `Theme::Windows10` + `win10(rgb)` (added by E0) gives the Win10 accent/light-dark look for free. E3 adds **no raw hex** (§2.1); any new accent/urgency color it needs is added as a role constant in `palette.rs` and remapped in `win10()`/`carbon()`. The toast "urgent" tint reuses the existing `palette::URGENT` role (already remapped to Carbon/Win10 danger red). Every `.size()` uses `metrics::UI_PX`/`INFO_TITLE_PX` (§2.3).

**Win+A keybind:** a labwc `rc.xml` keybind `Super+A` -> `mde action-center` (added by the E0/packaging keybind set, like the existing Start keybind). No compositor patching.

**Reused code (don't duplicate):** `tray.rs` zbus-on-a-thread + shared-`Arc<Mutex>` pattern (notifyd is its sibling); `popup.rs`/`menu.rs` layer-shell overlay scaffolding (namespace/style/update/view, Exclusive keyboard, Esc-close, command-on-click) for both surfaces; `mde_ui::widget` flat Carbon `frame`/`groupbox`/`button` for the cards and tiles; `icons::icon_any` for app + tile icons (missing -> empty `Space`, never tofu, §2.7); `state.rs` for `focus_assist` + `quick_actions` + the `notifications.json` mirror including the `last_read` marker (atomic `save`, §2.6).

#### Boundaries
- The daemon must run in **one** long-lived process: the panel hosts it; the center/toast read the persisted mirror + call `CloseNotification` over D-Bus. This avoids two processes fighting for `org.freedesktop.Notifications`.
- Badge state crosses processes only through the `notifications.json` mirror (`last_read` written by the center, read by the panel) — no private IPC channel.
- KDE Connect integration (D5) means the `MDE-KDECnt-Rust` bridge must target the standard `org.freedesktop.Notifications` name — no MDE-private interface — so it works whether MDE's daemon or a fallback daemon is active.

### E4 Multitasking — Task View, Virtual Desktops, Snap (Win10 era)

**Scope of this section:** the Win10 "Multitasking" chapter (Field Guide pp.14–20): **Task View**, **virtual desktops**, and **Snap / Snap Assist**. ALT+TAB is left to labwc's existing `NextWindow`/`PreviousWindow`. Out of scope and excluded: **Timeline** (Microsoft-account activity history — D9/D10 spirit; no Linux analog and not a pillar), the touchpad four-finger swipe (D9 no-touch), Clipboard history, and Screenshots (separate area). These appear on the source pages but are explicitly dropped.

#### What Windows 10 does (distilled from the source)

- **Task View** is a full-screen overlay invoked by **Win+Tab** or a dedicated taskbar button. It tiles thumbnails of every open window; clicking one focuses it and dismisses the overlay. Esc or a click on empty space closes it. Unlike ALT+TAB it is persistent (stays until you act) and full-screen, not a transient HUD.
- **Virtual desktops** live inside Task View as a band of desktop thumbnails along the **top** of the screen, plus a **"+ New desktop"** affordance in the **top-left**. Per-desktop actions on the source pages: **create** (Win+Ctrl+D), **switch** (click a thumbnail, or Win+Ctrl+Left / Win+Ctrl+Right), **close** (a close box on hover, or Win+Ctrl+F4 — windows fall back to an adjacent desktop), **rename** (right-click → Rename, or click the name to edit inline), **move a window between desktops** (drag a window thumbnail onto a desktop thumbnail), and "show this window on all desktops". Desktops are persistent across reboot/sign-out.
- **Snap** docks a floating window to a screen edge/corner. **Win+Left / Win+Right** = left/right half; following with **Win+Up / Win+Down** lands a quarter (corner), giving up to a 2×2 tile. **Win+Up** maximizes, **Win+Down** restores then minimizes. Dragging a window to an edge/corner with the mouse does the same. Repeatedly pressing Win+Right cycles snapped-right → snapped-left → floating.
- **Snap Assist:** the instant you snap one window to half the screen, Windows presents **thumbnails of the other open windows** filling the empty half; pick one and it snaps into the remaining space. Esc or clicking empty space dismisses Snap Assist without a second pick.

#### MDE-Retro design

**Two backends, cleanly split.** The *window-management mechanics* (snap geometry, desktop switching, ALT+TAB) belong to the compositor — **mde must never become a window manager** and **must never set window geometry, edges, or tiling** (CLAUDE.md §1 boundary). The *visual surfaces* (the Task View grid, the Snap Assist picker) are mde layer-shell overlays driven off `wlr-foreign-toplevel` (the already-built `wlr.rs`), and mde's only window action is **focus/activate + minimize**, exactly what `wlr.rs` already exposes (`Wm::focus`, `Wm::set_minimized` — there is no snap or geometry call, by design). So:

- **Snap = pure labwc keybinds** (`assets/labwc/rc.xml` + the `skel/.config/labwc/rc.xml` mirror that the rpm packages). labwc 0.9.6 has the native `SnapToEdge` action and drag-to-edge `<snapping>`. **All snap geometry runs in labwc; no mde code ever snaps a window.** New binds (Win10-flavored, mirroring the source shortcuts): `W-Left`/`W-Right` → `SnapToEdge` left/right; `W-Down` → `Iconify`. The existing `W-Up` → `ToggleMaximize` and `W-m` → `Iconify` binds in rc.xml are **preserved as-is** (they already cover Win10's Win+Up maximize). Corner snap is labwc's chained edge snap (snap left, then `W-Up`/`W-Down` re-snaps to a corner — labwc already does this when a half-snapped window is snapped vertically). These binds are **era-neutral** (they help Carbon/Win2000 too) so they ship unconditionally; only the **Snap Assist picker** overlay is Win10-gated.

- **Task View = NEW `mde/src/task_view.rs`**, a full-screen transparent layer-shell overlay built on the **`popup.rs`/`menu.rs` pattern** (`iced_layershell` `application`, anchored Top|Bottom|Left|Right, `KeyboardInteractivity::Exclusive`, Esc/click-out → `exit(0)`). Dispatched as **`mde task-view`** (a new arm in `main.rs`, a `mde-task-view` symlink added to the `%post` symlink loop in `mde/Cargo.toml`, and an rc.xml `W-Tab` bind — `A-Tab` stays labwc's `NextWindow`). It calls `wlr::start()` to get the `Wm` snapshot, then lays out one tile per `Window` in a centered responsive grid (`iced::widget` `Column` of `Row`s; tile = window title + app-id icon via `icons::icon_any`, sized with `metrics::UI_PX` plus a new `metrics::TASKVIEW_TILE` constant — no scattered literals per §2.3). Clicking a tile calls `wm.focus(id)` then `exit(0)`. Because foreign-toplevel gives no pixel thumbnails, each tile is **title + app icon on a layer-01 card** (honest: not a fake live preview — §3 no-mockups), which reads correctly as a Win10 "tile" under the flat Carbon look.

- **Virtual desktops** ride the **top band** of the same `task_view.rs` overlay. Backend = **labwc workspaces** (`<desktops>` in rc.xml define N named workspaces; actions `GoToDesktop`, `SendToDesktop`). The band shows one chip per desktop read from the **ext-workspace** protocol (`ext-workspace-v1`); a **"+ New desktop"** chip on the left and a hover **×** to close. Switching/creating/closing is done by mde acting as an `ext-workspace-v1` *client* to `activate`/`create`/`remove` a workspace group (a small `mde/src/workspace.rs` client thread modeled exactly on `wlr.rs`'s background-thread + `Arc<Mutex<Vec<…>>>` snapshot shape — reuse that scaffold, don't reinvent the event loop). mde never moves or resizes windows here; it only drives the ext-workspace manager's own activate/create/remove requests. Keyboard parity binds in rc.xml: `W-C-d` create, `W-C-Left`/`W-C-Right` switch, `W-C-F4` close (these map 1:1 to labwc desktop actions and work with the overlay closed too).

  **ext-workspace RISK + FALLBACK (required):** `ext-workspace-v1` is a recent protocol; a given labwc build (or the `wayland-protocols` version pinned) may not advertise `ext_workspace_manager_v1`. `workspace.rs::start()` returns `Option<Workspaces>` exactly like `wlr::start()` — `None` when the global is absent. **Fallback ladder:** (1) if ext-workspace is present → live chips, full create/switch/close from the overlay; (2) if absent but labwc workspaces are configured in `<desktops>` → the overlay shows a **fixed-count** desktop strip (read from a `state.rs` `virtual_desktops: u8` field, default 4) and switching is done purely by the `W-C-Left/Right` rc.xml binds (the chips become click-to-`GoToDesktop` via labwc, no live state readback — the active chip is tracked locally in the overlay session); (3) if neither → the desktop band is **omitted entirely** and Task View degrades to a single-desktop window grid (still useful). The overlay must render and not panic in all three states (bench check below). No `todo!()`/stub arms — case (3) is a real, shipped layout, not a placeholder.

- **Snap Assist = the same `task_view.rs` overlay in a second mode**, `mde task-view --snap-assist <SIDE>`. **The snap itself is always labwc's** — Snap Assist never makes mde set geometry. The Win10 session's rc.xml routes the half-snap binds through a thin launcher: `W-Left` runs `SnapToEdge` left (labwc, the real snap) **and** then `Execute mde task-view --snap-assist right` (the picker). The overlay shows the window grid clipped to the empty half (a half-screen `container` anchored to that side); picking a tile calls `wm.focus(id)` to activate that window, and the **second snap is performed by labwc** — the rc.xml launcher chains a `SnapToEdge` for the opposite edge after the picker exits (e.g. the picker writes the chosen state and the bind's trailing `SnapToEdge` fires against the now-focused window), so mde requests focus only and labwc owns every edge. Esc / empty-click dismisses (`exit(0)`) with no second snap — matching Win10. This whole Snap-Assist surface is **Win10-only**: it exists only in the win10 rc.xml variant; Carbon/Win2000 sessions keep the bare `SnapToEdge` binds with no picker.

**Era routing & look (D1/D2).** All three surfaces branch off `palette::theme()` like `panel.rs` already does. Under `Theme::Windows10` they use the flat Carbon widgets (reused `frame::flat`/card styling) with the Win10-variant palette/accent; under any other theme the `mde task-view` arm in `main.rs` is still reachable and Task View simply renders the grid in that era's look (so the binary never errors), while the Snap-Assist picker is absent because only the win10 rc.xml routes through it. **Add `Theme::Windows10`** to `palette.rs`'s `Theme` enum and a **`win10(rgb)` remap modeled on `carbon()`** (its own light/dark + accent), wired through the `match theme()` in `color()`. Task View's overlay scrim uses an existing dark role (`BACKGROUND` remapped) at reduced alpha — **no raw hex** anywhere outside `palette.rs` (§2.1).

**Reused code:** `wlr.rs` (`Wm`, `windows()`, `focus()`, `set_minimized()` — the foreign-toplevel snapshot already does everything Task View's grid needs; note it deliberately exposes NO snap/geometry call); `popup.rs`/`menu.rs` (the full-screen transparent layer-shell overlay skeleton, Esc/click-out close, the `mde()` self-path helper); `icons::icon_any` (per-window app icons); `metrics::*` (all sizes); `state.rs` (`virtual_desktops` count for the fallback); `palette::color` (the one theme edge); the `__wlr-list` debug-arm precedent in `main.rs` (the `__ws-list` debug arm mirrors it). New code is intentionally thin: `task_view.rs` (overlay + grid + desktop band + snap-assist mode), `workspace.rs` (ext-workspace client thread, copy of the `wlr.rs` scaffold), a `win10()` remap, one `main.rs` arm (`task-view`) plus the `__ws-list` debug arm, and rc.xml binds. No `mde snap` geometry arm exists — snapping is labwc-only.

### E5 Search + Quick Access (Win10 era · M1)

#### Win10 ground truth (Field Guide pp. 7, 39–42, 89–90)
- **Search (Win+S / WINKEY / type-at-Start).** "By default you access Search using the search box that appears on the taskbar to the right of Start. But you can configure smart search to appear as a button instead… or hide it and simply access this functionality from Start directly: just open Start and start typing." The guide *recommends hiding the box* because Search "works the same without a search box or button". The Search pane opens as a flyout above the taskbar: a single text field plus **filter tabs at the top — All / Apps / Documents / Web / Settings** — that narrow results to one type. Results are a live list that updates as you type; activating a result launches the app, opens the file, opens the Settings page, or hands a Web query to the browser. Cortana is a *separate* round button to the right of Search and is explicitly being deprecated ("we mostly ignore Cortana in this book") — **OUT for E5** (mentioned here only as ground-truth context, never built).
- **Quick Access power-user menu (Win+X / right-click Start).** "The Quick Access menu in Windows works like a hidden power user menu. It provides links to advanced legacy system tools… To display it, type WINKEY + X from anywhere, or right-click the Start button." Listed entries the guide calls out as broadly useful: **System** (PC info / security state), **Windows PowerShell** and **PowerShell (Admin)**, **Task Manager**, **Run** (Win+R), plus the legacy cluster (Device Manager, Disk Management, Power Options, Event Viewer, Network Connections, etc.). It is a flat, single-column dark menu anchored bottom-left, dismiss-on-outside-click — distinct from the cascading Start menu. (Note: this is the *taskbar/Start* Quick Access menu, **not** File Explorer's "Quick access" pinned-folders view, which is separate.)

#### MDE design

**Theme prerequisite (shared with all Win10 epics).** Add `Theme::Windows10` to `palette::Theme` and a `fn win10(rgb) -> Rgb` remap modeled on `carbon()` (D2: a Win10-flavored Carbon variant — reuse the flat widgets, shift palette/accent). `win10()` keeps Carbon's flat `draw_edge` and 2px-radius fills; it shifts the accent to Win10 blue and the surface ramp to the Win10 light/dark greys, honoring `palette::dark()`. `palette::color()` gets a 4th arm `Theme::Windows10 => win10(rgb)` (the packed `THEME` atomic gains value 3). `main.rs` gains a `"windows10" =>` arm in the startup `match st.theme.as_str()` block that calls `set_theme(Theme::Windows10)`. **No raw hex outside palette.rs**; every `.size()` uses `metrics::UI_PX`/`metrics::INFO_TITLE_PX`. This is listed as a dependency, not re-implemented here.

**New surface: `mde/src/search.rs` (`mde search`).** A full-screen transparent **layer-shell** overlay built exactly like `menu.rs`/`popup.rs` (`iced_layershell` `application(namespace, update, view)`, all-edge anchor `Top|Bottom|Left|Right`, `KeyboardInteractivity::Exclusive`, the four font registrations, `Color::TRANSPARENT` style edge via `palette::color`). It draws a Win10 Search flyout **bottom-left above the taskbar** (matching the Win10 anchor; reuse the bottom-left positioning math from `popup.rs::view` — a `Space::Fill` push that floats the panel to the bottom-left corner). Layout top→bottom:
- A **filter tab row** — All / Apps / Documents / Web / Settings — built from `mde_ui::widget::tabs` (already flat under Carbon, so flat under Win10). Selected tab is `palette::HIGHLIGHT`-accented.
- A **single-line query field** (iced `text_input`) auto-focused on map, styled through the palette edge.
- A **scrollable results list** (`scrollable` + `Column` of `button` rows, same `row_style` hover pattern as `popup.rs`), grouped/filtered by the active tab.

Results backends, all real and reachable (no demo data):
- **Apps** — reuse `apps::programs()` (already scans `.desktop` files). Because `programs()` returns `Vec<(String, Vec<App>)>` grouped by category, **flatten the groups** into a single `Vec<App>` first, then match query against `App.name`; activating runs `App.exec` honoring `App.terminal` (same launch path the Start menu uses).
- **Documents/Files** — shell out to `fd`/`find` under `$HOME` (cap results, debounce in `update`); activating opens the hit via `mde files "<parent>"` (reuse `files.rs`) or xdg-open for a file.
- **Settings** — match against a static list of Win10 Settings pages that maps to the era's config surface (D4: `mde settings` replaces Control Panel **in the Win10 era only**); each entry launches `mde settings <page>` (E2 dependency) or, until E2 lands, the existing `mde control-panel`/`mde display`/`mde system-properties` arms as the concrete fallback target.
- **Web** — the bottom "Search the web for '<query>'" row hands the query to **Firefox** (D12: Edge→Firefox): `firefox "https://duckduckgo.com/?q=<urlencoded>"`. This is the only "web" path; no third-party search service is embedded.

`update` keystroke handling: rebuild the filtered result set per character (debounced for the file backend), `Enter` activates the top result, `Esc`/outside-click `exit(0)` (same dismissal contract as `popup.rs`). Per-era guard: `search.rs::run` checks `palette::theme() == Theme::Windows10` and, under any other era, falls through to the existing `mde menu` (Win2000/Carbon search lives in the Start menu's existing `Search ▶` submenu in `menu.rs`) so the binary stays single and the keybind is harmless cross-era.

**Quick Access (Win+X): expand `popup.rs`.** Add a new `items_for("quickaccess")` arm returning the power-user list, reusing `popup.rs`'s existing flat single-column menu, bottom-left layout math, `frame::raised()` (which flattens under Carbon/Win10's `draw_edge`), and dismiss-on-outside-click — no new surface needed. Entries map Win10 tools onto concrete Linux backends:
- **System** → `mde system-properties` (reuse).
- **Device Manager** → `foot sh -c 'lshw -short || lspci'` (or the `mde system-properties` Devices tab when present).
- **Disk Management** → `foot sh -c 'lsblk -f; df -h'`.
- **Power Options** → `mde control-panel` / `mde settings` power page (per era).
- **Event Viewer** → `foot sh -c 'journalctl -e'`.
- **Network Connections** → `foot sh -c 'nmcli device status'` (NetworkManager).
- **Task Manager** → reuse the existing `btop || htop || top` entry already in `popup.rs`.
- **Terminal** / **Terminal (Admin)** → `foot` / `foot sh -c 'pkexec bash'` (the PowerShell/PowerShell-Admin pair; per era a Linux shell, not PowerShell).
- **Run** → `mde run` (reuse `dialogs::run_dialog`).
A `sep()` divides the legacy cluster from System/Run, matching the Win10 grouping.

**Reachability wiring (all 4 eras, harmless cross-era).**
- `main.rs` dispatch: add `"search" => search::run(rest)` arm; the Win+X case flows through `popup::run(["quickaccess"])` once the `items_for("quickaccess")` arm exists.
- labwc `mde/skel/.config/labwc/rc.xml`: bind `W-s` → `mde search` and `W-x` → `mde popup quickaccess` (one `<keybind key="…"><action name="Execute"><command>…</command></action></keybind>` each, matching the existing `W-d → mde menu` keybinds; no compositor patching). Win+X is era-agnostic (Quick Access is useful everywhere); the labwc keybind is unconditional and `search.rs` self-gates by era.
- `%post` symlink: add `mde-search` alongside the existing `mde-<cmd>` symlinks so the basename-dispatch path works too.

**Reused modules:** `popup.rs` (Quick Access host + bottom-left layout math + `row_style`), `menu.rs` (overlay/singleton/font/anchor pattern to clone into `search.rs`; existing `Search ▶` submenu is the cross-era fallback), `apps.rs::programs` (Apps backend; groups flattened), `files.rs` (open-folder target), `dialogs::run_dialog` (Run), `system_properties.rs`/`control_panel.rs`/`display.rs` (System / Settings fallback targets), `mde_ui::{widget::tabs, frame, metrics, palette}` (flat Win10 chrome via the palette edge), `tray.rs`/`icons.rs` (result-row icons via `icons::icon_any`). No new toolkit, no compositor patch.

### E6 Modern Settings app (Win10 era)

**Scope (M1):** a new `mde settings` xdg-toplevel surface — the Win10 "Settings" app — that **replaces the Control Panel in the Windows 10 era only** (D4). Win2000 and Carbon eras keep `mde control-panel` untouched. Target file: **NEW `mde/src/settings.rs`**.

#### What Windows 10 actually does (from the Field Guide, pp.68–89, 129–134, 176–179)

- **Launch surfaces.** Settings gear on the Start left rail, the Start all-apps list, Start search, or **WINKEY + I from anywhere**. It is the single configuration surface in Win10 (the old Control Panel is deprecated/hidden) — but only *in the Win10 era* (D4).
- **Top-level layout.** A landing "home" screen of named **category tiles** in a grid, each a 32px glyph + title + one-line caption: **System, Devices, Phone, Network & Internet, Personalization, Apps, Accounts, Time & Language, Ease of Access, Privacy, Update & Security**. (Gaming is OUT — Xbox/Games are out of scope; the Search category is deferred to its own epic, D3, not rendered here in M1.) A header search box sits above the grid.
- **Drill-in.** Selecting a category swaps to a two-pane view: a **left vertical nav rail** listing that category's pages (e.g. Personalization → Background, Colors, Lock screen, Themes, Start, Taskbar; System → Display, About; Privacy → Windows permissions / App permissions groups), and a **right content pane** of the selected page's controls. A back-arrow (top-left) returns to the home grid; a breadcrumb-ish title sits at the top of the content pane.
- **Controls.** Flat toggles (on/off "switch" pills), radio lists ("Picture / Solid color / Slideshow" for Background; "Light / Dark / Custom" for Colors), dropdowns (Display resolution & scaling — "(Recommended)" tag), sliders, and a swatch grid (accent color). Sections within a page have a bold heading and a thin rule. (Night-light strength and similar gamma sliders are descriptive Win10 detail only — they are NOT M1 deliverables, as MDE has no gamma backend yet.)
- **Era-defining detail.** Colors page = the Light/Dark mode + accent-color picker that drives the *whole* system — this is exactly MDE's `set_dark`/`set_accent` edge. Display page = resolution/scaling (reuses the existing `display.rs`). Devices = Bluetooth/printers. Accounts = sign-in/users. Update & Security = Windows Update.

**Signature shortcut:** **WINKEY + I** opens Settings (the one universally-bound Settings hotkey in Win10).

#### MDE design

**Module & dispatch.** New `mde/src/settings.rs`, mounted as a `mde settings` subcommand: add `"settings" => settings::run(rest)` to the `match cmd` in `main.rs` (alongside `"control-panel"`), an `mde-settings` symlink in the `%post` of `mde/Cargo.toml`'s generate-rpm config, and an `<keybind key="W-i"><action name="Execute"><command>mde settings</command></action></keybind>` in labwc `rc.xml`. It is a normal xdg-toplevel iced window (like `control_panel.rs`/`display.rs`), NOT layer-shell — **labwc draws its frame** (compositor boundary §1).

**Reuse `control_panel.rs`'s shape, not its content.** `settings.rs` mirrors the `control_panel.rs` iced `application(...).run_with(...)` skeleton, the `Message` enum + `update`/`view` split, the `mouse_area` click-catcher overlay pattern, and the per-tool installed-state caching (`fedora::is_installed` once at startup, never in `view`). The category→page tree is a `const` table of `Category { name, glyph_icons, caption, pages: &[Page] }`, and `Page { name, kind: PageKind }` where `PageKind` is an enum routing to one of three things:
1. **Embedded panel** — pages we render natively in iced (Colors).
2. **Reuse an existing mde surface** — `PageKind::Spawn("mde display")`, `PageKind::Spawn("mde system-properties")` (System ▸ Display / About launch the already-built `display.rs` / `system_properties.rs` as child processes, the same way `fedora::TOOLS` row "Display" already shells out to `mde display`).
3. **Drive a Linux backend tool** — `PageKind::Tool(&fedora::Tool)`, reusing the **existing `fedora::TOOLS`** rows (Printers→`system-config-printer`, Network→`nm-connection-editor`, Sounds→`pavucontrol`, Firewall→`firewall-config`, Users→`seahorse`, Date→`timedatectl`), launched/installed exactly as `control_panel.rs` does today (`fedora::launch` / `fedora::install` via `pkexec dnf`). **No backend logic is duplicated** — `settings.rs` is a Win10-styled re-skin over the same `fedora` table the Control Panel uses.

> **M1 page rule (§3 — no stub pages).** A page ships in M1 **only** if it has a real backend on day one: a native iced render (Colors), a `PageKind::Spawn` to an existing `mde` surface, or a `PageKind::Tool` backed by a concrete `fedora::TOOLS` row. Pages whose backend lives in an unfinished epic (Action Center toggles → E7; Your Phone → E-Phone; Windows Update GUI → E5; Power, AutoPlay, VPN, Proxy, Default apps, For-developers → no backend yet) are **NOT** rendered in M1. They are reserved as greyed/"coming in a later milestone" rail entries (consistent with the greyed non-functional toggles in §3 mockups) and become live when their owning epic lands — they do **not** masquerade as working pages.

**Category → Linux backend map (concrete, M1 — live pages only):**
| Category | M1 live pages | Backend | Deferred (greyed, owning epic) |
|---|---|---|---|
| System | Display, About | `mde display` (display.rs/`outputs`, Spawn), `mde system-properties` (sysinfo.rs, Spawn) | Notifications & actions (E7), Power & battery (no backend yet) |
| Devices | Printers | `system-config-printer` (fedora::TOOLS) | Bluetooth (`blueman`/`nm-connection-editor` — add as TOOLS row when wired), Mouse, AutoPlay |
| Phone | — | — | Your Phone = KDE Connect "Mobile Devices" (D5), owned by E-Phone |
| Network & Internet | Status / Wi-Fi | `nm-connection-editor` (fedora::TOOLS) | VPN, Proxy |
| Personalization | Colors, Background, Themes | **`palette::set_dark`/`set_accent` + `state.rs`** (Colors, native); `mde display` Wallpaper (Background, Spawn) | Lock screen / Start / Taskbar (LightDM greeter D13 / panel.rs — later milestone) |
| Apps | — | — | Apps & features / Default apps / Startup (`dnf` E5, `xdg-settings` Edge→Firefox D12) |
| Accounts | Users | `seahorse` (fedora::TOOLS "Users and Passwords") | Your info / Sign-in (LightDM greeter D13) |
| Time & Language | Date & time | `timedatectl` (fedora::TOOLS "Date and Time") | Region, Language (`localectl`) |
| Update & Security | Firewall | `firewall-config` (fedora::TOOLS) | Windows Update (E5), Backup/Recovery |

**Theme / era routing.** `settings.rs` only *renders* under Win10 styling, but it is built theme-agnostically through the existing `palette::color()` edge — no raw hex (§2.1), every `.size()` through `metrics::UI_PX`/`INFO_TITLE_PX` (§2.3). The **per-era routing** lives where the user *reaches* config, not inside settings.rs: the Start menu / panel "Settings gear" and the `mde control-panel` default arm branch on `palette::theme()` — under `Theme::Windows10` they exec `mde settings`; under `Win2000`/`Carbon` they exec `mde control-panel` (D4, "one config surface per era"). This mirrors how `panel.rs`/`menu.rs` already branch on `palette::theme()` for anchors. `mde control-panel` itself stays fully functional for the other eras; nothing is removed (D1 — Carbon stays default, Win10 is opt-in).

**`Theme::Windows10` styling (depends on E1/E2 theme work).** Win10 is a Carbon variant (D2): add `Theme::Windows10` to `palette::Theme` and a `win10(rgb)` remap function modeled on `carbon()` (the existing `match rgb { … }` token table) — Win10 Light = a near-white Gray-10-style field with a saturated blue/your-accent selection; Win10 Dark = the Carbon dark surfaces with the Win10 accent. `is_dark()`/`set_dark()` and `set_accent()` already exist and are reused as-is for the Colors page. The flat Carbon widgets (`button.rs`, `frame.rs` — Carbon `draw_edge` already flattens to a 2px-radius 1px-border fill, §2.5) render the toggles/tiles unchanged; only the palette tokens shift. Settings tiles reuse the flat `mde_ui::button` style; section rules reuse `mde_ui::infoband::rule`; category captions reuse the `infoband` muted text.

**Settings-internal navigation state.** A `View { Home, Category(usize), Page(usize, usize) }` enum on the app state; back-arrow pops one level; the home grid and category rail are two `view()` branches. The Colors page is the one M1 page with live native controls: Light/Dark radios call `palette::set_dark(..)` + persist to `state.rs` (`menu.json`, every field `#[serde(default)]`, §2.6), and a 6-swatch accent grid calls `palette::set_accent(..)` + persists. Because theme load is per-process (§7), the page shows a "Sign out to apply everywhere" info line and re-skins itself live.

**Search box.** The header search filters the flat list of all (category, M1-page) pairs and jumps on click — a pure in-memory filter over the `const` category table, no indexing service (Cortana / Windows Search are OUT).

#### Dependencies & boundaries
- Reuses: `control_panel.rs` (structure), `fedora.rs` `TOOLS`/`launch`/`install`/`is_installed`, `display.rs`+`outputs` (Display/Background), `system_properties.rs`+`sysinfo.rs` (About), `state.rs` (persist theme/accent), `icons::icon_any`, `mde_ui::{button,frame,infoband,metrics,palette}`.
- Out of scope here, owned by other epics: Action Center toggles (E7), Your Phone = KDE Connect Mobile Devices (D5, E-Phone), lock/greeter (E-Lock/D13), `Theme::Windows10` palette tokens (E1/E2), Windows Update `dnf` GUI (E5), the Search pillar (D3). settings.rs *reserves greyed rail entries for* these but does **not** render them as working pages until the owning epic lands — every page rendered in M1 is reachable and non-stub on day one (§3).
- Globally out of scope (never a feature): Gaming/Xbox, touch/tablet/Ink, Store, Mail, Skype, Calendar, OneNote, People, Maps, Groove, Movies & TV.

### E7 Personalization — Settings ▸ Personalization (Win10 era)

#### Win10 behaviors distilled (from the Personalize chapter, printed pp. 68–92)

The Settings app (WINKEY+I) is the single config surface; its **Personalization** section is a left **category rail** with the pages **Background · Colors · Lock screen · Themes · Start · Taskbar**. Each page is a flat, scrolling stack of headered control groups with a large live **preview** at the top. Specific behaviors:

- **Background.** A "Background" dropdown chooses **Picture / Solid color / Slideshow**. Picture mode shows a recent-images thumbnail strip + **Browse**; a "Choose a fit" dropdown (Fill / Fit / Stretch / Tile / Center / Span). Slideshow picks an album folder + a change interval. A monitor-shaped preview reflects the choice.
- **Colors.** A "Choose your color" dropdown = **Light / Dark / Custom**; Custom splits into "default Windows mode" (system UI) and "default app mode" (windows), each Light/Dark. An **accent color** grid (swatches) with an **"Automatically pick an accent color from my background"** toggle. Two checkboxes: "Show accent color on Start, taskbar, and action center" and "...on title bars and window borders". A transparency-effects toggle.
- **Lock screen.** Background = **Windows Spotlight / Picture / Slideshow** (Spotlight = a daily image; we drop the Bing/ad fetch per OUT-OF-SCOPE). A picture thumbnail strip + Browse. Toggles: show lock-screen background on the sign-in screen; show notifications on the lock screen. The lock screen is "the first GUI you see on power-on/wake" — date/time + background.
- **Themes.** A theme = **{background, accent color, sound scheme, cursor}** bundled. A gallery of saved theme tiles; selecting one applies the whole bundle; **Save theme**. ("Get more themes in Store" is OUT.)
- **Start.** Toggles: **Show more tiles** (3→4 medium wide), **Use Start full screen**, show recently-added/most-used apps, show recently-opened items in jump lists, and **"Choose which folders appear on Start"** (Documents, Downloads, Pictures, Settings, File Explorer, …).
- **Taskbar.** Toggles: lock the taskbar, auto-hide, use small taskbar buttons, **taskbar location on screen** (Bottom/Top/Left/Right), combine buttons, and the **notification-area** sub-pages "Select which icons appear on the taskbar" / "Turn system icons on or off", plus Search = box/button/hidden and Show Task View button. (Cortana toggle is OUT.)

Signature: **WINKEY+I** opens Settings; right-click desktop ▸ "Personalize" jumps straight to this section; right-click taskbar ▸ "Taskbar settings" jumps to the Taskbar page.

#### MDE design

**Locked decision D4:** in the Win10 era the **modern Settings app replaces the Control Panel**. So Personalization is **not** a standalone applet — it is the `Personalization` section of a new unified `mde settings` surface (owned by epic E4 Settings; E7 builds the Personalization pages and the shared category-rail shell *if E4 has not yet landed it*, see Dependencies). Win2000/Carbon eras keep `mde display`/`mde control-panel` untouched.

**Module:** new `rust/mde/src/settings/personalization.rs` (a sub-view of the `settings` module), plus a `personalization` dispatch shim so the page is independently launchable for bench/grim. Reuse, do not duplicate `display.rs`:
- The **monitor preview** (`monitor_graphic`/`screen_preview`), wallpaper scanning (`scan_wallpapers`), `Browse` via `mde filedialog`, and `BgMode↔swaybg` mapping are lifted out of `display.rs` into a shared `crate::wallpaper` helper module and consumed by both the legacy Display applet and the new Personalization ▸ Background page (no copy-paste).
- Apply path drives the **same Linux backends** already wired in `outputs.rs`: wallpaper = `swaybg` (`outputs::apply_wallpaper` / the generated `wallpaper.sh` in `~/.config/mde/`), persisted via `outputs::persist`. Colors/accent/mode = `state.rs` (`theme`, `theme_mode`, `icon_color`) + `apply_appearance`'s shell-restart + labwc-themerc rewrite (lifted from `display.rs`). Lock screen = `lightdm-gtk-greeter` config (see below).

**Fit modes (reuse-exact):** the "Choose a fit" dropdown is the existing `BgMode` enum reused verbatim — **Center / Tile / Stretch / Fit / Fill** (the five `BgMode::ALL` values, each mapping 1:1 to a real `swaybg -m` mode). Win10's "Span" (one image across multiple monitors) has **no swaybg equivalent**, so it is **dropped** (selecting a would-be Span degrades to Fill); we do not invent a backend.

**Per-era routing (off `palette::theme()`):** `mde settings` only registers the Personalization section's Win10 layout when `palette::theme() == Theme::Windows10`. The desktop/taskbar right-click context menus (`menu.rs`/`panel.rs`) branch the same way: under Windows10 they emit **"Personalize" → `mde settings personalization`** and **"Taskbar settings" → `mde settings personalization --page taskbar`**; under Carbon/Win2000 they keep emitting "Properties"/"Display Properties" → `mde display`. This mirrors the existing `palette::is_carbon()` branches already in `panel.rs:134,362`.

**`Theme::Windows10` styling (D1/D2):** Personalization renders with the flat Carbon widgets (`group_box`, flat `draw_edge`, `sunken_picklist`, `checkbox_style`) — no 3D bevels — so it inherits the Win10-flavored Carbon look for free. The new `palette::win10(rgb)` remap (added alongside `carbon()`, modeled on it; see E2 Theme epic) supplies the cooler Win10 grays + the **user-selectable accent** (Win10 ships a swatch grid, not a single Blue 60). Because the existing `set_accent` only tints **icons** (the Carbon UI accent is hardwired to Blue 60), the user accent is carried by a **new** settable value that the `win10()` remap reads when producing the accent role under `Theme::Windows10`: the Colors page writes `state.win10_accent`, `main.rs` loads it into this new accent slot, and because every accent consumer still resolves through the single `palette::color()` edge, selection/focus/Start-tile/taskbar-highlight all retint **with no call-site changes** — only the `win10()` remap itself reads the new slot. Light/Dark + Custom (system vs app mode) map onto the existing `theme_mode` plus a new `app_mode` state field, both consumed by `set_dark`.

**Theme bundle (scope):** the MDE Themes page bundles only the parts with a real backend — **{background (swaybg), accent (`win10_accent`), light/dark mode}**. Win10's "sound scheme" and "cursor" are **dropped** (MDE has no sound-scheme or cursor backend); this matches the worklist E7.7 bundle and avoids a stub.

**Lock screen backend (D13):** the Lock screen page writes `lightdm-gtk-greeter.conf` (`[greeter] background=`, and a notification toggle stored in `state`) under `/etc/lightdm/` via a pkexec'd helper (the greeter is already a packaged dependency — `catalogue.rs:38`, `tui_setup.rs`). "Windows Spotlight" degrades to a **local** rotating wallpaper from the bundled `~/.local/share/mde/wallpapers` set (NO network/Bing fetch — OUT-OF-SCOPE ads). The sign-in-background toggle just mirrors the desktop wallpaper into the greeter config.

**New state fields** (all `#[serde(default)]`, with a `parse("{}")`-agreeing manual default, per §2.6): `win10_accent: String` (named swatch, default "blue"), `app_mode: String` ("light"/"dark"), `bg_kind: String` ("picture"/"solid"/"slideshow"), `lock_bg: String`, `lock_notifications: bool`, `taskbar_location: String` ("bottom"/"top"/"left"/"right" — feeds the per-theme layer anchor in `panel.rs`), `start_full_screen: bool`, `start_more_tiles: bool`. Taskbar-location persistence reuses the panel anchor machinery already keyed per-theme in `panel.rs` (Carbon top / Win2000 bottom / BeOS left) — Win10 defaults to bottom and the page lets the user move it, applied by a shell restart.

**Reused code:** `display.rs` preview + wallpaper scan + `filedialog` Browse + `outputs.rs` swaybg/persist + `apply_appearance`/`set_labwc_title_colors`/`restart_shell` + `state.rs` + `taskbar_properties.rs` (existing small-icons/greyed-toggle wiring, reused by the Taskbar page) + `mde_ui::{group_box,button,frame,checkbox_style,sunken_picklist,scrollbar}` + `metrics::UI_PX`. New code is the category-rail layout, the accent-swatch grid, the Colors/Themes/Start/Taskbar/Lock pages, the `win10()` remap edge (incl. its new settable accent slot), and the lightdm-greeter writer.

### E19 Power / Session (Shutdown · Restart · Sleep · Lock · Sign-Off)

**Source pages (printed):** "Shutdown, Restart, Sleep, Lock and Sign-Off" p.38, "Sign-in as a different user" / Lock & Switch User p.190–191, "Customize the lock screen and sign-in screen" p.92.

#### Win10 behaviors distilled
Windows 10 splits session control across two Start anchors:
- **Power button** (bottom-left of Start, above the user tile in the left rail): a small flyout listing **Sleep · Shut down · Restart** (and Hibernate when the firmware exposes it). "The power management options you see here will vary by PC" — items are conditional on capability, not always all four.
- **User tile / account name** (bottom-left of Start): a flyout listing **Lock · Sign out · Switch user** plus the names of other configured accounts. Selecting another account name raises that account's sign-in screen while leaving the current session signed in (Win10 supports multiple simultaneously signed-in users, one interactive at a time).
- **Signature shortcut:** **WINKEY + L** locks immediately (documented on p.191). The Quick Access menu (WINKEY + X, right-click Start) also exposes "Shut down or sign out" as a submenu.
- **Lock screen** is the first surface seen on power-on / wake / lock: background image, date/time, and notification glances; PIN/password unlock raises the sign-in screen. Customized in Settings > Personalization > Lock screen (out of scope here — owned by E15 Personalization).

States per action: Sleep → suspend-to-RAM (wakes to lock screen); Shut down / Restart → full power transition; Lock → blank to lock surface, session preserved; Sign out → session torn down, greeter shows account list.

#### MDE design

**Module:** `mde/src/dialogs.rs` (extend the existing file — it already hosts `logoff()`, `shutdown()`, and `do_shutdown()` which maps `ShutDown→systemctl poweroff`, `Restart→systemctl reboot`, `StandBy→systemctl suspend`, `LogOff→labwc --exit`). We add **Lock** and a **Win10-era power flyout**, and route the existing dialogs per era.

**Per-era routing (off `palette::theme()`):**
- `Theme::Win2000` / `Theme::Beos` / `Theme::Carbon`: keep today's fixed-window `mde shutdown` dropdown dialog and `mde logoff` confirm dialog unchanged. (Carbon already restyles them via the `palette::color` edge — no call-site change.)
- `Theme::Windows10` (new 4th era, D1): `mde shutdown` renders the **Win10 power flyout** — a small layer-shell-free fixed window listing flat full-width rows **Sleep · Shut down · Restart**, each a `mde_ui::button` styled flat by the new `win10(rgb)` remap. `mde logoff` renders the **Win10 account flyout** — rows **Lock · Sign out**, styled identically. Both branch inside `shutdown_view`/`logoff_view` on `palette::is_windows10()` (new predicate mirroring `is_carbon()`), reusing the same `Shutdown`/`update` state machine and the same `do_shutdown()` backend. No new iced application boilerplate.

**New action — Lock.** Add `Choice::Lock` to the `Choice` enum and a `do_lock()` that shells `loginctl lock-session` (the systemd-logind canonical lock signal), with the same status-checked exit pattern as `do_shutdown` — a failed lock (ENOENT, no session) must not exit 0. The logind `Lock` signal is caught by the **session's screen locker** — in this repo that is `swaylock`, already shipped (catalogue `SESSION` group) and wired into the idle path in `outputs.rs` (swayidle → `swaylock`); WINKEY+L thus raises the swaylock lock surface, session preserved. (LightDM-gtk-greeter is NOT involved in lock — it is the login/switch-user greeter; see the table.) New subcommand **`mde lock`** (main.rs dispatch arm → `dialogs::lock()` which calls `do_lock()` directly, no window) so WINKEY+L can bind straight to it via labwc `rc.xml`. The existing `StandBy`/suspend path is surfaced as the Win10 "Sleep" row (same `systemctl suspend` backend) — no new backend.

**`Theme::Windows10` styling.** Implemented entirely through the single theme edge: add `Theme::Windows10` to `palette::Theme`, a `win10(rgb)` remap modeled on `carbon()` (Win10 reuses flat Carbon widgets per D2 — start by delegating to `carbon(rgb)` then shifting the accent to Win10 system-accent blue and the surface to the Win10 flyout charcoal), and wire THEME atomic value `3`. `dialogs.rs` needs zero color literals (§2.1) — it already paints via `palette::color(palette::MENU)` for the body and `mde_ui::button` for rows. The flat full-width row look falls out of the Carbon `draw_edge` flatten path already reused in Win10.

**Linux backend mapping (named, concrete):**
| Win10 action | MDE backend |
|---|---|
| Sleep | `systemctl suspend` (already wired as StandBy) |
| Shut down | `systemctl poweroff` (already wired) |
| Restart | `systemctl reboot` (already wired) |
| Sign out | `labwc --exit` (already wired as LogOff) — session torn down; LightDM-gtk-greeter shows the account list (D13) |
| Lock + WINKEY+L | `loginctl lock-session` → logind `Lock` signal → the session locker (`swaylock`, already shipped) raises the lock surface; session preserved |
| Switch user | `labwc --exit` for P0 (Sign out → LightDM greeter's account list is the switch surface); a dedicated in-session picker is future work, recorded as a follow-up |

**Reused code:** `dialogs.rs` `silver()` body, `M`/`Choice` state machine, `do_shutdown` status-checked exec, `key_subscription`/`is_enter`/`is_escape` (Enter=default, Esc=cancel — matches Win10 flyout dismissal), `mde_ui::button`, `metrics::UI_PX` for every `.size()`, `palette::color`/`MENU`. Lock locker chain: `swaylock` already in catalogue `SESSION` + `outputs.rs` idle script. Panel/Start integration: the Win10 Start left rail (E17) calls `mde shutdown` (power tile) and `mde logoff` (user tile) — same subcommands, era-routed.

**Packaging:** `mde-lock` %post symlink + `WINKEY+L` keybind in `rc.xml` (no compositor patch). `mde lock` delegates to the existing `swaylock` locker (already in the catalogue and the swayidle idle path), so no new locker dependency is added — swaylock remains the canonical on-lock screen locker regardless of LightDM, which owns only the login/switch-user greeter.

### E8 File Explorer + cloud-as-devices

#### Win10 behavior distilled from the Field Guide (pp. 93–104, "Files and Storage")

File Explorer in Windows 10 is "the same file management application since 1995" re-skinned with a **ribbon**:

- **Ribbon command bar.** Tabs **Home / Share / View** (plus context tabs that appear only when relevant). The ribbon is **collapsed by default** ("svelte and minimalistic"); clicking a tab temporarily shows it; the **Expand the Ribbon caret** (top-right) or **Ctrl+F1** keeps it pinned. Commands are context-sensitive (e.g. a *Share* button only when files are selected).
- **Quick access is the default landing view** (instead of *This PC*). It is document-centric and dynamic: a **Frequent folders** section (folders) + a **Recent files** section (documents), populated automatically as you open things on the PC *or local network*. It also holds **pinned items** (Desktop, Documents, Downloads, Pictures auto-pinned). Right-click → **Unpin from Quick access** / **Remove from Quick access**.
- **Folder Options** (View ▸ Options ▸ *Change folder and search options*, General tab): "Open File Explorer to:" → **Quick access / This PC**; Privacy section toggles for "Show recently used files" / "Show frequently used folders". View tab has a "Show sync provider notifications" toggle (the OneDrive advertising the book tells you to disable).
- **This PC** (was My Computer/Computer/My PC): drives + the user folders.
- **Network view**: navigation-pane "Network" node; network discovery + file sharing must be turned on to make the PC visible and to browse other machines' shares ("Network discovery and file sharing are turned off… Click to change").
- **Share** (Share tab → Share button) opens a floating share pane: share *with people* (frequent contacts) or *by app*.
- **OneDrive / Files On-Demand** is the "cloud" surface: a top-level **OneDrive** node in the navigation pane, a special **Status column** with four cloud states — *Always keep on this device* (synced/offline), *Available on this device* (downloaded by use, offline), *Available when online* (blue-outlined cloud, placeholder, online-only), and *Syncing/Sync pending*. Right-click → **Always keep on this device** / **Free up space** drives sync.
- **WinKey+E** opens a new Explorer window; **Shift+right-click a folder → Open in new window**; drag-drop between windows. Details view sorts/groups by column.

Per the locked decisions: **D10 maps "cloud" off OneDrive onto the user's KDE Connect-paired devices** (remote-filesystem browse/sync), **not a third-party cloud**. Share-with-people/by-app, Personal Vault, and OneDrive proper are OUT (D10/scope).

#### MDE-Retro design

**Module:** extend the existing `mde/src/files.rs` (the `mde files` xdg-toplevel). No new subcommand for the window itself; one new dispatch verb (`mde files <path>` already exists) plus an internal mount-helper subcommand (below). Reuse `wlr.rs`-adjacent process model: each launch re-reads `menu.json` and sets the palette (CLAUDE.md §7).

**Era routing — branch off `palette::theme()` the same way `panel.rs` branches on `palette::is_carbon()` (near line 134).** Both helpers read the same packed theme atomic. `files.rs` keeps ONE widget tree; the *chrome it renders* and the *default landing view* switch on the era:

- `Theme::Win2000` / `Theme::Beos` / `Theme::Carbon` → the existing menubar (`File/Edit/View/Favorites/Tools/Help`) + raised toolbar + Address bar + sunken details list + web-view/tree left pane. Unchanged. Landing = home dir.
- `Theme::Windows10` → a **flat command bar** replaces the menubar+toolbar (a `fn command_bar()` rendered when `palette::theme()==Theme::Windows10`): a left cluster of flat buttons (New folder, Cut, Copy, Paste, Delete, Rename, Properties) reusing the **flat Carbon `mde_ui::button`** (D2: Win10 is a Carbon variant — no new widget chrome), a center **breadcrumb address bar** (clickable path segments) reusing the existing `address_bar` `text_input` styled with `mde_ui::sunken_field`, and a right **search-in-folder** box. Landing = **Quick access** (see below). The ribbon's *tabbed* form is intentionally distilled to a single always-visible flat command row — collapsing/Ctrl+F1 toggling is a non-load-bearing flourish we drop (record in commit body), keeping the §3 "no mockup" bar.

**`Theme::Windows10` palette (E1 dependency).** E8 consumes the new `win10(rgb)` remap added to `palette.rs` (modeled on `carbon()`, registered in `color()` alongside `Theme::Carbon`, with the Win10 enum variant added to `Theme` and `theme()`'s match) **and the new `"windows10"` value in `state.rs`'s `theme` field handled by `main.rs`'s `set_theme` startup match** (so the era is opt-in via persisted state, never an env var — D1). E8 adds NO hex — every color it draws goes through existing role constants (`WINDOW`, `MENU`, `HIGHLIGHT`, `WINDOW_TEXT`, `INFO_BAND`) so the new selection accent (Win10 blue), light/dark field surfaces, and flat selection band fall out of `win10()` for free (§2.1). New `.size()` uses existing `metrics::UI_PX`/`INFO_TITLE_PX` (§2.3).

**Quick access (the Win10 default landing view).** A new in-`files.rs` virtual view (`enum Pane { QuickAccess, ThisPc, Folder(PathBuf), Network, CloudDevice(DeviceId) }`) selected by the navigation pane and persisted via a new `#[serde(default)]` field `explorer_landing: "quick"|"thispc"` on `state.rs` (§2.6 — default fn + `parse("{}")` agreement test). Quick access renders two sections built from real data, no demo constants (§3):
- **Frequent folders** = pinned set (`~/Desktop`, `~/Documents`, `~/Downloads`, `~/Pictures` if they exist, auto-pinned) plus a persisted user pin list (`explorer_pins: Vec<PathBuf>` in `state.rs`). Right-click → **Pin to Quick access / Unpin from Quick access** edits the persisted list (reuses the existing `context_menu` + `RowContext` machinery, two new `Message` arms).
- **Recent files** = the most-recently-modified files under the pinned folders, read live via `std::fs` mtime (the existing `load()` IO path), capped (e.g. 20). No history daemon — "recent" is derived, not tracked, so it works on first launch.
The Folder Options "Open File Explorer to" choice maps to `explorer_landing`; the privacy toggles map to whether Recent files renders. (This sub-pane lives behind View on the command bar; under non-Win10 eras the field is simply ignored.)

**This PC + Network + SMB shares.** `Pane::ThisPc` lists drive roots from `/` + mounted volumes parsed from `/proc/mounts` (filtered to real block/removable mounts) plus the user folders — reusing `subdirs()` / icon resolution. `Pane::Network` mirrors the Win10 Network node: enumerate SMB hosts via the system browser. Backend: shell out to **`gio mount -l`** / **`smbclient -L`** for discovery and **`gio mount smb://host/share`** (GVfs) to mount, then navigate into the GVfs FUSE path (`/run/user/$UID/gvfs/...`) using the *existing* `navigate()`/`load()` — so once mounted, an SMB share is just a folder to the rest of `files.rs`. Discovery failures surface in the existing `status_bar` error slot (no panic, no silent empty list — matches the `load()` error handling already there). This is the concrete Linux mapping for Win10's "turn on network discovery and browse shares".

**Cloud Files = KDE Connect device endpoints (D5/D10).** A `Pane::CloudDevice(DeviceId)` node group ("This PC" siblings labelled by paired-device name, cloud-glyph icon) lists the user's **paired KDE Connect devices** (from `~/.config/mde/connect/` pairing store, the shared crate's on-disk store). Selecting a device **remote-browses it over sftp**: MDE-Retro mounts the device's exported filesystem with **`sshfs`** (or `gio mount sftp://`) to a per-device path under `/run/user/$UID/mde-cloud/<device>/`, then navigates in via the existing folder machinery. Sync mirrors Win10's "Always keep on this device" / "Free up space": right-click a remote entry → **Make available offline** copies it down (`std::fs::copy`, the existing `paste()` copy path) into a local mirror dir; **Free up space** deletes the local copy. A lightweight **Status column** (Win10's four cloud states) renders per remote entry when in a `CloudDevice` pane: *Online-only* (remote, not mirrored), *Available offline* (mirrored), *Syncing* (copy in flight). No placeholder/Files-On-Demand virtualization — status is computed from whether a local mirror file exists, so it is real and observable.

**Shared-crate integration.** Device discovery/pairing comes from `MDE-KDECnt-Rust` (`mde-kdc-proto` + its host layer when landed); E8 consumes the pairing store + device list and the share/run-command plugins. The actual byte transport for browse/sync is **sshfs/sftp over the paired link** (matching the roadmap's "remote sftp"), keeping the protocol crate's job to discovery/pairing/capabilities, not file bytes. If the host layer isn't ready, E8 degrades to reading the on-disk pairing store directly (graceful fallback, D8 spirit) so the pane still lists devices and mounts.

**Mount helper subcommand.** Because mounting (gio/sshfs) is privileged-adjacent and slow, add `mde mount <uri>` — a thin dispatch arm in `main.rs` (alongside `files`/`properties`) that performs the gio/sshfs mount and prints the resulting path, so `files.rs` spawns it (`Command::new(current_exe).arg("mount")…`, the same pattern `CtxProperties` already uses to relaunch `mde properties`) and navigates into the result. Keeps `files.rs` non-blocking and the mount logic testable in isolation.

**Reused code (no duplication, §2.7):** `navigate()`/`load()`/`sort_entries()`/`activate()`/`paste()`/`trash()` (all IO + selection); `context_menu`/`command_menu`/`RowContext`/`CloseCtx` (right-click pin/sync); `icons::icon_any` + `icon_names` (folder/drive/cloud glyphs, extended with `"folder-remote"`/`"network-server"`/`"folder-cloud"` candidates); `frame::raised`/`sunken`, `mde_ui::button`, `mde_ui::sunken_field`, `mde_ui::scrollbar`; `status_bar` for errors; `state.rs` for the new persisted fields. The breadcrumb and command bar are new `view`-helper fns gated on `palette::theme()==Theme::Windows10`, not new modules.

**Backend summary (named, per the architecture rule):** GVfs/`gio mount` + `smbclient` (SMB shares + Network node), `/proc/mounts` (This PC drives), `sshfs`/`gio mount sftp://` (Cloud Files browse), `std::fs::copy` (make-available-offline sync), `xdg-open` (file open, already wired), `MDE-KDECnt-Rust` pairing store (device list).

> **Driving a Win10 capture (§7).** There is no theme env override; the era is selected from the `theme` field in `~/.config/mde/menu.json` (`state.rs::config_path`, honors `XDG_CONFIG_HOME`). To render the Win10 chrome in the harness, point `XDG_CONFIG_HOME` at a temp dir whose `mde/menu.json` has `"theme":"windows10"`, then launch `mde files` there. Carbon/Win2000 parity captures use `"carbon"`/`"win2000"` the same way.

### E9 Your Phone — unified KDE Connect surface (Mobile Devices / D5)

> Milestone **M2**. New module `mde/src/phone.rs` (the `mde phone` subcommand) plus shared backend `mde/src/connect.rs`. Hard deps: **E3** (`notifyd` notification daemon, shared bus) and the **`MDE-KDECnt-Rust`** crate (LAN transport + sftp + telephony/SMS plugins). This module **is** the D5 "Mobile Devices" surface across all eras; only its chrome/labels route per era.

#### What Windows 10 actually does (distilled from the Field Guide, "Phone" chapter, printed pp.149-167)

The **Your Phone** app links one or more paired handsets to the PC over a companion app, and exposes the phone's features as **views selected from a left navigation pane**:

- **Notifications** — a live stream of the phone's notifications as they arrive; each row has a **Dismiss ("X")** button and a **"Clear all"** link; a **"Customize"** link jumps to per-app notification settings. Clearing on the phone clears here and vice-versa; only notifications arriving *after* the feature is enabled appear. A new phone notification raises a **standard Windows notification banner**; selecting it opens this view, and missed ones land in **Action Center**.
- **Messages** — a two-pane SMS view: a **Recent messages** list on the left, the selected **conversation thread** on the right. **"+ New Message"** opens a compose row with **contact auto-complete** recipients; a **"Send a message"** field + **Enter / Send button** sends. Inbound SMS raises a notification banner you can **reply to inline**. No MMS/multimedia attach from the PC.
- **Photos** — a grid of the **25 most recent** camera photos. Selecting one opens it inline with **Back ("<") / Forward (">")** navigation (also left/right arrow keys) and per-photo actions: **Open** (in Photos), **Open with…**, **Copy** (to clipboard), **Save as…** (to PC), **Share**, **Delete** (deletes on the phone). Same actions on right-click in the grid.
- **Calls** — a center list of **recent calls** (sent + received); selecting one reveals **call / text** icons. A **rightmost pane** searches a contact or accepts a dialed number to **place a call through the phone using the PC's mic/speakers**. An in-call **notification window** (expandable: **Mute / Keypad / Use phone**) runs for the duration. **Inbound calls** raise a large notification with **Answer / Decline / reply-by-text**. You cannot edit the recent-calls list.
- **Apps** (Samsung-only screen mirroring) — **OUT for MDE** (no remote-input / screen-mirror; matches the "remote-input deferred" worklist note).
- **Settings** (from the nav pane): **General** (badge / banner display), **My Devices** (pick the primary among multiple paired phones; **"…" › Deactivate** unlinks), **Features** (per-function toggles), **Personalization** (show the phone's **wallpaper** in the nav pane, audio-player display). A **jump list** on the taskbar/Start icon gives quick access to each view.
- **Linking**: Settings (WIN+I) › Phone › **Add a phone**; install the companion app, sign in / scan a **QR code**, answer prompts; the nav pane is then **tinted with the phone's wallpaper**.

Signature interactions: **left nav switches views**; **Enter / Send** in Messages; **left/right arrows** or **< / >** to page photos; **Dismiss X / Clear all** in Notifications; **Answer / Decline / text** on an inbound-call toast; inbound SMS/notification/call all raise **toasts that also persist in Action Center**.

#### MDE design

**Module split.** A single new shell module pair:
- **`mde/src/connect.rs`** — the era-agnostic KDE Connect backend. A long-lived **`mde connect` user daemon** (already scoped in the Mobile-Devices backlog) owns the `MDE-KDECnt-Rust` host (LAN transport, pairing, plugin event stream). `connect.rs` is the in-process client: it speaks to the daemon over the project bus (zbus, the same `tray.rs`/`notifyd` pattern — `zbus::blocking::Connection` on a bg thread, results pushed to the iced `update` via a channel/`subscription`). It exposes typed accessors: `devices()`, `notifications(dev)`, `sms_threads(dev)` / `sms_send(dev, addr, body)`, `recent_photos(dev) -> sftp paths`, `call_log(dev)` / `call_place(dev, number)` / `call_answer` / `call_hangup`, and per-feature enable toggles. This is built **once** and reused by both E9 and the E8 "cloud-as-devices" file endpoints.
- **`mde/src/phone.rs`** — the `mde phone` **xdg-toplevel** window (not layer-shell; it is an app window like `files.rs`, so labwc draws its title bar — compositor boundary preserved). Dispatch arm in `main.rs` (`"phone" => phone::run(rest)`), a `%post` `mde-phone` symlink (CLAUDE.md §4), and a labwc `rc.xml` keybind. `mde phone --view=messages|photos|calls|notifications` and a `--device=<id>` deep-link both views and the jump list.

**Layout (reuses `files.rs` patterns wholesale).** A three-region window mirroring `files.rs`'s nav-tree + list + content split:
- **Left nav rail** — device picker at top (multiple paired phones, primary highlighted; this is "My Devices"), then the view list **Notifications / Messages / Photos / Calls / Settings**, built from `icons::icon_any` rows with the existing `row_style(selected)` from `files.rs` (lift the helper into a shared spot or reuse via `crate::files::row_style`). Under Win10 + Personalization-on, the rail is tinted with the device wallpaper (an `image()` background behind the rail, fetched once over sftp).
- **Content pane** — switches on the selected `View` enum:
  - *Notifications*: a `scrollable` Column of notification cards (app icon via `icon_any`, title, body, timestamp), each with a **Dismiss** button (`connect::notif_dismiss`) and a header **"Clear all"** (`connect::notif_clear`). Fed by the **same `notifyd` history store** (E3) filtered to KDE-Connect-origin notifications, so phone notifications also appear in Action Center for free.
  - *Messages*: reuse the two-pane idiom — `Recent` thread list (left sub-column) + selected-thread bubble Column (right) with a `text_input` compose field bound to **Enter** → `connect::sms_send`; **"+ New Message"** opens a recipient `text_input` with contact auto-complete from the KDE Connect contacts plugin (recipient lookup only — not a standalone People surface, D12). Inbound SMS arrives as a `notifyd` toast carrying a **reply action** (notification "actions" capability) that calls `sms_send`.
  - *Photos*: a wrapped grid of the 25 most-recent thumbnails (sftp-fetched into the OS cache; `iced::widget::image`). Selecting one opens an inline viewer with **< / >** buttons and **Left/Right arrow** key handling (the same `Key::Named(ArrowLeft/Right)` pattern `files.rs` already uses), and a per-photo action row: **Open** (spawn `xdg-open`), **Save as…** (reuse `filedialog.rs`), **Copy** (wl-clipboard), **Delete** (`connect::sftp_delete`). Right-click reuses the `files.rs` context-menu builder.
  - *Calls*: a recent-calls Column (direction icon + number/contact + time, fed by `connect::call_log`) and a right sub-pane with a dial `text_input` + Call button → `connect::call_place`. Inbound call → a `notifyd` toast with **Answer / Decline / Text** actions (`call_answer` / `call_hangup` / open Messages compose). Audio routes through the phone (telephony plugin); the PC-mic bridge is out of LAN-only scope, so the toast offers **"Use phone"** (hand off to the handset) as the default action.
  - *Settings*: General (badge/banner toggles → persisted in `state.rs`), My Devices (primary picker + **Deactivate**/unpair → `connect::unpair`), Features (per-plugin enable toggles → KDE Connect plugin config), Personalization (wallpaper-in-rail toggle).

**Per-era routing (off `palette::theme()`, like `panel.rs`).** One module, three skins — the *backend and views are identical*; only chrome/labels/affordances branch:
- **`Theme::Windows10`** → titled **"Your Phone"**, accent-tinted nav selection, optional wallpaper-tinted rail, toast styling from E3. Reachable from tiled Start (E1), the taskbar **jump list** (E2 — `mde phone --view=…` entries), and a `rc.xml` keybind. (Win10 era has no Control Panel, D4 — entry is Start/jump-list/keybind only.)
- **`Theme::Win2000` / `Theme::Carbon`** → titled **"Mobile Devices"**, no wallpaper rail, plain selection; same views. Reachable from the existing **Control Panel** (`control_panel.rs` gains a "Mobile Devices" applet launching `mde phone`) — satisfying the standing backlog item without a second codebase.

**Theme styling.** `phone.rs` names **zero** colors directly; every surface uses existing widget styles (`mde_ui::scrollbar`, `row_style`, container/frame styles) which already route through `palette::color()`. `Theme::Windows10` (added in E0 via a `win10(rgb)` remap modeled on `carbon()`) therefore restyles the whole surface with no call-site churn. The only new bit of look is the optional wallpaper rail (an `image()` layer, not a color).

**Linux backend mapping.** **KDE Connect protocol** via `MDE-KDECnt-Rust` (LAN UDP 1716 + rustls), plugins: **notifications** (→ shared with `notifyd`), **telephony + SMS** (calls + messages), **sftp / share** (photos + file pull), **contacts** (auto-complete), **battery/connectivity** (status in the device picker). File actions reuse **wl-clipboard**, **`xdg-open`**, and `filedialog.rs`. No new protocol surface in `phone.rs` — it is a thin view over `connect.rs`.

**Reused code:** `files.rs` (three-pane layout, `row_style`, context-menu builder, arrow-key nav, grid/list scaffolding), `icons.rs::icon_any`, `filedialog.rs` (Save as…), `notifyd`/E3 (toasts + Action-Center history + notification *actions* for reply/answer), `tray.rs`'s zbus-on-bg-thread + channel-to-`update` idiom, `state.rs` (`#[serde(default)]` settings: paired device id, primary, per-feature toggles, wallpaper-rail flag), `dialogs.rs` (unpair confirmation), `panel.rs`/`menu.rs` (per-era routing precedent). `control_panel.rs` gets the legacy-era applet entry.

**Compositor boundary.** `mde phone` is an xdg-toplevel — labwc owns its frame/title/z-order; toasts and the call notification are E3 layer-shell surfaces, **not** windows mde manages. No window-manager behavior added.

### E10 Accounts / Lock / Sign-in (Win10 era)

**Scope (D13):** the Win10 lock screen + sign-in greeter (driven by LightDM-gtk-greeter), and the **Accounts** category contributed to the Win10-era Settings app (`mde settings ▸ Accounts`): local users, Sign-in options, account picture. Windows Hello (Face/Fingerprint/Security Key) and Microsoft-account/AAD/online-account/Family-Safety cloud features are explicitly **future** — noted, not built.

> **Epic boundary:** the Settings *app shell* itself (the `mde settings` nav+detail toplevel) is the P0 "Settings + Quick Access" pillar (D3) and is owned by the **Settings epic**. E10 **depends on** that shell and only **contributes the Accounts category** and its sub-pages — it does not stand up Settings.

#### What Windows 10 does (distilled from the Field Guide, pp.37–39, 92, 175–196)

- **Lock screen** (FG p.92) is the first surface on power-on/wake. It shows **date + time large**, a **background image** (or photo slideshow), and **app notification badges**. Any key / click / swipe-up dismisses it to reveal the **sign-in screen**.
- **Sign-in screen** (FG p.190–191): on a fresh boot or sign-out it **lists all available user accounts at the lower-left**; you select a user, then authenticate. The default authenticator is the **password field**; Win10 also offers a **PIN**, plus the (future) Hello methods. Each account shows its **user picture** + display name.
- **Lock/sign-off/switch entry points** (FG p.38–39): from Start's left-rail **user tile** → menu of **Lock / Sign out / Switch user**; signature shortcut **WINKEY+L locks** immediately. **Dynamic Lock** (p.185) auto-locks when a paired Bluetooth phone walks away — maps cleanly to KDE-Connect presence (E5/D5), noted as a follow-up.
- **Accounts settings** (FG p.176–196), the pages relevant in-scope:
  - **Your info** — current user's display name + **account picture**, with *Create your picture → Camera / Browse for one* (p.182). (The "convert to Microsoft account" link is out-of-scope cloud.)
  - **Sign-in options** (p.183–184) — list of methods, each with its own enrollment. In-scope: **Password** (change) and **PIN** (set/change — "provide the same code twice"). Hello Face/Fingerprint/Security Key/Picture Password are listed but **future**.
  - **Family & other users** (p.186–188) — **Add someone else to this PC**, **Change account type** (Standard ⇄ Administrator), **Remove**. Rule: ≥1 Administrator must always exist; you can't remove the account you're signed-in as.
- Out of scope per the locked decisions and the chapter's cloud emphasis: Microsoft accounts, AAD work/school, Email & accounts / online accounts (Mail/People/Calendar), Sync your settings, app accounts.

#### MDE design

**Two deliverables, both era-gated off `palette::theme() == Theme::Windows10`.**

**A. Settings ▸ Accounts category — contributed into `mde settings`.**
Per D4 the Win10 era replaces Control Panel with a modern Settings app; that app shell (left category nav + right detail pane, flat-Win10-styled, modeled on `control_panel.rs`) is the **Settings epic's** deliverable (D3). E10 **registers the Accounts category** into it and implements the three sub-pages (Your info / Sign-in options / Family & other users). If the Settings shell has not yet landed when E10 starts, E10 may bring up a minimal nav+detail host from the `control_panel.rs` skeleton to host Accounts, to be folded into the Settings epic's shell — but the Settings *pillar* work is not counted here. It reuses:
- `mde-ui` flat widgets (`button.rs`, `groupbox.rs`, `frame.rs`) — the Win10 look is a Carbon variant (D2), so no new widget code.
- `metrics::UI_PX` / `INFO_TITLE_PX` for every `.size()` (§2.3); no scattered literals.
- `icons.rs::icon_any` for the category glyphs and the round account-picture mask; `filedialog.rs` for *Browse for one* (returns a path); `dialogs.rs` for confirm prompts (Remove user, "you can't remove yourself").
- `state.rs` for the **account picture path** (new `#[serde(default)] account_picture: String`, §2.6 — empty → generic glyph) and a cached **display name**.

Backend mapping (named, concrete):
- **Local users** = real `/etc/passwd` accounts in the human UID range (≥1000), read by a **new enumeration helper** (added to `sysinfo.rs`, which today only probes OS/CPU/mem/devices) that walks `getpwent`/`/etc/passwd`. **Add user / Change type / Remove** drive `pkexec` wrappers around **`useradd`/`usermod -aG wheel`/`gpasswd -d`/`userdel`** (admin = membership in `wheel`). The privilege prompt reuses the same `pkexec` pattern `installer.rs` already uses for `mde setup --tui`.
- **Password** change = `pkexec passwd <user>` in a Win10-blue `foot` terminal, reusing the installer's terminal-launch pattern (`installer.rs::launch_tui_terminal`, promoted to `pub` or lifted to a shared helper, since it is currently private).
- **PIN** = a numeric quick-unlock distinct from the password, stored **hashed** in `~/.config/mde/pin.hash` using the **`argon2` crate (a new dependency to add)** — no hand-rolled hashing — and consumed only by the greeter/locker (below). Enroll = "enter the same code twice" dialog, matching FG p.184.
- **Account picture** = copy the chosen file to `~/.face` (and `/var/lib/AccountsService/icons/<user>` via pkexec) — the standard path LightDM/AccountsService reads, so the greeter and Start tile both pick it up. *Camera* is a follow-up (needs a capture pipeline); ship *Browse for one* now.

**B. Lock + sign-in greeter — LightDM-gtk-greeter theme + a `lightdm-gtk-greeter.conf` we ship.**
mde does **not** become a display manager (no compositor patching). Instead E10 ships a **theme package** for the system LightDM-gtk-greeter so the login surface matches the Win10 era:
- A GTK CSS theme (`assets/greeter/win10.css`) + `background` wallpaper + an `lightdm-gtk-greeter.conf` selecting it. Colors are **generated** from `palette.rs` Win10 tokens by a build step (`tests/stage-greeter-assets.sh`, modeled on `tests/stage-rpm-assets.sh`) so §2.1 holds — no hand-typed hex in the CSS; the script emits CSS vars from `palette::win10(...)`. This keeps the one-theme-edge invariant even across the GTK boundary.
- The greeter shows the **user list**, **per-user `~/.face` picture**, **large clock**, and **wallpaper** — i.e. the Win10 lock+sign-in composite. The clock/date come from the greeter's built-in clock widget configured in the `.conf`.
- **In-session lock** (WINKEY+L) is a `mde lock` subcommand: a `iced_layershell` **overlay** surface (exclusive, top layer, `popup.rs`-style) that draws the Win10 lock face (wallpaper + big clock + "click to unlock" → password/PIN field) and on success calls `loginctl unlock-session`; it registers with `logind` as the locker. A labwc `rc.xml` keybind `W-l → mde lock` ships in the era's keybind set. The overlay render and PIN-unlock are runtime-reachable and screenshot-observable; the greeter theme covers true boot/sign-out.
- **PIN unlock** in `mde lock` checks `~/.config/mde/pin.hash` (argon2 verify) and is fully harness-observable. **Password unlock** shells to PAM via `loginctl`-mediated reauth; because PAM/`logind` are not present in the isolated nested-sway harness, that path is verified by a **manual/integration check on a real session**, not by the screenshot harness. Dynamic Lock (auto-lock on KDE-Connect phone-away) is a **noted follow-up** wired to E5's presence signal, not built here.

**Theme routing.** `Theme::Windows10` + a `win10(rgb)` remap in `palette.rs` (modeled on `carbon()`) are the **E1** deliverable; E10 **consumes** them. `main.rs` startup maps `state.theme == "windows10"` → `set_theme(Theme::Windows10)`; the Accounts pages and `lock.rs` branch their layout/anchor choices off `palette::theme()` exactly like `panel.rs` does today. Under any non-Win10 era, `mde settings`/`mde lock` are inert no-ops (Control Panel + Win2000/Carbon greeter stay), preserving D1/D4 (one config surface per era).

**Reused modules:** `control_panel.rs` (nav/detail skeleton, owned by the Settings epic), `popup.rs` (layer-shell overlay → `lock.rs`), `filedialog.rs` (picture browse), `dialogs.rs` (confirm), `sysinfo.rs` (extended with a new user-enumeration helper), `state.rs` (picture/display-name persistence), `installer.rs::launch_tui_terminal` (pkexec terminal pattern; promote to `pub`/shared), `icons.rs` (glyphs + round mask), `tests/stage-rpm-assets.sh` (model for the greeter staging script). New dependency: `argon2` (PIN hashing).

**Future (note, don't build):** Windows Hello Face/Fingerprint/Security Key, Picture Password, Microsoft/AAD accounts, Email & online accounts, Sync settings, Family Safety parental controls, Dynamic Lock, Camera-captured picture.

### E11 Win10 OOBE (Out-Of-Box Experience)

**Milestone M2.** Target files: `mde/src/installer.rs`, `mde/src/tui_setup.rs` (extend, do not fork). Driven by `palette::theme()` / the persisted `state.rs` `theme` string. Default era stays Carbon (D1); the Win10 OOBE is reached only when the install/first-run path targets the Windows 10 era.

#### What Windows 10 actually does (from the Field Guide, "Install Windows 10")

The OOBE is the second, post-image phase of Windows Setup — a full-screen, single-pane, one-question-per-screen wizard with a colored flat background, a large heading, a small body paragraph, the relevant control(s), and a bottom-right **Next** button (a **Back** chevron top-left after step 1). The canonical step order:

1. **Region** — "Let's start with region. Is this right?" A long scrollable single-select list, pre-highlighted to the detected region ("United States"). One choice, **Yes**.
2. **Keyboard layout** — "Is this the right keyboard layout?" Single-select list pre-set to the detected layout ("US"). **Yes**.
3. **Second keyboard layout (optional)** — "Want to add a second keyboard layout?" Two buttons: **Add layout** / **Skip** (most users Skip).
4. **Network** — "Let's connect you to a network." Wi-Fi SSID list with signal bars; an SSID expands to a password field + Connect. *Wired Ethernet users skip this step entirely.*
5. **Account** — "Let's add your account." Primary path is a Microsoft account sign-in (email field → password). A small **Offline account** link (Pro only) drops to a local-account flow: username field → password → confirm → security questions.
6. **PIN / Windows Hello** — "Create a PIN." Offered after sign-in (the book flags Hello as hardware-gated; **future** per D13).
7. **Privacy settings** — "Choose privacy settings for your device." A list of toggles (Location, Diagnostic data, Tailored experiences, Find my device, Inking & typing, Advertising ID), each defaulting On, with **Accept**. *(The "Inking & typing" toggle is intentionally NOT carried into the MDE toggle set — Windows Ink is out of scope per D9; see the MDE backend mapping below.)*
8. **Your Phone (optional)** — "Get instant access to your Android phone…" A phone-number field; **Next** or **Do it later**.
9. **Personalize / Cortana / OneDrive / 365 trial** — in the real OOBE these are separate screens. In MDE the Cortana screen is dropped (Cortana is OUT OF SCOPE) and the OneDrive/365-trial screens are dropped (out of 2.0 scope), so this collapses to a single **Personalize** screen: pick accent color + light/dark.
10. **"Hi…" finalize** — color-animated "We're getting everything ready for you" screens while updates/apps install, then the **desktop** appears (taskbar with Start, Search box, Task View button, tray + clock).

Signature shortcut from the desktop the OOBE hands you to: **Win+I** opens Settings, **Win+X** the Quick Access (power-user) menu — both belong to sibling epics but the desktop the OOBE lands on must show the Win10 taskbar.

#### MDE design — extend the existing one-engine installer

MDE already has the exact shape this needs: `tui_setup.rs` is a screen-state machine (`enum Screen` + an `event_loop` advancing on Enter, a centered body, a bottom key-bar) and `installer.rs` is the themed iced face that *collects* then hands off to the verified engine (one install path, §3). We **add a Win10 OOBE branch to both**, reusing those state machines rather than a new module.

**Era routing.** A new `enum OobeEra { Classic, Win10 }` selected from the same source `main.rs` startup reads: `state::load().theme`. `installer::dispatch` gains `--era=win10` (default Classic) and the engine picks the screen set accordingly. When `palette::theme() == Theme::Windows10` (the new 4th era — added in E1, where the `Theme` enum gains a `Windows10` variant and `state.theme` gains the `"windows10"` value; assumed here as a hard dependency on E1), the GUI face renders the Win10 OOBE; otherwise the existing Win2000-blue Setup is untouched (Win2000/Carbon keep their flow — D4/D1).

**`installer.rs` (iced GUI face).** Add a `Stage` enum — `Region, Keyboard, Keyboard2, Network, Account, Privacy, Phone, Personalize, Finalize` — and drive a single full-screen pane (reuse the existing `container(...)` screen shell, but swap `bg_gradient()` for a flat Win10 fill via `palette::color`). Each stage is `heading + body + control + nav-row(Back, Next/Skip)`. Reuse `mde_ui::button` (flat under Win10 once the `win10()` remap lands), `mde_ui::scrollbar` (the existing exported `scrollable` style fn) for the region/keyboard/SSID lists, and `metrics::WIZARD_HEADING_PX` (=15) / `metrics::UI_PX` for sizes (no new literals, §2.3). Nav is the existing `Row` with a `Space::with_width(Length::Fill)` pushing Next right.

**`tui_setup.rs` (headless engine + bench surface).** Add the same `Stage` set as new `Screen` variants ahead of `Welcome` when `--era=win10`, rendered by the existing `render_components`-style windowed list helper (region/keyboard/SSID are all the same "scrollable single-select" widget — factor one `render_picker`). The verified install steps (`run_step`) are unchanged; the OOBE collects choices into a new `OobeChoices` struct and applies them between `Finalize`'s steps so there is still exactly one engine.

**Linux backend mapping (each stage drives a real backend, no mockups — §3):**
- **Region** → write `localectl set-locale LANG=…` (region→locale map) and the timezone via `timedatectl set-timezone` (replaces the Win10 "auto-detect time" post-task).
- **Keyboard / second layout** → `localectl set-x11-keymap <layout>[,<layout2>]`; the live labwc session reads it on next start.
- **Network** → **NetworkManager** via `nmcli`: `nmcli -t -f SSID,SIGNAL dev wifi` to populate the list, `nmcli dev wifi connect <ssid> password <pw>`. Wired-present detection (`nmcli -t -f TYPE,STATE dev` shows an active `ethernet`) **skips** the stage exactly as Win10 does.
- **Account** → local account only (Microsoft sign-in has no analog): `useradd`/`passwd` (already root in the TUI engine) writing the new user; the "Offline account" path *is* the only path here, which inverts the Win10 default cleanly. KDE Connect pairing for the "Your Phone" promise is deferred to its own surface.
- **PIN/Hello** → recorded as **future** (D13) — render the screen as a single **Skip** to keep the flow faithful without a stub backend.
- **Privacy** → real toggles bound to: Location (`geoclue` autostart on/off), Diagnostic data (fedora `abrt`/telemetry opt-out flag), Find-my-device (KDE Connect ping enable), Advertising/Tailored (a `state.json` flag consumed by the shell). The Win10 "Inking & typing" toggle is intentionally **omitted** (Windows Ink is out of scope, D9). Each remaining toggle writes a config it actually controls.
- **Your Phone** → opens (or queues) **KDE Connect** pairing via the `MDE-KDECnt-Rust` crate — the unified Your Phone surface (D5); "Do it later" simply skips.
- **Personalize** → writes `theme_mode` (light/dark → `palette::set_dark`) and the accent index into `~/.config/mde/menu.json` through `state::save` (atomic, §2.6); applied at next shell launch (per-process theme load, §7).
- **Finalize** → the existing `run_step` chain (dnf install, session register, branding) — the "Hi…" animated screens map to the existing progress list; the desktop it hands to is the Win10 panel from E3.

**Theme styling.** Win10 reuses the flat Carbon widgets shifted by a new `win10(rgb)` remap in `palette.rs` modeled on `carbon()` (D2) — added in E1/E2 and consumed here; this epic adds **no** new hex (§2.1) and routes purely through `palette::color`. The OOBE's full-screen flat fill uses an existing role key (e.g. `SETUP_GRADIENT_TOP` remapped to a flat Win10 surface) so no new color constant is required.

**State.** A new `oobe_done: bool` (`#[serde(default)]`, false) in `MenuState` so the GUI OOBE is shown once on first session and never again; the manual `Default` must agree with `parse("{}")` (the §2.6 test). All choices persist through the existing `state::save`.

**Reused code:** `tui_setup.rs` screen machine + `render_components`/`centered`/key-bar; `installer.rs` iced shell, `bg`/nav `Row`, `mde_ui::button`/`mde_ui::scrollbar`; `state.rs` load/save/Default-parity; `catalogue` (Finalize install); `palette`/`metrics`/`font` for all sizing and color. New surface follows the §3 one-engine rule: GUI collects → verified TUI engine applies.

### E12 Settings ▸ Devices

**Source:** Windows 10 Field Guide, "Devices" chapter (printed pp.129–148; PDF
pp.140–159). In scope: the **Bluetooth & other devices**, **Printers & scanners**,
**Mouse**, **Touchpad**, **Typing (keyboard)**, **AutoPlay** (removable storage),
and **second-display / Project** pages. **Pen & Windows Ink, the touch keyboard, and
Miracast "Connect" are OUT** (D9; Connect surfaces nothing).

#### Win10 behavior distilled from the pages

Devices settings is one **category** of the modern Settings app, reached via
**WINKEY + I → Devices**. Layout is the standard Settings two-pane: a left **page
list** (Bluetooth & other devices · Printers & scanners · Mouse · Touchpad · Typing
· Pen & Windows Ink [out] · AutoPlay) and a right **content pane** of
section headers, toggles, dropdowns, and "+" add buttons. Pages appear
conditionally — **Mouse and Touchpad only show when that peripheral is present.**

- **Bluetooth & other devices** (pp.132–134): a master **Bluetooth On/Off** toggle;
  an **"+ Add Bluetooth or other device"** button that opens an **Add a device**
  modal (Bluetooth / Wireless display or dock / Everything else). Choosing
  Bluetooth shows a live **discovery list**; selecting a device runs a
  **"Connecting…"** step (sometimes a PIN/passkey confirm), then **"ready to use."**
  Paired devices list below grouped (Audio, Mouse/keyboard/pen, Other) each with a
  **Connected/Paired** status and a **Remove device** action. (Swift Pair is a toast
  offer — maps to Action Center, E3.)
- **Printers & scanners** (pp.139–142): **"+ Add a printer or scanner"** (scan →
  list → add); a printers/scanners list; per-printer **Open queue / Manage / Remove**;
  Manage page has **Set as default**, **Printing preferences**, **Print test page**;
  a **"Let Windows manage my default printer"** toggle (when on, default = most
  recently used, and the per-printer Set-as-default button hides). A built-in
  **Microsoft Print to PDF** virtual printer always present.
- **Mouse** (pp.135–136): **Select your primary button** (Left/Right) dropdown;
  **Roll the mouse wheel to scroll** (Multiple lines / One screen) + a
  **lines-per-scroll** slider; **Scroll inactive windows when hovering** toggle
  (advisory — see backend note); an **"Additional mouse options"** link to the legacy
  Mouse Properties.
- **Touchpad** (pp.136–137): a **Touchpad On/Off** toggle; **sensitivity** dropdown;
  **tap** options; **two-finger scroll** + **scrolling direction** (natural/normal);
  multi-finger gestures (precision touchpads) — assignable or set to "Nothing."
- **Typing / keyboard** (p.138): Spelling (autocorrect/highlight misspellings),
  **Typing** (key suggestions), **Hardware keyboard** (autocorrect/suggestions),
  **Multilingual** input switching. (Touch keyboard section OUT.)
- **AutoPlay / removable storage** (pp.131–132, 138): a master **AutoPlay On/Off**
  toggle, plus a per-device-type default-action dropdown (**Removable drive**,
  **Memory card**) — choices like *Open folder to view files / Take no action*.
  First connect raises an AutoPlay toast (E3).
- **Second display** (pp.143–148): connecting a display trills a sound and starts in
  **Duplicate**. **WINKEY + P** opens the **Project** pane with four modes:
  **PC screen only · Duplicate · Extend · Second screen only**. Full per-display
  resolution/scale/orientation lives in System ▸ Display. Miracast "Connect" is a
  wireless-display variant — **OUT of scope; we surface Project only.**

**Signature shortcuts:** `WINKEY+I` (Settings), `WINKEY+P` (Project pane). (Miracast
`WINKEY+K`/Connect is out — not bound, surfaces nothing.)

#### MDE-Retro design

**Host:** Devices is a **category inside the modern Settings app** (E6,
`mde/src/settings.rs`), not a standalone window — D4 makes Settings the one config
surface **in the Win10 era only** (Win2000/Carbon keep the Control Panel). E12 ships
the Devices **module** the Settings host mounts: new file
**`mde/src/settings/devices.rs`** (the Settings host owns the left category rail,
window chrome, and the `mde settings` dispatch arm in `main.rs`; E12 owns the
Devices pane and its backends). For standalone bench-testing and the labwc keybind,
add a thin **`mde settings --page devices[:bluetooth|printers|mouse|touchpad|typing|autoplay]`**
deep-link (one match arm + `%post` symlink `mde-settings`), so each page is reachable
and screenshot-able without driving the whole nav.

**Theme/era routing.** Devices renders **only under `palette::theme() == Theme::Windows10`**
(added by E0). The Win10 Settings look is a Win10-flavored Carbon variant (D2): reuse
the flat Carbon widgets unchanged — `mde_ui::group_box` for section headers,
`mde_ui::button`, `iced::widget::{toggler, pick_list, slider, checkbox}` styled by the
existing `mde_ui::checkbox_style`/`sunken_picklist`/`scrollbar`. Win10 chrome
specifics (the page header in Segoe-substitute, ~28px row height, the colored "+"
add buttons) come from the `win10(rgb)` remap and `metrics::` constants; **no raw hex
and no scattered `.size()`** (§2.1/§2.3). The Win10 accent (default blue, with the
light/dark + accent-system from E0) drives toggles and the Add button. If E0/E6 are
not yet merged, E12's pages still build under Carbon for headless `--page` testing;
the era gate just selects styling. **mde draws only client content** — the Project
pane is a layer-shell popup; no title-bar/frame drawing (labwc owns chrome, §1).

**Linux backend mapping (each named, no hand-waving):**

| Win10 page | MDE-Retro backend | How |
|---|---|---|
| Bluetooth & other devices | **BlueZ via zbus** (reuse the `zbus` dep already used by `tray.rs`) | `org.bluez` D-Bus: `Adapter1.Powered` toggle; `StartDiscovery`/`StopDiscovery` feeds the live list from `InterfacesAdded`; `Device1.Pair`/`Connect`/`Disconnect`; `Adapter1.RemoveDevice`. PIN/passkey via a registered `org.bluez.Agent1`. A new `mde/src/bluez.rs` data layer (mirrors `wlr.rs`'s bg-thread pattern). |
| Printers & scanners | **CUPS** via `lpstat`/`lpadmin`/`lpoptions` + reuse the existing `system-config-printer` tool already in `fedora.rs` | List `lpstat -p -d`; Add = `lpinfo -v` device scan then `lpadmin`; **Set as default** = `lpoptions -d`; **Print test page** = `lp` a bundled page; **Manage** opens the CUPS web UI or `system-config-printer`. **Microsoft Print to PDF** = the **cups-pdf** virtual queue (install via `fedora::install` if absent), surfaced as a permanent printer. |
| Mouse | **labwc libinput** config in `rc.xml` `<libinput>` + `<context>` | Primary button = `<leftHanded>`; scroll mode/lines-per-scroll = `<scrollFactor>`; natural-scroll direction = `<naturalScroll>`. **"Scroll inactive windows when hovering" has no labwc/libinput backend** — it is an **advisory toggle persisted to `menu.json`** and clearly labeled (not faked into rc.xml). "Additional mouse options" links to this same libinput section (Win2000's `display.rs` Mouse legacy is absent in the Win10 era). Rewrite `rc.xml` then `labwc --reconfigure` (reuse the rewriter pattern from `display.rs::set_labwc_title_colors`, keeping the **`<mouse><default/>`** rule, §7). |
| Touchpad | same labwc `<libinput>` (tap, two-finger scroll, natural-scroll, sensitivity → `<pointerSpeed>`) | Page **hidden when no touchpad** (probe `libinput list-devices`/`/sys/class/input`), matching the Win10 conditional-page rule. |
| Typing / keyboard | **labwc `<keyboard>`** (layout/repeat) + note that spelling/suggestions are app-level | Page surfaces layout + key-repeat (rate/delay) via `rc.xml`; the Win10 "Spelling/Typing suggestions" toggles are persisted to `menu.json` (advisory; no system speller) and clearly labeled, not faked. |
| AutoPlay / removable storage | **udisks2** + a small per-user handler | Master toggle + per-type default action persisted in `state.rs`; on/off writes whether the **`mde devices-monitor`** udisks2 subscription auto-opens `mde files` on mount. First-mount toast goes through E3's notify daemon. |
| Second display / Project | **reuse `display.rs` + `outputs.rs` (wlr-randr)** | The Project pane is a separate **`mde project`** layer-shell popup (modeled on `popup.rs`) bound to **WINKEY+P** in `rc.xml`. Each mode builds an `outputs::Desired { outputs: Vec<DesiredOutput> }` (the real API; positions set via the explicit `x`/`y` fields on `DesiredOutput`) and calls **`outputs::apply_live(&desired)`**: *PC screen only* = enable primary only, disable output 2; *Duplicate* = both outputs at `x:0,y:0` with the same mode; *Extend* = output 2 at `x = primary.width, y: 0`; *Second screen only* = disable primary, enable output 2. The full per-display resolution/scale UI stays in `display.rs` (Settings ▸ Display links to it). |

**Reused code:** `mde_ui::{group_box, button, frame, metrics, palette}`; `zbus`
(from `tray.rs`); `outputs.rs` (`Desired`/`DesiredOutput`/`apply_live`) and
`display.rs` for the Project pane; `fedora::install` for cups-pdf/bluez packages;
`state.rs` for persisted AutoPlay/typing/mouse-advisory prefs (every field
`#[serde(default)]`, §2.6); `popup.rs` pattern for the Project layer surface; the
`display.rs` `rc.xml`-rewriter pattern for libinput. **Status/empty states** follow
Win10: a present-but-disabled "Turn on Bluetooth to see devices" when the adapter is
off, and conditional Mouse/Touchpad pages — never tofu, never a mockup.

### E13 Windows Update (Settings - Update & Security)

**Milestone:** M2. **Target files:** new `mde/src/settings_update.rs` (rendered inside the E6 `settings.rs` shell as the *Update & Security - Windows Update* category); a `dnf` backend helper in `sysinfo.rs`; and a wire-up of the existing `system_properties.rs` *Automatic Updates* tab so its radios actually persist.

#### What Windows 10 does (distilled from pp. 197-203, PDF 208-214)

Windows Update lives at **Settings (WINKEY + I) > Update & Security > Windows Update**. Layout, top to bottom:

- A **status header** -- "You're up to date" / "Updates are available", last-checked timestamp, and a list of pending updates with their state (Downloading / Pending install / Pending restart).
- A primary **"Check for updates"** button (manual check; updates otherwise install automatically).
- Four **action links**, each opening its own sub-page:
  1. **Pause updates for 7 days** -- repeatable in 7-day steps up to **35 days total**; while paused the link flips to **"Resume updates"**.
  2. **Change active hours** -- the window (default **08:00-17:00**) during which Update will *not* auto-restart; a "Change" control sets a start/end pair (or "automatically adjust based on activity").
  3. **View update history** -- installed updates grouped into **Feature updates** and **Quality updates**, with **Uninstall updates** and **Recovery options** links at the top.
  4. **Advanced options** -- toggles: *Receive updates for other Microsoft products*, *Download over metered connections*, *Restart as soon as possible* (all off by default); an *update-notification* toggle ("notify when a restart is required", off by default); and the same **Pause** control in 1-day increments.
- An **"View optional updates"** link appears only when driver/optional updates exist; it lists checkable groups (e.g. Driver updates) with a **Download and install** button.

Two update classes are surfaced: **Quality updates** (bug/security, monthly-ish) and **Feature updates** (version upgrades). Key signature: **WINKEY + I** opens Settings.

#### MDE-Retro design

**Surface & era routing.** This is a Win10-era-only surface. It is reached two ways, both branching off `palette::theme()`:
- As the **Update & Security** category inside the E6 Settings app: `settings.rs` only lists this category when `palette::theme() == Theme::Windows10`; selecting it renders `settings_update::view`.
- As a **direct deep-link** `mde settings update` (new `main.rs` dispatch arm `"settings"` with an optional category arg, plus a `mde-settings` %post symlink). When run while the active theme is *not* Windows10, it prints a one-line "Settings is the Windows 10 era configuration surface" notice and exits 0 (Control Panel remains the config surface for Win2000/Carbon per D4) -- no panic, bench-observable.

`settings_update.rs` is a normal `iced::application` window (xdg-toplevel, like `system_properties.rs`/`control_panel.rs`), NOT a layer-shell surface -- it is an app window, so labwc owns its frame. It follows the `control_panel.rs` shape: an `update`/`view` pair, `Theme::Light` base with `mde_ui` flat widgets, `font::ui()` default.

**Look (D2 Win10-flavored Carbon variant).** All colors route through `palette::color()`; E0 adds `Theme::Windows10` + a `win10(rgb)` remap modeled on `carbon()`. This pane names **no raw hex** -- it reuses existing role constants: the status header card sits on `WINDOW`/layer-01; the big status glyph and primary button use `palette::accent()` (the Win10 accent, supplied by E0's `win10()` accent + the existing accent-hue machinery); pending/secondary text uses `GRAY_TEXT`; the four action links use the `INFO_BAND` accent role (already remapped to the era accent in `carbon()`/`win10()`). Every `.size()` uses `metrics::UI_PX` / `metrics::INFO_TITLE_PX` for the status headline. Buttons are `mde_ui::button` (flat under the Carbon/Win10 remap). No new widgets.

**Linux backend = dnf (the load-bearing mapping).** A new `sysinfo` update section (toolkit-agnostic, reused headless) wraps dnf so the pane drives real package state:
- **Check for updates** -> `dnf check-update --refresh` (run via `Task::perform` off the UI thread; exit code 100 = updates available, 0 = none). The parsed `pkg version repo` lines populate the pending list. *Quality updates* = ordinary package updates; *Feature updates* = a Fedora **`dnf system-upgrade`** availability probe (newer-release check against `/etc/os-release`), surfaced as the one "Feature update" row when a newer Fedora release is offered.
- **Install / Download and install** -> `pkexec dnf -y upgrade` (or the checked optional/driver subset by package name); pending rows transition Downloading -> Installed; a row needing a kernel/glibc bump sets the **"restart required"** flag.
- **View update history** -> `dnf history list` parsed into transactions, split into *Quality updates* (normal upgrade txns) and *Feature updates* (system-upgrade txns), matching Win10's two-group layout. **Uninstall updates** -> `pkexec dnf history undo <id>` on the selected transaction; **Recovery options** -> a `launch_button` to the existing System Restore / Timeshift path (reusing `restore_tab` semantics) -- this is E13's complete, shipped behavior. (E14 Recovery later *replaces* this link with a richer Recovery sub-page; E13 does not depend on E14 to be done.)
- **Active hours** -> persisted to `state.rs` and applied to `dnf-automatic.timer`'s `OnCalendar` (the timer only fires outside active hours) via a `pkexec` systemd timer override drop-in. Default 08:00-17:00 to match Win10.
- **Pause updates (7-day steps, max 35)** -> `pkexec systemctl mask --now dnf-automatic.timer` plus a `paused_until` timestamp in `state.rs`; the link computes remaining days and flips to **Resume** (`unmask --now`) once paused. Bench-observable via `systemctl is-enabled dnf-automatic.timer` flipping masked/enabled.
- **Advanced toggles** map to `automatic.conf` keys via a `pkexec` config writer: *metered* -> a NetworkManager metered-connection guard, *restart ASAP* -> `reboot = true`, *other products* -> include third-party repos. *Restart-required notification* -> emits a freedesktop notification through the **E3 notify daemon** (`notifyd`) when the pending set requires reboot, giving the Win10 "notify when a restart is needed" toast. dbus round-trip observable.

**Wiring the System Properties stub (the explicit M2 ask).** Today `system_properties.rs::updates_tab` (lines 301-323) shows three radios (Off / DownloadOnly / Install) whose only effect is an `Apply` button that enable/disables `dnf-automatic.timer`; the selection is **not persisted** to `state.rs` and `auto_sel` is re-seeded from a live probe each launch. E13:
- Promotes the dnf logic into `sysinfo::set_auto(AutoMode)` (one privileged helper: Off=`disable --now`, DownloadOnly=enable+`apply_updates=no`, Install=enable+`apply_updates=yes`), so **both** `system_properties.rs` and `settings_update.rs` call the same backend instead of inlining command strings.
- Persists the chosen `AutoMode` to `state.rs` (a new `#[serde(default)]` field with an explicit default fn, and the manual `Default` impl updated so it agrees with `parse("{}")` per §2.6's enforced test) so the posture survives relaunch; `sysinfo::advanced()` keeps reading live timer state as the source of truth, and the radios reflect it.
- The Win10 Update pane's "automatic updates" posture and the System Properties radios are now the **same setting** through one backend -- changing it in either surface and reopening the other shows the new value (screenshot-observable parity).

**Reused code.** `control_panel.rs` window/`item_style` patterns; `system_properties.rs` `launch_button`/`group_box`/`restore_tab` patterns; `sysinfo.rs` `AutoMode`/`Advanced`/`cmd_line`/`set_auto`; `state.rs` persisted prefs (§2.6 serde defaults); `dialogs.rs` for the reboot-confirmation prompt; the E3 `notifyd` for restart toasts; `palette::color`/`accent` + `metrics::` for all styling. No new widget, no compositor patch, no third-party update tooling.

### E14 Security (Windows Security dashboard) — Milestone M2

#### Win10 ground truth (Field Guide, "Security" chapter, printed pp. 204–220)

Windows 10 surfaces every scattered security feature through **one dashboard app, "Windows Security"** (Settings > Update & Security > Windows Security > Open Windows Security). The main page is a grid of **status tiles, each with a green check mark when healthy**. The tiles are:

- **Virus & threat protection** — front-end to Windows Defender (AV/anti-malware). Has a **Quick scan** button, a **Scan options** link (Quick / Full / Custom / offline), "current threats" + last-scan status, and a Files Explorer right-click **"Scan with Windows Defender"** that opens the app on this page running a *custom* scan.
- **Account protection** — links into the Accounts area of Settings (Account info, Sign-in options, Dynamic lock).
- **Firewall & network protection** — a "friendly interface to the network firewall." Shows firewall on/off per network profile (Domain / Private / Public), with an **"Advanced settings"** link to the legacy *Windows Defender Firewall with Advanced Security* MMC.
- **App & browser control** — SmartScreen download/website reputation + exploit protection (advisory on Linux).
- **Device security** — view-only status of core isolation, security processor (TPM), Secure Boot.
- **Device performance & health** — mini health dashboard + Fresh start link.
- **Family options** — link to web parental controls.

Separately:
- **Find My Device** lives in Settings > Update & Security > Find my device — a toggle, plus a **map view + per-device list** on account.microsoft.com/devices where each PC can be **Find** (locate on map) or **Lock** (remote-lock + enable location tracking).
- **BitLocker** (full-disk encryption) is managed from the *BitLocker Drive Encryption* legacy Control Panel: per-drive rows under "Operating system drive" / "Fixed data drives" / "Removable – BitLocker To Go," each with **Turn on/off BitLocker**, **Suspend protection**, **Back up your recovery key** (to account / file / printout), and a wizard (recovery-key backup → how-much-to-encrypt → encryption mode → confirm + "Run BitLocker system check").

Signature interactions: **green check = healthy**; tile click drills into a detail page; right-click "Scan with Windows Defender" on a file; per-drive "Turn on BitLocker"; Find-my-device toggle + per-device Find/Lock.

#### MDE-Retro design (Win10 era only — D1/D4)

New surface **`mde security`** — a single iced/iced_layershell *xdg-toplevel* window (modeled on `system_properties.rs`: a left rail + a content panel fed by a toolkit-agnostic data layer), reachable from a `main.rs` dispatch arm, a `%post` `mde-security` symlink, and a labwc `rc.xml` keybind (`Win+Shift+S` advisory; gated by era at launch). This is the **Win10-era unification** of today's scattered tools: it absorbs `fedora.rs`' "Windows Firewall" (`firewall-config`), "Security Center" (`sealert -b`), and "Lynis (Audit)" entries, which become *Advanced settings* deep-links rather than free-floating Control Panel rows.

New modules:
- **`mde/src/security.rs`** — the dashboard surface: an iced `update`/`view` app. Left rail lists the tiles; the home view is the **status grid** (one `groupbox`-style tile per pillar with an icon, title, one-line status, and a **status glyph**). Detail pages render on tile click (`Message::OpenTile(Pillar)`).
- **`mde/src/security_probe.rs`** — the **toolkit-agnostic data layer** (mirrors `sysinfo.rs`), one `SecurityStatus` struct populated by spawning backend probes **once at startup** (off the draw hot path, exactly like `control_panel.rs::installed` and `system_properties`' async `Loaded`). Provides `fn status() -> SecurityStatus` and pure parser fns (unit-testable headless, like `sysinfo::parse_*`). **All security probing — firewalld, LUKS, ClamAV, and the new TPM / Secure-Boot / sysfs reads — lives here**; `sysinfo.rs` today carries only os-release/cpu/meminfo parsers, so the TPM/Secure-Boot helpers are *new* code in `security_probe.rs`, written in the same pure-parser style.

Each tile maps a Win10 pillar to a concrete Linux backend:

| Tile | Win10 backend | MDE-Retro backend (concrete) | State shown |
|---|---|---|---|
| **Virus & threat** | Windows Defender | **ClamAV** if present (`clamscan --version`, `clamd` running, freshclam DB age) — **Quick scan** button runs `clamscan -r ~ ` detached; else **advisory** tile ("No on-access AV installed — Fedora relies on SELinux + updates"). Reuses `fedora.rs` install-on-click (`pkexec dnf install clamav clamav-update`). | DB date, clamd up/down, last-scan, or advisory |
| **Firewall & network** | Defender Firewall | **firewalld** (`firewall-cmd --state`, `--get-default-zone`, `--get-active-zones`). Per-zone on/off mirrors Win10's Domain/Private/Public profiles. **Advanced settings** launches `firewall-config` (the existing fedora.rs tool). | running/not, default zone, active zones |
| **Device encryption** | BitLocker | **LUKS / cryptsetup** (`lsblk -o NAME,TYPE,FSTYPE` → find `crypto_LUKS`; `cryptsetup status` per mapping). Per-drive rows ("Operating system drive" / "Fixed" / "Removable") echo the BitLocker panel. **Back up recovery key** = `cryptsetup luksHeaderBackup` to a path chosen via `filedialog.rs` (`mde filedialog --save`); **Turn on** = advisory wizard that emits the exact `cryptsetup luksFormat` command behind a `dialogs.rs`-style confirm modal (destructive → confirm, never auto-runs). | encrypted/not per block device |
| **Find my device** | account.microsoft.com | **KDE Connect** (D5/D10 — MDE-KDECnt-Rust crate / `mde connect`): toggle = pairing state; per-device list with **Ring** (KDE Connect findmyphone plugin) and **Lock** (lock-device plugin) replacing Win10 Find/Lock. No GPS map — list of paired devices + battery/last-seen. | paired devices, reachable/not |
| **Account protection** | Accounts settings | Deep-link into the Win10-era **Settings > Accounts** surface (E13). Until E13 lands: links to `mde control-panel` user-tool. | sign-in method present |
| **Device security** | TPM / Secure Boot | **read-only**, probed in `security_probe.rs` (new sysfs helpers): Secure Boot (`mokutil --sb-state` or `/sys/firmware/efi/efivars/SecureBoot-*`), TPM (`/sys/class/tpm/tpm0`). | enabled/absent |

**App & browser control**, **Family options**, **Device performance & health/Fresh start** are rendered as **advisory tiles** (status text + a "Learn more"/launch link), never as fake toggles — §3 forbids mockups. Exploit-protection/SmartScreen have no Linux analog, so they state that plainly.

#### Theme routing & styling

- The surface is **Win10-era only (D4)**: `main.rs`'s `"security"` arm checks the active theme; under `Theme::Windows10` it launches `security::run`, otherwise the arm is inert. The labwc keybind + Start tile only appear in the Win10 era. (Note: panel/menu currently branch on `palette::is_carbon()`, which cannot tell Win10 from Carbon — so a new `palette::is_windows10()` / `theme() == Theme::Windows10` predicate is added alongside `is_carbon()` and used at every Win10-gated branch.)
- All chrome goes through existing `mde-ui` flat **Carbon widgets** (`groupbox`, `button`, `frame`, `tabs`) — **no new widgets**, per D2 (Win10 = Carbon variant). No raw hex (§2.1): the **green "healthy" check, amber "attention", red "at risk"** status glyphs are **new palette role constants** added to `palette.rs` (`STATUS_OK`/`STATUS_WARN`/`STATUS_RISK`, modeled on the existing danger-red sentinel) and remapped in the new **`win10(rgb)`** edge (D2: a Carbon variant — clone `carbon()`, shift surfaces/accent to the Win10 palette, keep Carbon's flat bevel). `Theme::Windows10` is added to the `Theme` enum + the `color()` `match`, with `set_theme`/`THEME` index `3`.
- Every `.size()` uses `metrics::` constants (§2.3); a `metrics::SECURITY_TILE` size constant is added and pinned in `checklist.rs` alongside the new status-color sentinels (§2.2).

#### Reuse (don't duplicate)

`system_properties.rs` (rail+panel+async-`Loaded` pattern), `control_panel.rs` (install-missing-on-click via `fedora.rs`), `dialogs.rs` (its `Confirm` pattern for the destructive encrypt-command modal), `filedialog.rs` (`--save` flow for the recovery-key backup path), `icons.rs`/`embedded_icons.rs` (Carbon SVG shield/lock/firewall glyphs via `icon_any`), and the KDE Connect crate for Find-my-device. (TPM/Secure-Boot/sysfs probing is **new** in `security_probe.rs`, not borrowed from `sysinfo.rs` — that module only carries os-release/cpu/meminfo parsers today.) A future at-risk SNI badge via `tray.rs` is noted but **not P0**.

#### Backends (named): firewalld, LUKS/cryptsetup, ClamAV (optional), KDE Connect (MDE-KDECnt-Rust), mokutil/sysfs (TPM/Secure Boot), pkexec dnf (install-missing).

### E15 Networking — Settings ▸ Network & Internet (NetworkManager)

> Milestone **M2**. Depends on **E0** (`Theme::Windows10` + `win10()` remap, era plumbing), **E6** (`mde/src/settings.rs` shell that hosts category pages and routes off `palette::theme()`), and **E3** (the Action-Center quick toggles + the `notifyd` freedesktop notification daemon, used by the data-limit warning). The Action-Center quick toggles (E3) call into the same NM helpers defined here. Carbon/Win2000 are untouched — they keep `mde control-panel` → `nm-connection-editor` (`fedora.rs`).

#### What Windows 10 does (Field Guide pp. 221–233)

The **Network icon** in the notification area opens a **Network access flyout** — a single front-end listing connected + available networks plus an **Airplane mode** tile and a "Network & Internet settings" link. The flyout's icon reflects connection type (Wi-Fi five-bars, Ethernet, cellular triangle).

The **Settings ▸ Network & Internet** app has a left category rail with these pages:
- **Status** — graphical "how you're connected to the Internet" diagram, the active connection's profile (Private/Public) with a **Properties** button, plus "Advanced network settings" links (legacy Network and Sharing Center, change adapter options).
- **Wi-Fi** — list of available SSIDs; selecting one prompts for the **network security key** (password) and asks whether the PC should be **discoverable** (Yes → Private profile, No → Public). An **auto-connect to known networks** toggle.
- **Ethernet** — wired connection, auto-configured Private, switchable to Public via Properties.
- **VPN** — integrated VPN connection list ("not really an end-user feature", check with workplace).
- **Mobile hotspot** — "Share my Internet connection with other devices" toggle; **Edit** button to set network name + password + band ("All available" default); shares to up to 8 devices; auto-off when idle.
- **Proxy** — manual proxy config (address/port/bypass list), "businesses only".
- **Data usage** — per-network usage totals; **set a data limit** to warn before exceeding a metered allotment.
- **Airplane mode** — master toggle that kills all wireless radios at once; individual radios (Wi-Fi, Bluetooth) re-enableable. Most easily toggled from the Action Center tile.
- **Cellular** — metered-connection management (master switch, metered toggle, roaming, "use cellular instead of Wi-Fi", per-app data). **OUT for MDE** (no carrier modem on a Linux workstation) — collapse cellular/eSIM/data-plan purchase to a single greyed "No cellular hardware detected" row, mirroring how §3 mockups grey non-functional controls. Data usage stays (NM has per-device byte counters).

**Signature interactions:** **Win+A** opens Action Center (hosts the Airplane mode + Mobile hotspot quick tiles); clicking the taskbar Network icon opens the network flyout.

#### MDE design

**Surface 1 — Network flyout (`mde net-flyout`, new layer-shell module `mde/src/net_flyout.rs`).**
A layer-shell popup modeled on `popup.rs`/`menu.rs` (anchored to the panel's network glyph; Win10 era → top-right under the Carbon-style header that E0 establishes). It lists the active connection at top (type icon + SSID/Ethernet name + Connected/Properties), then available Wi-Fi SSIDs (signal bars + secured padlock), then a footer row of toggle pills: **Wi-Fi**, **Airplane mode**, and a "Network & Internet settings" link that `setsid`-spawns `mde settings --page network` (pattern copied from `display.rs:890` and `panel.rs:344`). Selecting an unsaved SSID opens an inline password field; on submit it shells `nmcli device wifi connect "<ssid>" password "<key>"`. The panel's existing `net_glyph`/`poll_net` (`panel.rs:668`, `:763`) is the launch button — replace its `Launch("nm-connection-editor")` arm **for the Win10 era only** (branch on `palette::theme() == Theme::Windows10`) with `Launch self net-flyout`; other eras keep `nm-connection-editor`.

**Surface 2 — Network & Internet page set, hosted inside `mde settings` (E6).**
`mde/src/settings.rs` owns the left rail + page router; E15 contributes a `settings/network.rs` submodule (a `view_*` fn per page + a `Message` enum, the same iced `application(update,view)` shape as `control_panel.rs`). Pages: Status, Wi-Fi, Ethernet, VPN, Mobile hotspot, Proxy, Data usage, Airplane mode. All controls are the **flat Carbon widgets** (`mde-ui::widget` button/groupbox/frame) — under `Theme::Windows10` they render with the Win10-flavored palette from E0's `win10()` remap (light/dark + accent), so no new widget code. Every `.size()` uses `metrics::UI_PX`/`INFO_TITLE_PX` (§2.3); zero raw hex (§2.1) — any new chrome color is a role constant added to `palette.rs`.

**Linux backend — one NM helper module (`mde/src/nm.rs`), all parse-only `nmcli`/`rfkill`/`nm-online` (no D-Bus dep added; matches the existing `nmcli -t -f` style in `panel.rs:670`).**
- Wi-Fi scan/list: `nmcli -t -f SSID,SIGNAL,SECURITY,IN-USE device wifi list`.
- Connect/forget: `nmcli device wifi connect` / `nmcli connection delete`.
- Status diagram + profile: `nmcli -t -f NAME,TYPE,DEVICE,STATE connection show --active`; profile Private/Public maps to NM `connection.zone` (firewalld zones `home`↔`public`, dovetails with E-firewall) toggled via `nmcli connection modify <c> connection.zone <zone>` — this is the concrete Linux equivalent of the Win10 discoverable/Private vs Public switch.
- Auto-connect: per-connection `nmcli connection modify <c> connection.autoconnect yes|no` (drives the Wi-Fi page "auto-connect to known networks" toggle).
- VPN list/up/down: `nmcli -t -f NAME,TYPE connection show` filtered to `vpn`/`wireguard`; `nmcli connection up/down`. "Add VPN" hands off to `nm-connection-editor` (already in `fedora.rs`) for the import dialog rather than reimplementing credential forms.
- Mobile hotspot: `nmcli device wifi hotspot ssid <n> password <p> band <b>` to enable, `nmcli connection down Hotspot` to disable; Edit dialog persists name/password.
- Proxy: writes the GNOME/`org.gnome.system.proxy` gsettings keys (mode manual/auto/none, host, port, ignore-hosts) — the proxy surface most Wayland apps honor — via `gsettings set`.
- Airplane mode: `rfkill block all` / `rfkill unblock all`; per-radio Wi-Fi toggle = `nmcli radio wifi on|off`. State read from `rfkill -no SOFT,HARD,TYPE`. The Action-Center "Airplane mode" + "Mobile hotspot" tiles (E3) call these same `nm::set_airplane` / `nm::set_hotspot` fns.
- Data usage: read NM device byte counters from `/sys/class/net/<dev>/statistics/{rx,tx}_bytes`; the "set a data limit" warning threshold persists in `state.rs` and, when exceeded, fires a freedesktop notification through the E3 notify daemon (`notifyd`).

**Reused code:** `popup.rs`/`menu.rs` (layer-shell scaffold for the flyout), `control_panel.rs` (page/menu/`update`/`view` shape + `setsid`-spawn launcher), `panel.rs` `net_glyph`/`poll_net`/`is_network_sni` (icon + state, now feeding the flyout), `fedora.rs` (`nm-connection-editor` / `NetworkManager` entries for the advanced/VPN-import handoff), `icons.rs` `icon_any` (network/wifi/vpn/lock glyphs), `state.rs` (`#[serde(default)]` data-limit + last-hotspot-SSID fields), `dialogs.rs` (the password/Edit modal pattern), `tray.rs` (don't double-draw the NM SNI icon when our flyout owns it). New persisted fields follow §2.6 (every field `#[serde(default)]`, `Default` agrees with `parse("{}")`).

**Era routing:** the new surfaces only mount under `Theme::Windows10`. `main.rs` gains `"net-flyout" => net_flyout::run(rest)` and the network page is reachable as `mde settings --page network`; both are wired with a `%post` symlink + a labwc `rc.xml` keybind (no compositor patching). In Win2000/Carbon the panel network glyph keeps its current `nm-connection-editor` behavior, so D4 (one config surface per era) holds.

### E16 Clipboard History + Screenshots (Win10 era)

Scope (locked): Clipboard history (WINKEY+V) over the Wayland clipboard; Snip and Sketch (WINKEY+SHIFT+S) plus PrintScreen capture, driven by grim/slurp/wl-copy. New modules mde/src/clipboard.rs and mde/src/snip.rs. Win10 era only (D1/D4); Carbon/Win2000 keep their existing copy/paste with no history UI.

#### What Windows 10 actually does (Field Guide, printed pp. 20-23)

Clipboard. CTRL+C / CTRL+X copy/cut to the system clipboard, CTRL+V pastes the most-recent item. The advanced feature is clipboard HISTORY: by default the clipboard holds only the last item, but with history enabled it stores multiple items, and WINKEY+V opens a small floating clipboard-history window (anchored near the caret, not full screen). Each history entry is a row showing a text snippet or image thumbnail; clicking an entry pastes it. Entries can be pinned and individually deleted; "Clear all" empties non-pinned history. History and sync-across-devices are off until enabled in Settings > System > Clipboard. (Sync-across-devices / cloud clipboard is a Microsoft-account cloud feature; per D10 we reinterpret cloud as KDE-Connect devices, but cross-device clipboard sync is deferred to E5/E18 KDE Connect, not built here; the Settings > System > Clipboard toggles live in E11 Settings.)

Screenshots. Several capture paths, none of which capture the pointer: PRTSCN = whole screen to the clipboard only; WINKEY+PRTSCN = whole screen to the clipboard AND saved as a PNG in Pictures/Screenshots; ALT+PRTSCN = active window to the clipboard; Screen snip = a Quick Action in Action Center and the WINKEY+SHIFT+S shortcut, which dims the screen, shows a small top toolbar (rectangular / free-form / window / full-screen), captures to the clipboard, then raises a Snip-and-Sketch toast to open the capture for crop/annotate/save/share. Optionally PRTSCN can be remapped (Settings > Ease of Access > Keyboard) to launch Screen snip.

#### MDE design

Era routing. Both subcommands branch on palette::theme() at entry (like panel.rs). Under Theme::Windows10 they render the full UI; under any other theme they print a one-line "available in the Windows 10 era" notice to stderr and exit(0) with no surface, so the binary is multiplexed but the feature is opt-in per D1. The Win10 era is added as palette::Theme::Windows10 with a win10(rgb) remap modeled on carbon() (a Win10-flavored Carbon variant per D2): same flat draw_edge, shifted accent (Win10 system blue 0x0078D7 / dark 0x2B88D8), Win10 light/dark surfaces (light 0xF3F3F3 chrome / dark 0x202020); main.rs startup gains a windows10 arm. The Theme::Windows10 enum + win10() remap + checklist pins are shared infra owned by E2 Theme; E16 only consumes them (see dependency E2).

mde clipboard (clipboard.rs) is a layer-shell history popup modeled on popup.rs, in two halves. (1) A persistent monitor: mde clipboard daemon spawns wl-paste --watch (one watcher for text, one with --type image/png) which on each clipboard change appends an entry to a ring buffer at ~/.local/share/mde/clipboard/ (text inline in a JSON index via state.rs atomic-save helpers; images as NNN.png + thumbnail). Ring capped at 25 unpinned + unlimited pinned; pinned flags persisted. The daemon is launched once from panel autostart (reuse the panel's existing process-spawn path) and is lockfile-idempotent. (2) The popup: mde clipboard (WINKEY+V) opens a transparent full-screen layer-shell overlay (same Anchor Top|Bottom|Left|Right, KeyboardInteractivity::Exclusive, click-outside/Esc-to-close pattern as popup.rs), drawing a ~320px flat Win10 card near the top-left. Each row is an mde-ui flat button (reuse widget::button via palette::color): a text snippet (first ~60 chars) or a 64px PNG thumbnail (iced image widget, already a dep), plus a pin toggle and a delete x. Clicking a row runs wl-copy with that entry's bytes (re-priming the clipboard so the next CTRL+V pastes it; labwc/Wayland has no synthetic paste injection, so we re-copy rather than auto-paste, and the toast says "Copied - paste with Ctrl+V"), then exit(0). A footer has Clear all (drops unpinned) and an empty-state (Clipboard history is empty). Pin/delete mutate the index and rebuild the view in-place.

mde snip (snip.rs) is the Screen snip front-end, a thin orchestrator over grim+slurp (no custom annotation canvas in P0; annotation = open the PNG in the user's image tool). mde snip (WINKEY+SHIFT+S, default = rect) runs slurp to get a region, pipes geometry to grim -g, writes the PNG to ~/Pictures/Screenshots/Screenshot_ts.png AND pipes it to wl-copy --type image/png (matching Win10 file+clipboard), then a notify-send toast (E7 daemon) titled "Snip saved", body=filename, actions Open and Copy-again, icon=thumbnail. No surface of our own is drawn during selection; slurp owns the dimmed-screen drag UI, the closest analog to Win10's dim snip toolbar. Modes map the Win10 toolbar onto args: rect (slurp region, default), window (the focused toplevel: raise it via wlr.rs focus() so it sits on top, then capture with slurp's geometry-free drag — the wlr-foreign-toplevel protocol carries NO window rect, so there is no geometry path; the user drags around the now-topmost window), full (grim whole output, no slurp), clip (clipboard-only, plain PRTSCN: wl-copy and NO file). Keybind mapping: PrintScreen to clip; WINKEY+PRTSCN to full; ALT+PRTSCN to window. All four modes share one capture(mode) returning an optional PathBuf (single grim/wl-copy/notify path). Pointer is never captured (grim default), matching Win10.

labwc keybinds (skel + assets rc.xml), Win10-era only, added alongside W-e/W-r: W-v runs mde clipboard; W-S-s runs mde snip rect; Print runs mde snip clip; W-Print runs mde snip full; A-Print runs mde snip window. These no-op gracefully outside the Win10 era because the subcommands self-gate on palette::theme().

Reused code: popup.rs layer-shell scaffold (anchors, Exclusive keyboard, click-catcher, Esc-close, application(namespace,update,view)); mde-ui widget::button/frame::raised/palette::color/metrics::UI_PX for all chrome; state.rs atomic JSON save + XDG-aware path helper for the index; icons.rs/icon_any for pin/delete/clear and snip-toolbar glyphs; wlr.rs focus() to raise the active toplevel for window-mode snip (geometry-free — the protocol exposes no rect); the E7 Action Center notification daemon for the snip and Copied toasts; main.rs dispatch + the mde/Cargo.toml %post symlink loop (add clipboard and snip so mde-clipboard/mde-snip resolve via argv[0]).

Backends: wl-paste --watch + wl-copy (wl-clipboard) for history/capture-to-clipboard; grim + slurp for screenshots; notify-send to the E7 daemon for toasts; files under ~/Pictures/Screenshots and ~/.local/share/mde/clipboard.

Out of scope (do not build): cloud/Microsoft-account clipboard sync, cross-device clipboard (deferred to KDE Connect E5/E18), Windows Ink / pen annotation, the in-Snip-and-Sketch markup canvas, touch edge-gestures.

### E17 Storage / Backup / Recovery

**Scope:** the Windows 10 *Settings ▸ System ▸ Storage* page, the *File History* backup/restore surface (relocated by Win10 under *Update & Security ▸ Backup*), and *Update & Security ▸ Recovery* (Reset this PC, recovery drive, advanced startup). Maps onto Fedora storage/backup/recovery tooling. Win10-era only (D4: the modern Settings app replaces the Control Panel in this era; Carbon/Win2000 keep `control-panel`).

#### Win10 behaviors distilled from the Field Guide (pp.105–127)

**Storage settings (Settings ▸ System ▸ Storage):**
- Top of page: a **Storage Sense On/Off toggle**. Below it, link **"Configure Storage Sense or run it now"** -> a sub-page with a **"Clean now" button** at the bottom (described as a friendly front-end to legacy Disk Cleanup).
- A **"This PC (C:)" usage breakdown grouped by type** — the drive labeled by its File Explorer name, with horizontal bars/rows per category (Apps & features, System & reserved, Temporary files, Documents, Pictures, etc.). Some rows read-only; some drill in. Selecting **Apps & features** opens a list where apps can be uninstalled or **Moved** to another drive (Move button greys out when not movable).
- **"View storage usage on other drives"** under a *More storage settings* header — usage for each fixed/removable device.
- **"Change where new content is saved"** — per-content-type drive dropdowns (apps, documents, music, photos & videos…).

**Backup — File History (Settings ▸ Update & Security ▸ Backup):**
- Requires a **separate drive** (2nd internal, external USB, or a network location). **"Add a drive"** lists acceptable targets; once chosen, **"Automatically back up my files"** flips On.
- **"More options"** page: **Back up now** button; **"Back up my files"** schedule dropdown (every 10/15/20/30 min, hourly [default], 3/6/12 h, Daily); **"Keep my backups"** retention dropdown (Until space is needed, 1/3/6/9 months, 1/2 years, Forever [default]); an **included-folders list** (add/remove, with the long default set: Desktop, Documents, Music, Pictures, Videos…); **"Stop using drive"** to switch target; **"Restore files from a current backup"** link.
- **Restore UI** ("Restore your files with File History"): a time-navigated browser — navigation buttons at the bottom "go back in time" through backup sets; Details / Large-Icon view toggle; preview a file by double-click; multi-select; a big green **"Restore to original location"** button; an Options gear -> **"Restore to"** an alternate path.

**Recovery (Settings ▸ Update & Security ▸ Recovery):**
- **Reset this PC** with a **Get started** button -> choose **"Keep my files"** (reinstall OS, keep account/data/installed apps; desktop apps removed) or **"Remove everything"** (wipe all). For Remove everything, a **"Clean data?"** On/Off deep-wipe option behind "Change settings".
- **Advanced startup** with a **Restart now** button -> reboots into the Windows Recovery Environment (Choose an option -> Troubleshoot -> Advanced options): Startup Repair, Uninstall Updates, Startup Settings, **UEFI Firmware Settings**, etc.
- **Create a recovery drive** wizard (Start-search "recovery"): "Back up system files to the recovery drive" checkbox, pick a USB stick (contents erased), Next -> writes a bootable rescue stick.
- Signature: `WIN+I` opens Settings.

#### MDE design

**New surfaces, all Win10-era only, drop-in `mde <sub>` layer-shell/xdg-toplevel modules following `popup.rs`/`control_panel.rs`:**

1. **`settings.rs` — `mde settings [page]`** (xdg-toplevel, iced application like `control_panel.rs`). This is the Win10 modern Settings shell (the home/category grid + a left nav rail) that E-series epics share; **E17 owns the `storage`, `backup`, and `recovery` pages** inside it. Reuses `control_panel.rs`'s window scaffold pattern (menubar dropped — Win10 Settings has none), `sysinfo.rs` for facts, `icons.rs::icon_any`, `frame::` flat Carbon widgets, `metrics::UI_PX`/`INFO_TITLE_PX`. Per-era routing: `panel.rs`/`menu.rs` already branch on `palette::theme()`; under `Theme::Windows10` the Start/settings entry launches `mde settings` instead of `mde control-panel` (D4). Pages addressable as `mde settings storage`, `mde settings backup`, `mde settings recovery` so each is independently grim-capturable and `WIN+I` (labwc keybind) opens `mde settings`.

2. **Storage page (`settings::storage` module):**
   - **Storage Sense toggle** -> backed by a small persisted struct in `state.rs` (`storage_sense: bool`, `#[serde(default)]` per §2.6) **driving a systemd user timer** that runs the cleanup action. "Configure Storage Sense or run it now" sub-page; **"Clean now"** shells the real cleanup: `dnf clean all` (via the existing `fedora::install`/`pkexec` privilege pattern) plus `rm` of `~/.cache/thumbnails`, `~/.local/share/Trash`, and journald vacuum (`journalctl --vacuum-time`), surfaced through `dialogs.rs` confirm + a freed-bytes result toast.
   - **Usage breakdown** ("This PC") — read live, no mockups: enumerate mounts and per-category bytes from a new `sysinfo.rs` function `storage_usage()` parsing `df` / `statvfs` for the root device + `du`-summarized category roots (Apps≈`/usr` + flatpak, Documents/Pictures/Videos = the XDG user dirs, Temporary = caches/trash, System = remainder). Render as horizontal bars using `frame::` fill + the new Win10 accent role (routed through `win10()`/`color()` under `Theme::Windows10`); Win10's grouped rows become a `Column` of category rows with bytes + a proportional bar.
   - **"Apps & features"** row drills into an installed-package list — built on `fedora.rs`'s package-detection helpers (`rpm_installed`/`is_installed`) and a small `rpm -qa`/flatpak enumeration, NOT `apps.rs` (which is the desktop-entry *launcher* catalogue, not an uninstall surface). Uninstall is a NEW `pkexec dnf remove <pkg>` action (reusing the `fedora`/`pkexec` privilege path) behind a `dialogs.rs` confirm. The Win10 **Move** button is greyed (no per-app drive move on Fedora — kept present-but-disabled like `control_panel.rs`'s greyed menu items, honoring the convention rather than faking it).
   - **"View storage usage on other drives"** — lists every mount from `storage_usage()` with per-device used/free bars.

3. **Backup page (`settings::backup` module) — File History over Timeshift + `rsync`:**
   - **"Add a drive"** lists candidate targets from `storage_usage()` (removable + non-root fixed mounts). Choosing one writes `backup_drive` to `state.rs` and configures **Timeshift** (the named backend) via `pkexec timeshift --snapshot-device <dev>`; "Automatically back up my files" On schedules a **systemd user timer** at the chosen cadence.
   - **"More options"** page maps 1:1: **Back up now** -> `pkexec timeshift --create --comments "File History"`; **schedule dropdown** -> writes the timer `OnCalendar=` (hourly default); **retention dropdown** -> Timeshift's keep-N config; **included-folders list** -> seeds Timeshift's rsync include set from the XDG user dirs (Desktop/Documents/Music/Pictures/Videos by default), add/remove via `filedialog.rs` folder pick; **"Stop using drive"** clears `backup_drive`.
   - **Restore UI** (`mde settings backup --restore`, also reachable from the "Restore files from a current backup" link): a `files.rs`-style browser over `timeshift --list` snapshots with bottom **prev/next time-navigation buttons** ("go back in time"), a Details/Large-icon toggle (reuse `files.rs` view modes), multi-select, a **green "Restore to original location"** primary button colored from a NEW `RESTORE_PRIMARY` Win10 palette role (Win10 success-green) routed through `win10()`/`color()` — no raw color at the call site (§2.1) — wired to `pkexec timeshift --restore --snapshot <id>`, plus an Options gear -> **"Restore to…"** alternate path via `filedialog.rs`. Confirm/replace prompts via `dialogs.rs`.

4. **Recovery page (`settings::recovery` module):**
   - **Reset this PC -> Get started**: `dialogs.rs` two-mode chooser — **"Keep my files"** maps to a non-destructive **`dnf distro-sync` + reinstall of the base group** (`@core`/`@workstation-product`) keeping `/home`; **"Remove everything"** is surfaced but, being genuinely destructive (§0.5 / CLAUDE.md), gates behind an explicit typed-confirm `dialogs.rs` modal and **only prints/launches the official reinstall path** (boots the install media / `mde setup`, reusing `installer.rs`) rather than silently wiping — the "Clean data?" deep-wipe toggle maps to a documented `blkdiscard`/`shred` warning, executed only on typed confirmation.
   - **Advanced startup -> Restart now**: reboots into the firmware/GRUB rescue menu — `pkexec systemctl reboot --firmware-setup` (the UEFI Firmware Settings equivalent) for the "boot into recovery environment" action; Startup Repair/Uninstall Updates map to `dnf history undo last` (the named Update-undo backend) behind confirm.
   - **Create a recovery drive** (`mde settings recovery --usb-drive`): a wizard mirroring `installer.rs`/`tui_setup.rs` — pick a USB device from `storage_usage()` removable list, "back up system files" checkbox, an erase-warning confirm, then write a bootable Fedora rescue image (`dd`/`livecd-iso-to-disk` of the install ISO) with a progress bar reusing the installer's progress widget.

**Theme routing / `Theme::Windows10`:** add `Theme::Windows10` to `palette.rs` `enum Theme` (4th variant; THEME atom value `3`), a `win10(rgb)` remap modeled on `carbon()`, and a `win10()` arm in `color()`. Per D2 it is a Win10-flavored Carbon variant: reuse the flat Carbon widget bevels (`draw_edge` flattening stays), and introduce a NEW Win10 system-accent (Win10 blue) — a real UI-accent palette role, since today `palette::accent()` resolves to Carbon Blue 60 and `set_accent` is only an icon-tint hue index, so the configurable accent is built here, not inherited. Surfaces shift to Win10 light (page/card) / dark; the illustrative targets (e.g. `#0078d4` accent, `#f3f3f3` page / white cards / `#202020`/`#2b2b2b` dark, plus the `RESTORE_PRIMARY` green) become NEW palette role constants defined **only in `palette.rs`** and routed through `win10()` — **no raw hex outside `palette.rs`** (§2.1). `main.rs` startup gains a `"windows10" => Theme::Windows10` arm in the `st.theme` match (D1: additive, Carbon stays default). `set_dark` already carries Carbon light/dark mode and is reused for the Win10 light/dark split (D2).

**Reused code:** `control_panel.rs` window scaffold + greyed-disabled convention; `fedora.rs` (package-detect helpers + the `pkexec`/install privilege path, extended with a `dnf remove` uninstall action for Apps & features); `files.rs` (restore browser, view modes); `filedialog.rs` (folder include/exclude, Restore-to); `dialogs.rs` (confirm/replace/typed-destructive modals, reboot); `installer.rs`/`tui_setup.rs` (recovery-drive wizard + progress); `sysinfo.rs` (new `storage_usage()` parser, unit-tested headless); `state.rs` (`storage_sense`, `backup_drive`, schedule/retention fields, all `#[serde(default)]`); `icons.rs::icon_any`; `frame`/`metrics`/`palette` from `mde-ui` (with new Win10 accent + `RESTORE_PRIMARY` role constants). Note: `apps.rs` is NOT reused for uninstall — it is a desktop-entry launcher catalogue and has no package-removal capability.

**Compositor boundary:** all new windows are xdg-toplevels (labwc draws frames) or layer-shell sub-pages; mde draws only client areas. The recovery-drive write and resets shell out to system tools — mde never becomes a partition/installer itself beyond reusing `installer.rs`.

### E18 Edge -> Firefox integration (default-browser surfacing) — Win10 era

**Scope (locked D12):** integration only — Firefox stands in for Microsoft Edge as the era's default web browser, surfaced where Win10 surfaces Edge (Start, taskbar/Quick Launch pin, File Explorer Quick Access, jump list). **No browser rebuild.** OneNote and People are out. This is the Win10-era veneer over the existing Firefox launch points the shell already has.

#### Win10 behaviors distilled (Field Guide pp. 254-259, 249-251, 6, 227)

- **Edge is *the* bundled browser**, pinned to the taskbar and present in Start by default; "we highly recommend using it." It is the registered default for the web (`http`/`https`) and HTML.
- **Default apps** live in *Settings > Apps > Default apps* (p. 249-250): a category list where each row shows the *currently configured* app and clicking it pops a chooser. The legacy "Set Default Programs" control panel was replaced by this Settings surface (D4: Settings replaces the Control Panel **in the Win10 era only**). The field guide's full category list (Email, Web browser, Maps, Music player, …) is recorded for context only — **MDE surfaces only the categories with in-scope backends: Email and Web browser.** Maps and Music player are OUT OF SCOPE (no Maps/Groove surface) and are NOT rendered as default-apps rows.
- **Jump lists** (p. 6, p. 227 "Show recently-opened items in Jump Lists"): right-clicking a taskbar button (or a Start tile, or a Quick Access entry) shows an app-specific right-click menu. For a browser it lists *recently closed web pages / recent sites*; for File Explorer it lists Frequent locations. Jump lists also surface pinned/common tasks (e.g. "New window"; the field guide's Edge wording is "New InPrivate window" — **the shipped Firefox equivalent is "New private window"**, since the shell never emits an Edge-branded label for a browser it doesn't ship).
- **Pin web pages to the taskbar** (p. 257): Edge's "… > More tools > Pin to taskbar" pins a site as a taskbar shortcut. The three rightmost taskbar shortcuts in the screenshot were sites pinned from Edge. (PWA install chrome is browser-internal and out of scope — see below.)
- A global toggle (Settings) governs whether jump lists show recent items at all; default on.

#### MDE design

**Era routing.** Everything below branches off `palette::theme()`; the new `Theme::Windows10` arm (added in the E13 theme epic, `win10(rgb)` remap modeled on `carbon()`) only changes *labels, icon, and styling* — the underlying launch is `firefox` on every era. Carbon remains the **default** theme; Win10 is opt-in (D1). Under Win2000/Carbon the existing `menu.rs` "Firefox" leaf (`menu.rs:232`) and `apps.rs` WebBrowser handling are untouched (this epic adds no out-of-era behavior).

**1. Default-browser registration & surfacing (`state.rs` + new `browser.rs`).**
New small module `mde/src/browser.rs` owns the era-agnostic facts:
- `pub fn default_browser() -> Browser` resolves the system default by shelling `xdg-settings get default-web-browser` (already on PATH) and mapping the returned `.desktop` id to a display name + `icon_any` key; falls back to `firefox` when unset or unreadable. No raw hex; it returns names/commands only. The desktop-id → name/icon mapping is a pure function (unit-testable without touching the system default).
- `pub fn launch_url(url: &str)` / `pub fn launch()` spawn the resolved browser (`xdg-open <url>` for URLs, the browser desktop-exec for a bare window), reusing the same fire-and-forget `Command::spawn` pattern `menu.rs`/`files.rs` already use (`let _ = Command::new(...).spawn()`).
- `pub fn set_default_cmd() -> Vec<Command>` / `pub fn set_default()` build (and, for `set_default`, run) `xdg-settings set default-web-browser firefox.desktop` (+ `xdg-mime default firefox.desktop x-scheme-handler/http x-scheme-handler/https text/html`) — the Linux backend for "make Firefox the default", invoked from the Settings default-apps row (E15 Settings) and from a first-run nudge. The command-builder (`set_default_cmd`) is exposed separately so it can be asserted in a test without mutating the developer's real session default.

Per-era *label/icon* of the browser entry routes here. The browser entry always uses the **real product name "Firefox"** — the shell never fakes a brand it doesn't ship (§2.4 "don't launder the gap"); there is no "Microsoft Edge" label anywhere. The Win10 flavor is the *placement and styling* (a live tile, taskbar pin, jump list), not a fake brand. Icon = embedded `firefox.svg` (already shipped at `mde/src/embedded_icons/firefox.svg`), tinted via `icon_any`.

**2. Start tile + taskbar/Quick-Launch pin (`menu.rs`, `panel.rs`, `state.rs`).**
- In the Win10-era tiled Start (E16/D6), the left rail + tiles include a Firefox *medium tile* as a default pin, built from `browser::default_browser()`. Carbon/Win2000 Start keeps the existing flat "Firefox" leaf — `build_root()` already lists it; only the tiled-Start builder (E16) consults `browser.rs` for the tile.
- `state.rs::default_state()` seeds the Firefox pin into `pinned` for the Win10 era so it appears in the taskbar Quick Launch strip that `panel.rs` already renders from `self.pinned`. No new persistence shape beyond the existing `PinnedItem { name, command }`, so §2.6 state-compat holds (every field already `#[serde(default)]`; the `parse("{}")`/`Default` agreement test must stay green).
- "Pin web page to taskbar" (p. 257) maps to: a `PinnedItem { name: <site>, command: "xdg-open <url>" }` appended to `pinned` and saved via the existing atomic `state::save()`. Reuse the URL → command shape `search_tree()` already uses (`menu.rs:292`, `xdg-open https://…`).

**3. Browser jump list (new `mde browser-jumplist` layer-shell surface).**
A drop-in subcommand modeled on `popup.rs`/`menu.rs`:
- New `mde/src/browser_jumplist.rs` exposing `pub fn run(args)` — a single `iced_layershell` popup anchored bottom-left (Win10) like the existing popups, styled through the active theme edge (flat Win10/Carbon list rows, Win2000 raised when in that era). Reuses `popup.rs`'s `Item`/`Popup` row pattern and the same `mde_ui::{frame, button, palette, metrics}` styling (`.size(metrics::UI_PX)`), so no new metric/hex literals. **WM boundary respected:** this is a layer-shell popup only; it draws no title bar / frame (labwc owns those).
- Sections, matching the Win10 jump list:
  - **Tasks:** "New window" (`firefox --new-window`), "New private window" (`firefox --private-window`).
  - **Recent:** the last N visited URLs, read from Firefox's own history — `~/.mozilla/firefox/<profile>/places.sqlite`, the `moz_places.last_visit_date`/`url`/`title` columns, top 8. Read via a bundled **read-only `rusqlite`** (the one new dependency; chosen over a `sqlite3` CLI shell-out because `sqlite3` is not guaranteed on PATH and would otherwise need its own RPM dep). Each row is `Act` = `browser::launch_url(url)`. When the DB is locked (Firefox running) we copy-to-temp + open read-only with `?immutable=1`, the standard `places.sqlite`-busy workaround; on any failure the Recent section is simply omitted (no stub row).
  - A footer "Unpin from taskbar" when invoked from a pinned button.
- Dispatch: add `"browser-jumplist" => browser_jumplist::run(rest)` to the `match cmd` in `main.rs` (alongside `popup`/`menu`), an `mde-browser-jumplist` `%post` symlink, and **no** labwc `rc.xml` keybind (it's invoked by right-clicking the taskbar/tile button). `panel.rs` spawns it on a right-click of the Firefox Quick-Launch button: panel already routes `StartContext`/`TaskbarContext` right-clicks and reaps spawned children each tick via `push_child`/`spawn_child` (`panel.rs:901`/`:906`, `try_wait` at `:216`); add a `QuickLaunchContext(idx)` message arm that calls `push_child(spawn_child(&["browser-jumplist", "--pin", &idx]))`.

**4. Default-apps rows in Settings (defers to E15, scoped).** In the Win10 Settings > Apps > Default apps surface (E15), MDE renders rows **only for in-scope categories: Email and Web browser** (Maps/Music are out of scope, D-list — no row, no chooser). The "Web browser" row reads `browser::default_browser()` for its current value and calls `browser::set_default()` from the chooser. This epic ships `browser.rs` with those two entry points so E15 wires the row without new logic; the row itself is E15's story, not E18's.

**Reused code:** `embedded_icons/firefox.svg` (already shipped) via `icons::icon_any`; the `xdg-open` spawn pattern from `files.rs`/`menu.rs`; `popup.rs` row/anchor scaffolding; `state.rs` `PinnedItem` + atomic `save()`; `panel.rs` Quick-Launch render + `push_child`/`spawn_child` reaping; the single `palette::color` theme edge. **Linux backends:** `xdg-settings`/`xdg-mime` (default registration), `xdg-open`/`firefox` CLI (launch), Firefox `places.sqlite` via read-only `rusqlite` (recent/jump-list history). **New dependency:** `rusqlite` (read-only). No compositor patching.

**Out of scope (must not appear):** importing bookmarks/sync UI, tracking-protection settings, search-engine config, Immersive Reader, PWA install chrome (p. 254-259) — those are browser-internal, not shell integration. Maps and Music-player default-apps rows. OneNote, People.

### E20 Polish + accuracy (Windows 10 era) — cross-cutting parity gate

This epic owns no new user-facing surface. It is the accuracy/parity gate that makes the Win10 era a real, shippable fourth theme on equal footing with Win2000/BeOS/Carbon, under the same Definition of Done (CLAUDE.md §3) and the same accuracy harness (CLAUDE.md §4). It pins `Theme::Windows10` (authored by E10) into the static checklist, extends the screenshot gallery to capture every era, and verifies keyboard navigation parity across all four eras. It is the dependency sink for E10-E19: each P0 surface lands its capture/pin here so the gate stays green. Per D1 this is additive — Carbon stays the default and Win10 is opt-in.

#### What Windows 10 actually does (the parity bar to hit)

Win10 is a coherent system, not a bag of apps: a consistent accent color flows through Start, the taskbar, Action Center, Settings, and selection highlights; every chrome surface honors the light/dark app mode; and every control is reachable by keyboard (Tab/Shift+Tab focus rings, Enter/Space activate, arrow keys within lists/tiles, Esc dismisses popups). The signature global hotkeys (Win, Win+A action center, Win+Tab task view, Win+I settings, Win+S search, Win+arrows snap, Win+Ctrl+D/F4 virtual desktops) form a learnable keyboard map. The polish bar for parity is therefore: one accent edge, one app-mode edge, consistent focus rings, and a complete keyboard map — verified by screenshot, not eyeballed.

#### MDE design

1. `Theme::Windows10` is the 4th variant on the one theme edge (`mde-ui/src/palette.rs`). Per D1/D2 this epic does **not** author the remap — E10 owns adding the variant to `enum Theme`, the `theme()` atomic value (`3 => Theme::Windows10`), `set_theme`, an `is_windows10()` helper mirroring `is_carbon()`, the `color()` arm (`Theme::Windows10 => win10(rgb)`), and **broadening the flat-widget predicate**. The flatten path in `widget/mod.rs::draw_edge` (and the scrollbar rail / `border_radius`) is currently gated on `palette::is_carbon()` alone, so a naive Win10 variant would fall through to the 3D bevel path and violate D2; E10 must widen that predicate to `is_carbon() || is_windows10()` (or an `is_flat()` helper). This epic does not assume the flatten "already" fires for Win10 — it **pins** that it does (see point 2). E20 owns the pinned ground truth and the era captures for E10's authored remap.

2. Static pins live in `mde-ui/tests/checklist.rs` (§2.2). A new test `windows10_remap_is_pinned` asserts the *output of `palette::color()`* for the load-bearing roles under `Theme::Windows10`, exactly as `carbon_sentinels_and_header_are_pinned` does for Carbon — no raw hex is authored in the test or this SPEC (§2.1): it asserts the role constants (`palette::color(palette::HIGHLIGHT)` / `ACTIVE_TITLE` for the accent, the white-text sentinel `HIGHLIGHT_TEXT` = `0xff,0xff,0xfe` staying light on accent/dark chrome, the window-frame sentinel `WINDOW_FRAME` = `0x00,0x00,0x01` mapping to the Win10 chrome line, and the desktop/surface tokens for both app modes via `set_dark(true/false)` toggled inside the test). The concrete Win10 accent value lives in `palette.rs` (authored by E10, backed by the Win10 reference asset per §2.2), not here. This makes any silent drift in E10's `win10()` fail CI immediately — Win10 held to the same no-eyeball standard as the other three eras. The existing `bevel_*` and `app_chrome_colors_are_pinned` tests are theme-agnostic and continue to gate.

3. The `mde-ui` bevel/draw-edge unit test gains a `Theme::Windows10` case **pinning** that the flat (non-3D) path fires — i.e. that E10 broadened the predicate. This is an assertion, not a passive confirmation: if Win10 ever takes the 3D bevel branch, the test fails, holding D2 (reuse the flat Carbon widgets).

4. Per-surface gallery captures span all four eras (`tests/accuracy/gallery.sh`). Today `gallery.sh` captures the Carbon default and crops the top strip. This epic generalizes it to loop over eras by setting the state file before each launch (the shell reads `~/.config/mde/menu.json` per process — §7). The `shot()` helper gains an era dimension: write a throwaway `menu.json` (theme + theme_mode) into an isolated `XDG_CONFIG_HOME`, launch the component, capture, restore. The panel crop becomes era-aware (Carbon/Win10 -> `0,0 1280x40` top; Win2000 -> `0,920 1280x40` bottom; BeOS -> `0,0 120x960` left), keyed off the era being shot — the §7 anchor table. Output lands under `captures/gallery/<era>/<component>.png`, and the contact sheet becomes one-per-era. The new Win10 surfaces (E11 Action Center, E12 Settings, E13 Start, E14 Task View, E15 Search) register as gallery components here so a single `./preview.sh gallery` proves the whole era renders.

5. Dynamic accuracy points for Win10 (`tests/accuracy/checklist.toml` + `mde/tests/accuracy.rs`). The harness already decodes a capture and spot-checks pixels within tolerance. This epic adds `[capture.win10-*]` groups asserting the Win10 accent paints where it should (Start rail highlight, taskbar search box, Action Center toggle tile, Settings selected-nav row), grounded in the `win10()` token values authored by E10 — the dynamic mirror of the static pins, again referencing the palette role rather than naming hex. The harness's skip-on-missing-capture behavior means partial era runs still verify what they have; the only `accuracy.rs` change is teaching it the era-subdir capture path (resolve `captures/gallery/<era>/<file>` when a `[capture.win10-*]` group names one).

6. Keyboard-nav parity is bench-observable per surface. iced gives focus traversal for free, but the signature Win10 keyboard map (the global hotkeys) is delivered by labwc `rc.xml` keybinds that launch the new `mde <sub>` surfaces (E11-E15 each add their bind via the `%post` rc.xml fragment). This epic owns the parity check: a no-panic launch per era confirming each surface maps and accepts focus, plus a documented rc.xml keybind table (Win+A -> `mde action-center`, Win+Tab -> `mde task-view`, Win+I -> `mde settings`, Win+S -> `mde search`, Win -> `mde menu`) verified by `timeout 3 mde <sub>` exiting clean and a grim capture showing the focus ring on first focusable. Per D4 the Win2000/Carbon/BeOS eras keep Control Panel; the table is era-gated in the `%post` fragment so Win+I routes to `mde settings` only when the active theme is Windows10, else `mde control-panel`. The fragment starts `<mouse><default/>` per §7. labwc draws all title bars/frames/z-order; mde draws only its layer-shell + client surfaces (compositor boundary), so no keybind makes mde a window manager.

7. Reused code: `palette.rs` (the one theme edge), `widget/mod.rs` `draw_edge` flatten path (predicate broadened by E10), `checklist.rs` pinning pattern, `gallery.sh`/`accuracy.rs`/`checklist.toml` harness, `state.rs` for the throwaway `menu.json`, the §7 anchor table in `panel.rs`. No new compositor patching: every keybind is an rc.xml `<keybind>` launching an existing `mde` subcommand.

## Open risks

**R1 — No pixel buffers from wlr-foreign-toplevel (E2, E4).** Live hover thumbnails, Task View tiles, and Snap Assist cards can only show **icon+title** in P0; real thumbnails need a heavier labwc screencopy / `ext-image-capture` path deferred to M2/M3. *Decision:* accept icon+title cards as the honest P0 substitute (recommended), or scope a screencopy thumbnail cache as a separate epic.

**R2 — `ext-workspace-v1` may be absent from the pinned `wayland-protocols` crate (E4).** Virtual desktops then need a hand-written protocol XML + scanner (as `wlr.rs` vendors wlr-foreign-toplevel) or a `state.rs`-backed labwc-IPC fallback strip. Highest-uncertainty backend dependency in the whole release.

**R3 — Notify daemon ownership + coexistence (E3, cross-cutting).** Panel-hosted (dies/churns with panel restarts) vs. a supervised `mde notifyd` from labwc autostart. And: yield silently to a pre-existing mako/gnome-shell owner of `org.freedesktop.Notifications`, or packaging-conflict against mako so MDE wins under the Win10 era? Both affect whether E9/E12/E13/E15/E16/E17 toasts are reliable.

**R4 — KDE Connect host-layer landing order (E8, E9).** Does MDE-KDECnt-Rust's LAN transport land before E8/E9, or must they ship against the on-disk pairing store alone for the first cut? Gates whether *live* device browse is in M2 scope. Also: SMS thread/history read vs. send+inbound-only (Messages mirrors Win10's "only new notifications" limit if the latter); contact auto-complete source; per-vendor DCIM camera path.

**R5 — `rc.xml` packaging shape + the `<mouse><default/>` trap (cross-cutting).** Two per-era skel `rc.xml` variants vs. one always-on Win10 keymap with surfaces self-suppressing off `palette::theme()`. Any `<mouse>` edit that omits `<default/>` silently kills all mousebinds (§7) — must be guarded in the %post fragment and any era-switch tooling.

**R6 — Settings page-registration API (E6, E12, E13, E15, E17).** Per-epic drop-in page modules vs. one monolithic match; one long-lived Settings process vs. independent per-page processes (grim-capturable, matches the one-process-per-subcommand model). This decision ripples through every M2 Settings category.

**R7 — Privilege escalation policy (E6, E7, E10, E11, E12, E14).** `pkexec`/PolicyKit is the assumed path for `useradd`, `/etc/lightdm` writes, `/var/lib/AccountsService` icons, LUKS, BlueZ pairing. Confirm pkexec over a setuid helper across the board, and confirm LightDM-gtk-greeter is guaranteed installed (packaging dep + %post vs. assumed present) in the Win10 image.

**R8 — Destructive-op ceilings (E13, E14, E17).** Locked conservative stances to re-confirm: device-encryption is **advisory-only** (render the `luksFormat` command, never run it); "Reset this PC" never silently wipes (typed-confirm + official install media); Storage Sense auto-clean is opt-in and capped to caches/Trash/journal (never user documents/Downloads). "Uninstall updates" gating behind a `dialogs.rs` confirm given `dnf history undo` can roll back security fixes.

**R9 — Win10 accent + taskbar metric pins (E0, E20).** Pin the default accent (`#0078d4` light / `#2899f5` dark, `#1f1f1f` surface) so `checklist.rs` can assert it; decide whether to expose the full Win10 accent palette in E0/E7 now or reuse Carbon icon-accent atoms until E7's Colors page. Taskbar: reuse the Win2000 bottom branch or add a distinct `WIN10_BAR_H` (~40px) — and whether to constrain Win10 to Top/Bottom (avoiding a vertical-taskbar rewrite in `panel.rs`) despite Win10 supporting Left/Right.

**R10 — Honest departures from the screenshots (E8, E2, E5).** Distilled single always-visible Explorer command row instead of the tabbed Home/Share/View ribbon; Win10-era Search hidden/keybind-only vs. a visible box; Miracast/wireless-display has no practical Wayland sink (omit vs. disabled affordance). Each is a deliberate, recorded fidelity drop under §2.4-style honesty.
