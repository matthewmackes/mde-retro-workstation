---
name: audit
description: >-
  Integrity sweep of the MDE-Retro shell: find dead/unreachable code, stubs,
  mockups passing as features, convention violations (raw hex, scattered metrics),
  and stale docs — each finding gets a FINISH-or-REMOVE verdict. TRIGGER when the
  user asks to "audit", "evaluate compliance", "check for dead code/stubs", or
  "find what's not really done" in the Rust shell. Produces a findings table /
  report; it does NOT fix things unless asked.
---

# audit — compliance & integrity sweep (MDE-Retro)

Catches the gap between "marked done" and "actually reachable + correct", and
checks compliance with `.claude/CLAUDE.md`. Output is a findings **table**
(`Location | Category | Evidence | Confidence | Verdict`) plus a short summary;
verdict is binary **FINISH** (wire it up / make it real) or **REMOVE** (delete the
dead surface). Don't fix unless asked — report first.

## Passes (run in parallel where possible)

1. **Unreachable code** — `pub mod`/`mod` with no external `<mod>::` ref; `pub fn`
   never called; dead `match` arms; a feature with no `mde <subcommand>` path to it.
2. **Stubs** — `todo!()`, `unimplemented!()`, `panic!("not …")`, stub arms,
   `pub mod foo;` with zero refs, "wiring in a follow-up" commit bodies.
3. **Mockups** — `demo_data`/placeholder constants, "coming soon"/"placeholder"
   strings, tabs/panels that render but do nothing.
4. **Convention violations** (CLAUDE.md §2):
   - raw hex/RGB literal anywhere except `palette.rs`
     (`rg -n '#[0-9a-fA-F]{6}|from_rgb8?\(' rust/mde/src rust/mde-ui/src` minus palette.rs);
   - `.size(` with a literal instead of `metrics::UI_PX`;
   - a palette/metric value changed without a matching `checklist.rs` assertion;
   - mde drawing client-side title bars (compositor boundary, §1).
5. **Doc drift** — prose claiming facts the code contradicts (e.g. "sway"/"Win2000
   default" vs labwc + Carbon-dark default). Each stale claim is a FINISH (fix the doc).
6. **Packaging reachability** — symbols/assets the RPM `%files`/`assets` list ships
   but nothing uses, or shell subcommands with no `mde-<cmd>` symlink in `%post`.

## Safeguards (avoid false positives)

Framework lifecycle callbacks (`iced` `update`/`view`/`subscription`, `Default`,
`Drop`, serde derives), `#[test]`/`#[cfg(test)]` helpers, and declaratively-wired
handlers are **reachable** even with no direct textual caller — don't flag them.
Confirm a "dead" symbol with `rg` across the whole workspace before the verdict.

## Output

A markdown findings table + counts by category, written to
`docs/COMPLIANCE.md` (or returned inline for a quick check). Lift every FINISH into
`docs/PROJECT_WORKLIST.md` so the sweep produces actionable work, not just a report.
