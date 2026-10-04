# Plan: unit-only testing — remove the integration setup and adopt the nine working rules

**Status:** Implemented, uncommitted (author verifies, commits) — decisions marked *confirmed* are settled with the author
on this date; *default* are my assumptions until reviewed.
**Date:** 2026-10-04
**Baseline:** HEAD `24bc595`.
**Delivery:** no commits or PR by the agent; the author commits and pushes after
verifying the result (*confirmed*).

## Why

The integration tier (Rust tests over a copied, committed test workspace; a
frontend tier booting a live backend) makes the suite *feel* like it proves the
assembled system works. It does not, and the agent cannot do real E2E. The
author does the final E2E. The suite's job becomes: types that make invalid
states unrepresentable, parsers at thin boundaries, a pure core — each with
small, isolated unit tests (*confirmed*).

A unit test is **isolated**: it builds plain values, calls a function, checks
the result. No files, no temp dirs, no network, no processes, no shared fixture
world, no mocks. In-memory SQLite is allowed — it runs inside the test and needs
nothing built (*confirmed*). If a test needs setup, the code has the wrong shape:
move the logic out of the I/O code instead of building a harness.

The agent never runs the app: it runs all verification, both release builds,
then hands over (*confirmed*).

## Steps

### 1. Remove the integration setup

Rescue (already compliant — move into `src/` as `#[cfg(test)]` modules):
- `crates/brightflow-engine/tests/insights_pipeline.rs`, `drivers_report.rs` —
  in-memory DataFrames.
- `crates/brightflow-types/tests/ossie_conformance.rs` — parser test over an
  `include_str!` fixture.

Delete:
- `crates/brightflow-api/tests/*` (9 files), `crates/brightflow-store/tests/enrichment_functions.rs`,
  `crates/brightflow-llm/tests/mock_server.rs` (local HTTP server).
- `crates/brightflow-test-support` and its dev-dependency edges.
- `testdata/workspaces/test`, `testdata/seed` (only the template script reads it).
  `testdata/eval/` stays — `enrich eval` reads it.
- `scripts/build-test-template.sh`, `scripts/test-env.sh`.
- Frontend: the six `*.integration.test.ts`, `src/testing/{withBackend,globalSetup,cookieFetch}.ts`
  (+ `cookieFetch.test.ts`), the `integration` Vitest project, `test:integration`.
- The CLAUDE.md sections (root and `brightflow-app/`) describing all of the above.

Old `plans/` that reference the setup stay: they are dated.

### 2. Rules into CLAUDE.md

Merge the nine rules into Conventions, keeping the existing comment and
prose-file rules (own-file comments, no documented absences, the
normative/dated/generated test). Rules 4–6 apply to new and changed code, not as
a tree-wide audit. Replace the handler carve-out and the "run backend and
frontend when debugging" workflow line.

### 3. Unit tests without temp dirs

Tests in `src/` that create a `TempDir`: SQL logic moves to in-memory SQLite;
logic tangled with file I/O is extracted into pure functions where cheap;
tests of file I/O itself are deleted. Drop the `tempfile` dev-dependency where
it becomes unused.

### 4. Mechanical rule-6 fixes

- `let _ = write!(…)` without a reason → `push_fmt` or a reason comment.
- `push_fmt`: drop the `unreachable!`; discard the infallible `String` write
  explicitly with the reason (rule 6 names this case) (*default*).
- jemalloc decay tuning: move from the `unsafe` `mallctl` writes to the
  build-time `JEMALLOC_SYS_WITH_MALLOC_CONF` in `.cargo/config.toml`, removing
  the `unsafe` block and the `tikv-jemalloc-ctl` dependency (*default*).
- `panic` lint `warn` → `deny`.

### 5. Rules 4 and 5 on existing code

Applied as code is touched, not as a full rewrite (*default*). One targeted
item: the auth-exempt routes as a plain list the router is built from, unit
tested — replacing what `router_contract.rs` covered.

## Verification gate (every step)

`cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace`, `npm run check`, `npm run test`,
`cargo build --release`, `npm run build`. Then hand over.

## E2E checklist for the author

What the deleted tests guarded and no unit test now covers:

- **Auth wiring:** a protected `/api` route answers 401 without a session; the
  public ones (login, event collection, `/script.js`, `/health`) answer
  without one; `..` in a route param is rejected. (The public *list* is now
  unit-tested in `routes.rs`; that the layers are mounted is not.)
- **Action bus + undo:** define, column semantics, table settings, reset,
  vocabulary hierarchy, saved views (save, overwrite, rename, delete) — each
  round-trips through undo.
- **Agent runs:** a `describe_table` run sees column profiles; proposals land
  at the agent layer, held until approved, applied at once when auto-applying;
  approve/reject as one batch.
- **Enrichment:** two functions materialising into one table at the same time
  both land; ticket classify → six columns; extract → `{table}_mentions` child
  table, flags, unresolved queue, and a re-enriched ticket with fewer mentions
  leaves no stale rows.
- **Insights:** a post-sync auto-run records a run but no history; a manual run
  records both.
- **Models:** a build materialises the chain and carries input semantics; a
  model over a model rebuilds after its feeder; an empty result keeps the table.
- **Semantic model import/export:** import applies per table, refuses a
  malformed model, dry run writes nothing; export rebuilds the document.
- **Store file I/O:** merge/ingest write Parquet under `{source}/{table}/`;
  `write_table` replaces and unlinks the old file and keeps a zero-row table;
  `replace_table_data` consolidates multi-file tables.
- **Pool:** connections run in WAL; `transaction` takes the write lock up front
  (`BEGIN IMMEDIATE`).
- **LLM transport:** bearer auth is sent; a real 429/5xx is retried.
- **Frontend data layer:** login + session cookie, sources/catalog, events and
  web-analytics reads, insights run log, enrichment function CRUD, Text Explorer
  search.
