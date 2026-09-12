# Plan: Explore reads the semantic layer, the bus can write it, and time fields bucket by period

**Status:** Proposal — decisions marked *confirmed* are settled with the author
on this date; *default* are my assumptions until reviewed.
**Date:** 2026-09-12
**Context:** Follows the three September reports
([`2026-09-08_litehouse-as-semantic-layer.md`](../reports/2026-09-08_litehouse-as-semantic-layer.md),
[`2026-09-08_data-platform-completeness-and-operations-language.md`](../reports/2026-09-08_data-platform-completeness-and-operations-language.md),
[`2026-09-12_tool-platform-or-workbench.md`](../reports/2026-09-12_tool-platform-or-workbench.md))
and a same-day discussion of what to build next. The author confirmed the
recommendation: give `column_semantics` its first consumer and its missing
write paths, and add date bucketing to Explore as the first derived-column
node of the operations language. Both are "consumer first, schema second"
moves; neither designs the modelling layer.

---

## Context

What the research found, verified against HEAD `6946e4d` on this date:

- **Semantics are stored and never read by Explore.** `column_semantics`
  (`crates/brightflow-store/migrations/005_column_semantics.sql`,
  `018_column_polarity.sql`) holds role, `is_kpi`, polarity, label and
  description per column. `POST .../tables/{name}/load` merges role, KPI,
  label and polarity into `ColumnInfo`
  (`crates/brightflow-api/src/analytics/handlers.rs:64-76`) but not
  description, because `ColumnInfo` has no such field
  (`analytics/session.rs:116-137`). On the frontend the only readers of
  `role`/`isKpi`/`label`/`polarity` are the two palette entries in
  `components/command/paletteActions.ts:375-443`, and those are offered on
  the insights route only (`composables/useCommandPalette.ts:184-191`).
  Explore's column list (`components/query/QueryBuilder.vue:45-52`), chips
  (`components/pivot/BucketDropzone.vue:187`), filter and sort selects,
  table headers and chart labels all show raw column names and include
  `ignored` columns. `stores/pivot.ts:91-103` picks the default aggregation
  from dtype alone.
- **No write path for role, label or description.** `ACTION_KINDS`
  (`crates/brightflow-api/src/actions/types.rs:481-563`) has `set_kpi` and
  `set_column_polarity` only. The `PUT .../semantics/{col}` route was
  removed in the 2026-08-03 simplification; the surviving GET routes have
  no frontend caller and no `services/api` module. After the GitHub seed
  (`state.rs:405-571`, keyed by bare table name, runs once when the table
  is empty) nobody can change a role.
- **First-touch writes silently declare a column a measure.**
  `SemanticSnapshot::default()` in `actions/exec/semantics.rs:97-106` sets
  `role: "measure"`, so the first `set_kpi` or `set_column_polarity` on an
  unseeded column writes that role whatever the engine detected. The
  `AppState.schemas` cache (`state.rs:34`) that could supply the detected
  role is declared and invalidated but never populated; `build_schema` is
  called from `insights/handlers.rs:322` only.
- **Enrichment materialises columns with no semantics.**
  `enrichment/runner.rs::materialize` writes `summary`, `language`,
  `category`, `subcategory`, `sentiment` (ticket classify), the four flag
  columns and `{function}__status`, then `replace_table_data`, then
  invalidates the schema cache. No `column_semantics` row is written, so
  Explore will treat `summary` as a dimension and `{function}__status` as a
  real column. `replace_table_data` (`store/src/ingest.rs:335-408`) is four
  separate DB calls, not one transaction, so a semantics write cannot join
  it today.
- **Role and polarity strings are still mapped in four places** (the
  2026-08-03 report's finding, line numbers now stale):
  `ColumnRole::parse` and its hand-written `Deserialize`
  (`engine/src/data/config.rs:26-59`), `role_str`
  (`api/src/analytics/handlers.rs:88-97`), `convert_semantic_row`
  (`api/src/state.rs:377-400`), and the actions crate's own
  `ColumnPolarity` enum mirroring the engine's `Polarity`
  (`actions/types.rs:162-179`). `ColumnRole` is not exported to TypeScript;
  role crosses the wire as a bare `string`.
- **Every time column in the product is a String.** Checked with a scratch
  Polars program against the committed test workspace: `orders.order_date`,
  `events.timestamp`, and every connector `created_at`/`updated_at` are
  `String`. Longbow infers Arrow types from JSON and keeps strings as Utf8
  (`../longbow/src/parquet.rs:75-97`); web analytics buckets by a 10-char
  string prefix (`web_analytics/queries.rs:98-117`); the engine parses
  strings in Rust (`engine/src/analysis/period.rs:249-279`). No Polars
  `dt()` call exists anywhere in `crates/brightflow-api`.
- **The operations language has no derived-column node.** `Operation`
  (`analytics/types.rs:35-72`) is filter / select / groupBy / pivot / sort /
  limit; `GroupBy.by` is plain column names and the executor
  (`analytics/executor.rs:90-94`) does no casting of keys. The frontend
  has no time-column concept: `utils/dtype.ts` normalises `datetime`/`date`
  to `'string'`.
- **Polars 0.48.1 has what bucketing needs**, already enabled through the
  default `temporal` feature: `str().to_date(StrptimeOptions)`,
  `dt().truncate(lit("1q"))` (quarter is a valid duration unit),
  `dt().strftime`, `dt().year()`, `dt().quarter()`, and `format_str`.
  `timezones` is off, which is fine for date-precision labels.
- **Period labels already have a shared vocabulary.** The engine emits
  `2024-03-01`, `2024-W11`, `2024-03`, `2024-Q1`, `2024`
  (`period.rs:155-168`, strings on purpose) and the frontend humanises them
  (`utils/format.ts::humanizePeriod`, `humanizePeriodShort`). Those labels
  sort lexicographically in chronological order within a granularity.

Binding repo rules: pure logic gets co-located tests (rule 2); Axum
handlers and `.vue` files are carve-outs; module headers state this file's
contract, not observations about others (rule 1); this file is dated prose
(rule 3). `cargo build --release` closes every Rust step.

## Decisions reached

- **Confirmed — this is the next piece of work**, chosen over saved views,
  quality findings, connectors, and the full modelling layer.
- **Confirmed — consumer first.** Explore reads what is already stored
  before anything new is stored. §0 and §3 land before §5.
- **Default — one wire vocabulary for role, polarity and granularity.** The
  engine's `ColumnRole`, `Polarity` and `TimeGranularity` gain `TS` and
  `JsonSchema` derives and become the types used by `ColumnInfo`, the
  action payloads and the period operation. `role_str` and the actions
  crate's `ColumnPolarity` are deleted. This adds `schemars` to the engine
  crate; it already carries `ts-rs` for the same reason.
- **Default — three new action kinds, not one.** `set_column_role`,
  `set_column_label`, `set_column_description`, each mirroring
  `set_column_polarity` end to end (variant, registry tuple, dispatch arm,
  executor, undo). One kind per field keeps the LLM manifest honest and the
  undo payload small. A single shared `UndoOp::RestoreColumnSemantic`
  carrying the previous `SemanticSnapshot` serves all three; the existing
  `RestoreKpi`/`RestorePolarity` variants stay because persisted
  `undo_json` rows reference them.
- **Default — the first write to a column inherits the detected role.**
  `SemanticSnapshot::default()` stops saying `measure`; the executor asks
  `detect_schema` over the table's first parquet file for that column's
  role and uses that, falling back to `dimension` for strings and `measure`
  for numerics only if detection fails. The never-populated
  `AppState.schemas` field is removed in the same commit.
- **Default — enrichment functions declare their output semantics next to
  their output columns**, in the engine, as data. Materialisation writes
  those rows *only where no row exists*, so a user's later edit to a label
  survives a re-run. Written after `replace_table_data`, not inside it; the
  window between the two is accepted and documented in the runner header.
- **Default — `ignored` columns are hidden in Explore**, not greyed. Making
  a column visible again is a role change through the bus (§4). The
  `{function}__status` column is the first thing this hides.
- **Default — default aggregation follows role, then dtype.** `measure` →
  `sum`; `dimension`, `entity`, `time` → `count`; no role → today's dtype
  rule. A numeric column the engine detected as a dimension (few distinct
  values) stops defaulting to `sum`.
- **Default — the derived-column node is `withColumns` with a one-variant
  expression tree**, `{ fn: "period", column, granularity }`, named
  `period` rather than `truncate` because its output is the product's
  period *label* shared with the engine, not a Polars datetime. It is the
  seed the completeness report asks for (§4b item 2) and stays a
  serialisable data structure; no expression strings.
- **Default — period output is a String label**, identical to the engine's
  format, so `humanizePeriod` applies, lexicographic sort is chronological,
  and Explore and the insights feed name the same week the same way.
- **Default — String time columns are parsed as ISO dates**: first ten
  characters through `to_date("%Y-%m-%d")`, non-strict, nulls on failure.
  This covers every column the product produces today. Non-ISO CSV dates
  bucket to null and are a typing-at-ingest problem, listed below.
- **Default — the granularity control appears on a field chip only when
  the column is a time column**: role `time`, or dtype `date`/`datetime`.
  A String date column becomes bucketable by setting its role to `time`,
  which is the semantic layer earning its keep. Default granularity is the
  table's `time_granularity` setting when present, else `week` (the
  engine's default).
- **Default — time fields sort by label ascending** when added, overriding
  the value-descending default from the pivot-chart plan; chronological is
  what a time axis means.

## §0 — One vocabulary on the wire (1 commit, Rust)

`crates/brightflow-engine/src/data/config.rs`: derive `TS` (`#[ts(export)]`)
and `JsonSchema` on `ColumnRole`, `Polarity`, `TimeGranularity`. Add
`ColumnRole::as_str()` beside `Polarity::as_str()`. The hand-written
`Deserialize` on `ColumnRole` (legacy `kpi`/`metric`) stays; the schema
describes the five canonical names, and the module header says why the two
differ. Add `schemars = "1"` to the engine's `Cargo.toml`.

`crates/brightflow-api/src/analytics/session.rs`: `ColumnInfo.role` becomes
`Option<ColumnRole>`, `polarity` becomes `Option<Polarity>`, and a new
`description: Option<String>` joins them with the same `skip_serializing_if`
and `#[ts(optional)]` treatment. `analytics/handlers.rs::load_table` copies
`description` from the override and drops `role_str`.
`LoadTableResponse` gains `time_granularity: Option<TimeGranularity>` read
from `state.settings_overrides`, so the frontend learns the table's bucket
default without a new API module.

`crates/brightflow-api/src/actions/types.rs`: delete `ColumnPolarity`;
`SetColumnPolarity.polarity` is `Polarity`. `exec/semantics.rs` and the
manifest tests follow. `state.rs::convert_semantic_row` keeps using
`ColumnRole::parse` / `Polarity::parse`, now the only string→enum sites.

Tests: `config.rs` gets a serde round-trip per enum pinning the wire names
(`measure`, `higher_is_better`, `week`); the existing `actions/types.rs`
manifest tests must still pass with `Polarity` in the sample. Regenerate
TypeScript (`cargo test --workspace` with `TS_RS_EXPORT_DIR`, then
`scripts/generate-ts-barrel.sh`); `ColumnPolarity.ts` disappears,
`ColumnRole.ts`, `Polarity.ts`, `TimeGranularity.ts` appear. Frontend
`results.test.ts:13` literal gains nothing (all optional).

## §1 — Write paths on the bus (2 commits, Rust)

### 1a. Detected role as the default

`actions/exec/semantics.rs`: replace `SemanticSnapshot::default()` with
`SemanticSnapshot::detected(state, source_id, table, column)`: scan the
table's first parquet file's schema, run the engine's `detect_schema` over
an empty frame of that schema (or the existing DataFrame path if the
detector needs values — check `detect_schema`'s cardinality branch and use
`scan_parquet(...).limit(N)` if so), and take the column's role; fall back
to `dimension` for `String`/`Boolean`, `measure` for numerics, `time` for
temporal dtypes. Remove `AppState.schemas` and the `remove` in
`invalidate_schema_cache` (`state.rs:34,194`); its name now describes only
`schema_overrides`. Test in the `semantics.rs` tests module: an unseeded
string column touched by `set_kpi` lands as `dimension`, not `measure`.

### 1b. Three kinds

`actions/types.rs`: variants

```rust
SetColumnRole        { scope, column, role: ColumnRole }
SetColumnLabel       { scope, column, label: Option<String> }        // None clears
SetColumnDescription { scope, column, description: Option<String> }  // None clears
```

with doc comments written for the LLM manifest (the registry's third
tuple field), `kind()`/`scope()` arms, three `ACTION_KINDS` tuples (all
undoable), and `UndoOp::RestoreColumnSemantic { source_id, table, column,
snapshot: SemanticSnapshot }` (`SemanticSnapshot` gains `Serialize`,
`Deserialize`). `handlers.rs::execute_action` and the undo match gain three
arms each. `exec/semantics.rs` gains `execute_set_column_role` /
`_label` / `_description`, each a closure over `upsert_semantic_preserving`,
and one `undo_restore_column_semantic` that writes the snapshot back.
Labels are trimmed; an empty label is `None`. A role change on a column
that is currently a KPI with role ≠ `measure` clears `is_kpi` and says so
in the result JSON.

Tests: `manifest_registry_is_complete` gets the three samples; `semantics.rs`
tests cover set → undo restores the full snapshot, clear-label writes
`NULL`, and role change clears KPI. Integration:
`crates/brightflow-api/tests/action_log.rs` gets one dispatch-then-undo
round trip for `set_column_role` against the test workspace.

## §2 — Enrichment declares its output semantics (1 commit, Rust)

`crates/brightflow-engine/src/enrichment/ticket_classify.rs` and
`mentions.rs`: next to `OUTPUT_COLUMNS` / `FLAG_COLUMNS`, a
`pub const OUTPUT_SEMANTICS: &[OutputSemantic]` with
`OutputSemantic { name, role: ColumnRole, label, description }` (type in
`enrichment/mod.rs`):

| Column | Role | Label |
|---|---|---|
| `summary` | ignored | Summary |
| `language` | dimension | Language |
| `category` | dimension | Category |
| `subcategory` | dimension | Subcategory |
| `sentiment` | dimension | Sentiment |
| `has_feedback` | dimension | Has feedback |
| `has_incidental_feedback` | dimension | Has incidental feedback |
| `has_competitor_mention` | dimension | Mentions a competitor |
| `mention_count` | measure | Mention count |

Descriptions are one sentence each, taken from the definitions the prompts
already carry (sentiment's four values, the flag definitions), so the
vocabulary block and the column description cannot disagree. A unit test
asserts every `OUTPUT_COLUMNS` name has exactly one `OUTPUT_SEMANTICS`
entry and vice versa.

`crates/brightflow-store/src/db/catalog.rs`: `insert_column_semantics_if_absent(table_id, rows)`
— `INSERT ... ON CONFLICT DO NOTHING` in one transaction, returning the
number inserted; facade on `ParquetStore`. Unit test in the store's
existing catalog tests: second call inserts zero, an edited label survives.

`crates/brightflow-api/src/enrichment/runner.rs::materialize`: after
`replace_table_data` succeeds and before the schema-cache invalidation,
build rows from `RunSpec`'s `OUTPUT_SEMANTICS` plus
`{function}__status` → `ignored`, call the new store method, then also
refresh `state.schema_overrides` for the table (the same block
`exec/semantics.rs:154-174` uses; extract it into one `refresh_overrides`
on `AppState` so it stops being copied). The runner header records that
semantics are written after the data, not with it. The integration test
`tests/enrichment_materialize.rs` asserts the rows exist after a
materialise and that a pre-set label is untouched by a second one.

## §3 — Explore reads the semantic layer (2 commits, frontend)

### 3a. Store and pure helpers

`stores/dataset.ts` (new `dataset.test.ts` beside it, `setActivePinia`
pattern from `pivot.test.ts`):

- `visibleColumns` — `columns` minus `role === 'ignored'`.
- `timeColumns` — `role === 'time'` or `isTemporalDtype(dtype)`.
- `columnByName(name)`, `labelFor(name)` — label, else `humanizeColumn(name)`
  (already in `utils/format.ts`, unused by Explore today), else the name.
- `timeGranularity` — from the load response, default `'week'`.
- `applySemanticAction(entry: ActionLogEntry)` — patches one column's
  role / label / description / isKpi / polarity from an applied
  `set_column_*` or `set_kpi` entry whose scope matches the loaded table;
  ignores everything else. Pure over the store's state; tested per kind.

`utils/dtype.ts`: `isTemporalDtype` for `date`, `datetime`; `normalizeDtype`
unchanged (the frontend still has no temporal operator set; out of scope).
`stores/pivot.ts`: `PivotField` gains `role?: ColumnRole`;
`addValueField(column, dtype, role, aggregation?)` applies the role rule
from *Decisions*; `addRowField`/`addColumnField` carry role through.
`pivot.test.ts` gains: measure defaults to `sum`, numeric dimension to
`count`, roleless numeric to `sum`.

### 3b. Components

- `QueryBuilder.vue`: column list from `visibleColumns`; label from
  `labelFor`; `description` as `UTooltip`; icon by role first
  (`time` → `i-lucide-calendar`, `measure` → `i-lucide-hash`,
  `dimension` → `i-lucide-tag`, `entity` → `i-lucide-user`), dtype icon as
  fallback; a small `KPI` badge when `isKpi`. Sort-by select uses labels.
- `BucketDropzone.vue`: chip text from `labelFor`; the values chip shows a
  polarity arrow (`↑` higher-is-better, `↓` lower-is-better) after the
  aggregation select when set.
- `FilterBar.vue`: column select shows labels, values stay raw names.
- `DataTable.vue` and `PivotTable.vue`: header text through `labelFor`;
  aggregate columns (`sum`, `count`) keep their alias.
- `ChartView.vue`: axis name and series names through `labelFor`.
- `ExploreTool.vue`: subscribes to `actionEvent` via the existing
  `stores/curation.ts` realtime hook and calls
  `datasetStore.applySemanticAction` — the dataset store is patched from
  the bus rather than reloaded.

Carve-outs on the `.vue` files; the store tests carry the logic.

## §4 — Editing semantics from Explore (1 commit, frontend)

Two entry points, both dispatching through `useInsightActions.dispatchWithFeedback`
so the toast-with-Undo flow is unchanged:

- **Context menu on a column item** in `QueryBuilder.vue`'s column list,
  using `UContextMenu` as `DataTable.vue:107-130` already does: *Role ▸*
  (five radio-style entries, current one checked), *Set / Unset KPI*
  (measures only), *Polarity ▸* (measures only), *Rename…*, *Describe…*.
  Rename and Describe open one small `UModal` with a `UInput`/`UTextarea`,
  empty submit clears. A *Show ignored columns* toggle at the bottom of the
  list reveals hidden columns greyed, so an ignored column can be reached
  to un-ignore it.
- **Command palette on the explore route**: `paletteActions.ts` gains
  `COLUMN_KINDS` (`set_column_role`, `set_column_label`,
  `set_column_description`, `set_kpi`, `set_column_polarity`);
  `useCommandPalette.ts` offers `COLUMN_KINDS` on explore and insights
  routes. The two existing entries drop their `role == null || role ===
  'measure'` filter for role, and keep it for KPI and polarity.

The pure part, `components/command/paletteActions.ts`'s entry builders for
the three new kinds, gets tests in the existing pattern for that file.

## §5 — `withColumns` and the `period` expression (1 commit, Rust)

`crates/brightflow-api/src/analytics/types.rs`:

```rust
Operation::WithColumns { columns: Vec<DerivedColumn> }
struct DerivedColumn { name: String, expr: DerivedExpr }
#[serde(tag = "fn", rename_all = "camelCase")]
enum DerivedExpr { Period { column: String, granularity: TimeGranularity } }
```

All `#[ts(export)]`, `JsonSchema` not needed yet. The module doc states the
contract: derived expressions are a typed tree, each variant names what it
computes, and the executor is the only compiler.

`analytics/executor.rs`: `apply_operation` arm for `WithColumns` calls
`lf.collect_schema()` once, then per column builds `period_expr(col, dtype,
granularity)`:

1. to a `Date` expression: `String` → `col.str().slice(0, 10).str().to_date(StrptimeOptions { format: Some("%Y-%m-%d"), strict: false, exact: true, cache: true })`;
   `Date` → as is; `Datetime` → `.cast(Date)`; anything else →
   `AppError::BadRequest` naming the column and dtype.
2. to a label: `day` → `strftime("%Y-%m-%d")`; `week` → `strftime("%G-W%V")`
   (ISO year and week, matching `period.rs::format_period`); `month` →
   `strftime("%Y-%m")`; `quarter` → `format_str("{}-Q{}", [year(), quarter()])`;
   `year` → `strftime("%Y")`. Aliased to `name`.

A `WithColumns` before a `GroupBy`/`Pivot` on `name` is then ordinary; the
executor needs no other change. `dtype_to_string` is unchanged.

Tests in the executor's `tests` module (inline `df!` as today, first
temporal fixtures in the file): a String column of ISO datetimes bucketed
at each of the five granularities yields the engine's labels for known
dates including a year-boundary ISO week (`2024-12-30` → `2025-W01`); a
`Date` column gives identical labels; an unparsable string yields null; a
numeric column errors. One test pins the engine agreement directly: run
`period.rs::format_period` on the same dates and compare. Regenerate
TypeScript.

## §6 — Granularity on time-field chips (1 commit, frontend)

- `PivotField` gains `granularity?: TimeGranularity`; `addRowField` /
  `addColumnField` set it to `datasetStore.timeGranularity` when the column
  is a time column, and set `sort` to `{ by: 'label', descending: false }`
  for those fields. `setFieldGranularity(id, g)`. Tests in `pivot.test.ts`.
- `BucketDropzone.vue`: a granularity `USelectMenu` (day / week / month /
  quarter / year) on row and column chips that have one, next to the sort
  select; chip text becomes `label · month`.
- `composables/useWsQuery.ts::buildOperations`: for every row/column field
  with a granularity, push one `withColumns` operation (all derived
  columns in one op) *before* the groupBy/pivot, and reference the derived
  name (`{column}__{granularity}`) in `by` / `index` / `columns`. Extract
  the operation building into a pure `utils/buildOperations.ts` with a
  test file; the composable calls it. Tests: rows-only with a month
  bucket, pivot with a bucketed column field, a field without granularity
  emits no `withColumns`.
- `PivotTable.vue`: index cells from a bucketed field render the raw label
  (compact, sortable). `ChartView.vue`: category axis labels for a
  bucketed X go through `humanizePeriodShort`; tooltip through
  `humanizePeriod`. `pivotOrder.ts` needs no change: label sort on ISO
  labels is chronological.

## Execution order & sizing

| Step | Commits | Depends on | Note |
|---|---|---|---|
| §0 | 1 | — | Rust; one enum vocabulary, `description` and `timeGranularity` on the wire |
| §1 | 2 | §0 | detected-role default; three action kinds |
| §2 | 1 | §0 | enrichment writes semantics, insert-if-absent |
| §3 | 2 | §0 | Explore reads; the first consumer |
| §4 | 1 | §1, §3 | context menu and palette editing |
| §5 | 1 | §0 | `withColumns` / `period` in the executor |
| §6 | 1 | §3, §5 | granularity chips, chart labels |

§1 and §2 can land in either order after §0; §5 can land any time after
§0. Each Rust commit: `cargo fmt --check`, `cargo clippy`,
`cargo test -p <crate>`, `cargo build --release`, TypeScript regenerated
and formatted. Each frontend commit: `npm run check`, `npm run test`.
`./scripts/check-conventions.sh` runs from the pre-commit hook. Where the
committed test workspace changes shape (new semantics rows after §2), run
`./scripts/build-test-template.sh --check` and rebuild if it drifts.

## Verification

- **Rust unit** — enum wire names (§0); detected-role default, set/undo
  snapshots, KPI cleared on role change (§1); output-semantics coverage,
  insert-if-absent (§2); five granularities on String and Date, ISO-week
  year boundary, engine label agreement, null and error cases (§5).
- **Rust integration** — `action_log.rs` round trip for `set_column_role`;
  `enrichment_materialize.rs` semantics rows present and label preserved.
- **Frontend unit** — `dataset.test.ts` (visible, time, labelFor,
  applySemanticAction per kind), `pivot.test.ts` (role-based default agg,
  granularity default and sort), `buildOperations.test.ts`,
  `paletteActions` entries.
- **Frontend check** — `npm run check`; integration tier
  (`TEST_INTEGRATION=1`) still green against the rebuilt workspace.
- **Manual, both servers as background tasks, on the test workspace:**
  1. Explore on `issues`: `{function}__status` and `summary` are gone
     from the column list; `Created at` reads as a label with a calendar
     icon; hover shows the description on enriched columns.
  2. Right-click `order_date` on `orders`, set role → time: toast with
     Undo, chip icon changes without reload, granularity select appears
     when the column is dropped into Rows. Undo from the toast reverts
     the icon.
  3. Rows = `order_date` · month, Values = revenue: table rows read
     `2026-01`, `2026-02`… ascending; chart X axis reads `Jan 26`; switch
     to quarter and week and confirm `2026-Q1`, `2026-W03`, matching what
     the insights feed calls the same weeks.
  4. Rows = `category`, Columns = `created_at` · year on `issues`: the
     wide pivot spreads years across.
  5. Add `units` to Values on `orders`: defaults to `sum`. Set its role to
     dimension, drag it again: defaults to `count`.
  6. Rename `revenue` to "Revenue (SEK)": every header, chip and the chart
     legend update; Activity page shows the action; undo restores.
  7. Run classify on `issues` in Text enrichment, return to Explore:
     `Category`, `Subcategory`, `Sentiment` appear with labels and
     descriptions without a reload.

## Out of scope

- **Labels and descriptions inside the engine.** `build_schema` still
  drops them and insight headlines still `humanize_column`. A separate
  small change once `DataSchema` has a place for them.
- **Named measures and relationships tables** (semantic-layer report §8
  steps 3–4). `withColumns` and the write paths are their prerequisites,
  not their first rows.
- **Typing dates at ingest.** Longbow, CSV upload and the event buffer all
  land time as `String`. The right fix is inference or a `@longbow`
  declaration at the door, which would let `period` drop its string
  branch; it changes the store's column stats and belongs to Longbow.
- **Temporal filter operators** (`before`, `after`, `between`) in
  `useOperators.ts`; date columns keep string operators for now.
- **A second derived expression.** `Period` is deliberately alone;
  arithmetic, case-when and coalesce arrive with the modelling plan.
- **LLM context bundle** from semantics (report §8 step 5) — the three
  prompt paths stay as they are.
- **Table settings UI** for `time_granularity` and `display_name`. The
  PUT route exists; a settings pane can follow once §6 shows whether users
  reach for a per-table default at all.
- **Generated Ossie export.**

## Open questions

1. Should `entity` columns be offered in Row/Column buckets? The engine
   treats entity and ignored alike; Explore under this plan shows entities
   as pickable dimensions with a user icon. (Default: show them.)
2. Derived column name on the wire: `{column}__{granularity}` (stable,
   ugly, never shown) versus letting the client pick. (Default: the client
   sends the name; the backend only requires uniqueness.)
3. ISO week versus locale week for `week`. The engine uses ISO; Excel and
   most users in Sweden and Finland expect ISO; US users would not.
   (Default: ISO, matching the engine, no setting.)
4. Should `set_column_role` to `ignored` on a column currently used in the
   pivot remove it from the buckets, or leave the query running on a
   hidden column? (Default: leave the query; the chip stays until removed.)
