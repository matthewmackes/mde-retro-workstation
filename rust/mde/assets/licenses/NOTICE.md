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

If in doubt, install code-only and run `mde install --assets`, which fetches
these from upstream at first run instead of redistributing them.
