# Litehouse as Catalog *and* Semantic Layer

**Date:** 2026-09-08
**Scope:** Assess the idea that Litehouse (the SQLite catalog over Parquet in
`crates/brightflow-store`) should be not only the lakehouse catalog, in the
DuckLake sense, but also the semantic layer: the place where dimensions,
measures, labels, descriptions, vocabularies, and relationships are defined,
consumed by Explore, the insights engine, and the LLM alike.
**Method:** Three parallel research passes: (1) full map of the store crate's
schema, write paths, and module docs; (2) inventory of every place semantic
knowledge is defined or hardcoded outside the store, in the API, engine, and
frontend, and how it flows to the query engine and to the LLM; (3) web research
on whether the industry is fusing catalog and semantic layer (DuckLake, Unity
Catalog, Snowflake, Polaris, Ossie/OSI, MotherDuck, small embedded tools).
Headline code claims (no write path for roles, no Explore consumer, no frontend
caller of the semantics routes) were grep-verified before inclusion. No code
changed.
**Baseline:** HEAD `6946e4d`. Related prior reports:
`reports/2026-08-03_philosophy-and-simplification.md` (flagged the role/polarity
mapping duplication, still present) and
`reports/2026-08-26_folder-per-tenant-and-test-environments.md` (the workspace
folder as unit of isolation, which this idea depends on).

---

## 1. The idea, stated precisely

DuckLake's thesis is that lakehouse *metadata* belongs in a SQL database, not in
scattered files on object storage. Litehouse already follows that thesis for
the catalog half: one `litehouse.db` per workspace indexes the Parquet files.

The proposal extends the thesis one layer up: the *meaning* of the data
(which columns are dimensions or measures, what they are called, what a metric
is, how tables relate, which vocabulary a classifier resolves against) also
belongs in that same SQL database, as first-class rows, next to the file index.
One file would then hold the data index, the interpretation layer, and the audit
trail of who changed the interpretation and when.

## 2. What Litehouse actually is today

Two findings reframe the question before any judgement.

### 2a. Litehouse is not DuckLake-shaped

| DuckLake construct | Litehouse equivalent |
|---|---|
| `ducklake_snapshot`, begin/end snapshot on every data file, time travel | None. `tables.version` is an optimistic-concurrency counter (`ingest.rs`, `StoreError::VersionConflict`), not a snapshot id. Old files are physically deleted on rewrite. |
| Delete files, deletion vectors | None. Merge rewrites the whole table into one new file (`ingest.rs` `merge_parquet`). |
| Transactional multi-table commit | Per-statement; compaction's catalog swap is a sequence, not one transaction (`lib.rs` `compact_partition`). |
| `ducklake_data_file`, per-file column stats, partition values | Yes: `table_files`, `file_column_stats`, `file_partitions`, pruned via `scan.rs`. |
| Views, macros, tags, key/value metadata | None. |

Litehouse is closer to a Hive-style partitioned tree with a SQLite index and
per-file zone maps than to DuckLake. What it shares with DuckLake is exactly the
idea under discussion: catalog-in-SQL. The "Litehouse" name flatters the catalog
half; the comparison holds at the level of philosophy, not of features.

### 2b. Litehouse is already more than half a semantic layer

Of the 19 live tables in `litehouse.db`, 7 are catalog and 12 are interpretation:

| Kind | Tables |
|---|---|
| Catalog (what data exists) | `tables`, `table_files`, `table_column_stats`, `file_partitions`, `file_column_stats`, `sources`, `schema_migrations` |
| Column and table meaning | `column_semantics` (role ∈ measure/dimension/time/entity/ignored, `is_kpi`, polarity, label, description), `table_analysis_settings` (display name, description, time granularity, comparison periods) |
| Vocabulary / glossary | `taxonomy_categories` (five kinds, two levels, description = the definition the model classifies against, aliases, `frozen`), `unresolved_subjects` |
| Derived-field definitions | `enrichment_functions`, `enrichment_function_versions` (immutable `config_json` spec snapshots), `enrichment_runs`, `enrichment_cache` |
| Curation state | `insight_history`, `insight_state`, `insight_suppressions`, `insight_runs` |
| Action bus | `action_log` (actor, provenance, undo payload, idempotency key), `agent_runs` |

The catalog module's own header (`crates/brightflow-store/src/db/catalog.rs`)
already describes itself as "the 'what data exists and what does it mean' half
of the store." So the idea is not a pivot. It is a name for a direction the
crate has been drifting in since migration 005, and a decision to finish it.

## 3. Where semantic knowledge lives outside Litehouse

The second research pass found that the store is one of several places meaning
is defined, and the least-consumed of them.

### 3a. Inventory

| Knowledge | Where it lives today | Form |
|---|---|---|
| Auto-detected roles (numeric with <20 distinct and ratio <0.05 → dimension, else measure; first date column → time) | `crates/brightflow-engine/src/data/schema.rs` | Two magic constants, no config |
| GitHub `issues`/`pull_requests`/`issue_comments` roles, ~60 triples, matched by bare table name | `crates/brightflow-api/src/state.rs` `seed_column_semantics` | Hardcoded seed, applied only when `column_semantics` is empty |
| Which column is the id, title, body, URL, timestamp; and the join key to the `{table}_mentions` child | `crates/brightflow-api/src/enrichment/display.rs` `DocDisplay::for_table` | Hardcoded `match` on table name |
| Web-analytics metrics: visitors = `n_unique(visitor_id)`, pageviews = `len(id)`, bounce = sessions with one `"pageview"` | `crates/brightflow-api/src/web_analytics/queries.rs` | Polars expressions inline in handlers |
| Entity resolution: `effective_user = user_id if non-empty else visitor_id` | `crates/brightflow-api/src/product_analytics/queries.rs` | Inline coalesce |
| Dimension per endpoint (`pathname`, `referrer_source`, `utm_source`, `browser`, `country`) | `crates/brightflow-api/src/web_analytics/handlers.rs` | One literal per handler |
| Enrichment output columns, sentiment values and their definitions, mention types | `crates/brightflow-engine/src/enrichment/ticket_classify.rs`, `mentions.rs` | Rust constants |
| Vocabulary kinds, caps, reserved `other`, health thresholds | `crates/brightflow-engine/src/enrichment/vocabulary.rs` | Rust constants, re-hardcoded in `VocabularyTree.vue` |
| Connector table names, primary keys, derived fields | `crates/brightflow-connect/connectors/*.lua` | Lua; primary keys never reach the store |
| Default aggregation (numeric → sum, else count), operator sets, humanised labels | `brightflow-app/src/stores/pivot.ts`, `composables/useOperators.ts`, `utils/format.ts` | TypeScript, dtype-derived |

### 3b. How it flows to the query engine: it does not

The Explore path is purely physical. `Operation` in
`crates/brightflow-api/src/analytics/types.rs` is Filter/Select/GroupBy/Pivot/
Sort/Limit over raw column names. `analytics/executor.rs` and
`analytics/session.rs` build every `ColumnInfo` with `role: None`. Only the
`load_table` handler merges stored semantics in, and the only consumer of that
merged data is the command palette. Field pickers show raw column names. A
"measure" in Explore is an ad-hoc `(column, aggregation)` pair invented in the
pivot bucket, with no name, no reuse, no format, no polarity.

Two role systems therefore never meet: `DataSchema.measure_columns` (declared,
insights only) and `datasetStore.numericColumns` (dtype-derived, Explore only).
Explore offers `ignored` columns as row fields and any numeric column as a value.

### 3c. How it flows to the insights engine: partially

`build_schema` in `crates/brightflow-engine/src/data/merge.rs` merges
auto-detection with `column_semantics`, and KPI flags and polarity do reach
scoring and sentiment. But `build_schema` drops `label` and `description`, and
`DataSchema` has no field for them. A curated display name can never reach a
chart axis or an insight headline; the engine always falls back to
`humanize_column`. `ColumnRole::Entity` parses but lands in no list, so an
entity column is indistinguishable downstream from an ignored one.

### 3d. How it flows to the LLM: not at all

Three prompt paths exist: insight triage and narration (`agent/runner.rs`),
vocabulary induction (same file plus `agent/sampling.rs`), and per-row
classification and extraction (`engine/enrichment/*`). None of them carries a
column role, polarity, label, or table description. The triage prompt sees only
the engine's already-rendered prose. The classification prompt sees the
vocabulary block, which is the one piece of Litehouse semantics that does reach
a model, and it works well precisely because it has a consumer.

### 3e. Write paths

`ACTION_KINDS` in `crates/brightflow-api/src/actions/types.rs` contains exactly
two semantic actions: `set_kpi` and `set_column_polarity`. There is no
`set_column_role`, `set_column_label`, or `set_column_description`. The PUT and
DELETE semantics routes were removed in the 2026-08-03 simplification; the
remaining GET routes in `crates/brightflow-api/src/semantics/` have no frontend
caller. After the hardcoded GitHub seed, no user and no agent can change a
column's role. Enrichment materialises new columns (`summary`, `category`,
`sentiment`, the mention flags) and writes no `column_semantics` rows for them.

The action path also defaults a previously-unseen column to `role: "measure"`
when toggling KPI, so flagging a string column as KPI silently declares it a
measure.

## 4. Do others think the same?

Yes. 2026 is the year the semantic layer moved *into* the catalog, at the level
of catalog objects rather than table-format metadata.

| Who | What | When |
|---|---|---|
| Databricks | Unity Catalog Metric Views: YAML body with sources, dimensions, measures, relationships, synonyms, agent metadata, as a UC view object with UC permissions and lineage | GA 2026-04-02; core open-sourced into Apache Spark 4.2, 2026-07-14 |
| Snowflake | `CREATE SEMANTIC VIEW`: tables, relationships, facts, dimensions, metrics, synonyms, verified queries, as schema-level objects; "Horizon Context" positions the catalog as the governed context layer for agents | Standard-SQL querying GA 2026-03-02; Horizon Context announced 2026-06-02 |
| Apache Polaris | `SEMANTIC_MODEL` entity storing Ossie YAML documents beside Iceberg tables, CRUD REST under the namespace, existing RBAC | Merged July 2026, beta, off by default |
| Open Semantic Interchange → Apache Ossie | YAML interchange spec (datasets, dimensions, measures, metrics, relationships, context); working groups for metric language, catalog integration, ontology | Spec v1.0 2026-01-29; Apache Incubator 2026-06-22; converters exist, no product ships native import yet |
| MotherDuck | "Context belongs in the warehouse": markdown Guides stored as versioned warehouse objects, managed via SQL or MCP | July 2026 |
| DuckLake 1.0 | Views, macros, tags, key/value metadata in the catalog DB. **No** semantic constructs; roadmap (v1.1 variant inlining, v2.0 branching and RBAC) has none | 2026-04-13 |

At the small and embedded end, nobody does this. Rill, Lightdash, Omni, and
Evidence persist semantics as YAML in git. Metabase persists models and metrics
as rows in its application database. No single-node analytics tool stores metric
definitions in the lakehouse catalog database itself. That space is empty. It is
either an opportunity or a warning, and the dissent below says which.

### The dissent

- **Economic** (Benn Stancil, "The context layer", 2025-08-29): metrics layers
  stalled because they earn no standalone revenue and BI vendors prefer
  proprietary semantics; a context layer for agents faces the same problem. Not
  a concern for a product that owns its whole stack.
- **Lock-in** (Cube and independent analysts): warehouse-native semantics stop at
  the platform boundary; Snowflake and Databricks semantic objects do not migrate
  to each other; OSI import is not production-grade.
- **Registry without a consumer** (Robert Stupp on the Polaris entity): storing
  semantic documents as opaque strings with no consumer flow is "storage plumbing
  for a low-level document registry." Section 3 shows this repo already has one.
- **Prose beats DSLs for LLMs** (MotherDuck evals, arXiv 2604.25149): column
  comments added at most about one point on BIRD Mini-Dev; a 4 KB markdown
  semantics document lifted accuracy 17 to 23 points and erased model
  differences; a semantic layer tuned for one LLM becomes coupled to that model.
  dbt's own 2026-04-07 benchmark shows the semantic layer's real advantage is
  that it fails loudly where text-to-SQL fails silently.

## 5. Pros and cons for Brightflow

### Pros

1. **The governance comes free.** Databricks and Snowflake had to build
   permissions, lineage, and audit around their semantic objects. Here the action
   bus, undo payloads, actor provenance, and agent-run linkage already live in the
   same SQLite file. A semantic edit is one more action kind and one more row in
   `action_log`, in the same transaction as the definition it changes.
2. **Transactional coherence with materialisation.** An enrichment run that
   writes a `sentiment` column can commit its role, label, and polarity in the
   same transaction as the file registration. Today it writes none.
3. **One workspace, one file, portable.** Paths are already root-relative so the
   catalog survives a copy or move. Meaning travelling with the data is exactly
   what the folder-per-tenant report argued for. It is also the project's
   primitives principle applied: SQLite's strengths are one file, real
   transactions, and relational metadata, and definitions as rows lean on all
   three where YAML in git would lean on none. JSON1 and FTS5 are further SQLite
   strengths this layer can use, for flexible per-row metadata and for searching
   descriptions.
4. **One place to build LLM context.** Three prompt paths carry zero column
   semantics today. A context bundle assembled from Litehouse (table description,
   column roles and descriptions, vocabulary definitions, metric definitions,
   relationships) fixes all three in one spot, and matches the published evidence
   that structured context helps most where join paths and metric definitions
   are not obvious.
5. **It is the stated philosophy.** `docs/human_ai_interaction.md` says the human
   and the AI "co-edit the interpretation layer (labels, clusters, taxonomies,
   annotations) on top of immutable data." Litehouse is already that layer's
   store. The taxonomy subsystem implements the philosophy fully: agent proposes,
   human edits or freezes, definitions version the classifier cache, every step
   is an undoable action. The column layer implements almost none of it.

### Cons and risks

1. **The registry-without-a-consumer failure is already present.** Section 3 is
   the evidence: stored labels read nowhere, semantics routes with no caller, an
   Explore that never sees a role. Adding a metrics table before adding a reader
   would deepen the problem. The ordering must be consumer first.
2. **Portability.** Rows keyed by an internal uuidv7 `table_id` are not
   exchangeable with anything. The mitigation matches the repo's own rule 3:
   SQLite is the source of truth, and an Ossie YAML export is a *generated*
   artifact, regenerated not edited. That also keeps a two-way door open if the
   industry settles on Ossie.
3. **Scope temptation.** A full metric language (formulas, ratio metrics, join
   graphs, entity resolution rules) is a large build, and the evidence says prose
   descriptions capture most of the LLM value. Named measures with a bound
   aggregation and filter, labels, descriptions, and a relationships table cover
   the product's actual needs.
4. **The DuckLake half stays unfinished.** Snapshots and time travel are a
   separate, larger project with no current product pull. Adopting the semantic
   half does not require them and should not be gated on them.
5. **Model coupling.** If descriptions get tuned until one LLM answers well, they
   become a map of how that model sees the data. Keep descriptions human-facing
   and let the deterministic engine remain the source of statistical claims, as
   the philosophy doc already demands.

## 6. Concrete motivation in the code

Two examples make the case without any hypothetical:

- **Visitors, pageviews, and bounce rate** are metric definitions expressed as
  Polars expressions inside HTTP handlers. They have no label, no polarity, no
  description, and no representation that Explore, the insights engine, or the
  LLM can see. A user opening the same `events_*` table in Explore gets raw
  columns and has to rediscover the metric. These are the natural first rows of a
  measures table.
- **The parent table to `{table}_mentions` relationship** is the only join the
  product has, and it lives in a hardcoded `match` on a bare table name in the
  display module, keyed unscoped across sources while everything else in the
  system is carefully source-scoped. It is the natural first row of a
  relationships table.

## 7. Fit with project philosophy

Aligned on every stated axis:

1. **Co-edited interpretation layer on immutable data.** This *is* the
   proposal. Parquet stays immutable; meaning lives in rows that humans and
   agents both edit through the bus.
2. **One action bus, everything logged, undo over approval.** Semantic edits as
   action kinds inherit all three. No new mechanism.
3. **Opinionated defaults, always allow configuration.** The default is the
   engine's auto-detection plus what enrichment declares about its own output.
   The configuration half is currently unreachable; this work makes it real.
4. **Stochastic proposes, deterministic grounds.** An agent can propose a role,
   label, or metric; the engine and the user decide. The existing taxonomy flow
   is the template.
5. **Two-way door.** Every step below is additive rows and one consumer, each
   individually revertable, with a generated export as the escape hatch.

## 8. Recommended sequencing

Consumer first, schema second, at every step.

1. **Make Explore read `column_semantics`.** Labels in field pickers, hide
   `ignored`, default aggregation from role, KPI and polarity visible on values.
   This alone gives the existing table a reader.
2. **Add the missing write paths.** `set_column_role`, `set_column_label`,
   `set_column_description` as action kinds; enrichment writes semantic rows for
   its output columns in the same transaction as materialisation; collapse the
   four role/polarity string mappings the 2026-08-03 report flagged.
3. **Named measures.** A `measures` table: name, table, column, aggregation,
   optional filter, polarity, label, description, format. Move visitors,
   pageviews, and bounce rate into it and have both the web-analytics handlers
   and Explore read from it.
4. **Relationships.** A `table_relationships` table replacing the `DocDisplay`
   match; connector primary keys flow through to it.
5. **LLM context bundle.** One function that assembles table, column, measure,
   relationship, and vocabulary rows into the prompt context for all three agent
   paths.
6. **Generated Ossie export.** A derived YAML file per workspace, regenerated
   from Litehouse, never hand-edited.

Hold off on: a formula language, cross-table entity graphs, and DuckLake-style
snapshots. None has a consumer yet.

## 9. Bottom line

The idea is sound, is already the crate's de facto direction (12 of 19 tables
are interpretation, not catalog), and is where the industry went in 2026:
Databricks, Snowflake, and Apache Polaris all store metric and relationship
definitions as catalog objects, while DuckLake itself stays semantics-free. The
risk is not the architecture but the repo's own history with it: a semantic
layer nobody reads. Litehouse already has stored labels no code consumes and
roles no user can change. The concrete near-term move is therefore to give the
existing `column_semantics` table a consumer in Explore and a write path on the
action bus, then grow it into named measures and relationships with the
web-analytics metrics and the mentions join as the first residents, and to keep
portability as a generated Ossie export rather than as the storage format.

## 10. References

Sources fetched and read during research unless marked *(search summary)*,
which means seen only in search-result snippets.

**DuckLake**
- DuckLake 1.0 announcement: https://ducklake.select/2026/04/13/ducklake-10/
- Specification introduction: https://ducklake.select/docs/stable/specification/introduction
- Catalog table overview: https://ducklake.select/docs/stable/specification/tables/overview
- InfoQ coverage: https://www.infoq.com/news/2026/05/ducklake-sql-catalog/

**Databricks**
- Metric Views docs: https://docs.databricks.com/aws/en/uc-semantics/metric-views/
- Business Semantics GA: https://www.databricks.com/blog/redefining-semantics-data-layer-future-bi-and-ai
- Spark 4.2: https://www.databricks.com/blog/introducing-apache-spark-42

**Snowflake**
- Semantic Views guide (Atlan): https://atlan.com/know/snowflake/snowflake-semantic-views/
- Horizon Context: https://www.snowflake.com/en/blog/horizon-context-governed-context/
- OSI launch press release: https://www.snowflake.com/en/news/press-releases/snowflake-salesforce-dbt-labs-and-more-revolutionize-data-readiness-for-ai-with-open-semantic-interchange-initiative/
- Snowflake vs Databricks semantic objects *(search summary)*: https://colrows.com/blogs/snowflake-semantic-views-vs-databricks-metric-views/

**Apache Polaris and Ossie**
- Polaris semantic models write-up: https://datalakehousehub.com/blog/apache-ossie-polaris-semantic-models/
- Polaris generic tables: https://polaris.apache.org/releases/1.1.0/generic-table/
- Apache Ossie incubator page: https://incubator.apache.org/projects/ossie.html
- dbt on the OSI spec: https://www.getdbt.com/blog/the-osi-spec-updates
- OSI tooling status *(search summary)*: https://datus.ai/blog/semantic-layer-tools-list-osi/

**Google and Microsoft** *(search summaries)*
- Rittman on Google Next 2026: https://blog.rittmananalytics.com/google-next-2026-whats-new-for-looker-bigquery-data-platforms-and-agentic-analytics-732cb3c1aa1b
- Conversational Analytics GA: https://cloud.google.com/blog/products/data-analytics/conversational-analytics-in-google-data-cloud-in-q326
- OneLake catalog: https://community.fabric.microsoft.com/t5/Fabric-Updates-Blog/OneLake-catalog-The-trusted-catalog-for-organizations-worldwide/ba-p/5161117
- Fabric MCP server: https://github.com/mcp/com.microsoft/microsoft-fabric

**The debate**
- MotherDuck, Context belongs in the warehouse: https://motherduck.com/blog/context-belongs-in-the-warehouse/
- Benn Stancil, The context layer: https://benn.substack.com/p/the-context-layer
- Cube, semantic layer for AI and BI 2026 *(search summary)*: https://cube.dev/articles/best-semantic-layer-for-ai-and-bi-2026
- MotherDuck's BIRD Mini-Dev column-comment result and "Oops, maybe we do need
  semantic layers": cited from the MotherDuck blog; exact URLs not captured.

**LLM evidence**
- dbt semantic layer vs text-to-SQL benchmark: https://docs.getdbt.com/blog/semantic-layer-vs-text-to-sql-2026
- Benchmark repo: https://github.com/dbt-labs/dbt-llm-sl-bench
- Rumiantsau and Fokeev, semantics document study: https://arxiv.org/abs/2604.25149
- Spider2-snow semantic-layer agent *(search summary)*: https://arxiv.org/abs/2606.31041
- dbt MCP with Snowflake Cortex: https://docs.getdbt.com/docs/dbt-ai/integrate-mcp-snowflake-cortex
- Snowflake MCP server: https://github.com/Snowflake-Labs/mcp

**Small and embedded tools**
- Rill: https://github.com/rilldata/rill
- Lightdash metrics: https://docs.lightdash.com/references/metrics
- Metabase models: https://www.metabase.com/features/models
- sidemantic: https://github.com/sidequery/sidemantic
