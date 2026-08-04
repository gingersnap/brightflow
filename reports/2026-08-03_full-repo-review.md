# Full-repo review: identity, philosophy adherence, and simplification

**Date:** 2026-08-03
**Scope:** the entire repository — all 8 Rust crates (~49,400 lines of Rust), the Vue app (~20,000 hand-written lines), docs/plans/reports, scripts, and repo configuration.
**Method:** four parallel read-only research passes (backend architecture, backend conventions + simplification, frontend, meta-layer/philosophy), each returning file:line evidence; the highest-impact claims were then spot-verified directly against source. No commands beyond `ls`/`grep`/`wc`-class measurement were run; the test/lint/build suites were reported passing by the maintainer and were not re-run. Every claim below carries a file reference; where a claim is a judgment rather than a measurement, it is phrased as one.

---

## 1. What Brightflow is

Brightflow is a self-hosted analytics platform built around a specific thesis, stated in
`docs/human_ai_interaction.md`: **pair analysis, not automated analysis**. A human and an
AI co-edit an interpretation layer (labels, clusters, taxonomies, insights) on top of
immutable data, through one shared action bus, with undo preferred over approval gates.
The deterministic engine computes statistics; the LLM narrates and curates but "is never
the source of a statistical claim."

Concretely it is:

- **A Rust backend** of 8 crates (~47,200 lines under `crates/*/src`), with a clean
  dependency fan: `core` (paths only, zero deps) at the bottom; `store`, `engine`, `llm`,
  `connect` as independent leaves; `scheduler` and `api` composing them; `cli` on top.
  The HTTP surface is one Axum router with 85 routes (`crates/brightflow-api/src/routes.rs`).
- **"Litehouse" storage**: SQLite catalogs over immutable, UUIDv7-named Parquet files
  (`crates/brightflow-store`). Files are registered after the bytes land; reads
  concatenate registered files; cache invalidation rides a `tables.version` counter so an
  out-of-process CLI ingest invalidates API caches for free.
- **A Polars lazy query engine** with store-side file pruning: `store/src/scan.rs` builds
  SQL against per-file partition/column stats to choose which Parquet files even enter
  the scan, before Polars predicate pushdown takes over.
- **An analysis/NLP engine** (`crates/brightflow-engine`, 20,477 lines): significance
  tests, trend/driver/anomaly detection, embeddings (model2vec), HDBSCAN/k-means topic
  clustering, c-TF-IDF naming, a trained intent-classifier head (linfa-logistic).
- **LLM enrichment + a curation agent** (`crates/brightflow-llm` + `api/agent`,
  `api/enrichment`): batch prompt-derived columns cached by input hash, and a
  tool-calling agent whose every tool call goes through the same `dispatch_action` as a
  human click, recorded as a proposal.
- **A Vue 3 frontend** (Nuxt UI 4, Tailwind 4, Pinia + Pinia Colada, vue-echarts):
  WebSocket-first querying, a query builder as primary UX, command palette, and
  ts-rs-generated types (131 files) keeping the wire contract in sync with Rust.

Development is solo-plus-agent, trunk-based on Codeberg, no CI, no review gate other
than a pre-commit hook and the author. The history is unusually disciplined:
conventional commits with essay-length bodies, a visible research → dated plan →
execution → verification loop, and a deletion-dominant recent history (~25 of the last
50 commits delete or collapse code).

---

## 2. Does it fulfill its philosophy?

The repo's `CLAUDE.md` states three rules: (1) inline comments are the primary,
maintained documentation; (2) unit tests per touched piece; (3) prose files must be
unable to go stale (normative / dated / generated). The one-line verdict — which the
repo's own earlier audit also reached — still holds: **the conventions hold exactly as
far as mechanical enforcement reaches, and no further.**

### Rule 1 — inline comments: presence perfect, truth mostly good, with countable lapses

**Presence is at 100%**, measured tree-wide: 177/177 Rust files carry a `//!` module doc
in the first 3 lines, 46/46 hand-written `.ts` and 90/90 `.vue` files carry a `/** */`
header. The backlog burn-down (commits `678b9d8`, `e614787`, `036e27a`) is complete.

**Quality is genuinely high at the top.** Docs that state a load-bearing *why* are the
norm, e.g. `api/src/ingest/buffer.rs:1-9` (rejected alternatives and their failure
modes), `engine/src/stats/significance.rs:1-12` (error *policy* with rationale),
`api/src/ingest/geo.rs:7-8` (`open_readfile` over `open_mmap` because the mmap path
carries RUSTSEC-2025-0132), `api/src/routes.rs:1-10` (the router is the API doc because
a hand-written one drifted to 10/95 accurate routes).

**The tail restates *what*.** ~17 Rust files carry one-line filename restatements
(`store/src/error.rs:1` "Error types for the store crate";
`engine/src/nlp/error.rs:1`). On the frontend, `services/api.ts:1` — the 525-line API
client — has the weakest header in the tree (`/** REST API client */`, zero rationale).

**The truth half — which nothing mechanical can check — has live violations:**

| Location | The lie |
|---|---|
| `brightflow-app/src/stores/pivot.ts:107-116` | "Trigger reactivity" comment justifying a `new Set(...)` clone; Vue 3.5 refs deep-track `Set.add/delete`, so the stated reason is false and the clone is waste. |
| `brightflow-app/src/router.ts:1-4` | Header says every view is lazily imported so the shell stays small — but `SourceLayout.vue:14-27` statically imports all 14 tool components, pulling ECharts, the pivot grid, and all 12 insight renderers into one chunk. |
| `brightflow-app/src/services/wsGuards.ts:1` | "One copy so every store agrees" — while `isActionBatch` lives in `stores/curation.ts:22` and `isInsightsComputed` in `stores/insightsActivity.ts:20`. |
| `brightflow-app/src/stores/index.ts:21-23` | Refers to an "Auth store" that no longer exists (auth moved to `composables/useAuth.ts`). |
| `crates/brightflow-api/src/scheduler/handlers.rs:79,105,122,141,180` | Comments document routes as `/api/connectors[/:id]`; `routes.rs:243-253` mounts them at `/api/connector-configs[/{id}]`. |
| `crates/brightflow-cli/src/main.rs:3-6` | Header says "thin dispatch... logic that could be reused belongs in a crate" over a 1,617-line file containing full analysis routines (`topics_eval`, `main.rs:1410-1523`). |

None of these is catastrophic; all are exactly the drift class the rule predicts, and
they cluster in files not recently touched — evidence that "re-verify comments on what
you touch" works where it fires and has no reach where it doesn't.

### Rule 2 — unit tests per touched piece: the weakest rule, by a wide margin

- **Rust:** 74/177 files have `#[cfg(test)]` (426 unit tests). After honoring every
  carve-out, **54 pure-logic files have no coverage**. The standout:
  `engine/src/stats/significance.rs` — 11 pure numeric functions (p-values, Pearson
  correlation, linear regression, autocorrelation, prediction intervals) with **zero
  tests**, in the crate whose whole philosophical job is being the deterministic ground
  truth the LLM is not. Also untested: `analysis/tree.rs` (1,253 lines, pure),
  `analysis/engine/payloads.rs` (`downsample()`), `data/merge.rs` (schema
  override-merge), `product_analytics/queries.rs` period-key arithmetic.
- **The Axum carve-out is claimed but not paid.** The rule exempts handlers because
  "integration tests in `crates/*/tests/` cover them" — `brightflow-api` has 17
  `handlers.rs` files (~5,000 lines) behind 85 routes, and exactly **2 integration
  tests** (241 lines) covering two flows. Auth, analytics query, CSV upload, sources,
  connect, scheduler, agent, and enrichment handlers have no coverage of any kind.
  `brightflow-cli` (1,617 lines), `brightflow-connect`, `brightflow-scheduler/src/db.rs`
  (400 lines of SQL): zero tests.
- **Frontend:** 6 test files (791 lines) against 46 modules. The riskiest untested pure
  code: `components/sources/csvPreview.ts` (a hand-written quote-aware CSV state
  machine), `composables/useTextExplore.ts:22` (`parseTermInput` regex tokenizer),
  `components/enrich/promptTokens.ts` (documented as mirroring Rust
  `extract_column_refs` — a cross-language contract with no test pinning it).
- **Nothing enforces this rule.** The hook runs all tests but never correlates diff with
  coverage. It is the only one of the three rules with no mechanical backing at all.

Where integration tests *do* exist, they are excellent — they pin claims, not
mechanics (`api/tests/action_log.rs`: recluster writes an action-log row *even when the
refit fails*; `engine/tests/drivers_report.rs`: planted-delta attribution and sign
cancellation).

### Rule 3 — the staleness test: holds everywhere the checker looks, fails exactly where it doesn't

- `docs/` is clean: all three files are genuinely normative.
- `plans/` and `reports/` are frozen 10/11. The one violation: commit `211e85e` flipped
  `plans/2026-08-03_simplification-and-philosophy-closure.md` from `Status: in progress`
  to `implemented` — six hours *after* the audit that certified 9/9 files never-edited.
  The `**Status:**` front-matter field structurally invites exactly the post-hoc edit
  the rule forbids; nothing reconciles this tension.
- **The three files the checker exempts by name are the three that violate the rule.**
  `README.md` is demonstrably stale right now on two verified claims: line 9 calls
  `brightflow-core` "Shared types and errors" (it contains only `WorkspacePaths`;
  `BrightflowError` was deleted in `863bf39` — the README went stale one day after its
  last refresh), and lines 16/30-31 document a `cargo run -- schedule` subcommand that
  does not exist (no `Schedule` variant in the CLI's `Commands` enum; verified). Notably,
  the repo's own 2026-08-03 audit graded README.md "currently accurate" — a hand-written
  state doc fooled the audit *of* hand-written state docs, which is the rule's thesis
  demonstrated on the rule's own paperwork.
- `brightflow-app/CLAUDE.md` is a state doc by category but the best-maintained one,
  and contains the repo's best artifact: the `.vue type-checking` section with an
  explicit **removal trigger** instructing deletion of the section itself.

### Enforcement posture

Everything mechanical lives in one local, opt-in, `--no-verify`-bypassable pre-commit
hook. **There is no CI** — a documented deliberate choice (Codeberg, hook strengthened
instead), but the consequence deserves stating plainly: on any machine where
`install-hooks.sh` was never run, enforcement is zero. Two smaller contradictions
between config and stated convention:

- `CLAUDE.md` prescribes `#[expect(clippy::..., reason)]` for test modules (because
  `expect` warns when stale); actual usage is **20 `#[allow]` / 0 `#[expect]`** — and
  most of those allows are no-ops anyway, since `clippy.toml`'s `allow-*-in-tests` keys
  already cover unwrap/expect/panic/indexing in tests.
- The workspace `Cargo.toml` configures ~40 clippy lints carefully; then four of five
  binary/lib crates blanket-allow the code-quality half at the crate root
  (`api/src/lib.rs:19-25`, `engine/src/lib.rs:21-30`, `store/src/lib.rs:11-23`,
  `cli/src/main.rs:17-23`). `indexing_slicing` being off crate-wide is what lets two
  panic-capable byte-slices of date strings through
  (`api/src/product_analytics/queries.rs:421`, `api/src/analytics/events_scan.rs:67-68`
  — these panic on non-ASCII input at byte 10).

---

## 3. Pros

1. **The comment culture is real, not aspirational.** 100% presence, and the best
   headers in the repo (`buffer.rs`, `significance.rs`, `geo.rs`,
   `services/websocket.ts`) are better documentation than most projects' wikis. The
   scripts document their own engineering decisions (`check-conventions.sh:26-30`
   explains the deliberate absence of `pipefail`, with the SIGPIPE bug that motivated it).
2. **Epistemic honesty as a system property.** The pre-commit audit line prints "not
   fetched — run ./scripts/audit.sh for a real audit" instead of a false green.
   `.cargo/audit.toml` gives every ignore a reason *and a recheck trigger*, plus a
   documented decision against a severity threshold with the empirical case that would
   have burned them. Verification commits report their own error rates ("verify the
   backfilled .vue headers, fix 16").
3. **Production Rust genuinely cannot unwrap.** `unwrap_used`/`expect_used`/`todo`/
   `print_*` all deny; only 18 `expect`s survive outside test code, each with a local
   allow and rationale (mostly `LazyLock` regex literals).
4. **Several architectural decisions are quietly excellent:** catalog-over-immutable-
   files with version-ride cache invalidation; store-side file pruning before Polars;
   the engine being SQLite-free by construction; one `Action` enum feeding
   serde/ts-rs/schemars so human clicks, agent tool calls, and the wire format cannot
   diverge; `AppError::parts()` making status code and error code inseparable, with a
   test pinning the 4xx/5xx classes; CORS that hard-fails in production rather than
   defaulting open (`api/src/lib.rs:366-382`).
5. **The frontend contract rules hold where checkable:** "stores never import
   `services/api`" — 0 hits; all server data through Pinia Colada; generated types +
   barrel regenerated from the hook.
6. **Deletion-dominant history.** The codebase is actively shrinking where it should
   (~990-line dead TF-IDF bridge, dead error enums, ~340 lines of dead routes all
   removed recently). Simplification is a practiced habit here, which is why the
   remaining findings below are mostly *recent* accretions or blind spots, not neglect.

## 4. Cons

1. **The test gap contradicts the philosophy's core claim.** The engine is supposed to
   be the deterministic ground truth, and its statistics module has zero tests. Rule 2
   is the only rule with no enforcement and, unsurprisingly, the lowest adherence
   (~55% of eligible Rust files, 1/12 frontend stores).
2. **`store/src/db.rs` is a 2,152-line god module** — 109 methods in one impl block,
   holding insight history, the action log, cluster edits, taxonomies, document labels,
   agent runs, and enrichment functions. Those are product/curation domains living in
   the *storage* crate, reached via a `store.db()` escape hatch (`store/src/lib.rs:553`)
   ~35 times from the API. The façade exists and is bypassed.
3. **Layering inversions at the edges.** `brightflow-cli` imports non-HTTP helpers from
   `brightflow-api` (`load_label_targets`, `AuthDb`, `hash_password` — `main.rs:987,
   1066, 1317`), dragging the whole axum/tower/sqlx/argon2 graph into `topics fit`. CLI
   flags are passed by mutating process env vars and re-reading them
   (`main.rs:731-739` → `ServeConfig::from_env()`).
4. **Error-strategy fragmentation.** Four strategies across crates (structured
   `AppError`, thiserror enums, `anyhow` collapsed into one variant, stringly
   `ConnectError`), and the same precondition — store not configured — surfaces as
   `400 BadRequest` in 26 routes and `500 Internal` in 15 others with three different
   message strings.
5. **Five SQLite databases, five hand-copied pool bootstraps** that have already
   diverged: `foreign_keys(true)` is present in store and ingest but absent in auth and
   scheduler.
6. **Unsupervised background loops.** Scheduler tick, flush task, sampler, and session
   expiry are bare `tokio::spawn` with no shutdown path; the one `abort()` is
   unreachable in practice (`api/src/lib.rs:423`).
7. **Single-point enforcement** (no CI, bypassable hook) — accepted deliberately, but
   it makes every guarantee in §2 contingent on one symlink existing.
8. Minor: LLM API keys stored plaintext in the auth SQLite DB (fine for self-hosted,
   worth a note in the provider table's module doc); `rpassword = "5"` is two majors
   behind; `brightflow-connect` depends on `longbow` via a path outside the repo, so the
   workspace does not build from a clone of this repo alone; `.env.development`/
   `.env.production` are tracked (benign contents, but the `.gitignore` `.env` pattern
   silently doesn't cover them).

---

## 5. Simplification catalog

This is the requested centerpiece: what can be deleted, what should use a library
already in the tree, and what is duplicated or at the wrong altitude. Ordered by
effort-to-payoff. Everything cites the dependency that is *already installed* — nothing
below proposes a new dependency.

### 5.1 Delete — dead code (zero behavior change, ~900+ lines)

| What | Where | Evidence |
|---|---|---|
| Sparse k-means module, **373 lines** | `engine/src/nlp/clustering.rs` (+ re-export `nlp/mod.rs:29`) | `kmeans(` has zero callers outside its own test module (verified by grep across all crates). Production uses `kmeans_dense`/`hdbscan_dense`. Keep `SparseVec`/`similarity::cosine` — still used by `topic_enricher.rs`. |
| Unreachable query-store state, **~90 lines** | `brightflow-app/src/stores/query.ts:55-67, 86-115, 138-155` | `selectedColumns`, `groupByColumns`, `aggregations`, `pivot` are never touched outside the store and its test; the `groupBy`/`pivot`/`select` branches of `operations` can never fire; `previewTexts` has 0 references app-wide. `isValid` is a constant `true` whose only consumer tail-calls it. |
| Marketing-only CSS, **~200 lines** | `brightflow-app/src/assets/miami.css:355-437, 458-538` | `glass`, `card-surface*`, `bg-grid`, `mask-fade-*`, `reveal-*`, four `.bg-gradient-*` classes, three keyframes: 0 references in `src/`. The `.bg-gradient-*` four are plain classes, so they emit CSS unconditionally. Split app tokens from marketing CSS. |
| Empty directories | `brightflow-app/src/views/`, `schemas/` (repo root) | Both empty; `views/` is a dead layer (route targets live under `components/`), `schemas/` is a vestige of a prior layout. |
| Unused exports | `services/api.ts:147` (`RunStatus`), `types/index.ts:151` (`AggregationDef`), `types/enrichment.ts:46` (`FunctionConfig`), `stores/index.ts:49-57` (8 barrel re-exports; all 52 consumers import from `@/stores/<name>` directly) | Zero references each. |
| Stale generated refs | `brightflow-app/components.d.ts:14-16` | References three deleted `__scratch/__gate` test components; regenerate. |
| Removable runtime dependency: `@lucide/vue` | 22 files import SFC icons while 43 files use iconify `i-lucide-*` strings | Nuxt UI's own item APIs only accept the iconify string form, so the repo already runs both systems. `@iconify-json/lucide` covers 100% of usage; drop the package. |

### 5.2 Use the library already in the repo

**Rust — chrono (already a dependency of both crates involved):**

| Hand-rolled | Where | Replacement |
|---|---|---|
| 12-arm month-name `match` (`"01" => "January"`) + `split('-')` parsing | `engine/src/analysis/tree.rs:585-605` | `NaiveDate::parse_from_str(...).format("%B %Y")` — ~20 lines → 2 |
| Manual month arithmetic (`year * 12 + month - 1 + offset`, with `unwrap_or(2026)` fallbacks that silently produce wrong data in 2027) | `api/src/product_analytics/queries.rs:453-464` | `NaiveDate::checked_add_months(Months::new(n))`; also deletes the `#[allow(cast_*)]` at line 435 |
| `format("%Y-%m-01T00:00:00")` to get month start | `api/src/analytics/events_scan.rs:54` | `now.with_day(1)` (`Datelike` already imported) |
| Byte-slicing date strings `&ts[..10]` (panics on non-ASCII at byte 10; `indexing_slicing` lint is blanket-allowed so nothing flags it) | `api/src/product_analytics/queries.rs:421`, `api/src/analytics/events_scan.rs:67-68` | `ts.get(..10)` or format from the already-parsed `DateTime` on the adjacent line |

**Rust — other in-tree crates/std:**

| Hand-rolled | Where | Replacement |
|---|---|---|
| Manual `Display` + `Error` impls (the only hand-written pair in the workspace) | `connect/src/lib.rs:20-30` | `#[derive(thiserror::Error)] #[error("{0}")]` — thiserror is a workspace dep |
| 12-line `impl Default` setting every field to zero | `api/src/system/sampler.rs:28-39` | `#[derive(Default)]` |
| `loop { sleep(30s); tick() }` (period drifts by tick duration) | `scheduler/src/lib.rs:79`, `api/src/ingest/flush.rs:55` | `tokio::time::interval` — the same crate already uses it correctly in `system/sampler.rs:43` |
| SplitMix64 constants re-implemented in a test module | `engine/src/analysis/null_models.rs:221-228` | `use crate::nlp::SplitMix64` — same crate, already exported, already used by `api/src/agent/sampling.rs` |

**Frontend — @vueuse/core (already installed and used in the very same files):**

| Hand-rolled | Where | Replacement |
|---|---|---|
| `relativeTime()` copy-pasted byte-identically, plus a sign-flipped `relativeTimeUntil()` — and both render a frozen `Date.now()` snapshot that never ticks | `components/connect/RunHistoryTable.vue:27-39`, `ScheduleList.vue:41-66` | `useTimeAgo` — reactive (fixes the frozen-time bug), handles future timestamps natively |
| Scroll-position math with a magic `< 40` to detect "at bottom" | `components/system/SystemView.vue:88-107` | `useScroll` → `arrivedState.bottom` with an offset |

**Frontend — the framework itself:**

| Hand-rolled | Where | Replacement |
|---|---|---|
| `new Set(...)` clone per toggle "to trigger reactivity" | `stores/pivot.ts:107-116` | Nothing — Vue 3.5 refs deep-track `Set.add/delete`. Delete the clone and the false comment. |
| DOM archaeology to find the right-clicked cell (`closest('td')`, `cellIndex`, `rowIndex - 1`) | `components/results/DataTable.vue:67-101` | Nuxt UI `UTable` cell slots carry column/value directly (already used in `RunHistoryTable.vue:76`); the current form breaks on any UTable markup change — the exact breakage class that motivated adding vue-tsc |
| 110 lines of `@theme static` color aliases duplicating the palette mapping already declared to the Nuxt UI vite plugin (`vite.config.ts:88-96`) — and `brightflow-app/CLAUDE.md` itself says "colors configured in vite plugin, not CSS variables" | `assets/miami.css:84-193` | Keep only what Nuxt UI can't derive: `--color-brand-*` and the `--color-data-1..12` chart palette |
| `useQuery` + `watch(data → store.setX)` bridge, twice | `composables/useCuration.ts:28-49` | Pinia Colada's own `onSuccess`/direct `data` reads |
| Per-render `new Intl.NumberFormat` + locale-varying `toLocaleString` (pivot grid and data table format the same number differently) | `pivot/PivotTable.vue:288-322`, `charts/BigNumber.vue:133-147` | `utils/format.ts` — the cached, locale-pinned formatters already exist |
| Hardcoded `hsl(210, 70%, …)` conditional-formatting ramp, not dark-mode aware | `pivot/PivotTable.vue:263-285` | `color-mix(in oklab, var(--ui-bg-accented) …%, var(--ui-bg))` — mode-aware for free |

### 5.3 Deduplicate — one canonical copy

**Backend:**

1. **SQLite pool bootstrap ×5**, already divergent on `foreign_keys`
   (`store/src/db.rs:30-45`, `api/src/auth/db.rs:23-38`, `scheduler/src/db.rs:27-42`,
   `api/src/ingest/db.rs:24-41`, `api/src/ingest/buffer.rs:85+`). One
   `open_sqlite_pool(url, opts)` helper — `brightflow-core` is the natural home if it
   stays dep-free-adjacent, otherwise a tiny shared function in store.
2. **Engine text-prep re-implemented in the CLI ×2** (`cli/main.rs:1188-1211`,
   `1426-1450` copy `engine`'s private `build_combined_text`/`build_clean_texts`,
   `topic_enricher.rs:197-246`). Export the engine functions; delete the copies.
3. **"Write enriched parquet → ingest Overwrite → delete temp" ×3**
   (`api/topics/handlers.rs:397-423`, `cli/main.rs:1371-1397`, `1593-1613`).
4. **Enrichment-config resolution ×4 reading three different sources of truth** —
   CLI reads the *deprecated* `table_enrichment_settings`, scheduler reads promoted
   functions, API reads the in-memory overrides DashMap; the same match expression
   additionally appears 3× inside `topics/handlers.rs` (315, 853, 1005). This is the one
   duplication that is a *correctness* risk, not just bulk: the CLI path can silently
   use stale settings.
5. **Source-id/table-name conventions hardcoded at ~12 sites** (`format!("web:{}")`,
   `"connector:{}"`, `"events_{}"`, `"upload:{}"` across api/store/scheduler).
   `brightflow-core` describes itself as the single source of truth for where data
   lives; these naming conventions are the same kind of fact and belong there.
6. **`get_scheduler_db` helper exists (`api/connect/handlers.rs:17-24`) and is
   re-inlined 7×** in `api/scheduler/handlers.rs`. Similarly, "store not configured"
   should be one constructor with one status code (currently 400×26, 500×15, three
   strings).
7. Engine-internal: concentration-finding construction ~45 lines duplicated between
   `trends.rs:305-352` and `drivers.rs:320-370`; cosine/centroid/normalize loops
   duplicated across `nlp/{similarity,cluster_metrics,density,clustering,dense_clustering}.rs`;
   sort-by-significance closure in both output renderers.

**Frontend:**

8. **dtype classification ×7 with divergent lists — two of them wrong**
   (`BucketDropzone.vue:100` and `stores/dataset.ts:29` miss `i64`/`f64`, which the
   other five include). The complete implementation already exists as the
   module-private `normalizeType()` in `composables/useOperators.ts:33`. Export it,
   delete 10 inline arrays. Like §5.3.4, this is a live-bug class, not style.
9. **ECharts option skeleton ×8 insight renderers** (identical grid/tooltip/axis
   blocks; `TrendRenderer` vs `SeasonalityRenderer` differ by one `smooth` flag and a
   color index). One `periodSeriesOption()` helper.
10. **Empty-state markup ×23** (`flex h-full items-center justify-center` across 18
    files). One `common/EmptyState.vue`, consistent with the existing
    `CollapsibleSection` precedent.
11. **`useWsQuery`'s two 30-line query blocks** (`useWsQuery.ts:55-79` vs `180-205`)
    — extracting `sendQuery(ops): Promise<ResultData>` also surfaces the latent
    correlation bug: two in-flight queries both resolve on the first `queryResult`
    frame because there is no request id.
12. `stores/system.ts:69-88` duplicates `stores/connection.ts:88-115` socket-status
    plumbing and re-declares the status union locally instead of importing
    `ConnectionStatus`.

### 5.4 Altitude — same behavior, restructured (larger, optional)

- `engine/analysis/engine/trends.rs::run_trends_impl` — a 500-line single function
  whose 7 match arms are each a self-contained detect→score→floor→emit block; extract
  one function per arm (same shape in `review.rs`). Pure mechanics, no design work.
- `api/actions/handlers.rs` — `execute_action` (455 lines) and `apply_undo` (190
  lines) are two parallel giant matches over ~27 variants kept in sync by hand; a
  per-variant `execute`/`undo` pairing would make forgetting one a compile error.
- `store/src/db.rs` — split by domain (catalog / curation / insights / agent /
  enrichment) even if it stays one crate; the API's ~35 `store.db()` calls make the
  current façade a fiction.
- `cli/main.rs` — move `topics_eval`/`topics_eval_classifier` analysis bodies into the
  engine crate (also fixes the api-import layering inversion); `api/src/lib.rs::serve`
  (301 lines, 8 bootstrap jobs) splits naturally into named phases.
- Frontend: `stores/results.ts` — ten parallel refs that are really two
  `ResultSet` records (~60 of 180 lines); `stores/ui.ts` passthrough setters with one
  call site each; `FunctionEditor.vue` (567 lines) branching on `kind` throughout —
  two thin editors behind a shell; `services/api.ts` — 18 namespace objects and
  mid-file imports → split by domain around the shared `request` core;
  `SourceLayout.vue` — `defineAsyncComponent` the tool map to restore the router
  header's (currently false) lazy-loading claim.

### 5.5 Checked and clean — no findings

For calibration, these were explicitly audited and are already using the library
defaults correctly: migrations (`sqlx::migrate!` everywhere, no hand-rolled framework),
CLI parsing (full clap derive), JSON (serde_json throughout), middleware (tower-http
`TraceLayer`/`CorsLayer`; the two custom `from_fn` layers are auth guards with no
tower-http equivalent), threading (no `std::thread`/`mpsc` anywhere; tokio + dashmap),
CSV/Parquet (all Polars), row mapping (126 `query_as` + `FromRow`, zero `row.get`).
The hand-rolled LLM retry/backoff and the scheduler's `${VAR}` expansion are both
justified (no backoff or templating crate in tree, both centralized and documented).
There is exactly one trait in the workspace and it earns its dyn dispatch. No
speculative generics, no pathological clone density.

---

## 6. Recommendations, ranked

1. **The delete batch (§5.1)** — ~900+ lines and one npm dependency removed with zero
   behavior change. Purely mechanical.
2. **The two live-bug-class dedups**: dtype lists (§5.3.8) and enrichment-config
   resolution (§5.3.4) — these are the findings where duplication is currently
   producing *wrong behavior*, not just bulk. Add the `useWsQuery` request-id fix
   (§5.3.11) and the two panic-capable date slices (§5.2).
3. **Library swaps (§5.2)** — chrono dates, thiserror, `useTimeAgo`, interval,
   derive-Default. Each is minutes of work; together they delete the majority of the
   "own implementation where a default exists" surface.
4. **Test the two standout gaps**: `stats/significance.rs` (the philosophy's own
   ground-truth claim) and `csvPreview.ts` + `promptTokens.ts` (cross-language contract
   with Rust). This is not "raise coverage" — it is five files.
5. **Close the enforcement gaps that are trivially scriptable**: reject staged edits to
   existing `plans/*`/`reports/*` files (would have caught `211e85e`); swap test-module
   `#[allow]` → `#[expect]` per the repo's own rule; fix or freeze `README.md` (its two
   stale claims are exactly the checker's by-name exemption at work — consider
   generating the crate table from `cargo metadata` instead).
6. **Then the structural items (§5.4)** — highest payoff is `db.rs` by domain and the
   `execute_action`/`apply_undo` pairing; both are shape changes the deletion-heavy
   habits of this repo are well suited to.

One meta-observation to close on. This repo already audits itself — the 2026-08-03
philosophy report found and drove out a large simplification wave days before this
review. What this pass adds is mostly (a) the places the previous wave's blind spots
were structural (exempted-by-name files, unenforced rule 2, crate-level lint allows
masking real panics), and (b) the duplication that accreted *across* boundaries
(CLI↔engine, api↔store, component↔component) rather than within one file. The
philosophy is sound and unusually well practiced; its remaining failures are precisely
where its own enforcement model predicts they would be.
