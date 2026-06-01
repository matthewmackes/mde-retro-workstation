# MDE-Retro rebrand spec (captured from 20 Q&A, 2026-05-31)

**Delivery:** apply to this machine now via a `sudo install-branding.sh` **AND**
bake into the RPM **AND** run automatically as a step of `mde setup`. A full
backup + one-command `revert-branding.sh` (restores stock Fedora) is mandatory.

**Brand mapping:** *Everything is MDE Retro.* Product = "MDE Retro Workstation";
Fedora appears only as the small-print base ("Built on Fedora 44"). No Microsoft.

**Brand identity (one look everywhere):** the Start-menu **side-banner** style —
black + blue-glow background, "MDE Retro" white + "Workstation" light blue
(#3a6ad0 family). Used by the logo, boot splash, login, About, wallpaper.

**Logo mark:** design a new square MDE Retro mark (carbon layout-grid motif on
the black/blue brand background). Shared by About, Plymouth, login.

**Version:** lead with MDE version (e.g. "MDE Retro Workstation 1.0", from the
package version) + "Built on Fedora 44" small print.

## 1. About page
- **Both** a standalone `mde about` winver-style dialog (Start ▸ Help) **and** the
  Control Panel ▸ System Properties ▸ General surface.
- **Content (full winver):** logo + product + version + "Built on Fedora 44" +
  "Registered to: <current user's GECOS full name → username>" + live specs
  (CPU, RAM) + kernel.

## 2. Plymouth boot splash
- Win2000 look: **black** screen, centered MDE logo + wordmark, the classic
  **indeterminate sliding-blue-blocks** progress strip, "Built on Fedora 44" footer.
- Covers **boot + shutdown/reboot**; style the LUKS password prompt if encryption present.
- Needs root: install theme, `plymouth-set-default-theme -R` + `dracut -f`.

## 3. Login (LightDM)
- **Switch greetd → LightDM** with the **web greeter** (webkit2/web-greeter) for a
  near-pixel "Log On to Windows" dialog (centered grey box, user + password,
  OK/Cancel/Shutdown), over the **Win2000 desktop blue (#3a6ea5)**.
- NOTE: the web greeter may need a COPR/source build on Fedora 44 — flag at build.

## 4. OS rebrand (full cosmetic)
- `/etc/os-release` + `/usr/lib/os-release`: NAME/PRETTY_NAME/LOGO/HOME_URL → MDE
  Retro; **keep `ID=fedora` + VERSION** (so dnf/repos/tooling keep working).
- **GRUB:** full graphical GRUB theme matching the brand (bg + logo + menu).
- `/etc/issue` + MOTD console banner → MDE Retro.
- **fastfetch:** custom MDE ASCII logo + rebranded name/version.

## 5. Desktop
- Generate a **subtle branded wallpaper** (logo/wordmark, brand colors), set as
  default (still overridable in Display).
- **Startup sound:** keep the current Chicago95 login chime.

## Packaging
- All assets + scripts bundled in the RPM; `mde setup` runs the rebrand as an
  automatic step; licenses already covered (assets are MDE-original).
