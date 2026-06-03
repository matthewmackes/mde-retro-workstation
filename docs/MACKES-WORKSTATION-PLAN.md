# Mackes Workstation — Fusion Plan (PLANNING ONLY)

> **Status:** discovery + architecture + decision-space complete; **blocked on the
> 50 questions below** before it becomes an executable, sequenced plan. Nothing in
> here is built. Interim home in MDE-Retro `docs/`; the real home depends on **Q1**
> (repo shape). Generated 2026-06-03 from an exhaustive parallel review of both
> repos (119 features inventoried across MDE + MDE-Retro).

## 0. Decisions (answered by the owner)

*Load-bearing four — 2026-06-03:*
- **Q8 Compositor → labwc.** Keep MDE-Retro's labwc substrate; MDE's sway-specific
  bits adapt to labwc/wlroots. The Win10 shell is the primary UX.
- **Q16 mesh-storage → PRECONDITION, on LizardFS (NOT Gluster).** *(Revised
  2026-06-03 against current MDE @ `6459e17`.)* Shared **mesh-storage** is a
  precondition (the Workstation is not single-box-only), implemented by **LizardFS**
  — `docs/design/v5.0.0-mesh-storage-lizardfs.md` (locked 2026-05-29) supersedes the
  GlusterFS design **wholesale** (Gluster never shipped; no migration). Depends on
  the Nebula fabric + the Bus. Drop **all** Gluster assumptions; the MESHFS-* tasks
  are the LizardFS path. Mesh-storage `XDG` dirs are LizardFS-mounted.

*Resolved by the refresh against current MDE (see §0.1):*
- **Q4 terminology → MDE = "MackesDE for Workgroups" (a.k.a. MDE4WG)**, the platform,
  releasing as **v10.0.0**. "Mackes Workstation" is the new Win10-shell fusion on top.
- **Q11 D-Bus vs Bus → Bus.** The platform is retiring all MDE-internal D-Bus to Bus
  topics (EPIC-RETIRE-DBUS; only FDO interop like `org.freedesktop.Notifications`
  stays). Mackes Workstation surfaces consume the **Bus**, not internal D-Bus.

*Batch 2 — 2026-06-03:*
- **Surfaces → the Win10 shell REPLACES `mde-portal`.** One daily-driver shell
  (MDE-Retro's Win10). The Portal is retired/absorbed — its functions reappear as
  Win10 idioms (Start/Settings/Explorer/Action Center); its distinctive power-user
  layers (mesh roster, tags, library) move to the **Workbench**.
- **Q9 daemon → `mackesd` as a supervised service.** It owns the workers + mesh/CA
  state; the Win10 shell + Workbench consume state and send actions over the **Bus**
  (no in-process worker pool in the shell).
- **Q26 KDE Connect → `MDE-KDECnt-Rust` is canonical.** The monorepo depends on the
  extracted proto+host crate; MDE's in-tree `mde-kdc` host converges onto it. (The
  in-progress host 3b work is already on the canonical path.)
- **Q40 Workbench → RE-SKIN: Windows-10 design concepts in the Microsoft Server
  2003 "Manage Your Server" mold, wearing the platform's color + icon theme.**
  *(Refined by owner 2026-06-03.)* The Workbench is a **task/role-oriented admin
  console** — the "Manage Your Server" pattern: a left nav (the 9 groups / 43
  panels) + a main pane of **role/section cards**, each with a heading, a short
  descriptive paragraph, and **action links** ("Add…", "Manage…", "Configure…"),
  plus a "Tools / See also" sidebar — rendered with **Win10 design concepts** (flat
  surfaces, accent, the Win10 control idiom). It **inherits the overall platform's
  color theme** (the `palette::color` Carbon/Win10 palette — no separate Material
  palette) **and icon theme** (the platform icon set via `icon_any`). *Implications:*
  unify on the MDE-Retro `mde-ui` flat-Carbon widget kit + palette + icons across
  both surfaces (resolves Q23/Q24 → one design system); the 43 Workbench panels are
  re-laid-out into the role-card/action-link "Manage Your Server" structure, not just
  re-colored. Keep the Material/ChromeOS-Classic content OUT.
- **Q1 Repo → MONOREPO.** One cargo workspace absorbs MDE's platform crates +
  MDE-Retro's shell + the Workbench as members. The existing repos become upstream
  history. (This is the final home for *this* plan once the repo exists.)
- **Q5 v1 scope → EVERYTHING.** v1 = the full fusion: mesh, fleet/playbooks, VoIP,
  compute (KVM/Podman), music, all 43 Workbench panels, + the Win10 shell. The RPM
  is held until all of it is ready; hardware bench follows release.

*Implications:* labwc + Gluster-mandatory + monorepo + full-scope means the plan
optimizes for one large integrated workspace, not a minimal shell. Per-crate reuse
is maximal (shared crates in one workspace). Remaining questions refine the daemon
model, the bus, KDE Connect convergence, Win10-vs-Workbench placement, and the
Workbench's look.

*Batch 3 — placement, 2026-06-03:*
- **Q34 first-run → merged Win10 OOBE + mesh enrolment.** The shipped Win10 OOBE
  gains Birthright's stages (mesh enrolment/pairing, DND) as Win10-styled steps —
  one first-run that also enrols the node on the mesh.
- **Q31 VoIP → a Win10 "Phone/Calls" app + incoming-call toast/HUD** (reuse
  `mde-voice` + `mde-voice-hud`).
- **Q32 compute → a Workbench role** ("Virtualization & Containers" — KVM + Podman),
  fitting the Manage-Your-Server console.
- **Q29 files → Win10 Explorer primary; mesh/peer locations + mesh-storage fold into
  Quick access** (reuse `mde-files`' mesh-browse backend; retire its separate UI).

## 0.1 Information currency

The original review snapshot was **65 commits stale**; refreshed against current MDE
**`6459e17`** (2026-06-03). Material deltas since the snapshot, folded into this plan:
- **GlusterFS → LizardFS** for mesh-storage (Gluster fully retired).
- Platform release is **v10.0.0 "MackesDE for Workgroups"** (= MDE / MDE4WG).
- **D-Bus retirement** in progress (EPIC-RETIRE-DBUS) → Bus topics; FDO interop only.
- **`mde-portal` has grown into a major surface** (Library / Control / Network layers,
  a tag system, the Birthright wizard rendered inside Portal-full) — relevant to the
  Win10-shell-vs-Portal-vs-Workbench surface question.
- **QNM-Shared → `workgroup_root`** (EPIC-RETIRE-QNM); a large native **music**
  player (`mde-music`/`mde-musicd`, AIR-*) shipped.
- **Before producing the executable plan, the full feature inventory should be
  re-derived against `6459e17`** — the architecture decisions hold, but per-crate
  details (Portal scope, music, mesh-storage) moved.

## 1. Vision

**Mackes Workstation** is a new project that fuses two existing codebases:

- **MDE** (`github.com/matthewmackes/MDE`) — a Rust *platform*: ~37 crates providing
  encrypted mesh (Nebula), KDE Connect, a supervised daemon (`mackesd` + ~40
  workers), GlusterFS mesh-home, a pub/sub bus, voice/VoIP, music, files, and a
  power-user **Workbench** console.
- **MDE-Retro** (this repo) — a Windows 10 / IBM Carbon **shell**: the daily-driver
  desktop UX (taskbar, Start, Settings, Explorer, Action Center, Search, Task View,
  OOBE) on labwc, with a mature reusable design system.

The result: **MDE's platform sits *underneath* to empower the MDE-Retro Win10
shell, with reuse as the spine.** Anything that doesn't fit the Windows 10 idiom
goes to the **Workbench** surface (MDE's `mde-workbench`). **REUSE IS KEY** — new
code is glue, not reimplementation.

## 2. Architecture (proposed — every fork maps to a question below)

```
┌─ Win10 Shell (MDE-Retro, primary UX) ──────────┐   ┌─ Workbench (mde-workbench) ──────┐
│ taskbar · Start · Settings · Explorer · Action │   │ ChromeOS-Classic/PatternFly      │
│ Center · Search · Task View · OOBE · flyouts   │   │ console for non-Win10 features:  │
│ surfaces platform features in the Win10 idiom  │   │ fleet · mesh · maintain · netadmin│
└───────────────┬────────────────────────────────┘   │ presets · compute · metrics       │
                │  launched from Start ("Mackes Workbench") └──────────────┬──────────────────┘
┌───────────────┴──── Platform substrate ("underneath") ──────────────────┴──────────────────┐
│ mackesd + workers · mde-bus · KDE Connect (mde-kdc-proto, shared) · Nebula mesh · Gluster    │
│ mesh-home · config · metrics — consumed as libraries + a supervised daemon                   │
└─────────────────────────────────────────────────────────────────────────────────────────────┘
```

## 3. Feature inventory (119 features, by target layer)

**Platform — underneath (40):** `mackesd` daemon + ~40 workers · `mde-bus` (pub/sub,
replacing internal D-Bus) · `mde-session` · `mde-config`/`mackes-config` · Nebula
mesh (`mackes-nebula-https-tunnel` + CA/lighthouse in `mackesd`) · **KDE Connect**
(`mde-kdc-proto` + host — already being extracted to `MDE-KDECnt-Rust`) · GlusterFS
mesh-home · Netdata metrics + `mde-alert-emit` · `mde-musicd` · `mde-voice-config` ·
`mde-clipd` · `mde-installer` · plus MDE-Retro's platform bits (one-binary
dispatcher, accuracy harness, `wlr.rs` window control, outputs/bluez/cups/mount/etc.).

**Win10 surface (38):** the whole MDE-Retro Win10 set — panel/taskbar, tiled Start,
Settings, Explorer, Action Center, Search, Task View + virtual desktops, OOBE,
security dashboard, network flyout, clipboard/snip, lock/power, browser, About,
System Properties, Run, context menus, tray.

**Workbench surface (20):** `mde-workbench` — 240px sidebar, **9 groups / 43 panels**:
Dashboard · Apps · Devices (9) · **Fleet** (inventory/playbooks/run-history/settings/
revisions) · Look & Feel (4) · **Maintain** (hub/snapshots/debloat/health/repair/
drift) · **Network** (13: wifi/mesh-control/pending/history/join/ssh/topology/services/
bus/federation/service-publishing/vpn/firewall/remote-desktop) · System (5) · Help ·
plus the **Preset/drift engine** (Hashbang/Mackes/Daylight/Vanilla/Node variants).

**Shared libs (15):** MDE's `mde-theme` · `mde-iced-components` · `mde-card` schema ·
mesh-types; MDE-Retro's `palette.rs` color() edge · `metrics.rs` · flat Carbon
widget set · bevel · icon-resolution chain · tolerant-serde `state.rs` · font system.

**Ambiguous (6) — placement is a question:** `mde-files` (mesh-first manager) ·
`mde-music` · **`mde-voice`** (PJSIP softphone/VoIP) · **`mde-virtual`** (KVM+Podman
compute) · `mde-drawer` (quick actions) · `mde-wizard` (Birthright first-run).

## 4. Reuse strategy (REUSE IS KEY)

- **New repo `mackes-workstation`** wires the three layers together; new code is glue.
- **Shared crates as the spine:** `mde-kdc-proto` is already shared; the platform
  crates become libraries (+ `mackesd` as a supervised service); MDE-Retro's
  theme/widget/state/icon kits power the Win10 shell; the Workbench ports in largely
  as-is.
- **Two design languages coexist by design:** Win10/Carbon for the shell, Material/
  ChromeOS-Classic for the Workbench (the deliberate "power-user contrast").
- Per-crate reuse classification (as-is / adapt / rebuild) is the **next deliverable**
  (pending the load-bearing answers).

## 5. Standing requirements (owner-set, 2026-06-03)

- **Disclaimer on every surface.** The Mackes Workstation Warning/Disclaimer/Mission
  ([`../DISCLAIMER.md`](../DISCLAIMER.md)) appears on all About + informational
  surfaces. Already wired in MDE-Retro (About dialog, System Properties, READMEs,
  embedded via `disclaimer.rs`/`include_str!`). Any **new** surface in Mackes
  Workstation (Workbench About/Help, the platform daemon `--version`/banner, the
  installer) must pull from the same single source.
- **Hold the RPM** until **all features are ready**; do not cut a release before then.
- **Hardware-bench testing happens *after* release** — so hardware-/interactive-gated
  features (KDE Connect phone round-trips, BlueZ pairing, PAM unlock, live dnf
  streaming, vertical-taskbar UX) are *build-complete* now; their device/UX bench is
  a post-release step, not a blocker for "ready."

## 6. The 50 questions (the decision gate)

### A · Strategy, repo & scope
1. Repo shape: monorepo absorbing both codebases, or a thin integration repo that git-deps them?
2. Does MDE-Retro continue independently, or is it subsumed (and archived)?
3. Does MDE continue shipping independently, or become "the platform layer of Mackes Workstation"?
4. Terminology: which is "underneath" (MDE) vs "on top" (MDE-Retro)? What does **MDE4WG** stand for?
5. MVP for v1: which features must ship first vs deferred?
6. Audience: personal/home, small-fleet ops, or a distributable product?
7. Versioning: own line, or track MDE's planned 1.0 "brand reset"?

### B · The fusion architecture
8. **Compositor:** labwc (MDE-Retro) vs sway (MDE) — standardize on one (which?) or stay compositor-agnostic via layer-shell only?
9. Daemon model: adopt `mackesd` as the single supervisor, import `mackesd_core` as a lib, or shell out to CLI subcommands?
10. Session bootstrap: keep MDE-Retro's labwc session, MDE's `mde-session`, or a new launcher?
11. D-Bus vs `mde-bus`: target the stable-but-deprecated D-Bus API or the future Bus?
12. One multiplexed binary for surfaces + separate supervised services for daemons?
13. Live theme switching via a `ThemeChanged` Bus signal, or keep "relaunch to switch"?
14. Settings as a registered-module registry (extensible) vs the current monolithic match?
15. Win10↔Workbench boundary: a hard rule, or case-by-case per feature?

### C · Platform substrate (mesh, storage, scale)
16. **Gluster mesh-home:** precondition, or optional (local-only state first-class)?
17. Nebula mesh: mandatory substrate or optional capability?
18. 8-peer cap: applies to the Workstation? target or hard limit?
19. QNM-Shared / coordination files: keep, or local state for single-box?
20. Fleet/playbooks (multi-machine): in scope for a "Workstation," or MDE-server territory?
21. Netdata/metrics: ship it? surface in the Win10 shell or only the Workbench?
22. Must every platform feature degrade gracefully with no mesh / no peers (standalone first-class)?

### D · Reuse mechanics
23. Two design languages coexist (Win10 shell + Material Workbench), confirmed?
24. Two widget kits (`mde-ui` flat-Carbon + `mde-iced-components`), or unify?
25. Per-crate: as-is / re-skinned / rebuilt — produce the table once 8/23 land.
26. Is `MDE-KDECnt-Rust`'s host the canonical KDE Connect, with MDE's `mde-kdc` converging onto it (one host, not two)?
27. Config: unify on MDE's TOML `mde-config` + `mackesd`, or keep MDE-Retro's `menu.json` + bridge?
28. Extend MDE-Retro's accuracy harness to cover the Workbench + platform surfaces?

### E · Win10-surface vs Workbench placement (per feature)
29. File manager: merge MDE's mesh-first `mde-files` into the Win10 Explorer, or keep it separate/Workbench?
30. Music (`mde-music`): Win10 app or Workbench?
31. Voice/VoIP (`mde-voice`, PJSIP softphone): Win10 "Phone" app, tray/flyout, or Workbench? Include in v1 at all?
32. Compute manager (`mde-virtual`, KVM+Podman): Workbench panel, standalone admin app, or out of scope?
33. Quick-actions drawer (`mde-drawer`): fold into the Win10 Action Center, or keep separate?
34. First-run: MDE's Birthright wizard, MDE-Retro's Win10 OOBE, or a merged flow?
35. Device surfaces: do "Your Phone"/Mobile Devices and the Workbench mesh views share one device model?
36. Maintain (snapshots/debloat/health/repair/drift): all Workbench, or do any surface in Win10 Settings (e.g. snapshots→System Restore)?
37. Network admin (VPN/firewall/remote-desktop/service-publishing): Workbench, or consumer bits in the Win10 flyout + admin in Workbench?
38. Presets/drift: Workbench-only, or a "restore my setup" in the Win10 shell?
39. Applets (17 `mde-applets`): map to Win10 tray/Action-Center tiles, or remain a separate applet system?

### F · The Workbench surface
40. Keep the ChromeOS-Classic/PatternFly identity (deliberate contrast), or re-skin to a Win10 "Computer Management" look?
41. Entry point from the Win10 shell: Start tile, a "Mackes Workbench" app, a keybind, or Settings "advanced"?
42. Nav model: keep the fixed 9-group/43-panel tree, or make it dynamic?
43. Scope: are the 11 known Workbench port-gaps v1 blockers or v1.1?

### G · Connectivity & phone
44. KDE Connect pairing store: one shared store across shell + `mackesd`, or per-binary?
45. Phone identity: are the Nebula cert (mesh CA) and KDE Connect fingerprint bound to one identity, or two?
46. Phone transport preference: KDC-TLS (battery) vs Nebula — what drives it, and is it user-visible?
47. Phone data (battery/ring/find): from KDE Connect plugins, Nebula probes, or both (authoritative source)?
48. **Finish the KDE Connect inbound listener (host 3b.2e) now, or is outbound-`open` enough for v1's phone flows?** *(KDE Connect crate work is parked on this.)*

### H · Migration, packaging, licensing
49. Packaging relative to MDE's `mde-core`/`mde-desktop` split — new flavor, drop-in replacement, or conflicting? (RPM **held until ready**.)
50. Licensing/IP: both GPL-3.0; the "Win10 look" raises trade-dress questions — guidance on resemblance, branding, attribution?

## 7. Load-bearing four (answering these unblocks ~80%)

**Q8** (compositor) · **Q16** (Gluster precondition) · **Q1** (repo shape) · **Q5** (MVP scope).

## 8. Next deliverable (pending answers)

Once the load-bearing four (ideally all 50) are answered, the executable plan:
the **epic breakdown**, the **per-crate reuse table** (all ~37 MDE crates + MDE-Retro,
classified as-is / adapt / rebuild), the new repo's **workspace layout**, and the
**milestone sequence** — still planning-only until "execute."
