# The Modelling Layer and the Transformation Language: What Each Is, What Exists, and the Routes

**Date:** 2026-09-13
**Scope:** The one structural gap every review since August has ranked
first: there is no way to define a named, saved, derived table. Three
questions. Are "modelling" and "transformation" the same thing? What does
the code offer today towards either? Which route should the first cut
take, given that every operation must be performable by a person and by
an agent through the same path.
**Method:** Four parallel research passes at the baseline below: the
contract crate and the semantic model as they stand after the September
migration; the store, scheduler and enrichment mechanics a model would
reuse; the query language, saved views and agent surfaces; and outside
precedent (dbt, SQLMesh, Metabase, Rill, Malloy, Cube, Lightdash, Omni,
Databricks metric views, DuckLake 1.0, Apache Ossie, and the small
transform DSLs). Every code claim names a file; every outside claim has a
source at the end, marked *(search summary)* where only a snippet was
seen. No code changed.
**Baseline:** HEAD `3698b6a`. Builds on
`reports/2026-09-08_data-platform-completeness-and-operations-language.md`
(which proposed growing the operation chain into the modelling language)
and `reports/2026-09-12_tool-platform-or-workbench.md` (the substrate rule:
a tool belongs if it reads tables plus semantics and writes back columns,
tables or findings). Both conclusions survive; this report makes them
concrete against the code that landed since.

---

## 1. Three words, three slots

Every tool in the comparison set ends up with the same three slots,
whatever it calls them:

| Slot | What it is | dbt | Metabase | Rill | Databricks |
|---|---|---|---|---|---|
| **Transformation** | The computation: filter, join, derive, aggregate | a model's SQL | transform / query-builder | model SQL | Lakeflow pipeline |
| **Model** | A named, saved output of a transformation, with a place in the catalog; virtual or materialised | `view` / `table` / `incremental` | model, transform | model (view by default) | table, view |
| **Semantic** | What a table and its columns mean; metrics defined over a table | semantic model YAML | metrics | metrics view | metric view |

So the answer to the first question is no, they are not the same, but
they are two halves of one feature. A transformation is a verb; a model is
the noun it produces. The semantic layer sits on top of both, and a metric
is defined over a model exactly as it is over a raw table.

In Brightflow's terms: the operation chain in
`crates/brightflow-api/src/analytics/types.rs` is the transformation
language; Litehouse's layered semantics
(`crates/brightflow-store/migrations/027_layered_semantics.sql`) is the
semantic layer; the model is the missing noun. A table that knows its
recipe.

The second recurring pattern is a **promotion path**: Metabase goes saved
question → model → transform; Omni goes workbook → shared model → dbt;
Lightdash goes virtual view → dbt. Brightflow's version is saved view →
model → materialised model, and the first step shipped in `b16ff5b`.

## 2. What the code offers today

Verified at the baseline, one line each, with the seam a model would hit.

**Transformation language** (`analytics/types.rs:44-125`,
`analytics/executor.rs`)
- Seven operations: filter, select, groupBy, pivot, sort, limit,
  withColumns. One derived expression, `period`. Temporal filters compare
  dates since `cc171f1`.
- No join anywhere in the API or engine crates. No source reference:
  `Query.dataset_id` names an in-memory session registered by
  `load_table`, not a catalog table. No compound filters (and/or trees).
- `Operation`, `Query`, `DerivedExpr` derive `Deserialize` and `TS` only.
  **No `Serialize`, no `JsonSchema`.** A chain cannot be stored typed, and
  cannot be handed to an agent as a tool. `Aggregation` and `FilterOp`,
  which live in the contract crate, have both.
- Pivot collects eagerly; everything else stays lazy. The executor reads
  every file of a table unpruned; the catalog's partition and stats pruning
  (`store/src/scan.rs`) is used only by the web-analytics events path.

**Catalog** (`crates/brightflow-store`)
- `tables` has `id, name, source_id, version, schema_json, primary_keys,
  partition_columns, total_rows` (`models.rs:17-28`). **No kind, no
  recipe, no inputs.** A derived table would be indistinguishable from a
  synced one.
- Write paths take a file (`ingest_parquet`, `merge_parquet`) or require
  the table to exist (`replace_table_data`). **No DataFrame-taking create
  path.** Enrichment's child table writes a scratch Parquet and ingests it
  (`enrichment/runner.rs:1192-1231`).
- `sources.kind` is CHECK-constrained to `upload | connector | web`, and
  `tables.source_id` is a caller-supplied string with no FK. A model can be
  written under an existing source's id with no schema change and inherit
  its Explore page, relationships and export.

**Versioning pattern** (enrichment, `migrations/016`, `runner.rs:881-1010`)
- Immutable config snapshots per version, a content-hashed cache keyed on
  spec hash and input hash, materialise-from-cache as a separate verb,
  output semantics declared under producer `enrichment:{fn}` after the
  data lands, drop-columns on delete. This is a model in all but name and
  is the pattern to copy, minus the LLM cache.

**Scheduling** (`crates/brightflow-scheduler`, `api/src/bootstrap.rs:178-197`)
- One job kind, connector sync. The post-sync hook runs detection, then
  spawns enrichment and insights **concurrently**; insights usually run
  before the enrichment materialisation of the same table. There is no
  ordering to slot a rebuild into yet. `apply_declarations` is the one
  explicit "data before meaning" step.

**Semantic model** (`crates/brightflow-types`)
- `Metric` is column plus aggregation plus equality filters (`MetricExpr`,
  `ext.rs:372-390`), never executed: the engine uses it to mark a column a
  KPI and to boost a series that happens to match (`scoring.rs:133-190`).
  Web-analytics visitors, pageviews and bounce rate are still inline
  Polars (`web_analytics/queries.rs:38-97`).
- Ossie's `Field.expression` is silently dropped on apply
  (`resolved.rs:81-98`); the contract renders SQL from structured
  expressions and never parses it. Provenance's producer is a free string,
  so `model:{name}` at the Declared layer needs no schema change.
- The contract crate's own header says the operations language "is the
  next candidate and is deliberately not here yet" (`lib.rs:25-27`).

**Saved views** (`b16ff5b`)
- Spec stored as the client's opaque JSON; applied client-side by writing
  into the Pinia stores. The spec is a strict superset of what
  `buildPivotOperations` needs, so a view **is** compilable to a chain, but
  the only compiler is TypeScript. Actions `save_view`, `rename_view`,
  `delete_view` exist with undo; none is in any agent's tool list.

**Connectors** (Longbow)
- The Lua `map` is one record in, one record out, per page. No reduce,
  join or cross-endpoint stage. An endpoint without an HTTP path is not
  representable. Connectors cannot declare a derived table.

**Agents** (`api/src/agent`)
- Six run kinds; tools are slices of the action manifest, schemas derived
  by schemars from `Action`; `auto_apply` is now the default and undo is
  the safety net. An agent cannot emit a query or a chain today, only
  actions.

## 3. Outside precedent, what matters from it

- **Virtual is the field's default** (dbt `view`, SQLMesh `VIEW`, Rill
  views, Metabase models run live), with materialisation as a per-model
  switch. Incremental strategies where they exist: append, merge by key,
  partition replace, time window; SQLMesh's processed-interval bookkeeping
  is the strongest design for the last.
- **DuckLake 1.0** stores views as SQL text in the catalog, snapshot
  versioned; materialised views are on the roadmap with no version.
  Nothing else new in 2026 on views over Parquet with a SQLite catalog.
- **Expression sublanguages:** trees where validation matters (MBQL's
  bracketed clauses, Polars' `Expr`, Ibis nodes), a tiny text formula
  wherever humans read them (Metabase formulas, Vega `calculate`, PRQL
  `case`), SQL snippets where the tool is SQL-native (Cube, Malloy's
  escape hatch). Polars' own serialised plan is explicitly unstable across
  versions and its JSON form is deprecated; never a storage format.
- **Agent-authored transformations in 2026** are structured operators with
  a data preview and an explicit accept (dbt Canvas, Lakeflow Designer);
  metrics are YAML the user accepts (Hex, Lightdash); ad-hoc queries stay
  SQL text (Snowflake Cortex, PostHog). dbt's April benchmark: structured
  queries fail by refusing, SQL fails with plausible wrong answers.
- **Small transform DSLs** (VRL, Bloblang, tremor-script, Vega expressions)
  all exclude the same things: unbounded loops, user-defined functions as
  values, side effects. The tools that embed full Lua pay in sandbox CVEs.
- **Ossie** has datasets, relationships and metrics with dialect-tagged SQL
  strings, no derived datasets and no dependencies, but a dataset's
  `source` may be a query. A model exports as a dataset whose source is
  rendered SQL; the metric-language working group has not delivered.

## 4. The constraint that shapes everything: one bus, two actors

Every operation in this report is an action kind on the bus, dispatched
identically by a person and by an agent, logged with its actor, undoable.
This is not a new requirement; it is `docs/human_ai_interaction.md`
principle one, and the agent runner already builds its tools from the
action manifest. It has three consequences for the design:

1. **The model recipe is a data structure, not text.** An agent emits it
   through a schemars-derived tool schema, the UI builds it from bucket
   state, and both land in the same `create_model` action. SQL text would
   need a parser before either the manifest or undo could see inside it.
2. **Every model action has an undo.** Create → drop the output table and
   the recipe; update → restore the previous version and rebuild;
   delete → recreate under the original id. A rebuild is idempotent and
   needs no undo. With `auto_apply` the default, undo is the whole safety
   story, so the recipe's version history is not optional.
3. **The agent needs the same preview a person gets.** The 2026 pattern
   is operators plus a data preview plus accept; here accept is implicit
   and undo is explicit, so the preview is a query over the chain before
   materialisation, which the executor can already run.

## 5. Routes

**A. Models as materialised chains.** A `models` table holds name,
source, inputs, the chain, and immutable version snapshots on the
enrichment pattern. A rebuild executes the chain, writes the output as an
ordinary table under the same source id, declares its column semantics
under producer `model:{name}`, and carries labels and descriptions through
from inputs where a column passes unchanged. Rebuilds run from the
post-sync hook in dependency order, before enrichment and insights. Every
reader sees a plain table and needs no change. This is what the 2026-09-08
report chose; the research strengthens it, because the workbench rule says
tools talk only through tables and single-node full refresh is cheap.

**B. Virtual-first, materialise as a switch.** The field's default. It
avoids a write path and scheduling, but every reader in Brightflow uses
`read_table`: the engine, enrichment, sampling, describe. A virtual model
needs a second read path in each. Not a first cut; a later switch on top
of A.

**C. SQL text models.** Rejected in September and the reasons hold: the
Polars SQL context is partial, a tree gives validation, undo and agent
authoring for free, and the contract crate already treats SQL as rendered
output. The dbt benchmark adds the failure-mode argument.

**D. Lua in Longbow.** The map is per record; the DSL survey shows every
purpose-built mapping language excludes exactly what full Lua brings.
Connectors remain the door in.

**E. A tiny text formula surface.** Everyone whose humans read a tree adds
one. A phase-two authoring surface that parses into the tree, never the
stored form.

## 6. Recommendation

Route A, in three bounded pieces, each usable on its own.

**Piece 1, the noun (about a week).** `models` and `model_versions`
tables; `Operation` gains `Serialize` and `JsonSchema` and moves to the
contract crate as its header anticipates; a Rust twin of the view-to-chain
compiler; a DataFrame-taking create path in the store; actions
`create_model`, `update_model`, `delete_model`, `rebuild_model` with undo;
"Save as model" in Explore from any pivot or table state; the output
appears as a table with a model badge and its recipe on the Semantics
page; rebuild after every sync, in input order, ahead of enrichment and
insights. No join, no expressions. This alone is "save this Explore view as
a table that refreshes".

**Piece 2, the language (about a week).** A source reference; join with
default keys from `relationships`; compound filters; an expression tree of
about eight functions named after Polars (arithmetic, comparison, case,
coalesce, concat, lower, cast). Explore gets the join and derived-column
steps; the agent gets the same tool schema.

**Piece 3, metrics come home (days).** `count_distinct` joins
`Aggregation`; web-analytics visitors, pageviews and bounce rate become
declared metrics over an events model and the handlers read them; funnels
and retention stay hand-written until the language has windows.

Later, only if pulled: incremental strategies (append, merge by key, time
window), a virtual switch, the text formula surface, Ossie export of a
model as a query-sourced dataset.

## 7. Decisions to take before a plan

1. **Materialised only in piece 1, or virtual with a switch from the
   start?** Recommendation: materialised only. Every reader stays
   untouched, and the switch can be added when a model is too large or too
   volatile to keep on disk.
2. **Where does the recipe live in the catalog?** A `models` table
   pointing at the output `table_id`, leaving `tables` as it is, or a
   `kind` column on `tables`. Recommendation: the `models` table; a table
   stays a table, and a model is one table's producer.
3. **Which source id does a model belong to?** Recommendation: the source
   of its first input. A cross-source model is possible later and would
   need a source kind of its own, which is a CHECK rebuild.
4. **Does a model rebuild block the sync hook or spawn?** Recommendation:
   block, in topological order, so enrichment and insights see the
   rebuilt table. That also fixes the existing race between enrichment
   and insights, which should be ordered at the same time.
5. **Does the agent get `create_model` from day one?** Recommendation:
   yes, in the `describe_table` run's tool list first, since that run
   already reads the whole table and could propose a cleaned model, with
   `auto_apply` and undo as everywhere else.

## Sources

Outside claims, fetched unless marked *(search summary)*.

- dbt materializations: https://docs.getdbt.com/docs/build/materializations
- dbt semantic models: https://docs.getdbt.com/docs/build/semantic-models
- dbt incremental strategies: https://docs.getdbt.com/docs/build/incremental-strategy
- dbt semantic layer vs text-to-SQL benchmark (2026-04): https://docs.getdbt.com/blog/semantic-layer-vs-text-to-sql-2026
- dbt Canvas Copilot: https://docs.getdbt.com/docs/platform/build-canvas-copilot
- SQLMesh model kinds: https://sqlmesh.readthedocs.io/en/stable/concepts/models/model_kinds/
- SQLMesh virtual data environments: https://www.tobikodata.com/blog/virtual-data-environments
- Malloy sources: https://docs.malloydata.dev/documentation/language/source.html
- Malloy expressions: https://docs.malloydata.dev/documentation/language/expressions
- Metabase models: https://www.metabase.com/docs/latest/data-modeling/models
- Metabase transforms: https://www.metabase.com/docs/latest/data-studio/transforms/transforms-overview
- Metabase metrics: https://www.metabase.com/docs/latest/data-modeling/metrics
- Metabase expression schema (MBQL): https://github.com/metabase/metabase/blob/master/src/metabase/lib/schema/expression/conditional.cljc
- Rill models: https://docs.rilldata.com/reference/project-files/models
- Rill incremental models: https://docs.rilldata.com/developers/build/models/incremental-models
- Rill metrics views: https://docs.rilldata.com/reference/project-files/metrics-views
- Cube views: https://docs.cube.dev/reference/data-modeling/view
- Cube measures: https://docs.cube.dev/reference/data-modeling/measures
- Lightdash virtual views: https://docs.lightdash.com/semantic-layer/virtual-views
- Lightdash metrics: https://docs.lightdash.com/references/metrics
- Omni modeling layers: https://docs.omni.co/modeling
- Databricks metric views: https://docs.databricks.com/aws/en/uc-semantics/metric-views/
- Databricks metric view YAML: https://docs.databricks.com/aws/en/uc-semantics/metric-views/yaml-reference
- Lakeflow Designer *(search summary)*: https://www.databricks.com/blog/announcing-public-preview-lakeflow-designer
- Snowflake Cortex Agents over semantic views: https://docs.snowflake.com/en/release-notes/2026/other/2026-04-13-cortex-agents-agentic-analyst
- Hex semantic authoring: https://hex.tech/blog/introducing-semantic-authoring/
- DuckLake 1.0: https://ducklake.select/2026/04/13/ducklake-10/
- DuckLake catalog tables: https://ducklake.select/docs/stable/specification/tables/overview
- DuckLake roadmap: https://ducklake.select/roadmap
- Vega-Lite calculate: https://vega.github.io/vega-lite/docs/calculate.html
- Vega expression language: https://vega.github.io/vega/docs/expressions/
- Polars LazyFrame serialize (stability note): https://docs.pola.rs/api/python/stable/reference/lazyframe/api/polars.LazyFrame.serialize.html
- Polars dsl-schema feature: https://github.com/pola-rs/polars/blob/main/crates/polars-plan/Cargo.toml
- Ibis internals: https://ibis-project.org/concepts/internals
- PRQL case: https://prql-lang.org/book/reference/syntax/case.html
- Apache Ossie spec: https://github.com/apache/ossie/blob/main/core-spec/spec.md
- Ossie working groups: https://www.snowflake.com/en/blog/apache-ossie-open-semantic-interchange-incubator/
- Vector VRL design: https://github.com/vectordotdev/vrl/blob/main/DESIGN.md
- Bloblang: https://warpstreamlabs.github.io/bento/docs/guides/bloblang/about/
- tremor-script: https://www.tremor.rs/docs/0.11/tremor-script/
- Redis Lua sandbox: https://redis.io/docs/latest/develop/programmability/eval-intro/
- Semantic-doc study (arXiv 2604.25149): https://arxiv.org/abs/2604.25149
- Prior reports: `reports/2026-09-08_data-platform-completeness-and-operations-language.md`,
  `reports/2026-09-12_tool-platform-or-workbench.md`,
  `reports/2026-09-13_capabilities-and-structure-compared.md`
