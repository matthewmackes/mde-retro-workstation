---
name: release
description: >-
  Cut an MDE-Retro RPM release: pre-flight gates, version bump, asset staging,
  cargo-generate-rpm build, then commit/tag. TRIGGER ONLY when the operator
  explicitly types "cut release X.Y.Z" / "build the RPM" / "release it" for this
  project. NEVER auto-trigger from a /ship run — releasing is always operator-gated.
---

# release — RPM cut (MDE-Retro)

Operator-triggered only. Pushing tags and publishing are outward-facing (CLAUDE.md
§0.6) — confirm before anything leaves the machine.

## Pre-flight gates (all must hold)

1. Clean git tree on `main`; nothing un-committed that belongs in the cut.
2. `docs/PROJECT_WORKLIST.md` has no open `[ ]`/`[>]` blocking the release scope
   (or the operator explicitly scoped a partial "cut for testing").
3. `cargo build --release` clean; `cargo test` green; `./preview.sh verify`
   passes (real render check, not the silent-skip path).
4. `cargo clippy --all-targets` and `cargo fmt --all --check` clean.

## Steps

1. **Bump version** in `rust/mde/Cargo.toml` (`[package] version` and the
   `[package.metadata.generate-rpm] release` — bump `release` on packaging/asset-only
   changes, `version` on shell changes). Keep the workspace crates consistent.
2. **Update** `CHANGELOG`/release notes if present.
3. **Stage assets:** `cd rust && tests/stage-rpm-assets.sh` (stages the bundled
   Win2k/Haiku icons + Chicago95 cursors/sounds + Plex fonts into `target/rpm-assets/`,
   which the RPM `assets` list references). The 76MB Chicago95 fallback is **not**
   bundled — `mde install --assets` fetches it at first run (locked decision #7:
   ship code-only, redistribute no third-party asset bytes beyond the primary set).
   Verify `assets/licenses/NOTICE.md` covers anything bundled before a public RPM.
4. **Build:** `cargo generate-rpm -p mde` → `target/generate-rpm/mde-*.rpm`.
   Never raw `rpmbuild --short-circuit`.
5. **Smoke test:** install in a throwaway env or at least confirm `%post` would
   symlink every `mde-<subcommand>`; branding is applied out-of-band by the
   `mde-activate-branding.service` one-shot on next boot (NOT inline in the txn).
6. **Commit** the version bump (named pathspecs, `Co-Authored-By` trailer).
   **Tag + push only after explicit operator go-ahead** (§0.1/§0.6).

## Failure modes

generate-rpm missing → `cargo install cargo-generate-rpm`. Asset list points at
absent `target/rpm-assets/**` → re-run `stage-rpm-assets.sh`. Branding must never
run inside the rpm transaction (dnf lock / no network) — it's the posttrans one-shot.
