# Plan: Capture the test workspace from a real system

**Status:** Proposal — decisions marked *confirmed* are settled with the author;
*default* are my assumptions until reviewed.
**Date:** 2026-08-30
**Context:** Supersedes the fixture-content half of
[`2026-08-29_review-followup-and-test-env-depth.md`](2026-08-29_review-followup-and-test-env-depth.md)
(Wave B4–B7) and returns to the intent of
[`2026-08-29_test-workspace-architecture.md`](2026-08-29_test-workspace-architecture.md).
The infrastructure waves of the former stand; only what fills the template
changes.

---

## Context

The committed template is currently **fabricated**. Its two connector tables
come from CSVs written by hand for the purpose: `issues.csv` (40 rows invented
to cluster into two themes) and `orders.csv` (810 generated rows with a planted
anomaly). `orders` exists only because the invented `issues` had no date column,
so one fabrication was stacked on another to give the insight analyses something
to find.

That defeats the point of the whole arrangement. A test workspace earns its
keep by answering *"will this work in production?"*, and it can only answer that
if its data has production's shape. Data shaped by whoever wrote the seed script
answers a different and useless question: whether the code works on the data the
test author imagined.

The gap is not theoretical. Audited against a real workspace, the current
template diverges in five specific ways:

1. `connector:sample` is a literal that joins to nothing. Production mints
   `connector:{config_id}` where that id references a `connector_configs` row.
2. `connector_configs`, `scheduler_jobs`, `sync_state` and `sync_runs` are all
   empty, so the entire connector/scheduler subsystem is unrepresented.
3. `issues` carries `id, title, body`. The real GitHub connector emits
   `number, state, created_at, closed_at, labels, user, html_url, …`.
4. `orders` has a column set no connector produces.
5. Connector tables carry zero `file_column_stats`; the events table (which came
   through the real ingest path) carries 27.

The web/events half *is* production-shaped, because it was produced by the real
ingress: `web:{uuid}` joined to a real `ingest.db` source, `events_{uuid}` via
the canonical helper, partitioned by date, with real column stats. That half is
the existence proof for what this plan does to the rest.

**What is kept.** Everything that does not depend on where the content came
from: the workspace-folder-as-unit-of-isolation design, the byte-copy at test
time, `brightflow-test-support` knowing no schema, the WAL checkpoint and
sidecar assertions, the relative-path fix, and the whole of the earlier plan's
Waves A and C. What changes is that the script stops *seeding* the template and
starts *scrubbing and verifying* one captured from a running system.

**Binding repo rules for every step:** co-located unit tests for touched pure
logic (rule 2); `//!` / `/** */` docs on new files, re-verified on touched ones,
stating contracts rather than history (rule 1); new prose only in the dated
`plans/` class (rule 3); strict clippy stays green; Rust work ends with
`cargo build --release`.

## Decisions reached

- **Confirmed — the template is captured, not generated.** Its contents come
  from a real Brightflow workspace that really synced a real connector. No mock
  upstream, no hand-written seed rows, no fabricated table shapes.
- **Confirmed — sensitive material is removed from the capture**, not avoided by
  fabricating around it.
- **Default — the script becomes a scrubber/verifier.** It takes a real
  workspace, applies reviewable scrub rules, verifies invariants, and publishes.
  It can no longer *reproduce* the template, so `--check` changes meaning (§B4).
- **Confirmed — the capture target is `dylanaraps/neofetch`** (§A2): archived,
  so the upstream cannot drift; 2,294 issues+PRs and 7,667 comments, which
  commits cleanly and syncs in ~115 requests.
- **Confirmed — the test tier never fetches.** Capture is a one-time human act;
  tests copy committed bytes. Wave C1 deletes the last thing that talked to a
  server during a build.
- **Default — capture a small repository rather than trimming a large one**
  (§A2). Trimming after the fact is where shape quietly leaks away.

---

## Wave A — prerequisites for a clean capture (2 commits)

### A1. A manual sync and a scheduled sync of the same connector run concurrently

`inflight_key` (`crates/brightflow-scheduler/src/lib.rs:418`) keys ad-hoc runs by
connector id and scheduled runs by job id. Different keys, so the in-flight set
never sees a conflict and both proceed. Observed live: a manual run
(`job_id NULL`) started 06:21:26 and the 6-hour scheduled job fired at 06:21:45,
both against `posthog/posthog`, both still running 1h46m later — doubling API
consumption and memory against a rate-limited upstream.

The comment above the function says this arrangement "keeps them from ever
colliding with a scheduled run of the same connector". The effect is the
opposite: it guarantees they can overlap. Correct the code, then correct the
comment to state what it actually guarantees.

- Key **by connector id** whenever a run targets a connector, regardless of
  whether a job triggered it, so one connector has at most one run in flight.
- Unit tests: an ad-hoc key and a scheduled key for the same connector must
  collide; two different connectors must not.
- This is a prerequisite because a capture taken while two syncs race is a
  capture of a broken state.

### A2. Capture target: `dylanaraps/neofetch` *(confirmed)*

`posthog/posthog` — the sync running when this plan was written — is 90,773
issues+PRs plus repo-wide issue comments: roughly 4,500–6,500 API requests per
sync against a 5,000/hour limit, accumulated entirely in memory before a single
write. Not a viable fixture source, and probably not a viable sync at all on
this connector's all-or-nothing execution model.

Candidates measured against the GitHub API:

| Repo | Archived | Span | Issues+PRs | Comments |
|---|---|---|---|---|
| **dylanaraps/neofetch** | **yes** | 2015-12 → 2024-07 | 2,294 | 7,667 |
| httpie/cli | no | 2012-02 → 2024-12 | 1,707 | 4,988 |
| sharkdp/bat | no | 2018 → 2026 | — | 10,204 |
| jgthms/bulma | no | 2016 → 2026 | — | 10,471 |
| junegunn/fzf | no | 2013 → 2026 | — | 15,648 |
| posthog/posthog | no | — | 90,773 | (vast) |

**Why `neofetch`.** It is **archived**, so the upstream is genuinely immutable —
no new issues, no new comments, no edits. MIT licensed, 8.6-year span, ~115 API
requests to sync in full, and verified diversity in a sampled page: state 43/57
closed/open, `is_pull_request` 49/51, `comments` 0–29 (median 1, mean 2.4),
`reactions_total` 0–21, 96 distinct authors per 100 rows.

**Why a frozen upstream matters even though the tier never fetches.** The
committed fixture is frozen by construction — it is bytes in git, and tests copy
them. The *source* being frozen is a second, independent guarantee, and it is
the one that pays off later:

- **Re-capture stays reviewable.** Re-capture will happen (a schema change, a new
  connector kind, a topics artifact-version bump). Against an archived repo the
  diff shows only the intended change; against a live repo it shows that change
  tangled with a year of upstream drift.
- **Specs can assert exact values** — row counts, a known title, a label
  distribution — and have them survive re-capture. A moving upstream forces
  permanently fuzzy assertions.
- **Provenance is verifiable.** Anyone can open the archived repo and confirm the
  fixture matches its source. Once an upstream has moved, nobody can check that
  a capture was faithful.
- **No accidental contamination** from a scheduler tick pulling new rows between
  the sync and the capture.

**The connector's output shape is why no supplement is needed.** `github.lua`
maps issues to 22 columns: measures `comments` and `reactions_total`; times
`created_at`, `updated_at`, `closed_at` (time-to-close is derivable); dimensions
`state`, `state_reason`, `is_pull_request`, `author_association`, `label_names`,
`user_login`, `assignee_logins`, `milestone_title`; text `title` and `body`.
That is a complete analytics table on its own — time, measures and categoricals
at several cardinalities. The fabricated `orders` table existed only because the
invented `issues` had none of this; against a real capture the need disappears.

**Known weaknesses of this choice**, recorded so they are not rediscovered as
surprises: label coverage is sparse on recent issues (0–13 per 100) and
concentrated on older triaged ones; `author_association` is heavily skewed to
`NONE`; and one sampled body ran to 46 KB, so the table has a long text tail.

**Fallbacks if `neofetch` proves unsuitable**, in preference order: a
date-bounded slice of a larger repo (pre-seed the `issues` cursor so `since`
limits the window — real incremental-sync behaviour, so the capture stays
honest); and, last, full-sync-then-trim, which is the worst option because
trimming Parquet rows desynchronises `tables.total_rows`,
`table_files.num_rows` and `file_column_stats`, and a recomputed catalog is a
reconstructed one — the failure mode this plan exists to end.

**Sync twice.** One sync produces one Parquet per table, so multi-file
`concat_df`, partition pruning and compaction stay unexercised (every table in
the current template is a single file). A second incremental sync after the
first gives at least one table two files and real cursor advancement in
`sync_state`, at no fidelity cost.

---

## Wave B — the capture, scrub and verify pipeline (5 commits)

### B1. `scripts/capture-test-template.sh`

Replaces `scripts/build-test-template.sh`. Takes a path to a real workspace
directory and produces the committed template.

```
capture-test-template.sh <workspace-dir>   capture, scrub, verify, publish
capture-test-template.sh --verify          verify the committed template only
```

Stages, in order:

1. **Copy** the source workspace to a scratch dir. Never operate in place: the
   source may be someone's live workspace, and a scrub bug must not touch it.
2. **Checkpoint** every `*.db` in the tree (`PRAGMA wal_checkpoint(TRUNCATE)`),
   then assert no `-wal`/`-shm` survives. Reused unchanged from the current
   script; a captured workspace needs this exactly as a built one did.
3. **Scrub** per §B2.
4. **Verify** per §B3. Verification failure aborts before publishing — a
   template that reaches `testdata/` must already have passed.
5. **Publish** to `testdata/workspaces/test/`, and re-add the `.gitkeep` files
   for the directories git cannot carry.

### B2. Scrub rules *(default — confirm each)*

The rules operate on the copy and are the reviewable artifact: a reader should
be able to audit what leaves the machine by reading this one file.

**Must remove — credentials.** Non-negotiable, and verified in §B3.

| Location | Rule |
|---|---|
| `scheduler.db` `connector_configs.token` | Set to `NULL`. **Currently holds a live 40-character GitHub PAT.** |
| `auth.db` `llm_providers.api_key` | Set to `NULL`; keep the rows so the provider-list shape survives. |
| `auth.db` `users` | Delete all, then create the demo account through `create-admin` (with `BRIGHTFLOW_ADMIN_PASSWORD`) so the harness's known login works and the hash is a real Argon2 hash of a published password. |
| `auth.db` `tower_sessions` | Delete all. Live session records for a real operator. |
| `ingest.db` `salts` | Delete all. These are the visitor-hash salts; with them, committed `visitor_id` values become correlatable. |

**Should decide — personal data.** These are judgement calls about third
parties, not secrets:

| Location | Options |
|---|---|
| Connector table `user` / login columns | Keep verbatim (public GitHub handles) **or** pseudonymize to same-charset, same-length values. |
| Connector table `body` / `title` | Keep verbatim (public issue text) **or** truncate. Keeping them is what makes the topics/text-explorer fixtures meaningful. |
| Connector table `body`, **specific to `neofetch`** | `neofetch` is a system-information tool, so its bug reports routinely paste its own output — which prints `user@hostname` along with OS, kernel and hardware detail. Public, but it is third-party machine and account detail arriving in bulk rather than incidentally. Decide deliberately: keep verbatim, or rewrite `\w+@\w+` host banners while leaving the rest of the body intact. |
| `html_url` / `avatar_url` | Keep verbatim (public) **or** rewrite the host. |
| `ingest.db` `user_profiles.traits` | Delete rows — arbitrary caller-supplied traits, unknowable content. |
| Events `hostname` / `page_url` | Rewrite the real site's hostname to `fixture.brightflow.local`, preserving path structure. |
| `litehouse.db` free-text tables (`action_log`, `agent_runs`, `cluster_edits`, `document_labels`, `insight_history`, `enrichment_cache`) | Keep — they are derived from the connector data, so they inherit whatever decision the columns above get. |

**Standing hazard: issue text is user-pasted.** Any repository's issue bodies
can contain a token, a key or a private URL that someone pasted by accident.
That is not a `neofetch` problem, it is a property of captured discussion text,
and it is why §B3's secret sweep runs over every text column rather than only
over the columns known to hold credentials.

**Must not touch.** Anything whose alteration changes shape: ids, foreign keys,
`source_id` values, table/partition names, timestamps, row counts, `total_rows`,
`file_column_stats`, `table_files.path`. If a scrub rule would desynchronise the
catalog from the Parquet, it is the wrong rule.

### B3. Verification invariants

Run at capture time and by `--verify` against the committed template. These are
the assertions that make the fixture trustworthy without re-reading 600 KB of
binary each time.

**Secrets** — the gate that matters most:
- Every `token` and `api_key` column is `NULL`.
- `tower_sessions` and `salts` are empty.
- `users` holds exactly the demo account.
- A regex sweep over every text column and every committed seed for
  `gh[pousr]_[A-Za-z0-9]{36,}`, `sk-[A-Za-z0-9]{20,}`, `-----BEGIN .* KEY-----`
  and bare `Authorization:` headers. Fails loudly on a hit.

**Shape** — the assertions that say this is production-shaped:
- Every `tables.source_id` matches `^(web|connector|upload):` and **resolves**:
  `connector:{id}` joins to `connector_configs`, `web:{id}` joins to `ingest.db`
  `sources`, and the events table is named `events_{id}`.
- `connector_configs`, `scheduler_jobs`, `sync_runs` and `sync_state` are all
  non-empty — the whole point of capturing rather than seeding.
- Every `table_files.path` is relative, so the catalog survives the copy.
- Every table has at least one `file_column_stats` row.
- At least one table has more than one file (§A2's second sync).
- No `-wal`/`-shm` anywhere.

**Size:** total template under 5 MB *(default — the earlier plan's 2 MB guard
was set for a fabricated fixture; real connector text is bulkier, and the
number should be set deliberately rather than inherited)*.

### B4. `--verify` replaces `--check`

The current `--check` rebuilds the template and diffs the result. A capture
cannot be rebuilt — that is the price of using real data, and it is worth
paying. `--verify` instead asserts the invariants in §B3 against what is
committed. It answers "is this template still sound and still secret-free?",
which is the question that actually matters; "would rebuilding produce this?"
was only ever a proxy for it.

Wire `--verify` into `scripts/check-conventions.sh` or the pre-commit hook
*(default — decide which)* so a template that acquires a secret cannot be
committed quietly.

### B5. Re-capture procedure

Document in the script header: capture is a deliberate act, not automation.
Re-capture when the fixture's baseline intentionally changes (a new connector
kind, a schema change that makes the old capture stale, a topics artifact
version bump). The steps are the same every time because they are the script.

---

## Wave C — retire the fabricated fixture (4 commits)

### C1. Delete the fabrications

- `testdata/seed/issues.csv`, `testdata/seed/orders.csv`, and the whole
  `testdata/seed/` directory.
- The seeding stages of `build-test-template.sh`: `store ingest` of the CSVs,
  the HTTP event posting, the enrichment-function creation, the insight runs,
  and the `topics fit` step. All of it is replaced by "the real system did this
  already, we captured the result".
- The build-time dependency on the 125 MB embedding model. A captured workspace
  brings its topic artifacts with it; nothing needs refitting at capture time.

### C2. Keep, deliberately

- **`store ingest` accepting CSV** (`crates/brightflow-cli/src/commands/store.rs`).
  Added to serve the fabricated fixture, but independently useful and symmetric
  with the `store export` that already writes CSV. Keeping it is a judgement
  call to make explicitly rather than by inertia. *(default — keep)*
- **`BRIGHTFLOW_ADMIN_PASSWORD`** — now load-bearing: the scrub step needs a
  non-interactive way to plant the demo account.
- **`ensure_dirs()` in the CLI and `test-server`**, the `relativize_path` fix,
  and the WAL/sidecar machinery. All apply unchanged, and the path fix matters
  *more* here: a captured workspace is copied by definition, and absolute paths
  would empty its tables on arrival.

### C3. Rewrite the integration specs against captured data

Every current spec hardcodes a fabricated identity: `connector:sample`,
`issues`, 40 rows, `orders`, `VAT`, two clusters.

- **Discover, don't hardcode.** The source id is a captured uuid, so specs
  resolve it from `/api/sources` or the table catalog — as
  `events.integration.test.ts` already does for the web source.
- **Assert on shape and invariants, not on magic numbers.** "The issues table
  has a `created_at` column and more than N rows" survives a re-capture;
  "`num_rows` is exactly 40" does not. A handful of exact assertions are still
  worth having where the value is the point (the demo user's email, the source
  kinds present).
- Insight-run assertions move onto the captured connector table, which now has a
  real date column — the reason `orders` existed disappears.
- The topics spec asserts against the captured artifacts; expected `k` and
  cluster count come from the capture rather than from a `--clusters 2` flag.

### C4. Update `CLAUDE.md`

Replace the `build-test-template.sh` entries in the Commands block with the
capture/verify commands, and state the testing-philosophy line this plan turns
on: the test workspace is captured from a running system, because a fixture can
only tell you production works if it has production's shape.

---

## Execution order & sizing

- **A1 first** — a capture taken while two syncs race is a capture of a broken
  state.
- **A2 next**, and it is the author's call: it decides what the fixture *is*.
- **B before C.** The captured template must exist and verify before the
  fabricated one is deleted, so the tier is never left with no fixture at all.
- **C3 is the long pole** — six spec files, all currently written against
  invented identities.

Roughly: Wave A ≈ 2 commits, Wave B ≈ 5, Wave C ≈ 4.

## Verification

Standard gates, all green today and required to stay green:

```
cargo fmt --check
cargo clippy --workspace --all-targets
cargo test --workspace
./scripts/audit.sh
cargo build --release
cd brightflow-app && npm run check && npm run test && npm run test:integration
```

Plus, specific to this plan:

- `scripts/capture-test-template.sh --verify` passes on the committed template.
- The secret sweep finds nothing, asserted by a test that plants a fake
  `ghp_`-shaped string in a scratch copy and confirms verification **fails** —
  a scanner nobody has seen fail is a scanner nobody should trust.
- The fresh-clone simulation still boots and serves:
  ```
  git archive HEAD testdata | tar -x -C "$tmp"
  BRIGHTFLOW_DATA_DIR="$tmp/testdata" BRIGHTFLOW_WORKSPACE=test \
    BRIGHTFLOW_TEST_PORT=0 ./target/debug/test-server
  ```
- A1's overlap fix is pinned by unit tests over `inflight_key`.

## Out of scope

- **CI** — still the deferred phase of the original test-workspace plan.
- **Playwright/DOM/browser tiers.**
- **The connector's all-or-nothing execution model.** `execute()` fetching every
  endpoint and page into memory before a single write is what makes a large
  repository unsyncable and hides all progress until the end. Real, and out of
  scope here; §A2 works around it by choosing a smaller target.
- **A supported "snapshot production" command.** Capture stays a deliberate,
  manual, reviewed act. The earlier plan's guardrail against a routine
  production-snapshot pipeline still holds.
- The store's `source:name` on-disk directory convention (colon, so no Windows
  checkout).
- **A Hacker News connector.** Researched alongside the capture target and worth
  building on its own merits: the Algolia HN API needs no authentication (so no
  credential would ever enter a capture) and returns `created_at`, `points`,
  `num_comments`, `author`, `title` and `url` — time plus two numeric measures
  plus two high-cardinality dimensions plus text, natively. It would fit beside
  the existing Bluesky connector as community monitoring, and once it existed a
  second capture would close the "only one connector kind" gap this fixture
  still has. Out of scope here because it is a product feature, not a fixture
  task, and `neofetch` needs no new code.
- **Stack Exchange as a source.** Ruled out rather than deferred: its content is
  CC BY-SA 4.0, and republication requires indicating the source, linking back
  to each original question, and naming every author — an obligation not worth
  taking on for a test fixture.

## Open questions

1. **Personal data** (§B2): verbatim public GitHub content, or pseudonymized
   logins — and the `neofetch`-specific question of whether pasted
   `user@hostname` banners in issue bodies are rewritten.
2. **Size ceiling** (§B3): is 5 MB the right number, given ~2,294 issues and
   ~7,667 comments of real text?
3. **Endpoint scope**: sync all five endpoints, or skip `issue_comments`?
   Comments are the bulk of both the request count and the fixture size, and the
   only fixture value they add beyond volume is a second table with a
   `story`-to-`parent` style relationship.
4. **`--verify` placement** (§B4): pre-commit hook, `check-conventions.sh`, or
   manual only?
5. **CSV ingest** (§C2): keep the CLI feature now that its original motivation
   is gone?
