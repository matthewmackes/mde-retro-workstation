---
name: ship
description: >-
  Autonomously drain the MDE-Retro worklist: a rescue pass to catch dead/mock
  code, then implement open tasks fully (no stubs), building + accuracy-verifying
  each, committing as you go. TRIGGER when the user says "ship it", "execute",
  "continue", "drain the worklist", or "work through the backlog" for this Rust
  shell. Do NOT use for a single scoped edit (just do it) or anything needing a
  release cut (use /release).
---

# ship — autonomous worklist drain (MDE-Retro)

Implements `docs/PROJECT_WORKLIST.md` to empty, under the standing autonomy in
`.claude/CLAUDE.md` §6. Heads-down: the commit body is the record, one short note
per phase boundary, no marketing copy.

## Phase 0 — Rescue pass (always first)

Before new work, catch the project's recurring failure mode (shipped-but-dead /
mockup-only code). This is the single highest-value step.

1. **Dead-module grep** (`rust/`): for each `pub mod`/`mod`, confirm an external
   `<mod>::` reference exists. A module with helpers + tests but no caller is **not
   done** — it's unreachable. List offenders.
2. **Stub/mock grep:** `rg 'todo!\(|unimplemented!\(|panic!\("not |coming soon|placeholder|demo_data'`
   across `rust/mde*/src`. Each hit is either real work or a mislabelled task.
3. **Reachability:** every feature must be reachable from an `mde <subcommand>`
   path and *do something* when launched (`timeout 3 ./target/debug/mde <sub>`).
4. **Re-cue misleading `[✓]`:** any worklist item marked done but failing 1–3
   flips back to `[>]` with a one-line note. If ≥3 rescues, write a short audit note.

## Phase 1–N — Drain loop

For each open `[ ]` task, highest priority first:

1. Mark `[>]` in the worklist (restart-safe claim).
2. Implement **fully** per CLAUDE.md §3 — no stubs, runtime-reachable, no raw hex
   outside `palette.rs` (§2.1), metrics single-source (§2.3).
3. **Gate before commit** (auto-fix in scope; SOFT-ESCAPE if the same fix fails 3×):
   - `cargo build` (or `cargo build --release` for packaging tasks)
   - `cargo test` (and `cargo test -p mde-ui` for palette/metric changes)
   - `cargo clippy --all-targets` · `cargo fmt --all`
   - **Visual tasks:** `./preview.sh gallery` (or `verify`) and confirm the render
     in the captured PNG — a green `cargo test` alone does NOT verify rendering
     (the dynamic harness silently skips headless).
4. Commit named pathspecs with a why-not-what message + the `Co-Authored-By` trailer.
   Flip the task `[✓]`. **Do not push** (§0.1) — that stays gated.
5. Run independent tasks in parallel where they don't touch the same files.

## Stop conditions

Worklist empty (only gated items remain) · a push/release/cutover moment · a
destructive op · a product-direction change · two consecutive unexplained gate
failures · ≥10 rescues at once. On stop: a short factual summary + what's left.

## NOT this skill

Single obvious edit → just do it. Release cut → `/release`. Deep integrity sweep
with a written report → `/audit`.
