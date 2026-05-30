# System Properties + Device Manager spec

Replicates the Windows XP **System Properties** dialog (ref:
sfu.ca/.../rdc_fig1.jpg), rendered in the MDE-Retro Win2000 classic theme. All
7 tabs, each wired to its Fedora equivalent. Opened from the Control Panel
"System" item (and Win+Pause).

## Tabs (XP layout, two rows)
General · Computer Name · Hardware · Advanced · System Restore · Automatic
Updates · Remote

| Tab | Wiring (decided) |
|-----|------------------|
| **General** | Live system info: `/etc/os-release`, kernel, `lscpu` CPU, `/proc/meminfo` RAM, "Registered to" = user. |
| **Computer Name** | Hostname + description; **Change…** sets hostname via `hostnamectl` (pkexec); a Samba **Workgroup** field. |
| **Hardware** | **Device Manager = native iced** hardware tree (decided 2026-05-30). A **[Device Manager]** button opens an mde view that reads `/proc`, `/sys`, `lscpu`, `lsblk`, `lspci`/`lsusb` into an MS-style category tree. NOT GTK HardInfo2 — see the decision note below. |
| **Advanced** | Full set: **Environment Variables** (user + system), **Performance** (zram/swappiness), **Startup & Recovery** (default boot entry + GRUB timeout). |
| **System Restore** | → **Timeshift** (enable/create snapshots).  *[confirm in Q5]* |
| **Automatic Updates** | → **dnf-automatic** (enable timer, configure).  *[confirm in Q6]* |
| **Remote** | **Remote Desktop** = wayvnc (allow users to connect); **Remote Assistance** = one-time invite.  *[details in Q7–10]* |

## Device Manager (= native iced — DECIDED 2026-05-30)

**Decision:** reimplement the hardware view in **pure iced**, honoring locked
decision #4 (no GTK). The vendored GTK **HardInfo2 is dropped** — do NOT build
or ship it; the `vendor/hardinfo2` submodule can be removed (`.gitmodules`).
Rejected alternatives: shipping HardInfo2 as a themed GTK subprocess (breaks
no-GTK purity); dropping the component for CLI tools (least Win2000-faithful).

- **Data source:** Rust reads `/proc/cpuinfo`, `/proc/meminfo`, `/sys/class/*`,
  and shells out to `lscpu`/`lsblk`/`lspci`/`lsusb` (already-required CLI tools),
  parsed into a category tree. A toolkit-agnostic `sysinfo`-style data layer
  feeds both the Hardware tab's summary and the Device Manager tree.
- **UI:** an mde-ui tree widget (the same one the Explorer tree pane needs —
  build once, reuse) showing MS-style category names; later refinements: the
  four view modes and status icons (no-driver = ⚠, disabled = ↓), Show hidden.
- **Scope guard:** match the Win2000 Device Manager look against a reference,
  not general hardware-tool feature expansion.

## Remote Desktop (Wayland)
- Backend: **wayvnc** (wlroots/sway VNC). Install via pkexec dnf if missing.
- "Allow users to connect remotely" toggles the wayvnc service; show the
  connection address (host:5900). *[security/users details pending Q7–10]*

## Still to confirm (System Properties Q5–10)
System Restore backend, Automatic Updates backend, Remote Desktop vs RDP,
Select Remote Users semantics, Remote Assistance, and security (password/TLS,
localhost+SSH vs LAN, port). Answer while building those tabs.
