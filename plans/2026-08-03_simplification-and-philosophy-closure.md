# Simplification & Philosophy-Closure

**Date:** 2026-08-03
**Status:** in progress

## Context

The deep-research report (`reports/2026-08-03_philosophy-and-simplification.md`)
found ~2,900 lines of verified-dead/duplicated code, several places where
hand-written code should be library defaults, and four philosophy gaps. The user
approved fixing everything except the `longbow` path dependency (own session)
and the typed engine→api error boundary (own session; only the mechanical dedup
lands now). Decisions made:

1. Close both action-bus bypasses (recluster, column semantics).
2. Extend `check-conventions.sh` to `.vue` **and** backfill all missing headers now.
3. Bonferroni vs "false-discovery control": fix the one line in
   `docs/human_ai_interaction.md` — the *why* already lives inline
   (`null_models.rs`, `drivers.rs`), which is where the user wants it; the
   normative doc just needs its claim corrected.
4. Frontend data layer: **Pinia Colada for all server data; Pinia stores hold
   client state only; stores never fetch.** (Most opinionated setup, fewest
   per-case decisions. Rule gets written into `brightflow-app/CLAUDE.md`.)
5. Error boundary: mechanical dedup only (`parts()` fn).
6. Nuxt UI sweep: forms, tables, duplicates only (no token/toast sweep).

All dead-code claims below were grep-verified by research agents; headline items
(NUL byte, dead components, dead `DebugLog`, dead period fns, dead connect
store) were independently re-verified. Re-verify per item at execution time
anyway (`grep` before delete); items marked ⚠ carry a specific caveat.

**Commit style:** small conventional commits per phase/item, matching the
2026-08-02 hardening style (`refactor(engine): ...`, `fix(app): ...`). The
pre-commit hook (conventions → fmt → clippy -D warnings → tests → vp) gates
every commit. Repo rule 2 applies throughout: touched pure logic gets a
co-located test; repo rule 1: re-read and correct comments covering touched code.

---

## Phase A — Trivial fixes (one commit each, minutes)

1. **NUL byte**: `brightflow-app/src/composables/useTextExplore.ts:80` —
   replace `.join('\x00')` with `JSON.stringify(terms.value.map(...))`
   (collision-proof AND printable; a plain `' '` separator could collide with
   term text). Restores the file to grep visibility.
2. **Phantom diff**: `brightflow-app/vite.config.ts` — add `'auto-imports.d.ts'`
   and `'components.d.ts'` to `fmt.ignorePatterns` and `lint.ignorePatterns`;
   commit the regenerated (semicolon-free) files once. Kills the permanent
   72-line churn.
3. **Phantom crate edges**: remove `brightflow-core` from
   `crates/brightflow-store/Cargo.toml:7` and
   `crates/brightflow-engine/Cargo.toml:12` (zero `brightflow_core` references
   in either crate).
4. **Unused/redundant deps**:
   - `dotenvy` from `brightflow-api` (only call site is `cli/main.rs:449`).
   - `password-hash` direct dep in api — reached via `argon2` re-export; ⚠
     verify with `cargo tree -e features` that argon2's default `rand` feature
     supplies `rand_core` before deleting.
   - `once_cell` workspace-wide: 3 sites (`nlp/clean.rs:11`,
     `embedding/backend.rs:14` → `std::sync::LazyLock`;
     `embedding/encoder.rs:16` → `std::sync::OnceLock`). Std equivalents already
     used in 5 places.
   - polars `json` feature in workspace `Cargo.toml` — ⚠ grep for
     `JsonReader|JsonWriter|scan_ndjson|LazyJsonLineReader|json_normalize`
     first (agent found none).
   - Frontend: delete `@fontsource-variable/outfit` (zero references). Drop
     `'JetBrains Mono'` from `--font-mono` in `miami.css:216` (never loaded),
     and make `@utility font-mono-data` (`miami.css:514`) use `--font-mono`.
5. **Doc corrections** (hand-written-state files that are carved out but must be true):
   - Root `CLAUDE.md`: core is a paths crate (`WorkspacePaths`) — `TenantId`/
     `DatasetId` don't exist; fix the cli command list (add topics,
     create-admin, migrate-events, compact, sources).
   - `brightflow-app/CLAUDE.md`: delete the false "No UBadge usages exist"
     claim (9 files use it); fix the understated features list; note
     `FunctionBadge.vue:30` uses banned `size="sm"` — fix the component instead.
   - `docs/human_ai_interaction.md`: "effect size with false-discovery control"
     → wording that matches the code (Bonferroni-style multiplicity control).

## Phase B — Backend dead code (~1,245 lines)

Delete, in separate commits, re-grepping each before removal:

1. **`DebugLog` facility**: `engine/src/debug.rs` (203 lines) + 31
   `debug.section/kv` call sites + 6 `debug: &DebugLog` params in
   `analysis/engine/{drivers,review,trends}.rs`, `insights/handlers.rs`,
   `cli/main.rs`, `tests/insights_pipeline.rs`. `to_file()` has zero callers;
   all 9 constructions are `disabled()`.
2. **Sparse-TF-IDF subtree**: `engine/src/nlp/polars/{enrichment,transform,centroids,serde_utils}.rs`
   (~968 lines) + exports in `nlp/mod.rs:22`. ⚠ First verify the CLI `insights`
   CSV path and `output/*.rs` don't reach any of these exports.
3. **`analysis/period.rs`**: `compare_periods` (:176), `compare_periods_cached`
   (:133), `find_anomalous_period` (:292), `find_anomalous_period_cached`
   (:221), then the orphaned `aggregate_by_period` (:80). Keep
   `aggregate_by_period_cached` (live).
4. **`output/markdown.rs:133-274`**: `write_markdown_compact` + `write_ascii_tree`.
5. **AppState dead surface** (`api/src/state.rs`): `with_log_sender` (:118),
   `with_store_and_log_sender` (:310), `get_or_build_schema` (:211),
   `load_all_store_tables` (:522), `unload_store_tables` (:472),
   `table_exists` (:491). Then collapse the two surviving constructors' 24-field
   literals via `..Self::new()` struct-update.
6. **Legacy TOML config island** (`engine/src/data/config.rs` + `schema.rs:34`):
   `SchemaConfig`, `ColumnConfig`, `AnalysisSettings`,
   `default_comparison_periods`, `DataSchema::from_config` and friends. Keep
   `ColumnRole`, `Polarity`, `TimeGranularity`.
7. **Dead DB methods/handlers/routes** (~340 lines):
   `StoreDb::{delete_table_settings, get_all_enrichment_settings, delete_enrichment_settings, get_enrichment_function_by_name}`;
   `IngestDb::cleanup_old_salts` (⚠ salts grow unbounded — record as follow-up
   in the commit message; wiring it up is separate feature work);
   `SchedulerDb::get_sync_state`; api scheduler handlers
   `{list_sync_runs, get_sync_run, get_sync_state, create_job, list_jobs, get_job, trigger_run}`
   + their routes; connect handlers `{list_connectors, run_preset, schedule_preset}`
   + routes (fix the backwards "legacy (still used)" comment at
   `services/api.ts:209` and delete the dead `runPreset`/`schedulePreset`/
   `updateJob`/`deleteJob` wrappers);
   `cache.rs:52 dimension_coverage_in_period`;
   `classification_metrics.rs` `macro_precision`/`macro_recall`;
   `drivers.rs:97 two_period_significance`; `null_models.rs:185,221`;
   `FilePartitionRow` (`store/models.rs:44`); `RunRequest`
   (`api/connect/types.rs:25`, also removes its dead generated TS file);
   CLI `Commands::Schedule` stub. ⚠ Route deletions assume no external script
   callers — frontend verified clean.
8. **Dead error variants**: `BrightflowError::{DatasetNotFound, Storage, Serialization, Io}`,
   `StoreError::{InvalidTableName, SchemaMismatch}`, `IngestError::UnknownDomain`,
   `SubtextError::{NotFitted, InvalidParameter}`,
   `TopicError::{Embedder, Artifact}` (⚠ these two are `#[from]` — confirm no
   `?` conversion constructs them before removing). Then delete
   `BrightflowError` entirely: sole consumer `brightflow-connect` uses only
   `Other(String)` via `map_err` ×11 — switch connect to its own tiny error or
   `anyhow`, and `brightflow-core` becomes a zero-dep paths crate (drop its
   `serde_json` + `thiserror`).
9. Stale `#[allow(dead_code)]` on `Scheduler.paths` (`scheduler/lib.rs:48` —
   field is read); `generate_period_keys` dead `_period_type` param + mid-file
   `use chrono::Datelike` (`product_analytics/queries.rs:433,469`).

## Phase C — Frontend dead code (~1,000 lines)

1. Delete `components/analytics/AnalyticsView.vue` (455),
   `components/connect/ConnectorCard.vue` (231),
   `components/layout/DatasetPickerModal.vue` (166) — zero references each
   (re-verified). `components.d.ts` regenerates.
2. Delete `stores/connect.ts` (54) + its export in `stores/index.ts:58`
   (also removes the repo's one outright-false module header).
3. Delete `semanticsApi` (`services/api.ts:141-170`) and `datasetApi.upload`
   (:287-305) — zero callers. (Semantics routes die in Phase F.)
4. Dead store members (⚠ verify each name again before deleting — check
   template usage and `storeToRefs` destructuring): the ~40 listed in the
   report §5.1 across `dataset`, `results` (incl. the 3 "legacy computed"
   aliases), `pivot`, `connection`, `query` (the never-built `aggregations`
   block :178-200 — also remove its only writer in `query.test.ts:96`),
   `insightsActivity`, `ui` (incl. `sidebarCollapsed`, redundant with
   `UDashboardGroup storage="local"`), `source`, `insights`.
5. `useWsQuery.ts:229-231` `execute()` alias — inline `executePivot` at
   `QueryBuilder.vue:40`. Remove unreachable `messageQueue`/`flushMessageQueue`
   from `services/websocket.ts` (only caller gates on `isConnected`).
6. Delete empty `src/components/inputs/` directory.

## Phase D — Backend duplication collapse (~275 lines)

1. `dtype_to_string` ×3 → one fn in `api/src/analytics/` (executor's variant;
   note the `UInt*`→`"int"` difference deliberately).
2. `now_epoch()` ×4 → `chrono::Utc::now().timestamp()` inline (chrono already a
   dep); delete the helpers.
3. Dense `dot()` ×4 (`nlp/reduce.rs:183`, `nlp/density.rs:143`,
   `nlp/dense_clustering.rs:183`, `topic_enricher.rs:716`) + dense cosine in
   `enrichment/curation.rs:76` → `nlp/similarity.rs`. Pick `mul_add`
   deliberately (FP-observable in `reduce.rs`); run engine tests to confirm no
   locked-value drift. Unit-test the shared fns (rule 2).
4. `web_analytics`/`product_analytics` shared `resolve_dates`/`default_period`/
   `scan_source_events` (~80 lines) → shared module (e.g.
   `api/src/analytics/events_scan.rs`).
5. Byte-identical `read_string_at`/`read_i64_at`/`read_id_at`/`derive_title`
   (`topics/handlers.rs:683-712` ≡ `textexplore/handlers.rs:202-231`) → `shared/`.
6. `ParquetStore` 8× table-resolution preamble (`store/lib.rs`) → private
   `async fn table_id(...)`; drop the pure passthrough
   `has_any_column_semantics` (:580).
7. `agent/runner.rs:205` `chat_with_retry` → `brightflow-llm`'s
   `chat_with_backoff` with `RetryPolicy { max_attempts: 2, base_delay_ms: 5000 }`
   (behavior note: adds jitter).
8. `AppError`: collapse `into_response()`/`error_code()` twin 13-arm matches
   into one `fn parts(&self) -> (StatusCode, &'static str, String)`
   (`api/src/shared/error.rs:59-138`). Unit-test code↔status agreement.
9. `Action::scope()` 54-line match (`actions/types.rs:229`) → flattened
   `#[serde(flatten)] scope: Scope` struct on the variants. ⚠ Wire-format
   sensitive: JSON shape must be unchanged; regenerate ts-rs types and diff
   `types/generated/`; `every_variant_has_a_manifest_entry` and the manifest
   schema must pass unchanged. If ts-rs/schemars flatten output differs, abort
   this item and keep the match.
10. `ColumnRole` string→enum ×3 (`state.rs:554`, `actions/handlers.rs:1316`,
    `semantics/handlers.rs:325`) → `ColumnRole::parse` in
    `engine/src/data/config.rs` next to `Polarity::parse` (:70), with test.
11. IPv4 parsing ×2 (`api/lib.rs:91`, `cli/main.rs:752`) →
    `str::parse::<std::net::Ipv4Addr>()`; `ServeConfig.host` becomes `Ipv4Addr`.
12. `parse_date_string` hand-rolled tz stripping
    (`engine/analysis/period.rs:570-593`) → chain
    `DateTime::parse_from_rfc3339(s).map(|d| d.date_naive())` (pattern already
    correct at `product_analytics/queries.rs:416`). Existing tests at
    `period.rs:630+` cover it; add a multi-byte-input case (the current code
    has an indexing panic risk there).
13. Optional (low priority): `store/db.rs:296,316` row-by-row inserts →
    `sqlx::QueryBuilder::push_values`.

## Phase E — Frontend duplication + @vueuse (~250 lines)

1. **Declare `@vueuse/core`** (already in node_modules via Nuxt UI — zero new
   bundle weight).
2. **Merge `stores/system.ts`'s WS client** (:34-135) into
   `services/websocket.ts` via a `path` option (−90 lines; system gains
   heartbeat). While touching it, fix the broken contract: `disconnect()`
   permanently sets `reconnect = false` (:68) — make `connect()` reset it.
   Add unit tests for backoff schedule, unsubscribe closure, and the
   disconnect/reconnect contract (pure logic, rule 2).
3. Debounce clones (`QueryBuilder.vue:119-141`, `FilterBar.vue:36-51`) →
   `watchDebounced` (also fixes the missing unmount cleanup).
4. localStorage boilerplate (`ui.ts:30-95`, `source.ts:15-46`,
   `insightsActivity.ts:32,72`) → `useLocalStorage`.
5. Polling (`SchedulesPanel.vue:68-79`, `useEnrichRun.ts:21-49`) →
   `useIntervalFn`/`useTimeoutPoll`.
6. Clipboard ×5 → `useClipboard`.
7. `useChartColors` → `useCssVar` (fixes the theme-reactivity gap its header
   overstates; correct the header, rule 1).
8. `isActionEvent` ×2 (`curation.ts:28`, `insights.ts:101`) → one export in
   `services/` or `types/`; delete the "(same as curation store)" comment.
9. Filter→operation mapping ×3 (`useWsQuery.ts:33-45,97-110`, `stores/query.ts`)
   → one exported fn (the tested `query.ts` logic), tests move with it.
10. Teach `request()` (`services/api.ts`) to pass `FormData` (skip
    `JSON.stringify` + Content-Type); switch `sourceApi.uploadCsv` to it
    (restores 401→`clearAuth()` handling that the bespoke path skips).
11. ECharts registration ×4 → one `services/echarts.ts` (or keep the two
    per-area setup files and delete the inline `use([...])` calls — smallest
    diff wins).
12. `DataTable.vue` `formatCell` → tested `formatNumber` from `utils/format.ts`.
13. Error-ref renaming: rename shadowing `error` refs to `errorMessage` (as
    `curation.ts` already does) and delete the 12
    `oxlint-disable-next-line unicorn/catch-error-name` suppressions.

## Phase F — Action-bus closure (philosophy enforcement)

**Recluster (frontend-used bypass):**
1. Extract the body of `post_recluster` (`topics/handlers.rs:333`) into a plain
   service fn `topics::run_recluster(state, source_id, table, req) -> Result<...>`
   (no Axum extractors). HTTP handler becomes a thin wrapper... and is then
   **deleted** along with the route (`routes.rs:190`) — one door.
   `run_recluster_for_action` (:293) calls the service directly; this also
   removes the extractor-manufacturing inversion at `actions/handlers.rs:642,665`.
2. Frontend: replace `topicsApi.recluster` (`services/api.ts:416-423`) with
   `actionsApi.dispatch(Action::Recluster)` in `TopicsView.vue:71`,
   `TopicModelForm.vue`, `EnrichmentSettingsPanel.vue`, `paletteActions.ts`.
   ⚠ UX check: confirm what `ActionResponse` carries for recluster and refetch
   the topics overview on completion; keep the existing loading state.
   Result: recluster lands in the action log + feed like every other mutation.
3. Integration test in `crates/brightflow-api/tests/`: dispatching
   `Action::Recluster` produces an action-log row (the thing the bypass didn't).

**Column semantics (dead bypass):**
4. Delete the unlogged mutation routes `PUT .../semantics` (bulk),
   `PUT/DELETE .../semantics/{col}` (`routes.rs:174-181`) + their handlers in
   `semantics/handlers.rs` (frontend `semanticsApi` already deleted in Phase C;
   zero callers). Keep `GET` `list_semantics` and the table-settings routes
   (table settings are config-tier, like connectors/schedules — not
   interpretation-layer state). `upsert_semantic_preserving` via
   `Action::SetKpi`/`SetColumnPolarity` becomes the only write path to
   `column_semantics`.
5. The duplicated 25-line cache-invalidation blocks collapse to the one copy in
   `actions/handlers.rs:1291-1313` (extract to a named fn if >1 caller remains).

## Phase G — Colada-only data layer

**Rule (add to `brightflow-app/CLAUDE.md`):** *Server data is fetched and cached
by Pinia Colada (`useQuery`/`useMutation`) in components/composables. Pinia
stores hold client state only (selections, overlays, UI flags). Stores never
import `services/api`.*

Migrate the five live hand-rolled stores, one commit each:
1. `auth.ts` (82) — session check → `useQuery` in an `useAuth` composable;
   login/logout → mutations; keep the "loading starts true" cold-load behavior
   (its header documents why).
2. `dataset.ts` (181) — fetches → colada; store keeps active-dataset selection.
3. `insightsActivity.ts` (176) — hydrate calls → colada; keep localStorage
   seen-state (now via `useLocalStorage`).
4. `curation.ts` (270) — fetch → colada; keep WS-event merge logic in store
   (client state), test `upsertEntry` ordering while touching it (rule 2).
5. `insights.ts` (478) — fetches → colada; overlay reducer
   (`applyPatch`/`revertPatch`/`visibleRoots`) stays store-side as client
   state; add unit tests for the overlay pair being exact inverses (rule 2 —
   highest-value untested pure logic in the app).
6. **Fix the AppSidebar dual-ownership** (`AppSidebar.vue:27-39`): the source
   list becomes colada-only (components read the query); `source.ts` keeps
   selection only; delete `setSourcesData` and the query-fn side effects.
   Update `stores/source.ts` header (it currently documents the smell).

## Phase H — Nuxt UI sweep (forms, tables, duplicates)

1. 4 raw `<select>` → `USelect` (`InsightsPanel.vue:202`,
   `SchedulesPanel.vue:167`; the other two die with dead components in Phase C).
2. 7 raw `<input>` → `UInput` (`WebSourceSettings.vue:97`,
   `ConnectorSourceSettings.vue:178,228`, `NewSourcePage.vue:175,186`; others
   die in Phase C). Kills the 5× identical 130-char class string and the
   blue-focus-on-purple bug. Password-reveal (`ConnectorSourceSettings.vue:220-240`)
   → `UInput type="password"` + `#trailing` slot, `size="xs"`.
3. Duplicated inline-rename widget (`WebSourceSettings.vue:93-115` ≡
   `ConnectorSourceSettings.vue:174-197`) → one small component
   (`UFormField` + `UInput` + `UButtonGroup`).
4. Segmented-control `<button>` strips → `UTabs`/`UButtonGroup`
   (`InsightsPanel.vue:186-195,216-228`, plus `PivotTable.vue`,
   `ChartView.vue`, `WordsPanel.vue`, `TermInputBar.vue` where the pattern
   matches — judge per site; don't force it where the control is genuinely custom).
5. `BreakdownTable.vue` for the 4 surviving breakdown-table clones in
   `WebDashboard.vue:131-203` (~-80 lines).
6. Fix `FunctionBadge.vue:30` banned `size="sm"`; add `size` to the 5 `UButton`s
   missing it.
7. **Generated barrel**: make `types/generated/index.ts` actually generated —
   emit it from the file list in the pre-commit step that already runs
   `TS_RS_EXPORT_DIR` regen (a `ls`-derived barrel is "generated" under rule 3).
   Then delete the hand-mirrors: `SourceKind`/`SourceTable`/`UnifiedSource` in
   `types/index.ts:45-73` (use generated), `ToolId` becomes
   `SourceTool | 'settings'`; reconcile `types/enrichment.ts` against the 21
   now-exported generated types. Remove the third re-export layer in
   `services/api.ts:127,225-233`.

## Phase I — Conventions enforcement + `.vue` header backfill

1. Extend `scripts/check-conventions.sh` check 2 glob to
   `\.(ts|vue)$`. For `.vue`, require `/**` within the first ~5 lines (the file
   starts with `<script setup lang="ts">`; match the pattern of the 16 already-
   compliant files — verify their exact shape first and set the line window to
   fit). Update the script's header comment and CLAUDE.md's description.
2. Backfill headers on all remaining `.vue` files (~72 after Phase C deletions),
   in waves by directory (insights → enrich → topics → connect → analytics →
   command/query/rest), following the repo's backfill precedent: write *why*
   headers, never restate the filename.
3. **Verification pass** (the repo's own pattern): re-read every backfilled
   header against the code, fix overstatements — as a separate commit.
4. Same-pattern small wave for the two crates that never got verification:
   rewrite the one-liner WHAT docs in `brightflow-store` (5 of 8 files, incl.
   the 2,188-line `db.rs`) and `brightflow-connect/src/lib.rs` into genuine
   *why* headers.
5. Fix the two in-code rule violations found by the audit:
   `path_guard.rs:8` ("nothing downstream re-checks it" — absence assertion;
   restate as this file's own contract) and `scan.rs:248` caller observation;
   plus the cross-file claims in `auth/rate_limit.rs:109`, `auth/password.rs:5`,
   `auth/db.rs:6`, `enrichment/function.rs:5` (restate as own-file contracts or
   delete the pointers).

---

## Execution order & sizing

A → C → B → D → E → F → G → H → I. (C before B so the `.vue` file count and
frontend grep results are stable; I last so headers describe final code.)
Phases A–E are mechanical (biggest, lowest risk). F–H each carry one design
verification (marked ⚠). This is realistically 3–5 working sessions; each phase
is independently committable and shippable, so stopping between phases is safe.

## Verification

Per commit (enforced by pre-commit anyway): `./scripts/check-conventions.sh`,
`cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`,
`cargo test --workspace`, `npm run check`, `npm run test`.

Per phase, additionally:
- **B/D**: engine locked-value tests must pass unchanged (esp. after the `dot()`
  consolidation and fingerprint-adjacent edits); `cargo test -p brightflow-engine`.
- **D#9 (scope flatten)**: regenerate ts-rs (`TS_RS_EXPORT_DIR=... cargo test`),
  `git diff types/generated/` must show no shape changes; manifest tests pass.
- **F**: new integration test proves recluster logs an action row; manual smoke:
  run backend+frontend (`cargo run -- run-all` + `npm run dev` as background
  tasks per CLAUDE.md workflow), trigger Recluster from the UI, confirm it
  appears in the activity feed; confirm SetKpi still round-trips.
- **G**: manual smoke per migrated store's surface (login/logout, source list
  render, insights panel, curation queue); frontend log
  (`logs/frontend.log`) clean of console errors via `server.forwardConsole`.
- **H**: visual pass over touched forms/tables in the dev server; focus ring now
  purple.
- **Final**: `cargo build --release` (per CLAUDE.md), `./scripts/audit.sh`.

## Explicitly out of scope (deferred, per user)

- `longbow` path dependency (own session).
- Typed engine→api error boundary / anyhow→500 flattening (own session; only
  the mechanical `parts()` dedup lands here).
- Token/toast sweep (88 raw palette classes), CI setup, embedding backends,
  `store/db.rs` god-object decomposition, salt-cleanup wiring
  (`cleanup_old_salts` — flagged as follow-up), background jobs on the feed.
