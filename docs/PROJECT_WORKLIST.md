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

- [ ] **§2.1** Move the Carbon icon-accent table (`icons.rs:154-159`, 8 RGB
  literals) into `palette.rs` as named roles, reusing `carbon_accent()`/`URGENT`
  where they already match; pin in `checklist.rs`.
- [ ] **§2.3** Replace raw `.size()` literals with named `metrics` constants:
  `display.rs:756` (`48.0` identify overlay → `IDENTIFY_PX`), `installer.rs:196,240`
  (`10.0`/`15.0` → `metrics` constants); pin them.
- [ ] **§2.2** Pin `TASKBAR_BUTTON_MIN = 160` in `checklist.rs` (low priority).
- [ ] **§3 mockup** `display.rs` Effects tab — 3 enabled checkboxes whose state is
  never read/persisted: grey out (`cbox_disabled`) or persist via `state.rs`.
- [ ] **§3 mockup** `taskbar_properties.rs` "Show clock" + "Use Personalized Menus" —
  enabled but discarded: grey out (matches the file's own pattern) or wire.
- [ ] **§4 packaging** Add `assets/licenses/DroidSans-Apache-2.0.txt` to the asset
  list + a Droid Sans entry to `NOTICE.md` (the font is embedded in the shipped
  binary; IBM Plex — embedded the same way — is already covered).
- [ ] **§3 decision** `wlr.rs` `Wm::close` / `Wm::set_maximized` and `outputs.rs`
  `Output::make` are `#[allow(dead_code)]`: remove, or keep as deliberate
  protocol/EDID API with a comment justifying the retention.
- [ ] **§1 docs** Deeper prose pass: the reorg fixed each doc's headline claims +
  added status banners, but some body paragraphs in `PREVIEW.md`/`ACCURACY.md`
  still narrate the pre-cutover/Win2000-only world. Full rewrite when convenient.

## Backlog

- [ ] Carbon polish (from the theme survey): primary-blue / ghost button variants
  (current Carbon buttons are flat secondary); explicit accent-tinted labwc
  titlebar buttons; popup.rs context menus still bottom-anchored under the top bar.
