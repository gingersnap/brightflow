# Plan: Post-migration review follow-up — correctness fixes and a reproducible, deep test workspace

**Status:** Proposal — decisions marked *confirmed* are settled with the author;
*default* are my assumptions until reviewed.
**Date:** 2026-08-29
**Context:** Follows a review of the ten commits `46fe4a2..2a35031` (the
conventions sweep, the sqlx→rusqlite migration, and the frontend↔backend
integration tier). Continues
[`2026-08-29_test-workspace-architecture.md`](2026-08-29_test-workspace-architecture.md)
and
[`2026-08-29_frontend-backend-integration-tests.md`](2026-08-29_frontend-backend-integration-tests.md),
neither of which is revised — this plan implements the one part of the former
that was specified as prose (§4) and deepens the fixture the latter is
bottlenecked on.

---

## Context

The reviewed range is in good shape: the workspace builds clean, `cargo clippy
--workspace --all-targets` is quiet, 800+ Rust tests and both frontend tiers
pass, `cargo fmt --check` and `npm run check` are green, and `sqlx` is gone from
`Cargo.lock` including its mysql/postgres ghost crates. The staged migration
(infrastructure → per-crate conversion → removal) left every intermediate commit
buildable, and mechanical-conversion hazards were checked and found absent: no
SQL-placeholder/`params!` arity mismatches anywhere in the tree, no `execute()`
on a `RETURNING` statement, no bare nullable aggregate mapped to a non-`Option`
scalar.

What this plan addresses is what the review found *behind* that green wall:

1. **Two proven engine bugs** in chart-payload code, one of which the range's own
   `downsample` fix made reachable in a wider range of inputs.
2. **A test workspace whose provenance is prose.** The committed template
   (`testdata/workspaces/test/`, 364 KB of binary `.db` plus one Parquet file) is
   produced by the manual procedure in §4 of the test-workspace plan. Nothing
   executable rebuilds it, nobody but its author can extend it, and it already
   silently captured author-machine state that git cannot carry.
3. **A fixture too thin for the tier built on it.** One 5-row Parquet table, one
   user, one catalog source, and nothing else — so two of the four integration
   specs assert "a typed empty list, not a 401 or 500". That is a real assertion,
   but the tier cannot grow further against this fixture without mostly proving
   that empty is empty.

The architecture underneath is not in question and is not revisited. "A workspace
folder is the unit of isolation, so make it the unit of test" is the right
decision, and it pays off exactly where fixtures usually rot:
`brightflow-test-support` copies bytes and knows nothing about schema, and the
committed DBs carry complete `schema_migrations` rows (6 / 19 / 6, matching the
migration directories), so new migrations apply forward onto the template with no
template step at all. The confirmed decisions of the earlier plan — no
generation at test time, no per-boot refresh, two lifecycles from one template,
small curated fixtures with no LFS, schema-is-the-code — all stand.

**Binding repo rules for every step below:** co-located unit tests for touched
pure logic (rule 2); `//!` / `/** */` docs written for new files and re-verified
on touched ones, stating contracts rather than cross-file observations (rule 1);
new prose only in the dated `plans/` class (rule 3); strict clippy stays green;
Rust work ends with `cargo build --release`.

## Decisions reached

- **Confirmed — scope.** All review findings are in, with the test-workspace
  hardening as one wave among several.
- **Confirmed — executable builder.** §4 of the test-workspace plan becomes
  `scripts/build-test-template.sh`. This does **not** reopen the rejected
  "generator": what was rejected is *generation at test time*, which puts schema
  knowledge into test code. A reproducible build of a committed artifact, run on
  demand through the real API paths, is a different thing — the runtime still
  does a plain `cp`, and `brightflow-test-support` still knows nothing.
- **Confirmed — binary `.db` files stay.** With a builder in place, the script is
  the reviewable artifact and the database is its output. No data-only SQL dumps.
- **Confirmed — fixture depth across all four domains:** ingest source + events,
  insight runs + novelty history, enrichment functions + versions, and topics ML
  artifacts (the last under an explicit size guard, §B7).
- **Confirmed — no CI phase.** No CI exists in the repo; it stays the deferred
  Phase 3 of the test-workspace plan. The mechanism remains CI-ready by
  construction.

---

## Wave A — correctness fixes (6 commits, independent of B and C)

### A1. The trend fit line does not track the points it is drawn over

`crates/brightflow-engine/src/analysis/engine/payloads.rs:55`
(`build_trend_data`). The series is downsampled to at most `MAX_POINTS`, then the
fit is recomputed over downsampled indices `0..n` using the **per-original-row**
`slope`. With a stride of `s`, downsampled index `i` holds the value from
original index `i * s`, so the fit under-slopes by exactly `s`.

Proven with a temporary probe (reverted) on a perfectly linear 399-point series,
slope 1.0:

```
stride=2 len=200 point[10]=20 fit[10]=109.5     // fit[10] should be 20.0
```

This is pre-existing, but `46fe4a2`'s ceiling-division fix widened its range: with
flooring, stride was 1 for `200 < n < 400` and that band was accidentally correct;
now it downsamples and the fit is wrong there too.

- `downsample` (`:20`) returns the stride it used: `(Vec<String>, Vec<f64>,
  usize)`.
- `build_trend_data` fits with `slope * stride as f64`; `build_anomaly_series`
  discards the stride (its bands are constants, unaffected).
- Test: extend `trend_fit_matches_series_length_and_slope` with an `n = 399`
  case asserting `fit[i] == vals[i]` for a linear input. It fails before the fix.
- Comment: the module `//!` claim that overlays "always have the same length as
  the points actually drawn" is true and stays true; add that the fit is
  expressed in downsampled-index space, which is the fact that was missing.

### A2. `build_scatter_data` panics on empty input

Same file, `:275`. `take` is `xs.len().min(ys.len()).min(MAX_POINTS)`, and
`stride` is `(xs.len() / take).max(1)` — integer division by zero when both
slices are empty. Proven:

```
panicked at payloads.rs:275: attempt to divide by zero
```

- Early-return an empty `NodeData::Scatter` with `fit_slope: None` /
  `fit_intercept: None` when either slice is empty, mirroring the `n == 0` guard
  `build_trend_data` already has.
- Test: `scatter_empty_input_is_safe`, alongside the existing
  `trend_data_empty_input_is_safe`.
- While here: the file has two stride rules (`downsample` ceils; scatter floors
  and truncates with `.take(take)`). Scatter's is *not* buggy — it refits OLS
  from the sampled pairs, so it stays self-consistent. Leave the shapes separate
  and record why in a comment rather than forcing a shared helper.

### A3. `SqlitePool::transaction` should use `Immediate` behavior

`crates/brightflow-store/src/pool.rs:121` opens a deferred `BEGIN`
(`conn.transaction()`), documented as matching sqlx's `pool.begin()`.
`update_enrichment_function_config` (`crates/brightflow-store/src/db/enrichment.rs:200`)
is a genuine read-then-write transaction: it selects `current_version + 1`, then
inserts and updates. Under WAL, a concurrent commit landing between the read and
the write makes the write fail with `SQLITE_BUSY_SNAPSHOT` — which SQLite does
**not** route through the busy handler, so the 5-second `busy_timeout` never
applies and the error surfaces to the caller.

- Switch to `conn.transaction_with_behavior(TransactionBehavior::Immediate)`.
  Every current call site either writes first or is read-then-write, so nothing
  loses anything and the read-then-write case starts serializing on the write
  lock (where `busy_timeout` *does* apply).
- Update the doc comment: this is now a deliberate divergence from sqlx, not
  parity. Say which, and why.
- The in-code comment at `enrichment.rs:210` explains the collision the
  transaction prevents; make it say *how* it now prevents it (serialized write
  lock) rather than leaving the mechanism implied.
- Test in `pool.rs`: two tasks each running a read-then-write transaction against
  the same row on the same pool; both must succeed and produce distinct versions.

### A4. The migrator lost sqlx's content validation

`crates/brightflow-store/src/migrate.rs` catches version gaps, reorders,
renames, and a database ahead of the binary — but not an **edit to the SQL of an
already-applied migration**, which sqlx caught by checksum and which is the
failure that actually happens in practice.

- `run_migrations` computes `blake3::hash(m.sql.as_bytes())` and stores the hex
  in a `checksum` column of `schema_migrations`. `blake3` is already a direct
  dependency of `brightflow-api` and `brightflow-engine`, so adding it to
  `brightflow-store` introduces no new workspace dependency.
- `schema_migrations` is owned by this module, so evolve it in place:
  `ALTER TABLE schema_migrations ADD COLUMN checksum TEXT` when the column is
  absent. Rows with a NULL checksum are grandfathered and backfilled on the next
  run, so existing databases and the committed template keep working unchanged;
  B1's rebuild bakes real values in.
- Mismatch is `MigrateError::Invalid` with the version, name, and both hashes —
  and the same actionable shape as the `_sqlx_migrations` message added in
  `4d82305`, which is the model to follow here.
- Tests: an edited applied migration is rejected; a NULL checksum is backfilled
  rather than rejected; a fresh apply writes checksums.

### A5. The cookie-fetch wrapper is never restored and nests on repeat

`brightflow-app/src/testing/withBackend.ts` discards the `restore()` returned by
`installCookieFetch()` (`cookieFetch.ts`). Each spec's `beforeAll` therefore wraps
the *previous* wrapper, and since every layer calls `headers.set('Cookie', …)`,
the innermost (first-installed) jar wins. Vitest's per-file isolation hides this
today; it is a trap for the fifth spec.

- Make `installCookieFetch` idempotent: tag the wrapper it installs and, when the
  current `globalThis.fetch` already carries the tag, return the existing jar's
  restore instead of wrapping again. Tagging makes the property true regardless
  of how many specs call it, which is stronger than asking callers to call once.
- Add `resetCookieJar()` so a spec can drop a session deliberately.
- Test in the **unit** tier (`cookieFetch.test.ts`, no backend needed): install
  twice, assert `globalThis.fetch` is wrapped exactly once and that one
  `Set-Cookie` round-trips.

### A6. `EventBuffer::insert` clones the whole event per request

`crates/brightflow-api/src/ingest/buffer.rs:103` does `let row = event.clone()`
so the pooled closure can be `'static` — roughly 27 `String` allocations on the
hot ingest path, which the sqlx version did not pay (it bound by reference).

- Take the event with `std::mem::take(event)` (requires `Event: Default`; derive
  it if absent), move it into the closure, and return it: `Ok((session_id, row))`.
  `insert` writes the row back and sets `session_id`. This removes the clone while
  keeping the single pooled round-trip the comment at `:99` justifies.
- **Measure first.** This is a micro-optimisation on a path with no benchmark. If
  it does not show against B4's seeded event set, record the decision in the
  comment and stop — do not churn the code for a number nobody has.

---

## Wave B — the test workspace: reproducible template, deep fixtures, widened tier (8 commits)

### B1. `scripts/build-test-template.sh` — §4 made executable

The single structural fix. Today §4 is four numbered manual steps; the artifact
they produce is a 364 KB binary blob that cannot be reviewed, cannot be rebuilt by
anyone else, and captures whatever happened to be on the author's disk.

- Boots a throwaway workspace (`mktemp -d`, `BRIGHTFLOW_DATA_DIR`,
  `BRIGHTFLOW_WORKSPACE=test`) so every schema self-creates and self-migrates on
  open, exactly as §4 step 1 specifies.
- Seeds fixtures **through the real API/connector paths** (§4 step 2) — the
  reason the template is worth having at all. Connector configs point at a local
  file/mock endpoint, never a real source (guardrail §6.3 of the original plan).
- Clean shutdown, then assert sidecar-free before copying (§4 step 3) — the
  invariant that makes `cp -R` safe, already asserted at *copy* time in
  `test-env.sh`; now asserted at *build* time too.
- `--check` mode: rebuild into a temp dir and diff **logical** content — a
  `sqlite3 .dump` of each database minus volatile columns (`applied_at`,
  `created_at`/`updated_at`, generated ids), plus Parquet row counts — against
  the committed template. Byte-equality is not achievable and not the goal;
  detecting that the committed template no longer matches what the procedure
  produces is.
- Header comment states the contract: this script is the authority on how the
  template is produced; §4 of the dated plan is the record of the decision, not
  the procedure to follow.

### B2. Materialise the directories git cannot carry

`events/`, `events-buffer/`, and `connector-configs/` are empty directories, so
they do not survive a clone. Verified: `git archive HEAD testdata | tar -x`
yields only `store/connector:sample/issues/`, and
`sqlite3 …/events-buffer/x.db` fails with `unable to open database file`. Nothing
exercises them today, which is exactly why this is worth fixing before B4 does.

- Add `.gitkeep` to each. `copy_template` copies bytes, so they ride along with no
  change to `brightflow-test-support`.
- **And** call `paths.ensure_dirs()?` in
  `crates/brightflow-api/src/bin/test-server.rs` right after
  `WorkspacePaths::from_env()`. Production does this at `api/src/lib.rs:128`; the
  test server currently does not, and a harness that diverges from production
  startup is the harness lying about what it proves.
- Regression check (see Verification): the fresh-clone simulation must boot and
  accept an event.

### B3. Commit `ingest.db` into the template

`auth.db`, `litehouse.db`, and `scheduler.db` are committed; `ingest.db` is not,
and nothing in the test-workspace plan excludes it. The consequence is that the
ingest `sources` table is empty on every run, so `POST /api/event` answers
`Unknown domain` and the entire ingestion path is unreachable. Add it to the
builder's copy set.

### B4. Fixture: ingest source + events *(confirmed)*

The highest-value single addition, and the domain the rusqlite conversion touched
hardest (`ingest/db.rs`, `buffer.rs`, `flush.rs`, `derive_session_id`) with no
HTTP-level coverage at all.

- Seed one ingest source on a domain **distinct** from the `integration.test`
  domain that `client.integration.test.ts` creates, so the create-path spec keeps
  getting a fresh id.
- Event set spanning at least two sessions, two visitors, one `user_id`
  continuity case across a visitor rotation, and two days — flushed to Parquet so
  both the buffer and the flushed store path carry data.
- Unlocks: `POST /api/event` end to end, `derive_session_id` over real rows, the
  `FlushTask` chunked delete introduced in `6fce264`, and the
  `web_analytics/queries` surface that gained ~170 lines and unit tests in
  `46fe4a2` but no coverage through the wire.
- Seed a non-pageview event deliberately, so the finding `46fe4a2` raised and
  chose not to fix — `query_stats` counts all events as pageviews while the bounce
  math filters to real pageviews — becomes *observable* by a future spec. Do not
  fix it here; it stays a raised finding.

### B5. Fixture: insight runs + novelty history *(confirmed)*

- A few `insight_runs` rows (at least one completed, one failed) and
  `insight_history` novelty rows for `connector:sample/issues`, so
  `insights.integration.test.ts` asserts content instead of `[]`.
- Seed one run in the `Interrupted by server restart` state — the other finding
  `46fe4a2` raised and left alone (the matcher has no writer anywhere). Seeding it
  makes the gap visible in the fixture rather than only in a commit message.

### B6. Fixture: enrichment functions + versions *(confirmed)*

- One enrichment function with two versions and a promoted current version, plus
  an `enrichment_cache` row.
- Unlocks, at HTTP level, the read-then-write transaction fixed in A3 and the
  `is_unique_violation()` → conflict mapping introduced in `de4789c` (which today
  is only covered by a store-level unit test).

### B7. Fixture: topics ML artifacts *(confirmed, under a size guard)*

`topicsApi.overview` reads precomputed embeddings and labels the template does
not ship, which is why `2a35031` could only reach Text Explorer — the one part of
the topics module needing no trained artifacts.

- Seed embeddings, cluster labels, and taxonomy rows by running the real
  `topics fit` path over the 5-row table, and commit **only** the resulting
  database rows and Parquet — never a model file.
- **Size guard:** the confirmed constraint from the original plan is "small
  curated, no LFS". If the template crosses roughly 2 MB, stop and return to the
  size decision rather than pushing through. This is the fixture most likely to
  need rebuilding when schemas move, which is precisely what B1 makes affordable.
- Sequence this last inside the wave: it is the one step to abandon if the guard
  trips, and abandoning it must not block B4–B6.

### B8. Widen the tier onto the deeper fixture

- New `*.integration.test.ts` specs: `events` (ingest round-trip plus a
  web-analytics read), `enrichment` (list / create / promote, and duplicate → the
  conflict response), `topics` (overview against the real artifacts).
- Replace the empty-list assertions in `insights.integration.test.ts` with
  content assertions now that B5 supplies rows. Keep one deliberate
  empty-response case somewhere — "the silence is the contract" is a real
  assertion worth keeping, just not the whole tier.
- Backend side: extend `crates/brightflow-api/tests/router_contract.rs` with the
  ingest routes, now that a source exists to address them.
- Keep `fileParallelism: false` and one server per run; every spec continues to
  open its own session through `useIntegrationBackend()`.

---

## Wave C — hygiene, supply chain, doc truth (5 commits)

### C1. `scripts/test-env.sh` — `setup` silently destroys the persistent env

`setup` and `reset` both dispatch to `copy_template`, which begins
`rm -rf "$RUNTIME"` — while the file's own header states "The persistent env is
wiped only deliberately, via `reset` — never on boot." Running `setup` twice loses
interactive working state.

- `setup` refuses when `$RUNTIME` already exists, naming `reset` as the
  destructive path; `reset` keeps today's behavior.
- `schema_version` can return `"?"`, which then reaches the `-ne` arithmetic
  comparison in `status`; guard both sides, not just `tv`.
- Add a `command -v sqlite3` guard so a missing tool reports itself instead of
  silently reporting `"?"`.

### C2. `test-server.rs` — unreachable `sweeper.abort()`

It sits after `axum::serve(...).await`, which does not return in practice. Drop
it: the process is short-lived and the harness kills it. (Adding a
`tokio::select!` shutdown path to make the line true is the alternative, but it
buys the harness nothing.)

### C3. Supply chain: `./scripts/audit.sh` is red

RUSTSEC-2026-0258 — `h2` 0.4.15, fixed in ≥ 0.4.16 — reached via `reqwest` and
`axum`. **Pre-existing, not from this range:** `h2` was already 0.4.15 at
`HEAD~10`; the advisory published 2026-08-17. It still fails the repo's own gate.

- Try `cargo update -p h2` first; if a direct `axum`/`reqwest` bump is needed, do
  it in its own commit.
- Only if it proves genuinely unreachable, add it to `.cargo/audit.toml` with a
  reason **and** a recheck trigger, per the repo's existing rule.

### C4. Windows checkout *(default — confirm)*

`testdata/workspaces/test/store/connector:sample/` contains a colon, so the repo
no longer clones on Windows. Two options: (a) accept it, and say so in the
builder script's header; (b) have the builder write source directories with a
filesystem-safe encoding. Recommend **(a)** — the colon comes from the store's
own runtime directory naming, so (b) is really a question about changing *that*
convention, which is out of scope here. `buffer.rs`'s `delete_source` comment
already treats Windows as a best-effort dev platform, so this is consistent.

### C5. Truth-up the record

- `46fe4a2`'s message lists "ua.rs: … wide mobile-OS devices stay Tablet (iPad
  landscape)" under bug fixes. The removed `w <= 991` arm was dead — the arm below
  it already returned `Tablet` — so behavior did not change. The comment the
  commit added to the code is accurate; only the changelog line overstates. The
  commit is pushed and is not being amended: this note is the correction, so the
  line is not later cited as evidence of a behavior change.
- Several new headers are written in changelog voice
  (`auth/session_store.rs`'s "replacing `tower-sessions-sqlx-store`",
  `migrate.rs`'s "the same per-file semantics sqlx had"). Rule 1 asks for the
  contract the file offers, not its history. Low priority — trim on next touch of
  those files, **not** as a tree-wide sweep.

---

## Execution order & sizing

- **Wave A first.** It is independent of B and C, and A1/A2 are the only findings
  a user can see. A4 lands before B1 so the rebuilt template carries checksums
  from the start.
- **B1–B3 gate B4–B8.** Deepening fixtures before the builder exists just makes a
  bigger unreproducible blob.
- **B7 last within B**, behind its size guard.
- **Wave C anytime.** C3 is what makes `./scripts/audit.sh` green again.

Roughly: Wave A ≈ 6 commits, Wave B ≈ 8, Wave C ≈ 5.

## Verification

Standard repo gates, all of which are green today and must stay green:

```
cargo fmt --check
cargo clippy --workspace --all-targets
cargo test --workspace
./scripts/audit.sh                       # currently red — C3
cargo build --release
cd brightflow-app && npm run check && npm run test && npm run test:integration
```

Plus, specific to this plan:

- `scripts/build-test-template.sh --check` reports no logical drift against the
  committed template.
- **Fresh-clone simulation** — the regression test for B2, and the check that
  would have caught it:

  ```
  git archive HEAD testdata | tar -x -C "$tmp"
  BRIGHTFLOW_DATA_DIR="$tmp/testdata" BRIGHTFLOW_WORKSPACE=test \
    BRIGHTFLOW_TEST_PORT=0 ./target/debug/test-server
  # must print TEST_SERVER_LISTENING and accept POST /api/event
  ```

- A1 and A2 each fail their new test before the fix and pass after.

## Out of scope

- **CI** *(confirmed)* — stays Phase 3 of the test-workspace plan.
- Playwright, DOM, browser-mode, or any mounted-component tier.
- The per-request tenant→folder routing layer (deferred by the original plan).
- Production/staging snapshots — the operator's private, gitignored concern.
- Data-only SQL dumps of the fixture *(decided against; the builder is the
  reviewable artifact)*.
- The store's on-disk `source:name` directory convention (see C4).
- `query_stats`' pageview/bounce inconsistency and `add_tokens`' completion-only
  token splits — raised in `46fe4a2`, still deliberately unfixed. B4 makes the
  first observable; neither is fixed here.
