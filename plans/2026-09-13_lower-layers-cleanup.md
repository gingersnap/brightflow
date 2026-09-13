# Plan: one path each — cleaning up the types crate, Litehouse, the connector seam and the engine's schema after the semantics migration

**Status:** Proposal — decisions marked *confirmed* are settled with the author
on this date; *default* are my assumptions until reviewed.
**Date:** 2026-09-13
**Baseline:** HEAD `db0c1ab`. Longbow at `../longbow` `da71b00`.
**Context:** [`plans/2026-09-12_types-crate-and-source-declared-semantics.md`](2026-09-12_types-crate-and-source-declared-semantics.md)
landed as `51ce4b5..2652472`, followed by five feature commits
(`dad6d68..7ba1c9e`: prompt context, polarity and metrics in scoring,
temporal filters, reset-to-declared and upgrade diffs, Ossie import). That
work added a second way of doing several things without removing the first,
which is normal mid-migration and wrong to leave. This plan is the pass that
removes the first way. It is deliberately bounded: every step is a deletion
or a merge of two paths into one, and no step adds an abstraction. The next
consumer, an agent that writes the `agent` layer, needs the prompt context,
the action bus and the resolved reads, and no step here touches those.

Research: five grep passes over HEAD for callers of every public function in
the store, the types crate, the engine and the API (`handlers.rs` and
`routes.rs` excluded), plus the same over Longbow; a scan of the two real
workspaces' semantic rows; a count of facade bypasses. Numbers below are from
those passes. No code changed.

---

## Findings

### F1. Detection runs twice, on different samples

`semantics::detect::declare_detected` runs `detected_declaration` over a
10 000-row sample and stores a `detected` layer. At insight time
`insights::handlers::run_report_core` calls `build_schema`, which runs
`detect_schema` over the *whole* frame and then overlays the resolved rows
(`crates/brightflow-engine/src/data/merge.rs:41-44`). Two detections, two
samples, and for a column with no stored row the frame-time guess wins
silently. `detect_schema` has 14 other callers: the CLI `insights` command,
which has no store, and eleven engine tests.

### F2. Two read paths for the same semantics

`AppState.schema_overrides` and `settings_overrides` are in-memory copies of
what the store resolves, refreshed by six writers (`refresh_overrides_from_store`
call sites: `actions/exec/semantics.rs` ×7, `enrichment/runner.rs` ×2,
`analytics/handlers.rs`, `bootstrap.rs`, `enrichment/handlers.rs`,
`semantics/handlers.rs`). `load_table` already reads the store directly; the
insights path (`resolve_dataset`) still reads the caches. `ColumnOverride`
and `TableSettingsOverride` (`merge.rs`) exist only to carry cache contents
into the engine; they are projections of `ResolvedColumn` and
`ResolvedTable`. `set_column_override` has no caller left (one doc mention);
`remove_column_override` has one.

### F3. Four readers of `schema_json`, two shapes each

`shared::schema_has_text_column`, `enrichment::validate::table_columns`,
`enrichment::handlers::table_columns` and the store's `schema_column_names`
each parse `schema_json`, each accepting the pre-027 `{fields:[{name,type}]}`
shape and the contract's `{columns:[{name,datatype}]}`. The frontend's
`FunctionCard.vue` is a fifth (`schema?.columns ?? schema?.fields`). The
contract already has `TableSchema`; nothing calls it for this.

### F4. Declaration fields the store never applies

`TableDeclaration` carries `schema`, `primary_key`, `unique_keys` and
`cursor_field`. `apply_declaration` reads none of them. `primary_key` and
`unique_keys` are copies of `Dataset.primary_key` / `Dataset.unique_keys`
(`import.rs:57-59`, `from_endpoint_json` sets the dataset's); the store's
`tables.primary_keys` is set by `merge_parquet` from the endpoint's own key,
which reaches the scheduler on `EndpointResultInfo`, not on the declaration.
`schema` is set only in a test. `cursor_field` is ingestion state and lives
on the endpoint result too.

### F5. `EndpointResultInfo` copies seven of Longbow's eight fields

`brightflow-connect` mirrors `longbow::pipeline::EndpointResult` field for
field and adds one of its own: the *typed* `declaration`. A test pins the
copy (`map_run_result_translates_every_field_verbatim`). This is not dead
code — the typed field is the point of the crate — but the seven copied
fields and their mapping are.

### F6. `OutputSemantic` is a `Field` with another name

`brightflow_engine::enrichment::OutputSemantic {name, datatype, role, label,
description}` is converted to a `Field` on every materialisation
(`runner.rs::declare_output_semantics`). The contract type covers it.

### F7. Public functions nothing calls

From the caller scan, with same-file uses counted:

| Where | Item | Uses | Note |
|---|---|---|---|
| `api/state.rs` | `set_column_override` | 0 | superseded by `refresh_overrides_from_store` |
| `engine/data/schema.rs` | `DataSchema::label_for` | 0 | `AnalysisTree::label` does this |
| `engine/analysis/tree.rs` | `AnalysisType::natural_summary` | 0 | only `natural_summary_with` is called |
| `api/analytics/session.rs` | `DatasetManager::has_dataset` | 3, all tests | |
| `types/datatype.rs` | `from_arrow`, `from_polars_field`, `to_polars`, `is_numeric` | 0 outside file | library surface; see D4 |
| `types/ext.rs` | `MetricExpr::with_filter` | 0 outside file | |
| `types/semantic.rs` | `extension_json`, `from_json` | 0 outside file | |

Longbow has no unused public function. The frontend has no unused export in
`utils/`, `composables/` or `services/api/`.

### F8. Store re-exports nobody outside the store uses

Fifteen names in `brightflow_store`'s `pub use` list have zero users outside
the crate: `AppliedDeclaration`, `ColumnSemanticRow`, `DeclarationChangeRow`,
`EnrichmentFunctionVersionRow`, `FileColumnStatRow`, `FromRow`,
`InsightStateRow`, `InsightSuppressionRow`, `MergeMetrics`, `MetricRow`,
`RelationshipRow`, `SourceRow`, `StoredMetric`, `StoredRelationship`,
`TableSemanticsRow`.

### F9. The facade is a thin shell over `db()`

Callers reach through `store.db()` 71 times in the API and scheduler
(`get_table` ×12, `write_column_opinion` ×4, …) and 583 times across the
workspace. `ParquetStore`'s own methods that touch Parquet (`read_table`,
`ingest_parquet`, `merge_parquet`, `replace_table_data`) are used; its
name-resolving semantic wrappers (`resolved_columns` ×18, `apply_declaration`
×23) are used; four methods have one caller each (`register_file`,
`register_existing_events`, `root_path`, `scan_table`). The facade is not
the API; `db()` is. That is fine, but it should be said once instead of
implied by a `pub fn db()` with a comment about seeding.

### F10. Small duplications

- `push_fmt` exists three times (`engine/enrichment/ticket_classify.rs`,
  `engine/enrichment/mentions.rs`, `api/semantics/prompt.rs`).
- The frontend keeps two lists of the column-semantic action kinds:
  `SEMANTIC_KINDS` in `stores/dataset.ts` and `COLUMN_KINDS` in
  `paletteActions.ts`; they drifted already (`reset_column_semantics` and
  `set_table_settings` are in one and not the other, correctly, but nothing
  enforces which list means what).
- `DocDisplay::infer` guesses doc columns from names at read time; the
  detector could declare them once at detection time, which is the same
  guess in a producer's place.

### F11. Legacy rows and legacy code

The `default` workspace has 61 `column_semantics` rows under
`producer='legacy'`, `layer='declared'` (the migrated GitHub seed and
enrichment rows); the `test` workspace has none. Neither workspace has an
`action_log` row whose undo is `RestoreKpi` or `RestorePolarity`, so the
legacy undo arms in `actions/exec/semantics.rs` (`undo_restore_kpi`,
`undo_restore_polarity`, `LEGACY_PRODUCER`, `legacy_provenance`) guard rows
that do not exist. The `sources` table is empty in both workspaces (no sync
has run since the producer columns were added) and its `kind` CHECK admits
`web`, which nothing registers.

---

## Decisions reached

- **D1 (confirmed, this discussion): a bounded pass, before the agent
  producer.** Two days. One commit per finding. Each change is a deletion or
  a merge of two paths into one; no new abstraction. The existing tests are
  the guardrail; a step that needs a new test to prove the merged path
  behaves as both old paths did writes that test first.
- **D2 (default): the store's resolved rows are the engine's schema source.**
  When a table has semantic rows, `build_schema` places columns from them
  and runs detection only for columns no row mentions. `detect_schema` stays
  for the CLI and for tables with no rows at all. This makes the detected
  layer the one detection, and makes the 10 000-row sample the sample.
- **D3 (default): `db()` is the catalog API.** The facade keeps the methods
  that touch Parquet files and the name-resolving semantic wrappers; the
  four one-caller methods are inlined or moved next to their caller. The
  module doc says this, so nobody adds a wrapper out of politeness.
- **D4 (default): the contract crate keeps its conversion surface.**
  `from_arrow`, `to_polars`, `from_polars_field` and `is_numeric` are the
  library's API and are covered by its own tests; they stay. `with_filter`,
  `extension_json` and `from_json` are checked individually in S9.
- **D5 (default): legacy undo arms go now.** No workspace has a row they
  would serve. Removing them also removes `LEGACY_PRODUCER`. The 61 `legacy`
  declaration rows in `default` are left for the next GitHub sync to
  supersede; a one-shot delete is offered as an optional CLI step, not run
  by a migration, because the rows are correct semantics with a stale
  producer, not garbage.
- **D6 (default): `schema_json` becomes the contract's `TableSchema` on
  disk.** Migration 029 rewrites pre-027 rows into the `columns` shape; the
  four readers become one `TableSchema` parse. Old shape support goes.
- **D7 (default): `TableDeclaration` loses what the store ignores.**
  `schema`, `primary_key`, `unique_keys` and `cursor_field` are removed;
  readers use `dataset.primary_key` / `dataset.unique_keys`, and ingestion
  state stays on the endpoint result. `validate()` keeps checking the
  dataset's keys against its fields.

---

## Steps

Each step is one commit with the verification in *Verification* run before
it. Order matters where noted.

### S1. One detection (F1) — engine, API

- `build_schema(df, resolved: &[ResolvedColumn], table: Option<&ResolvedTable>, metrics)`
  replaces the `overrides`/`settings` parameters. When `resolved` is
  non-empty, columns with a resolved role are placed from it; columns with
  no row go through the detector's per-column rule (factor the per-column
  arm of `detect_schema` into `detect_column(name, series, row_count)` so
  both callers share it); the time axis is the resolved `time` column,
  else the detector's. When `resolved` is empty the whole of
  `detect_schema` runs, as today.
- `insights::handlers::resolve_dataset` returns `store.resolved_columns` and
  `store.resolved_table` instead of the cache contents.
- Tests: `merge.rs` gains a test that a resolved `dimension` role on a
  numeric column wins over detection and that an unmentioned column is
  detected; the engine's existing tests keep `detect_schema`.

### S2. One read path (F2) — API

Depends on S1.

- Delete `schema_overrides`, `settings_overrides`,
  `load_overrides_from_store`, `refresh_overrides_from_store`,
  `set_column_override`, `remove_column_override`, `override_from_resolved`,
  `settings_from_resolved` from `state.rs`; delete `ColumnOverride` and
  `TableSettingsOverride` from `merge.rs`.
- The six refresh call sites go. `enrichment/handlers.rs` stops removing a
  cache entry when it drops a column.
- `bootstrap.rs` stops calling `load_overrides_from_store` at boot.
- Tests that assert on `state.schema_overrides` (`action_log.rs` ×3,
  `enrichment_materialize.rs`, `semantic_model.rs`) assert on
  `store.resolved_columns` instead — most already do both.

### S3. One `schema_json` shape (F3) — store, API, frontend

- Migration `029_schema_json_columns`: for every `tables` row whose
  `schema_json` has a top-level `fields` array, rewrite it as
  `TableSchema { columns: [{ name, datatype: LogicalType::from the old
  Polars spelling, physical }] }`. The mapping from the old `type` strings
  (`str`, `i64`, `f64`, `datetime[μs]`, …) is `LogicalType::from_polars`
  over `DataType`'s `Display` names; unknowns become `Opaque` with the
  string kept as `physical`.
- `TableRow::schema() -> Option<TableSchema>` in the store; the four Rust
  readers call it; `schema_has_text_column` becomes
  `schema.columns.iter().any(|c| c.datatype == LogicalType::String)`.
- `FunctionCard.vue` reads `columns` only. `TableInfo.schema` is typed
  `TableSchema` in `ts-rs` output instead of `unknown`.
- Test: the migration over a fixture row in each shape.

### S4. Declaration fields the store ignores (F4) — types, connect, import

- Remove `schema`, `primary_key`, `unique_keys`, `cursor_field` from
  `TableDeclaration`. `from_endpoint_json(name, source, primary_key, value)`
  still takes the endpoint's key to set `dataset.primary_key`; `cursor_field`
  is dropped from its signature. `connect::parse_declaration` and
  `import::declarations_of` follow.
- Tests in `declaration.rs`, `connect/lib.rs` and `import.rs` that assert on
  the removed fields assert on `dataset.primary_key`.

### S5. The connect result type (F5) — connect, scheduler

- `EndpointResultInfo { inner: longbow::pipeline::EndpointResult,
  declaration: Option<TableDeclaration> }` with `Deref` to `inner`, or the
  seven fields kept and the type renamed to say it is a translation. Default:
  the wrapper. `map_run_result_translates_every_field_verbatim` becomes a
  test of the one field the crate adds.
- Scheduler call sites (`ep_result.name` etc.) compile unchanged through
  `Deref`; the test literal in `scheduler/lib.rs:647` builds the wrapper.

### S6. `OutputSemantic` becomes `Field` (F6) — engine, API

- `OUTPUT_SEMANTICS` and `FLAG_SEMANTICS` become `fn output_fields() ->
  Vec<Field>` built with the contract's builders (`Field::column(..)
  .with_datatype(..).with_description(..).with_brightflow(..)`); the label
  rides in `ColumnExt`. `RunSpec::output_semantics` returns `&[Field]`
  (cached in a `LazyLock`, since `Field` is not `const`).
- `declare_output_semantics` pushes them straight into the dataset.

### S7. Dead functions and narrowed visibility (F7, F8, F9)

- Delete `set_column_override` (already gone in S2), `DataSchema::label_for`,
  `AnalysisType::natural_summary` (rename `natural_summary_with` to
  `natural_summary`).
- `has_dataset` becomes `#[cfg(test)]`.
- The fifteen store re-exports with no external user are removed from
  `lib.rs`; `models.rs` types used only inside the store become
  `pub(crate)`. `db/mod.rs`'s module doc states D3; `ParquetStore::db`'s
  comment stops talking about seeding.
- `register_existing_events`, `register_file`, `scan_table`, `root_path`:
  keep those the CLI or ingest flush call (`register_file` from flush,
  `register_existing_events` from the `migrate-events` command); move
  `root_path` and `scan_table` next to their single callers or make them
  `pub(crate)`.

### S8. Small duplications (F10)

- One `push_fmt` in the engine's `enrichment/mod.rs`, `pub(crate)`; the API's
  prompt renderer uses `std::fmt::Write` with the same infallible pattern
  inline rather than importing across crates for one helper.
- The frontend's action-kind lists: `paletteActions.ts` exports
  `COLUMN_ACTION_KINDS` (per-column) and `TABLE_ACTION_KINDS` (per-table);
  `stores/dataset.ts` imports the per-column list instead of keeping its own.
- The detector declares doc columns: `detected_declaration` fills
  `DatasetExt.doc` from the same name rules `DocDisplay::infer` uses
  (`id`/`uri`, `number`, `title`, `body`/`text`, `created_at`,
  `html_url`); `DocDisplay::infer` then reduces to the fallback for a table
  with no rows at all, and its name tables are deleted in favour of the
  detector's. `DocDisplay::from_doc` and `resolve` stay.

### S9. Contract surface check (F7, D4)

- `MetricExpr::with_filter`: keep if the import tests or Longbow's README
  examples use the builder form; otherwise delete.
- `extension_json` / `from_json` on `CustomExtension`: keep if the Ossie
  conformance test round-trips through them; otherwise delete.
- Either way, a one-line doc on `lib.rs` names which functions exist for
  external producers rather than for this workspace.

### S10. Legacy code (F11, D5) — API, store, CLI

- Delete `UndoOp::RestoreKpi`, `UndoOp::RestorePolarity`,
  `undo_restore_kpi`, `undo_restore_polarity`, `LEGACY_PRODUCER`,
  `legacy_provenance`; `RestoreColumnSemantic.provenance` becomes required
  and `existed` loses its `default = "true"`. `apply_undo` refuses an
  unknown op with a clear error rather than a silent no-op, which is what a
  pre-layer row would now hit.
- `sources.kind` CHECK drops `web` in migration 029 (same migration as S3)
  unless the ingest flush starts registering web sources, which is a one-line
  change in `flush.rs` and the better fix. Default: register them, keep the
  CHECK.
- Optional CLI: `brightflow store forget-producer --source <id> --producer
  legacy` deleting a producer's rows across a source, for the `default`
  workspace's 61 rows. Not a migration.

---

## Execution order & sizing

| Step | Finding | Depends on | Size |
|---|---|---|---|
| S1 | F1 | — | half a day: the merge logic and its test |
| S2 | F2 | S1 | two hours, mostly deletion |
| S3 | F3 | — | half a day: migration, four readers, frontend |
| S4 | F4 | — | one hour |
| S5 | F5 | — | one hour |
| S6 | F6 | — | two hours |
| S7 | F7–F9 | S2 | one hour |
| S8 | F10 | S1 (doc columns from the detector) | two hours |
| S9 | F7 | — | half an hour |
| S10 | F11 | — | one hour |

S1→S2→S7 is the spine; the rest are independent and can land in any order.
Total two days at the pace of the last two, with the verification below run
per commit at `-j 2` (the pre-commit hook's full workspace test run is still
killed by the OOM killer on this machine).

## Verification

Per commit: `cargo test -p <touched crates>`, `cargo clippy --workspace
--all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`,
`npm run check`, `npm run test`; after S3 and S10, `npm run test:integration`
and `./scripts/build-test-template.sh --check` (both migrations change the
committed workspace, so a rebuild is expected once). After the last step,
`cargo build --release`.

Behavioural pins to add before the deletions they protect:

- S1: a numeric column resolved as `dimension` is a dimension in the built
  schema, and a column with no resolved row is placed by detection.
- S2: after a `set_column_label` through the bus, `load_table` and an
  insights run both see the label with no cache in between (the existing
  `action_log.rs` test plus one insights assertion).
- S3: a pre-027 `schema_json` migrates to the `columns` shape with the right
  logical types.

## Out of scope

- The `table_index` cache on `AppState` (a list of `TableInfo`), which is a
  different kind of cache with its own refresh and no semantic content.
- The `#![allow(...)]` lint blocks at the top of the store's `lib.rs`.
- Anything in Longbow: its scan found nothing unused and its shape is what
  Brightflow asked for.
- The Ossie types themselves: they match a published spec and simplifying
  them would be a cost, not a saving.
- Detection quality (entity columns by name, temporal from string patterns):
  worth doing, and made safe by the layering, but a feature rather than a
  cleanup.

## Open questions

1. **S1's placement rule for a resolved column with no role.** A row that
   only carries a label or a description says nothing about placement.
   Default: such a column goes through detection like an unmentioned one,
   which is what `merge_overrides` does today for `role: None`.
2. **S5: wrapper with `Deref`, or keep the copied fields with a name that
   says "translation"?** `Deref` to a foreign type is convenient and slightly
   magical. Default: the wrapper; the scheduler's field accesses are the only
   readers and they do not change.
3. **S10: register web sources, or drop `web` from the CHECK?** Registering
   costs one line in `flush.rs` and makes the `sources` table what its name
   says. Default: register.
