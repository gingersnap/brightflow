# Capabilities and Structure Compared: Brightflow Against the Tools It Resembles

**Date:** 2026-09-13
**Scope:** What Brightflow can do today, laid next to the tools that do
parts of the same job, and how each of them organises those parts for the
person at the screen. Two questions: where is Brightflow genuinely
different, and where is it behind in ways that matter for its shape.
**Method:** Capability inventory from the code at the baseline below (the
route index in `crates/brightflow-api/src/routes.rs`, the engine's analysis
modules, the enrichment and agent modules, the store's semantics tables, the
frontend's tool definitions). Comparables from web research on their
September 2026 state, each claim sourced at the end; where a claim is from
general knowledge rather than a source it is marked as such. Builds on
`reports/2026-09-08_data-platform-completeness-and-operations-language.md`
(the layer table and gap ranking) and
`reports/2026-09-12_tool-platform-or-workbench.md` (the consumer test). No
code changed.
**Baseline:** HEAD `81b3726`.

---

## 1. What Brightflow does today, as a list

The route index and the module tree give this inventory. Grouped by the
sidebar's new headings so the structure question can be asked against it.

**Ingest.** Longbow Lua connectors (GitHub shipped; the framework handles
auth, pagination, rate limits, retries, and lets a connector declare its
tables' semantics). First-party web and product event ingestion: a tracking
script, collect/track/identify endpoints, geo and user-agent enrichment, a
SQLite buffer flushed to Parquet. CSV upload. Interval scheduler with a
post-sync insights hook.

**Store.** Parquet files under a SQLite catalog (Litehouse) with per-file
stats and partitions. Four-layer column and table semantics (detected,
declared, agent, user) with provenance per row, declaration diffs across
connector versions, and an Ossie-format model import and export.

**Analyze.** Explore: a typed operation chain compiled to Polars, with pivot
and chart. Text Explorer: a term index with highlighting over a table's text
columns. Web and product analytics for event sources: stats, time series,
top pages, referrers, UTM, geo, devices, funnels, retention, user profiles
and timelines. Insights: a deterministic engine with trend, anomaly, change
point, seasonality, forecast deviation, distribution shift, concentration,
membership change, outlier clusters and drivers; scored by effect size with
polarity and KPI weighting; novelty against history; run on demand or after
every sync; curated by dismiss, pin, annotate and suppress.

**Data.** Text enrichment: two versioned LLM functions per table, ticket
classification against a category and subcategory vocabulary and mention
extraction against a feedback vocabulary, with cost estimate, sample run,
materialise, per-language usage, vocabulary health and an unresolved-subject
queue. Semantics: the layered model per table, editable per column, with the
declaration history. A `describe_table` agent that proposes descriptions,
roles, polarity and KPIs.

**Agents and the bus.** One action manifest that humans and agents share;
proposals, approval, rejection, undo, per-run approve-all, reject-all and
undo-all; agent runs for insight triage and narration, vocabulary induction
at three levels, and table description; every LLM call gets the table's
resolved semantics as context.

**Ops.** Session auth, LLM provider management, a system view, one activity
feed for everything.

Two facts about this list drive the comparison. Everything runs in one
process over local files, with no warehouse. And numbers and text are
first-class in the same tool: the engine scores a revenue trend and the
enrichment classifies a support ticket, on tables that sit side by side.

## 2. The comparison set

No single product covers this list. Six families cover slices of it.

| Family | Products looked at | Slice of Brightflow they overlap |
|---|---|---|
| BI with a semantic layer | Metabase, Lightdash, Rill, Evidence | Explore, Semantics |
| Catalog with AI documentation | Databricks Unity Catalog | Semantics, the describe agent |
| Automated insights | ThoughtSpot SpotIQ and Spotter, Amplitude agents | Insights, triage and narration |
| Product analytics with data management | PostHog, Amplitude | Event ingestion, web analytics, Semantics |
| Feedback analytics | Enterpret, Thematic, Unwrap | Text enrichment, vocabularies |
| Extract and load | Airbyte, Fivetran, dlt (general knowledge) | Longbow connectors |

## 3. Capability by capability

The verdict column is against the closest peer in each row, for
Brightflow's intended scale: one workspace, one node, a few people.

| Capability | Brightflow | Closest peers | Verdict |
|---|---|---|---|
| Ad hoc exploration | Operation chain to Polars, pivot, chart, column semantics inline | Metabase question builder; Lightdash explores over dbt; Rill dashboards from YAML | At par for querying; behind on saving. Nothing in Brightflow can be saved, named, or shared. Every peer has saved questions and dashboards. |
| Semantic layer | Four layers with provenance, per column and table; connector-declared; Ossie import and export; consumed by Explore, the engine and every LLM call | Metabase Data Studio: models, metrics, glossary, semantic types, consumed by Metabot. Lightdash: YAML or dbt, consumed by the AI agent. | Ahead on provenance and layering; nobody else records who said what and lets a person's row outrank a connector's. Behind on metrics and relationships: declared, stored, exported, but only sum, avg and count with equality filters reach the engine, and joins are not queryable. |
| AI-written documentation | `describe_table` proposes descriptions, roles, polarity, KPIs; reviewed per run; lands at its own layer; undoable | Unity Catalog AI comments: per-column accept, edit or reject, human review recommended, no programmatic bulk accept | Ahead. Databricks has no bulk accept and no layer; Brightflow has both and the model's closing note. Behind on "edit before accept", which Databricks has and Brightflow's review card lacks. |
| Automated insights | Nine deterministic detectors, multiplicity-controlled scoring, novelty, post-sync runs, curation feeding back into ranking, LLM narration on top | SpotIQ: trends, outliers, key drivers, change analysis with drill-down. Amplitude Dashboard Monitoring Agent: metric change detection, root cause, delivery to Slack or email. | At par on detection breadth; the philosophy of deterministic first, LLM narrating, is the same one ThoughtSpot now describes as "trusted deterministic insights". Behind on delivery: findings stay in the app, nobody gets a message. |
| Feedback and text | Two LLM calls with a curated vocabulary, versioned and cost-estimated; agent-induced categories at three levels; unresolved-subject queue; term search | Enterpret adaptive taxonomy that learns per customer; Thematic theme discovery with curated approval; Unwrap auto-tagging with alerts on emerging patterns | At par on classification with a reviewed taxonomy, which is Thematic's model. Behind on the taxonomy evolving on its own and on alerts when a theme grows. Ahead in that the same tool holds the numbers the tickets are about. |
| Event analytics | Script, collect, identify, funnels, retention, user timelines, geo, devices, UTM | PostHog and Amplitude, the whole product | Behind, by design. A capable subset, not a product analytics suite; no session replay, no experiments, no cohorts as saved objects. |
| Data management | Semantics page per table; declaration history; Activity feed | PostHog Data Management: definitions with verified tags, freshness and query volume per definition, schema enforcement, a history per object | Roughly at par on the page; behind on freshness and usage signals per definition, which PostHog shows and Brightflow does not collect. |
| Human and agent actions | One bus, one feed, one undo; agents propose or auto-apply; per-run review | Amplitude agents act in the product; Metabase v61 adds AI governance, per-group limits, usage analytics | Ahead on the unified model; no peer runs humans and agents through one logged, undoable path. Behind on governance: no roles, no per-user limits, no usage accounting. |
| Connectors | Lua scripts declaring their tables' semantics; one shipped | Airbyte and dlt with hundreds of sources (general knowledge) | Behind on count, ahead on the connector carrying meaning: no ELT tool lets a source declare roles, polarity and doc columns. |
| Modelling and transformation | None as a layer (the 2026-09-08 report's first gap) | dbt everywhere; Rill SQL models; Bruin assets with quality checks in one binary | Behind, structurally. Still the largest gap. |
| Deployment | Single Rust binary, local files, no warehouse, self-hosted | Rill single binary; Evidence static site; Metabase self-hosted; Lightdash self-hosted but its AI agent is cloud-only | At par with the local-first tools, and ahead of them on AI, since Brightflow's agents run wherever the binary runs against any OpenAI-compatible endpoint. |
| Outward door | None: no SQL, no MCP, no export | Amplitude MCP into Claude, ChatGPT, Cursor; Metabase AI terminal and dashboards-as-code | Behind. The 2026-09-12 report called this the platform test; the comparison shows MCP became the door everyone opened in 2026. |

## 4. How they structure it

Four navigation shapes appear across the set.

**Tool-centric with a definitions area.** Metabase: browse and question
tools on one side, Data Studio as a separate workbench for models, metrics
and glossary, admin apart. Amplitude and Mixpanel: analysis reports, a Data
section for taxonomy and governance, settings. This is the shape
Brightflow now has after the headings: Analyze, Data, Settings per source.

**Object-centric.** Unity Catalog and the catalogs: the table is the page,
with Schema, Lineage, History tabs and AI comments inline on the column
row. Analysis tools link out from it. Brightflow's Semantics page is one
such page under a tool tab; the table picker at the top of every tool says
the table is the real unit, but navigation is still by tool.

**Definitions as a hub with signals.** PostHog Data Management: each event
and property definition is an object with a verified flag, freshness, 30-day
volume, query volume, tags, and its own history. The signals, not the
descriptions, are what make people open it. Brightflow's Semantics page has
the descriptions and the history and none of the signals.

**Code-centric.** Lightdash, Rill, Evidence, Bruin: the model is YAML or SQL
in a repository, the UI renders it. Brightflow's Ossie import and export is
the seam to this world, and the connector declaration is code in the same
sense; a person's edits in the UI are the layer above.

Where agents sit differs too. Amplitude and ThoughtSpot put the agent in
front, as the entry point that builds dashboards and investigates. Metabase
puts Metabot beside the tools with the semantic layer under both.
Brightflow puts agents inside each tool as actions whose output lands
where a person's would, reviewed in one feed. The Brightflow shape is the
least visible and the most accountable; the Amplitude shape is the most
visible and the hardest to audit.

## 5. What is genuinely distinctive

Five things no peer in the set has together, and most have none of.

1. **Provenance as a first-class layer.** Every semantic fact knows who
   said it, and a person's row outranks an agent's outranks a connector's
   outranks the detector's. Peers store one description per column.
2. **One bus for people and agents.** The same manifest, log, approval and
   undo for both. Peers bolt AI governance on as limits and quotas.
3. **Deterministic engine, LLM narration.** Findings are statistical claims
   the model is never the source of. ThoughtSpot markets this; Brightflow's
   philosophy document wrote it down first, and the engine enforces it.
4. **Numbers and text in one tool.** A support ticket's category and a
   revenue measure are columns on tables in the same catalog, analysed by
   the same engine, described by the same semantics.
5. **Local-first with agents.** One binary, plain Parquet, any
   OpenAI-compatible model. The local-first BI tools have no agents; the
   agent-first tools are cloud services.

## 6. Where it is behind in ways that matter

Ordered by how much each would change what a person can do, not by size.

1. **Nothing can be saved or shared.** Every peer has saved questions,
   dashboards, or reports. An Explore view, an insight, a Text Explorer
   query: none survives navigation. This is the gap a first user hits in
   the first hour.
2. **Findings do not leave the app.** Amplitude's monitoring agent and
   Unwrap's alerts deliver to Slack or email. Brightflow's post-sync runs
   put findings in a feed nobody is told about.
3. **No modelling layer.** Named, saved derived tables with a registered
   place in Litehouse. The 2026-09-08 report ranked it first and nothing has
   changed; the semantic layer can still only describe raw connector output
   and enrichment columns.
4. **No outward door.** MCP is how Amplitude and others put their data in
   front of coding agents and chat clients in 2026. Brightflow's action
   manifest and semantics are already the right shape for an MCP server:
   the tools would be the actions, the resources the resolved tables.
5. **Signals on definitions.** Freshness, volume and query usage per column
   are what make PostHog's data management a place people go. Brightflow
   has the stats in `file_column_stats` and the history in Activity, but
   the Semantics page shows neither usage nor freshness.
6. **Edit before accept.** A describe proposal can be approved or rejected,
   not corrected in place. Databricks has edit-then-accept on every AI
   comment; the philosophy document's fourth principle, "AI output is
   editable, not just acceptable", asks for the same.

Behind in ways that do not matter for the shape: warehouse scale, session
replay and experiments, hundreds of connectors, enterprise roles. Those are
other products.

## 7. What follows for structure

The headings added at this baseline match the tool-centric-with-a-Data-area
shape that Metabase and Amplitude converged on, so they are the right first
step and need no revisiting. Three structural moves follow from the
comparison, in the order they pay off.

1. **Make Overview the definitions hub.** The Dashboard tab is a table list
   with sync status. PostHog's lesson is that per-table signals bring people
   to the definitions: rows, freshness, columns described versus not,
   pending proposals, last insight run. That turns the landing page into
   the object-centric entry the catalogs use, without changing routing.
2. **Move the model import and export to Semantics.** It is a Data
   capability filed under Settings. Small, and it makes the Data heading
   true.
3. **Keep agents inside tools.** The comparison does not argue for an
   agent-first entry point. The accountable shape is the distinctive one;
   what it lacks is delivery, which is a notification problem, not a
   navigation one.

The saved-artifact gap is not a structure question; it is a missing noun.
Once views and findings can be saved, they will need a place, and the
Analyze heading is where the peers put it.

---

## Sources

- Metabase AI releases, Data Studio and Metabot: https://www.metabase.com/releases-ai , https://www.metabase.com/features/semantic-layer , https://www.metabase.com/docs/latest/ai/metabot
- PostHog Data Management and schema management: https://posthog.com/blog/data-management-feature , https://posthog.com/docs/product-analytics/schema-management
- Databricks Unity Catalog AI-generated comments: https://docs.databricks.com/aws/en/comments/ai-comments , https://www.databricks.com/blog/announcing-public-preview-ai-generated-documentation-databricks-unity-catalog
- ThoughtSpot SpotIQ and Spotter: https://www.thoughtspot.com/product/analytics/spotiq , https://www.techtarget.com/searchbusinessanalytics/news/366636078/ThoughtSpot-automates-full-platform-with-new-Spotter-agents
- Amplitude AI agents and MCP: https://amplitude.com/press/amplitude-introduces-agentic-ai-analytics-for-the-next-era-of-product-experiences , https://amplitude.com/ai-agents
- Feedback analytics taxonomy comparison: https://www.enterpret.com/guides/customer-feedback-analysis-tools-with-taxonomy-management , https://www.unwrap.ai/post/thematic-alternatives
- Lightdash: https://github.com/lightdash/lightdash , https://www.modern-datatools.com/tools/lightdash
- Local-first and BI-as-code tools (Rill, Evidence, Bruin): https://www.rilldata.com/blog/building-an-agent-friendly-local-first-analytics-stack-with-motherduck-and-rill , https://motherduck.com/blog/the-future-of-bi-bi-as-code-duckdb-impact/ , https://getbruin.com/project-showcase/
