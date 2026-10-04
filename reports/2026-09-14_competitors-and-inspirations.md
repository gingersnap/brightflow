# Competitors and Inspirations: The Field Around Brightflow, September 2026

**Date:** 2026-09-14
**Scope:** A list. Who does part of what Brightflow does, who Brightflow
should learn from, and what the field converged on in 2025–2026. Eight
families, roughly 250 products looked at, about 120 kept. Extends
`reports/2026-09-13_capabilities-and-structure-compared.md` (six families,
twenty products, verdict per capability) from a capability comparison to a
landscape; that report's "ahead / behind" findings are re-checked here
against a much wider set and mostly hold.
**Method:** Eight parallel web-research passes, one per family, each given
the same one-paragraph description of Brightflow and asked for status as of
September 2026, deployment shape, overlap, what to steal, a verdict, and
sources. Primary sources (docs, changelogs, pricing pages, GitHub) were
preferred; claims a pass could not confirm are marked *(unverified)* in the
appendix. Several passes ran out of web-search budget partway and finished
on direct page fetches only, so the tail of each family is thinner than its
head. The raw passes, with every URL, are in
`reports/2026-09-14_competitors-and-inspirations-appendix.md`. No code
changed.
**Baseline:** HEAD `08dc6ac` (models shipped: create, update, rebuild,
delete, agent-proposed).

---

## 1. The short list

If only fifteen names get remembered, these. Verdicts: **C** competitor for
the same buyer, **I** inspiration, **B** both.

| | Product | Why it matters |
|---|---|---|
| B | **Rill** | The closest open-source architectural twin: Go single binary, DuckDB, YAML metrics views, MCP server, two in-product agents, "model tests" after every rebuild. Dashboards-first, no events, insights are LLM-only. |
| C | **Metabase** | The default self-hosted BI. In 2026 its AI went open source (v60), got an MCP server and a CLI for coding agents, and v61 added per-group token budgets and usage analytics. Warehouse-attached, no local-file story. |
| B | **PostHog** | The whole product-analytics half of Brightflow at far greater scale, plus the reference "Data Management" page (verified flag, last seen, history), a data warehouse pulling in business tables, and "Self-driving" (Scouts → Signals → Inbox) launched June 2026. Heavy to self-host. |
| C | **Definite** | The most similar *product thesis*: one vendor, connectors, DuckDB lakehouse, semantic layer, BI, an agent that builds. Cloud-deployed, credit-priced, $250/month. |
| B | **Lightdash** | Semantic-layer BI whose cloud agents ship evals, verified answers, and "Issues" tied to the definition that caused a wrong answer. OSS self-host has no agents; cloud is $3,000/month. |
| I | **Tableau Pulse** | Says "deterministic statistical models produce facts, the LLM narrates" verbatim. Fourteen named insight types, impact-scored, delivered as digests with off-cycle escalation. The closest philosophical match to the insights engine. |
| I | **Yellowfin Signals** | The closest detector set to Brightflow's nine, including membership change and step change as distinct kinds; per-user ranking learned from watch/ignore behaviour. |
| I | **Linear** | Agents install as workspace members via OAuth, are *delegates not assignees*, emit typed activities (thought, action, elicitation, response), and agent-written text carries author indicators. The clearest "same actions, one feed" implementation anywhere. |
| I | **Claude Code** | Six permission modes with allow/ask/deny rules layered on top, a named list of actions no mode auto-approves, hooks that can rewrite an action's input before it runs, and `/rewind` with separate code and conversation axes. |
| I | **Clay** | The reference LLM-column flow: cost per row shown before running, test on ten rows then run all, confidence and reasoning as sibling columns, no charge when the model returns nothing. |
| I | **Thematic** + **Zendesk Intelligent Triage** | Together, the best-documented vocabulary review loop: a quality tracker with coverage, specificity and accuracy scores; a scheduled "new themes for review" digest; suggestion cards carrying coverage % and five example tickets with accept or decline-with-reason. |
| I | **Mixpanel Lexicon** | Thirty-day volume and query count per event and property, and hide / drop / merge as three distinct governance verbs. The signals that make a definitions page a place people go. |
| I | **DuckLake 1.0** | SQLite-or-Postgres catalog plus Parquet, with a backward-compatibility guarantee, data inlining for small writes, and a growing set of tools that write to it. Litehouse is structurally a private DuckLake. |
| I | **Apache Ossie** | The semantic-model interchange spec Brightflow already imports and exports. It lacks polarity, KPI flags, column roles and provenance; `custom_extensions` is the slot. |
| C | **Amplitude AI Feedback** | The former Kraftful, now free on every Amplitude plan, correlating "checkout is broken" tickets with a conversion drop. The direct threat to Brightflow's text-next-to-numbers differentiator, given away by an incumbent. |

## 2. The full list, by family

One line per product. Status is as of September 2026; "gone" means
acquired into something else, shut down, or dormant.

### 2.1 Local-first, open-source and BI-as-code tools

| V | Product | Status and what to take |
|---|---|---|
| B | Rill | Apache-2.0, single Go binary, DuckDB. Monthly releases through 0.89 (Aug 2026): MCP, project AI agent, "Explain this error", model tests, Metrics SQL. Take: files as state that agents edit; assertions on derived tables after rebuild. |
| C | Metabase | AGPL. v58 Metabot GA, v60 AI open-sourced + MCP, v61 governance, v62 CLI for coding agents. Take: prefer an existing saved question before generating; per-group budgets; MCP + CLI + dashboards-as-code as one surface exposed three ways. |
| B | Lightdash | OSS core; agents cloud-only. Take: evals as a stored question→answer set run after every semantic change; verified answers and Issues as objects tied to a semantic row. |
| I | Evidence | MIT; SQL + Markdown → static site; Evidence Studio (2025) adds an agent that reads compile errors. Take: report as plain text, diffable and agent-editable; validation feedback to the agent before the human sees anything. |
| I | Datasette | Apache-2.0, still 1.0 alpha (Sep 2026 security releases). Take: every view is a URL with a `.json` twin; facets with counts as the first look at a table. |
| I | DuckDB UI / MotherDuck | UI extension MIT but the frontend is closed and cannot run offline. MotherDuck: Dives (draft → ready → endorsed → archived), Flights, MCP with an explicit `query_rw` tool. Take: column explorer beside every result grid; read vs read-write as separate tools; Instant SQL's run-as-you-type feel. |
| I | Malloy / Publisher | MIT, Linux Foundation. Publisher serves one model over REST and MCP with a `compile_model` tool. Take: validation as an agent tool; every saved query extends the model. |
| B | Count | Cloud canvas. Agent output is ordinary cells with visible reasoning; metadata-only vs row-level consent for the model. Take: the consent split. |
| B | Hex | $70M Series C. Notebook Agent with cell-level diff accept/reject and a saved version per agent thread. Take: undo the whole agent run as a unit. |
| I | Steep | Swedish, metrics-first, $15/seat. 2026: define-in-code with git sync, targets on metrics, MCP, EU model region. Take: metrics as the navigation object; targets as a detector input. |
| I | Holistics | AML/AQL; its argument that a stepwise, composable query language is what makes LLM output inspectable is Brightflow's typed-chain argument. |
| A | Superset / Preset | 6.1 (2026): MCP as a separate JWT-authenticated process; Preset Agent Skills. Take: MCP out of process; ship skills. |
| I | Grafana | Grafana 13: Git Sync GA (PR from the UI), Assistant tunnels local data to a remote model with consent. Take: a reviewable change set exported from a GUI edit. |
| I | marimo | Acquired by CoreWeave (Oct 2025). Reactive dependency graph; errors as clickable context chips; `marimo pair` attaches an external coding agent. |
| A | Omni | $120M Series C at $1.5B (Apr 2026). Workbook → shared model promotion path. |
| B | Wren AI | Apache-2.0, 17.6k stars. May 2026: retired its chat UI; the semantic "context layer" with MCP is now the product. |
| I | nao | YC, Apache-2.0. Context as a file tree; the agent audits its own answers and opens PRs against the context repo. |
| A | Chartbrew | FSL/MIT, 4.1k stars. Connector templates ("add Stripe, get eight insights"). |
| I | SQLPage, toptopduck, Medama, GoatCounter | Single-binary proofs: Rust `.sql`-files app; Tauri + DuckDB local BI with a privacy contract; Go + DuckDB and Go + SQLite web analytics. |
| gone | Quary (dormant since Jan 2025), Briefer (→ Resend), Mode (→ ThoughtSpot), Redash (maintenance), Quadratic (repo gone, likely closed-source), Whaly (pivoted to sales), Blendo | Cautionary: small OSS BI with no AI story did not survive 2025. |

### 2.2 AI-native and agentic analytics

| V | Product | Status and what to take |
|---|---|---|
| C | Basedash | Seed-stage, weekly releases; moved to $1,000/month. "AI Sources" shows context chips and the exact SQL; mutations render as separate cards from answers. Take: the answer/mutation split. |
| I | Zenlytic | Git-versioned "Cognitive Layer" with natural-language instructions; every figure cited to its metric. Take: NL instructions as part of the semantic layer. |
| I | Dot | A training surface where the data team promotes Q→answer pairs to trusted. |
| I | TextQL | Ontology built incrementally through real work. |
| C | Querio | Analytics-as-code context repo (memory, skills) that humans and agents both edit. |
| C | Fabi.ai | SMB notebook with scheduled "push this to Slack" workflows. |
| A | Julius AI | Consumer file-drop analyst; reviewers flag non-reproducible numbers. |
| I | Tellius | 6.3 (Jun 2026): "Missions" are saved, re-runnable investigations that return the same answer every run; deterministic math engines, LLM interprets. |
| I | ThoughtSpot Spotter | Spotter Semantics (Mar 2026), MCP. Explicitly not text-to-SQL: search tokens → deterministic SQL. Take: market the determinism. |
| I | Databricks Genie | Agent mode plans, runs many queries, returns a cited report; "trusted asset" badge on answers from verified logic. |
| A | Snowflake Cortex Analyst | Verified queries attached to the semantic view; managed MCP with a 50-tool cap. |
| C | Metabase Metabot | See 2.1. |
| C | PostHog AI | Typed tools over the same UI primitives; inline "ask here" on every filter; project memory. |
| C | Amplitude agents | Global + four specialist agents; agent memory; an admin Agent Audit tab. |
| C | Mixpanel Agent | Spark renamed (Jun 2026); MCP with 40+ read/write tools. |
| C | OpenAI Data agent | ChatGPT Work (Sep 10, 2026): connects warehouses, consumes dbt / Genie / Horizon semantic context, pushes to BI tools. Take: export the semantic model in shapes others consume. |
| A | Claude for Excel/Sheets | Adjacent. |
| I | Deepnote | Core Apache-2.0. Agent has "undo all changes from this run" via one icon; the nearest thing to Brightflow's run-level undo in this family. |
| I | Jupyter AI 3.0 | Stopped building its own agent; hosts Claude, Codex, Gemini via Agent Client Protocol. |
| I | Athena Intelligence | Per-cell attribution of who or what wrote each value. |
| C | Arcwise | Pivoted to "detect metric changes and explain them, fully traced": the insights engine as a standalone product. |
| I | Kubit | Warehouse-native, "analytics as the verifier" for agents. |
| gone | Vanna (repo archived Mar 2026), Seek AI (→ IBM), Numbers Station (→ Alation), camelAI (pivoted to inference) | Free text-to-SQL lost; governed context won. |

### 2.3 Automated insights and metric monitoring

| V | Product | Detection and what to take |
|---|---|---|
| B | Tableau Pulse | Fourteen deterministic insight types, impact-scored, thumbs feedback personalises; digests with off-cycle alerts for unfavourable moves only. Take: "concentrated contribution" as a named alert; drivers/detractors split by polarity. |
| B | ThoughtSpot SpotIQ | Outliers (z-score, S-H-ESD), trend, correlation, change analysis with cardinality-scaled z thresholds. Take: the cardinality heuristic; "share of total changed most" view. |
| B | Yellowfin Signals | Spikes, breakouts, step change, trend reversal, volatility, new/lost attributes; "not useful" feeds back. Take: transient vs regime change as different kinds with different half-lives. |
| I | Snowflake `TOP_INSIGHTS` (ex-Sisu) | Decision-tree drivers with absolute and relative contribution and a redundancy filter so "country=SE" and "city=Stockholm" do not both appear. |
| I | Power BI | Anomaly attribution by share of deviation explained per dimension member: trivially computable in Polars. |
| I | Oracle Explain | Facts → drivers → segments → anomalies as a four-pane table page. |
| I | GA4 Insights | Bayesian state-space; segment insights only above a 0.05 % support floor. |
| C | Amplitude Dashboard Agent | Compares against its previous run to label findings new / changed / persistent; posts to Slack. |
| I | Mixpanel alerts | Moved from Prophet to TimesFM (Apr 2026); backtest "how often would this have fired" before enabling; trigger history. |
| B | PostHog alerts + Self-driving | Thirteen detectors with AND/OR ensembles, simulate on history; Signal = what / evidence / suggested action; Inbox ranked by priority. |
| C | Databox | Sensitivity expressed as a target flag rate ("about 1 % of points"). |
| I | Datadog Watchdog | Alert only when X % of the window is anomalous; grouped incidents. |
| A | Monte Carlo, Bigeye, Metaplane (→ Datadog) | The same detectors pointed at freshness, volume and nulls: a data-quality lane for connector tables. |
| A | Anodot (→ Glassbox, Nov 2025) | Grouping simultaneous anomalies into one incident. |
| A | Domo, Sigma agents (Apr 2026), Looker, Klipfolio, Geckoboard, Metabase alerts, Push.ai, Unsupervised | Threshold alerts or LLM-only; Brightflow is ahead. |
| gone | Kausa, Falkon, Outlier.ai, Rupert, Narrative BI (→ Cube), HyperArc (→ Zendesk), Avora (dormant) | The standalone decision-intelligence cohort is gone; LLM-first insight products did not survive alone. |
| libs | Prophet (maintenance mode Aug 2026), Kats, Merlion (archived Mar 2026), ruptures, Nixtla, Darts, Greykite, TimesFM 3.0 (non-commercial weights), Chronos-2 | Two packaging idioms: forecast-interval anomaly, and calibration to a target alert rate. |

### 2.4 Feedback analytics and ticket triage

| V | Product | Taxonomy model and what to take |
|---|---|---|
| C | Enterpret | Adaptive taxonomy + Customer Context Graph attaching LTV, lifecycle and usage to every signal; $25M raised; setup and maintenance overhead is the main complaint. |
| B | Thematic | Bottom-up two-level themes; Theme Quality Tracker (coverage, specificity, accuracy); monthly emerging-themes digest; portable `.themes` files (Jul 2026); MCP. |
| C | Unwrap.ai | Zero-shot auto-tagging; from $24,000/year, volume-priced, never per seat. |
| C | Amplitude AI Feedback (ex-Kraftful) | Free on all plans; complaint → metric → segment → replay in one click; recurrence counts across channels. |
| C | Chattermill | Enterprise; Lyra AI; MCP; anomaly alerts to Slack/Jira. |
| I | Caplena | Curated topic collections the LLM keeps MECE; per-assignment quality score; driver analysis of topics against numeric variables; 100+ languages. |
| C | Kapiche, Birdie, Syncly, Anecdote, Olvy | Mid-market and small; Birdie and Syncly do metric correlation. |
| I | Relative Insight | Comparison-first (A vs B corpora) rather than absolute counts. |
| I | Zendesk Intelligent Triage | Two-level custom intents; weekly suggestion cards with coverage %, five example tickets, accept or decline-with-reason. |
| A | Intercom Fin (→ Salesforce, closed Sep 10, 2026) | Topics auto-generated and non-editable; weekly "what moved and why" trends. |
| I | Help Scout AI Topics | Human edits do not feed back during beta: the anti-pattern. |
| I | Freshdesk Freddy auto-triage | Your existing ticket fields are the taxonomy; agreement rate per field. |
| I | Linear Triage Intelligence | Suggestions show their reasoning; standing workspace guidance steers the classifier. |
| I | Dosu, GitHub agentic workflows | The only open-source correction-driven auto-labelling with adoption. |
| I | Canny Autopilot | Manual vs automated mode: the same inbox is a review queue or an audit log; $0.10 per credit. |
| A | Productboard Spark (GA Jun 2026), Dovetail, Marvin, Cycle, Featurebase | PM-side; feature tree as taxonomy; every insight traceable to its note. |
| I | SentiSum | Writes classifications back into the helpdesk as fields. |
| A | Pylon, Plain, Gainsight, Vitally, Planhat | Support and CS platforms with Snowflake/CRM joins. |
| A | Qualtrics XM Discover, Medallia (incl. MonkeyLearn) | Enterprise suites; AI-bootstrapped hierarchical topic models. |
| gone | Viable, Zeda.io (winding down), Monterey (→ Reforge), Idiomatic (→ Front), Lang.ai (→ Capacity), Levity (pivoted), Prodsight, Painboard | Point solutions without a data moat were absorbed into helpdesks, product analytics and CRMs. |

### 2.5 Product and web analytics

| V | Product | Architecture and what to take |
|---|---|---|
| B | PostHog | ClickHouse + Postgres + Kafka; MIT. Definitions with verified, last seen, history; ingestion warnings as a queue; `posthog.json` schema with generated types; 86+ MCP tools added Aug–Sep 2026; "failed materialised view becomes a signal". |
| I | Amplitude | June.so acqui-hired (Jul 2025); Statsig sold to OpenAI (Sep 2025) then brand and customers to Amplitude (May 2026). Take: post-ingest transformations (rename, redact) that apply retroactively without touching the SDK. |
| I | Mixpanel | Lexicon: 30-day volume and query counts, hide / drop / merge; MCP "Verified Mode" flags unverified entities to agents. |
| I | Heap / Contentsquare | Retroactive virtual events defined over raw captured data: a saved predicate, not a tracking change. |
| C | Umami v3 | MIT, Postgres only; funnels with per-step property filters, retention, journeys, replay, heatmaps. The closest OSS feature match without ClickHouse. |
| C | Plausible | AGPL CE, ClickHouse; no user model by design; "AI assistants" as a referrer channel (Jun 2026); chart annotations. |
| C | Rybbit, OpenPanel, Swetrix, Vemetric, Databuddy, Seline | 2024–26 entrants, all ClickHouse + Postgres + Redis; OpenPanel is the Mixpanel-shaped one; Databuddy ships an investigation agent; Vemetric sells "web + product in one". |
| A | Matomo (6.0 beta Sep 2026), Fathom (V4 engine Mar 2026), Pirsch, Countly, Aptabase, Simple Analytics | Web-only or paid-gated funnels. |
| I | GoatCounter, Medama | Go single binaries on SQLite and DuckDB: the only established projects in Brightflow's deployment shape, web-only. |
| A | Snowplow | Moved to a limited-use licence; OpenSnowcat forked. Cautionary. |
| I | Jitsu | Live-events debugger; MCP that rewires pipelines and fixes functions. |
| A | RudderStack, Segment Protocols | Versioned tracking plans with violations and quarantine. |
| A | Microsoft Clarity (MCP Aug 2026), Fullstory, Pendo, LogRocket, Datadog Product Analytics, Usermaven, Kissmetrics, Woopra, Vercel / Netlify / Cloudflare analytics, Tinybird starter kit | Adjacent. |
| gone | Smartlook (end of sale May 2026), Flurry (2024), Fibotalk, mParticle Analytics | |

### 2.6 Semantic layers, catalogs, modelling and data stack

| V | Product | Shape and what to take |
|---|---|---|
| I | Apache Ossie | v0.1.1, incubating since Jul 2026, 50+ orgs; dbt reads it. Fields: datasets, fields, metrics with dialect-tagged expressions, relationships, `ai_context` (instructions, synonyms, examples), `custom_extensions`. Absent: polarity, KPI, roles, provenance. |
| I | dbt | Merged with Fivetran (closed Jun 2026); Core 2.0 in Rust; Developer agent proposes semantic models as diffs; "state explain" per model. |
| A | Cube | Apache-2.0 semantic layer, now "agentic analytics"; views as curated facades exposed to agents. |
| I | SQLMesh | Fivetran → Linux Foundation (Mar 2026); plan/apply preview of what a change rebuilds; fingerprint-based skip. |
| I | Malloy, LookML | Queries extend the model; declared drill paths per measure. |
| I | Databricks Unity Catalog | Metric views YAML with synonyms ≤10; AI comments with Accept or Edit; Glossary and Domains (Jun 2026). Provenance lost on save. |
| I | Snowflake semantic views | Verified queries attached to the model. |
| A | AtScale SML | Apache-2.0 YAML with "packages" reusable across repos. |
| I | DuckLake 1.0 | Apr 2026, compatibility guarantee, data inlining ≤10 rows, deletion vectors; Windmill, Sling, Tailpipe write to it. Take: publish Litehouse's catalog schema; a frozen read-only snapshot as the first outward door. |
| A | Iceberg + Lakekeeper | Rust REST catalog, single binary; Generic Table API; credential vending per agent. |
| A | Lance/LanceDB, Delta, Hudi, pg_lake, pg_duckdb, pg_mooncake, GlareDB, ParadeDB | Adjacent formats and engines. |
| I | Roapi | Rust single binary: YAML-declared tables → read-only SQL/GraphQL/REST. The nearest existing shape to a Brightflow read-only door. |
| B | Bruin | Go single binary: ingest + transform + quality; assets declare columns with description, PK, checks, owner. The only EL tool where the connector carries meaning. |
| B | dlt | Declarative REST source spec (auth, pagination, cursor as data); context files for agents writing connectors. |
| A | Sling, Airbyte, Meltano, CloudQuery, Estuary, OLake | Sling's six named load modes; Airbyte's 600 connectors. |
| I | Steampipe / Tailpipe | Five-tool MCP (`table_list`, `table_show`, …); Tailpipe moving onto DuckLake. |
| I | Windmill | Pipelines wire scripts by asset lineage and materialise DuckDB steps into DuckLake tables. |
| A | Dagster, Kestra, Mage, Prefect | Per-materialisation credits as a cost model. |
| A | DataHub, OpenMetadata, Atlan, Select Star, Secoda, Alation, Collibra | All rebranded as "context layer for AI" in 2026; OpenMetadata exports PROV-O; Atlan's bootstrap → certify → activate names Brightflow's layers. |
| I | Marmot, Xorq | Single-binary OSS context layer; content-addressed executable catalog. |
| A | Soda, Great Expectations, Elementary | `checks:` on a column declaration. |
| A | Tinybird, Y42, Keboola, Datacoves, Paradime, Coalesce | Warehouse-centric platforms. |

### 2.7 Small-team stacks, spreadsheets and SMB dashboards

| V | Product | Status and what to take |
|---|---|---|
| C | Definite | Free / $250 per month unlimited users; one credit = compute hour or 100K tokens; "inspector" shows query and lineage behind anything the agent built. |
| C | Peliqan | €75 per source; "secured read + controlled write-back actions" through MCP. |
| C | Airbook, Mozart Data, Y42, Keboola | Bundled stacks; Keboola's glossary carries metric owners. |
| I | Equals | $24K–60K per year; "click any figure to see the formula and source query". |
| I | Rows (→ Superhuman) | Two metering currencies: AI tasks vs enrichment tasks; thumbs on every AI result. |
| A | Sourcetable, Quadratic, Row Zero | Per-credit prices published; "auditable, editable" AI as the headline. |
| C | Steep, Basedash, Databox | See above; Databox now "agentic analytics" with a skills marketplace. |
| C | Baremetrics (stagnant), ChartMogul, ProfitWell (free) | SaaS-metrics vertical. |
| A | Geckoboard, Klipfolio, Grow, Panintelligence, Luzmo, Trevor / Embeddable, Draxlr | Dashboards; Embeddable puts dashboards as files in the repo. |
| I | VisiData | Every action a keystroke and a replayable command log: a cousin of the typed chain. |
| I | Bauplan | Git-for-data branches: branch → agent works → human merges. |
| I | DataGrip 2026.2, DataZen, Duckle | Skills for external coding agents; local-first MCP client and server; Rust + DuckDB ETL studio with 385 connectors. |
| C | BitBoard (YC, Jun 2026), ktx, DAC (Bruin) | "Analytics workspace for agents"; executable context layer; dashboards-as-code for agents and humans. |
| gone | Causal (→ Lucanet), Explo (→ Omni), Outerbase (→ Cloudflare), Obviously AI, Tad (dormant), Akkio (pivoted) | The low end vacated; funded players moved up. |

### 2.8 Human–agent interaction models

| V | Product | The pattern |
|---|---|---|
| I | Linear | Delegate not assignee; typed activities incl. elicitation; Loops (recurring agent workflows, Sep 2026); author indicators on agent text; `/mcp/readonly`. |
| I | Claude Code | Modes with rules layered on top; deny always wins; named non-auto-approvable actions; hooks with `updatedInput`; `/rewind` with code / conversation axes; documents what undo does not cover. |
| I | Cursor | Checkpoints per file, restore keeps conversation; hooks with `failClosed`. |
| I | Codex, Jules, Replit | `on-failure` as a middle notch; a Planning Critic reviewing auto-approved plans; task board with apply / dismiss and spend limits. |
| I | GitHub Copilot coding agent | Copilot as author, human as co-author, session log linked from every commit. |
| I | Devin, OpenHands | Take over mid-session in the same workspace; a control plane that hosts other vendors' agents. |
| I | Notion agents | Audit triple: what triggered it, what it did, why; per-agent credits; suggest-edits per change. |
| I | Microsoft Copilot Studio | Agents as identities under the same RBAC as users; credit caps. |
| A | Salesforce Agentforce, Slack, Zapier | Same automation primitives as humans, at the workflow layer. |
| I | Windmill, Retool agents | Approval records who approved as data; run replay with token cost per run. |
| I | Intercom Fin, Amplitude agents | Tests before deploy and QA after, not per action; approval only at the customer-facing boundary. |
| I | PostHog | Feedback on an agent report becomes context for the next run; external MCP agents claim internal Signal reports. |
| I | Databricks AI comments | Accept or Edit before save; provenance lost afterwards. |
| I | Clay, Attio, Google Sheets `AI()` | See the short list; AI attributes with a research agent; 350-cell cap and "regenerate, don't undo". |
| I | Raycast, Warp, Obsidian Bases, tldraw offline | Command bar as agent surface; verification artefact per agent action; views over plain files; local whiteboard "for you and your agents". |
| I | Nuxt UI 4, shadcn | CommandPalette and ChatMessages free in v4; shadcn's Questionnaire component for agent elicitation. |
| I | MCP spec 2026-07-28 | Stateless; `input_required` round-trips for eliciting a human decision; MCP Apps for inline UI; `readOnlyHint` / `destructiveHint`; tool lists vary by granted scope. |
| I | OpenAI Agents SDK, LangGraph, gotoHuman, Yjs | `needs_approval` with `always_approve`; edit state before continuing; origin-scoped undo (`trackedOrigins`) so undoing an agent's run never undoes a colleague's edit. |
| gone | Relay.app (shut down Sep 2026), HumanLayer (pivoted) | Approval-first automation as a standalone category shrank. |
| I | AGENTS.md, llms.txt | 60k+ repos; generate `llms.txt` from the action manifest. |

## 3. What the field converged on in 2025–2026

Ten patterns that recur across all eight families.

1. **MCP is the outward door everyone built.** Metabase, PostHog, Amplitude,
   Mixpanel, dbt, Snowflake, Databricks, Rill, Superset, Malloy, Cube,
   Steep, Lightdash, Linear, Notion, Figma, Canva, Clarity, Grafana and the
   catalogs all ship one. The safeguards converged too: a read-only
   endpoint or scope, per-tool RBAC, "the agent sees what the user sees",
   and enterprise SSO for MCP from August 2026. Brightflow's "no outward
   door" is now the exception in every family.
2. **The semantic layer is called "context" and lives in files.** YAML
   metrics views, context repos, `.themes` files, Ossie documents. Chosen
   because agents edit files well and git gives review. The three extras
   every vendor added: descriptions, synonyms, and "AI context" or verified
   queries.
3. **Deterministic first, LLM narrates, is now the enterprise pitch.**
   Tableau Pulse, ThoughtSpot, Tellius and Qlik say it; PostHog's Signal
   schema encodes it. LLM-first insight products (Narrative BI, Outlier)
   and free text-to-SQL (Vanna, Seek, Numbers Station) did not survive
   standalone.
4. **Undo and checkpoints replaced approval as the default safety net** in
   coding tools (Claude Code, Cursor, Replit, Lovable) and knowledge tools
   (Notion). Approval survives only for external, customer-facing or
   irreversible actions. Relay shut down; HumanLayer pivoted.
5. **Agents are workspace members with identity and typed activity.**
   Linear's four activity kinds, GitHub's co-author attribution, Notion's
   trigger / what / why. The refinement arriving now is *dual*
   attribution: the human who triggered, the agent that acted.
6. **Governance arrived in analytics this year:** per-group token budgets
   and question-level usage (Metabase v61), per-agent credits (Notion),
   per-user budgets with expiry (Copilot), agent audit tabs (Amplitude).
7. **Delivery is uniform:** Slack and email digests, weekly or monthly,
   with off-cycle escalation for unfavourable moves and "what moved and
   why" in plain language. Every surviving insights or feedback product
   ships it.
8. **The LLM-column flow standardised on Clay's shape:** cost per row
   before running, test on ten rows, run all, confidence and reasoning as
   sibling columns, free when empty. Metering split into two currencies
   (AI calls vs data fetches) almost everywhere.
9. **Taxonomy has a stable feature vocabulary:** description, owner, tags,
   verified, last seen, 30-day volume and query count, hide / drop / merge,
   a history log, ingestion warnings, retroactive definitions. None of the
   lightweight OSS analytics tools have any of it.
10. **Consolidation emptied the low end.** Basedash to $1,000/month,
    Lightdash Cloud $3,000, Equals $24K/year; Explo, Rows, Outerbase,
    Causal, Mode, Briefer, June, Kraftful, Idiomatic, Monterey, Lang.ai,
    Seek, Numbers Station, Statsig all absorbed. ClickHouse became the
    default engine for every new product-analytics entrant. The
    self-hosted single-binary tier is held only by OSS projects, none of
    which combine events, connectors, semantics and agents.

## 4. Where Brightflow stands

Re-checked against the wider set. The 2026-09-13 verdicts hold; two
"ahead" items are stronger than that report said, and the "behind" list
gains four entries.

**Ahead, and nobody in ~250 products does the same:**

- Provenance as a layer (detected → connector → agent → user, per row).
  Databricks loses provenance on save; Atlan names the stages but stores
  one description; Ossie has no field for it.
- One action bus for people and agents with one-step undo. Linear and
  GitHub do it in substance for their own objects; no analytics vendor
  advertises undo for agent writes.
- Nine deterministic detectors with effect-size scoring and multiplicity
  control. No product documentation in the insights family names
  Bonferroni or FDR; the visible controls are cardinality-scaled
  thresholds and support floors.
- Text and numbers in one engine with shared semantics. Every peer joins
  them by importing one side into the other; Amplitude now gives that
  import away free.
- Single binary, Parquet on disk, any OpenAI-compatible model. Rill and
  Grafana are the credible single binaries and neither owns storage or
  events; every agent-first tool is a cloud service.
- Connector-declared semantics. Bruin is the only EL tool where the asset
  declares column meaning; none declares polarity or KPI status.
- Per-language enrichment usage and evaluation. Vendors list languages;
  none publishes per-language quality.

**Behind, in the order it would change what a person can do:**

1. No outward door. An MCP server over the action manifest, with a
   read-only variant and `input_required` for "Ask me first", closes this
   and half of the next item at once.
2. Findings do not leave the app. Digest plus off-cycle escalation to
   Slack or email is table stakes in every family.
3. No signals on definitions: last seen, volume, query count, ingestion
   warnings. The stats exist in the store; the Semantics page shows none.
4. No saved cohorts. Every product-analytics tool from Umami up has them,
   and agents use them as nouns.
5. No edit-before-accept on proposals. Databricks has it; the fourth
   principle in `docs/human_ai_interaction.md` asks for it.
6. No usage accounting or budgets per actor. Metabase v61 and Notion
   define the minimum: per-actor spend, per-group limits, a question-level
   log.
7. Rebuild everything after every sync. SQLMesh and dbt fingerprint
   definitions and skip; the models table already has versions and builds.
8. No synonyms, example questions or verified queries in the semantic
   model. Three cheap slots every LLM-grounded peer added.
9. No simulate-on-history for detector settings. Mixpanel and PostHog let
   a user see how often a watch would have fired before enabling it.
10. Connector breadth: one shipped, against hundreds everywhere else.

## 5. The ten things most worth stealing, ranked

1. **MCP server from the action manifest** (Linear's `/mcp/readonly`,
   MotherDuck's `query_rw`, the 2026-07-28 spec's `input_required`).
2. **Digest delivery with polarity-aware escalation** (Tableau Pulse:
   scheduled digest, off-cycle alert only for unfavourable moves).
3. **Signals on the Semantics page** (Mixpanel Lexicon: volume, query
   count; PostHog: last seen, verified, history, ingestion warnings).
4. **The Clay enrichment flow**: per-row cost before running, ten-row
   sample, run all, confidence and reasoning columns, free on empty. Half
   of this exists as cost estimate and sample run; the rest is UI.
5. **Vocabulary review cards** (Zendesk: coverage %, five examples,
   decline-with-reason; Thematic: coverage / specificity / accuracy
   scores; Linear: show the reasoning, accept standing guidance).
6. **Dual attribution and origin-scoped undo** (GitHub co-author, Yjs
   `trackedOrigins`) so undo-all on an agent run is safe with two people in
   a workspace.
7. **Detector calibration UX**: simulate on history (Mixpanel, PostHog),
   sensitivity as a target flag rate (Databox), grouping co-occurring
   findings into one incident (Anodot, PostHog Reports).
8. **Model tests and fingerprinted rebuilds** (Rill model tests, SQLMesh
   plan/apply, dbt state explain).
9. **Litehouse as a public format**: publish the catalog schema with a
   version table, and a frozen read-only snapshot any DuckDB can open
   (DuckLake 1.0 as the reference).
10. **Ossie `custom_extensions` for polarity, KPI, roles and provenance**,
    so round-trips stop losing what Brightflow is ahead on.

---

## Sources

Every product entry above is expanded, with the URLs actually fetched and
the *(unverified)* marks, in
`reports/2026-09-14_competitors-and-inspirations-appendix.md`, one section
per family in the same order as §2.
