# Is the Stack a Data Platform? Gaps, and the Operations Language as Modelling Layer

**Date:** 2026-09-08
**Scope:** Two linked questions. First: given Longbow for connector-based
extraction, Rust + SQLite + Polars + Parquet for storage and compute, Litehouse
as catalog and semantic layer, and Vue + ECharts for presentation, is anything
missing for a complete data platform? Second: the Explore tool already sends the
backend a typed chain of operations that compiles to Polars. Can that chain grow
into the transformation and modelling language, instead of adopting SQL?
**Method:** Static review of this repo (the `Operation` enum and executor, the
scheduler, the store's write paths, the insights engine's own algebra, Cargo
features of the pinned Polars), plus web research into whether anyone has built a
transformation language over Polars and into the structured query languages that
BI tools and Google have converged on. Every recommendation is then checked
against the project's stated design principle: pick a few strong primitives
(Rust, Tokio, Axum, SQLite, Polars, Parquet, Vue, Nuxt UI, ECharts) and lean on
their strengths rather than build around them. Claims about Polars features were checked
against the pinned crate source in the local Cargo registry. No code changed.
**Baseline:** HEAD `6946e4d`. Companion report, written the same day:
`reports/2026-09-08_litehouse-as-semantic-layer.md`. This report assumes its
conclusion (Litehouse holds the semantic layer; consumer first, schema second)
and builds on it.

---

## 1. The stack, layer by layer

Measured against the conventional layers of a data platform:

| Layer | What Brightflow has | Verdict |
|---|---|---|
| Extract and load | Longbow connectors as Lua sources (`crates/brightflow-connect`), plus first-party event, track, and identify HTTP endpoints with a SQLite buffer flushed to Parquet (`crates/brightflow-api/src/ingest`) | Covered |
| Storage | Parquet files, UUIDv7-named, never edited in place; SQLite index with per-file stats and partitions (`crates/brightflow-store`) | Covered |
| Compute | Polars LazyFrame, single node, in-process (`crates/brightflow-api/src/analytics/executor.rs`) | Covered for the intended scale |
| Catalog | Litehouse `tables`, `table_files`, stats, partitions | Covered, without snapshots |
| Semantic layer | `column_semantics`, `table_analysis_settings`, `taxonomy_categories`; unfinished per the companion report | Partially covered |
| Transformation and modelling | Lua `map` at extract time; LLM enrichment functions; Polars expressions inline in handlers | **Missing as a layer** |
| Orchestration | 30-second tick over `scheduler_jobs` with `interval_secs`, one post-sync hook (`crates/brightflow-scheduler/src/lib.rs`) | Minimal, no dependencies |
| Quality and observability | None: no freshness, row-count, null-rate, or schema-drift checks | **Missing** |
| Governance | Session auth with rate limiting (`crates/brightflow-api/src/auth`); migration 005 dropped `is_admin`, so no roles; no PII tagging or retention | Minimal |
| Serving | Axum HTTP + WebSocket, typed via `ts-rs` | Covered for the app; no outward door |
| Presentation | Vue 3, Nuxt UI, ECharts: Explore pivot, insights feed, text enrichment, web-analytics dashboard | Covered; nothing saved or shared |
| Agent and LLM | Provider-agnostic client, action bus with undo, agent runs, enrichment | Covered, ahead of most platforms |

The vertical is complete for a single-node analytics product. The gaps are the
connective tissue between layers, and one of them, modelling, is structural.

## 2. The gaps, ranked

1. **Transformation and modelling.** There is no way to define a cleaned table,
   a join, or an aggregate as a saved, named table registered in Litehouse.
   Derived data exists in three unrelated forms: connector-side Lua (`label_names`
   flattened to a comma-joined string in `github.lua`), versioned LLM enrichment
   functions, and Polars expressions written directly into the web-analytics
   handlers (visitors, pageviews, bounce rate). Without a modelling layer the
   semantic layer can only describe raw connector output. Section 4 is about
   this gap.
2. **Dependency-aware orchestration.** Jobs are independent and interval-driven.
   Once models exist, sync, model, enrichment, and insights form a chain that
   needs ordering and incremental triggering. The post-sync hook is the seed.
3. **Data quality and observability.** Nothing checks freshness, row-count
   anomalies, null rates, or connector schema drift. The engine already has the
   deterministic statistics to do this cheaply, and the philosophy doc's model of
   "the engine proposes findings into the feed" fits it exactly.
4. **Versioning, time travel, backups.** Overwrite and merge physically delete
   replaced files (`ingest.rs`); the `version` counter is optimistic concurrency
   only; a backup is a folder copy. Acceptable for one user, not once other
   systems depend on the data. This is the DuckLake half the companion report
   deferred; it moves up as soon as other systems start reading the folder.
5. **An outward door.** No SQL endpoint, no Arrow Flight or ADBC, no CSV or
   Parquet export, no API for other tools. This gap mostly dissolves under the
   primitives principle: storage is plain Parquet in a folder and the catalog is
   SQLite, both open formats that DuckDB and every other engine read today.
   Interoperability comes from the primitives, not from a serving layer.
   Documenting the folder layout as a contract is the whole job; a read-only SQL
   endpoint is a later option only if a consumer cannot read files.
6. **Saved artifacts.** Explore state is ephemeral; the frontend router has no
   saved query or dashboard route. Expected for anything BI-shaped.
7. **Governance details.** No PII flag, no column-level access, no retention or
   deletion for event rows carrying visitor ids. A PII flag belongs in
   `column_semantics` and rides on the semantic-layer work.
8. **Outbound alerts and lineage.** Insights live only in-app. Lineage from sync
   run to file to enrichment version is partial and falls out of a modelling
   layer whose rows record their inputs.

**Not a gap.** Single-node compute is a deliberate choice. The pinned Polars
0.48.1 offers `sql`, `serde`, `serde-lazy`, and `new_streaming` features and this
workspace enables none of them (root `Cargo.toml`). The primitives principle
says to enable a primitive's capability before building around it: streaming
when tables outgrow memory, schema inference from the lazy plan for validation,
possibly `serde` as an in-process cache. It also says which feature not to lean
on. The SQL context is Polars' weak side, a partial dialect with known
window-function gaps, so SQL as the modelling language would lean on a weakness.

## 3. The operations language as it stands

`crates/brightflow-api/src/analytics/types.rs` defines a `Query` as a
`dataset_id` plus an ordered `Vec<Operation>`. The module doc gives the reason:
"the query builder lets users compose filter/group/sort/limit in any order and
the result depends on that order." Six operations exist:

| Operation | Shape | Polars mapping in `executor.rs` |
|---|---|---|
| `Filter` | one column, one `FilterOp` (eq, ne, gt, gte, lt, lte, contains, in, isNull, isNotNull), one JSON value | `lf.filter(expr)` |
| `Select` | column list | `lf.select` |
| `GroupBy` | `by` columns, `AggSpec { column, function, alias }` with count/sum/avg/min/max/median/std/first/last | `lf.group_by().agg()` |
| `Pivot` | index, columns, values, optional agg | collect, `pivot_stable`, back to lazy |
| `Sort` | one column, descending flag | `lf.sort` |
| `Limit` | n | `lf.limit` |

Three properties matter for what follows. The enum is shared with the frontend
through `ts-rs`-generated TypeScript, so both sides speak it by construction.
The executor is already a compiler from the enum to a `LazyFrame`, so the
Polars optimiser handles pushdown. And it is a data structure, which the pivot
store builds programmatically today.

A second algebra lives in the insights engine (`crates/brightflow-engine/src/analysis/candidates.rs`):
`MeasureRef` (row count or column), `Aggregation` (count, sum, mean), and
`Derivation` (delta, percent change, share of total, rank among siblings). It
operates on DataFrames directly and never meets the `Operation` enum.

## 4. Can the operations language become the modelling language?

### 4a. The case for

- **It exists, is typed end to end, and needs no parser.** Adopting SQL would
  mean a parser and a dialect. Polars' SQL context is a partial dialect with
  known window-function gaps, and it is not enabled in this build anyway.
- **A data structure beats text for every consumer this product has.** The UI
  builds it. The LLM can emit it against a JSON schema, which the agent runner
  already derives for actions with `schemars`. It can be validated against the
  semantic layer before execution, stored as rows in Litehouse, versioned like
  `enrichment_function_versions`, diffed, and undone through the action bus.
  SQL text offers none of that without first parsing it.
- **Semantic objects are fragments of it.** A named measure is a column, an
  aggregation, and an optional filter. A model is a chain with a source. One
  vocabulary then serves Explore, saved models, and the semantic layer.
- **It unifies the engine's algebra.** If insights emitted chains, "open this
  finding in Explore" would fall out, and the two measure notions the companion
  report flagged (declared roles for insights, dtype-derived for Explore) would
  finally meet.
- **GUI first.** A pipe language maps one to one onto a step-based UI, and each
  prefix of a chain is itself a valid query. Round-tripping a GUI with SQL text
  is a well-known hard problem; here the UI and the language are one thing.

### 4b. What the language lacks for modelling

It is a query language over a loaded session, not yet a transformation language
over the catalog. In order of need:

1. **A source reference.** `dataset_id` names an in-memory session. Modelling
   needs `From { source, table }` or `From { model }`.
2. **Derived columns.** No expression node exists. Minimum: arithmetic, string
   operations, date truncation, case-when, coalesce. Date truncation first,
   because time granularity is already a semantic setting.
3. **Join.** None. The parent-to-`{table}_mentions` relation and identity
   resolution both need it; the relationships table proposed in the companion
   report supplies default keys.
4. **Compound filters.** One column and one operator per filter today; no AND or
   OR trees; no filter on aggregated output.
5. **Window and set operations.** The engine's four derivations belong here, as
   does union for events across sources.
6. **Reshaping.** Unpivot, explode (the inverse of the connector's comma-join),
   distinct, rename, cast.
7. **Semantic references.** Aggregate by measure name rather than by raw column
   and function.

### 4c. Has anyone built this on Polars?

No. The space has three layers and none is an established language.

| Layer | What exists | Why it is not the answer |
|---|---|---|
| Polars' own serialised plan | Rust `DslPlan`, 22 variants, hash-versioned behind the `serde` feature; a `dsl-schema` feature emits its JSON Schema; Polars Cloud uses it for transport between pinned versions | Documented as not stable across Polars versions; JSON form deprecated with an open issue to remove it; Substrait closed as not planned |
| Python APIs compiling to Polars | Ibis (real lazy expression tree to LazyFrame; several benchmark queries fail for lack of scalar subqueries), Narwhals (API subset), PRQL (compiles to SQL, then Polars SQL context) | APIs and Python objects, not durable documents; no Rust surface |
| Declarative formats over Polars | Flowfile (visual ETL, YAML flows, ~46 node types, pre-1.0, MIT), polars-ds ML pipelines, MEDS_transforms (domain-specific YAML) | Young, Python-only, nothing in Rust or JavaScript |

dbt, SQLMesh, and Malloy have no Polars target. Nothing Rust-native exposes a
JSON or YAML pipeline over Polars.

### 4d. Where the real precedents are

The successful structured pipe languages live outside Polars, and they converge
on the shape the `Operation` enum already has. One difference matters more than
the similarities: each of these compiles to many backends, so each had to become
a language in its own right. Brightflow has one backend. Under the primitives
principle the chain should not be a language of its own but a serialisable
subset of Polars: variants named after Polars operations, an expression tree
that mirrors a small slice of Polars `Expr`, a one-to-one executor, and Polars'
own optimiser and schema inference doing the planning and typing.

- **Metabase MBQL.** A JSON query object with source table or source query,
  joins, expressions, filter, aggregation, breakout, fields, order by, and
  limit. Every clause is a tagged array; it evolved to a stage-based form with a
  versioned changelog and compiles to SQL and several non-SQL backends. The
  closest analogue in audience and shape, worth studying for scope and for its
  versioning history, but not a design to copy: its generality exists to serve
  thirty backends.
- **Cube's REST query.** Measures, dimensions, filters, time dimensions with
  granularity, order, limit; also exposed over MCP for agents.
- **Vega-Lite transforms.** Nineteen JSON transforms applied in array order,
  including filter, calculate, aggregate, window, lookup, pivot, fold, bin, and
  time unit. The most complete "JSON pipe" precedent.
- **Google pipe syntax in SQL** (VLDB 2024, shipped in BigQuery). FROM-first
  ordered pipelines fix clause-order mismatch; each prefix is a valid query.
  Over a thousand weekly users inside Google within months.

The dissent is Rill, which prefers SQL for its metrics layer over "inventing a
new query language." Rill's users are engineers writing YAML in git; this
product's users are people and agents clicking and emitting JSON, which is the
population MBQL serves.

### 4e. Risks

1. **The expression sublanguage.** Adding derived columns means designing an
   expression language, and scope creep lives there. Keep it a small typed JSON
   tree that mirrors a deliberate slice of Polars `Expr`, named after the Polars
   methods it maps to, grow by need, and refuse a raw expression-string escape
   hatch early. Escape hatches become the real language.
2. **Nobody else speaks it.** Analysts know SQL. Because it is a data structure,
   a compiler to SQL for export or DuckDB verification is possible later, and an
   LLM can translate in both directions. The enum is the truth; text is a view.
3. **Typing it by hand is poor.** A JSON chain is not a command-bar experience.
   The primitives principle rules out the obvious fix, a text syntax with its own
   parser, because that is a new primitive for a marginal gain. The translator
   from prose to chain is the LLM the project already owns.
4. **Schema inference is required.** Each operation must report its output
   schema without executing, for UI, validation, and registering a materialised
   model's columns in Litehouse. Polars provides this from the lazy plan.
5. **Language versioning.** Stored models must parse after the enum grows.
   Additive variants plus the immutable config-snapshot pattern already used for
   enrichment versions cover it. MBQL's changelog shows what a decade of this
   looks like.
6. **Do not store Polars' plan.** Enable `serde` only as a cache or transport
   detail behind the enum, never as the persisted format, because Polars
   guarantees no cross-version stability.

## 5. Fit with project philosophy

1. **One action bus.** A model definition is an action; a model run is an agent
   or scheduler run. Nothing new mechanically.
2. **AI output is editable, not just acceptable.** An agent that proposes a chain
   proposes a data structure the user can edit step by step in the same UI that
   built it. This is impossible with generated SQL without a parser.
3. **Simple should be easy, complex should be possible.** Six operations stay
   simple; derive, join, and window make complex possible.
4. **Two-way door.** Every step below is additive enum variants and one
   consumer, individually revertable, with SQL export as the eventual exit.
5. **Speed is the feature.** Chains compile to one LazyFrame; leading filters on
   partition or stats columns can feed Litehouse's `ScanFilter` pruning before
   Polars ever reads a file.
6. **Lean on the primitives.** Rows in SQLite for definitions, Polars for every
   transformation and for schema inference, serde and `ts-rs` for the wire
   contract, ECharts for rendering only. ECharts has its own declarative dataset
   transforms; do not use them. Shape once in Polars and hand ECharts finished
   rows.

## 6. Recommended sequencing

Design the modelling layer and the semantic layer together, since a named
measure is the smallest model.

1. **Grow the language as a Polars subset:** `From` source and model references,
   `WithColumns` carrying a small typed expression tree, `Join`, compound
   filters, each variant named after the Polars operation it maps to. Extend the
   `ts-rs` types and the executor in step.
2. **Schema inference:** output schema per operation from the lazy plan, exposed
   to the frontend and used to validate chains against `column_semantics`.
3. **Models table in Litehouse:** a chain plus name, description, and input
   references; materialise to Parquet, register the output as a table with its
   semantic rows written in the same transaction; version like enrichment
   functions.
4. **Move the inline metrics in:** visitors, pageviews, and bounce rate become
   measures over an events model; the web-analytics handlers and Explore both
   read them.
5. **Dependency-aware scheduling:** models declare inputs; the scheduler orders
   sync, model, enrichment, insights, and triggers incrementally.
6. **Quality checks as engine findings:** freshness, row count, null rate, and
   schema drift over model outputs, proposed into the feed.
7. **Engine emits chains:** insights findings carry the `Operation` chain that
   reproduces them.
8. **Later, if pulled:** SQL or Ossie export, file retention and snapshots,
   outbound alerts. Not a text syntax with its own parser; see risk 3.

## 7. Bottom line

The stack is a complete vertical for a single-node analytics product: extract,
store, compute, catalog, present, and an agent layer most platforms lack. What is
missing is the connective tissue, and the structural gap is a modelling layer,
without which the semantic layer can only describe raw connector output.
Orchestration, quality, versioning, and an outward door follow from it.

For that modelling layer, growing the existing `Operation` chain is the right
call over SQL. It is typed across the wire, already compiles to Polars, and is a
data structure that the UI, the LLM, the action bus, and Litehouse can all
handle directly. Nobody has built an established transformation language over
Polars, so this would be Brightflow's own format. It should not be a language of
its own but a serialisable subset of Polars, because Brightflow has one backend
and the project's principle is to lean on its primitives. The shape has still
been independently reached by Metabase's MBQL, Cube, Vega-Lite, and Google's pipe
SQL: an ordered list of tagged operations from source through filter, derive,
join, aggregate, window, to sort and limit, with expressions as small tagged
trees. Name the variants after Polars operations, keep the expression tree a
small mirror of Polars `Expr`, let Polars plan and type, never persist Polars'
own plan, and design the first models alongside the first named measures.

## 8. References

Sources fetched and read during research unless marked *(search summary)*.

**Polars**
- LazyFrame serialisation and its stability warning: https://docs.pola.rs/api/python/dev/reference/lazyframe/api/polars.LazyFrame.serialize.html
- Issue to remove JSON plan serialisation: https://github.com/pola-rs/polars/issues/18284
- Substrait support closed as not planned: https://github.com/pola-rs/polars/issues/7404
- `polars-plan` features including `serde` and `dsl-schema`: https://docs.rs/crate/polars-plan/latest/features
- Polars Cloud on shipping the DSL tree to the server: https://pola.rs/posts/polars-cloud-what-we-are-building/
- Polars SQL context *(search summary)*: https://docs.pola.rs/user-guide/sql/intro/

**Languages and tools over Polars**
- Ibis Polars backend: https://ibis-project.org/backends/polars
- Narwhals: https://pypi.org/project/narwhals/
- pyprql, PRQL to SQL to Polars: https://github.com/PRQL/pyprql
- Flowfile: https://github.com/edwardvaneechoud/Flowfile
- polars-ds pipelines: https://polars-ds-extension.readthedocs.io/en/latest/pipeline.html
- MEDS_transforms: https://github.com/mmcdermott/MEDS_transforms
- dbt-duckdb Python models returning Polars frames: https://github.com/duckdb/dbt-duckdb
- SQLMesh Python models *(search summary)*: https://sqlmesh.readthedocs.io/en/latest/concepts/models/python_models/
- Koheesio, for contrast: https://github.com/Nike-Inc/koheesio
- Awesome Polars index: https://ddotta.github.io/awesome-polars/
- Validation-library comparison *(search summary)*: https://opensource.posit.co/blog/2025-06-04_validation-libs-2025/

**Structured pipe languages outside Polars**
- Metabase MBQL library changelog: https://www.metabase.com/docs/latest/developers-guide/mbql-library-changelog
- Cube REST query format: https://docs.cube.dev/reference/core-data-apis/rest-api/query-format
- Vega-Lite transforms: https://vega.github.io/vega-lite/docs/transform.html
- Google, "SQL Has Problems. We Can Fix Them: Pipe Syntax in SQL", VLDB 2024: https://vldb.org/pvldb/vol17/p4051-shute.pdf
- Simon Willison's summary of the pipe syntax paper: https://simonwillison.net/2024/Aug/24/pipe-syntax-in-sql/
- PRQL rationale *(search summary)*: https://prql-lang.org/faq/
- wvlet flow-style language, for contrast: https://github.com/wvlet/wvlet
