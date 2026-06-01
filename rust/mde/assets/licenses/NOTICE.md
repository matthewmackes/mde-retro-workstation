# Bundled third-party assets — provenance & licenses

MDE-Retro's own code is MIT (see ../../../LICENSE / README). The desktop also
bundles classic-look assets from third parties, under their own terms:

## Chicago95  (cursors, sounds, and the Chicago95 icon fallback)
- Upstream: https://github.com/grassmunk/Chicago95
- License: **GPL-3.0** — full text in `GPL-3.0.txt` (this directory). Source is
  available at the upstream URL above, satisfying the GPL-3 source requirement.

## Win2k icon theme  (the primary shell icons)
- Source: KDE-Store item 1120706 (a 2001 KDE2 icon set), bridged to freedesktop
  naming by MDE-Retro's installer.
- License: the KDE-Store item's terms. **VERIFY these before redistributing the
  bundled RPM publicly** — MDE-Retro does not assert a redistribution right over
  this third-party art.

## Haiku icon theme  (alternate icon set — Display ▸ Appearance)
- Upstream: https://github.com/lxmx/haiku-icons (based on phillbush/haiku-icons
  and the Haiku OS originals).
- License: **MIT/X** — full text in `haiku-icons-MIT.txt` (this directory).
  Freely redistributable; bundled in the RPM.

## IBM Plex Sans  (the Carbon/BeOS-theme UI font, embedded in the binary)
- Upstream: https://github.com/IBM/plex
- License: **SIL Open Font License 1.1** — full text in `IBMPlexSans-OFL.txt`.
  Freely redistributable; the Regular + Bold faces are compiled into the mde
  binary (used when the Carbon or BeOS theme is active).

## Droid Sans  (the Windows 2000-theme UI font, embedded in the binary)
- Upstream: the Android Open Source Project (Droid Sans, Ascender Corp.).
- License: **Apache License 2.0** — full text in `DroidSans-Apache-2.0.txt`.
  Freely redistributable; the Regular + Bold faces are compiled into the mde
  binary as the Tahoma stand-in for the Windows 2000 Classic look.

If in doubt, install code-only and run `mde install --assets`, which fetches
these from upstream at first run instead of redistributing them.
