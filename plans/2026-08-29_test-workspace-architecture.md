# Implementation Plan: Public Test Workspace (Committed Template + Fresh-Copy Test Runs)

**Date:** 2026-08-29
**Status:** Proposal — decisions below flagged as *confirmed* are settled with the
author; *default* are my assumptions until reviewed.
**Context:** Continues the assessment in
[`../reports/2026-08-26_folder-per-tenant-and-test-environments.md`](../reports/2026-08-26_folder-per-tenant-and-test-environments.md),
narrowed through discussion into a concrete, minimal design.

---

## 1. The converged design

Brightflow's data layer already treats **a workspace folder as the unit of
isolation** (`{data_dir}/workspaces/{name}/`, everything self-migrating on
open). We make that the unit of test too, with a **single committed template
(folder) as the public test set**, and give it two kinds of runtime copies:

| Copy | Lifecycle | Used by |
|---|---|---|
| **Ephemeral** | Fresh copy of template → throwaway `TempDir`, per test-suite run, discarded after | Automated test suite (local now; CI later — same mechanism) |
| **Persistent** | Long-lived copy at `data/workspaces/test/`, reset only deliberately | Interactive env: `run-all`, frontend dev, bug repro, demo |

Both copies derive from the **same committed template**, so there is one source
of truth, and no per-version ceremony. Schema changes ride on the existing
self-migration-on-open; template *content* changes happen inline with the PR
that needs them (see §5).

Local and CI use the identical mechanism because the whole test env lives in
the repo: a run just copies the template and runs the suite against the copy.

## 2. Decisions reached (summary)

- **Confirmed — no generator.** The template is a static committed folder; a run
  does a filesystem `cp`, not a schema-aware rebuild. No per-run generation.
- **Confirmed — no per-boot refresh of the interactive env.** The ephemeral copy
  (per suite run) is where freshness lives; the persistent env is wiped only by a
  deliberate `reset` (re-copy). Auto-refresh on boot would destroy working state
  and re-introduce the per-run-rebuild habit we ruled out.
- **Confirmed — two lifecycles, one template.** Ephemeral for the suite,
  persistent for interactive work.
- **Confirmed — local and CI share the mechanism.** No CI exists in the repo
  today (per the 2026-08-02 test plan) ; the design is CI-ready on day one by
  construction.
- **Confirmed — repo size.** OK to add size, but not "gigantic" → **small curated
  fixture set**, no Git LFS, no synthetic-at-scale.
- **Confirmed — schema is the code, not the template.** Migrations remain the
  source of truth; the template is fixture *data*. The committed `.db` files are
  fixture state, never edited by hand in schema terms.

## 3. Design — concrete mechanics

### 3.1 Template location and git hygiene (*default*)

- Committed tree: `testdata/workspaces/test/` at repo root. Chosen deliberately
  over `data/` because `.gitignore` already ignores `/data/` (both the repo-root
  one and target workspaces live under it at runtime). `testdata/` is a new,
  non-ignored root.
- **Sidecar-free rule:** only checkpointed `.db` files are committed, and never
  the transient sidecars. Add gitignore guards so a stray `-wal`/`-shm` can never
  be staged:
  ```gitignore
  # WAL/shm sidecars are transient — a committed DB must be cleanly checkpointed
  **/*.db-wal
  **/*.db-shm
  **/*.wal
  **/*.shm
  ```
  (These live in `testdata/` and would otherwise be visible; the guard makes the
  contract mechanical.)
- **Runtime copies live under gitignored `/data/`:** ephemeral in a `TempDir`
  under the OS temp dir; persistent at `data/workspaces/test/`.
- **Why committing `.db` + Parquet is acceptable here:** the template is a small,
  static, cleanly-checkpointed data snapshot (not a hot DB). The versioned schema
  lives in migrations; self-migration brings the folder forward on open. This is
  the data analogue of the repo's "generated, not edited" rule — the template is
  *derived* (built by a defined procedure, §4), committed as an artifact, and any
  later Schema change is handled by existing migration machinery, so it does not
  hand-drift the way a hand-edited doc would.

### 3.2 Ephemeral copy — the automated suite (*default*)

A shared Rust test-helper copies the committed template into a `tempfile::TempDir`
once per test-suite run:

- **Location:** a small crate or module reachable from the integration tests of
  `brightflow-api` and `brightflow-store`. Options:
  - a `test-support` helper crate (dev-dependency), or
  - a `#[cfg(test)]` helper re-exported from `brightflow-core`
    (core already owns `WorkspacePaths`).
- **Behavior:** resolve the template path (env override `BRIGHTFLOW_TESTDATA_DIR`,
  default repo-root `testdata/workspaces/test`), `cp -r` into a fresh
  `tempfile::TempDir`, and hand back the dir. The suite then boots the app/store
  against that dir exactly as it boots production (`WorkspacePaths` + self-migrate
  on open).
- **This is a copy, not generation.** No schema knowledge in the helper; it copies
  bytes. Identical mechanism for local and CI.
- **Unit tests that need no data layer are untouched** — the template copy only
  replaces the storage-backed *integration* paths.

### 3.3 Persistent interactive env (*default*)

- Runtime dir: `data/workspaces/test/`, reached by `BRIGHTFLOW_WORKSPACE=test`.
- A tiny script `scripts/test-env.sh` (matching the repo's `scripts/` convention):
  - `test-env.sh setup` → copy template → `data/workspaces/test/`
  - `test-env.sh reset` → rm the runtime copy, re-copy from template
  - `test-env.sh status` → show where template and runtime live / whether they
    differ in schema version
- The frontend needs no changes: it targets the API at `VITE_API_BASE`
  (`localhost:8080`); the env switch is backend-side only.

## 4. Building the template (how the committed folder is *produced*)

This is a defined, repeatable **build procedure** (one-time / on-fixture-change),
not an ongoing generator:

1. Boot a throwaway workspace (`BRIGHTFLOW_DATA_DIR=<tmp>` with a fresh name) via
   `run-all`; empty schema self-creates and self-migrates on open (*confirmed*:
   every DB calls `migrate` at pool creation).
2. Seed the fixture *through the real API/connector paths* so the template
   exercises the genuine read/sync path: create users (existing admin auto-seed +
   `create-admin`), register a demo connector pointing at a **local/mock
   endpoint** (never a real production source), ingest a small sample event set,
   run a connector sync, fit a small topics model, write schemas.
3. **Clean shutdown** (`run-all` stop). Per SQLite WAL docs, the last connection
   close performs a final checkpoint and deletes the `-wal`/`-shm` sidecars —
   leaving a checkpointed, sidecar-free, copy-safe folder. This is the step that
   makes `cp -r` safe.
4. `cp` the cleaned folder → `testdata/workspaces/test/`, sanity-check no sidecars
   staged, and commit it *with the code/PR that defines the fixtures it contains*.

**Fixture-scope guard (*default*):** small curated set — a few sources, one demo
connector (mock endpoint), handful of Parquet files, a couple of users. Sized to
sit comfortably in git with no LFS.

## 5. Keeping it from going stale — the ordinary dev flow

- **New schema:** pull new code, boot against an *existing* runtime copy → it
  self-migrates on open. No template step. (Reset-to-template first only if a
  migration is destructive and a clean baseline is preferred.)
- **New test data/fixtures:** edit/commit the template in the same PR as the code
  that needs them — ordinary test maintenance, same as any setup. Rebuild the
  committed template via §4 when the baseline intentionally changes.
- **No template release pipeline** (no "rebuild on every migration"). That is
  exactly the ceremony we rejected.

## 6. Guardrails / deliberate non-goals

1. **Keep the pure-unit inner loop cheap.** Pure unit tests that don't touch the
   data layer never copy the template. Only storage-backed integration tests do.
   (Self-migrations + small template keep even the copy runs cheap.)
2. **Do not build the routing layer.** The test env is a process-level env choice
   (`BRIGHTFLOW_WORKSPACE`), resolved once `from_env()`. No per-request
   tenant→folder mapping. That stays deferred until the product serves multiple
   tenants.
3. **Never point test fixtures at real production sources.** "Production-like"
   means shape, not destinations; connector configs in the template target local
   mocks/consumer endpoints.
4. **Staging is out of scope here.** A real production snapshot (with real/PII
   data, WAL-safe copy of a *running* prod) is the operator's ad-hoc, private,
   gitignored concern — deliberately *not* a supported command, and not the public
   test set.
5. **Inodes/FDs:** fine for a handful of env copies; revisit only if the count
   grows by orders of magnitude (report §5).

## 7. Proposed phases

- **Phase 1 — infrastructure:** build the template via §4; add the Rust copy-helper
  (§3.2); add `scripts/test-env.sh` (§3.3); add the `.gitignore` sidecar guards.
- **Phase 2 — migrate existing storage-backed integration tests** (`api/tests/*`
  and `store/tests/*` currently open empty workspaces in `TempDir`:
  `router_contract.rs`, `action_log.rs`, `insight_runs.rs`,
  `enrichment_functions.rs`) to boot from the template copy, gaining real-data
  coverage of the genuine Parquet/SQLite read path.
- **Phase 3 (optional, separate plan):** a CI workflow (none exists in the repo)
  that checks out, copies the template, and runs the suite — by construction uses
  the exact same mechanism as local.

## 8. To confirm before implementation

- **Default:** template at `testdata/workspaces/test/` and helper-crate vs
  `core`-re-export for the test helper (I lean a small dev-only helper crate to
  keep core zero-dep).
- **Default:** `scripts/test-env.sh` for the persistent env; bare `cp` also
  acceptable if you'd rather not add a script.
- **Scope:** is Phase 2 (migrating existing integration tests to the template)
  part of this plan, or a follow-on? The infra in Phase 1 stands alone either way.
