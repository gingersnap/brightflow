# Plan: implement the improvements from reports/2026-08-03_full-repo-review.md

**Status:** implemented (2026-08-04)
**Date:** 2026-08-04

## Context

The 2026-08-03 full-repo review identified improvements across the Rust workspace and
the Vue app: dead code, hand-rolled code where an already-installed library suffices,
duplications (two of live-bug class), missing tests on the code the philosophy leans on
hardest, and larger structural refactors. This plan implements **all** of them, in three
waves, except:

- **EXCLUDED: `brightflow-app/src/assets/miami.css` is never touched** (shared with a
  marketing site — the unused-marketing-CSS and duplicate-color-alias findings are out
  of scope). Component-local styling changes are fine.
- **User decision: no plans/reports immutability check** is added to
  `check-conventions.sh` (honor system stays).
- **User decision: README.md is hand-fixed** (no generated crate table).

Binding repo rules for every step: co-located tests for touched pure logic (rule 2);
`//!` / `/** */` docs written for new files and re-verified/corrected on touched files
(rule 1); no new external dependencies (removing `@lucide/vue` is fine; internal
workspace path-deps are fine); strict clippy stays green; Rust work ends with
`cargo build --release`.

**Design corrections found during planning** (supersede the report where they differ):
- The enrichment-config divergence is *five*-way, not four: `api/src/state.rs:386`
  `hydrate_enrichment_overrides` picks the topic_model function **regardless of
  promoted status**, while the scheduler requires promoted. Consolidation unifies on
  promoted-only (deliberate behavior change, called out in its commit).
- `execute_action`/`apply_undo` match different enums (17 `Action` + 10 `UndoOp`
  variants); both are already exhaustive — the hazard is *adjacency*, not missed arms.
  `UndoOp` serialization must not change (stored `undo_json` rows).
- The CLI's `std::env::set_var` flag-passing (`cli/main.rs:731-739`) runs inside the
  tokio runtime — a genuine thread-safety hazard, extra motivation for the fix.
- sqlx enables `foreign_keys` by default, so unifying the pool bootstrap on explicit
  `.foreign_keys(true)` is a behavioral no-op (safe).
- Frontend branches on the store-error **message** (`format.ts:177` matches
  `startsWith('No data store configured')`), never the status code — the 503
  unification is safe iff that exact message text is preserved.
- `useTimeAgo` can't be called per-row in render functions → use `useNow({interval})`
  + `formatTimeAgo`. Pinia Colada 1.3 has no per-query `onSuccess` → store-write moves
  into the query function.

---

## Wave A — backend mechanical (11 commits)

### A1. Delete dead code
- Delete `crates/brightflow-engine/src/nlp/clustering.rs` entirely (contains only
  `kmeans`, `ClusterResult`, `kmeans_pp_init` + own tests; `SparseVec` is in
  `sparse.rs`, `cosine` in `similarity.rs` — untouched). Remove `pub mod clustering;`
  and the re-export at `nlp/mod.rs:29`.
- `rmdir schemas/` (empty, untracked).

### A2. Engine chrono/RNG swaps
- `analysis/tree.rs:584-605`: month-name match → `chrono::NaiveDate::parse_from_str`
  + `.format("%B %Y")`; garbage like `"2023-XX"` now passes through unchanged (note in
  fn comment). Create the file's first `#[cfg(test)]` module: `humanize_period` cases
  (month/week/quarter/year/garbage) + `pluralize`.
- `analysis/null_models.rs:221-228`: test `noise()` → `use crate::nlp::SplitMix64`.
  Risk: the noise stream changes (old copy omitted a mix constant) — the calibration
  tests (FP-rate < 0.10 at n=400) should still pass; if one flips, adjust seed
  derivation, never the assertion.

### A3. API/scheduler swaps + period-key tests
- `api/src/product_analytics/queries.rs`: `ts_to_period_key` fallback →
  `ts.get(..10).unwrap_or(ts)` (removes panic path); `offset_period_key` month arm →
  `NaiveDate::from_ymd_opt` + `checked_add_months(Months::new(n))`, unparseable keys
  returned unchanged (replaces `unwrap_or(2026)` silent fallback; matches the fn's
  existing malformed-shape policy — callers unchanged); delete the
  `#[allow(clippy::cast_*)]`; tidy week arm to `split_once("-W")`. New test module:
  `ts_to_period_key` (RFC3339 month/week incl. year boundary, non-RFC fallback, short
  input), `offset_period_key` (in-year, year-crossing, week, malformed passthrough,
  `"2026-99"` passthrough pinned), `generate_period_keys`.
- `api/src/analytics/events_scan.rs`: `:54` month-start → `now.with_day(1)`;
  `:67-68` slices → `.get(..10).unwrap_or(...)`. Extract pure
  `resolve_dates_at(now, period, start, end)` (thin `Utc::now()` wrapper remains);
  extend the existing test module (`month` start with fixed now, `today`, `7d`,
  explicit bounds, `date_filters` prefix + short-string passthrough).
- `api/src/system/sampler.rs:28-39`: manual `impl Default` → `#[derive(Default)]`.
- `scheduler/src/lib.rs:78-83` + `api/src/ingest/flush.rs:54-59`: sleep-loops →
  `tokio::time::interval_at(now + period, period)` with
  `MissedTickBehavior::Delay` — `interval_at` keeps the current wait-one-period-first
  startup behavior (a plain `interval` fires immediately and would race scheduler
  startup); `Delay` approximates the old sleep-after-work spacing.

### A4. `ConnectError` → thiserror
`crates/brightflow-connect`: add `thiserror = { workspace = true }`; replace the manual
`Display`/`Error` impls (`lib.rs:20-30`) with
`#[derive(Debug, thiserror::Error)] #[error("{0}")]`. Keep the existing doc comment
(still true). Add the crate's first test: `to_string()` round-trip.

### A5. Shared SQLite pool bootstrap
New `crates/brightflow-store/src/sqlite.rs` (NOT in db.rs — avoids collision with the
Wave B split): `open_sqlite_pool(url, profile)` with
`SqlitePoolProfile::{METADATA, BUFFER}` (max_connections/cache_size/mmap_size), WAL +
`synchronous=NORMAL` + `foreign_keys(true)` + `busy_timeout(5s)`. Rewrite the 5 call
sites: `store/src/db.rs:30-43`, `api/src/auth/db.rs:23-35`, `api/src/ingest/db.rs:24-38`,
`scheduler/src/db.rs:27-40`, `api/src/ingest/buffer.rs:85-100` (BUFFER profile).
Migrations stay at each site. Test: tokio + tempfile, open a pool, assert
`PRAGMA foreign_keys`=1 and `journal_mode`='wal'.

### A6. Source-id naming → brightflow-core
New `crates/brightflow-core/src/source_ids.rs` (pure `format!`, keeps core zero-dep):
`web_source_id`, `connector_source_id`, `upload_source_id`, `events_table_name`, each
with a literal-pinning test. Update `core/src/lib.rs` `//!` (scope widens beyond paths).
Rewrite ~13 call sites (api: `ingest/flush.rs:125,129`, `ingest/handlers.rs:290`,
`analytics/events_scan.rs:112-113`, `analytics/handlers.rs:234`,
`sources/handlers.rs:23,60`, `scheduler/handlers.rs:203`; store: `lib.rs:576,591`;
scheduler: `lib.rs:340`). `brightflow-store/Cargo.toml` gains the path-dep on core.
Parse-side helpers (`starts_with("upload:")`) are a flagged follow-up, not in scope.

### A7. Enrichment text builders + enriched-table overwrite
- Make `build_combined_text`/`build_clean_texts`
  (`engine/src/enrichment/topic_enricher.rs:197,234`) `pub`, re-export from
  `enrichment/mod.rs`; replace the two byte-similar CLI copies
  (`cli/main.rs:1188-1211`, `:1426-1450`). Add unit tests for both builders (join,
  missing-column error, `None` for too-thin rows).
- Replace the 3 "temp parquet → ingest Overwrite → delete" blocks
  (`api/topics/handlers.rs:391-424`, `cli/main.rs:1371-1397`, `:1593-1613`) with the
  existing `ParquetStore::replace_table_data(source, table, df, None)` — no new helper
  needed; it already does this transactionally and is store-test covered.

### A8. Store-error unification + AppState accessors + route comments
- New `AppError::StoreUnavailable` in `api/src/shared/error.rs`: message exactly
  `"No data store configured"` (frontend matches on it), status **503**, code
  `STORE_UNAVAILABLE`; extend the pinned parts() unit test.
- `AppState::require_store()` and `require_scheduler_db()` in `state.rs`; delete
  `connect/handlers.rs:17-24 get_scheduler_db`; replace the 7 inlined copies in
  `scheduler/handlers.rs` and sweep all ~41 store-missing sites (grep the three
  message strings) to `state.require_store()?`. Check `api/tests/` for pinned 400s.
- Fix the 5 route comments in `api/src/scheduler/handlers.rs` (`/api/connectors` →
  `/api/connector-configs`).

### A9. Engine-internal dedups
- Concentration: rename `drivers.rs:315 detect_drivers_concentration` →
  `add_concentration_finding`, `pub(super)`, fix its doc ("shared by Trends and
  Drivers"); trends' arm (`trends.rs:298-353`) collapses to one call. Behavior gated
  by `drivers_report.rs` + `insights_pipeline.rs`.
- NLP vector math: `cluster_metrics::cosine_distance` delegates to
  `similarity::dense_cosine_unnormalized`; new `pub(crate) mean_centroids(...)` +
  `l2_normalize_in_place(...)` in `dense_clustering.rs`, rewired into `density.rs`,
  `cluster_metrics.rs` (no normalize there — deliberate), `recompute_centroids`; both
  test-module `norm()` copies use the shared fn. Unify on the `> 0.0` zero-guard;
  `topics_quality.rs` confirms reproducibility. Unit tests for both helpers + a
  known-value cosine_distance test.
- Sort-by-significance: `AnalysisTree::sorted_by_significance_desc(&[NodeId])` in
  `tree.rs` (lifted closure, NaN-safe, `.get` not indexing); both
  `output/html.rs:174-182` and `output/markdown.rs:114-122` call it. Test in the
  tree.rs module created in A2.

### A10. `stats/significance.rs` test module
~18 tests over all 11 fns, tolerance-based asserts (no float_cmp expect needed):
`p_value_from_z` (0 → 1.0, 1.96 → ≈0.05, symmetry); `p_value_for_correlation`
(n≤2 policy, r=±1 → 0, known r=0.5/n=30 ≈ 0.0049); `p_value_welch_t_test` (small-n
and sd=0 policies, separated groups); `z_score` (known value, sd=0 → 0);
`mean`/`std_dev` (empty → 0, known values); `pearson_correlation` (±1, constant →
None, short/mismatched → None); `linear_regression` (exact line, constant x → None,
constant y → r²=0); `autocorrelation` (short → None, period-4 signal at lag 4 ≈ 1);
`p_value_for_autocorrelation` (branch pins — assert current behavior, don't "fix");
`prediction_interval` (n<4 → None, exact/off-line/noisy cases).

### A11. `#[allow]` → `#[expect(..., reason)]` sweep
~22 `#[cfg(test)]` modules with genuinely-uncovered lints (float_cmp, cast_*,
shadow_unrelated, …) convert to `#[expect]` with one-line reasons; lints already
covered by clippy.toml's `allow-*-in-tests` are **deleted from the lists**, not
converted (they'd warn as unfulfilled). The 8 `crates/*/tests/` files (NOT covered by
clippy.toml) convert their inner `#![allow]` to `#![expect(..., reason = "integration
tests panic on failure by design")]`. Then
`cargo clippy --workspace --all-targets` and prune anything reported unfulfilled.

### A12. README hand-fix
Line 9: `# Shared types and errors` → `# Workspace paths (WorkspacePaths), zero-dep`;
line 16: drop `schedule` from the command list; delete the
`cargo run -- schedule` stub block (~lines 29-31).

---

## Wave B — backend structural (9 commits, in this order)

### B1. Pin the undo round-trip first
New integration test in `api/tests/action_log.rs`: dispatch `Action::ExcludeTerm`
(needs no embedder), assert Applied + `undo_json` present, run undo via the public
surface, assert status `undone` and the term gone. This pins execute→capture→undo
across the seam B4 cuts.

### B2. Split `store/src/db.rs` (2,152 lines) by domain — motion-only
`db.rs` → `db/` directory; `StoreDb` struct + `new()` stay in `db/mod.rs` (public path
unchanged; child modules see the private `pool` field). One `impl StoreDb` per file:
`catalog.rs` (tables/files/stats/partitions/semantics/analysis settings :50-562 minus
:563-616, registered sources :1647-1699), `insights.rs` (:617-895), `actions.rs`
(:896-1034), `curation.rs` (:1035-1561), `agent.rs` (:1562-1646), `enrichment.rs`
(:563-616 deprecated settings + :1701-2152 functions/runs/cache). Zero SQL/signature
edits; review with `git diff --color-moved=dimmed-zebra`. Rewrite `db/mod.rs`'s `//!`
(the "one flat query module" rationale is being reversed — correct it, don't water it
down). New `//!` per file (content specified in the design; each states its domain).

### B3. One enrichment-config resolution (the correctness commit)
- Store (`db/enrichment.rs`): `get_promoted_function_config(table_id, kind) ->
  Option<String>` — one SQL join of promoted function → current version's
  `config_json`. Test in `store/tests/enrichment_functions.rs` (returns current
  version; None for non-promoted).
- Engine: `pub fn resolve_topic_config(table_name, stored_spec_json: Option<&str>,
  overrides: Option<&EnrichmentOverrides>) -> Option<EnrichmentConfig>` — precedence
  overrides > stored promoted spec > builtin; no text_columns filter inside (callers
  differ). Co-located tests incl. unreadable JSON. Fix the stale `//!` in
  `enrichment/config.rs` (still names `table_enrichment_settings`).
- Migrate all five sites: scheduler `resolve_topic_config` (5-line glue), API
  `resolve_enrichment` + the two other match copies in `topics/handlers.rs`,
  `hydrate_enrichment_overrides` (**now promoted-only — deliberate**), CLI
  `resolve_enrichment_config` (stops reading deprecated `table_enrichment_settings`;
  non-builtin tables with a promoted function become CLI-enrichable — intended).
- Delete the now-caller-less `StoreDb::get_enrichment_settings`. Keep the dual-write
  path (dropping the table is a separate migration decision — note in the doc).

### B4–B9 (per the detailed designs)
- **B4** `engine`: pure eval extraction — `nlp/cluster_eval.rs::eval_clustering` and
  `classifier_eval` (bodies of `cli/main.rs:1473-1508`, `:1101-1153`) with synthetic-
  vector tests; `read_row_ids` + `label_join_id_column` move to engine
  (`enrichment/labels_io.rs`); `api/topics/labels.rs` shrinks to store-fetch glue
  (API callers unchanged); `topics/display.rs` delegates `id_column`.
- **B5** `core`+`cli`: `WorkspacePaths` gains explicit override fields + builder
  methods (`with_store/with_connector_configs/with_auth_url`; precedence override →
  env → default, with tests); `build_serve_config` becomes value-based — **no more
  `std::env::set_var` inside the runtime**.
- **B6** `cli`: split `main.rs` (1,617 lines) into `logging.rs` +
  `commands/{serve,insights,store,admin,topics}.rs`; topics glue localized; the api
  imports drop to `auth` (create-admin keeps `AuthDb`/`hash_password` deliberately —
  it's server admin tooling; note in its `//!`) + `serve`. No CLI tests exist; pin by
  build + smoke-running `topics list` / `insights` on a fixture CSV.
- **B7** `api`: actions split into `actions/exec/{mod,clusters,insights,semantics,
  taxonomy}.rs` — adjacent `execute_x`/`undo_x` per domain, helpers move along
  (`edit_cluster`, `average_label_centroids` + its tests, `upsert_semantic_preserving`,
  `owned_category`, …); `execute_action`/`apply_undo` stay in `handlers.rs` as
  exhaustive one-line-per-arm dispatchers (no wildcard, no traits); **`Action`/`UndoOp`
  enums untouched** (stored `undo_json` must keep deserializing); borrowed-args structs
  for the two wide undo variants (6-arg clippy threshold). Gated by B1's test.
- **B8** `engine`: per-arm extraction in `trends.rs`/`review.rs` — shared tail
  `add_scored_root(tree, ctx, analysis, description, data, RootMeta) -> Option<NodeId>`
  in `engine/meta.rs` (with a unit test: floored-out vs passing), per-arm methods with
  a `TrendCtx` struct; **keep the `first_level_count`/`deeper_count` increments exactly
  where they are relative to depth guards** (counter semantics). Gated by
  `insights_pipeline.rs` + `drivers_report.rs`. Afterwards trial-remove
  `too_many_lines`/`cognitive_complexity` from the engine crate-root allow and report
  the residue (don't gate on it).
- **B9** `api`: `serve` split into `bootstrap.rs` phases (build_state,
  seed_and_recover, init_auth, start_scheduler, start_ingest,
  build_session_auth_layers, build_cors) in identical call order; `build_cors`
  parameterized on `app_env` so the production bail becomes pure — with unit tests
  (explicit origin OK; production + no origin errors; dev default). `serve` shrinks to
  ~50-line orchestration.

Also fold the crate-root blanket-`#[allow]` reduction into B7/B8/B9 where the
refactors eliminate the need (measure, shrink lists to what still fires, convert
survivors' rationale comments to item-scoped `#[expect]` where feasible).

---

## Wave C — frontend (12 commits / 10 phases, in this order)

Line numbers verified 2026-08-04; the design includes exact edits per phase.

1. **Deletions** — `stores/query.ts` dead state (~90 lines: dead refs, unreachable
   `operations` branches, `previewTexts`, `isValid`; `canExecute()` in
   `useWsQuery.ts:212-222` becomes `isConnected && hasData`; `QueryBuilder.vue:140`
   simplifies), cascade-delete `PivotState`/`Aggregation` and shrink `QuerySections`
   to `{filter, sort, limit}` in `types/index.ts`; update `query.test.ts` + its
   header. Delete unused exports (`RunStatus`, `AggregationDef`, `FunctionConfig`,
   the `stores/index.ts:50-58` barrel block — keep `resetAllStores`/`resetOnLogout`).
   `rmdir src/views`. Regenerate `components.d.ts` via `npm run build` (do NOT
   hand-edit; verify the diff removes exactly the 3 stale lines). Fix the stale
   auth-store comment in `stores/index.ts:19-23`.
2. **Shared pure modules** — new `src/utils/dtype.ts`: move `normalizeType` from
   `useOperators.ts:33-54` as `normalizeDtype` + `isNumericDtype`/`isStringDtype`/
   `isFloatDtype` (deliberate extension: `'number'` → int bucket, since call sites
   include it); migrate all 8 sites incl. the two wrong-list bugs
   (`BucketDropzone.vue:100`, `stores/dataset.ts:29`); tests incl. i64/f64
   regressions. Extend `utils/format.ts` with cached `formatDecimal(value,
   maxDecimals)` + tests. New `utils/csv.ts`: `escapeCsvCell` (moved) + `buildCsv` +
   tests. Move `isActionBatch`/`isInsightsComputed` into `services/wsGuards.ts`
   (header becomes true) + guard tests.
3. **Store shapes** — `stores/results.ts`: 10 refs → two `ref<ResultSet>` + a
   `Record<ResultType, Ref<ResultSet>>`; `setResults(type, data)`; delete aliases,
   `columnNames`, wrappers; **move `exportCsv` to `ResultsPanel.vue`** using
   `buildCsv` + deferred `revokeObjectURL`; migrate 7 consumers; new
   `results.test.ts` (canonical Pinia pattern). `stores/ui.ts`: `toggleSection` via
   `Record<SectionName, Ref<boolean>>`; delete passthrough setters (3 call sites go
   direct; the localStorage serializers are read-only — verified no behavior change).
   `stores/pivot.ts:112-114`: delete the Set clone + false comment (Vue 3.5 tracks
   collection mutations; consumers verified reactive).
4. **WS request-id correlation + dedup** (touches backend):
   `crates/brightflow-api/src/analytics/types.rs` — `Query` gains
   `#[serde(default)] request_id: Option<String>`; `QueryResponse` and the `Error`
   variant gain skip-if-none `request_id` echoed in `execute_ws_query` (None at the
   REST path and parse-error sites); serde round-trip tests; regenerate
   `types/generated/` via the pre-commit recipe; `cargo build --release`. Frontend
   `useWsQuery.ts`: one `sendQuery(operations): Promise<ResultData>` with
   `crypto.randomUUID()` correlation (accept uncorrelated errors — parse failures
   carry no id); `loadTableData`/`executePivot` become thin wrappers. Fixes the
   two-in-flight-queries bug.
5. **Library swaps** — `useNow({interval: 30_000})` + `formatTimeAgo` replace the
   copy-pasted frozen `relativeTime`/`relativeTimeUntil`
   (`RunHistoryTable.vue`, `ScheduleList.vue`; accepted copy change to vueuse's long
   form). `SystemView.vue` scroll → `useScroll` `arrivedState.bottom` (offset 40).
   `useCuration.ts` → store-write inside the query fn, both `watch` bridges deleted
   (also fixes the cached-remount seeding bug). `bindSocketStatus(client, status,
   openStatus?)` in `services/websocket.ts` + tests; `stores/system.ts` imports shared
   `ConnectionStatus` and both stores use the helper.
6. **Rendering/formatting dedups** — `PivotTable.vue`/`BigNumber.vue` formatting →
   `formatDecimal`/`formatCompact`/dtype helpers (en-US pinned — the point);
   `getCellStyle` heatmap → `color-mix(in oklab, var(--ui-bg-accented) pct%,
   var(--ui-bg))` (decide neutral vs primary-hued visually in dev, light + dark). New
   `common/EmptyState.vue` (CollapsibleSection precedent) migrating the 12 renderer
   fallbacks + ResultsPanel/PivotTable/BigNumber/UserExplorerPanel/SystemView empty
   states (skip BucketDropzone — drop-target affordance, and layout wrappers). New
   pure `renderers/periodSeriesOption.ts` (spec: labels/series/yName + xFormatter/
   rotate/gridBottom/boundaryGap/legend/yScale/tooltip/axis-overrides) + tests;
   migrate the 8 period renderers; leave Concentration/Correlation/Segment/Membership
   alone.
7. **DataTable context menu** — column defs get `id` + `accessorFn` (kills the latent
   dotted-name accessorKey bug) with `header`/`cell` render functions carrying
   `onContextmenu` → `openMenu(e, {colName, value})`; delete the
   `closest()`/`cellIndex` block (:58-118). Known tradeoff: right-clicks on td
   padding — keep a wrapper-level no-op `@contextmenu.prevent`; check in dev.
8. **Structure** — `SourceLayout.vue`: 12 static tool imports →
   `defineAsyncComponent` (router.ts's lazy-loading header becomes true; verify
   per-tool chunks in `npm run build`). `FunctionEditor.vue` (567 lines) →
   `LlmFunctionEditor.vue` (sample loop, LLM draft) + `TopicFunctionEditor.vue`
   (text columns, topic draft) via `v-model:config` + `defineExpose({restoreConfig})`,
   shell keeps save/promote/rerun-dialog/version-restore/delete (~180 lines).
   `services/api.ts` → `services/api/` directory (`core.ts` with
   `request`/`ApiError`/401 handling + 8 domain files + barrel `index.ts`) — every
   existing `@/services/api` import keeps working; mid-file imports fixed.
9. **`@lucide/vue` removal** — migrate the 22 files (enumerated in the design with
   per-file icon lists and the `Component`→string type change in
   `insights/nodeMeta.ts`); kebab-case mapping with rename checks
   (`AlertTriangle→i-lucide-triangle-alert`, `HelpCircle→i-lucide-circle-help`);
   remove the package + `npm install`; `grep -r "@lucide/vue" src` empty; commit
   regenerated `components.d.ts`/`auto-imports.d.ts`; manual icon sweep per area.
10. **Tests for untested pure modules** — `csvPreview.test.ts` (quote/CRLF/maxRows/
    slugify cases), `promptTokens.test.ts` (parity fixture pinned to the Rust
    `extracts_refs_in_order_deduplicated` test, named in a comment so divergence is
    found from either side), `useTextExplore.test.ts` (`parseTermInput` — already
    exported, no source change).

---

## Cross-wave ordering & conflicts

Run **Wave A → Wave B → Wave C** (C4's small backend edit lives inside Wave C).
Known shared files, resolved by this order: `topics/handlers.rs` (A7 C3-swap, then B3
resolve consolidation, then B7 dispatch arms); `trends.rs`/`drivers.rs` (A9
concentration dedup lands before B8's per-arm extraction, which then extracts an
already-one-line arm); `cli/main.rs` (A7 text-builder swap before B6's file split);
`store/src/db.rs` (A5 touches only its pool bootstrap lines; B2's split moves them
into `db/mod.rs` untouched); `topic_enricher.rs` (A7 makes builders pub; B4 reuses).

## Verification

Per commit: the commands listed with each item, plus
`cargo fmt --check && cargo clippy --workspace --all-targets` (Rust) or
`npm run check && npm run test` (frontend), and `./scripts/check-conventions.sh` with
the work staged.

Final gate, after each wave and at the end:
```
cargo fmt --check && cargo clippy --workspace --all-targets && cargo test --workspace
cargo build --release
cd brightflow-app && npm run check && npm run test && npm run build
```
Manual dev-server checks (run backend + frontend as background tasks, watch
`logs/`): Explore query + pivot in quick succession (request-id fix), CSV export,
pivot collapse/expand + heatmap light/dark, Insights feed all card types, Topics
curation approve/undo, Enrich editor both kinds + version restore, /system log tail +
status dot, Sources/Connect relative times, icon sweep, `APP_ENV=production` CORS bail
once.

## Explicitly out of scope (flagged follow-ups, not in this plan)
- miami.css in any form (user exclusion).
- Dated-prose immutability check (user decision).
- Parse-side source-id helpers (`starts_with("upload:")` sites).
- Dropping the deprecated `table_enrichment_settings` write path/table (separate
  migration decision).
- CI, LLM-key encryption at rest, `rpassword` major bump, `longbow` path-dep — cons
  noted in the report but not part of its recommendation list.
