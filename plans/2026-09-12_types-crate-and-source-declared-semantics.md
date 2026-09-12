# Plan: a types crate as the contract, connectors declare types and semantics, Litehouse holds them as layered rows

**Status:** Proposal — decisions marked *confirmed* are settled with the author
on this date; *default* are my assumptions until reviewed.
**Date:** 2026-09-12
**Baseline:** HEAD `d29a3a7`. Longbow at `../longbow` (path dependency).
**Context:** A same-day discussion about where data types and the semantic
layer should live between Longbow (ingestion) and Litehouse (the SQLite
catalog over Parquet in `crates/brightflow-store`). It follows
[`reports/2026-09-08_litehouse-as-semantic-layer.md`](../reports/2026-09-08_litehouse-as-semantic-layer.md)
(Litehouse is catalog *and* semantic layer),
[`reports/2026-09-08_data-platform-completeness-and-operations-language.md`](../reports/2026-09-08_data-platform-completeness-and-operations-language.md)
(the operations language as the modelling language) and
[`plans/2026-09-12_semantics-consumer-and-period-bucketing.md`](2026-09-12_semantics-consumer-and-period-bucketing.md)
(landed as `b7aabf9..d29a3a7`: one enum vocabulary, write paths on the bus,
Explore reads semantics, `withColumns`/`period`). Three research passes were
made for this plan: a map of Longbow's type and metadata surface, a map of
every type and semantic vocabulary in this repo, and a survey of how
connector ecosystems, catalogs, semantic-layer specs and LLM tooling handle
source-shipped semantics. Code claims below were spot-checked against HEAD.
No code changed.

---

## Context

### The design, as agreed

1. **Litehouse is both catalog and semantic layer**, one SQLite file per
   workspace, holding the *actual* state of that workspace. It has vocabulary
   (what a role, a relationship, a metric is) but no content of its own: no
   source templates, nothing pre-stated before a producer hands it something.
   Because it holds state rather than templates, it is editable in place.
2. **Longbow connectors carry the source's types and semantics.** The GitHub
   connector knows which columns are timestamps, ids, measures, and how
   `issue_comments` joins `issues`. It declares that next to the `map`
   function that produces the columns, and Litehouse learns it when the table
   is created.
3. **Users and agents author too**, through the action bus, and their edits
   survive re-sync and connector upgrades. That requires Litehouse to remember
   which layer wrote each fact, which is provenance, not a template.
4. **The interchange is Ossie-shaped.** Apache Ossie (incubating, formerly
   Open Semantic Interchange) is the one standard for semantic-model
   interchange with a JSON Schema, converters and catalog integration
   (Apache Polaris). Its data model is adopted; its YAML is not. Brightflow's
   own vocabulary (role, polarity, KPI, display label) rides in Ossie's
   `custom_extensions` until the spec grows those fields.
5. **A types crate is the contract.** One small crate, types and validation
   only, no I/O, holding the logical data types, the ingest declaration, the
   Ossie-shaped semantic model, Brightflow's extension vocabulary and the
   provenance type. Longbow, Litehouse, the engine, the API and the generated
   TypeScript all depend on it, so no two of them can disagree. The precedent
   is `arrow-schema`: a tiny crate every reader, writer and engine shares.
6. **Each primitive carries what it is good at.** Parquet carries types
   (logical types are the Longbow-to-Litehouse type contract, and Parquet
   cannot carry meaning). SQLite carries meaning, provenance and transactions,
   with JSON1 for extension bags and FTS5 available for descriptions. Polars is
   the consumer: metrics are structured (column, aggregation, filter) and
   compile to Polars expressions through the operations language; Ossie's SQL
   metric expressions are produced on export only.

### What the code says today

**Longbow's type surface** (`../longbow/src/parquet.rs`):
- `infer_type` maps every JSON string to `Utf8` (`:86`); there is no date or
  timestamp branch and no declared-type override. Every `created_at`,
  `updated_at`, `closed_at`, `merged_at`, `indexed_at` from both connectors
  lands in Parquet as a string.
- The vocabulary it can emit is `Null`, `Boolean`, `Int64`, `Float64`,
  `Utf8`, `List`, `Struct`. `merge_types` resolves conflicts "first type
  wins" silently; struct fields are inferred from one record at a time;
  fields are sorted alphabetically before writing; all are marked nullable.
- The writer sets no Parquet key/value metadata and no Arrow field metadata
  (`:34-36`). There is no in-band channel for anything Longbow knows and
  Litehouse wants.
- Longbow uses `arrow` 54.3.1 and `parquet` 54.3.1 (arrow-rs). Brightflow
  uses `polars` 0.48.1 with `polars-arrow`. Both stacks are linked into one
  binary (`Cargo.lock`); their only interop is the Parquet file on disk.

**What a connector can declare** (`../longbow/src/pipeline.rs`): `path`,
`params`, `headers`, `map`, `records_path`, `primary_key` (default `["id"]`),
`cursor_field`. Nothing about column types, labels, descriptions or
relationships. The frontmatter (`--[[ @longbow ... ]]`, TOML) recognises
`name`, `version`, `description` only (`:48-53`), without
`deny_unknown_fields`, so extra TOML parses and is silently discarded.

**What crosses into Litehouse**: `store.merge_parquet(&source_id,
&ep_result.name, &parquet_file, &ep_result.primary_key)` in
`crates/brightflow-scheduler/src/lib.rs:312`. Four arguments, one of them
semantic. `map_run_result` in `crates/brightflow-connect/src/lib.rs:166`
drops Longbow's `RunResult.meta` (connector name, version, source hash);
`ConnectorResult` (`:110`) has no `meta` field. Cursor values go to the
scheduler's own SQLite, never to Litehouse. Primary keys are persisted as
`tables.primary_keys` and used only as merge keys; nothing reads them as
semantics. Litehouse cannot say which connector produced a table, at which
version. The `sources` table (`migrations/015_sources.sql`) is
`CHECK (kind IN ('upload'))`.

**The GitHub template already exists, in the API crate.**
`seed_column_semantics` in `crates/brightflow-api/src/state.rs:440` hardcodes
61 column-role pairs for `issues`, `pull_requests`, `issue_comments`, keyed by
bare table name, applied to every source whose table has that name, gated on
"no semantics rows anywhere in the database" (`:442`). Once anyone edits
anything the seed never fires again, including for a GitHub connector added
later. Every column name in it is a `map` output in
`crates/brightflow-connect/connectors/github.lua`, maintained in lockstep
across two repos with nothing checking drift. `DocDisplay::for_table` in
`crates/brightflow-api/src/enrichment/display.rs` (id, title, body, URL,
timestamp columns, matched on table name) and the Bluesky permalink builder
in the same file are the same knowledge in the same wrong place. The
`issue_comments.issue_number` to `issues.number` relation exists only as a
regex and a Lua comment in `github.lua`.

**Six data-type vocabularies, no mapping between them**: arrow-rs `DataType`
(Longbow), Polars `DataType`, Polars `Display` strings persisted in
`tables.schema_json` (`crates/brightflow-store/src/ingest.rs::schema_to_json`,
`nullable` hardcoded `true`), `dtype_to_string` in
`crates/brightflow-api/src/analytics/executor.rs`, the alias sets in
`brightflow-app/src/utils/dtype.ts` (which lists every spelling of every type
in one set), and `supports_min_max` in `crates/brightflow-store/src/stats.rs`.
`schema_has_text_column` in `crates/brightflow-api/src/shared/mod.rs` tests
`t == "str" || t == "string"` to bridge two of them. Events tables never get a
`schema_json` at all (`register_file` does not write it).

**The semantic vocabulary and the table that stores it live in crates that do
not see each other.** `ColumnRole`, `Polarity`, `TimeGranularity` are in
`crates/brightflow-engine/src/data/config.rs`; `column_semantics` is in the
store, whose row structs are deliberately stringly typed
(`crates/brightflow-store/src/models.rs`); neither crate depends on the
other; `brightflow-api` is the only crate that sees both, so every
string-to-enum parse lives there, and its two writers disagree on strictness
(`state.rs::convert_semantic_row` warns and drops, `actions/exec/semantics.rs`
errors on role and defaults on polarity). The SQL `CHECK` constraints are a
third copy of the value set, `semanticLabels.ts` a fourth. Two wire types
carry the same concept, `ColumnInfo.role: Option<ColumnRole>` and
`ColumnSemantic.role: String`. `DataSchema` has no label, description or
entity list, so `build_schema` drops labels and descriptions and treats
`entity` like `ignored`.

**Producers of semantics today**: the GitHub seed (once, off the bus); the
five `set_column_*` actions (on the bus, undoable); the detected-role default
computed at action time inside `actions/exec/semantics.rs`; enrichment's
`OUTPUT_SEMANTICS` / `FLAG_SEMANTICS` written with `INSERT ... ON CONFLICT DO
NOTHING` after materialisation (the `{table}_mentions` child table gets none);
and `PUT .../settings`, the one semantic write not on the bus. Nothing records
which of these wrote a row.

### What the survey found

- **Every mature connector ecosystem splits semantic metadata by writer, not
  by content.** Singer's catalog distinguishes *discoverable* metadata (the
  tap writes it: key properties, replication keys, inclusion) from
  *non-discoverable* (the user or UI writes it: selected, replication
  method). Airbyte makes the split two types, `AirbyteCatalog` versus
  `ConfiguredAirbyteCatalog`. DataHub keeps `schemaMetadata` (ingestion) apart
  from `editableSchemaMetadata` (UI) precisely so re-ingest cannot clobber a
  human edit; its known wart is that a column the source stops emitting does
  not cascade into the editable aspect. Kubernetes server-side apply tracks
  ownership per field and conflicts instead of overwriting. dlt writes the
  inferred schema once to a user-writable import location and thereafter
  treats hand edits as authoritative, detecting them by hash.
- **Nobody ships a per-connector semantic pack.** Fivetran's dbt packages are
  the nearest thing, and Fivetran is merging its raw-source and modelled
  packages back into one because the split cost more than it bought. Looker
  Blocks are the only true layering mechanism (read-only import, `+view`
  refinements with per-key replace-or-accumulate rules). Rill, Lightdash,
  Evidence and Metabase have no overlay: the template is a repo you clone.
- **Arrow and Parquet cannot carry meaning.** There is no standard key for a
  column description or label. Arrow field metadata is dropped by Polars on
  read; Polars exposes file-level key/value only, through experimental
  APIs. Iceberg has a per-field `doc`; DuckLake stores comments as a reserved
  key in a generic, snapshot-versioned `ducklake_column_tag` table. The
  Polaris converter decomposes an Ossie model into Iceberg schema and
  properties rather than storing the document.
- **Nobody durably marks LLM-authored definitions.** Snowflake's generator
  uses a trailing `__` in a YAML comment that vanishes on acceptance;
  Databricks' AI comment becomes an ordinary comment; dbt Copilot, Cube and
  Wren AI record nothing. Only catalog vendors (Atlan, Alation) keep a badge,
  with the rule "AI writes only where a human has not; human takes display
  precedence". Snowflake's `verified_by`/`verified_at` attest example
  queries, not definitions.
- **Ossie, read in full** (`core-spec/spec.yaml`, `ossie-schema.json`,
  `spec.md`, `ROADMAP.md`, converters, examples). It is a data model with a
  JSON Schema; the README calls it "a single JSON- and YAML-based
  specification", the Python package is Pydantic models, the CLI is Go, and an
  open discussion (#62) argues plain YAML is too weak and proposes CUE or
  Jsonnet. Version 0.1.1 shipped 2025-12-11; the current `0.2.0.dev0` schema
  says "do not depend on this version in production". The model: a
  semantic model has `datasets`, `relationships`, `metrics`; a dataset has
  `name`, `source`, `primary_key`, `unique_keys`, `description`, `fields`; a
  field has `name`, a **required** `expression` (SQL per dialect), optional
  `dimension.is_time` (a role flag, explicitly independent of datatype),
  `datatype`, `label` (a categorisation tag, *not* a display name),
  `description`; a relationship is many-to-one with `from`/`to` and column
  lists; a metric is a name plus a **required** SQL aggregate expression.
  Every level carries `ai_context` (`instructions`, `synonyms`, `examples`)
  and `custom_extensions` (`vendor_name` plus `data`, which the schema types
  as a JSON *string*). Ten logical datatypes: `String`, `Integer`, `Decimal`,
  `Float`, `Boolean`, `Date`, `Time`, `DateTime`, `DateTimeTz`, `Opaque`. No
  measure/dimension role beyond `is_time` (measures versus metrics is
  unresolved in their working group, discussion #29); no display name,
  polarity, KPI flag or default aggregation (all in the open "Extended
  Metadata" proposal, issue #100, 2026-03-26); no provenance, layering or
  per-object versioning.

Binding repo rules: pure logic gets co-located tests (rule 2); Axum handlers
and `.vue` files are carve-outs; module headers state this file's contract,
not observations about others (rule 1); this file is dated prose (rule 3).
`cargo build --release` closes every Rust step.

## Decisions reached

- **Confirmed — Litehouse is catalog and semantic layer**, holds actual
  state, no source templates. Producers hand it declarations; it stores rows.
- **Confirmed — connectors declare types and semantics**, inline in the Lua
  connector next to the `map` that produces the columns. The GitHub seed in
  the API crate is deleted, its content moves into `github.lua`.
- **Confirmed — the interchange is Ossie-shaped**, not Ossie-YAML. Ossie's
  names are used wherever a concept exists; Brightflow's additions ride in a
  `BRIGHTFLOW` custom extension. Ossie validity is a generated-export test,
  not the storage format.
- **Confirmed — one types crate is the contract**, types and validation only,
  no import, no export, no I/O. Litehouse owns apply and export; Longbow owns
  write.
- **Confirmed — the crate's name ends in `-types`.** *Default:*
  `brightflow-types` as the directory and package name while it lives in this
  workspace; a name of its own when it moves out, which changes no dependency
  arrow.
- **Confirmed — serialisation per context**: Lua tables inline in
  `p.endpoint` for connector declarations; JSON on the wire (Longbow result,
  HTTP); TOML only for anything a person hand-authors outside a connector;
  rows in SQLite at rest; YAML never, except optionally at export because
  other tools' converters read it.
- **Confirmed — metrics are structured** (column, aggregation, optional
  filter), compiled to Polars through the operations language. Ossie SQL is
  rendered on export only; arbitrary SQL metrics are not imported. Named
  ratios such as bounce rate need a second metric kind and wait for the
  modelling layer.
- **Default — the types crate does not depend on Polars or arrow-rs
  unconditionally.** A `polars` feature adds `LogicalType` to and from Polars
  `DataType`; an `arrow` feature adds the same for arrow-rs 54. Litehouse and
  the engine enable `polars`; Longbow enables `arrow`. Moving Longbow's writer
  onto Polars (which would also replace its hand-rolled inference) is noted as
  a later option, not taken now.
- **Default — semantic rows are layered, one row per (object, layer), with a
  per-field resolved view.** Layers in precedence order: `user` > `agent` >
  `declared` > `detected`. Resolution is a per-column `COALESCE` across layers
  in that order, so a user who changes only a label inherits the connector's
  description. A producer re-declaring rewrites only its own `declared` rows.
  This is the Singer/DataHub split as SQL, and dbt-osmosis's placeholder rule
  as a view.
- **Default — `declared` rows carry the producer and its version**
  (`connector:github`, `0.2.0`, source hash; `enrichment:ticket_classify`,
  function version). A connector's declaration is applied on every sync, not
  only on table creation, because it only ever touches its own rows.
- **Default — the detector becomes a producer.** For tables with no
  declaration (uploads, events), detection runs at table creation and writes
  `detected` rows, so the resolved view always has a base and
  `GET .../semantics` and `load_table` stop answering differently for an
  un-annotated column. The detected-role computation at action time in
  `actions/exec/semantics.rs` goes away.
- **Default — `PUT .../settings` moves onto the bus** as `set_table_settings`
  (display name, description, granularity, comparison periods), the last
  off-bus semantic write.
- **Default — the `sources` table widens** to `kind IN ('upload',
  'connector', 'web')` and gains producer name, version and hash, so Litehouse
  knows which connector at which version produced a table without consulting
  the scheduler database.
- **Default — Longbow reads exactly one key from a column declaration**,
  `datatype`, to choose the Arrow type, and passes the rest through as an
  opaque JSON value it never opens. It stays ignorant of roles. Declared type
  beats inferred; a value that fails to parse as the declared type becomes
  null and is counted, never a run failure.
- **Default — column order in the Parquet file follows the declaration**,
  then alphabetical for undeclared columns.
- **Default — fixed semantic columns plus one JSON column for the extension
  bag**, not a generic tag table. Fixed columns are validated by SQLite and
  simple to query; the long tail of keys nobody has invented yet goes in
  JSON1. A store unit test asserts the `CHECK` literal lists equal the
  crate's enum value lists, so the SQL copy of the vocabulary cannot drift.
- **Default — the operations language stays in the API for now.** It is the
  next candidate for the types crate (an LLM emitting operations against
  contract types is its second boundary), and the crate is laid out knowing
  that, but it does not move in this plan.

## §0 — The types crate (2 commits, Rust)

`crates/brightflow-types/`, `[dependencies]`: `serde`, `serde_json`,
`ts-rs`, `schemars`, `thiserror`. Features: `polars` (adds the workspace
`polars` dependency), `arrow` (adds `arrow = "54"`). Module layout, each with
a `//!` header stating its contract:

- `datatype.rs` — `LogicalType { String, Integer, Decimal, Float, Boolean,
  Date, Time, DateTime, DateTimeTz, Opaque }`, wire names exactly Ossie's.
  `ColumnSchema { name, datatype: LogicalType, nullable: bool, physical:
  Option<String> }` where `physical` keeps the engine-specific spelling
  (`datetime[μs]`, `list[str]`) the way the Polaris converter keeps Iceberg's.
  `TableSchema(Vec<ColumnSchema>)`. Under `polars`: `LogicalType::from_polars
  (&DataType)` (`Int*`/`UInt*` → `Integer`, `Float*` → `Float`, `Decimal` →
  `Decimal`, `Utf8`/`Categorical` → `String`, `Date` → `Date`, `Datetime(_,
  None)` → `DateTime`, `Datetime(_, Some(_))` → `DateTimeTz`, `Time` → `Time`,
  `Boolean` → `Boolean`, else `Opaque` with `physical` set) and
  `to_polars()` for the nine concrete types. Under `arrow`: the same pair for
  arrow-rs (`DateTime` → `Timestamp(Microsecond, None)`, `DateTimeTz` →
  `Timestamp(Microsecond, Some("UTC"))`, `Date` → `Date32`, `Decimal` →
  `Float64` with a documented loss until precision is declared, `Time` →
  `Time64(Microsecond)`).
- `semantic.rs` — the Ossie-shaped model: `SemanticModel`, `Dataset`,
  `Field`, `Relationship`, `Metric`, `AiContext`, `Extensions`. Field
  `expression` is `Option<String>` here and serialises as the Ossie
  `{dialects: [{dialect: "ANSI_SQL", expression}]}` object, defaulting to the
  field name; `is_time: Option<bool>` with Ossie's default rule
  (`resolved_is_time()` returns true for temporal datatypes when unset).
  `Metric.expression` is the structured `MetricExpr { column, aggregation:
  Aggregation, filter: Option<Filter> }` and serialises as an ANSI_SQL string
  (`SUM(orders.revenue)`) with the structured form duplicated in the
  `BRIGHTFLOW` extension so the document round-trips losslessly. `Field` lists
  deserialise from either an array (Ossie JSON) or a map keyed by name (Lua
  and TOML authors), via a `deserialize_with`.
- `ext.rs` — Brightflow's extension vocabulary. `ColumnRole`, `Polarity`,
  `TimeGranularity` move here from `crates/brightflow-engine/src/data/config.rs`
  unchanged (including the hand-written `Deserialize` that accepts legacy
  `kpi`/`metric`, and the `ALL` lists). `ColumnExt { role: Option<ColumnRole>,
  is_kpi: Option<bool>, polarity: Option<Polarity>, label: Option<String> }`
  (Brightflow's display label; Ossie's `label` is a tag and is left unused).
  `DatasetExt { display_name, time_granularity, comparison_periods, doc:
  Option<DocFields> }` where `DocFields { id, number, title, body, timestamp,
  url_template }` replaces `DocDisplay`; `url_template` uses `{column}`
  placeholders so the Bluesky permalink is data (`https://bsky.app/profile/{author_handle}/post/{rkey}`).
  `MetricExt { expr: MetricExpr, is_kpi, polarity, format }`. Each has
  `to_extension()` / `from_extensions(&Extensions)` under `vendor_name =
  "BRIGHTFLOW"`, encoding `data` as the JSON string Ossie requires.
- `declaration.rs` — what a producer hands the store: `TableDeclaration {
  name, schema: Option<TableSchema>, primary_key, unique_keys, cursor_field,
  dataset: Option<Dataset>, relationships: Vec<Relationship>, metrics:
  Vec<Metric> }`.
- `provenance.rs` — `Layer { Detected, Declared, Agent, User }` with
  `PRECEDENCE` (highest first), `Provenance { layer, producer: String,
  version: Option<String>, hash: Option<String> }`.
- `validate.rs` — `SemanticModel::validate() -> Result<(), Vec<Violation>>`:
  unique dataset, field, relationship and metric names; relationships
  reference existing datasets and fields with equal column counts; metrics
  reference existing fields; primary and unique keys name declared fields
  when a schema is present. Pure, no I/O.

Before §2 starts, one check: Longbow will depend on this crate by path
(`../brightflow/crates/brightflow-types`) while this workspace depends on
Longbow by path, so the crate is reachable through two paths. Cargo
identifies path packages by canonical path, so it should resolve as one
package; confirm with `cargo tree -p brightflow-connect -i brightflow-types`
showing a single node once §2's dependency is added. If it does not, the
crate moves to its own repository before §2 rather than after.

All public types derive `Serialize`, `Deserialize`, `TS` (`#[ts(export)]`),
`JsonSchema`, `PartialEq`. The engine's `Cargo.toml` drops `schemars` if
nothing else there needs it; `scripts/generate-ts-barrel.sh` and the
`TS_RS_EXPORT_DIR` step point at this crate as the single source of generated
TypeScript for these types.

Tests (`#[cfg(test)] mod tests` per file): wire names pinned per enum; Polars
and arrow round-trips for the nine concrete types and `Opaque` with
`physical`; `Field` list from array and from map produce equal values;
`MetricExpr` renders `SUM(t.c)`, `COUNT(DISTINCT t.c)`, and a filtered
metric; extension encode/decode round-trips; `resolved_is_time()` per the
Ossie rule; each `validate()` violation kind. A fixture test serialises a
two-dataset model and checks it against `ossie-schema.json` vendored under
`crates/brightflow-types/tests/fixtures/` with a note of the commit it was
taken from.

## §1 — Litehouse typed against the contract (3 commits, Rust)

### 1a. Migration `027_layered_semantics.sql`

- `column_semantics`: rebuilt with primary key `(table_id, column_name,
  layer)`; columns `layer TEXT CHECK IN ('detected','declared','agent','user')`,
  `producer TEXT`, `producer_version TEXT`, `producer_hash TEXT`, `datatype
  TEXT` (logical), `is_time INTEGER` (nullable), `role`, `is_kpi` (nullable
  now: `NULL` means "no opinion at this layer"), `polarity` (nullable),
  `label`, `description`, `ai_context_json`, `extensions_json`, `updated_at`.
  Existing rows migrate by consulting `action_log`: a row whose
  `(table, column)` appears in an `applied` or `undone` action of kind
  `set_kpi`, `set_column_polarity`, `set_column_role`, `set_column_label` or
  `set_column_description` (`params_json` carries `scope` and `column`)
  becomes `layer = 'user'` (or `'agent'` when `actor_type = 'agent'`); every
  other row becomes `layer = 'declared'`, `producer = 'legacy'`. That puts the
  GitHub seed's rows and enrichment's rows under `declared`, where the next
  sync or enrichment run supersedes them, and keeps a person's edits on top.
  The committed test workspace is the concrete case: its `issues` table is a
  CSV ingested as `connector:sample`, and the seed matched it by name, so it
  carries 22 GitHub rows for columns it mostly does not have. Those migrate to
  `declared/legacy` and are replaced by the detector's rows on rebuild.
- `table_semantics` replaces `table_analysis_settings` with the same layer,
  producer and version columns plus `description`, `display_name`,
  `time_granularity`, `comparison_periods`, `ai_context_json`,
  `extensions_json`, `doc_json`.
- `relationships (id, source_id, name, from_table_id, to_table_id,
  from_columns_json, to_columns_json, layer, producer, producer_version,
  ai_context_json, extensions_json)`.
- `metrics (id, table_id, name, expr_json, datatype, description, is_kpi,
  polarity, format, layer, producer, producer_version, ai_context_json,
  extensions_json)`.
- `sources`: `kind CHECK IN ('upload','connector','web')`, plus `producer`,
  `producer_version`, `producer_hash`.
- Views `column_semantics_resolved`, `table_semantics_resolved`: per-field
  `COALESCE` across layers in precedence order. A blank string in a higher
  layer clears a lower layer's value; `NULL` means no opinion.

`crates/brightflow-store/src/models.rs` rows convert to and from contract
types at the door (`TryFrom<ColumnSemanticRow> for (Field, ColumnExt,
Provenance)` and back); the "stringly typed on purpose" header is rewritten
to say the migration is the schema and the contract crate is the vocabulary,
and a unit test asserts each `CHECK` list equals the crate's `ALL` list by
reading the migration file.

### 1b. Apply and export

`crates/brightflow-store/src/db/semantics.rs` (new; the semantic half moves
out of `catalog.rs`, whose header shrinks to the catalog half):

- `apply_declaration(&self, source_id, decl: &TableDeclaration, prov:
  &Provenance) -> Result<AppliedDeclaration>`: in one transaction, resolve or
  create the table row, delete this producer's rows at this layer for the
  table, insert the declaration's column, table, relationship and metric
  rows, and return counts. Declared columns that are absent from the table's
  Parquet (a connector `map` that emitted `nil` for every record) are stored
  anyway and reported in the counts as `columns_without_data`; the resolved
  view carries them and Explore shows only columns present in the frame, so
  they are harmless until the column appears. Relationships resolve
  `to_table_id` by name within the source; an unresolved target (the
  connector was run with `--only`, or the target endpoint has not synced yet)
  is skipped with a warning and counted, never an error, and is picked up by
  the next apply.
- `resolved_columns(table_id) -> Vec<(Field, ColumnExt)>` and
  `resolved_table(table_id)`, reading the views.
- `export_model(source_id) -> SemanticModel`: datasets from tables,
  fields from the resolved view, relationships and metrics; the caller
  serialises (`serde_json` is the Ossie JSON form; YAML is a one-line
  `serde_yaml` behind a feature if wanted later).
- The `ParquetStore` facade exposes the same; `upsert_column_semantic`,
  `delete_column_semantic`, `delete_all_column_semantics` and
  `insert_column_semantics_if_absent` are removed (the last one becomes a
  `Declared` apply from the enrichment producer, §5).

### 1c. Types at the catalog boundary

`ingest.rs::schema_to_json` writes `ColumnSchema` JSON (logical type plus
`physical`) instead of Polars `Display` strings; `register_file` writes it
too, so events tables stop having a `NULL` schema. `stats.rs::supports_min_max`
is expressed over `LogicalType`. `schema_has_text_column` and
`enrichment/validate.rs::table_columns` in the API read `ColumnSchema`.

Tests: apply then resolve (user label over declared description); a second
apply from the same producer replaces only its rows; a different producer's
rows survive; blank-string clearing; export round-trips through
`SemanticModel` and validates; migration of a pre-027 workspace copy
(`brightflow-test-support` has the fixture pattern).

## §2 — Longbow: declared types, passthrough, provenance (2 commits, Rust, `../longbow`)

- `Cargo.toml`: `brightflow-types = { path = "../brightflow/crates/brightflow-types",
  features = ["arrow"] }`. The dependency arrow is contract ← longbow ←
  brightflow-connect; there is no cycle.
- `pipeline.rs`: `Endpoint` gains `declaration: Option<serde_json::Value>`
  captured from the `p.endpoint` config keys `columns`, `description`,
  `ai_context`, `relationships`, `metrics`, `brightflow` (anything not in the
  existing seven keys is collected, not dropped). `EndpointResult` gains
  `declaration: Option<serde_json::Value>` and `type_errors: u64`.
  `RunResult.meta` is unchanged.
- `parquet.rs`: `write_parquet(records, declared: &[(String, LogicalType)])`.
  For a declared column, the Arrow type comes from
  `LogicalType::to_arrow()`; `DateTime`/`DateTimeTz`/`Date` parse ISO-8601
  strings (RFC 3339 with or without offset, plus a bare date) and count
  failures as nulls; `Integer`/`Float`/`Boolean` coerce from JSON numbers,
  bools and numeric strings; `Opaque` keeps inference. Undeclared columns keep
  today's inference. Field order: declared order first, then alphabetical.
  The module header records that declared beats inferred and that parse
  failure is a null and a count, not an error.
- `README.md` and `CLAUDE.md`: the endpoint reference gains `columns` with
  `datatype` and says every other key is passed through to the caller
  verbatim. `plan.md` is left as the dated v1 spec it is.

Tests in `parquet.rs`: each declared type from representative JSON values;
mixed valid and invalid timestamps yield nulls and the right count; declared
order precedes inferred order; an undeclared column is unchanged. In
`pipeline.rs`: the declaration survives from Lua to `EndpointResult`.

## §3 — brightflow-connect: typed declaration and provenance (1 commit, Rust)

`crates/brightflow-connect/src/lib.rs`: `ConnectorResult` gains `meta:
ConnectorMeta`; `EndpointResultInfo` gains `declaration:
Option<TableDeclaration>` (parsed from Longbow's JSON with the map-or-array
`Field` deserialiser; the `brightflow` sub-table on a column or dataset is
mapped into the `BRIGHTFLOW` extension) and `type_errors`. A parse failure of
a declaration is a `ConnectError` naming the endpoint and the key, since a
connector author wrote it and should see it. `provenance_for(&meta) ->
Provenance { layer: Declared, producer: "connector:{name}", version,
hash }`.

Tests: the existing Lua-evaluating test pattern (`lib.rs` tests) with a
connector declaring one endpoint, asserting the typed declaration and the
`BRIGHTFLOW` extension contents.

## §4 — Scheduler applies; GitHub declares; the seed goes (3 commits)

### 4a. Scheduler (Rust)

`crates/brightflow-scheduler/src/lib.rs::execute_sync`: after
`merge_parquet` for an endpoint with a declaration, call
`store.apply_declaration(&source_id, &decl, &prov)`; on the run's first
endpoint, `register_source(source_id, "connector", name, version, hash)`.
The two store calls are separate transactions; the window is accepted and
recorded in the function's comment. Relationships are applied after all
endpoints of the run, so the target table exists.

### 4b. `github.lua` and `bluesky.lua` (Lua)

Every column that the seed named gets a `columns` entry with `datatype` and,
where the seed said so, `brightflow = { role, is_kpi }`. `created_at`,
`updated_at`, `closed_at`, `merged_at` are `DateTime`. Datasets get
`description` and `brightflow = { time_granularity = "week",
comparison_periods = 4, doc = { id = "id", number = "number", title =
"title", body = "body", timestamp = "created_at", url_template = "{html_url}" } }`.
`issue_comments` declares `relationships = { { name = "issue", to =
"issues", from_columns = {"issue_number"}, to_columns = {"number"} } }`.
Bluesky's `posts` gets `indexed_at`/`created_at` as `DateTimeTz`, `doc.url_template`
with the permalink, and `profile_snapshot.snapshot_date` as `Date`.
Frontmatter `version` is bumped on both.

### 4c. Delete the seed and `DocDisplay` (Rust)

`seed_column_semantics` and its call in `bootstrap.rs` go. `DocDisplay::for_table`
becomes `DocFields` read from `resolved_table`, with the old defaults
(`id`, `created_at`) as the fallback when a table has none; the Bluesky
permalink function is replaced by `url_template` rendering in one pure
function with a test. `sources/profiles.rs` keeps keying tools off the
connector name, now read from `sources.producer` instead of the scheduler
database.

Integration: `crates/brightflow-scheduler/tests/` or the API's existing sync
test runs a stub connector declaring two endpoints and a relationship against
a temp workspace and asserts the resolved rows, the source row and the
relationship.

## §5 — Engine and enrichment read the contract (2 commits, Rust)

- `crates/brightflow-engine/src/data/config.rs` re-exports the three enums
  from the types crate and shrinks to `ColumnConfig`; `merge.rs`'s
  `ColumnOverride` becomes `(Field, ColumnExt)`; `DataSchema` gains
  `labels: HashMap<String, String>`, `descriptions`, `entity_columns`.
  `build_schema` fills them; `humanize_column` becomes the fallback only.
  `detect_schema` gains a sibling `detected_declaration(&DataFrame) ->
  TableDeclaration` (roles from today's rules, `is_time` from dtype or name,
  `datatype` from `LogicalType::from_polars`) so the detector is a producer.
- Enrichment: `OutputSemantic` in `enrichment/mod.rs` becomes `(Field,
  ColumnExt)`; `ticket_classify.rs` and `mentions.rs` build a
  `TableDeclaration` for the parent's new columns and one for the
  `{table}_mentions` child (its columns, primary key, and a relationship to
  the parent), applied by `runner.rs::materialize` through
  `apply_declaration` with `producer = "enrichment:{function}"`, `version` =
  the function version. The child table finally has semantics and the
  suffix convention has a row.

Tests: `build_schema` carries label, description and entity; `detected_declaration`
on the existing fixtures; enrichment declarations cover every output column
and the child's relationship.

## §6 — API and frontend on one wire type (3 commits)

This is the widest ripple in the plan and is sized accordingly. Changing
`ColumnInfo.dtype` from a free string to `LogicalType` touches every
frontend reader of `dtype`: `utils/dtype.ts` and its tests,
`composables/useOperators.ts` (operator sets by type), `utils/format.ts`
(number and date formatting), `stores/dataset.ts`, `stores/pivot.ts`
(default aggregation), `components/query/QueryBuilder.vue` (type icons),
`components/query/FilterBar.vue`, and the integration tier's fixtures. The
first commit is the Rust wire change with `dtype` kept as a deprecated
duplicate string; the second moves the frontend over; the third removes the
duplicate. Temporal filter operators (`before`, `after`, `between`) become
possible once `LogicalType` is on the wire but are not added here.

- `analytics/session.rs::ColumnInfo` becomes `{ schema: ColumnSchema,
  field: Option<Field>, ext: Option<ColumnExt> }` flattened for the wire;
  `dtype_to_string` is deleted in favour of `LogicalType` plus `physical`.
  `semantics/types.rs::ColumnSemantic` is deleted; `GET .../semantics` returns
  the resolved rows in the same shape plus, on request (`?layers=1`), the
  per-layer rows for a provenance view. `set_table_settings` joins
  `ACTION_KINDS`; the `PUT` route goes. The action executors write `user` or
  `agent` layer rows by `Actor`; undo restores that layer's row snapshot.
  A new `GET /api/sources/{source_id}/semantic-model` returns the Ossie JSON
  document (`export_model` + `serde_json`), the generated artifact the
  2026-09-08 report asked for.
- Frontend: regenerate TypeScript from the types crate; `utils/dtype.ts`
  reduces to `LogicalType` predicates (`isNumeric`, `isTemporal`); `stores/dataset.ts`,
  `pivot.ts`, `columnMenu.ts`, `paletteActions.ts` and `semanticLabels.ts`
  import the generated `ColumnRole`/`Polarity`/`LogicalType`. The column
  context menu shows the resolving layer ("from GitHub connector 0.2.0",
  "edited by you") as a hint line, which is the provenance badge the survey
  found only catalog vendors ship.

Tests: `dataset.test.ts` and `pivot.test.ts` updated for `LogicalType`;
`paletteActions` entries for `set_table_settings`; `action_log.rs` round trip
for a user edit over a declared row, asserting the resolved value before,
after, and after undo.

## Execution order & sizing

| Step | Commits | Depends on | Note |
|---|---|---|---|
| §0 | 2 | — | crate, enums moved, features, fixture validation |
| §1 | 3 | §0 | migration, apply/export, typed catalog boundary |
| §5 | 2 | §0, §1 | engine and enrichment as producers |
| §6 | 3 | §1, §5 | one wire type (staged), bus for settings, export route |
| §2 | 2 | §0 | Longbow (sibling repo), declared types, passthrough |
| §3 | 1 | §2 | connect parses and attaches provenance |
| §4 | 3 | §1, §3 | scheduler applies; `github.lua`; seed deleted |

§2 and §3 can proceed in parallel with §5 and §6 after §0 and §1. §4 is
last because it removes the seed, which the test workspace still relies on
until the connector declares. Each Rust commit: `cargo fmt --check`, `cargo
clippy`, `cargo test -p <crate>`, `cargo build --release`, TypeScript
regenerated and formatted. Longbow commits: its own `cargo test` and
`cargo clippy`. Each frontend commit: `npm run check`, `npm run test`.
After §1 and again after §4, `./scripts/build-test-template.sh` rebuilds the
committed test workspace and `--check` confirms it. The rebuild needs no
GitHub token: the script ingests CSVs and event fixtures only, and the one
connector-style source is `connector:sample`. Expect the `issues` table's
semantics to change from 22 seed rows to the detector's rows for the columns
the CSV actually has; that is the fix, not drift. The connector path
(§2–§4) is covered by the stub-connector integration test, not by the
committed workspace.

## Verification

- **Rust unit** — every §0 module; store apply/resolve/export and the
  `CHECK`-equals-enum test; engine `build_schema` and
  `detected_declaration`; enrichment declarations; Longbow declared-type
  parsing and ordering; connect parsing and provenance.
- **Rust integration** — stub-connector sync into a temp workspace; action
  round trip over a declared row; enrichment materialise declares parent and
  child; migration of a pre-027 workspace copy.
- **Ossie conformance** — the exported document of the test workspace
  validates against the vendored `ossie-schema.json` in a test, and the
  test names the schema commit so a later spec bump is a visible diff.
- **Frontend** — unit tiers as listed; integration tier (`TEST_INTEGRATION=1`)
  against the rebuilt workspace.
- **Manual, both servers as background tasks, on the test workspace:**
  1. Sync the GitHub connector. Explore on `issues`: `created_at` has a
     calendar icon without anyone setting a role; `reactions_total` shows the
     KPI badge; hover on a column shows the connector's description and
     "from GitHub connector".
  2. Rename `reactions_total` to "Reactions" in the context menu. Re-sync.
     The label survives, the description is still the connector's.
  3. Bump `github.lua`'s description for `comments`, re-sync: the new
     description shows; the user's label still wins.
  4. Upload a CSV with an ISO date column: it lands as `DateTime`, buckets
     by period in Explore with no role edit.
  5. Run classify on `issues`: `issues_mentions` appears with labelled
     columns and a relation to `issues` in the semantics view.
  6. `GET /api/sources/{id}/semantic-model` returns a document that passes
     `validate.py` from the Ossie repo.

## Out of scope

- **The operations language in the types crate.** Filter, group, pivot,
  `withColumns` stay in `brightflow-api`; they move when the LLM starts
  emitting them.
- **Moving Longbow onto Polars.** Feature-gated adapters cover the type
  contract; the inference rewrite is a separate decision.
- **Metric kinds beyond (column, aggregation, filter).** Ratios, and any
  expression language, wait for the modelling layer.
- **Explore consumers for relationships and metrics** (a join in the
  executor, a metric picker). The tables land here with the export and the
  enrichment child relation as their first consumers; the Explore consumers
  are the next plan, and by the consumer-first rule they should follow
  closely.
- **LLM context bundle and agent authoring.** The export document is the
  bundle's input; wiring it into the three prompt paths and adding the
  `set_column_*` tools to a run kind is a separate change.
- **Connector-upgrade diff UI.** A producer re-declaring rewrites its own
  rows silently; showing what changed since the user last looked is later.
- **Snapshots and time travel** for semantic rows.
- **Cursor values into Litehouse.** They stay in the scheduler database.
- **Renaming `brightflow-types`** or moving it out of the workspace.

## Open questions

1. **`Decimal` in Longbow.** Ossie leaves precision unspecified; arrow-rs
   needs it. Default above is `Float64` with a documented loss. Alternative:
   accept `precision`/`scale` keys on the column declaration and write
   `Decimal128`.
2. **Layered storage: one row per layer, or one row with per-field owner
   columns** (the Kubernetes shape)? One row per layer is simpler to reason
   about and to undo; per-field owners make "who set this label" one lookup.
   Default is one row per layer.
3. **Should a user be able to pin a field so a future connector version
   cannot change it** (Looker's `final`)? With user > declared precedence a
   user value already wins; the question is only about fields the user has
   *not* touched. Default: no pinning.
4. **Where the detector's rows are written for events tables**, which are
   registered per file rather than created once: at first `register_file`
   for the table, or lazily on first `load_table`. Default: first register.
5. **ISO parsing strictness in Longbow** for declared `DateTime`: accept
   `2024-01-15T10:30:00Z`, `2024-01-15T10:30:00+02:00`, `2024-01-15
   10:30:00`, and `2024-01-15`. Anything else is null. Is a bare date into a
   `DateTime` column acceptable, or should it be a type error?
6. **Contract version.** Should `TableDeclaration` carry a version of the
   types crate's schema, so a Longbow built against a newer contract is
   detected by an older Litehouse? Default: yes, one integer, checked in
   `apply_declaration`.

## References

**Ossie (Apache, incubating)**
- Core spec YAML: https://github.com/apache/ossie/blob/main/core-spec/spec.yaml
- JSON Schema: https://github.com/apache/ossie/blob/main/core-spec/ossie-schema.json
- Prose spec (type versus role, custom extensions, version history): https://github.com/apache/ossie/blob/main/core-spec/spec.md
- Roadmap (extended metadata, catalog integration, query language): https://github.com/apache/ossie/blob/main/ROADMAP.md
- Extended Metadata proposal (polarity, default aggregation, display): https://github.com/apache/ossie/issues/100
- Metrics versus measures: https://github.com/apache/ossie/discussions/29
- Templating instead of plain YAML: https://github.com/apache/ossie/discussions/62
- Converters, hub-and-spoke: https://github.com/apache/ossie/blob/main/converters/README.md
- Polaris converter (decomposes into Iceberg schema and properties): https://github.com/apache/ossie/tree/main/converters/polaris

**Source-shipped semantics and layering**
- Singer discovery mode (discoverable versus non-discoverable metadata): https://github.com/singer-io/getting-started/blob/master/docs/DISCOVERY_MODE.md
- Airbyte protocol (`AirbyteCatalog` versus `ConfiguredAirbyteCatalog`): https://docs.airbyte.com/platform/understanding-airbyte/airbyte-protocol
- Airbyte data types (`airbyte_type`): https://github.com/airbytehq/airbyte/blob/master/docs/platform/understanding-airbyte/supported-data-types.md
- dlt schema contracts and import/export schema: https://dlthub.com/docs/general-usage/schema-contracts , https://dlthub.com/docs/walkthroughs/adjust-a-schema
- Fivetran dbt GitHub package (source package deprecated, merged): https://github.com/fivetran/dbt_github
- LookML refinements: https://docs.cloud.google.com/looker/docs/lookml-refinements
- dbt-osmosis inheritance: https://z3z1ma.github.io/dbt-osmosis/docs/tutorial-yaml/inheritance
- Kubernetes server-side apply field management: https://kubernetes.io/docs/reference/using-api/server-side-apply/
- DataHub editable aspects: https://datahubproject.io/docs/metadata-modeling/metadata-model/
- Snowplow SchemaVer: https://docs.snowplow.io/docs/fundamentals/schemas/versioning/

**Types in files and catalogs**
- Arrow canonical extensions and metadata: https://arrow.apache.org/docs/format/CanonicalExtensions.html
- Parquet format (LogicalType, KeyValue): https://github.com/apache/parquet-format/blob/master/src/main/thrift/parquet.thrift
- DuckLake column tags: https://ducklake.select/docs/stable/specification/tables/ducklake_column_tag
- Polars file-level Parquet metadata: https://docs.pola.rs/api/python/dev/reference/api/polars.read_parquet_metadata.html
- arrow-schema crate: https://docs.rs/arrow-schema

**LLM-authored semantics**
- Snowflake verified queries (`verified_by`, `verified_at`): https://docs.snowflake.com/en/user-guide/views-semantic/verified-query-repository
- Snowflake semantic model generator (`<FILL-OUT>` and auto-generated markers): https://github.com/Snowflake-Labs/semantic-model-generator
- Databricks AI-generated comments: https://docs.databricks.com/aws/en/comments/ai-comments
- Wren AI MDL: https://docs.getwren.ai/oss/reference/mdl

**Prior work in this repo**
- `reports/2026-09-08_litehouse-as-semantic-layer.md`
- `reports/2026-09-08_data-platform-completeness-and-operations-language.md`
- `reports/2026-09-12_tool-platform-or-workbench.md` (the layering of primitives, semi-primitives and tools; its section 6 on publishing is not a concern of this plan)
- `plans/2026-09-12_semantics-consumer-and-period-bucketing.md`
