# Plan: Models — named derived tables, materialised from an operation chain, rebuilt after every sync, created by a person or an agent through the same actions

**Status:** Proposal — decisions marked *confirmed* are settled with the author
on this date; *default* are my assumptions until reviewed.
**Date:** 2026-09-13
**Baseline:** HEAD `1f8a71c`.
**Context:** Piece 1 of
[`reports/2026-09-13_modelling-layer-and-transformation-language.md`](../reports/2026-09-13_modelling-layer-and-transformation-language.md):
the noun. A *model* is a table that knows its recipe. The recipe is an
operation chain over one input table; the output is an ordinary table under
the input's source, so Explore, the engine, enrichment and agents read it
without knowing it is derived. This plan adds no join, no expression tree
and no incremental strategy; those are pieces 2 and 3 of the report. Every
model operation is an action kind on the bus with an undo, dispatched by a
person or an agent through the same path, with `auto_apply` as the default
and undo as the safety net, per `docs/human_ai_interaction.md`.

---

## Context

What the code offers and where a model hits a seam, verified at the
baseline. Line numbers are as of `1f8a71c`.

- **The chain cannot be stored or offered to an agent.** `Operation`,
  `DerivedColumn`, `DerivedExpr`, `AggSpec` and `Query`
  (`crates/brightflow-api/src/analytics/types.rs:21-125`) derive
  `Deserialize` and `TS` only: no `Serialize`, no `JsonSchema`. The action
  manifest is `schemars::schema_for!(Action)`
  (`actions/types.rs:27-30`, `agent/runner.rs:306-350`), so a variant
  carrying a chain needs the chain's schema. The contract crate's header
  names the operations language as "the next candidate" to move there
  (`crates/brightflow-types/src/lib.rs:25-27`); `Aggregation`, `FilterOp`
  and `TimeGranularity` already live there with both derives. The only Rust
  importers are `analytics/executor.rs:9-10` and `analytics/handlers.rs:23`.
  `TS_RS_EXPORT_DIR` is workspace-wide (`.cargo/config.toml`), so the
  generated TypeScript lands in the same directory from either crate.
- **Executing a chain to a frame is one extraction away.**
  `execute_query` (`analytics/executor.rs:18-79`) builds the `LazyFrame`
  from `DatasetData`, folds the operations, collects, then converts to
  JSON. Lines 22-44 plus `collect()` are the `run_chain` a rebuild needs.
  `DatasetData::Parquet { files }` is built from
  `store.get_table_parquet_paths` (`state.rs:274`).
- **The catalog cannot say a table is derived.** `tables` has `id, name,
  source_id, version, schema_json, primary_keys, partition_columns,
  total_rows` (`store/src/models.rs:17-28`). Highest migration is
  `031_saved_views`; `MIGRATIONS` is append-only
  (`store/src/db/mod.rs:30-63`). `saved_views` (031) is the template for a
  row keyed to a table with `created_by`; `enrichment_function_versions`
  (016) is the template for immutable version snapshots, bumped inside
  `BEGIN IMMEDIATE` (`db/enrichment.rs:148-205`). Every child table
  declares `REFERENCES tables(id) ON DELETE CASCADE`, so a `models` row
  keyed on its output table cascades for free.
- **There is no DataFrame-taking create path.** `ingest_parquet` takes a
  file (`store/src/ingest.rs:62-150`); `replace_table_data` takes a frame
  but requires the row to exist (`:335-408`). Enrichment's child table
  writes a scratch Parquet and ingests it (`enrichment/runner.rs:1190-1231`).
  `schema_to_json` is `pub(crate)` inside `ingest.rs` (`:420-423`).
  `ingest_parquet` deletes a zero-row file and registers nothing
  (`:112-116`); a model whose chain yields no rows must still exist.
- **Drop is one call.** `ParquetStore::delete_table` (`lib.rs:217-220` →
  `table.rs:252-269`) deletes the row and `remove_dir_all`s the folder;
  semantics, views and enrichment rows cascade.
- **The post-sync hook is where a rebuild belongs.** `bootstrap.rs:176-197`:
  detection, then `enrichment::post_sync` (spawns runs and returns), then
  `insights::auto::post_sync` (spawns, 600 s debounce). Awaited inline by
  the scheduler per endpoint. Enrichment materialisation also rewrites
  tables (`runner.rs:971-975`, `enrichment/handlers.rs:355`); a model over
  an enriched table must rebuild after those too.
- **Jobs are a read-model union.** `JobKind` has four variants
  (`jobs/mod.rs:29-37`); `GET /api/jobs` merges four collectors
  (`:338-351`); `emit_job` pushes a `Job` over the socket
  (`actions/events.rs:149-154`). The frontend's `KIND_ICONS` in
  `components/actions/JobRow.vue:19-24` is an exhaustive `Record`, so a
  new kind fails type-check until it has an icon.
- **The action pattern is fully worked in views.** `exec/views.rs` is the
  template: `author(actor)` (`:18-23`), `checked_name` (`:26-44`), create
  returns `UndoOp::DeleteSavedView`, overwrite and delete return
  `UndoOp::RestoreSavedView { row }` (`actions/types.rs:575-579`), the
  actor reaches every arm of `execute_action`
  (`actions/handlers.rs:579-583`), and `initial_status` makes an agent's
  non-undoable action a proposal even under `auto_apply`
  (`handlers.rs:72-80`). A new variant needs its `kind()` and `scope()`
  arms, an `ACTION_KINDS` tuple, a sample in the manifest test
  (`types.rs:1035-1060`), a dispatch arm and an undo arm.
- **Semantics for an output table.** `declare_output_semantics`
  (`runner.rs:1071-1096`) builds a `TableDeclaration` and calls
  `store.apply_declaration(source_id, &decl, &Provenance::declared("enrichment:{fn}"))`;
  the store requires the table row to exist first
  (`db/semantics.rs:304-309`). Input labels and descriptions are read with
  `store.resolved_columns(source_id, table)` (`lib.rs:513-520`).
  `declare_detected_if_undescribed` (`semantics/detect.rs`) gives any table
  a detected base layer.
- **The frontend already captures the state a model needs.**
  `buildPivotOperations` (`utils/buildOperations.ts:48`) turns the pivot
  and query stores into `Operation[]`; `queryStore.operations` covers the
  table path. `captureExploreView` / `applyExploreView`
  (`utils/viewSpec.ts`) round-trip the same state through the stores.
  `SavedViewsBar.vue:45-58` is the save-as-new flow to copy;
  `useSavedViews.ts` the composable. The unified sources list
  (`UNIFIED_SOURCES_KEY`, `composables/useSources.ts:16`) feeds every table
  list, so a new table appears only when it is invalidated.
  `SourceTable` (`sources/types.rs:49-68`) has no field for "derived".
  `SemanticsTool.vue` has a Table section (`:268-347`) a Recipe section can
  follow. `useCommandPalette.ts:65` does not match the `semantics-table`
  route, so the palette offers nothing there today.
- **Agent runner.** `tools_for(kind)` (`runner.rs:273-302`) maps kind → a
  slice of action kinds plus the synthetic `done`; `done` is the only
  non-action name special-cased (`:101-105`); every other name is parsed as
  an `Action` with `kind`, `source_id`, `table` injected (`:220-235`).
  `describe::TOOLS` (`agent/describe.rs:33-40`) and `describe::context`
  (`:254-289`: column profiles plus sample rows) are the run a model
  proposal joins.
- **Test harness.** `copy_template()` and `AppState::with_store`
  (`crates/brightflow-test-support`, `state.rs:157`);
  `tests/enrichment_materialize.rs:48-65` plants a table from a `df!`.
  The pre-commit hook's full workspace test run is killed by the OOM killer
  on this machine (noted in `plans/2026-09-13_lower-layers-cleanup.md`), so
  verification below is per crate.

Binding repo rules: `//!` on every new Rust file, `/** */` on every new
frontend module, co-located tests for pure logic, handlers and `.vue` files
are carve-outs, this file is dated prose. `cargo build --release` closes
the Rust work.

## Decisions reached

- **Confirmed — piece 1 only.** One input table per model, no join, no
  expression tree, no incremental strategy. The language work is piece 2.
- **Confirmed — materialised only, full refresh.** Every reader stays
  untouched. A virtual switch is a later addition, not a first cut.
- **Confirmed — one bus, two actors.** `create_model`, `update_model`,
  `delete_model` are undoable action kinds any actor dispatches;
  `rebuild_model` is idempotent and not undoable, so an agent's rebuild is
  a proposal by the existing rule, and agents are not given it.
- **Default — a model is its output table.** The model's name is the
  output table's name; uniqueness is the catalog's `UNIQUE(source_id,
  name)`. A `models` row points at the output `table_id` and cascades with
  it; `tables` gains no column.
- **Default — a model belongs to its input's source.** Same `source_id`,
  so it inherits the source's Explore page, relationships and export. A
  model may take another model as input; a model may not take its own
  output, and the create path refuses a cycle by walking inputs.
- **Default — the recipe is `ModelRecipe { version: 1, operations }`** in
  the contract crate, next to the operation types it carries. The input is
  the action's scope, not a field of the recipe, so the agent's tool schema
  (which strips scope) is `name` plus `operations` and nothing else. An
  optional opaque `client_spec` travels beside the recipe so Explore can
  reopen a model for editing through `applyExploreView` without a
  chain-to-stores compiler.
- **Default — `Query` stays in the API crate.** It carries `dataset_id`
  and `request_id`, which are session concerns. `Operation`,
  `DerivedColumn`, `DerivedExpr`, `AggSpec` move to
  `brightflow_types::ops` with `Serialize` and `JsonSchema` added; the API
  re-exports them and the generated TypeScript is unchanged.
- **Default — a rebuild is a sync of a derived table.** After writing the
  output it runs the same post-sync sequence a connector endpoint gets:
  detection, dependent models, enrichment, insights. So a promoted
  enrichment function on a model output re-materialises from cache after
  every rebuild, and models over models rebuild in order. The sequence is
  extracted from the bootstrap closure into one function both callers use.
- **Default — rebuilds block the hook.** The scheduler already awaits the
  hook inline; dependent models rebuild depth-first before enrichment and
  insights are spawned for the synced table. The existing race between
  enrichment and insights is left as it is (out of scope, noted below).
- **Default — output semantics are declared under `model:{id}`.** For a
  column that passes through with the same name and datatype as an input
  column, the input's resolved label, description, role, polarity and KPI
  flag are copied. Aggregate outputs get role `measure`; period outputs
  get role `time`. Everything else gets the detected base layer. The
  dataset description names the input and the model. A person's later
  edit sits at the User layer and outranks all of it.
- **Default — an empty result still creates the table.** The write path
  registers a zero-row file for a model, diverging from `ingest_parquet`'s
  rule, so a model does not vanish when a filter matches nothing.
- **Default — the agent gets `create_model` in the describe-table run**,
  with a read-only `preview_model` tool that runs the chain over the input
  and returns the first rows and schema. No new run kind, no migration to
  the agent-runs CHECK.
- **Default — the recipe is shown as prose steps** on the Semantics page,
  from a pure `describeOperations(ops)`, the way `describeExploreView`
  describes a view. A step editor is out of scope.

## §0 — The chain crosses a second boundary (1 commit, Rust)

`crates/brightflow-types/src/ops.rs` (new): `Operation`, `DerivedColumn`,
`DerivedExpr`, `AggSpec` moved verbatim from `analytics/types.rs`, deriving
`Debug, Clone, PartialEq, Serialize, Deserialize, TS, JsonSchema`, plus

```rust
/// A model's recipe: the chain that builds its output from its input. The
/// input is the model's scope, not part of the recipe, so an agent's tool
/// schema is the name and the chain and nothing else.
pub struct ModelRecipe {
    pub version: u32,            // RECIPE_VERSION = 1
    pub operations: Vec<Operation>,
}
```

`TimeGranularity` comes from `ext.rs`. `Filter.value` keeps
`serde_json::Value` with `#[ts(type = "unknown")]` and a `schemars` schema of
`true`. `lib.rs` re-exports the module and its header drops the "next
candidate" sentence. `analytics/types.rs` keeps `Query` and re-exports the
four types so `executor.rs` and `handlers.rs` compile unchanged.

Tests in `ops.rs`: serde round trip of a chain with every variant; the
JSON Schema contains a `oneOf` with seven `type` constants; a recipe with
`version: 2` deserialises (forward-compatible) and `ModelRecipe::validate`
refuses it. Regenerate TypeScript; the diff must be empty apart from the
new `ModelRecipe.ts`.

## §1 — Store: the noun and the write path (1 commit, Rust)

`migrations/032_models.sql`:

```sql
CREATE TABLE models (
    id               TEXT PRIMARY KEY,                       -- uuidv7
    output_table_id  TEXT NOT NULL UNIQUE REFERENCES tables(id) ON DELETE CASCADE,
    input_table_id   TEXT REFERENCES tables(id) ON DELETE SET NULL,
    current_version  INTEGER NOT NULL DEFAULT 1,
    created_by       TEXT,
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL
);
CREATE TABLE model_versions (
    model_id     TEXT NOT NULL REFERENCES models(id) ON DELETE CASCADE,
    version      INTEGER NOT NULL,
    recipe_json  TEXT NOT NULL,        -- ModelRecipe, immutable
    client_spec  TEXT,                 -- opaque Explore snapshot, for editing
    created_by   TEXT,
    created_at   INTEGER NOT NULL,
    PRIMARY KEY (model_id, version)
);
CREATE TABLE model_builds (
    id           TEXT PRIMARY KEY,
    model_id     TEXT NOT NULL REFERENCES models(id) ON DELETE CASCADE,
    version      INTEGER NOT NULL,
    status       TEXT NOT NULL CHECK (status IN ('running','completed','failed')),
    triggered_by TEXT NOT NULL,        -- 'create' | 'update' | 'sync' | 'manual' | 'undo'
    rows         INTEGER,
    error        TEXT,
    started_at   INTEGER NOT NULL,
    finished_at  INTEGER
);
CREATE INDEX models_input_idx ON models(input_table_id);
CREATE INDEX model_builds_model_idx ON model_builds(model_id, started_at);
```

`ON DELETE SET NULL` on the input: deleting an input leaves the model and
its output in place, and the next rebuild fails with "input table is gone"
rather than the model silently disappearing.

`src/db/models.rs` (new, on the `views.rs` shape): `insert_model(row,
first_version)` in one transaction; `get_model`, `get_model_by_output(table_id)`,
`list_models_for_source(source_id)` (joined with `tables` for both names),
`models_with_input(table_id)`, `get_model_version(model_id, version)`,
`append_model_version(model_id, recipe_json, client_spec, created_by)`
(reads `current_version + 1` inside `BEGIN IMMEDIATE`, as enrichment does),
`set_current_version`, `delete_model`, `insert_build`, `finish_build`,
`list_recent_builds(limit)`, `fail_stuck_builds` at startup. Row structs
`ModelRow`, `ModelVersionRow`, `ModelBuildRow`, `ModelListRow` in
`models.rs` with `impl_from_row!`.

`ParquetStore::write_table(source_id, name, df) -> StoreResult<TableInfo>`
in `ingest.rs`, create-or-replace: if the row exists, the
`replace_table_data` body without the version check; else `create_table`,
write one zstd file under a new uuid path in `spawn_blocking`,
`add_table_file`, `update_table_meta`, `upsert_column_stats`. Registers a
zero-row file; the header says why it diverges from `ingest_parquet`.

Tests: `db/models.rs` unit tests on `temp_db` (insert → get by output →
append version bumps current → list for source shows both names → delete
cascades versions and builds → deleting the input table nulls
`input_table_id`); an `ingest.rs` test that `write_table` creates then
replaces and keeps a zero-row table.

## §2 — Rebuild: chain to table to semantics (1 commit, Rust)

`crates/brightflow-api/src/analytics/executor.rs`: extract
`pub fn run_chain(data: &DatasetData, operations: &[Operation]) -> AppResult<DataFrame>`
from `execute_query` (which calls it, keeping its two-pass `total_rows`
for limited queries). Test: the existing pivot tests run through it
unchanged.

`crates/brightflow-api/src/models/` (new):

- `mod.rs`: `pub async fn rebuild(state, model_id, trigger) -> AppResult<BuildOutcome>`.
  Takes `state.materialize_lock(cache_key(source, output))`, so a rebuild
  and an enrichment materialisation of the same output serialise. Loads the
  model, its current recipe, the input's parquet paths; `run_chain`;
  `store.write_table`; then `declare_detected_if_undescribed` on the output,
  then `semantics::declare_model_output`; records the build row and emits a
  `Job` at start and finish. On any error the build row is `failed` with
  the message and the output table is left as it was.
- `semantics.rs`: `declare_model_output(store, model, input_resolved,
  output_schema, operations)` builds the `TableDeclaration` per *Decisions*
  and applies it under `Provenance::declared(format!("model:{id}"))`. Pure
  helper `carry_over(input: &[ResolvedColumn], output: &TableSchema,
  operations) -> Vec<Field>` with tests: a pass-through column keeps its
  label and role; a renamed aggregate is a measure; a period column is
  time; a column with a different datatype in the output carries nothing.
- `deps.rs`: `pub fn dependents_in_order(models: &[ModelRow], changed: &str) -> Vec<String>`
  pure, depth-first over `input_table_id == output_table_id` edges with a
  visited set; tests for a chain of three, a fan-out, and a cycle guard.
  `pub async fn rebuild_dependents(state, source_id, table)` lists the
  source's models and rebuilds each in that order; each rebuild ends by
  running the post-sync sequence on its own output (§3), which recurses.
- `refresh`: after a write, the same in-memory table index refresh the CSV
  upload path performs, so `list_available_tables` sees the new table.

Integration test `crates/brightflow-api/tests/models.rs` (on the
`enrichment_materialize.rs` harness): plant `orders`, set a label on
`region`, call `rebuild` for a model `orders_by_region` whose chain is a
group-by; assert the output table exists with the expected rows, that
`region` carries the label under `model:{id}` at the Declared layer, that
the aggregate column is a measure, and that a second rebuild replaces
rather than appends. A model with an always-false filter yields a
zero-row table that still exists.

## §3 — The post-sync sequence, shared (1 commit, Rust)

`crates/brightflow-api/src/sync.rs` (new): `pub async fn after_table_write(state, source_id, table)`
with the body of today's hook closure in this order: detection →
`models::rebuild_dependents` (awaited) → `enrichment::post_sync` →
`insights::auto::post_sync`. `bootstrap.rs` installs it as the hook.
`models::rebuild` calls it on its output after a successful write.
`enrichment::runner::materialize` and `enrichment/handlers.rs::drop_output_columns`
call `models::rebuild_dependents` after their `replace_table_data`, so a
model over an enriched table follows the enrichment, and one over a reset
table follows the reset. Header comment states the order and why models
come before enrichment.

Test: a unit test on `dependents_in_order` is in §2; the integration test
in §2 gains a case where a model over a model rebuilds after the first,
verified by build timestamps.

## §4 — Actions, jobs, read route (1 commit, Rust)

`actions/types.rs`, four variants:

```rust
/// Create a model: a table named `name` in this table's source, built by
/// running `operations` over this table, rebuilt after every sync of it.
CreateModel { scope, name: String, recipe: ModelRecipe,
              #[serde(default)] #[ts(optional)] #[ts(type = "unknown")] client_spec: Option<serde_json::Value> },
/// Replace a model's recipe (and optionally rename it); a new version and
/// an immediate rebuild. Undo restores the previous version and rebuilds.
UpdateModel { scope, model_id: String,
              #[serde(default)] #[ts(optional)] name: Option<String>,
              #[serde(default)] #[ts(optional)] recipe: Option<ModelRecipe>,
              #[serde(default)] #[ts(optional)] #[ts(type = "unknown")] client_spec: Option<serde_json::Value> },
/// Remove a model and its output table. Undo recreates both.
DeleteModel { scope, model_id: String },
/// Build the model's output again from its current recipe. Idempotent.
RebuildModel { scope, model_id: String },
```

Scope of `CreateModel` is the **input** table; scope of the other three is
the model's output table, so the Explore page of a model addresses it
directly. `ACTION_KINDS`: the first three `undoable = true`, `rebuild_model`
`false`. `kind()`, `scope()`, the manifest samples, dispatch arms and undo
arms all gain their lines. Rename in `UpdateModel` is a `RenameTable` in
the store: the folder moves and the catalog name changes in one call
(new `ParquetStore::rename_table`), which `saved_views` and enrichment rows
survive because they key on `table_id`.

`UndoOp`: `DeleteModel { model_id }` (undo of create),
`RestoreModelVersion { model_id, version, name }` (undo of update: set
current, rename back if needed, rebuild), `RecreateModel { model: ModelRow,
versions: Vec<ModelVersionRow>, name, source_id }` (undo of delete:
reinsert the rows under their original ids, rebuild). The rows are captured
before the cascade, exactly as `RecreateVocabulary` does.

`actions/exec/models.rs` (new, on `views.rs`): `checked_name` (non-blank,
no path separators, not an existing table in the source, not the input),
`checked_recipe` (`ModelRecipe::validate`, every referenced column exists
in the input's schema, no cycle through `dependents_in_order`), `author`
from the actor. Create inserts the rows, rebuilds, and returns
`{ model_id, table, rows, created: true }`; a failed first build deletes
the rows again and returns the error, so a bad recipe never leaves a
half-model.

`jobs/mod.rs`: `JobKind::ModelBuild`, a `model_jobs` collector over
`list_recent_builds`, `emit_model_job`, and the `GET /api/jobs` merge.
`routes.rs`: `GET /api/sources/{source_id}/models` →
`models::handlers::list_models` returning `ModelSummary { id, table,
input_table, version, last_build: Option<{status, finished_at, rows,
error}>, created_by }` (`#[ts(export)]`); writes go through the bus.
`sources/types.rs::SourceTable` gains `model: Option<ModelBadge { id,
input_table }>`, filled in `source_tables`.

Tests: the manifest completeness tests force the samples; `exec/models.rs`
unit tests for `checked_name` and cycle refusal; the integration test in
§2 becomes a bus test: `create_model` → applied and the table exists →
`update_model` with a new filter → row count changes → undo → row count
back → `delete_model` → table gone → undo → table and rows back under the
same model id; an agent actor with `auto_apply` gets `applied` for create
and `proposed` for rebuild.

## §5 — Explore, table lists, Semantics, Activity (2 commits, frontend)

### 5a. Data layer and Explore

- `services/api/models.ts`: `modelsApi.list(sourceId)`.
- `composables/useModels.ts` (on `useSavedViews.ts`): query
  `['models', sourceId, curation.version]`; `create`, `update`, `remove`,
  `rebuild` through `dispatchWithFeedback`; after `create` and `remove`,
  invalidate `UNIFIED_SOURCES_KEY` and `['tables-index', sourceId]` so the
  table appears or disappears everywhere; after `create`, navigate to
  `explore-table` for the new table.
- `utils/modelRecipe.ts` (pure, tested): `captureRecipe(queryStore,
  pivotStore) -> { recipe: ModelRecipe, clientSpec: ExploreViewSpec }`
  using `buildPivotOperations` when a pivot is configured and
  `queryStore.operations` otherwise; `describeOperations(ops) -> string`
  ("filtered by region, grouped by order date (month), sum of revenue,
  top 100"). Tests: pivot and table paths, each operation kind named.
- `SavedViewsBar.vue`: a fourth menu group, *Save as model…*, prompting
  for a name and dispatching `create_model`. On a model's own table the
  same menu shows *Edit recipe* (applies `client_spec` to the input table's
  Explore page via the route and `applyExploreView`), *Update model from
  this view* (dispatches `update_model` with a fresh capture), *Rebuild*,
  and *Delete model*.
- `paletteActions.ts`: `MODEL_KINDS = ['create_model', 'update_model',
  'rebuild_model', 'delete_model']` offered on the explore route with the
  same flows; `useCommandPalette.ts` adds `semantics-table` to its route
  regex so the palette also works there.

### 5b. Where a model shows

- `TableSectionPane.vue` and `ConnectorDashboard.vue`: a `Model` badge
  (`UBadge size="md"`) with "of {input}" from `SourceTable.model`; the
  overview card shows the last build status and time.
- `SemanticsTool.vue`: a *Recipe* section after the Table section on a
  model table: input (link), version, `describeOperations` steps as a
  list, last build with status and error, and Rebuild / Edit / Delete
  buttons through `useModels`.
- `JobRow.vue`: `KIND_ICONS.model_build`; `activityGroups.ts` needs no
  change (labels come from the backend).

Frontend tests: `modelRecipe.test.ts`, `paletteActions.test.ts` entries for
the four kinds, `useModels` untested per the composable convention.

## §6 — The agent can propose a model (1 commit, Rust)

`agent/describe.rs`: `TOOLS` gains `create_model`; the system prompt gains
one numbered step: after describing the columns, if a cleaned or
aggregated table would serve the person better than the raw one, create
it with `create_model`, naming what it is for; call `preview_model` first.
`agent/runner.rs`: a second special-cased read-only tool, `preview_model
{ operations }`, built like `done`, which runs `run_chain` over the run's
table with a trailing `limit 20` and returns `{ columns, rows }` as the tool
result. `tools_for("describe_table")` includes it. `create_model` from an
agent is `applied` under `auto_apply` and appears in Activity like every
other action, with Undo.

Tests: `parse_action` keeps injecting scope for `create_model`; a runner
unit test that `preview_model` is dispatched to the executor and never to
the bus; the describe `TOOLS` test already asserts every tool is a real
kind.

## Execution order & sizing

| Step | Commits | Depends on | Size |
|---|---|---|---|
| §0 | 1 | — | half a day: a move plus derives and tests |
| §1 | 1 | — | one day: migration, `db/models.rs`, `write_table` |
| §2 | 1 | §0, §1 | one day: rebuild, carry-over semantics, integration test |
| §3 | 1 | §2 | half a day: the shared sequence and its callers |
| §4 | 1 | §2 | one day: four kinds, undo, jobs, read route, badge |
| §5 | 2 | §4 | one and a half days |
| §6 | 1 | §4 | half a day |

Six working days. §0 and §1 can land in either order. Per Rust commit:
`cargo fmt --check`, `cargo clippy --workspace --all-targets`,
`cargo test -p <touched crates>` (not `--workspace`, per the OOM note),
TypeScript regenerated with `TS_RS_EXPORT_DIR` and the barrel script; after
§1, `./scripts/build-test-template.sh --check` and a rebuild if the
migration changes the committed workspace. Per frontend commit:
`npm run check`, `npm run test`. `cargo build --release` after §6.

## Verification

- **Rust unit** — ops serde and schema (§0); models db and `write_table`
  (§1); `carry_over`, `dependents_in_order`, `run_chain` (§2);
  `checked_name`, cycle refusal, manifest completeness (§4); `preview_model`
  dispatch (§6).
- **Rust integration** — `tests/models.rs`: rebuild, semantics carried,
  model over model in order, then the full bus round trip with both actors.
- **Frontend unit** — `modelRecipe.test.ts`, `paletteActions.test.ts`.
- **Frontend integration tier** still green.
- **Headless browser drive**, as for Explore on 2026-09-12, on a fresh test
  workspace: open `orders`, set `order_date` to time, build revenue by
  month, save as model `revenue_by_month`; the table appears in the source
  list with a Model badge; open it in Explore and see three rows; open its
  Semantics page and read the recipe; trigger a rebuild from the palette
  and see the build in Activity; delete it and undo from the toast.
- **Manual with an LLM configured** — run describe-table on `orders` and
  check whether the agent proposes a model, that it previews first, and
  that the created table and its Undo appear in Activity.

## Out of scope

- **Join, expression tree, compound filters** — piece 2 of the report.
  `carry_over` is written so those add cases rather than reshape it.
- **Metrics over models and `count_distinct`** — piece 3.
- **Incremental strategies, a virtual switch, the text formula surface,
  Ossie export of a model as a query-sourced dataset.**
- **A step editor for the recipe.** Editing round-trips through Explore via
  `client_spec`; a chain built by an agent without a `client_spec` can be
  rebuilt, renamed and deleted but not reshaped in the UI until piece 2.
- **The enrichment → insights race.** The shared sequence spawns both as
  today. Triggering the insights auto-run at the end of an enrichment
  materialisation is a separate small change.
- **Cross-source models**, which need a source kind of their own and a
  CHECK rebuild on `sources`.
- **Registering the sample connector source in the test template**, noted
  on 2026-09-12; unrelated, still open.

## Open questions

1. **Should a rebuild after a sync be debounced?** A source with ten
   endpoints fires the hook ten times; a model over one endpoint rebuilds
   once, but a model over a model over two endpoints could rebuild twice.
   (Default: no debounce in piece 1; the lock serialises, and full refresh
   at this scale is seconds.)
2. **Does deleting a model's input delete the model?** (Default: no;
   `ON DELETE SET NULL`, the next build fails with a named error, and the
   Semantics page shows it. A person or agent deletes the model if they
   want it gone.)
3. **Should `update_model` with only a new `name` rebuild?** (Default: no;
   rename is a catalog and folder move, the data is unchanged.)
4. **Where does "Save as model" live for a non-pivot table state?** The
   same menu; the recipe is filters plus sort plus limit. (Default: allow
   it; a filtered subset is a legitimate model.)
5. **Is `describe_table` the right run for the first agent proposals, or
   does it dilute that run's purpose?** (Default: keep it there for piece 1
   and watch what the agent does; a `propose_model` run kind is a small
   follow-up if the describe run starts creating tables people did not
   want.)
