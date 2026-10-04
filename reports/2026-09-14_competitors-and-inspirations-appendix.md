# Appendix: Raw Research Passes Behind the Competitors and Inspirations List

**Date:** 2026-09-14
**Scope:** The eight research passes, unedited except for section headers,
that `reports/2026-09-14_competitors-and-inspirations.md` consolidates. Kept
so every claim in the main report can be traced to a URL, and so the
*(unverified)* marks survive. Each pass was a separate web-research agent
given the same description of Brightflow; several ran out of web-search
budget partway and finished on direct page fetches, which each pass notes
at its top. Status claims are as of this date and are not maintained.
**Baseline:** HEAD `08dc6ac`.

---

## Pass 1: Local-first / open-source / BI-as-code / DuckDB-era BI — competitive slice for Brightflow (as of 2026-09-14)


Legend: **C** = competitor, **I** = inspiration, **B** = both, **A** = adjacent. Claims without a fetched primary source are marked (unverified).

---

#### Rill — https://www.rilldata.com · https://github.com/rilldata/rill
BI-as-code: SQL models + YAML metrics views → instant Explore dashboards and Canvas pages; DuckDB embedded by default, ClickHouse for billion-row scale.
- **Status:** Apache-2.0, ~2.9k stars. Cloud pricing page returned 403 (tier shape unverified). Release cadence is monthly: 0.63 MCP server → 0.73 "Project AI Agent" → 0.75 AI charts → 0.77 "Explain Anomalies" + dashboard chat with context → 0.80 "AI Developer Agent" + dark mode (2026) → 0.81 shareable AI conversations + bring-your-own Claude → 0.82 AI feedback → 0.83 Canvas generation → 0.86 DuckLake/BigQuery live connectors + "Explain this error" → 0.87 metrics-view rollups → 0.89 (Aug 19 2026) pivot conditional formatting, scheduled Canvas PDF.
- **Deployment:** `curl https://rill.sh | sh` → single Go binary, `rill start` opens a local UI over a project directory of files. Rill Cloud for sharing.
- **Overlap:** explore (their Explore is a metrics-view browser, very close to Brightflow's Explore), semantic layer (metrics views in YAML), models (SQL models, incremental), AI/agents (MCP + two in-product agents), insights ("Explain Anomalies" is LLM-narrated, not a detector suite), dashboards (Canvas). No event analytics.
- **Steal:** (1) "Explain this error" — every failing model/metrics file gets a one-click LLM diagnosis, cheap and loved. (2) The project directory *is* the state: models, metrics, dashboards as files agents can edit; Rill's developer agent literally writes YAML. (3) "Model tests" (0.67) — assertions on derived tables that run after each rebuild; Brightflow models rebuilt after every sync want exactly this.
- **Verdict: B** — closest open-source architectural twin (single binary, DuckDB, metrics layer, agents), but Rill is dashboards-first, has no event tracking, and its insights are LLM-only.
- Sources: https://docs.rilldata.com/notes · https://github.com/rilldata/rill · https://www.rilldata.com/blog/rill-agentic-architecture-analytics-for-the-ai-era (403 on fetch; MCP/metrics-SQL claims from search summaries)

#### Evidence — https://evidence.dev · https://github.com/evidence-dev/evidence
SQL + Markdown → static reporting sites ("BI as code"), DuckDB "Universal SQL" in the build.
- **Status:** MIT, ~6.9k stars. **Evidence Studio** (launched Jun 6 2025) is the cloud future: web IDE with a schema-aware AI assistant that generates Evidence code and reads validation feedback, a new compact syntax (aggregation in component props, ~60% less code claimed), plus viewer-side AI chat / explore / SQL console; self-serve Pro tier with SSO/SCIM. OSS README now bills it as "agent-ready", suggests Claude Code/Cursor as authoring tools. Weekly active teams 1,113 → 1,492 last quarter (unverified, search snippet).
- **Deployment:** CLI (`evidence init`), builds a static site; self-host anywhere or Studio.
- **Overlap:** saved views/dashboards (reports), models (sources → queries chain), AI (Studio), no semantic layer to speak of, no insights, no events.
- **Steal:** (1) Report artifact = plain text file → trivially diffable, trivially LLM-editable; Brightflow's saved views/chains could have a text serialization for the same reason. (2) The "validation feedback to the agent" loop: agent edit → compile → errors fed back before the human sees anything.
- **Verdict: I** (A for the Studio cloud).
- Sources: https://evidence.dev/blog/evidence-studio · https://github.com/evidence-dev/evidence

#### Metabase — https://www.metabase.com · https://github.com/metabase/metabase
The default open-source BI: query builder, SQL, dashboards, models, now transforms and an AI (Metabot).
- **Status:** AGPL OSS + commercial license, ~49k stars. Pricing: OSS free; Starter $100/mo (5 users, +$6/user); Pro $575/mo (10 users, +$12/user); Enterprise from $20k/yr. Usage lines: AI tokens $3.75 per 1M (1M included), transforms $0.01/run after 1,000, storage from $2/1M rows. 2026 AI cadence: v58 (Jan) Metabot GA as paid add-on → v59 (Mar) AI SQL generation to OSS → v60 (Apr) "AI went open source": self-hosted Metabot with your own key, official MCP server, Slack → v61 (May) AI governance (per-group limits, custom system prompts, usage analytics down to tokens), dashboards-as-code from AI terminals → v62 (Jun) MCP renders charts inside Claude/Cursor/VS Code, Metabase CLI, schema viewer → v63 (Jul) treemaps, 2FA, OpenAI/Bedrock/Azure providers.
- **Deployment:** JAR or Docker, self-hosted or Cloud. Not single-binary but close (one JAR + app DB).
- **Overlap:** explore/query (GUI builder), saved questions/dashboards, semantic (table/column metadata, metrics), models/transforms (v61+ "transforms" are materialised scheduled SQL — Brightflow's models), AI (Metabot, MCP, CLI). No detector-based insights, no event tracking.
- **Steal:** (1) AI usage analytics per question and per token — Brightflow already estimates enrichment cost; a per-agent-run ledger in the activity feed is the natural extension. (2) The MCP + CLI + "dashboards-as-code" triad: the same action surface exposed three ways. (3) Per-group AI limits/system prompts.
- **Verdict: C** for the general case, but Metabase's AI is cloud-shaped (token metering) and it has no local-file/Parquet story.
- Sources: https://www.metabase.com/releases-ai · https://www.metabase.com/pricing · https://github.com/metabase/metabase

#### Lightdash — https://www.lightdash.com · https://github.com/lightdash/lightdash
dbt-native (or standalone YAML) semantic-layer BI that rebranded itself around "AI agents you can trust".
- **Status:** OSS (license not confirmed by fetch; MIT unverified), ~6.1k stars. Pricing: OSS self-hosted free, *no AI agents*; Cloud Pro $3,000/mo flat, unlimited users, AI included; Enterprise custom. Agents are configured with instructions, explores, evaluations, verified answers, knowledge documents and "Issues" (flagged wrong answers that feed context fixes); Slack/Teams with per-user OAuth; multilingual; LLM-agnostic (Anthropic/OpenAI). Repo carries an `agent-harness` dir and "Lightdash skills" for coding agents; Sept 2026 PRs mention an "autopilot" heartbeat runtime (always-on agent).
- **Deployment:** Docker/Kubernetes self-host; agents cloud-only.
- **Overlap:** semantic layer/metrics, explore, dashboards, AI. No models/materialisation, no insights engine, no events.
- **Steal:** (1) **Evaluations** — a stored set of question→expected-answer pairs run against the agent after every semantic change; Brightflow's vocabulary-versioned enrichment already has "sample runs" and could generalise this to the whole agent. (2) "Verified answer" + "Issue" as first-class objects tied back to the semantic row that caused them — maps neatly onto Brightflow's per-row provenance.
- **Verdict: B**.
- Sources: https://www.lightdash.com/pricing · https://www.lightdash.com/ai-agents · https://docs.lightdash.com/guides/ai-agents · https://github.com/lightdash/lightdash

#### Apache Superset / Preset — https://superset.apache.org · https://preset.io
The heavyweight OSS dashboard platform; Preset is the commercial cloud.
- **Status:** Apache-2.0. 6.0 (2025): AntD v5 theming overhaul. **6.1** (May/Jun 2026, 1,500+ PRs, 136 contributors): MCP service as a separate JWT-authenticated process (150+ PRs; tools for charts, dashboards, datasets, SQL Lab, virtual datasets, DB inspection, awareness of *unsaved* dashboard state), code-first Extensions framework, Global Task Framework (`@task` decorator, dedupe, REST for status/cancel), Matrixify pivot layout. Preset pricing: Starter free ≤5 users; Pro $20/user/mo (14-day AI chatbot trial); Enterprise = full chatbot (BYO OpenRouter key) + MCP. Preset Agent Skills (Jun 2026) = open-source skill packs for Claude Desktop/Cursor/Copilot.
- **Deployment:** Python + Postgres + Redis + Celery; Docker/k8s. The opposite of single-binary.
- **Overlap:** explore, dashboards, semantic (datasets, metrics), AI via MCP. No models, no insights, no events.
- **Steal:** (1) Ship *skills* (curated agent guidance with safety boundaries and tool routing) alongside the action bus — cheap, and makes external agents behave. (2) MCP as an out-of-process, separately-authenticated service rather than in the main server.
- **Verdict: A** (too heavy to compete for "one workspace, a few people"); I for the MCP/skills design.
- Sources: https://preset.io/blog/apache-superset-6-1-release/ · https://preset.io/pricing/ · https://preset.io/blog/announcing-preset-agent-skills/ (search-level)

#### Redash — https://redash.io · https://github.com/getredash/redash
SQL-query-and-dashboard tool from the 2015 era.
- **Status:** BSD-2, community-maintained in explicit maintenance mode (~7 maintainers), Databricks bought it in 2020, hosted plans sunset; latest v26.3.0 (year-based versioning → March 2026; GitHub page rendered the date inconsistently). No AI.
- **Deployment:** Docker Compose (Python, Postgres, Redis).
- **Overlap:** query/saved queries/dashboards only.
- **Steal:** nothing new; its "query results as reusable tables" is the ancestor of Brightflow models.
- **Verdict: A**.
- Sources: https://github.com/getredash/redash/releases · https://redash.io/community/ (search-level)

#### Quary — https://github.com/quarylabs/quary
"Open-source BI for engineers": Rust CLI + VS Code extension, SQL models, charts, dashboards (WIP), deploys models back to the DB.
- **Status:** Apache-2.0, ~2.4k stars, YC W24. Last release v0.10.1 on Jan 23 2025 was dependency bumps; nothing since → effectively dormant.
- **Deployment:** Rust binary + VS Code; DuckDB/SQLite/Postgres/BigQuery/Snowflake/Redshift/Supabase.
- **Overlap:** models/transforms, dashboards-as-code. No AI, no semantic layer, no insights.
- **Steal:** cautionary only — a Rust "BI for engineers" without a GUI-first experience didn't find a market.
- **Verdict: A** (dormant).
- Sources: https://github.com/quarylabs/quary · https://github.com/quarylabs/quary/releases

#### Datasette — https://datasette.io
Publish and explore SQLite tables as a browsable site + JSON API, huge plugin ecosystem (Simon Willison).
- **Status:** Apache-2.0 (unverified by fetch), Python. Still on the 1.0 alpha train: 1.0a39 and 0.65.4 security releases on Sep 11 2026, vulnerabilities found with frontier-LLM auditing; Datasette Cloud is the hosted product. No stable 1.0 date.
- **Deployment:** `pip install`, one process over SQLite files; also Datasette Lite in-browser (WASM).
- **Overlap:** explore (facets, filters, table views over a SQLite catalog — Brightflow's Litehouse is conceptually a Datasette catalog over Parquet), JSON API for every view. No dashboards to speak of, plugins for charts; AI via LLM plugins (unverified).
- **Steal:** (1) Every view is a URL and every URL has a `.json` twin — the single most agent-friendly property in the slice; make every Brightflow Explore chain addressable the same way. (2) Facets with counts as the default first look at any table. (3) Plugin hooks as the extension model instead of a monolith.
- **Verdict: I**.
- Sources: https://datasette.io/blog/2026/september-security-releases · https://simonwillison.net/2026/Sep/11/datasette-2/

#### DuckDB local UI + MotherDuck — https://duckdb.org/2025/03/12/duckdb-ui · https://motherduck.com
`duckdb -ui` starts a local notebook UI; MotherDuck is the cloud DuckDB with agent products.
- **Status:** UI extension backend is MIT (duckdb/duckdb-ui, ~464 stars) but the frontend is proprietary and proxied from ui.duckdb.org; offline mode "under development"; "most of the UI code is not yet publicly available". MotherDuck 2026: **Dives** GA (agent-built interactive data apps with version history and a draft → ready → endorsed → archived lifecycle, embeddable), **Flights** GA Sep 3 2026 (agent-native scheduled pipelines, all plans incl. Free), remote MCP server at api.motherduck.com/mcp with an explicit `query_rw` tool, CLI preview (Aug 14 2026), Terraform provider (Sep 10 2026), DuckDB 1.5.5.
- **Deployment:** UI is local process + remote-served frontend; MotherDuck cloud.
- **Overlap:** explore (SQL notebook + column explorer), AI/agents (MCP → Dives), models (Flights). No semantic layer, no insights, no events.
- **Steal:** (1) The **Column Explorer** panel: per-column min/max, null %, value distribution shown beside every result grid — cheap on Polars, huge for trust. (2) The Dive status lifecycle (draft/ready/endorsed/archived) is a lightweight governance model for agent-made artifacts. (3) Read vs read-write split as separate MCP tools.
- **Verdict: I** (A for MotherDuck as a platform).
- Sources: https://duckdb.org/2025/03/12/duckdb-ui · https://github.com/duckdb/duckdb-ui · https://motherduck.com/docs/about-motherduck/release-notes/

#### Malloy / Composer / Publisher — https://github.com/malloydata
Semantic modelling + query language (Looker lineage); Composer is a demo dashboard app; **Publisher** is the new server.
- **Status:** Malloy MIT; Composer README says "demo purposes only, not a supported product". Publisher (MIT, by Credible): one model served over REST **and MCP**, DuckDB built-in, connectors for BigQuery/Snowflake/Postgres/MySQL/Trino/Databricks/MotherDuck/DuckLake, `npx` or Docker, ships bundled agent skills (`search_database_schema`, `compile_model`, analysis).
- **Overlap:** semantic layer, query language, AI via MCP. No GUI explore worth copying, no insights.
- **Steal:** (1) `compile_model` as an agent tool — validation as a first-class tool call so agents iterate on the semantic model before proposing it; Brightflow's describe-table → propose-model run could expose the same. (2) Skills shipped in the same repo as the server.
- **Verdict: I**.
- Sources: https://github.com/malloydata/publisher · https://github.com/malloydata/malloy-composer

#### Omni — https://omni.co
Enterprise BI with a "just-in-time" semantic model, workbooks, spreadsheets, and AI.
- **Status:** Proprietary, cloud. Series C $120M at $1.5B (Apr 23 2026), 4x YoY revenue; positions the semantic model as a "governed context graph" consumed by Claude/ChatGPT/Cursor/VS Code.
- **Overlap:** explore/workbooks, semantic layer, AI. Enterprise scale, no self-host.
- **Steal:** the workbook → promote-to-shared-model flow (Omni's signature "start ad hoc, promote what sticks") is essentially Brightflow's layered semantic model with provenance; Omni proves the market wants it (flow detail unverified from primary).
- **Verdict: A**.
- Sources: https://omni.co/blog/press-release-omni-series-c-funding

#### Count — https://count.co
Infinite-canvas data workspace (whiteboard + SQL IDE + notebook + dbt) with an in-canvas AI agent.
- **Status:** Proprietary cloud. Free (3 editors, 3 canvases, AI fair use), Pro $49/editor/mo, Scale $69/editor/mo (15-seat min), Enterprise. The agent's conversations persist and are visible to everyone; every cell it makes is an ordinary canvas cell you can audit/edit; metadata-only DB access by default, row data requires approval; AI off by default in older workspaces.
- **Overlap:** explore, saved canvases, AI/agents (very close philosophy to Brightflow's "pair analysis"), models (dbt). No insights, no events.
- **Steal:** (1) "Agent output is just cells" — no separate AI pane; Brightflow's action bus already does this, but Count's visible-reasoning-in-place is the UX to match. (2) Explicit metadata-only vs row-level consent for the model.
- **Verdict: B** (closest UX philosophy, cloud-only).
- Sources: https://count.co/pricing · https://learn.count.co/count-ai-agent

#### Hex — https://hex.tech
Notebook + app builder; Notebook Agent.
- **Status:** Proprietary; Community free, Pro $36/editor, Team $75/editor, Enterprise (single-tenant add-on); AI metered in credits with per-seat grants. $70M Series C (May 2025, $172M total). Notebook Agent (Aug 27 2025): plan → find tables → SQL/Python cells → charts → summary, with **cell-level diff accept/reject** and an **auto-saved version per agent thread** so you can roll back a dead-end; Claude Sonnet 4 at launch; Dec 2025 updates added cell types and context awareness; semantic-model grounding promised.
- **Overlap:** explore/notebooks, AI/agents, saved apps. Warehouse-only, no local files, no insights engine.
- **Steal:** (1) Version-per-agent-thread + cell-level diffs — a richer form of Brightflow's one-step undo; consider "undo the whole agent run" as a unit. (2) Credits visibility per agent run.
- **Verdict: B**.
- Sources: https://hex.tech/blog/introducing-notebook-agent/ · https://hex.tech/pricing/

#### Mode — https://mode.com
- **Status:** Acquired by ThoughtSpot ($200M, 2023) and absorbed into ThoughtSpot Analyst Studio in 2025; no longer independent. **Verdict: A** (gone).
- Sources: https://www.thoughtspot.com/press-releases/thoughtspot-completes-200m-acquisition-of-mode-analytics (search-level)

#### Holistics — https://www.holistics.io
Code-first semantic BI (AML modelling, AQL query language) with Holistics AI.
- **Status:** Proprietary cloud; pricing per Capterra: Entry $800/mo (10 users), Standard $1,000/mo, security suite $2,000/mo (unverified). Holistics AI (May 28 2025, beta): generates AQL against the governed layer, "queries in steps, not all at once", OpenAI, metadata only; MCP server + CLI for agents (search-level, unverified).
- **Overlap:** semantic layer, explore, AI. No local, no insights, no events.
- **Steal:** the explicit argument that a **composable, stepwise query language** is what makes LLM output inspectable — Brightflow's typed operation chain is exactly that; lean into "the agent proposes chain steps, never SQL".
- **Verdict: I**.
- Sources: https://www.holistics.io/blog/introducing-holistics-ai/ · https://docs.holistics.io/as-code/amql (search-level)

#### Briefer — https://github.com/briefercloud/briefer
Notion-style notebooks + dashboards (SQL/Python/Markdown), DuckDB file upload, "Edit/Fix with AI" (OpenAI key).
- **Status:** AGPLv3; README now states "Briefer has been acquired by Resend"; briefer.cloud returns 404. Treat as ended.
- **Verdict: A**.
- Sources: https://github.com/briefercloud/briefer

#### Observable Framework / Notebooks 2.0 — https://observablehq.com/notebook-kit/
- **Status:** Notebooks 2.0 tech preview (Jul 31 2025): open HTML file format, open-source Notebook Kit for static builds, Observable Desktop (macOS), vanilla JS instead of Observable's dialect, explicitly "local-first" with web collaboration and AI to follow. 2026 state unverified (fetches rate-limited/blocked). Framework remains the static-site + build-time data loaders tool.
- **Overlap:** reports/dashboards as files. No semantic layer, no insights.
- **Steal:** build-time **data loaders** (any script that emits a file becomes a dataset) — a nice shape for Brightflow's Lua connectors' output contract.
- **Verdict: I**.
- Sources: https://observablehq.com/blog/previewing-notebooks-2 (429) · https://simonwillison.net/2025/Aug/6/observable-notebooks-20/ (search-level)

#### Grafana — https://grafana.com
Observability dashboards; single Go binary; increasingly general-purpose SQL dashboards.
- **Status:** AGPL-3.0 (unverified by fetch). Grafana 13 (Apr 21 2026): dynamic dashboards GA, **Git Sync GA** (edit in UI → commit → open a PR without leaving Grafana; 13.1 adds signed/verified commits), Assistant for OSS/Enterprise (LLM is routed through a Grafana Cloud account even when data stays on-prem; "free AI, please don't bankrupt us"), Assistant CLI + Tunnel (Mar 2026), SQL expressions. 
- **Overlap:** dashboards, alerts, "as code". No semantic layer, no models, LLM narration only.
- **Steal:** (1) Git Sync's "PR from the UI" is the GUI-first answer to BI-as-code — Brightflow could export a view/model diff as a reviewable change without making users edit files. (2) The Assistant *tunnel* pattern: local data, remote model, explicit consent.
- **Verdict: I**.
- Sources: https://grafana.com/blog/grafana-13-release-all-the-latest-features/ · https://grafana.com/whats-new/2026-04-20-git-sync-for-grafana-dashboards-and-folders-now-generally-available/ (search-level)

#### marimo — https://marimo.io
Reactive Python notebook stored as a .py file; SQL/DuckDB cells; AI-native.
- **Status:** Apache-2.0 (unverified by fetch); acquired by CoreWeave (Oct 30 2025); 0.24.2 on Sep 11 2026. 2026: AI chat sidebar with web search, "error-as-context" chips and "fix in chat", `marimo pair` to attach external coding agents to a running notebook, auto-discovery of data sources from env vars, cached WASM exports, molab hosted.
- **Overlap:** explore (notebook), AI. No semantic/insights/events.
- **Steal:** (1) **Reactive dependency graph** — change a cell, dependents re-run; Brightflow's chain → models → views already has the graph; surface it and rebuild only what changed. (2) Errors as clickable context chips for the agent. (3) "pair": let an external coding agent drive the app via the same bus.
- **Verdict: I**.
- Sources: https://github.com/marimo-team/marimo/releases · https://www.coreweave.com/news/coreweave-acquires-marimo-to-unify-the-generative-ai-developer-workflow (search-level)

#### Streamlit — https://streamlit.io
- **Status:** Apache-2.0, Snowflake-owned; 2026 news is all Streamlit-in-Snowflake (container runtime GA Mar, Workspaces GA Jun, restricted caller's rights). Adjacent app framework, no BI features of its own. **Verdict: A**.
- Sources: https://docs.snowflake.com/en/release-notes/2026/other/2026-06-01-streamlit-in-workspaces-ga (search-level)

#### Quadratic — https://www.quadratichq.com
Rust/WASM/WebGL spreadsheet running Python/SQL/JS in cells with AI.
- **Status:** Reportedly went closed-source in March 2026 (unverified; the GitHub repo now 404s, consistent with removal). Proprietary cloud. **Verdict: A**.
- Sources: https://www.quadratichq.com/ (blocked) · https://github.com/quadratichq/quadratic (404)

#### Whaly — https://whaly.io
- **Status:** French self-service BI that has pivoted to "AI sales copilot & revenue intelligence" for retail/CPG (Danone, Lactalis testimonials); no public pricing. Out of this slice now. **Verdict: A**.
- Sources: https://whaly.io/

#### Definite — https://www.definite.app
All-in-one: 500+ connectors → DuckDB/DuckLake lakehouse → semantic layer → BI → "Fi" agent.
- **Status:** Proprietary. Free (2 users, 2 connectors, Fi), Standard $250/mo (unlimited users), Enterprise; credit-based on queries/connectors/agent time, not seats. Private deployment in your cloud (AWS/GCP/Azure/k8s). Fi builds whole data apps (sidebars, filters, KPIs) with component-level editing and SQL transparency; claims to "find anomalies" (method unverified). DuckDB-Wasm in the browser for sub-second filtering.
- **Overlap:** the whole stack — connectors, storage (Parquet-ish lakehouse), semantic, explore, dashboards, agent, "insights". No event tracking, no local-machine story.
- **Steal:** (1) Pricing on credits across queries + connectors + agent time is the honest model for an agent-heavy tool. (2) "The agent builds, not just answers" positioning and the component-level edit after generation.
- **Verdict: C** (the most similar *product thesis*: one vendor, DuckDB, agent that acts), cloud-scale rather than one-binary.
- Sources: https://www.definite.app/ · https://www.definite.app/blog/best-self-hosted-bi-tools (search-level)

#### Chartbrew — https://github.com/chartbrew/chartbrew
Dashboards from APIs and DBs (Node/React), AI assistant.
- **Status:** FSL with MIT conversion (README), ~4.1k stars; v5.x (2025) brought an AI orchestrator, a "Semantic Visualization Engine", Stripe/Jira connectors; a chat-based AI assistant that generates charts/datasets/dashboards is in beta (2026 version unverified).
- **Deployment:** Docker; needs MySQL/Postgres + Redis.
- **Overlap:** dashboards, light AI, API connectors.
- **Steal:** connector templates ("add Stripe → get 8 predefined insights") — a cheap on-ramp Brightflow's GitHub connector could copy.
- **Verdict: A**.
- Sources: https://github.com/chartbrew/chartbrew · https://github.com/chartbrew/chartbrew/releases

#### Steep — https://steep.app
Metrics-first BI (Swedish); semantic layer, no dashboards-first.
- **Status:** Proprietary cloud; Pro $15/seat (free ≤3 seats, 1,000 AI credits), Business $25/seat, Enterprise; AI moving to credits "later this year". 2026: API (Jan), **define-in-code** with git sync (entities, derived metrics, slices, cohorts, targets, access policies), reports via API + MCP and a ChatGPT app (Jul 23), @-mention reports as AI context, Claude Sonnet 5 (Jul 16), drill-down on pivot/cohort tables + targets on big numbers (Aug 18), EU/US model region (Aug 31), Okabe-Ito colour theme (Sep 8).
- **Overlap:** semantic/metrics, explore, AI, saved reports. No models, no events, no detectors.
- **Steal:** (1) Metrics as the primary nav object rather than tables/dashboards. (2) Targets attached to metrics (a natural input to Brightflow's forecast-deviation detector). (3) EU model-region toggle and @-mentioning saved reports as agent context.
- **Verdict: I**.
- Sources: https://steep.app/changelog · https://steep.app/pricing

#### Push.ai — https://push.ai
"Business Graph" (metrics + ownership + context) with governed agents (agentic analytics, automated reviews, in-flow answers) and a Slack app.
- **Status:** Proprietary cloud, no public pricing, free trial; no dated 2025–26 launches visible.
- **Overlap:** semantic/metrics graph, AI agents, delivery to Slack.
- **Steal:** metric **ownership** as a semantic attribute (who to ask), which Brightflow's provenance layer could carry.
- **Verdict: A**.
- Sources: https://push.ai/ · https://www.push.ai/slack-app (search-level)

#### Sigma — https://www.sigmacomputing.com
- **Status:** Proprietary spreadsheet-on-warehouse; $80M Series E at $3B (May 18 2026), "agentic analytics" pivot. Enterprise, cloud-only. **Verdict: A**.
- Sources: https://siliconangle.com/2026/05/18/sigma-computing-seals-80m-funding-round-pivots-toward-agentic-analytics/ (search-level)

#### Draxlr, Cluvio — https://www.draxlr.com · https://www.cluvio.com
- **Status:** SQL-native SaaS dashboards with text-to-SQL. Draxlr $25–$125/mo tiers; Cluvio free 1-user plan, Pro $279/mo, Business $669/mo, on-prem option (all from third-party listings, unverified). **Verdict: A** both.
- Sources: https://www.draxlr.com/blogs/ai-powered-bi-tools/ · https://www.capterra.com/p/162332/Cluvio/ (search-level)

#### Blendo — skipped: acquired by RudderStack in May 2020, no longer a product.

---

#### New 2025–2026 entrants and single-binary/local-first finds

- **Wren AI** — https://github.com/Canner/WrenAI · Apache-2.0, ~17.6k stars. May 7 2026 inversion: the semantic engine moved into `core/`, the chat-first "GenBI" app was parked on `legacy/v1` as "Classic"; now a CLI (`pip install wrenai`) with MCP, Git Sync deploys, 22+ sources incl. DuckDB. Pattern: *the context layer is the product, the UI is optional*. **I** — validates a semantic layer with MCP as the agent contract.
- **nao** — https://github.com/getnao/nao · Apache-2.0, ~1.7k stars, YC. Launched Feb 10 2026 as an "analytics agent builder": `nao init/sync/chat`, file-system-shaped context (data, metadata, docs, tools, MCPs), Docker; May 2026 scheduled automations; Jun 2026 the agent audits its own answers and opens PRs against the context repo; "headless" positioning. **I** — the self-audit → PR loop is a strong idea for Brightflow's semantic-model proposals.
- **MindsDB Anton** — https://github.com/mindsdb/anton · MIT, launched Apr 2026, `curl … | sh` single-command local runtime, `.anton/` workspace with episodic/semantic/long-term memory, Anthropic/OpenAI/any OpenAI-compatible endpoint; general autonomous agent that also produces dashboards; ~750 stars. **A** — not BI-specific despite the press.
- **Buster** — buster.so now 301s to keystroke.ai (fetch blocked): the open-source "AI data analyst on dbt" appears to have renamed/pivoted (unverified). **A**.
- **SQLPage** — https://github.com/sqlpage/SQLPage · Rust single executable, `.sql` files rendered into forms/tables/charts; Postgres/MySQL/SQLite/MSSQL/ODBC; pre-1.0 (2022–). **I** — proof that "single binary + files = whole app" works for data UIs; no AI.
- **toptopduck** — https://github.com/toptopworks/toptopduck · AGPL-3.0, Tauri v2 + embedded DuckDB + React + Vega-Lite, BYOK agent loop *and* an external-CLI-agent mode via an MCP tool gateway, "only schema + minimal sample rows + queries leave the machine", refuses unsupported requests. 0 stars, 1,550 commits — nobody has noticed it yet. **I** — an almost exact local-first architecture twin worth reading for its privacy contract wording.
- **Dataline** — https://github.com/RamiAwar/dataline · OSS chat-with-your-data, Docker or binary, local storage, can hide row data from the LLM. **A**.
- **PyGWalker / Graphic Walker** — https://github.com/Kanaries/pygwalker · ~15.7k stars, DuckDB-backed Tableau-style drag-and-drop over dataframes, embeddable Graphic Walker component. **I** — the cheapest way to see what a keyboard-poor but discoverable "shelf" explore UI looks like.
- Also seen, unverified: Duckle (Tauri+DuckDB ETL studio), Duckling (Tauri Parquet/CSV viewer), duckcode-ai/dql (git-native "certified answers" on DuckDB/Snowflake), Helical Insight (Java "open-source agentic BI").

---

#### Patterns across this slice (2025–2026)

1. **MCP became the lingua franca for agents.** Rill (0.63), Metabase (v60/62), Superset (6.1), Malloy Publisher, MotherDuck, Steep, Preset, Wren, Lightdash — all expose the product to *external* agents (Claude, Cursor, ChatGPT) rather than only bundling a chatbot. Brightflow's action bus is the right primitive but is internal; exposing it as an MCP server (read tools vs `*_rw` tools, MotherDuck-style) is the single most obvious gap.
2. **The semantic layer is now called "context" and it lives in files.** YAML metrics views (Rill, Lightdash), AML (Holistics), define-in-code (Steep), Wren's `core/`, nao's context repo, Evidence's markdown — chosen because agents edit files well and git gives review. Brightflow's layered detected → connector → agent → user model with per-row provenance is *ahead* on UX (nobody else shows provenance per row) but *behind* on being exportable/diffable as text and reviewable as a change set.
3. **BYO key and "AI in open source" won.** Metabase v60, Rill 0.81, Preset (OpenRouter), marimo, Anton all let self-hosters plug their own model; where AI stayed cloud-only (Lightdash agents, Grafana Assistant's Cloud-account requirement, Evidence Studio) it is the paid moat. Brightflow's any-OpenAI-compatible, fully-offline stance is now table stakes for OSS and a differentiator against the cloud-gated ones.
4. **Agent work is being made reviewable, not just undoable.** Hex (cell diffs, version per agent thread), Count (agent output = ordinary cells with visible reasoning), MotherDuck Dives (draft/ready/endorsed/archived), Grafana Git Sync (PR from UI), nao (agent opens PRs on its own context). Brightflow's "undo over approval" is genuinely rarer than approval flows — nobody found offers one-step undo across human *and* agent actions on one feed — but the slice suggests adding run-level grouping (undo the whole agent run) and a lightweight status on agent-made artifacts.
5. **"Insights" almost everywhere means an LLM explaining a chart.** Rill "Explain Anomalies", Definite Fi "finds anomalies", Grafana Assistant investigations. A deterministic nine-detector engine with effect-size scoring and multiplicity control is not something this slice has; Brightflow is ahead here and should say so loudly (and borrow Steep's *targets* as a detector input).
6. **Single-binary / local-first is a thin, mostly AI-less niche.** Rill and Grafana are the credible single binaries; DuckDB's UI is local but its frontend is closed and can't run offline; SQLPage, Anton and toptopduck are the only Rust/local newcomers. Brightflow is unusual in combining single binary + Parquet on disk + built-in agent; the competitors that match the *thesis* (Definite, Count, Hex, Omni) are all cloud.
7. **Consolidation and attrition are heavy on the open-source side.** Mode → ThoughtSpot, Briefer → Resend, marimo → CoreWeave, Quary dormant, Buster renamed, Quadratic closed-source, Redash in maintenance, Whaly pivoted out of BI — while AI-native cloud BI raised at $1.5B (Omni) and $3B (Sigma). Small OSS BI with no AI story is not surviving.
8. **Where Brightflow is behind:** connector breadth (Definite 500+, Metabase 20+, Chartbrew templates vs GitHub only), dashboard/Canvas maturity and scheduled delivery (Rill PDF, Metabase subscriptions, Steep/Push Slack), evaluation harnesses for the agent (Lightdash evals, nao self-audit), column-level profiling in the grid (DuckDB UI), and addressable JSON-per-view (Datasette). **Where it is unique:** first-party event tracking + funnels + retention inside a BI tool (nothing in this slice does it), detector-based insights, and a single action bus shared by humans and agents with undo.

---

## Pass 2: AI-native / agentic analytics — competitor and inspiration scan (as of 2026-09-14)


Legend for "grounding": **free SQL** = LLM writes SQL/code against raw schema; **semantic** = LLM constrained to a governed metrics/semantic layer; **tools** = LLM calls typed, product-owned actions; **engine** = a deterministic engine computes numbers and the LLM only narrates.

---

#### Chat-first "AI data analyst" startups

##### Julius AI — https://julius.ai
- **What**: Consumer/prosumer "chat with your file" analyst; LLM writes Python/R, runs in a sandbox, returns charts.
- **Status**: Proprietary. Free (15 msgs/mo), Plus $20, Pro $45, Team $50/user, Business $375/mo (10 users). ~$11M raised (YC, Bessemer, 8VC), >$15M ARR reported by third parties (unverified). Connectors now include Snowflake, BigQuery, Postgres, Google/Meta Ads.
- **Deployment**: Cloud only; no BYO model; no MCP (per a March 2026 review).
- **How AI works**: Free code generation; the LLM-generated code *is* the source of numbers. Reviewers flag non-reproducibility ("same question, same data, different results"). No approval gate; edits are re-prompts.
- **Overlap**: CSV upload + chart. Otherwise the opposite architecture to Brightflow.
- **Steal**: (1) Zero-friction file-drop onboarding; (2) mobile apps for reading analyses.
- **Verdict**: adjacent (low-end consumer).
- **Sources**: https://mcpanalytics.ai/articles/julius-ai-review, https://coefficient.io/julius-ai-pricing

##### Fabi.ai — https://www.fabi.ai
- **What**: AI-native "smartbook" (SQL + Python notebook) with an AI analyst, workflows to Slack/email; "Fabi 2.0" (Feb 2026) pitches one analyst over warehouses, files, Sheets and SaaS apps (unverified — site blocked from fetch; from search summaries).
- **Status**: Proprietary; freemium from ~$39/mo. Blog product-news page only showed 2025 items (Workflows Jun 2025, full-screen chat Oct 2025).
- **Deployment**: Cloud (unverified on self-host/BYO model).
- **How AI works**: Free SQL/Python in notebook cells; user can inspect and edit code; no deterministic engine.
- **Overlap**: "Chain of steps you can inspect" is spiritually close to Explore's operation chain, but theirs is code cells.
- **Steal**: Scheduled "push the insight to Slack" workflows attached to an analysis.
- **Verdict**: competitor (SMB analyst tooling).
- **Sources**: https://www.fabi.ai/blog-categories/product-news, https://aichief.com/ai-data-management/fabi-ai/

##### Zenlytic (Zoë) — https://zenlytic.com
- **What**: Enterprise AI analyst over a warehouse, grounded in a YAML "Cognitive Layer" (semantic layer + natural-language instructions), git-versioned.
- **Status**: Proprietary, enterprise pricing. May 2026: "Zoë Self-Learning" (onboards itself). Zoë runs inside Claude, ChatGPT, Teams, Slack.
- **Deployment**: Cloud; warehouses Snowflake/BigQuery/Databricks/Redshift/Postgres/SQL Server.
- **How AI works**: **semantic**; a "Clarity Engine" checks results against the governed layer before render; every figure cited back to source tables/metrics (marketing claim). No undo concept; semantic layer edits go through PRs.
- **Overlap**: Layered semantic model with provenance is Brightflow's core, minus the git/PR flow.
- **Steal**: (1) NL "instructions" as a first-class part of the semantic layer (tribal knowledge next to typed definitions); (2) citing every number to the metric definition.
- **Verdict**: inspiration.
- **Sources**: https://zenlytic.com/blog/introducing-the-cognitive-layer, https://natlawreview.com/press-releases/zenlytic-launches-zoe-self-learning-ai-data-analyst-onboards-itself

##### Basedash — https://www.basedash.com
- **What**: AI-native BI: NL → SQL grounded in "Basedash Models" (SQL definitions with dimensions/measures), dashboards, Automations, daily "Insights" briefings, MCP server, AI Kit (developer API).
- **Status**: Proprietary; Starter/Growth/Enterprise; seed-stage, Montreal. Ships weekly in 2026: Tasks (Aug 14), Public Sharing (Aug 19), Grok bot (Aug 26), **AI Sources** (Sep 2).
- **Deployment**: Cloud, plus self-hosted/VPC on Enterprise.
- **How AI works**: **semantic + free SQL**: "every answer is computed from your governed metric definitions and shows the SQL". AI Sources shows context chips (tables, definitions, charts, MCP connections, web pages) and lets you inspect exact SQL + preview rows; **mutating actions render as separate cards** beneath the answer.
- **Overlap**: High — same "small team, AI-native BI, semantic definitions" wedge, but warehouse-centric.
- **Steal**: (1) AI Sources: collapsible "Analyzed for…" provenance with query inspection inline; (2) separating *answers* from *mutations* visually — maps directly onto Brightflow's action bus.
- **Verdict**: competitor.
- **Sources**: https://www.basedash.com/blog/introducing-basedash-ai-sources, https://www.basedash.com/

##### Vanna.ai — https://vanna.ai
- **What**: MIT RAG text-to-SQL Python framework; "Vanna 2.0" is agent-shaped (Tool base class, RunSqlTool, user-aware permissions).
- **Status**: **GitHub repo archived 2026-03-29** (23.8k stars). Paid Vanna Cloud continues (unverified). Community forks exist.
- **Deployment**: Self-host, BYO LLM (Ollama etc.), BYO DB/auth.
- **How AI works**: free SQL with retrieved examples/DDL/docs; LLM-produced SQL is the source of numbers.
- **Overlap**: BYO-model philosophy; otherwise a library, not a product.
- **Steal**: "Train on verified question→SQL pairs" — a persisted, user-curated example store the LLM retrieves from.
- **Verdict**: adjacent (fading).
- **Sources**: https://github.com/vanna-ai/vanna, https://pub.towardsai.net/i-turned-an-archived-23k-star-text-to-sql-project-into-a-self-hosted-tool-that-actually-works-out-b08abcb6d0e3

##### Dot (getdot.ai) — https://www.getdot.ai
- **What**: AI data analyst in Slack/Teams/web; Chat, Deep Analysis, Automated Reports, "Context Agent" keeping metric definitions consistent. Customers cited: Duolingo, Choco.
- **Status**: Proprietary, YC; SOC 2 II. Pricing by request.
- **Deployment**: Cloud (VPC unverified); integrates Snowflake/Redshift/Looker/dbt.
- **How AI works**: **semantic + free SQL**, with a "developer training space" where the data team validates answers before Dot uses them.
- **Overlap**: The "agent maintains definitions" idea parallels Brightflow's agent layer in the semantic model.
- **Steal**: A dedicated *training* surface where a human reviews Q→answer pairs and promotes them to trusted.
- **Verdict**: inspiration.
- **Sources**: https://docs.getdot.ai/, https://www.getdot.ai/blog/ai-data-analyst-software

##### Wren AI — https://www.getwren.ai
- **What**: Apache-2.0 "open context layer" (MDL semantics, definitions, examples, memory, governance) exposed to any agent via CLI, Python/WASM SDK and **MCP**; Wren Cloud on top.
- **Status**: 17.6k stars. 2026-05-07 Wren Engine merged into core/; the chat UI was **retired to `legacy/v1`** (v1-final). Jun 22 2026 post: "Agents don't need a BI tool. They need your business's meaning as machine-readable context."
- **Deployment**: OSS self-host (air-gapped ok), Cloud, private cloud.
- **How AI works**: **semantic**; Agentic Mode is sandboxed multi-step with "traceable, replayable execution"; LLM is BYO via the host agent.
- **Overlap**: Strong on the semantic-model-as-product idea; they abandoned the GUI that Brightflow keeps.
- **Steal**: (1) Ship the semantic model as a machine-readable artifact + MCP so external agents (Claude Code) get the same grounding; (2) "skills" for distributing workflows.
- **Verdict**: both (inspiration for the outward door; competitor for the OSS-self-host buyer).
- **Sources**: https://github.com/Canner/WrenAI, https://www.getwren.ai/post/genbi-for-ai-agents-open-context-layer, https://www.getwren.ai/

##### camelAI — https://camelai.com
- **What**: Was "AI data analyst that builds dashboards/apps"; the homepage now leads with **Stream** ($5/mo flat-rate inference) and **camelCode** (MIT coding agent with persistent workspace). Data-analyst framing survives in blog and directories.
- **Status**: Proprietary + MIT agent; appears to have pivoted toward inference/app-building (unverified as an official pivot).
- **How AI works**: free SQL/code → apps; no deterministic engine.
- **Verdict**: adjacent.
- **Sources**: https://camelai.com/, https://shyft.ai/tools/camelai

##### Querio — https://querio.ai
- **What**: "Analytics as code": a git-versioned context layer (datasources, dbt config, knowledge, memory, skills) used by agentic notebooks; works with any agent via MCP/API/CLI; RLS, SOC2/HIPAA, white-label embed.
- **Status**: Proprietary; third-party pricing quotes vary ($29–49/user; self-host at +50% with $60k/yr minimum, unverified).
- **How AI works**: free SQL/Python with editable code; context layer for grounding.
- **Steal**: Per-workspace "memory + skills" files checked into git that both humans and agents edit.
- **Verdict**: competitor (analyst-team tier).
- **Sources**: https://querio.ai/

##### TextQL (Ana) — https://textql.com
- **What**: Enterprise agent that writes SQL/Python in a sandbox ("Sandcastles"), grounded in a git-backed **Ontology** (3.0 in 2026: token-efficient knowledge graph; "Ana builds your ontology with you"); Data Apps launch.
- **Deployment**: Cloud, self-hosted option via SDK (per blog).
- **How AI works**: **semantic + free SQL**; a state-machine "Dakota architecture" for tool access.
- **Steal**: Ontology built *incrementally through real work* rather than upfront — matches Brightflow's agent-layer provenance.
- **Verdict**: inspiration.
- **Sources**: https://textql.com/blog/building-data-agent, https://textql.com/blog/why-ontology

##### Findly — https://www.findly.ai
- **What**: Slack-first NL analyst for marketing data (GA, Meta Ads, BigQuery, Shopify). Free (3 users), Standard $149/mo, Enterprise (on-prem, SSO). "Semantic layer guidance" on paid tiers.
- **How AI works**: free SQL over live sources.
- **Verdict**: adjacent (marketing niche).
- **Sources**: https://www.findly.ai/pricing

##### Seek AI — https://www.seek.ai
- **Status**: **Acquired by IBM, June 2025** (~$10M raised); tech folded into watsonx AI Labs. Text-to-SQL for enterprise.
- **Verdict**: gone as an independent competitor.
- **Sources**: https://techcrunch.com/2025/06/02/ibm-acquires-data-analysis-startup-seek-ai-opens-ai-accelerator-in-nyc/

##### Onyx — https://onyx.app (and https://onyx.dev/ai)
- **What**: onyx.app is the open-source (ex-Danswer) enterprise chat/RAG platform (20k stars, self-host, BYO LLM, MCP, code interpreter, deep research). onyx.dev is a *different* company: "agent builder for structured data" with workflow trees executing resolvers in-database.
- **How AI works**: onyx.app: RAG + tools; onyx.dev: explicit workflow trees over tables (inspectable, testable) — closer to typed tools than free SQL.
- **Steal**: onyx.dev's "explicit workflow trees over prompt chains" is the same argument as Brightflow's typed operation chain.
- **Verdict**: adjacent.
- **Sources**: https://onyx.app/, https://onyx.dev/ai

##### Sherloq — https://www.sherloqdata.io
- **What**: Chrome extension SQL repository ("GitHub for queries") with AI naming/description/autocomplete. YC W23, ~$500K raised. Docs domain did not resolve at fetch time (status uncertain).
- **Verdict**: adjacent.
- **Sources**: https://www.ycombinator.com/launches/ICO-sherloq-collaborate-and-manage-your-sql-data-with-the-power-of-gen-ai

##### Athena Intelligence (Olympus) — https://www.athenaintel.com
- **What**: "AI-native operating layer" where every asset is agent-readable/writable with **per-cell attribution**; versioned "Agent Operating Procedures"; model-agnostic routing; cloud/VPC/on-prem/air-gapped. ~11 people, ~$1.2M 2025 revenue (third-party estimate).
- **Steal**: Per-cell attribution (who/what wrote each value) — Brightflow's per-row provenance generalised to outputs.
- **Verdict**: inspiration.
- **Sources**: https://www.athenaintel.com/company, https://getlatka.com/companies/athenaintelligence.ai

##### Numbers Station
- **Status**: **Acquired by Alation, May 2025** (>$17M raised). Agents now sit on Alation's catalog metadata.
- **Verdict**: gone.
- **Sources**: https://techcrunch.com/2025/05/20/alation-acquires-numbers-station-to-bolster-its-ai-agent-offerings

##### Arcwise — https://www.arcwise.app
- **What**: Pivoted from a Sheets copilot to "detect metric changes and explain them with traced, auditable insights" over Snowflake/BigQuery/Databricks + context docs; "hallucination-free AI, fully traced".
- **Overlap**: This is Brightflow's insights engine pitch (change detection → explanation) as a standalone product.
- **Steal**: Attach business-context documents to metric definitions so narration can cite them.
- **Verdict**: competitor (insights slice).
- **Sources**: https://www.arcwise.app/

##### Definite — https://www.definite.app
- **What**: All-in-one data platform (500+ connectors, lakehouse, BI) with agent "Fi" that builds full data apps; **DuckDB-Wasm in-browser** query; private deployment in your cloud. Free (2 users/2 connectors), Standard $250/mo, credit-based.
- **Overlap**: Highest architectural overlap: connectors + local columnar engine + AI, no separate warehouse.
- **Steal**: Fi's outputs carry "audit trails explaining every metric"; client-side DuckDB-Wasm for sub-second interaction.
- **Verdict**: competitor.
- **Sources**: https://www.definite.app/

##### Bruin — https://getbruin.com
- **What**: OSS pipeline CLI (ingest/transform/quality/lineage) + cloud with an AI analyst answering in Slack/Teams/WhatsApp/email from a shared semantic layer; "agentic dashboards"; official MCP server (Q2 2026) for Cursor/Claude Code.
- **Steal**: Anomaly/trend monitoring wired to the same semantic layer the chat uses.
- **Verdict**: competitor.
- **Sources**: https://getbruin.com/platform/, https://getbruin.com/blog/best-ai-bi-tools-2026/

---

#### Notebook / code-native

##### Hex (Notebook Agent, Threads) — https://hex.tech
- **Status**: "Magic" rebranded to Hex AI/agents. Notebook Agent (Dec 2025, updated Mar 2026) is DAG-aware (knows which cells are used/published/redundant), moves/deletes cells; **Threads** = conversational self-serve in Slack, Claude, Cursor, Codex; Context Studio + semantic model sync + "AI guides"; Hex CLI for external agents. Proprietary, free tier.
- **How AI works**: free SQL/Python in cells, grounded by endorsed semantic models; the human reviews cell diffs (no explicit approval gate documented).
- **Steal**: (1) DAG-aware agent that sees "in use / published / redundant"; (2) "AI guides" written by the data team as governance.
- **Verdict**: inspiration.
- **Sources**: https://hex.tech/blog/notebook-agent-updates/, https://hex.tech/capability/ai/

##### Deepnote — https://deepnote.com
- **Status**: Core is **Apache-2.0** on GitHub (~3k stars; `.deepnote` YAML, local kernel); agent is cloud-only, Pro+ plans. Agent has Edit/Ask modes, shows a step plan, before/after diffs, **"undo all changes from a run" via one bin icon**, user-selectable model provider with "Automatic".
- **Steal**: Per-run undo of everything an agent did — the closest thing to Brightflow's one-step undo found in this slice.
- **Verdict**: inspiration.
- **Sources**: https://github.com/deepnote/deepnote, https://deepnote.com/docs/deepnote-agent

##### Jupyter AI 3.0 — https://github.com/jupyterlab/jupyter-ai
- **Status**: BSD OSS; v3.0 targeted 2026-03-31. **Stopped building its own agent**; hosts Claude, Codex, Gemini, Goose, Kiro, OpenCode via Agent Client Protocol; ships a Jupyter MCP server; permission system requiring user approval for agent actions.
- **Steal**: Host external coding agents through a protocol instead of owning one — Brightflow's action bus could speak ACP/MCP so Claude Code drives Explore.
- **Verdict**: inspiration.
- **Sources**: https://github.com/jupyterlab/jupyter-ai/issues/1531

##### Datalayer — https://datalayer.ai
- **What**: 5-person startup; `jupyter-ai-agents` (OSS) where agents manipulate notebooks via MCP tools and run on separate compute; Pydantic AI + Logfire; supports AG-UI/ACP/A2A; marketing mentions approvals, traces, cost controls.
- **Verdict**: adjacent.
- **Sources**: https://jupyter-ai-agents.datalayer.tech/, https://pydantic.dev/case-studies/datalayer

---

#### Frontier-lab "analysis" products

##### ChatGPT / OpenAI — Data agent for ChatGPT Work (2026-09-10)
- **What**: `@Data` in ChatGPT Work: connects Redshift, BigQuery, ClickHouse, Databricks, MongoDB, Snowflake, Datadog, Drive/SharePoint; uses semantic context from dbt, Databricks Genie Ontology, Snowflake Horizon; builds interactive dashboards; pushes to Tableau/Power BI/Sigma/ThoughtSpot/Omni. Also ChatGPT for Excel/Sheets (Business); ChatGPT Work (Jul 9 2026) merges Codex into a desktop agent.
- **How AI works**: free SQL + code with imported semantic context; existing warehouse permissions enforced; "review source data behind results".
- **Overlap**: Not for self-hosters, but this is the gravitational competitor for "chat with my data".
- **Steal**: Consume other vendors' semantic layers rather than rebuilding — Brightflow could *export* its semantic model in dbt/MDL shape.
- **Verdict**: competitor (cloud).
- **Sources**: https://aicybr.com/blog/openai-data-agent-chatgpt-work-bi-data-sources, https://help.openai.com/en/articles/20001063-chatgpt-for-excel-and-google-sheets

##### Claude analysis tool / Claude for Excel & Sheets
- **What**: Sidebar add-in that reads multi-tab workbooks, writes formulas keeping dependencies intact, builds charts; MCP connectors to S&P, FactSet etc.; Sonnet/Opus 4.6 switch; requires paid Claude plan (from $20). (anthropic.com page returned 404 at fetch; details from third-party reviews.)
- **How AI works**: free formula/code generation; cell-level traceability reported by reviewers (unverified).
- **Verdict**: adjacent.
- **Sources**: https://houtini.com/articles/claude-in-excel/, https://www.luzmo.com/blog/claude-for-data-analysis

---

#### Platform / warehouse-native

##### Databricks Genie (Genie One, Agent mode) — https://www.databricks.com
- **Status**: Genie One GA 2026-06-16 (unverified date); Agent mode (ex "Research Agent"/Deep Research) creates a research plan, runs multiple SQL queries, iterates, returns report with citations; Sep 2 2026: Agent mode APIs (SSE streaming), file reasoning over Unity Catalog volumes; MCP tool calling; "trusted assets" (verified queries) answer from verified logic.
- **How AI works**: free SQL grounded by curated Genie Space instructions + trusted assets; LLM plans, engine (Databricks SQL) computes.
- **Steal**: (1) "Trusted asset" badge on answers derived from verified logic vs generated; (2) hypothesis-tree "why" investigations with citations — Brightflow's drivers detector could feed this deterministically.
- **Verdict**: inspiration (cloud-only).
- **Sources**: https://www.databricks.com/blog/expanding-genie-agents-deep-analysis-file-reasoning-and-more, https://docs.databricks.com/aws/en/genie/agent-mode

##### ThoughtSpot Spotter — https://www.thoughtspot.com
- **Status**: Spotter Semantics (Mar 12 2026): knowledge-graph semantic layer + MCP server; Spotter for Industries (Mar 18); SpotterViz/Model/Code agents; agentic data prep + spreadsheet UI; SpotCache fixed-cost. 64% of customers use Spotter (vendor claim).
- **How AI works**: Explicitly **not** LLM text-to-SQL: patented search tokens → deterministic SQL; LLM interprets. Closest philosophical neighbour to "engine computes, LLM narrates".
- **Steal**: Market the determinism: "same question, same answer, every time" as a headline property.
- **Verdict**: inspiration (enterprise).
- **Sources**: https://www.globenewswire.com/news-release/2026/03/12/3254770/0/en/ThoughtSpot-Introduces-Spotter-Semantics-to-Bring-Trust-and-Context-to-Enterprise-AI.html

##### Tableau Agent + Pulse + Tableau Next
- **Status**: Pulse = metric feed with scoped NL Q&A (one published source per metric, Cloud only, included in Cloud; advanced AI in Tableau+). Tableau Agent: NL viz/calc authoring, dashboard narratives, **Q&A Calibration** (experts test agent against curated questions). Tableau Next = semantic-layer-first rebuild on Data Cloud with task agents. (tableau.com blocked; from search + colrows.)
- **Steal**: Q&A Calibration: a curated question set the owner runs against the agent to score accuracy after changes.
- **Verdict**: adjacent.
- **Sources**: https://colrows.com/blogs/tableau-pulse-alternatives/, https://www.tableau.com/products/new-features

##### Power BI Copilot / Fabric data agents / Fabric IQ
- **Status**: Build 2026: Fabric IQ + Operations agents GA; Data agents in M365 Copilot GA; Copilot web modeling (NL edits to relationships/DAX); **Fabric Skills** (OSS instruction packs for GitHub Copilot/Claude/Cursor); Agent Skills for Power BI (build full reports). **Retirement**: Copilot in Power BI ↔ Fabric data agent integration retired 2026-08-26 (replaced by Fabric IQ skill). Advanced DAX generation runs as an iterative sub-agent with tools.
- **How AI works**: semantic-model grounded (DAX over semantic models); LLM + tool loop.
- **Steal**: Ship "skills" (markdown instruction packs) so coding agents use your product correctly — cheap distribution.
- **Verdict**: adjacent.
- **Sources**: https://www.jamesserra.com/archive/2026/06/microsoft-build-2026-announcements/, https://community.fabric.microsoft.com/blog/fbc_fabricupdatesblogs/retirement-of-fabric-data-agent-integration-in-copilot-in-power-bi/5328344

##### Looker Conversational Analytics / Conversational Analytics API
- **Status**: GA for BigQuery and Looker; cross-Explore questions (up to 5 Explores); the API exposes "data agents" with SQL gen, Python interpreter, Vega-Altair charts, stateful/stateless modes; docs still warn output "can be factually incorrect".
- **How AI works**: semantic (LookML) + free SQL/Python.
- **Verdict**: adjacent.
- **Sources**: https://docs.cloud.google.com/gemini/data-agents/conversational-analytics-api/overview, https://cloud.google.com/blog/products/business-intelligence/looker-conversational-analytics-now-ga

##### Snowflake Cortex Analyst / Snowflake Intelligence (CoWork) / managed MCP
- **Status**: Cortex Analyst generates SQL from **semantic views** (schema objects; standard-SQL querying GA 2026-03-02) with verified queries + custom instructions; model auto-select (Claude Sonnet 4.6 → 4.5 → GPT-4.1 → Arctic Text2SQL); billed per message + warehouse. Snowflake-managed MCP server is **GA**: exposes Cortex Analyst, Search, Agents, SQL, UDF tools; OAuth; max 50 tools, recursion depth 10. Snowflake Intelligence rebranded CoWork at Summit 2026 (unverified).
- **Steal**: "Verified queries" attached to the semantic object, and recursion/tool caps as governance knobs.
- **Verdict**: adjacent.
- **Sources**: https://docs.snowflake.com/en/user-guide/snowflake-cortex/cortex-analyst, https://docs.snowflake.com/en/user-guide/snowflake-cortex/cortex-agents-mcp

##### Metabase Metabot — https://www.metabase.com
- **Status**: Metabot GA Jan 2026 (v58) as Cloud add-on; **v60 (Apr 2026): AI open-sourced**, official MCP server, Slack, BYO key for self-hosted; v61: AI governance (per-group access, token limits, system prompts, usage analytics), dashboards-as-code from a terminal; v62: interactive charts in MCP, terminal CLI "built for coding agents"; v63: OpenAI/Bedrock/Azure providers. BYO providers: Anthropic, OpenAI, Bedrock, Mistral, OpenRouter, Azure.
- **How AI works**: **tools**: reuses existing questions first, then the query builder; SQL generated but **not auto-executed** in the editor; "have Metabot fix it" on errors; admins scope AI to collections. MCP server: OAuth 2.0, read tools + write tools (create/update dashboards, execute SQL); "your client provides the AI".
- **Overlap**: The most direct self-hostable, BYO-model, OSS analogue. Metabase is warehouse-attached; Brightflow owns storage.
- **Steal**: (1) Per-group token/message budgets and AI usage analytics; (2) "prefer an existing saved question over generating" as the first tool step; (3) a CLI for coding agents alongside MCP.
- **Verdict**: competitor.
- **Sources**: https://www.metabase.com/docs/latest/ai/metabot, https://www.metabase.com/docs/latest/ai/mcp, https://www.metabase.com/releases-ai

##### PostHog AI (Max) — https://posthog.com
- **Status**: In-app analyst: queries events, builds insights, HogQL, finds replays, creates dashboards/surveys/flags/alerts, edits filters; Slack beta; project memory; can start a coding task against a repo and return a PR. Core refactored to a modular graph with modes (Deep Research). Onboarding wizard builds a project autonomously; "majority of dashboards created by agents" and MCP usage doubling monthly (vendor claim). MIT core; the separate `max-ai` repo archived Jul 2025.
- **How AI works**: **tools** (typed insight/filter/HogQL actions) — the same UI primitives humans use.
- **Overlap**: Very high on the product-analytics half of Brightflow (events, funnels, retention, timelines).
- **Steal**: (1) Inline "ask Max here" affordances on every filter/insight rather than one chat panel; (2) project memory the agent maintains.
- **Verdict**: competitor.
- **Sources**: https://posthog.com/docs/posthog-ai/overview.md, https://posthog.com/docs/model-context-protocol, https://posthog.com/blog/posthogs-next-chapter

##### Amplitude Agents + MCP — https://amplitude.com
- **Status**: Feb 2026: Global Agent + four specialized agents (dashboard monitoring, session review, experiments, feedback); later: image/voice input, **agent memory** that self-corrects, cross-project history, **Agent Audit tab** for admins; MCP (Oct 2025) with session-replay/trace/vitals tools, OSS skills marketplace, charts rendered in Claude mobile.
- **How AI works**: **tools** over Amplitude's engine; "takes action directly in Amplitude" (approval gate not documented).
- **Steal**: Admin audit tab of every agent prompt/response — Brightflow's activity feed already is this; expose it as governance.
- **Verdict**: competitor (product analytics).
- **Sources**: https://amplitude.com/blog/whats-new-with-amplitude-agents, https://amplitude.com/releases/amplitude-mcp

##### Mixpanel Spark → Mixpanel Agent
- **Status**: Spark (Mar 2024, OpenAI-powered) rebranded **Mixpanel Agent** June 2026; NL → reports, anomaly detection, segments.
- **How AI works**: tools over Mixpanel's query engine.
- **Verdict**: competitor (product analytics), thin public detail.
- **Sources**: https://mixpanel.com/blog/spark-bringing-generative-ai-to-mixpanel/

##### Tellius (Kaiya) — https://www.tellius.com
- **Status**: 6.3 (Jun 12 2026) "Kaiya Everywhere": browser extension, Slack/Teams, MCP for Claude/ChatGPT/Cursor, headless. Context engine + reasoning engine that "plans, decomposes drivers, ranks what matters"; **"Missions"** = conversationally built, reproducible analyses that return the same answer every run; "deterministic math via separate engines while language models handle interpretation".
- **Overlap**: Closest enterprise articulation of Brightflow's engine+narrate split, including driver analysis.
- **Steal**: "Mission" = a saved, re-runnable investigation (Brightflow: a saved view + detector set with narration).
- **Verdict**: inspiration.
- **Sources**: https://www.tellius.com/resources/blog/resources-whats-new-tellius-6-3

##### Sisense — https://www.sisense.com
- **Status**: Jan 2026: Assistant GA, MCP server (beta; OAuth 2.1; managed + self-hosted from 2026.3.1, Aug 14 2026), BYO OpenAI now, Bedrock/GCP later, Sisense-managed LLM Spring 2026; Data Modeling Agent GA; user impersonation with audit trails.
- **Verdict**: adjacent.
- **Sources**: https://www.sisense.com/blog/assistant-mcp-server-flexible-llm/, https://www.sisense.com/blog/product-updates-2026-3-mcp-search-csdkplugins/

##### Oracle Analytics Cloud
- **Status**: Jan 2026: AI Agents (dataset-scoped assistants + documents), assistant cluster/outlier analysis, feedback on AI insights; Jul: model selection; Sep 2026: **AI descriptions** (semantic metadata so agents understand columns), suggested trending/seasonality visuals.
- **Steal**: Author-supplied "AI descriptions" per column is literally Brightflow's user layer; their suggested "seasonality/trend" visuals mirror the detectors.
- **Verdict**: adjacent.
- **Sources**: https://blogs.oracle.com/analytics/oracle-analytics-january-2026-update, https://blogs.oracle.com/analytics/explore-the-oracle-analytics-september-2026-update

##### Qlik Answers / agentic experience
- **Status**: Announced Dec 3 2025, GA Feb 2026; agents for structured analytics, documents, **Discovery Agent** (anomaly monitoring), help; MCP server at "engine, tool, and agent levels" (GA early 2026); Apr 2026 expansion (prediction, automation, analytics-dev agents). No price change.
- **How AI works**: calculations "powered by the Qlik analytics engine", LLM reasons and cites — engine-grounded.
- **Verdict**: adjacent.
- **Sources**: https://www.qlik.com/us/news/company/press-room/press-releases/qlik-debuts-agentic-experience, https://community.qlik.com/t5/Official-Support-Articles/Qlik-Answers-Agentic-Analytics-FAQ/ta-p/2542617

##### Domo.AI / Agent Catalyst
- **Status**: Agent Catalyst (Domopalooza 2025): build agents in four steps (choose LLM, instructions, knowledge, tools); AI Agent Tasks, Text Generation Tiles, automated insights; "human oversight" language. (domo.com blocked; from press coverage.)
- **Verdict**: adjacent.
- **Sources**: https://www.sdxcentral.com/news/domo-launches-agent-catalyst-for-autonomous-ai-agents/

---

#### The outward door: MCP servers and "data for coding agents"

| Vendor | Shape | Notable |
|---|---|---|
| **PostHog MCP** — https://mcp.posthog.com | Hosted, free; OSS in monorepo | Insights, HogQL, flags, errors, CDP, tickets, **semantic-layer canonical metrics**; region-routed auth; some tools bill as AI spend |
| **Amplitude MCP** | Hosted (Oct 2025) | Charts render inside Claude mobile; OSS skills marketplace |
| **Metabase MCP** (v60+) | `/api/metabase-mcp` on self-hosted or Cloud; OAuth 2.0 | Read + write tools, interactive charts (v62), plus a terminal CLI for coding agents |
| **dbt MCP** — https://github.com/dbt-labs/dbt-mcp | OSS local (`uvx dbt-mcp`) + dbt-hosted remote | Semantic Layer, Discovery, SQL exec, CLI, codegen; all plans; updated Sep 10 2026 |
| **MotherDuck/DuckDB MCP** — https://github.com/motherduckdb/mcp-server-motherduck | MIT, local | `execute_query`, catalog tools; **read-only by default**; local .duckdb/S3/Parquet |
| **Snowflake managed MCP** | GA, hosted | Cortex Analyst/Search/Agents/SQL/UDF tools; OAuth; 50-tool cap, depth 10 |
| **Google MCP Toolbox for Databases** — https://github.com/googleapis/genai-toolbox | Apache-2.0, 16.4k stars | BigQuery, AlloyDB, Spanner, Postgres, Snowflake, Neo4j…; `tools.yaml` custom tools; Looker prebuilt (unverified in fetched README) |
| **Grafana MCP** — https://github.com/grafana/mcp-grafana + Cloud MCP (Jul 28 2026) | OSS + hosted single URL | Metrics/logs/traces/alerts/incidents, user-scoped |
| Also: Wren AI MCP, Bruin MCP, Sisense MCP (OAuth 2.1), Qlik MCP, Tellius MCP, ThoughtSpot MCP, Hex CLI, Basedash MCP, Querio MCP/CLI, Microsoft Fabric Skills + Power BI Agent Skills | | |

Emerging "context layer for agents" entrants: **Corvera** (YC W26, "context layer for CPG brands via MCP", $33k MRR in 4 weeks per YC), **Velum Labs** (W26, semantic control plane for data quality), **Moda** (W26, product analytics for AI apps). No W26/S26 company found that is a general self-hosted analytics workbench.

**Sources**: https://posthog.com/docs/model-context-protocol, https://docs.getdbt.com/docs/dbt-ai/about-mcp, https://github.com/motherduckdb/mcp-server-motherduck, https://docs.snowflake.com/en/user-guide/snowflake-cortex/cortex-agents-mcp, https://github.com/googleapis/genai-toolbox, https://grafana.com/whats-new/2026-07-28-cloud-mcp-server--connect-your-agents-to-grafana-cloud-with-a-single-url/, https://www.extruct.ai/data-room/ycombinator-companies-w26/, https://www.extruct.ai/data-room/ycombinator-companies-s26/

---

#### Patterns across this slice (2025–2026)

1. **Semantic-layer grounding won; free text-to-SQL lost.** Vanna archived; Seek and Numbers Station acquired into catalog/metadata vendors; every survivor (Zenlytic, Basedash, Dot, TextQL, Wren, Snowflake, Databricks, ThoughtSpot, Tellius) sells "governed context" first and SQL generation second. Brightflow's layered per-column semantic model with provenance is *at* the frontier, not behind it — but nobody else makes provenance (detected → connector → agent → user) visible per row; that is a differentiator worth surfacing in UI and in the MCP export.
2. **"Engine computes, LLM narrates" is now the enterprise pitch, but almost nobody ships it end-to-end.** ThoughtSpot (search tokens), Tellius (separate reasoning + math engines, reproducible Missions), Qlik (engine-powered calculations) say it; Julius-style code-generation still dominates the low end and is criticised for non-reproducibility. Brightflow is ahead here: nine deterministic detectors with effect-size scoring and multiplicity control is more rigorous than any public description found. Weakness: the market packages this as *investigations with citations* (Genie Agent mode, Tellius Missions) — Brightflow should present detector output as a cited, re-runnable investigation, not a feed.
3. **Provenance UI is the 2026 table stakes.** Basedash AI Sources, Zenlytic citations, Genie "trusted asset" answers, Olympus per-cell attribution, Arcwise "fully traced". The convergent pattern: collapsible "what I used" chips + inspectable query + rows, and *mutations rendered as separate cards from answers*.
4. **Undo/approval is rare and shallow.** Only Deepnote (per-run undo), Jupyter AI 3.0 (approval prompts), Metabase (SQL not auto-run; collection scoping), Amplitude (audit tab) touch it. Brightflow's "one bus, one feed, undo over approval" is clearly ahead; no analytics vendor found exposes symmetric human/agent actions with undo. Rill claims same-tools-for-humans-and-agents (unverified, page 403) — the nearest analogue.
5. **MCP is the distribution layer; the chat UI is optional.** Wren retired its chat UI to become an MCP/SDK context layer; Jupyter AI stopped building an agent and hosts others via ACP; Metabase/PostHog/Amplitude/dbt/Snowflake/Grafana all ship hosted MCP; Microsoft ships "Skills" markdown packs. Brightflow's action bus is a natural MCP surface: every typed action becomes a tool with the same undo semantics, and the semantic model becomes a resource — this is the single highest-leverage gap.
6. **Governance knobs are being productised**: per-group token/message budgets, AI usage analytics (Metabase 61), agent audit tabs (Amplitude), tool caps and recursion limits (Snowflake), calibration question sets (Tableau). Brightflow's cost estimates and sample runs for enrichment are a start; extend to per-workspace LLM budgets and a calibration set.
7. **Self-host + BYO OpenAI-compatible model is a real but thin lane.** Metabase (BYO key), Wren (Apache, air-gapped), Definite (private cloud), Deepnote core, Bruin CLI, Onyx.app. Metabase and Definite are the realistic head-to-head competitors for "small team, no warehouse, own your data"; Brightflow's single Rust binary with Parquet-under-SQLite is simpler to run than any of them.
8. **Product analytics incumbents are now agents with typed tools** (PostHog Max, Amplitude Global Agent, Mixpanel Agent) that edit filters, create insights, and act — the same shape as Brightflow's event tracking + action bus, at far greater scale and with hosted MCP. Brightflow's edge is owning both the event data *and* the connector/business data in one semantic model; the risk is UX parity on funnels/retention/timelines with inline "ask here" affordances.

**Unverified items to note**: Julius ARR; Fabi 2.0 details (site blocked); Dot VPC/BYO model; Rill's architecture (403); Genie One GA date; Snowflake "CoWork" rename; Querio self-host pricing; camelAI pivot; Claude for Excel cell-level tracing; Google Toolbox Looker prebuilt. Web search budget was exhausted before Lightdash, Omni, Sigma, Cube D3 and Count could be covered.

---

## Pass 3: Automated insights, anomaly/change detection, driver analysis, metric monitoring — landscape as of 2026‑09‑14


Notation: (unverified) = could not confirm with a primary source. "Deterministic" = statistical detectors whose output is the ground truth; "LLM" = model-generated claim.

#### Enterprise BI with built‑in insight engines

##### ThoughtSpot SpotIQ — thoughtspot.com/product/analytics/spotiq
Augmented-analytics layer inside ThoughtSpot (proprietary, enterprise/consumption pricing; 2025–26 pushed "Spotter" agents and a Snowflake-native agentic platform).
- **Detection**: four deterministic analyses — outliers (z‑score, median z‑score, Seasonal Hybrid ESD, or linear regression; user-tunable p‑values, min rows, max insights), trend, cross‑correlation, change analysis. Change analysis has three algorithms by measure type: simply decomposable (threshold from "top ten absolute changes"), ratios (hypothetical % change holding one member constant), complex (z‑score with cardinality‑scaled thresholds N=2.0 for ≤100 members up to 5.0 for ≥100k). Time‑series anomaly: moving median for 5–30 points, Prophet for >30 with an 80/20 train/predict split; needs two full seasonal cycles. Column selection "based on what it learned from your past activity" (usage‑driven, not dismissal‑driven). Ranking method beyond p‑value/threshold not documented; no published multiplicity control.
- **Delivery**: in‑app insights, scheduled analyses, KPI monitor alerts (Slack/email/mobile).
- **Overlap**: high on detectors (outlier, trend, change/driver decomposition); Brightflow's novelty and curation loop is richer than what is documented here.
- **Steal**: cardinality‑scaled z‑thresholds for driver members (a cheap multiplicity heuristic); the "share of total changed the most" driver view in addition to "largest % change".
- **Verdict**: competitor (enterprise) / inspiration.
- Sources: [change analysis](https://docs.thoughtspot.com/cloud/26.9.0.cl/spotiq-change), [anomaly detection](https://docs.thoughtspot.com/cloud/10.13.0.cl/time-series-anomaly), [SpotIQ analysis](https://docs.thoughtspot.com/cloud/latest/spotiq-custom)

##### Tableau Pulse — tableau.com/products/tableau-pulse
Metric‑centric insight feed inside Tableau Cloud; advanced insight types gated behind Tableau+ licensing. 2025–26 releases: threshold alerts, rolling windows, mobile push, Slack off‑digest alerts, forecast/pace‑to‑goal, Teams digests (Aug 2026), GPT‑5.2‑backed Tableau Agent (Jul 2026).
- **Detection**: explicitly "standardized, deterministic statistical models" that produce facts; LLM only narrates — the same architecture as Brightflow. Fourteen insight types: period‑over‑period, correlated metrics, record‑level outliers, forecast, current trend, trend change alert, unexpected values, goal/threshold, pace to goal, top drivers/detractors, concentrated contribution (≥50% from few members), top/bottom contributors. Each fact is "scored based on the impact it has on the metric value"; thumbs up/down "learns to personalise the types of insights". Seasonality‑aware expected ranges added 2025. No public multiplicity control or history‑novelty logic.
- **Delivery**: personalised digests (email/Slack/Teams), off‑cycle alerts, mobile push, in‑app metric pages.
- **Overlap**: near 1:1 in philosophy; Pulse is per‑followed‑metric rather than per‑table sweep.
- **Steal**: "concentrated contribution" as a named alert with a hard 50% rule; the drivers/detractors split by polarity of the metric move; digest cadence with off‑cycle escalation for unfavourable moves only.
- **Verdict**: both (closest philosophical match; enterprise‑priced).
- Sources: [insight types](https://help.tableau.com/current/online/en-us/pulse_insights_platform_insight_types.htm), [release notes](https://help.tableau.com/current/online/en-us/pulse_intro.htm)

##### Sisu Data → Snowflake — docs.snowflake.com (Contribution Explorer)
Sisu (decision‑tree driver analysis over warehouse data) was acquired by Snowflake on 2023‑10‑16 and the standalone product wound down. What became of it: Snowflake's ML functions `TOP_INSIGHTS` (key‑driver analysis via decision tree splitting control vs test rows, outputs CONTRIBUTION, RELATIVE_CONTRIBUTION, GROWTH_RATE, ranked by absolute contribution, "filtering out redundant segments") and `ANOMALY_DETECTION` (GBM with ARIMA‑style differencing, lags, calendar features; 0.99 prediction interval; optional supervised labels). Direct Sisu lineage is not stated in the docs (unverified, but widely assumed).
- **Overlap**: Brightflow's drivers/segment contribution detector is the same problem; the decision‑tree approach finds *combinations* of dimensions and prunes redundant ones.
- **Steal**: report both absolute and relative contribution plus within‑segment growth rate; explicit redundancy filter so "country=SE" and "city=Stockholm" don't both appear.
- **Verdict**: inspiration (algorithm), adjacent (Snowflake‑only).
- Sources: [acquisition](https://www.crunchbase.com/acquisition/snowflake-computing-acquires-sisu-data--ea3bc847), [TOP_INSIGHTS](https://docs.snowflake.com/en/user-guide/ml-functions/contribution-explorer), [ANOMALY_DETECTION](https://docs.snowflake.com/en/user-guide/ml-functions/anomaly-detection)

##### Oracle Analytics "Explain" — docs.oracle.com
One‑click column explainer: basic facts, key drivers (columns most associated with the target), segments (hidden groupings), anomalies (intersections behaving unexpectedly; a different algorithm from the outlier visual). Deterministic ML, ranked by "significance"/unusualness; no feedback loop, no monitoring cadence — it is on‑demand, not always‑on.
- **Steal**: the four‑pane framing (facts → drivers → segments → anomalies) is a clean UX for a table‑level insight page.
- **Verdict**: inspiration.
- Source: [What is Explain](https://docs.oracle.com/en/middleware/bi/analytics-server/user-oas/what-is-explain.html)

##### Power BI — Anomaly detection, Key Influencers, Copilot
- **Anomaly detection** (line charts): Azure Anomaly Detector SR‑CNN (spectral residual + CNN), sensitivity slider, min 4 points. Explanations rank fields by *strength* = ratio of the component series' (actual − expected) to the aggregate's (actual − expected) at the anomaly point — i.e. share of the deviation explained, a nice deterministic attribution rule. **Key Influencers**: regression/decision‑tree based (learn.microsoft.com blocked for me; unverified detail). **Copilot**: LLM summaries of visuals; Fabric‑capacity priced.
- **Steal**: the strength ratio for attributing an anomaly to dimension members — trivially computable from Polars group‑bys and directly comparable across dimensions.
- **Verdict**: inspiration.
- Source: [anomaly detection tutorial](https://learn.microsoft.com/en-us/power-bi/visuals/power-bi-visualization-anomaly-detection)

##### Yellowfin Signals — yellowfinbi.com/suite/signals
Always‑on automated analysis in Yellowfin (Idera‑owned; enterprise licence). Deterministic detectors: spikes/drops, breakouts, step change (stable‑state to stable‑state with ramp handling), trend reversal, volatility change, period comparisons, new/lost attributes (membership change!). Ranking is a "complex ranking algorithm" then per‑user personalisation from watch/rate/ignore behaviour; "not useful" ratings feed back. Correlated signals + assisted insights for root cause; NL explanation. Delivered to an in‑app Timeline; alerts.
- **Overlap**: the closest detector set to Brightflow's nine (including membership change and step change).
- **Steal**: separating "spike/drop" (transient) from "step change" (regime) as different insight kinds with different half‑lives; "new and lost attributes" as first‑class.
- **Verdict**: both.
- Sources: [Signals wiki](https://wiki.yellowfinbi.com/display/yfcurrent/Yellowfin+Signals), [step change](https://www.yellowfinbi.com/blog/yellowfin-signals-what-is-step-change)

##### Domo Alerts / Automated Insights — domo.com
Threshold and AI alerts, "Mr. Roboto" automated insights, anomaly‑classification agents with feedback, closed‑loop actions (ServiceNow tickets, pause campaigns, Slack). Detector details are not public (unverified beyond marketing); help centre blocked for me.
- **Steal**: alert → action routing (Brightflow's action bus is the natural home for "when insight X, run action Y").
- **Verdict**: adjacent competitor.
- Source: [Domo alerts](https://www.domo.com/business-intelligence/features/alerts)

##### Sigma — sigmacomputing.com
Conditional exports/alerts (static thresholds, formula bounds such as mean ± 1.5σ, "stop notifying after" caps) to email/Slack/Teams/webhooks. Sigma Agents (launched 2026‑04‑08) run scheduled LLM agents on warehouse data; anomaly detection is agent‑driven, method unspecified. Warehouse‑priced.
- **Verdict**: adjacent.
- Sources: [conditional alerts](https://help.sigmacomputing.com/docs/schedule-a-conditional-export-or-alert), [Sigma Agents](https://www.sigmacomputing.com/blog/introducing-sigma-agents)

#### Product analytics and dashboards

##### Google Analytics 4 Insights — support.google.com/analytics/answer/9517187
Free. Bayesian state‑space model predicts the latest point; outside the credible interval = anomaly. Training windows 2 weeks (hourly), 90 days (daily), 32 weeks (weekly). Segment anomalies: metrics normalised per user, PCA across metrics × dimension values, weekly cadence, surfaced only if the segment is ≥0.05% of users. Custom insights = user thresholds. In‑app cards, email for custom insights. No dismissal feedback into ranking.
- **Steal**: minimum‑support floor for segment insights; explicit different lookbacks per grain.
- **Verdict**: inspiration.

##### Amplitude — Anomaly + Forecast, Monitors, Agents
Anomaly + Forecast is Prophet: *agile* (95% CI, 120‑day train, daily/weekly seasonality) vs *robust* (365‑day, monthly/yearly) vs custom. Monitors/digest alerts email and Slack. June 2026: AI Agents beta — a Dashboard (Monitoring) Agent that analyses every chart on a dashboard, does week‑over‑week comparison against its own prior run to separate new vs persistent findings, prioritises insights and posts to Slack/email. Detection vs narration split not documented (LLM‑driven, unverified).
- **Steal**: the agent's "compare with previous run → new / changed / consistent" is exactly a novelty‑against‑history mechanic, exposed in the write‑up itself.
- **Verdict**: competitor (product‑analytics niche) / inspiration.
- Sources: [Anomaly + Forecast](https://amplitude.com/docs/analytics/anomaly-forecast), [Dashboard Agent](https://amplitude.com/docs/agents/dashboard-agent), [agents launch](https://itbrief.co.uk/story/amplitude-unveils-autonomous-ai-agents-for-analytics)

##### Mixpanel Alerts — docs.mixpanel.com/docs/features/alerts
2026‑04‑09 upgrade: anomaly alerts moved from Prophet to Google TimesFM V2 (claimed 85% lower MAPE across 3,000 real series); notification windows to suppress expected dips; **retroactive test** ("how many times would this have fired") before enabling; trigger history hub; email/Slack/webhook; root‑cause analysis on triggered alerts (breakdown by property).
- **Steal**: backtest‑before‑enable for any user‑configured watch; a per‑alert trigger history to distinguish recurring from one‑off.
- **Verdict**: inspiration.
- Source: [alerts upgrade](https://mixpanel.com/blog/mixpanel-alerts-upgraded-more-control-more-visibility-more-flexibility/)

##### PostHog — alerts + Self‑driving
OSS core + cloud. Alerts on trends: value, delta, or anomaly detection with 13 detectors (z‑score, MAD, IQR, threshold, Isolation Forest, KNN, LOF, ECOD, COPOD, HBOS, PCA, OCSVM) plus AND/OR ensembles; seasonality via differencing; sensitivity 0–1; simulate on history before deploy; in‑app/email/Slack/Discord/Teams/webhook. Self‑driving (2026‑06‑26): Scouts (scheduled agents) → Signals (finding + evidence + suggested action) → Reports → Inbox ranked by priority; anomaly alerts spawn LLM investigations (Jul/Aug 2026); $15 per generated PR, reports free.
- **Steal**: the Signal schema (what / evidence / suggested action) as the finding record; ensemble‑with‑voting as a false‑positive filter; simulate‑on‑history.
- **Verdict**: both (closest OSS neighbour; not self‑hosted‑single‑binary).
- Sources: [alerts](https://posthog.com/docs/alerts.md), [self‑driving](https://posthog.com/docs/self-driving), [changelog](https://posthog.com/changelog)

##### Metabase — metabase.com/docs/latest/questions/alerts
OSS. Alerts only: "returns results", goal‑line crossing, progress bar. No anomaly detection, no automated insights (the old "X‑rays" are on‑demand). Email/Slack.
- **Verdict**: adjacent; Brightflow is well ahead here.

##### Looker / Looker Studio — cloud.google.com/looker
Gemini assistants (insight, expression, visualization) and Conversational Analytics; forecasting/anomaly via code interpreter on demand. No always‑on insight feed with documented detectors (unverified whether Next 2026 changed this; Rittman page blocked).
- **Verdict**: adjacent.
- Source: [Gemini in Looker](https://docs.cloud.google.com/looker/docs/gemini-insight-asst)

##### Klipfolio PowerMetrics — powermetrics.app
Metric catalog SaaS; "Smart Metrics" flag statistically significant changes, learn normal ranges; anomaly detection and forecasting sit in a paid Advanced Analyses add‑on (unverified detail). Statistical thresholds, no seasonality modelling per third‑party reviews (unverified).
- **Verdict**: adjacent competitor (small‑team KPI niche).

##### Databox — help.databox.com/databoxs-anomaly-detection-methodology
SMB dashboards. Decomposes into trend/seasonality/events, forecasts an expected range (80%/95%), flags outside; anomaly score 0–100; sensitivity slider 0–99 where default 10 ≈ 1% of points flagged; "adaptability" controls how fast the baseline follows the data. Smart Alerts daily/weekly/monthly to email/Slack.
- **Steal**: expressing sensitivity as a target flag rate ("~1% of points") — far more legible than σ or CI.
- **Verdict**: competitor (SMB) / inspiration.

##### Geckoboard — geckoboard.com
TV dashboards; threshold status indicators and Slack/Teams alerts only. No detection.
- **Verdict**: adjacent.

#### Business‑monitoring specialists and decision‑intelligence startups

##### Anodot — anodot.com
Autonomous business monitoring over millions of metrics; per‑metric model selection, correlation grouping of related anomalies, feedback on anomalies. Acquired by Glassbox (announced 2025‑11‑10, terms undisclosed); pricing quotes reportedly $20–60k/yr (unverified). Site blocked for me; algorithm claims (unverified).
- **Steal**: grouping simultaneous anomalies across metrics into one incident.
- **Verdict**: competitor (enterprise) / inspiration.
- Source: [Glassbox acquires Anodot](https://finovate.com/glassbox-acquires-business-monitoring-and-analytics-specialist-anodot/)

##### Avora — avora.com
London augmented analytics (anomaly + root cause + alerts). Still listed, ~7 employees per Tracxn (unverified); site blocked. Effectively dormant.
- **Verdict**: adjacent / cautionary.

##### Kausa — kausa.ai
Berlin decision intelligence ("tests millions of hypotheses"). PitchBook lists it as acquired 2023‑09‑01; acquirer not found; kausa.ai returns 404. Dead as a standalone.
- **Verdict**: cautionary.
- Source: [PitchBook](https://pitchbook.com/profiles/company/459404-65)

##### Falkon — falkon.ai
Seattle "augmented analytics for revenue" ($16M Series A 2022). Site now reads "Falkon's Final Flight" — shut down.
- **Verdict**: cautionary.

##### Outlier.ai
Original Sean Byrnes "daily stories" automated‑analysis product; the outlier.ai domain is now Scale AI's labelling marketplace. Review sites still list the analytics product (last updated Sep 2026) but no primary source shows it being sold (unverified status; treat as gone).
- **Steal (historical)**: the "daily stories" format — a handful of narrated findings per day rather than a feed.
- **Verdict**: cautionary / inspiration.

##### Rupert — hirupert.co
Signal‑triggered playbooks and alerting on Snowflake/BigQuery; LinkedIn now says "Acquired by Sylvan Labs" and the site sells a CS AI tool (unverified pivot detail).
- **Verdict**: cautionary.

##### HyperArc — hyperarc.com
AI‑native BI with a "HyperGraph" memory of analyses. Acquired by Zendesk (2025). Not an insight engine per se.
- **Verdict**: adjacent.

##### Narrative BI — narrative.bi
LLM‑narrated automated insights to Slack/email for marketing data. Team joined Cube 2025‑09‑09; product pages now redirect to the Cube announcement.
- **Verdict**: cautionary (LLM‑first insights did not survive standalone).

##### Unsupervised — unsupervised.com
Originally unsupervised pattern discovery ranked by KPI impact; now sells agent‑control tooling (DeepWork, Panopticon) plus "Finder for Teams" with "KPI‑linked pattern ranking".
- **Steal**: rank patterns by impact on a declared KPI — Brightflow already does this via KPI flags.
- **Verdict**: adjacent.

##### Push.ai — push.ai
Now a "context layer" (Business Graph of metrics/ownership) with scheduled "Automated Reviews" delivered in‑flow; LLM reasoning over governed context. No published detectors.
- **Verdict**: adjacent.

##### Lumi AI — lumi-ai.com
Chat agent over ERP data, $3.7M seed; proactive insights on the roadmap. Skip.

##### Fiddler — skipped (ML model monitoring, not business metrics).

#### Adjacent: infra and data observability

##### Datadog anomaly monitors / Watchdog — docs.datadoghq.com
Three algorithms: basic (rolling quantile), agile (robust SARIMA), robust (seasonal‑trend decomposition); seasonality needs 3× period of history; band width in deviations; alert only when X% of the window is anomalous (a debouncing rule); outlier detection via DBSCAN; Watchdog runs with no config, adds root‑cause and impact analysis, and takes "not useful" feedback. Metaplane (data observability) acquired 2025‑04‑23.
- **Steal**: "% of window anomalous" trigger/recovery hysteresis; the agile/robust naming to expose a single meaningful choice to users.
- **Verdict**: adjacent pattern source.
- Sources: [anomaly monitor](https://docs.datadoghq.com/monitors/types/anomaly/), [Watchdog](https://docs.datadoghq.com/watchdog/), [Metaplane](https://www.datadoghq.com/blog/datadog-acquires-metaplane/)

##### Monte Carlo / Bigeye / Metaplane — data observability
Same detector family aimed at freshness, volume, nulls, distributions. Monte Carlo metric monitors: ML thresholds with backfill, bucketed by grain, "optimised for the extremes" (hypersensitive near 0–1% null rates), timezone‑aware buckets, up to 10k field/segment combos, mark‑as‑expected feedback, Slack/email. Bigeye deploys autometrics with autothresholds from history. Overlap with Brightflow is real: distribution shift and membership change are data‑quality detectors too.
- **Steal**: run the same detectors on Brightflow's own connector tables as a data‑quality lane (row counts per sync, null rates) and label those findings differently from business findings.
- **Verdict**: adjacent.
- Sources: [MC metric monitors](https://docs.getmontecarlo.com/docs/metric-monitors), [Bigeye autometrics](https://docs.bigeye.com/docs/autometrics)

##### Chronosphere — skipped beyond noting it markets AI RCA (unverified).

#### Open‑source detection libraries and how products package them

- **Prophet** (Meta) — maintenance mode since v1.4.0 (2026‑08‑01), no new features. Still the backbone of Amplitude and ThoughtSpot's >30‑point path; Mixpanel left it for TimesFM. [repo](https://github.com/facebook/prophet)
- **Kats** (Meta) — CUSUM, BOCPD, outlier, ProphetTrend, StatSig with seasonality removal, meta‑learning model selection; community‑maintained, 0.2.0. [repo](https://github.com/facebookresearch/Kats)
- **Merlion** (Salesforce) — archived 2026‑03‑11. Its calibrated anomaly scores and post‑processing rules remain a good design reference. [repo](https://github.com/salesforce/Merlion)
- **ruptures** — offline change point (PELT, Binseg, Window, Dynp, KernelCPD) with penalty selection; active, Python 3.10–3.14. [repo](https://github.com/deepcharles/ruptures)
- **Nixtla statsforecast** — AutoARIMA/ETS/CES/Theta, in‑sample prediction‑interval anomaly detection; Apache‑2.0, active. TimeGPT (hosted) has anomaly endpoints (docs page 404'd for me; unverified). [repo](https://github.com/Nixtla/statsforecast)
- **Darts** — scorer → detector → aggregator anomaly framework, PyOD wrappers, conformal intervals, foundation models (Chronos‑2, TimesFM, TiRex); active. [repo](https://github.com/unit8co/darts)
- **Greykite** (LinkedIn) — Silverkite with adaptive‑lasso changepoints; Greykite AD auto‑tunes intervals to an *expected alert rate* or labelled anomalies; low commit count. [repo](https://github.com/linkedin/greykite)
- **Foundation models** — TimesFM 3.0 (Aug 2026, weights now non‑commercial licence; 2.5 remains Apache), Chronos‑2 (Oct 2025, Apache‑2.0, 28M/120M). Products use them as forecasters whose intervals define anomalies, not as classifiers. [TimesFM](https://github.com/google-research/timesfm), [Chronos](https://github.com/amazon-science/chronos-forecasting)

For a Rust single binary the relevant lesson is not the libraries themselves but the two packaging idioms: (a) forecast‑interval anomaly (Prophet/GBM/foundation model → band → outside = anomaly), and (b) Greykite/Databox‑style calibration to a target alert rate rather than a raw σ.

#### 2025–2026 "always‑on insights" startups
Web search budget ran out before this sweep; from what was gathered: PostHog Self‑driving (Jun 2026), Amplitude Agents (Jun 2026), Sigma Agents (Apr 2026), and Slack‑native monitors such as Runbear and eesel AI ("weekly digests and trend alerts") are the visible 2026 entrants; the independent decision‑intelligence cohort (Kausa, Falkon, Rupert, Narrative BI, Outlier, Sisu, Anodot, HyperArc) has almost entirely been acquired or shut down. (unverified: any 2026 seed‑stage entrants I did not reach.)

#### Patterns across this slice

1. **Detectors are converging on the same list.** Period‑over‑period, forecast‑band anomaly, trend/trend change, step change, contribution/drivers, concentration, membership change, record outliers. Brightflow's nine already cover the union; nobody documents change‑point *and* distribution‑shift *and* membership change together except Yellowfin.
2. **Deterministic facts + LLM narration is now the stated best practice** (Tableau Pulse says it verbatim; PostHog's Signal schema encodes it). LLM‑first insight products (Narrative BI, Outlier) did not survive standalone. Brightflow is aligned, and ahead in making it a hard rule.
3. **Ranking is by business impact, not p‑value.** Pulse scores facts by impact on the metric; Snowflake orders by absolute contribution; Power BI attributes by share of deviation explained; Unsupervised ranks by KPI linkage. Brightflow's effect size × KPI weight × polarity matches; the industry adds *personalisation by user behaviour* (Yellowfin, Pulse) that Brightflow lacks.
4. **Multiplicity control is essentially absent in product docs.** Cardinality‑scaled thresholds (ThoughtSpot) and minimum‑support floors (GA4 0.05%) are the only visible controls; nobody names Bonferroni/FDR. Brightflow is ahead — and could make it a marketing point.
5. **False‑positive handling is calibration UX, not statistics**: target alert rate (Databox ~1%, Greykite), backtest/simulate before enabling (Mixpanel, PostHog), % of window hysteresis (Datadog), notification windows (Mixpanel), ensemble voting (PostHog). Brightflow has none of these user‑facing knobs; a "simulate on history" preview would be the cheapest win.
6. **Feedback loops are shallow**: thumbs up/down that "personalises insight types" (Pulse, Yellowfin, Watchdog "not useful"). Brightflow's dismiss/pin/annotate/suppress feeding back into ranking is more explicit than anything documented here.
7. **Novelty against history is handled by comparing runs** (Amplitude's Dashboard Agent week‑over‑week, Mixpanel trigger history, PostHog Reports bundling signals by root cause). Brightflow already tracks novelty; grouping co‑occurring findings into one incident (Anodot, PostHog Reports) is the missing step.
8. **Delivery is where Brightflow is furthest behind.** Every surviving product ships Slack and email, most ship digests with off‑cycle escalation for unfavourable moves, several route to actions. Brightflow's action bus makes "finding → action" natural, but findings still never leave the app.

---

## Pass 4: Customer feedback analytics, ticket triage and VoC — competitor/inspiration scan for Brightflow (as of 2026-09-14)


Scope: the "text enrichment" slice — ticket classification against a curated vocabulary, mention extraction, vocabulary health, agent-proposed vocabularies with human review, multilingual, cost transparency, BYO model, and the join to usage/revenue numbers. Note: the web-search budget for this session ran out partway through; the tail products were covered by fetching primary pages directly, and several vendor domains (Zendesk support, Qualtrics, Medallia, Sprig, Featurebase, Cycle, Kapiche, Front, Pylon, Productboard academy) were blocked from fetch, so those entries lean on search snippets and are marked accordingly.

---

#### Tier 1 — dedicated feedback-intelligence platforms (closest overlap)

##### Enterpret — https://www.enterpret.com
Feedback-intelligence platform built around an **Adaptive Taxonomy** and a **Customer Context Graph**.
- **Status:** Proprietary; pricing not published (the /pricing URL 404s). $4.3M seed (Kleiner Perkins) + $20.8M Series A led by Canaan (Dec 2024), $25.1M total. 2025–26: MCP-compatible workflows; positions itself as a data layer for Claude/ChatGPT.
- **Taxonomy:** adaptive/self-evolving — categories evolve with incoming feedback "so answers don't change every time." Human review, versioning and multilingual details are not on the public site (unverified). A third-party analysis credits it as "the most credible answer anyone has shipped to the feedback classification problem" but notes G2 users' main complaint is **setup investment and taxonomy-maintenance overhead**, and the structural limit that an adaptive taxonomy "cannot invent a category no customer mentioned."
- **Joins:** the Context Graph attaches segments, LTV, lifecycle stage, usage and product area to every signal and links issues to churn/expansion/adoption. Integrations: Slack, Jira, Linear, Salesforce, support/survey/review/call sources.
- **Alerts:** "tickets, alerts and workflows" (Slack presumed; unverified).
- **Overlap:** high — the closest philosophical match to Brightflow's semantic-layer-plus-classification. Enterpret does the join via CRM/usage *imports*; Brightflow does it natively in one engine.
- **Steal:** (1) attach account-level numeric context (LTV/lifecycle/usage) to each classified record as first-class columns; (2) "reinforce existing understanding instead of rebuilding" — treat vocabulary edits as incremental, never full re-induction.
- **Verdict:** competitor.
- Sources: [enterpret.com](https://www.enterpret.com/), [Koji: Enterpret alternatives 2026](https://www.koji.so/blog/enterpret-alternatives-2026), [Tracxn](https://tracxn.com/d/companies/enterpret/__NYa-u_JPq9-Kp2Bn-rLsy3v5-cQvjzzsw0QlpwhRzjQ), [Wikipedia](https://en.wikipedia.org/wiki/Enterpret)

##### Thematic — https://getthematic.com
Bottom-up theme discovery for CX/VoC with a human-in-the-loop theme editor.
- **Status:** Proprietary; pricing not published. Verified changelog: Themes V2 (Feb 2025); Theme Quality Tracker with colour-coded coverage/specificity/accuracy scores (Jul 2025); AI theme-discovery suggestions with user accept/reject (Jul 2025); **Automated Theme Discovery Workflow** — monthly email/Slack digest of emerging themes for review (Oct 2025); AI Theme Suggestions *agent* that proactively proposes improvements/restructures, and Agentic Answers with cited multi-turn Q&A (May 2026); **theme portability** — export a `.themes` file and upload to another portal for review before applying (Jul 2026); MCP server (Jul 2026).
- **Taxonomy:** adaptive, bottom-up, two-level (themes/sub-themes). Humans merge/split/rename and see the supporting phrases. Quality tracker = explicit vocabulary-health metrics. Versioning via file export.
- **Joins:** Snowflake reader; "one consistent analytical engine" across surveys/reviews/chat/calls; no native usage/revenue join.
- **Alerts:** monthly theme-discovery digest via email/Slack; "rare themes on the rise."
- **Overlap:** very high on vocabulary health + agent-proposed themes + human review.
- **Steal:** (1) the Quality Tracker's three axes — coverage, specificity, accuracy — as Brightflow's vocabulary-health scorecard; (2) a scheduled "new themes for review" digest that is a queue, not a notification; (3) theme files as a portable, reviewable artifact (maps well onto Brightflow's versioned vocabularies).
- **Verdict:** both.
- Sources: [changelog](https://getthematic.com/changelog), [tag-and-theme](https://getthematic.com/product/tag-and-theme/), [help: AI suggestions](https://help.getthematic.com/article/214-themes-editor-ai-suggestions)

##### Unwrap.ai — https://www.unwrap.ai
"Zero-shot" auto-tagging of feedback into groups without prior taxonomy input.
- **Status:** Proprietary; $12M Series A led by Scale VP with Atlassian Ventures (Jan 2025). Pricing: **from $24,000/year**, priced on monthly feedback volume + integrations, explicitly never per seat.
- **Taxonomy:** freeform-adaptive ("Auto Tagger"; surfaces trends "you didn't anticipate"). Custom taxonomy and human-editing details not public (unverified). Multilingual support and PII removal listed as features.
- **Joins:** "3K+ tools" via integrations; no product-usage/revenue join described.
- **Alerts:** real-time alerts "when new insights emerge"; Assistant (NL query) and Responder (auto-reply to customers).
- **Overlap:** medium — classification, but no curated vocabulary and no numbers.
- **Steal:** volume-based, seat-free pricing story is worth echoing in Brightflow's cost-estimate UI (cost per 1k tickets, not per user).
- **Verdict:** competitor.
- Sources: [unwrap.ai](https://www.unwrap.ai/), [pricing](https://www.unwrap.ai/pricing), [BusinessWire](https://www.businesswire.com/news/home/20250130865543/en/AI-powered-Analytics-Platform-Unwrap-Secures-Series-A-to-Redefine-Customer-Intelligence)

##### Kraftful → Amplitude AI Feedback — https://www.kraftful.com
- **Status:** **Acquired by Amplitude**; "Amplitude AI Feedback" launched Nov 2025 and is **included on all Amplitude plans including free** (acquisition date/terms unverified beyond the Amplitude blog). Sources: surveys, tickets, calls, app reviews, Reddit, Discord, X, G2.
- **Taxonomy:** freeform LLM themes/topics + sentiment + urgency; no curated vocabulary or review flow described.
- **Joins:** this is the one that does what Brightflow does: correlate "checkout is broken" with a conversion-rate drop, drill to the affected segment, jump to session replay; counts recurrence across channels ("47 times this week across six channels").
- **Alerts:** not described.
- **Overlap:** the strongest direct threat to Brightflow's differentiator, from a large incumbent giving it away free.
- **Steal:** (1) "complaint → metric → segment → evidence" as a single click-path; (2) recurrence counting across sources as a headline number.
- **Verdict:** competitor.
- Sources: [amplitude.com/blog/ai-feedback](https://amplitude.com/blog/ai-feedback), [Canny roundup](https://canny.io/blog/best-ai-feedback-tools/)

##### Chattermill — https://chattermill.com
Enterprise CX intelligence with proprietary "Lyra AI."
- **Status:** Proprietary; $26M Series B (Dec 2022), $34.8M total; no acquisition. 2025–26: Lyra AI model, MCP access for Claude/ChatGPT, "Lyra Agent" coming soon; anomaly detection with alerts to Slack/email/Jira; "XLG" framework tying feedback to revenue/retention/churn.
- **Taxonomy:** AI-tagged themes; editing UX not documented. Multilingual unverified.
- **Joins:** 50+ integrations (Zendesk, Intercom, Qualtrics, Trustpilot); "impact analysis" links themes to CX metrics, not to product usage.
- **Verdict:** competitor (enterprise tier).
- Sources: [chattermill.com](https://chattermill.com/), [Series B](https://chattermill.com/blog/chattermill-secures-26m-series-b)

##### Idiomatic → Front — https://front.com
- **Status:** Idiomatic **acquired by Front, Nov 2024**; founder Kevin Yang now runs Front's AI. Idiomatic's approach was supervised ML with hand-labelled, granular custom categorisations. Front now ships **AI Topics** (auto-classification) on its Enterprise plan alongside Smart QA/Smart CSAT; inferred CSAT and product-feedback extraction from every conversation. Front's Autopilot agent is "English-only officially."
- **Taxonomy:** curated-and-trained (Idiomatic heritage) → auto-topics inside Front; editing not documented.
- **Joins:** none beyond inbox analytics.
- **Verdict:** adjacent (helpdesk-embedded).
- Sources: [BusinessWire](https://www.businesswire.com/news/home/20241119246220/en/Front-Acquires-AI-powered-Voice-of-Customer-Platform-Idiomatic-to-Unlock-Insights-in-Customer-Conversations), [eesel on Front AI](https://www.eesel.ai/blog/best-ai-for-front)

##### Caplena — https://caplena.com
Swiss survey-coding tool, the most "codebook" of the bunch.
- **Status:** Proprietary; free trial, volume pricing; founded 2017. 
- **Taxonomy:** curated **topic collections** that the LLM helps make MECE; humans customise/add; **assignment-quality metrics** are a stated differentiator; "interactively guide" the model with business context. **100+ languages natively**, plus DeepL/Google Translate fallbacks. Driver analysis correlates topics with quantitative variables (NPS etc.), segment comparison, significant-change detection.
- **Verdict:** inspiration.
- **Steal:** (1) MECE check on the vocabulary (overlapping/ambiguous categories flagged); (2) per-assignment quality score exposed in the review UI; (3) driver analysis = topic-vs-numeric correlation, which Brightflow can do over Polars trivially.
- Sources: [features](https://caplena.com/en/features), [driver analysis docs](https://docs.caplena.com/driver-analysis)

##### Kapiche — https://www.kapiche.com
- **Status:** Proprietary; Bronze tier reportedly **$1,060/mo** (third-party, unverified); "no setup, no code frames"; Australian mid-market/enterprise. Positions on analysing 100% of conversations incl. calls.
- **Taxonomy:** bottom-up themes without pre-built taxonomies.
- **Verdict:** competitor (mid-market).
- Sources: [kapiche.com](https://www.kapiche.com/), [Koji comparison](https://www.koji.so/blog/enterpret-alternatives-2026)

##### Birdie — https://birdie.ai
- **Status:** Proprietary; claims 50M+ interactions/month; customers Nubank, Upwork, Betterment. Multilingual.
- **Joins:** explicitly "correlates insights with business metrics" and behavioural signals; root-cause detection.
- **Verdict:** competitor.
- Sources: [birdie.ai](https://birdie.ai/), [feedback-analytics](https://birdie.ai/feedback-analytics)

##### Syncly — https://syncly.app
- **Status:** YC; $3.3M seed (SoftBank Ventures, Nov 2023). Site now emphasises **social/video listening** (TikTok/Reels/Shorts, STT + vision) alongside ticket/chat/survey feedback; auto-tag + cluster with per-message sentiment; real-time alerts; MCP connector.
- **Verdict:** competitor (drifting toward social intelligence).
- Sources: [syncly.app](https://syncly.app/), [seed](https://syncly.app/blog/syncly-secures-3m-in-seed-funding)

##### Anecdote — https://www.anecdoteai.com (unverified URL; the .com domain I fetched is an unrelated retailer)
- **Status:** Proprietary; G2-reviewed in 2026; sources Intercom/Zendesk/app stores; semantic search + summaries; bug detection and Slack alerts per third-party listings (unverified).
- **Verdict:** competitor (small).
- Sources: [G2](https://www.g2.com/products/anecdote/reviews), [Usermaven roundup](https://usermaven.com/blog/customer-feedback-analysis-tools)

##### Relative Insight — https://relativeinsight.com
Comparative text analysis (A vs B corpora).
- **Status:** Proprietary; products Metric Radar, Explore, Heartbeat, Scoreboard, Accelerator AI; new **Zayker** CX AI assistant; "AI Integrity" — answers grounded only in own data, traceable to source; markets a mix of agentic, deterministic and generative approaches.
- **Verdict:** inspiration.
- **Steal:** comparison-first analytics (this segment vs that, this month vs last) as the primary lens rather than absolute counts — fits Brightflow's Text Explorer + TF-IDF directly.
- Sources: [relativeinsight.com](https://relativeinsight.com/)

##### Wonderflow — https://www.wonderflow.ai
- **Status:** Independent, ~79 staff, $25M raised; B2C review/VoC across 1,000+ sources. No acquisition found.
- **Verdict:** adjacent (consumer products).
- Sources: [Tracxn](https://tracxn.com/d/companies/wonderflow/__VmA5QWufPycwDEQZg002JvkGJ-CQFxPU8sXc46qrUE0)

##### Clarabridge → Qualtrics XM Discover — https://www.qualtrics.com
- **Status:** Clarabridge is XM Discover. Distinct from survey-level Text iQ. **Topic Hierarchy Generator** builds a topic model from your data + use case + persona + industry context; sentence-level topic/sentiment/effort/emotion/actionability fields. "AI Assisted Topics" for Engage on the 2026 roadmap (community post).
- **Taxonomy:** curated hierarchical topic models (rule + ML), AI-bootstrapped, human-edited; enterprise-grade versioning (unverified details, domain blocked).
- **Verdict:** adjacent (enterprise suite).
- Sources: [XM Discover AI-assisted](https://www.qualtrics.com/support/xm-discover/text-analytics/ai-assisted-text-analytics-in-xm-discover/), [community](https://community.qualtrics.com/artificial-intelligence-ai-149/a-new-way-to-build-topic-models-with-ai-32483)

##### Medallia (incl. MonkeyLearn) — https://www.medallia.com
- **Status:** MonkeyLearn acquired May 2022 and folded into Theme AI. **Athena** = AI layer (now "Medallia AI"): themes/topics, sentiment/emotion/effort/intent scoring, alerting, root cause; **Athena Studio** no/low-code custom classifier training (2022); **Insights Assistant** conversational analytics announced Feb 2026 (third-party). Not sold standalone. "Dozens of languages."
- **Taxonomy:** prebuilt industry models + custom trained classifiers.
- **Verdict:** adjacent.
- Sources: [Medallia AI](https://esnew.medallia.com/platform/athena), [Perspective AI on Athena](https://getperspective.ai/blog/medallia-athena-alternatives-2026)

---

#### Tier 2 — product-feedback / roadmap tools with AI classification

##### Canny Autopilot — https://canny.io/features/autopilot
- **Status:** Proprietary. Feedback Discovery from Intercom/Zendesk (count against limits), review sites, Help Scout, Gong; dedupe; smart replies; spam. **Manual vs Automated mode** — in manual the Autopilot inbox is a review queue, in automated it "becomes an audit log." Credits: $0.10 each; text = 1 credit, audio call = 5; spam detection free. Autopilot never auto-marks spam. Third-party claim: AI features on every plan incl. free (unverified vs. help-centre's "legacy Starter/Growth credit allotments").
- **Taxonomy:** boards/categories (curated, product-area shaped) — Autopilot routes into them.
- **Verdict:** inspiration.
- **Steal:** the manual/automated toggle where the same inbox is either a review queue or an audit log — maps exactly onto Brightflow's action bus with undo.
- Sources: [help: Autopilot](https://help.canny.io/en/articles/8202451-autopilot), [review sources](https://canny.io/blog/autopilot-review-sources/)

##### Productboard Spark / AI — https://www.productboard.com/product/spark/
- **Status:** Spark announced as GA on the blog dated **June 22, 2026** (a search snippet says Jan 27, 2026 — conflicting; likely preview vs GA). AI Suggested Insights flags feedback as it lands and proposes feature links; VoC reports with every insight traceable to a source note; "evidence-ranked opportunities." Pricing tier not stated.
- **Taxonomy:** the feature tree *is* the taxonomy — feedback is linked to features, not to categories.
- **Verdict:** adjacent/competitor for PM audiences.
- **Steal:** insight → feature linking with the source snippet preserved; "preview all edits before saving."
- Sources: [Spark blog](https://www.productboard.com/blog/introducing-spark-agentic-product-system/), [AI suggested insights](https://academy.productboard.com/ai-suggested-insights)

##### Dovetail — https://dovetail.com
- **Status:** Proprietary; free ≤3 users, Professional ~$39–49/editor/mo (third-party), Enterprise custom. Magic suite: auto-highlight, sentiment, theme suggestions, Magic cluster, AI Docs, AI Agents, semantic search. Funding not checked.
- **Taxonomy:** researcher-curated tags with AI suggestions.
- **Verdict:** adjacent (research repository).
- Sources: [CleverX review](https://cleverx.com/blog/dovetail-review-2026/)

##### Marvin (HeyMarvin) — https://heymarvin.com
- **Status:** Proprietary; **Agentic Ask AI** (Jan 2026) multi-agent search across video/audio/docs/tickets; MCP; PII blur; every answer cited.
- **Verdict:** adjacent.
- Sources: [heymarvin.com](https://heymarvin.com/), [Yahoo Finance](https://finance.yahoo.com/news/heymarvin-launches-industry-first-agentic-140000643.html)

##### Cycle.app — https://www.cycle.app
- **Status:** Proprietary; AI extraction of quotes/insights; deep Linear + GitHub integration to close the loop at release; pricing not public (domain blocked; details unverified).
- **Verdict:** adjacent.
- Sources: [G2](https://www.g2.com/products/cycle-app/reviews)

##### Featurebase — https://www.featurebase.app
- **Status:** Proprietary; AI duplicate detection + categorisation; in 2026 added inbox/tickets and an AI support agent "Fibi" (third-party; domain blocked).
- **Verdict:** adjacent.
- Sources: [aicxstack review](https://www.aicxstack.com/blog/featurebase-review)

##### Olvy — https://olvy.co
- **Status:** Active (page dated Aug 2026), ~5 staff. Auto-Listener agent filters noise; feedback-type classification (bug/feature), **auto-translation**, custom properties auto-fill, product-segment categorisation, Ask Olvy.
- **Verdict:** competitor (small).
- Sources: [olvy.co](https://olvy.co/), [PitchBook](https://pitchbook.com/profiles/company/520806-88)

##### Zeda.io — https://zeda.io
- **Status:** **Winding down in 2026** (single source — BuildBetter blog; unverified independently). ~$4M seed 2022.
- **Verdict:** gone.
- Sources: [BuildBetter](https://blog.buildbetter.ai/zeda-io-is-shutting-down-in-2026-what-happened-top-alternatives/)

##### Monterey AI → Reforge — https://www.monterey.ai
- **Status:** **Acquired by Reforge, Jan 15, 2025**; team building Reforge's AI-native PM tooling (Reforge site returned 403; product details unverified).
- **Verdict:** gone as standalone.
- Sources: [PitchBook](https://pitchbook.com/profiles/company/501459-76), [Balfour on X](https://x.com/bbalfour/status/1879676323796599291)

##### Viable — https://askviable.com
- **Status:** **Defunct** — site offline since 2025; PitchBook "Out of Business"; dang.ai marked inactive Jun 2024. Was GPT-4-based, $600–1,000+/mo by datapoint volume.
- Sources: [PitchBook](https://pitchbook.com/profiles/company/437198-68), [dang.ai](https://dang.ai/tool/ai-qualitative-data-analysis-tool-viable)

##### Prodsight, Painboard — (unverified) both domains failed (expired certificate / DNS not resolving) on 2026-09-14; treat as defunct or dormant.

##### Reveal by Sprig / Sprig AI — https://sprig.com/ai
- **Status:** Sprig markets "AI Analysis for Surveys" (summaries, ask-your-data, suggested questions) and AI agents; "Reveal" as a product name no longer appears (unverified; domain blocked).
- **Verdict:** adjacent.
- Sources: [G2](https://www.g2.com/products/sprig/reviews), [VentureBeat](https://venturebeat.com/ai/sprig-uses-ai-to-transform-product-surveys-into-conversational-data)

##### Zonka Feedback — https://www.zonkafeedback.com
- **Status:** Proprietary; thematic analysis, **entity/aspect auto-tagging** (agent/team/location/product), sentiment, **AI Impact Analysis** linking themes to NPS/CSAT/churn, anomaly alerts, AI co-pilot. Helpdesk + CRM integrations.
- **Verdict:** adjacent (survey-centric).
- Sources: [zonkafeedback.com](https://www.zonkafeedback.com/)

##### Survicate — https://survicate.com
- **Status:** Insights Hub (multi-source repository), Research Assistant (ask-AI), auto-categorisation of open-ends, sentiment; 50+ integrations. Free tier.
- **Verdict:** adjacent.
- Sources: [survicate.com](https://survicate.com/)

##### Hotjar / Contentsquare, Userpilot — AI survey summaries and (Userpilot) the "Lia" agent for metric monitoring; specific open-text classification features could not be verified (pages 404/blocked). Adjacent.
- Sources: [userpilot.com/ai](https://userpilot.com/ai/)

##### Insight7 — https://insight7.io
- **Status:** Has pivoted to **call analytics, QA and AI coaching** ("100% call coverage"); feedback analysis is secondary; no free tier.
- **Verdict:** adjacent.
- Sources: [insight7.io](https://insight7.io/)

##### Koji, Perspective AI, BuildBetter (2025–26 entrants seen in this scan)
- **Koji** (koji.so): AI-moderated interviews + thematic analysis, published pricing €1+/qualified interview; argues classification "cannot invent a category no customer mentioned." **Perspective AI** (getperspective.ai): same thesis, cites an 1,800-participant RCT on AI-assisted interviewing. **BuildBetter**: 35+ signal types from calls, claims 30,000+ teams (unverified). Trend: *generate better text* rather than *classify existing text*.
- **Verdict:** inspiration (the "unresolved subject" queue is Brightflow's answer to "the cost of the next question").
- Sources: [Koji](https://www.koji.so/blog/enterpret-alternatives-2026), [Perspective AI](https://getperspective.ai/blog/medallia-athena-alternatives-2026), [BuildBetter](https://blog.buildbetter.ai/zeda-io-is-shutting-down-in-2026-what-happened-top-alternatives/)

---

#### Tier 3 — helpdesk / issue-tracker triage AI

##### Zendesk Intelligent Triage + Zendesk QA — https://support.zendesk.com
- **Status:** Classifications (intent/topic, language, sentiment) on Suite and Support Professional+; using them in workflows needs the **Copilot add-on**. Custom intents with **categories and sub-categories**; import/export of intents. **Weekly personalised suggestions**: each carries name, description, category, **coverage %**, timeframe and **five example tickets**; admin clicks Add or Decline, and declining asks for a reason; accepted intents start detecting immediately. Zendesk QA: AutoQA scores 100% of conversations; Spotlight flags churn risk, dead air, outliers, compliance gaps, "loopy bots" (>90% accuracy claimed).
- **Taxonomy:** fixed prebuilt industry model + curated custom two-level intents + weekly agent-proposed additions with human accept/decline.
- **Joins:** none beyond Zendesk Explore.
- **Verdict:** inspiration (best-documented review loop in the slice).
- **Steal:** (1) suggestion card = coverage % + N example tickets + accept/decline-with-reason; (2) declined reasons feed back as negative signal; (3) a "standard set of topics" fallback for cold-start.
- Sources: [reviewing suggestions](https://support.zendesk.com/hc/en-us/articles/9484697389210-Reviewing-personalized-intelligent-triage-intent-suggestions), [custom intent categories](https://support.zendesk.com/hc/en-us/articles/9882930657050-Announcing-custom-intent-categories-for-intelligent-triage), [Spotlight](https://support.zendesk.com/hc/en-us/articles/7043759586074-Using-Zendesk-QA-Spotlight-insights-to-filter-conversations)

##### Intercom → Fin (Salesforce) — https://www.intercom.com
- **Status:** Company now brands itself **Fin**; **Salesforce signed a definitive agreement June 15, 2026 and completed the acquisition Sept 10, 2026** (Salesforce press-release index + Fin blog); ~$3.6B price (third-party, unverified). Fin pricing ≈ $0.99/outcome. AI Insights: **Topics Explorer** (auto topics/subtopics, no manual tagging, no custom-topic editing documented), **Trends** (weekly: biggest shifts explained in plain language), Monitors, CX Score — all behind the Pro add-on. Topic names unsupported (display in English) for Swahili, Bengali, Bosnian, Azerbaijani, Persian, Sinhala, Tamil; CX Score works in all languages. New Aug 2026: Evals and Releases for the agent.
- **Taxonomy:** freeform AI-generated, non-editable (as documented).
- **Verdict:** adjacent (helpdesk-embedded); worth watching post-Salesforce.
- **Steal:** weekly "what moved and why" narrative over topic volumes — Brightflow's detectors + LLM narration can do exactly this per vocabulary category.
- Sources: [Insights help article](https://www.intercom.com/help/en/articles/10576273-measure-customer-service-with-insights-built-for-the-ai-agent-era), [Fin blog](https://www.intercom.com/blog), [Salesforce press releases](https://www.salesforce.com/news/press-releases/)

##### Help Scout AI Topics — https://docs.helpscout.com/article/1773-categorize-conversations-automatically-with-ai-topics
- **Status:** Plus/Pro plans. Generates **8–15 topics per inbox** from history, scans daily, adds topics when trends emerge. Admins can edit name/emoji/description, add, delete or disable topics — but **manual edits do not feed back into the model during beta**. Topics usable as workflow conditions (not actions); Reports > Topics volume over time.
- **Taxonomy:** small auto-generated, human-editable, non-learning.
- **Verdict:** inspiration (of what not to do: edits that the model ignores).
- Sources: [docs](https://docs.helpscout.com/article/1773-categorize-conversations-automatically-with-ai-topics)

##### Plain — https://www.plain.com
- **Status:** B2B support; Plain Insights groups conversations into recurring themes, detects trends, flags churn signals, **daily summaries** to product teams at no extra cost (Plain blog via search summary; fetch returned only the shell — details unverified).
- **Verdict:** adjacent.
- Sources: [Plain AI features](https://www.plain.com/blog/introducing-ai-features)

##### Pylon — https://usepylon.com
- **Status:** $51M raised (a16z/Bain Series B $31M, Aug 2025); 750+ customers; July 2026 "agentic support" repositioning (third-party). Product intelligence surfaces churn signals, feature requests, renewal risk across accounts; ask-AI with sources; native Salesforce/HubSpot/Linear/Jira/**Snowflake**.
- **Verdict:** adjacent — but the Snowflake + CRM joins put it nearer Brightflow's thesis than most helpdesks.
- Sources: [SaaStr](https://www.saastr.com/saastr-ai-app-of-the-week-pylon-the-ai-native-support-platform-built-for-b2b-not-reheated-b2c/), [Tracxn](https://tracxn.com/d/companies/pylon/__Ac2-X2f7ntsfVm-XCmE_yD2caYAugIR4VjR9jMJvEcQ)

##### Freshdesk Freddy — https://support.freshdesk.com/support/solutions/articles/50000002117-setting-up-auto-triage
- **Status:** Auto Triage (Pro/Enterprise + Freddy Copilot add-on, ~$29/agent/mo) predicts Priority, Group, Type, plus **custom and dependent (nested) dropdown fields** from historical tickets; agents accept/modify/reject in the properties panel and the model learns from each decision.
- **Taxonomy:** your existing ticket fields *are* the taxonomy — supervised on history.
- **Verdict:** inspiration.
- **Steal:** train/evaluate classification against the customer's *existing* fields before proposing a new vocabulary; report agreement rate per field.
- Sources: [Freshdesk docs](https://support.freshdesk.com/support/solutions/articles/50000002117-setting-up-auto-triage), [eesel guide](https://www.eesel.ai/blog/freshdesk-ai-auto-triage)

##### Jira Service Management / Rovo — https://www.atlassian.com/software/jira/service-management/product-guide/tips-and-tricks/artificial-intelligence
- **Status:** AI triage recommends request type and fields for bulk categorisation; Rovo "Service Triage Assistant" agent reads content/sentiment to suggest type/urgency/priority; explicitly **advisory, humans decide**. Atlassian claims agentic automations growing 3.4x faster than rule automations.
- **Verdict:** adjacent.
- Sources: [Atlassian AI guide](https://www.atlassian.com/software/jira/service-management/product-guide/tips-and-tricks/artificial-intelligence), [Rovo agents blog](https://www.atlassian.com/blog/jira-service-management/rovo-agents-service-desk-instant-resolution)

##### Linear Product Intelligence → Triage Intelligence — https://linear.app/changelog/2025-08-14-product-intelligence-technology-preview
- **Status:** Tech preview Aug 2025 on Business/Enterprise; **renamed Triage Intelligence**. Suggests team/project/assignee/labels, related issues and duplicates; hover shows reasoning; accept/dismiss trains it; workspace/team-level guidance text. Customer Requests links feedback from sales/support to issues.
- **Taxonomy:** your labels/projects; the AI routes into them.
- **Verdict:** inspiration (Brightflow's GitHub-issues connector could mirror this).
- **Steal:** show *why* on every suggestion; let humans write standing guidance that steers the classifier, not just per-item corrections.
- Sources: [changelog](https://linear.app/changelog/2025-08-14-product-intelligence-technology-preview), [Intake](https://linear.app/intake)

##### GitHub Copilot issue triage (gh-aw) and Dosu — https://github.com/github/gh-aw, https://dosu.dev/oss
- **Status:** GitHub's **agentic workflows** ("[aw] Issue Triage Agent") run as Actions to label, ask for clarification, close duplicates and post daily PR-triage reports; visible failures in 2026 from invalid model configuration. **Dosu**: 50k+ projects; free for public repos, Pro trial, Enterprise with self-hosting; **auto-label** is an open-source Action that "learns from your corrections"; answers in the user's language; users can choose the model.
- **Taxonomy:** repo labels (curated), agent-applied.
- **Verdict:** inspiration for the GitHub connector; Dosu's OSS auto-label is the nearest open-source reference for correction-driven labelling.
- Sources: [gh-aw triage report](https://github.com/github/gh-aw/issues/58487), [Dosu OSS](https://dosu.dev/oss)

##### SentiSum — https://www.sentisum.com
- **Status:** Proprietary; tags "reasons for contact" in real time and **writes them back into the helpdesk** (standard tag field or custom field) for native reporting, prioritisation and routing. Taxonomy creation, hierarchy and retraining are not documented publicly (unverified).
- **Verdict:** competitor (support-analytics niche).
- **Steal:** write classifications back to the source system as fields, so the source's own reports pick them up.
- Sources: [help centre](https://help.sentisum.com/automatic-ticket-tagging)

##### Lang.ai → Capacity — **acquired April 21, 2026**, terms undisclosed; was no-code, human-controllable classification models for Zendesk/Salesforce, later "agentic analytics." Gone as standalone. Sources: [Yahoo Finance](https://finance.yahoo.com/sectors/technology/articles/capacity-acquires-lang-ai-adding-130000401.html)

##### Levity — pivoted away from feedback classification to logistics/B2B communication agents ($8.3M seed, Balderton). Not relevant. Sources: [Balderton](https://www.balderton.com/news/levity-raises-8-3m-to-bring-the-power-of-ai-to-businesses-of-all-sizes/)

---

#### Tier 4 — adjacent (calls, CS platforms)

- **Gong** — smart trackers now multilingual; AI "Ask Anything" and Deep Researcher; topic tracking, objections, sentiment. Accuracy still "inconsistent" on nuance per reviewers. Adjacent. [tldv review](https://tldv.io/blog/how-does-gong-work/)
- **Gainsight Staircase AI** — signals (email/calls/tickets) → scored risk + action; joins to usage/revenue inside Gainsight; demo-only pricing. Adjacent. [gainsight.com](https://www.gainsight.com/product/staircase-ai/)
- **Vitally** — AI summaries from transcripts/notes/tickets, agentic plays, MCP server. Adjacent. [vitally.io](https://www.vitally.io/)
- **Planhat** — "agentic customer platform," governed agents, health scoring blending human + AI inputs, conversation capture; no sentiment/taxonomy specifics public. Adjacent. [planhat.com](https://www.planhat.com/)

##### Open-source ticket classification (2025–26)
Nothing with real adoption beyond Dosu auto-label. GitHub's `ticket-classification` topic shows small LLM projects (LangGraph multi-agent routers, RAG+Llama 3.1, n8n+Gemini, TF-IDF+NB baselines at ~0.82 accuracy), all ≤4 stars. Brightflow's Rust primitives + versioned calls have no OSS peer in this space. Source: [github.com/topics/ticket-classification](https://github.com/topics/ticket-classification)

---

#### Patterns across this slice

1. **Taxonomy evolution has converged on "adaptive with a human gate."** Fixed taxonomies (Idiomatic-style supervised, Qualtrics/Medallia hierarchies) survive only in enterprise suites; freeform LLM tags (Intercom Topics, Help Scout, Amplitude) win on zero setup but are non-editable or non-learning; the credible middle is a curated vocabulary plus *scheduled* agent proposals with accept/decline (Thematic monthly digest, Zendesk weekly suggestions, Linear/Freshdesk per-item feedback). Brightflow's three-level induce/propose + review + undo is right on this line and is more explicit about *versioning* than anyone except Thematic's `.themes` files.
2. **The best review UIs put evidence on the suggestion card:** coverage %, N example tickets, a hover-able "why," and a reason captured on decline (Zendesk, Linear). Health metrics are becoming standard — Thematic's coverage/specificity/accuracy tracker and Caplena's per-assignment quality are the two to copy; Brightflow's "unresolved subject" queue is a coverage metric that no one else exposes as a first-class work queue.
3. **Human edits that the model ignores are the common failure** (Help Scout beta; Intercom no editing at all). Brightflow's action bus is a genuine advantage if every accepted/rejected/edited item demonstrably changes the next run.
4. **Multilingual is a checkbox, not a metric.** Vendors say "100+ languages" (Caplena, Olvy translation, Unwrap) or list exclusions (Intercom). Nobody publishes per-language quality or usage. Brightflow's per-language usage/eval (sv/fi/en) is ahead; the gap is making per-language *accuracy* visible the way Thematic makes theme quality visible.
5. **Cost transparency and BYO model are almost absent.** Canny's $0.10 credits and Fin's $0.99/outcome are the only published unit prices; Dosu is the only one letting users pick the model; six of eight platforms in the Koji comparison publish no price at all, Unwrap starts at $24k/yr. Brightflow's OpenAI-compatible endpoint + sample runs + cost estimates is a differentiator to lead with.
6. **Joining text to numbers is the 2026 battleground, and incumbents are moving in from the numbers side.** Amplitude (free AI Feedback tied to conversion/segments/replay), Pylon (Snowflake + CRM), Enterpret's Context Graph (imported LTV/usage), Zonka's "impact analysis," Caplena's driver analysis. Most do it by *importing* one side into the other; Brightflow is the only one where classified tickets and usage/revenue live in the same engine with shared semantics — but Amplitude giving the join away free is the threat to name.
7. **Consolidation is fast:** Kraftful→Amplitude, Idiomatic→Front, Monterey→Reforge, Lang.ai→Capacity, MonkeyLearn→Medallia, Intercom/Fin→Salesforce; Viable, Zeda, (likely) Prodsight and Painboard are gone. Point solutions without a data moat are being absorbed into helpdesks, product analytics and CRMs — Brightflow's self-hosted single-binary posture is orthogonal to that squeeze.
8. **Delivery is uniform:** Slack/email digests (weekly at Intercom/Zendesk, monthly at Thematic, daily at Plain/Help Scout) with plain-language "what moved and why." Brightflow's deterministic detectors + narration-only LLM is a cleaner version of the same pattern; the missing piece relative to the field is the scheduled digest as a delivery surface.

---

## Pass 5: Product & web analytics slice — competitor/inspiration scan (as of 2026-09-14)


Scope: product analytics and web analytics, weighted toward self-hosted / single-binary / privacy-first, plus each product's data-management (taxonomy) layer and AI/MCP surface. Brightflow's subset for comparison: tracking script + collect/track/identify, geo/UA enrichment, SQLite buffer → Parquet under a SQLite catalog; stats, time series, pages, referrers, UTM, geo, devices, funnels, retention, user profiles/timelines. No replay, experiments, or saved cohorts.

---

#### Tier 1 — the platforms that define the category

##### PostHog — https://posthog.com
All-in-one product OS: analytics, web analytics, replay, flags, experiments, errors, surveys, logs, data warehouse, messaging.
- **Status:** MIT core (39.8k stars); cloud-first, "97% of companies use PostHog for free"; per-product usage pricing with large free tiers (1M events, 5K replays, 1M rows warehouse, 500 AI credits ≈ $5). Self-hosting "not recommended above 300k events/month". 2026 launches: **Self-driving** platform (Jun 26: Scouts, Reports, Inbox — agents detect, investigate, open PRs); data-warehouse source blitz (50+ sources/month over Jul–Sep); 86+ new MCP tools Aug–Sep; "AI" traffic channel (Jul 28); governed metrics / semantic layer (beta); PostHog Desktop (beta); Workflows.
- **Architecture:** Django + ClickHouse + Postgres + Kafka + Redis; heavy multi-service self-host; Hobby docker-compose only.
- **Features vs Brightflow:** superset of everything Brightflow has; adds replay, flags, experiments, cohorts, surveys, warehouse, error tracking.
- **Data management (the reference):** event & property definitions with editable description, tags, **verified** flag, **last seen**, top custom properties per event; a **History** tab logging every change to descriptions/tags/verification/deletion; Actions (composed events); Annotations; **Ingestion warnings** (e.g. generic distinct_ids like `null`) logged but events still ingested; bulk verify via API; new **schema management** — define events + typed property groups in `posthog.json`, CLI generates TS/Go/Python types, `captureRaw()` escape hatch; enforcement is dev-time typing, no runtime blocking.
- **AI/agents:** PostHog AI (ex-Max) — builds insights, writes SQL, finds replays, creates flags/surveys/alerts, opens PRs; "agent mode"; persistent product memory; "respects access controls — sees only what you can see"; Slack surface; credit-priced. MCP: read+write across products, region-routed OAuth, "safer MCP intent redaction" (Sep 11), most-specific access-rule resolution.
- **Overlap:** high on analytics views; PostHog's warehouse is the direct analogue of "events next to Stripe/HubSpot in one SQL" — its strongest positioning move since 2024.
- **Steal:** (1) definition *history* + *last seen* + verified as first-class columns on the catalog; (2) "failed materialised view becomes a signal for the agent, not a stale dashboard"; (3) ingestion warnings as a visible queue rather than silent drops.
- **Verdict:** both (competitor on features, inspiration on taxonomy and agent governance).
- Sources: [Data docs](https://posthog.com/docs/data), [schema management](https://posthog.com/docs/product-analytics/schema-management), [pricing](https://posthog.com/pricing), [MCP](https://posthog.com/docs/model-context-protocol), [PostHog AI](https://posthog.com/docs/posthog-ai), [changelog](https://posthog.com/changelog), [data warehouse](https://posthog.com/docs/data-warehouse), [open-source roundup](https://posthog.com/blog/best-open-source-analytics-tools)

##### Amplitude — https://amplitude.com
Enterprise product analytics; now also flags/experiments, replay, guides & surveys, CDP, agents — and Statsig.
- **Status:** public (NASDAQ: AMPL). Event-metered since 2025: Free 2M events/mo; Plus scales to 70M; Growth/Enterprise custom. **June.so** wound down Aug 8, 2025 — founding team acqui-hired (announced Jul 8, 2025). **Statsig**: OpenAI bought it Sep 2, 2025 ($1.1B, all-stock, CEO became OpenAI CTO of Applications); on May 5, 2026 Amplitude took over the Statsig *brand, platform and customers* (~$16M incremental ARR) while the engineering team stayed at OpenAI — Phase 1 (6–18 months) is governance/MCP/AI-eval work, Phase 2 data-layer integration by end Q3 2026.
- **Architecture:** proprietary SaaS; warehouse-native option via Statsig.
- **Features vs Brightflow:** superset; behavioral cohorts and heatmaps gated to Growth+.
- **Data management:** Amplitude Data — tracking plan ("lock in a tracking plan, review changes, keep events on-spec"), event/property definitions with types and validation rules (regex, min/max), active/inactive status, ownership, @mention review, **transformations without redeploy** (rename/redact/reshape post-ingest), lookup tables, access controls on sensitive properties, Ampli type-safe codegen. Branches/versions and unexpected-event blocking exist (unverified from fetched pages — the doc URLs 404'd).
- **AI/agents:** MCP server GA Oct 21, 2025 (natural-language over live data in Claude/Cursor); Agents open beta — Dashboard Agent (root-cause, pushes to Slack/email) and Session Replay Agent; "governed access" claimed, mechanism unspecified. Included on all plans incl. Free.
- **Overlap:** conceptual; not a self-host competitor.
- **Steal:** post-ingest transformations (rename/redact) that apply retroactively without touching the SDK — maps cleanly onto Brightflow's semantic layer with provenance.
- **Verdict:** inspiration (taxonomy/agents); adjacent as a market.
- Sources: [MCP press](https://amplitude.com/press/amplitude-brings-behavioral-data-into-ai-tools-with-mcp-launch), [Amplitude Data](https://amplitude.com/docs/data), [tracking plan](https://amplitude.com/docs/data/create-tracking-plan), [pricing](https://amplitude.com/pricing), [June](https://www.june.so/blog/a-new-chapter), [Statsig→OpenAI](https://www.statsig.com/blog/openai-acquisition), [GeekWire](https://www.geekwire.com/2025/openai-acquires-statsig-for-1-1b-names-ceo-to-key-exec-role-in-surprise-exit-for-seattle-area-unicorn/), [Statsig Phase 1](https://www.statsig.com/blog/statsig-amplitude-phase-1), [MarTech](https://martech.org/amplitude-and-statsig-deal-raises-questions-for-customers/)

##### Mixpanel — https://mixpanel.com
Event analytics with replay, flags/experiments (Oct 2025), warehouse connectors, Metric Trees (Aug 2025).
- **Status:** private; Free 1M events/mo, Growth to 20M, Enterprise; startups first year free. Spark renamed **Mixpanel Agent** (Jun 2026). MCP server Apr 2026, on by default for new Free/Growth orgs after Aug 1, 2026.
- **Architecture:** proprietary (Arb engine); warehouse "Mirror mode" sync from Snowflake/BigQuery/Databricks/Redshift/Postgres.
- **Data management — Lexicon:** descriptions, owners, tags, platform, display names; **hide** (from builders, keep history), **drop/block** (stop ingesting), **merge** duplicates; **30-day volume and query counts** per event/property ("which data drives analysis, which is unused"); CSV import/export; Schemas API; Segment/Avo/mParticle import; Enterprise "verified" badges; data-quality issues you can dismiss.
- **AI/agents:** MCP with **40+ read/write tools** including `Edit-Event`, `Bulk-Edit-Events/Properties`, tag ops, cohort/dashboard creation, dismiss data-quality issues; **"Verified Mode"** — when on, MCP results flag unverified entities while still returning data; OAuth or service accounts; inherits project roles; 600 req/h/user.
- **Overlap:** conceptual only.
- **Steal:** (1) volume + query-count as the taxonomy's freshness/usage signal; (2) hide/drop/merge as three distinct governance verbs; (3) Verified Mode as an *agent-facing* filter over the same taxonomy humans curate.
- **Verdict:** inspiration.
- Sources: [Lexicon](https://docs.mixpanel.com/docs/data-governance/lexicon), [MCP](https://docs.mixpanel.com/docs/features/mcp), [facts](https://mixpanel.com/blog/mixpanel-facts/), [pricing](https://mixpanel.com/pricing)

##### Heap → Contentsquare Product Analytics — https://contentsquare.com/platform/product-analytics/
Autocapture analytics; acquired 2023, fully folded into Contentsquare with Hotjar since Nov 2024.
- **Status:** proprietary; Free/Growth/Pro(Sense AI)/Enterprise tiers. Hotjar.com now 301s to contentsquare.com/hotjar; free tier 200k sessions/mo with heatmaps, replay, funnels, surveys.
- **Data management:** retroactive **virtual events** defined over autocaptured data, event labelling and verification.
- **AI:** Sense AI, MCP ("LLM Connect" for ChatGPT/Claude/Copilot).
- **Steal:** retroactive definitions — define an event *after* the fact over raw captured data. Brightflow's Parquet-backed raw events make this cheap: an "event definition" can be a saved predicate in the semantic layer, not a tracking-code change.
- **Verdict:** inspiration/adjacent.
- Sources: [Contentsquare PA](https://contentsquare.com/platform/product-analytics/), [Hotjar page](https://contentsquare.com/hotjar/), [Heap next chapter](https://contentsquare.com/heap/)

---

#### Tier 2 — lightweight / privacy-first web analytics (Brightflow's direct neighbours)

##### Plausible — https://plausible.io
Cookieless web analytics; Elixir/Phoenix, **Postgres + ClickHouse**; AGPLv3 CE (29.1k stars), MIT tracker.
- **Status:** CE updated twice a year, community-supported; cloud-only: marketing funnels, **user journeys** (May 2026), revenue goals, SSO, advanced bot filtering. 2025–26: saved segments, scroll depth, engagement-based time on page, **AI Assistants channel** (Jun 2026), traffic-chart annotations (Jul 2026), funnel comparison + revenue per step (Sep 9, 2026).
- **Features vs Brightflow:** pages/referrers/UTM/geo/devices/goals/funnels yes; **no user profiles or timelines, no retention** by design (no identifiers).
- **Data management:** goals + custom props; no taxonomy layer. **AI:** none.
- **Steal:** annotations on the traffic chart; AI-assistant channel classification (PostHog and Matomo shipped the same in mid-2026 — a table-stakes referrer bucket now).
- **Verdict:** competitor (web slice).
- Sources: [changelog](https://plausible.io/changelog), [repo](https://github.com/plausible/analytics), [self-hosting](https://plausible.io/docs/self-hosting), [Loopwerk on CE breakage](https://www.loopwerk.io/articles/2026/plausible/)

##### Umami — https://umami.is
MIT, Next.js + **Postgres** (MySQL dropped in v3); v3 line through 2026 (v3.1.0 Apr 2026 per umami.is; later point-release dates unverified).
- **Features:** funnels with per-step property filters, **retention, journeys**, goals, attribution, segments/cohorts, **session replay (rrweb) and heatmaps (v3.2)**, Web Vitals, custom boards, revenue, identity stitching across devices.
- **Data mgmt / AI:** none notable. Docker-compose self-host; lowest complexity of the ClickHouse-free crowd.
- **Overlap:** very high — Umami v3 is essentially Brightflow's analytics subset plus replay/heatmaps, on Postgres.
- **Steal:** per-step property filters in funnels; cumulative-mode revenue report.
- **Verdict:** competitor.
- Sources: [releases](https://github.com/umami-software/umami/releases), [docs](https://docs.umami.is/docs)

##### Matomo — https://matomo.org
GPLv3 PHP/MySQL; 21.9k stars; 5.4 → 5.13 across 2025–26, **6.0 beta Sep 9, 2026**; cloud from €22/mo, on-prem free; Funnels/Retention are paid premium plugins.
- **AI:** Matomo MCP (authorised users ask about traffic/campaigns), AI-agent/assistant traffic reports. Verdict: competitor (heavyweight, dated). Sources: [changelog](https://matomo.org/changelog/), [blog](https://matomo.org/blog/), [pricing roundup](https://fpai.orora.co.jp/blog/matomo-pricing-2026/)

##### Fathom — https://usefathom.com
Proprietary SaaS. **V4 engine rebuilt (Feb 28 / Mar 4, 2026)**: new query layer, single-pass scans, pre-aggregated all-time; DB not disclosed. Gained secondary dimensions, entry/exit pages, search terms, bot-detection layer, per-box CSV exports, comparison tooltips. No AI/MCP, no product analytics. Verdict: adjacent. Sources: [rebuild](https://usefathom.com/changelog/mar2026-analytics-rebuild), [changelog](https://usefathom.com/changelog)

##### Pirsch — https://pirsch.io
Go + **ClickHouse**; AGPL *library* only (the product is SaaS, server-side/no-cookie). Not a single-binary self-host. Verdict: adjacent. Source: [repo](https://github.com/pirsch-analytics/pirsch)

##### Rybbit — https://rybbit.com
AGPLv3, ~13k stars, Node/React + **ClickHouse**, docker-compose. Sessions, **replay**, funnels, journeys, goals, retention, user profiles, error tracking, 3-level geo maps; Web Vitals cloud-only. No taxonomy/AI found. Verdict: competitor (closest OSS feature match, ClickHouse cost). Source: [repo](https://github.com/rybbit-io/rybbit)

##### Swetrix — https://swetrix.com
AGPLv3, ~1.2k stars, NestJS + MySQL + **ClickHouse** + Redis; errors, performance, replays (S3), funnels, flows, flags, experiments, "AI Chat" on cloud. Verdict: competitor. Source: [repo](https://github.com/swetrix/swetrix)

##### OpenPanel — https://openpanel.dev
AGPLv3, ~7k stars, Next.js/Fastify + Postgres + **ClickHouse** + Redis + BullMQ; funnels, retention, cohorts, profiles, session history, replay, A/B, **MCP server**; cloud/self-host parity. Verdict: competitor (the "Mixpanel-shaped OSS" reference). Sources: [repo](https://github.com/Openpanel-dev/openpanel), [survey](https://openpanel.dev/articles/open-source-web-analytics)

##### Countly — https://countly.com
AGPL (modified §7) Node + **MongoDB**; Lite is non-commercial; Flex from $175/mo; **funnels, retention, flows and "Cee" AI are Enterprise/Intelligence add-ons**. Mobile-first. Verdict: adjacent. Sources: [pricing](https://countly.com/pricing), [repo](https://github.com/Countly/countly-server)

##### Aptabase — https://aptabase.com
AGPLv3 (SDKs MIT), 1.8k stars; mobile/desktop/web app analytics with **no user identifiers at all** (sessions only). Free 20k events; $10/200k … $450/50M, **no overage — pauses**. Self-host via separate compose repo (stack unverified). Verdict: adjacent. Sources: [repo](https://github.com/aptabase/aptabase), [pricing](https://www.aptabase.com/pricing)

##### Vemetric — https://vemetric.com
AGPLv3, 390 stars, TypeScript + **ClickHouse** + Postgres; web + product in one (journeys, funnels, event stream, profiles); free 2.5k events, Pro from $5/mo; **self-host CE "planned, early"**. Verdict: competitor-in-waiting; same "web+product in one dashboard" pitch as Brightflow. Sources: [repo](https://github.com/vemetric/vemetric), [Capterra](https://www.capterra.com/p/10036187/Vemetric/)

##### Databuddy — https://databuddy.cc
AGPLv3, 1.2k stars, Bun/Next 16 microservices + Postgres + **ClickHouse** + Redis; visits/events/errors/funnels/flags/uptime/short links/Web Vitals; **"Databunny" AI agent for investigations + anomaly detection; MCP**. Free 10k events, Hobby $9.99/25k, overage $0.03→$0.01/1k. Verdict: competitor (feature-sprawl variant). Sources: [site](https://www.databuddy.cc/), [repo](https://github.com/databuddy-analytics/Databuddy)

##### Seline — https://seline.com
Proprietary, bootstrapped (Warsaw, 2024), **ClickHouse**; web + product (profiles, journeys, funnels, churn), Stripe/Polar/Shopify revenue, **AI assistant + MCP**; single Pro plan, 7-day trial, no free tier. Verdict: competitor (SaaS-only). Sources: [site](https://seline.com/), [pricing](https://seline.com/pricing)

##### Simple Analytics — https://www.simpleanalytics.com
Proprietary EU SaaS, since 2018, 31k users; pageviews/events/referrers/UTM only; "AI assistant" (plain-English → charts). Not self-hostable. Verdict: adjacent. Sources: [site](https://www.simpleanalytics.com/), [AI](https://www.simpleanalytics.com/ai)

##### GoatCounter, Medama (single-binary, ClickHouse-free)
- **GoatCounter** — https://github.com/arp242/goatcounter: Go **single static binary, SQLite or Postgres**, ~6k stars, v2.7; pages/referrers/geo/devices/campaigns/events; free hosted tier. No user model.
- **Medama** — https://github.com/medama-io/medama: Go **single binary on DuckDB (SQLite supported)**, Apache-2.0, 643 stars, <1KB tracker, 256MB RAM. Web-only, real-time.
- These two are the only established projects in Brightflow's exact deployment shape; neither has product analytics, profiles, or a taxonomy. Verdict: inspiration (architecture validation).

##### Other 2025–26 entrants found
- **Tianji** (Apache-2.0, 3.1k stars, Postgres) — web analytics + uptime + server status in one box. [repo](https://github.com/msgbyte/tianji)
- **Litlyx** (Apache-2.0, 1.7k stars, Nuxt + MongoDB) — events + "AI dashboard". [repo](https://github.com/Litlyx/litlyx)
- **lyraflow** (fair-code, ClickHouse+Postgres, v0.16, 2 stars) — journey analytics with anonymous→known stitching, funnels, retention grids, person merging; explicitly "early days". [repo](https://github.com/lyraflow/lyraflow)
- **Statsman** (SQLite/Postgres, tiny) and **theia** (Go, nginx-log parsing, no JS) — hobby-scale. [GitHub topic](https://github.com/topics/web-analytics?o=desc&s=updated)
- I found **no Rust single-binary product-analytics project** with traction; Rust entries on the topic pages are TUIs and SDKs. (Web-search budget ran out before targeted "Rust PostHog alternative" queries; treat as unverified negative.)

---

#### Tier 3 — event pipelines / CDPs (taxonomy owners)

##### Snowplow — https://snowplow.io
Pipeline apps (Collector 3.0+, Enrich 4.0+) moved from Apache-2.0 to the **Snowplow Limited Use License** ("non-production only, no distribution"); Community License for libs ("non-competing"); trackers stay Apache-2.0. **OpenSnowcat** is the Apache-2.0 fork. 2026 product is **Signals** (real-time profile/attribute API, 6ms p50) aimed at agentic apps. Verdict: adjacent; the license story is the cautionary tale. Sources: [licensing](https://docs.snowplow.io/docs/resources/copyright-license/), [Signals Jan 26](https://snowplow.io/blog/snowplow-signals-update-jan26), [OpenSnowcat](https://opensnowcat.io/manifesto)

##### Jitsu — https://jitsu.com
MIT; Go + TypeScript, Postgres + Kafka/Redpanda + ClickHouse + MongoDB (heavy); Segment-compatible collection, JS Functions, 70+ destinations, **Live Events** debugger; May 2026: OpenAPI spec, `jitsu-cli`, **MCP server that configures pipelines, watches live events, fixes Functions**. Free cloud 200k events. Verdict: adjacent (inspiration: live-events debugger, agent-operable pipeline). Sources: [repo](https://github.com/jitsucom/jitsu), [blog](https://next.jitsu.com/blog)

##### RudderStack — https://rudderstack.com
Private; MTU/event pricing from ~$600–750/mo. **Tracking Plans**: rules per track/identify/page/screen/group, required properties + types, violation context propagated to destinations, versioned plans validated against the version the SDK was built with, Codegen, Data Catalog API. Verdict: adjacent. Sources: [tracking plans](https://www.rudderstack.com/docs/data-governance/tracking-plans/), [pricing](https://www.trustradius.com/products/rudderstack/pricing)

##### Segment (Twilio) — https://segment.com
**Protocols** (Business-plan add-on): tracking plan, violations, block non-conforming events with optional **quarantine source**, transformations to rename without code, AI-detected naming inconsistencies. Verdict: adjacent. Sources: [Protocols](https://www.twilio.com/docs/segment/protocols), [product page](https://www.twilio.com/en-us/products/protocols)

##### Kubit — https://kubit.ai
Warehouse-native (Snowflake integration announced Mar 18, 2026), "dynamic semantic layer", AI agents that generate and execute SQL in-warehouse, MCP for Claude Code ("Analytics as the Verifier"); Aug 2026 pivot to correlating **agent traces with user funnels/retention**. Verdict: inspiration (semantic-layer-first analytics, agent-verifier framing). Sources: [product](https://kubit.ai/product/), [Snowflake PR](https://kubit.ai/press-releases/warehouse-native-digital-analytics-snowflake/), [home](https://kubit.ai/)

##### mParticle / Indicative — https://mparticle.com
Rokt acquired mParticle; positioning now "agentic intelligence"/"performance engine"; the Indicative-derived Analytics product is not surfaced on the homepage (status unverified; the /products/analytics URL 404s). Verdict: adjacent, fading.

---

#### Tier 4 — experience/observability adjacents (brief)

- **Microsoft Clarity** — free forever, no traffic limits; **MCP server (Aug 2026)**, Copilot, Citations/AI-visibility dashboards, bot analytics, mobile. Replay/heatmaps, not a data model. Adjacent. [blog](https://clarity.microsoft.com/blog)
- **Hotjar** — merged into Contentsquare (see above); Sense AI + LLM Connect. Adjacent.
- **LogRocket** — Galileo AI "watches every session"; funnels, paths, timeseries; no MCP found; G2 leader Spring 2026. Adjacent. [site](https://logrocket.com/)
- **Fullstory** — "Intelligent DXP", StoryAI + StoryAI Agents + **Fullstory MCP**, Fullcapture autocapture, warehouse enrichment. Adjacent. [site](https://www.fullstory.com/)
- **Pendo** — pivot to **Agent Analytics** (measure where *agents* fail), Headless Pendo APIs, **Pendo MCP**, Leo agent. Adjacent. [site](https://www.pendo.io/)
- **Smartlook** — Cisco; **winding down**: end of sale May 31, 2026, no renewals after Aug 31, 2026, decommission Sep 30, 2027; features migrate to Splunk DEA. Exiting. [site](https://www.smartlook.com/)
- **Usermaven** — web+product, attribution, journeys, retention, Maven AI, **MCP**, "Measurement Trust Center" (continuous data-confidence monitoring). Competitor-lite SaaS. [site](https://usermaven.com/)
- **Woopra** — journeys, retention, people profiles, automations, "160k+ customers"; owner (Appier acquisition) unverified. Adjacent/legacy. [site](https://www.woopra.com/)
- **Kissmetrics** — since 2008; funnels, cohorts, people, revenue, A/B, plain-English agentic queries, "LLM acquisition" channel; free-to-start. Ownership unverified. Legacy. [site](https://www.kissmetrics.io/)
- **Fibotalk** — site connection refused on 2026-09-14; presumed defunct (unverified).
- **Flurry** — **sunset Mar 15, 2024** (Yahoo). Gone. [site](https://www.flurry.com/)
- **Sentry** — 2026 changelog shows Agent Tracing GA (Sep 2026), AI Query Assistant, weekly-report revamp; **no product-analytics/funnel launch found**. Adjacent. [changelog](https://sentry.io/changelog/)
- **Grafana Faro** — web vitals, errors, logs, client traces into Grafana Cloud; no funnels/retention. Adjacent. [docs](https://grafana.com/docs/grafana-cloud/monitor-applications/frontend-observability/)
- **Datadog Product Analytics** — journeys/Sankey, funnels, retention, heatmaps, replay, user profiles on the RUM SDK; Bits AI + **MCP server**. Adjacent-but-encroaching. [product](https://www.datadoghq.com/product/product-analytics/)
- **Vercel Web Analytics** — Hobby 50k events/mo (1-month window, no custom events); Pro $0.03/1k events, 12-month window, custom events with 2 props; **Plus add-on $10/mo** for UTM + 8 props + 24 months; daily-rotating request hash for visitors; export via Drains. Adjacent. [pricing](https://vercel.com/docs/analytics/limits-and-pricing)
- **Netlify Analytics** — server-log based (no script, ad-block-proof): pageviews, uniques by IP, top pages/sources/locations, 404s, bandwidth; 30-day window. Adjacent. [docs](https://docs.netlify.com/monitor-sites/site-analytics/)
- **Cloudflare Web Analytics** — free on all plans, no proxy needed; visits, page load, Core Web Vitals; dimensions country/host/path/referer/device/browser/OS/site, bot exclusion. No custom events. Adjacent. [docs](https://developers.cloudflare.com/web-analytics/data-metrics/dimensions/)
- **Tinybird web-analytics starter** — MIT template (421 stars): flock.js tracker → Tinybird Events API → ClickHouse datasources/pipes → Next.js dashboard; hits + custom events + Web Vitals, multi-tenant. Adjacent (build-your-own on hosted ClickHouse). [repo](https://github.com/tinybirdco/web-analytics-starter-kit)

---

#### Patterns across this slice

1. **ClickHouse is the default, and its cost is the gap.** Every OSS product-analytics entrant of 2024–26 (Rybbit, OpenPanel, Swetrix, Vemetric, Databuddy, lyraflow, Seline, Pirsch, Plausible) sits on ClickHouse + Postgres + Redis (+ Kafka for PostHog/Jitsu). Only web-only tools escape it — Umami (Postgres), GoatCounter (SQLite), Medama (DuckDB). Nobody ships *product* analytics (profiles, retention, funnels) as a single binary on an embedded engine. Brightflow's SQLite-buffer → Parquet → Polars shape is genuinely unoccupied territory; the risk is the reverse — proving funnels/retention stay fast without ClickHouse at 10⁷–10⁸ events.

2. **Pricing converged on metered events with a fat free tier and "pause, don't bill" for hobbyists.** Amplitude 2M, PostHog 1M, Mixpanel 1M, Vercel 50k, Aptabase 20k, Databuddy 10k; overages ~$0.01–0.03/1k. Open-core is the OSS norm (AGPL with cloud-only funnels/journeys for Plausible, Enterprise-only funnels/retention for Countly and Matomo). Snowplow's SLULA and the OpenSnowcat fork show the license-tightening path is real and reputationally costly.

3. **Every vendor with a data model shipped an MCP server in the twelve months to Sep 2026** — PostHog (86+ tools added Aug–Sep), Amplitude (Oct 2025), Mixpanel (Apr 2026, 40+ read/write tools), Matomo, Clarity (Aug 2026), Fullstory, Pendo, Contentsquare, Datadog, Kubit, Jitsu, OpenPanel, Databuddy, Seline, Usermaven. Read-only chat is table stakes; the frontier is **write** tools (Mixpanel edits Lexicon, PostHog opens PRs, Jitsu rewires pipelines) and the governance around them: inherit-the-user's-role (PostHog, Mixpanel), Verified Mode flagging (Mixpanel), intent redaction and most-specific-rule resolution (PostHog Sep 2026), Amplitude/Statsig's "MCP governance" as a named Phase-1 deliverable. Brightflow's single action bus with undo shared by humans and agents is the right primitive; nobody in this slice advertises *undo* for agent writes — that is a differentiator worth naming.

4. **Autonomy is the 2026 pitch, framed as "self-driving" / "scouts" / "agents that watch".** PostHog Self-driving (Jun 2026), Amplitude Dashboard + Replay Agents, Databuddy's Databunny, LogRocket Galileo, Fullstory StoryAI Agents. The common shape: deterministic monitors raise a signal → LLM investigates and narrates → optional action. Brightflow's deterministic insights engine (trend/anomaly/change point after every sync) with LLM narration is the same architecture, arrived at earlier and cheaper; what it lacks is the *inbox* UX and the "failed materialisation is a signal" loop PostHog just added.

5. **Taxonomy is the layer that separates platforms from dashboards, and it has a stable feature vocabulary now:** description, owner, tags, verified, **last seen**, **30-day volume + query count**, hide / drop / merge, history log, ingestion warnings, retroactive/virtual definitions (Heap), and post-ingest transformations (Amplitude, Segment). Enforcement split two ways: dev-time codegen (PostHog CLI, Ampli, RudderTyper) vs runtime block/quarantine (Segment Protocols, RudderStack, Mixpanel drop). None of the lightweight OSS tools (Umami, Rybbit, OpenPanel, Plausible) have *any* of this — it is the single most valuable thing Brightflow can lift from PostHog/Mixpanel into a self-hosted product, and it slots directly into the existing per-table/column semantic model with provenance.

6. **"AI assistant" is now a referrer channel** — PostHog (Jul 28, 2026), Plausible (Jun 10, 2026), Matomo, Clarity Citations, Kissmetrics "LLM acquisition". Cheap to add to Brightflow's referrer classification; expected by 2026 users.

7. **Where Brightflow's "capable subset next to your other data" is ahead:** the one-catalog/one-engine story is exactly what PostHog Data Warehouse, Mixpanel Warehouse Connectors and Kubit are selling — but they achieve it by *pulling business data into the analytics vendor* (PostHog: 50+ sources/month) or *pushing analytics into the warehouse* (Kubit, Statsig WHN). Brightflow already has GitHub issues, tickets and revenue in the same Polars engine as events with no sync hop, and typed Explore chains as the query language. Ahead on architecture, behind on connector breadth and on replay/cohorts/experiments (which the market treats as bundled).

8. **Where it is behind:** (a) no saved cohorts — every product-analytics product from Umami up has them, and agents use them as nouns; (b) no taxonomy surface yet (see 5); (c) no MCP/agent surface exposed *outward* — internal action bus is stronger than most, but the market expects Claude/Cursor to reach in; (d) session replay and heatmaps are now free-tier features even in web-analytics tools (Umami, Rybbit, Contentsquare free) — a deliberate omission Brightflow should keep, but it must be argued, not assumed.

Not covered due to search-budget exhaustion (fetch-only, single primary page each, so thinner): Databuddy, Seline, Simple Analytics, Tinybird, Vercel/Netlify/Cloudflare, Clarity, Hotjar, LogRocket, Fullstory, Pendo, Smartlook, Usermaven, Woopra, Kissmetrics, mParticle, Fibotalk, Flurry, Sentry, Faro, Datadog. Ownership claims for Woopra and Kissmetrics are unverified.

---

## Pass 6: Competitor / inspiration research — semantic layers, catalogs, modelling tools, lakehouse formats, EL frameworks (as of 2026-09-14)


Method note: WebSearch budget ran out after the first ten queries, so most product facts below come from direct fetches of primary pages (GitHub READMEs, vendor docs, release pages) plus HN Algolia as a search substitute. Anything I could not confirm from a fetched source is marked (unverified).

#### 1. Semantic-layer standards and products

##### Apache Ossie (incubating) — the spec Brightflow imports/exports
- https://ossie.apache.org/ · https://github.com/apache/ossie — vendor-neutral YAML/JSON spec for exchanging semantic models across BI/AI tools; formerly "Open Semantic Interchange" (OSI).
- Status: Apache 2.0. OSI announced by Snowflake with Salesforce, dbt Labs, BlackRock, RelationalAI in Sep 2025; v0.1 spec published 29 Jan 2026; accepted into the Apache Incubator 10 Jul 2026. Released spec 0.1.1, working draft 0.2.0.dev0. Docs claim 50+ participating orgs (Databricks, Snowflake, dbt Labs, Salesforce, Collibra, Informatica, ThoughtSpot, Dremio, Starburst, Qlik…); Kyvos joined Aug 2026, Denodo Mar 2026. Honeydew calls itself a founding partner and ships a bidirectional converter.
- Shape: a document, not a service. Repo has `core-spec/spec.md`, `spec.yaml`, `ossie-schema.json`, `converters/` (dbt, GoodData, Polaris, Salesforce, Honeydew), a TPC-DS example and a Python validator.
- Semantic model (concrete field names from spec.md): top-level `version`, `semantic_model`; datasets (`name`, `source`, `description`, `primary_key`, `unique_keys`, `fields[]`, `ai_context`, `custom_extensions`); fields (`name`, `expression`, `datatype`, `label`, `description`, `dimension.is_time`, `ai_context`); metrics (`name`, `expression` with a `dialects[]` array of `{dialect, expression}` — ANSI_SQL, SNOWFLAKE, DATABRICKS, MDX, TABLEAU — plus `description`, `datatype`); relationships (`from`, `to`, `from_columns`, `to_columns`); `ai_context` = `{instructions, synonyms[], examples[]}` at every level; `custom_extensions` = `{vendor_name, data}` preserved on round-trip. **Absent**: polarity/"higher is better", KPI flag, explicit column role beyond `is_time`, and any provenance/author/source-of-definition field.
- Who writes it: code/YAML; every vendor's UI or AI writes their native form then exports.
- Adoption: dbt reads `.json` Ossie docs from an `osi/` dir (dbt ≥1.12, versions 0.1.0/0.1.1 only, datasets must resolve to dbt models); Honeydew import/export; Databricks/Snowflake announced participation but their own YAML formats (metric views, semantic views) remain the native form — no fetched Databricks/Snowflake doc mentions Ossie import/export yet (unverified whether shipped).
- Overlap with Brightflow: direct — Brightflow's import/export target.
- Worth stealing: (1) put Brightflow-only concepts (polarity, KPI flag, four-layer provenance, column roles) into `custom_extensions` with `vendor_name: brightflow` so round-trips don't lose them; (2) `ai_context.examples` per dataset/metric is the one Ossie field Brightflow's four layers do not obviously have — an "example questions" slot the LLM enrichment could fill; (3) pin to 0.1.1 and treat 0.2.0.dev0 field renames as a declaration diff, same as connector versions.
- Verdict: adjacent standard (inspiration + interop obligation).
- Sources: ossie.apache.org, github.com/apache/ossie (README, docs/index.md, core-spec/spec.md), docs.getdbt.com/docs/build/ossie-semantic-models, getdbt.com/blog/the-osi-spec-updates, honeydew.ai/docs/integration/apache-ossie-osi, snowflake.com press release 2025-09-23.

##### dbt (Semantic Layer / MetricFlow, Fusion, MCP)
- https://www.getdbt.com — SQL+YAML transformation framework; now merged with Fivetran.
- Status: Fivetran–dbt Labs all-stock merger announced 13 Oct 2025, closed 1 Jun 2026 (CEO George Fraser, President Tristan Handy). dbt Core 2.0 (Rust-based, Apache 2.0) alpha at Snowflake Summit Jun 2026, beta Aug 2026; the "dbt" Fusion distribution is proprietary/source-available on top. MetricFlow relicensed Apache 2.0 from v0.209.0 (was AGPL then BSL). 2026 release-note highlights: Semantic Layer YAML spec "new open standard" (Jan), Analyst agent beta (Jan), Developer agent (Apr/May — generates SQL, tests, docs, semantic models), Remote MCP server supports Admin API (Apr), DuckDB in dbt v2 CLI beta (Apr), dbt lint, Docs v2, dbt State, dbt Wizard (Jun), column-level tags in Catalog (Jun), AI features on by default (Sep). Kestra's write-up also lists "Agents Schema (open-source standard)" and "AI Connector Builder" launched at the merger.
- Shape: Rust CLI (Fusion), Python (dbt-core ≤1.x); state lives in `manifest.json`/`semantic_manifest.json`; warehouse-bound (DuckDB supported).
- Semantic model: YAML semantic models (entities, dimensions, measures) + metrics (simple/ratio/derived/cumulative), descriptions, meta; written by code or now by the Developer agent; no provenance layering; Ossie import as above.
- Modelling: SQL + Jinja, materialisations table/view/incremental/snapshot, DAG from `ref()`, state-aware orchestration decides rebuilds.
- Connectors: n/a (Fivetran side).
- Overlap: models-as-named-derived-tables with dependency-ordered rebuilds is the dbt idea; Brightflow does it without SQL.
- Steal: (1) "State explain" tab — per model, show why it was rebuilt/reused; (2) the Developer-agent pattern of AI proposing semantic models as a diff to accept; (3) column-level tags surfacing in the catalog.
- Verdict: inspiration (and Ossie co-target).
- Sources: docs.getdbt.com release notes, getdbt.com Fusion blog, kestra.io merger explainer, github.com/dbt-labs/metricflow.

##### Cube
- https://cube.dev — "agentic analytics platform" on an open-source semantic layer.
- Status: Cube Core Apache 2.0 (backend) / MIT (client), 20.8k stars; Cube Cloud Free / Starter $40/mo / Premium usage-based / Enterprise (PricingSaaS; enterprise "from $500k/yr" per a third-party review, unverified). Named in Gartner's 2026 Agentic Analytics Market Guide. 2026: MCP server, Claude connector and skills, Slack agent, Workbook/Dashboard agents, "agentic analytics harness".
- Shape: TypeScript/JS server + Rust (cubestore, cubesql); YAML/JS model files; not single-binary; self-host or cloud.
- Semantic model: cubes, views, measures, dimensions, joins, segments, pre-aggregations; `description` and `meta`/AI context fields; code-written (plus Cloud UI); no provenance layers; no Ossie support found on the site.
- Modelling: measures compile to SQL; pre-aggregations are the materialisation; refresh keys trigger rebuilds.
- Overlap: metrics + AI grounding on a governed model.
- Steal: the "view" concept (a curated facade over several cubes exposed to agents) — Brightflow's models could carry an "exposed to agents" flag; the MCP server tool list shape.
- Verdict: competitor in spirit, different scale (warehouse-first).
- Sources: cube.dev, github.com/cube-js/cube, pricingsaas.com/companies/cube.

##### SQLMesh (Tobiko → Fivetran → Linux Foundation)
- https://github.com/TobikoData/sqlmesh — SQL-first transformation framework with virtual data environments.
- Status: Apache 2.0, 3.3k stars. Fivetran acquired Tobiko 3 Sep 2025; donated SQLMesh to the Linux Foundation 25 Mar 2026. SQLGlot underneath.
- Shape: Python; DuckDB as first-class engine; state in a DB; CLI + UI.
- Semantic: none as such (column-level lineage, audits, unit tests). Modelling: SQL/Python models, plan/apply, incremental-by-time-range, virtual environments, fingerprint-based rebuild.
- Steal: (1) plan/apply preview of what a chain change will rebuild — Brightflow's "preview the chain first" is halfway there; (2) fingerprinting a model definition to skip rebuilds after sync when nothing upstream changed.
- Verdict: inspiration.
- Sources: github repo, fivetran.com press, thenewstack.io.

##### Malloy
- https://www.malloydata.dev — semantic modelling + query language compiling to SQL.
- Status: MIT, 2.6k stars, "a Series of LF Projects, LLC" (Linux Foundation); origin Google → Meta research. Engines: BigQuery, Snowflake, DuckDB, MotherDuck, Postgres, MySQL, Trino/Presto, Databricks. Malloy Publisher serves models over REST and MCP.
- Semantic model: `source` with dimensions, measures, joins, views; `#` annotations for metadata; every query leaves a reusable model. Code-only.
- Steal: (1) "queries extend the model" — a saved Explore chain in Brightflow already is a model; make the Explore→model promotion the default, not a menu item; (2) annotation-as-tags on fields.
- Verdict: inspiration.
- Sources: malloydata.dev, github.com/malloydata/malloy.

##### LookML (Looker)
- https://docs.cloud.google.com/looker/docs/what-is-lookml — views, explores, dimensions, measures, joins, descriptions, `value_format`, drill fields. 2025–26: Gemini writes LookML, Conversational Analytics data agents, Looker-managed MCP server. Proprietary, cloud-only. Verdict: adjacent; steal `drill_fields` (declared drill paths per measure) — cheap and useful for the insights engine.

##### Databricks Unity Catalog — metric views, AI comments, glossary
- Metric views: YAML (`version`, `source`, `filter`, `joins`, `fields[]{name, expr, comment, display_name, format, synonyms≤10}`, `measures[]{…, window}`, `materialization`). No ai_context, no polarity/KPI fields. Used by Genie, dashboards, Power BI/Tableau. AI-generated comments: LLM proposes a table/column comment, human accepts/edits (per-object, in Catalog Explorer). DAIS Jun 2026: multi-fact relationships, LOD calcs, metric materialisation, agent-driven metric authoring, Power BI/Tableau import; new **Glossary** (concepts/terms, AI-suggested, team-curated) and **Domains** (Public Preview); Unity Gateway with Contextual Service Policies for agents; OpenSharing donated to LF; geospatial GA in Delta and Iceberg v3.
- Steal: (1) accept/edit UX for AI comments is exactly Brightflow's agent layer → user layer promotion — copy the "suggested, not applied" affordance; (2) synonyms capped list per field; (3) a glossary as a fifth object type that fields link to rather than more per-column text.
- Verdict: adjacent (cloud-only); strongest UX reference for AI-written docs.
- Sources: docs.databricks.com metric-views + yaml-ref + ai-comments, databricks.com DAIS 2026 UC blog.

##### Snowflake semantic views
- Schema-level object: logical tables, relationships, facts, dimensions, metrics, synonyms, comments, Cortex hints, **verified queries**. Authored via `CREATE SEMANTIC VIEW`, YAML, Semantic Studio (visual) or Semantic View Autopilot (AI; also imports Tableau/Power BI files). Legacy stage-YAML semantic models still supported (1 MB / ~32K-token caps). Cortex Analyst reads the view's metadata to generate SQL. Steal: **verified queries** — a list of question→query pairs attached to the model that the LLM is told are correct; Brightflow's Explore chains are a natural vessel. Verdict: adjacent.
- Sources: docs.snowflake.com views-semantic/overview, atlan.com semantic-views guide.

##### AtScale / SML
- https://www.atscale.com · https://github.com/semanticdatalayer/SML — "universal semantic layer"; SML is an Apache 2.0 YAML language (catalog, package, model, dimension, metric, calculation, dataset, connection, row security, composite model) with npm SDK/CLI and converters for Snowflake Cortex, Databricks UC metrics, Power BI. 175 stars. MCP support marketed. Pricing undisclosed. Verdict: adjacent; steal the "package" idea (semantic definitions reusable across repos via git reference).

##### Rill (new in this slice; found via HN)
- https://github.com/rilldata/rill — Apache 2.0, Go + DuckDB/ClickHouse + Svelte, single binary (`curl https://rill.sh | sh`), 2.9k stars. Metrics view YAML (dimensions, measures with `expression`+`description`, time grains); "Metrics SQL" (Apr 2026) — SQL dialect over the metrics view for humans and agents; MCP server; ships `.claude`/`.mcp.json` on `rill init`. Overlap with Brightflow is the closest of anything here: local-first single binary, DuckDB-era, YAML semantics, MCP-first. Steal: Metrics SQL as the shape of a future "outward door" — a read-only, metric-aware SQL surface rather than raw SQL. Verdict: competitor.

##### Others seen on HN 2025–26
SLayer (MIT, Python, "agents evolve the semantic layer", expression queries like `revenue:sum`, memory store for context); a TypeScript semantic layer for ClickHouse with MCP; QueryWeaver (graph semantic layer on FalkorDB); AstroBee (AI-generated semantic layers). All small; signal is the direction, not the products.

#### 2. Catalogs and lakehouse table formats

##### DuckLake
- https://ducklake.select — lakehouse format storing all metadata in a SQL catalog (SQLite / PostgreSQL / DuckDB / MySQL) and data as Parquet.
- Status: announced 27 May 2025; **v1.0 on 13 Apr 2026** with backward-compat guarantee; MIT (unverified license name; repo is duckdb/ducklake). Governance by the DuckDB Foundation; spec maintained by DuckDB team. 1.0 features: data inlining (small writes stored in the catalog, default ≤10 rows), sorted tables, bucket partitioning (Murmur3), Iceberg-v3-compatible deletion vectors (Puffin), geometry + variant types with file-level stats. Non-DuckDB clients: DataFusion (Hotdata), Spark (MotherDuck), Trino (community), pandas. Ecosystem: Windmill materialises pipeline steps into DuckLake tables; Tailpipe is "building on DuckLake"; Sling writes to DuckLake; DuckLake-Guard (per-table ACL), O11yLite, frozen DuckLakes for multi-user read.
- Semantic model: none — comments only.
- Overlap: Brightflow's Litehouse (SQLite catalog + Parquet + per-file stats + partitions) is structurally a DuckLake sibling.
- Steal: (1) data inlining is the answer to Brightflow's small-event-batch problem; (2) publish Litehouse's catalog schema as a spec with a version table and a compatibility guarantee; (3) frozen-snapshot export ("frozen DuckLake") as the first outward door — a directory of Parquet + a SQLite file any DuckDB can open read-only.
- Verdict: adjacent format, the most important one to converge toward (or expose as an export).
- Sources: ducklake.select 1.0 post, docs, github.com/duckdb/ducklake, infoq, HN.

##### Apache Iceberg (+ iceberg-rust, Lakekeeper)
- Iceberg 1.11.0 is the latest docs version; v3 spec (deletion vectors, row lineage, variant, geo) GA'd across major platforms early 2026; v4 "adaptive metadata tree" proposed by Databricks, Delta 5.0 may adopt. iceberg-rust: Apache 2.0, 1.4k stars, catalogs REST/Glue/HMS/**SQL (SQLite)**/S3 Tables, `iceberg-datafusion` crate.
- **Lakekeeper** (https://github.com/lakekeeper/lakekeeper): Apache 2.0 Rust REST catalog, single binary, Postgres store; v0.12 (Apr 2026) Cedar policies + role nesting; v0.13 (30 Jun 2026) Generic Table API with **Lance** support, role-in-role, multiple OIDC providers, admission gates; v0.13.4 10 Sep 2026. Positions itself as "access control, credential vending and audit for every engine and AI agent".
- Catalog landscape Jun 2026 (Alex Merced): Polaris graduated TLP Feb 2026 (Snowflake Open Catalog = Polaris); Gravitino v1.2 (Mar 2026, "catalog of catalogs"); Nessie (git-like, no ACL); Glue/S3 Tables; Cyera bought Ryft ~$100–130M (Apr 2026).
- Steal: Lakekeeper's Generic Table API (a catalog that lists non-Iceberg tables too) mirrors Brightflow's models/builds; the credential-vending-per-agent model is the right security frame if an MCP door ever opens.
- Verdict: adjacent.

##### Delta Lake / Lance / Hudi
- Delta 4.4.0 on Spark 4.2.0; Linux Foundation; UniForm for Iceberg/Hudi readers; delta-kernel-rs being embedded (ClickHouse). Hudi: latest 1.2.0 (dates unverified), Java/Spark-first. Lance/LanceDB: Apache 2.0, Rust, 7.1k stars, Series A $30M (2025), "multimodal lakehouse for AI"; Lance Namespace catalog spec, DuckDB `lance` core extension, Lakekeeper support. Verdict: adjacent; only Lance is relevant (if Brightflow ever stores embeddings for text enrichment).

##### DuckDB core extensions (catalog-relevant)
`ducklake`, `iceberg`, `delta`, `unity_catalog`, `lance`, `postgres`, `sqlite`, `mysql`, `httpfs`, `aws`, `azure`. Note the DuckDB CLI plus these is what a user would reach for to read an exported Litehouse.

##### Postgres-side lakehouses
- **pg_lake** (Snowflake-Labs, Apache 2.0, 1.6k stars): Crunchy Data's Iceberg-in-Postgres; Crunchy acquired by Snowflake Jun 2025, open-sourced Nov 2025 (v3.0); `pgduck_server` sidecar runs DuckDB behind the wire protocol.
- **pg_duckdb** (MIT, DuckDB Labs + MotherDuck + Hydra, v1.1.1, PG14–18): reads Parquet/Iceberg/Delta from S3.
- **pg_mooncake** (MIT, 2k stars): columnstore mirrors of PG tables in Iceberg with sub-second freshness via moonlink. Databricks acquisition (unverified).
- Verdict: adjacent.

##### Engines: GlareDB, ParadeDB, Roapi, DataFusion family
- GlareDB: MIT, Rust, 1k stars, "whole new engine" rewrite Apr 2025, WebAssembly build; no cloud news found (unverified status of GlareDB Cloud).
- ParadeDB: AGPL, Rust/pgrx, $12M Series A, pg_search (BM25) + analytics; Cloud in beta.
- Roapi: Apache 2.0 Rust single binary on DataFusion; YAML-declared tables → SQL/GraphQL/REST read-only APIs; still maintained (314 commits, open PRs). **Closest existing shape to a Brightflow "outward door"**.
- DataFusion: subprojects Comet (Spark accelerator, 1.3k stars), Ballista (distributed, 2.1k, gap vs DataFusion acknowledged), datafusion-cli, Python/Java. Custom TableProvider + catalog abstractions.
- Steal: Roapi's config-first read-only API — a `--serve-readonly` mode over models.
- Verdict: adjacent.

##### Metadata catalogs
- **DataHub** (Apache 2.0, 12.7k): aspects, column-level lineage, glossary, `@acryldata/mcp-server-datahub`, Analytics Agent; DataHub Cloud (Acryl). **OpenMetadata** (Apache 2.0, 15.2k): 700+ JSON schemas, glossary with synonyms, DQ tests, MCP server, semantic search, "memory primitives" (decisions/assumptions as governed entities), RDF/OWL/PROV-O export; Collate commercial. **Atlan**: "Context Layer for AI", Context Agents generated 690K+ descriptions across 50+ customers by Apr 2026 (87% rated ≥ human), MCP server, versioned "Agent Skills", Iceberg-native, Gartner MQ leader 2025/26. **Select Star**: MCP server, reverse-engineers BI dashboards into Cortex Analyst semantic models. **Secoda**: nine named AI agents incl. Documentation and Memory agents. **Alation**: "Intelligence Operating System", Agent Studio. **Collibra**: "Enterprise AI Control Plane", ontology-based. **Marmot** (MIT, Go, single binary, 615 stars): "open-source context layer for your AI" — tables/topics/queues/APIs with lineage. **Xorq** (Apache 2.0, Python/Ibis/DataFusion, 551 stars): git-native, content-addressed "executable catalog" with a Claude Code plugin. **Neuralink datarepo**: Python-declared catalogs over Parquet/Delta generating static sites + read-only APIs.
- Steal: OpenMetadata's PROV-O export is a ready vocabulary for Brightflow's per-row provenance; Atlan's "bootstrap → collaborate/certify → activate" pipeline names the four layers Brightflow already has; Xorq's content-addressing for model versions.
- Verdict: adjacent (enterprise) / Marmot inspiration.

##### Data quality
- Soda Core v4 (YAML data contracts, DuckDB supported, 2.4k stars); Great Expectations (Apache 2.0 GX Core, 11.8k; company status 2026 unclear — one fetch claimed Fivetran maintains it, unverified and probably wrong); Elementary (Apache 2.0, dbt-native, Cloud adds AI reliability agents). Steal: Bruin/Soda-style `checks:` on a column declaration so connectors can ship quality expectations alongside semantics. Verdict: adjacent.

#### 3. Extract-and-load frameworks and orchestrators

##### dlt (dltHub)
- https://github.com/dlt-hub/dlt — Apache 2.0 Python EL library, 5.9k stars; sources via decorators or declarative REST API config; schema inference + contracts (evolve/freeze/discard); incremental cursors; destinations incl. DuckDB, filesystem Parquet, Iceberg, Delta; dlt+ commercial (quality, transforms, AI); MCP server; explicitly optimised for coding agents ("dlthub.com/context"). Latest 1.30.0 with cross-destination joins (date unverified).
- Connectors do not declare semantics beyond schema/PK/cursor. Steal: the REST-API declarative source spec (auth, pagination, incremental cursor as data) is the closest published analogue to Longbow; their `llms.txt`-style context for agents writing connectors.
- Verdict: both (a Longbow alternative in Python; an inspiration for agent-authored connectors).

##### Sling
- https://github.com/slingdata-io/sling-cli — GPL-3.0, Go single binary, 901 stars; YAML replications; 34+ DB connectors, Parquet/CSV/etc., lakes incl. Iceberg, **DuckLake**, S3 Tables; modes full-refresh/incremental/truncate/snapshot/backfill/change-capture; PK+update-key incremental. No semantics. Verdict: adjacent; steal the six named load modes as vocabulary for Longbow tables.

##### Bruin
- https://github.com/bruin-data/bruin — Apache 2.0, Go single binary, 1.7k stars; ingestion (ingestr) + SQL/Python/R transforms + quality in one CLI; assets as YAML/SQL headers with **columns**: `name, type, description, primary_key, checks, tags, owner, domains, meta`, foreign keys to other assets, `mask` rules on ingest. Steal: exactly this — column metadata declared at the asset (connector) level feeding lineage and quality. Verdict: competitor-adjacent (nearest "single binary, connectors declare columns" tool).

##### Airbyte
- MIT + ELv2, 22.1k stars, 600+ connectors; no-code Builder, low-code YAML CDK, Python CDK; PyAirbyte; Agent SDK for LLM access; Cloud Standard from $10/mo (volume), Pro (Data Worker capacity), Enterprise Flex. Connectors declare schema only. Verdict: adjacent/competitor on ingestion breadth.

##### Meltano / Singer
- MIT, 2.6k stars, "open-source project by Matatika" (so no longer the Meltano Inc./Arch team); Hub of Singer taps/targets; Meltano Cloud. Verdict: adjacent; low momentum.

##### CloudQuery
- MPL-2.0 core with some code moved closed; Go plugins, Arrow transport; priced by rows/year; positioned as cloud asset inventory for security/FinOps. Verdict: adjacent.

##### Steampipe / Tailpipe / Powerpipe (Turbot)
- Steampipe AGPL, Go, Postgres FDW, 2,000+ tables, v0.22; `steampipe-mcp` (Apache 2.0) exposes query/table_list/table_show/plugin tools. Tailpipe: "open source SIEM" collecting logs to DuckDB + hive-partitioned Parquet, with a "next evolution: building on DuckLake" post (date unverified). Plugin tables in Go declare columns with descriptions (docs per table, unverified detail). Steal: the MCP tool set shape (`table_list`, `table_show`) is the minimal outward door; Tailpipe's move to DuckLake validates SQLite/Postgres-catalog + Parquet for a single-binary Go/Rust tool. Verdict: inspiration.

##### Estuary Flow
- Rust + Go on Gazette; Apache 2.0 + BSL; 975 stars; 200+ connectors incl. Airbyte adapter; CDC-first; Iceberg destinations; "runtime rewritten" 2026 banner; pricing undisclosed. Verdict: adjacent.

##### OLake (new, HN)
- Apache 2.0 Go; DB/Kafka/S3 → Iceberg (Glue/Hive/JDBC/REST incl. Lakekeeper, Polaris, Unity) or Parquet; "no Spark/Flink/Debezium"; 1.4k stars. Verdict: adjacent.

##### Orchestrators
- **Windmill**: AGPL/Apache + EE, Rust backend, 17.9k stars; canonical data step is a native DuckDB script against workspace S3; new alpha **Pipelines** model wires scripts by asset lineage and materialises DuckDB steps into **DuckLake tables** — strikingly close to Brightflow models.
- **Dagster**: Apache 2.0, 16.2k; Components + `dg` CLI; Compass (AI); Dagster+ Solo $10/mo, Starter $100/mo, credits per materialisation.
- **Kestra**: Apache 2.0 Java, 28.1k, YAML flows, AI Copilot, AI agents as task types, built-in MCP server. **Mage**: Apache 2.0, 8.8k, Mage Pro adds AI. **Prefect**: Apache 2.0, 23.8k, `@materialize` assets with description/owners/URL lineage.
- Steal: Windmill's asset-lineage auto-wiring; Dagster's per-materialisation credit as a mental model for showing rebuild cost.
- Verdict: adjacent.

##### Platforms (briefly)
Tinybird (managed ClickHouse, `.datasource`/`.pipe` files, MCP, coding-agent workflow); Y42 (active, "turnkey data orchestration", no AI messaging); Keboola (378+ connectors, pivoted to finance-team AI, has a Business Glossary with formula/source/owner per metric); Datacoves (managed dbt Core + Airflow, in-VPC AI copilot); Paradime ("Cursor for Data", DinoAI agent); Coalesce (Transform + Catalog [ex-CastorDoc] + Quality, Scout agent, MCP). Verdict: adjacent, warehouse-centric.

#### Patterns across this slice

- **An open semantic interchange exists now, and it is thin.** Ossie 0.1.1 carries datasets/fields/metrics/relationships/`ai_context`, and nothing about polarity, KPI status, column role beyond `is_time`, or who wrote a definition. Brightflow's four provenance layers and polarity are ahead of the standard; the practical move is `custom_extensions` now and a proposal to the 0.2.0 draft later.
- **Vendors adopted the same three extras everywhere: descriptions, synonyms, and "AI context / verified queries."** Databricks (synonyms ≤10, comment), Snowflake (synonyms, Cortex hints, verified queries), Ossie (`ai_context.examples`), Cube (meta), Rill (descriptions). Brightflow lacks the synonyms and example-questions slots; both are cheap and read directly by an LLM.
- **AI-written metadata converged on "suggest, review, accept."** Databricks AI comments, Atlan Context Agents (690K descriptions, curated), dbt Developer agent, Secoda Documentation agent, Snowflake Autopilot. Brightflow's agent layer sitting below the user layer with per-row provenance is the more principled version of what these ship as a modal dialog; nobody surfaces declaration diffs across connector versions.
- **Connector-declared semantics is rare.** Bruin is the only EL tool fetched where the ingestion asset declares column descriptions, PKs, owners, checks and foreign keys; dlt/Sling/Airbyte/OLake stop at schema + cursor. Longbow's polarity/KPI/role declarations are ahead of the field.
- **SQL-catalog lakehouses won the small-tool tier.** DuckLake 1.0 (SQLite/Postgres catalog + Parquet), Tailpipe moving onto it, Windmill materialising into it, Sling writing it, iceberg-rust shipping a SQLite catalog. Litehouse is a private DuckLake; the cheapest credibility win is a read-only export/snapshot any DuckDB can open, or adopting DuckLake's metadata tables outright.
- **MCP is the default outward door for 2026.** dbt Remote MCP, Cube, Malloy Publisher, Rill, DataHub, OpenMetadata, Atlan, Select Star, Steampipe, Kestra, Looker, Lakekeeper-for-agents. Brightflow's "no outward door" is now the exception; Steampipe's five-tool set and Rill's Metrics SQL show two minimal, safe shapes.
- **Typed operation chains instead of SQL are still unusual.** Malloy (language), SLayer (expression strings), Rill Metrics SQL, Windmill Pipelines and Xorq (Ibis expressions) are the peers; every other tool models in SQL/YAML. Brightflow's chain is ahead on safety and undo, behind on joins and incremental materialisation (SQLMesh, dbt, DuckLake inlining all have both).
- **Fingerprint-driven rebuilds are table stakes.** SQLMesh plans, dbt State's "explain" tab, Dagster credits per materialisation. Brightflow's rebuild-everything-after-every-sync is behind; the models table already has versions and builds, which is the hook.
- **Consolidation: Fivetran now owns dbt, SQLMesh (via LF) and (unverified) Great Expectations; Snowflake owns Crunchy/pg_lake and steers Polaris + Ossie; Databricks steers Delta/Iceberg v4 and Unity.** Independent small tools (Rill, Bruin, Sling, Marmot, Lakekeeper, Windmill) are the real peer set for a single-binary Rust workbench, and none of them combines connector-declared semantics, provenance layers, and a deterministic insights engine.
- **Catalog-as-context-layer is the enterprise framing.** Atlan, Collibra, Alation, OpenMetadata ("memory primitives"), Marmot all rebranded around agent context in 2026. Brightflow's shared action bus with undo for humans and agents is a differentiator none of the fetched pages describe; worth naming loudly.

Sources (primary, used above): ossie.apache.org; github.com/apache/ossie; docs.getdbt.com (ossie-semantic-models, release-notes); getdbt.com/blog (osi-spec-updates, fusion); kestra.io merger explainer; cube.dev, github.com/cube-js/cube, pricingsaas.com; github.com/TobikoData/sqlmesh; fivetran.com press; malloydata.dev, github.com/malloydata/malloy; docs.databricks.com (metric-views, yaml-ref, ai-comments), databricks.com DAIS 2026 UC blog; docs.snowflake.com views-semantic; atscale.com, github.com/semanticdatalayer/SML; docs.cloud.google.com/looker; ducklake.select (1.0 post), github.com/duckdb/ducklake; lakekeeper releases; dev.to Iceberg catalogs June 2026; github.com/apache/iceberg-rust; delta.io; lancedb.com, github.com/lancedb/lance; hudi.apache.org; glaredb.com (+blog, GitHub); paradedb.com; github.com/roapi/roapi; datafusion.apache.org, Comet, Ballista repos; github.com/Snowflake-Labs/pg_lake, duckdb/pg_duckdb, Mooncake-Labs/pg_mooncake; duckdb.org core extensions; tinybird.co; y42.com; keboola.com; datacoves.com; paradime.io; coalesce.io; github.com/datahub-project/datahub, open-metadata/OpenMetadata; atlan.com; selectstar.com; secoda.co; alation.com; collibra.com; github.com/sodadata/soda-core, great-expectations, elementary-data/elementary, greatexpectations.io; github.com/dlt-hub/dlt (+releases); github.com/slingdata-io/sling-cli, docs.slingdata.io modes; github.com/bruin-data/bruin, Bruin columns docs; github.com/airbytehq/airbyte, airbyte.com/pricing; github.com/meltano/meltano, meltano.com; github.com/cloudquery/cloudquery, cloudquery.io/pricing; tailpipe.io, github.com/turbot/steampipe, turbot/steampipe-mcp; estuary.dev, github.com/estuary/flow; github.com/windmill-labs/windmill, windmill.dev data_pipelines docs; github.com/dagster-io/dagster, dagster.io/pricing; github.com/kestra-io/kestra; github.com/mage-ai/mage-ai; github.com/PrefectHQ/prefect, docs.prefect.io assets; github.com/rilldata/rill; github.com/MotleyAI/slayer, xorq-labs/xorq, marmotdata/marmot, neuralinkcorp/neuralake, AltimateAI/altimate-code, datazip-inc/olake; hn.algolia.com queries (single binary data pipeline, semantic layer, DuckLake, data catalog open source, Fivetran alternative, rust data catalog).

---

## Pass 7: Brightflow competitive slice: all-in-one small-team data stacks, spreadsheet/canvas hybrids, SMB dashboards, DuckDB-desktop tools, and agent-era UX inspirations (as of 2026-09-14)


Method note: the session's WebSearch budget was exhausted before this task began, so every claim below comes from primary pages fetched directly (homepages, pricing pages, changelogs, docs, GitHub/API) plus the HN Algolia API as a discovery substitute. Anything I could not reach is marked (unverified).

---

#### HALF 1 — All-in-one stacks, spreadsheet hybrids, SMB dashboards, DuckDB-desktop

##### All-in-one data stacks for small teams

**Definite** — https://www.definite.app — ETL + lakehouse + semantic layer + BI + AI agent ("Fi"), deployed into the customer's own cloud.
- Status: proprietary. Free (2 users, 2 connectors, 5 credits), Standard $250/mo unlimited users (100 credits), Enterprise. 1 credit = 1 compute-hour or 100K AI tokens; $1/credit overage. Uses DuckDB-Wasm in-browser for sub-second queries; MCP server exposing schema, lineage and permissions; "inspector" that shows the SQL and lineage behind anything Fi builds.
- Overlap: the closest commercial analogue to Brightflow's thesis (one binary/one price, connectors + storage + semantic layer + insights + agent). Differs in being cloud-deployed and enterprise-priced.
- Steal: (1) the "inspector" panel — every agent-built artefact exposes its query + lineage one click away; (2) one flat price, unlimited users, credits only for compute/AI — matches Brightflow's "few people, one workspace" shape; (3) "agent time" as a first-class metered resource.
- Verdict: competitor (upmarket).
- Sources: definite.app, definite.app/pricing.

**Y42** — https://www.y42.com — git-backed pipeline/orchestration platform (dbt/SQL/Python assets, DAG UI).
- Status: proprietary; Business from $500/mo (3 users, 2 spaces), Enterprise; ingestion add-on from $15/M rows. No AI claims on homepage.
- Overlap: low — it is a data-engineering tool, not a workbench.
- Steal: git-style branches and "data time travel" as the mental model for saved views/models.
- Verdict: adjacent.
- Sources: y42.com, y42.com/pricing.

**Keboola** — https://www.keboola.com — governed ETL/ELT platform, now positioned as "AI-ready data foundation for finance."
- Status: proprietary; free tier (60 compute-min/mo after month one, $0.14/min overage), Enterprise custom. Ships a Keboola MCP Server on the free tier; "AI agents for audit & controlling" that flag anomalies before month-end.
- Overlap: the "agents run after sync and flag things" idea, but enterprise-finance flavoured.
- Steal: business glossary with enforced metric definitions + owners — a semantic layer with accountability, which maps to Brightflow's provenance-carrying semantic model.
- Verdict: adjacent.
- Sources: keboola.com, keboola.com/pricing.

**Mozart Data** — https://www.mozartdata.com — YC-backed managed Snowflake + Fivetran-style connectors + dbt + catalog.
- Status: active; free "Sonata" (250K monthly active rows), Concerto $1,200/mo, Symphony $3,000, Opera $6,000; $1,000 implementation fee; "Mozart AI" recently launched. No shutdown/acquisition seen.
- Overlap: same "modern data stack in a box" pitch, but Snowflake-priced.
- Steal: bundled analyst hours as an upsell — for Brightflow, the equivalent is the LLM narrating insights.
- Verdict: adjacent (too heavy for indie).
- Sources: mozartdata.com, mozartdata.com/pricing.

**Peliqan** — https://peliqan.io — 300+ connectors → built-in warehouse (Trino) → spreadsheet UI, Streamlit apps, reverse ETL, MCP.
- Status: proprietary; from €75/mo per data source. Positions as "trust layer for AI & BI"; MCP server with controlled write-back actions.
- Overlap: high on scope (connectors + warehouse + spreadsheet UI + AI actions); differs on price model (per source) and hosting.
- Steal: "secured read + controlled write-back actions" framing for the action bus — actions are the unit of permission, not tables.
- Verdict: competitor (SMB Europe).
- Sources: peliqan.io.

**Airbook** — https://www.airbook.io — GTM data platform: warehouse + BI + workflows for RevOps.
- Status: proprietary; Free (10 AI requests), Starter $99, Growth $599, Scale $1,499, Enterprise. Airbook 2.0 on Product Hunt; Show HN posts Jan/Sep 2025.
- Overlap: "10 products in one" for small GTM teams; AI requests metered per plan.
- Steal: nothing structural; note its explicit per-plan AI request quota as the norm.
- Verdict: competitor (RevOps niche).
- Sources: airbook.io, airbook.io/pricing.

##### Spreadsheet / canvas hybrids

**Rows** — https://rows.com — AI spreadsheet with 50+ integrations.
- Status: "Rows joined Superhuman" banner on rows.com and docs (date/terms unverified; Superhuman is the Grammarly umbrella brand — unverified). Pricing still live: Free, Plus $8/user, Pro $79 + $8/user. AI metered as "AI Tasks" (5/200/1,000 per month) plus separate "cell enrichment tasks" (500 / 1M). Changelog: AI Analyst 6.4 with web search/URL scraping into tables (Jan 9 2026), thumbs feedback (Jan 16 2026).
- Overlap: LLM-driven enrichment of cells with quota — a direct cousin of Brightflow's text enrichment.
- Steal: (1) separating "AI tasks" (LLM calls) from "enrichment tasks" (integration fetches) in metering; (2) thumbs up/down on every AI result feeding a report channel.
- Verdict: inspiration (product now absorbed).
- Sources: rows.com, rows.com/pricing, rows.com/docs/whats-new-changelog.

**Equals** — https://equals.com — managed warehouse + spreadsheet calc layer + AI, for SaaS metrics.
- Status: proprietary; $24K / $36K / $60K per year, unlimited seats, AI spend bundled ($250/$500/$1,000 per month), forward-deployed analyst. "Click any figure to see the formula and source query."
- Overlap: same audience (Series A–C SaaS metrics) but 10–50× the price.
- Steal: bundle "AI spend" as a monthly allowance; universal "click a number → see its query" provenance.
- Verdict: competitor (upmarket).
- Sources: equals.com, equals.com/pricing.

**Sourcetable** — https://sourcetable.com — AI spreadsheet with Python/SQL toolkit and 40+ connectors.
- Status: proprietary; Free (50 credits), Pro $29/user (2,000), Max $100/user (6,000); extra credits $0.015–0.02. Cites "$43M in new funding" (Forbes). "Superagents" (spreadsheets connected to any DB/API/MCP) on Show HN Sep 9 2025 (35 pts); another Show HN Oct 30 2025.
- Overlap: AI-in-cells; credit-priced.
- Steal: publishing exact per-credit prices removes the "how much will this cost" anxiety — Brightflow should show token cost estimates before enrichment runs.
- Verdict: adjacent.
- Sources: sourcetable.com, sourcetable.com/pricing, HN Algolia.

**Quadratic** — https://www.quadratichq.com — Rust/WASM spreadsheet with Python/SQL/JS cells and AI.
- Status: Pro $18/user/mo (+$20 AI credits), Business $36 (+$40), Enterprise with self-host. Org shows `quadratic-selfhost`, `quadratic-mcp-plugin` (MIT); the main `quadratic` repo returned 404 — possibly made private (unverified). 2025–26 releases: Agent Sync, MCP, API-triggered AI runs, scheduled tasks, collaborative AI chats, 100M-row support.
- Overlap: "typed cells compiled to code" is a spreadsheet cousin of Brightflow's typed operation chain.
- Steal: AI credits denominated in dollars ("$20 in AI credits") rather than opaque units.
- Verdict: adjacent.
- Sources: quadratichq.com, quadratichq.com/pricing, github.com/quadratichq, GitHub search API.

**Causal** — https://causal.app → redirects to Lucanet xP&A. The Causal brand is gone from the page; Lucanet markets a "Modeler Agent." Verdict: gone (absorbed). Source: lucanet.com/en/solutions/extended-planning-and-analysis/.

**Row Zero** — https://rowzero.com — "billion-row spreadsheet" with warehouse connectors and an "auditable, editable" AI assistant; Show HN Mar 13 2026. Verdict: adjacent. Steal: "fully auditable and editable" AI output as the headline promise.

##### Database GUIs / query tools with AI

**Outerbase** — https://outerbase.com — DB interface with EZQL AI; "joined Cloudflare." Outerbase Studio and Starbase are OSS. Verdict: adjacent. Source: outerbase.com.

**Basedash** — https://www.basedash.com — AI-native BI with warehouse, models, automations, MCP.
- Status: moved upmarket: Startup $1,000/mo + AI usage (25 users, $1,000 AI credits), Enterprise; self-hosting available; daily AI briefings; "Models" from natural language; ranks itself #1 on "BI Bench."
- Overlap: strong feature overlap (models, insights, MCP, self-host); price now excludes indies.
- Steal: "daily briefing" as the default insight surface; NL-authored models with measures/dimensions/segments.
- Verdict: competitor (vacated the low end — opportunity).
- Sources: basedash.com, basedash.com/pricing.

**Trevor.io** — https://trevor.io — no-code query builder + dashboards, free tier with unlimited users; the team launched **Embeddable** (code-first embedded analytics: YAML/TSX in repo, Cube semantic layer, web components, RLS). Verdict: adjacent (Trevor) / adjacent-brief (Embeddable). Steal from Embeddable: dashboards as reviewable files in a repo. Sources: trevor.io, embeddable.com.

**Lightdash** — https://www.lightdash.com — OSS "agentic BI" on dbt.
- Status: OSS self-host free; Cloud Pro $3,000/mo unlimited users; AI agents an add-on: Slack agents, MCP (Claude/ChatGPT/Cursor/Codex), scheduled agent answers, evals, verified answers, "AI writeback"/coding agents (beta), "self-improving semantic layer." 6.1k stars; v2.211.1 on 2026-09-14.
- Overlap: semantic-layer-grounded agents; scheduled agent runs delivered to Slack/email.
- Steal: (1) verified answers + eval suites as governance for the narrator; (2) agents proposing semantic-layer changes as reviewable diffs.
- Verdict: both (OSS competitor for dbt shops; inspiration on agent governance).
- Sources: lightdash.com, lightdash.com/pricing, docs.lightdash.com/guides/ai-agents, GitHub API.

**Steep** — https://steep.app — metrics-first BI; Pro $15/seat (free ≤3 seats, 80 metrics), Business $25; Steep Agent free "within reasonable limits" for now, credit pricing coming; MCP/API. Verdict: competitor (closest in price to indie). Steal: metrics (not dashboards) as the atomic object. Sources: steep.app, steep.app/pricing.

##### SMB metrics dashboards

**Baremetrics** — https://baremetrics.com — subscription analytics. Launch $75/mo, Growth $255, Scale $1,152 (annual ~35% off); add-ons $129 each. No AI; latest blog Oct 2023. Ownership by Xenon Partners (unverified). Verdict: competitor (SaaS-metrics vertical), stagnant. Sources: baremetrics.com, /pricing.

**ChartMogul** — https://chartmogul.com — "AI-native" subscription analytics + built-in CRM. Free under $120K ARR; Pro $1,188–$14,388/yr; Enterprise from $19,900/yr. Verdict: competitor (vertical). Steal: "generic AI on messy billing data gives confident nonsense" — the argument for a curated semantic layer before narration. Sources: chartmogul.com, /pricing.

**ProfitWell Metrics (Paddle)** — https://www.paddle.com/profitwell-metrics — still free, maintained, tighter Paddle integration. Verdict: competitor (free floor for SaaS metrics). Source: paddle.com.

**Databox** — https://databox.com — "agentic analytics": Genie AI analyst, agents & automations, semantic layer, MCP, Skills marketplace. Free (50 AI credits), Analyst $71, Team Core $199, Team Scale $319, Agency $79 + client packs. Verdict: competitor. Steal: skills marketplace pattern for connector/insight recipes. Sources: databox.com, /pricing.

**Geckoboard** — https://www.geckoboard.com — TV/KPI dashboards; $79 / $319 / Enterprise; "Metrics for AI (MCP)." Verdict: adjacent. Source: geckoboard.com, /pricing.

**Klipfolio** — https://www.klipfolio.com — Klips from $120/mo; PowerMetrics "powered by AI"; claims 646 updates/yr. Verdict: adjacent. Source: klipfolio.com.

**Grow (Epicor)** — https://www.growdataplatform.ai — no-code full-stack BI, unlimited users, no AI on page. Verdict: adjacent. Source: growdataplatform.ai.

**Panintelligence** — https://www.panintelligence.com — embedded + internal BI, PiPredict, custom pricing. Verdict: adjacent. **Whaly** — https://whaly.io — pivoted to sales/revenue intelligence with "copilot builder." Verdict: adjacent. **Polymer** — https://www.polymersearch.com — AI embedded analytics, API from $500/mo. Verdict: adjacent. **Akkio** — https://www.akkio.com — now agency-focused media AI. Verdict: not a competitor. **Obviously AI** — now Zams; site is an archive "through 2025." Verdict: gone. **Explo** — https://www.explo.co — acquired by Omni (Oct 22 2025; 12-month transition). Verdict: gone into Omni. **Luzmo** — https://www.luzmo.com — embedded; Luzmo IQ conversation/summary/developer agents; Embedded MCP Server; "Context for embedded AI" (Jul 2026). Verdict: adjacent. **Chartbrew** — https://chartbrew.com — OSS (FSL dual-licence), 4.1k stars, Node/React, v5.3.1 (2026-07-31) added a "Dataset Intelligence model" for its AI orchestrator. Verdict: competitor (OSS self-host dashboards). **Draxlr** — https://www.draxlr.com — no-code BI over SQL DBs with MCP. Verdict: adjacent.
- Sources: respective homepages; omni.co/blog/omni-acquires-explo; GitHub API (chartbrew).

**Omni (for context)** — Routines: scheduled prompt → semantic-layer-governed analysis pushed to email/Slack (Jul 16 2026); "Run your agent like a data product with AI Evals" (Jul 7 2026); agentic harness benchmark (Jul 28 2026). Steal: "agent can't invent a definition, can't return anything the user couldn't query themselves" as the safety argument for narration. Sources: omni.co/blog.

##### DuckDB-desktop / local-first tools

**DuckDB UI** — https://duckdb.org/2025/03/12/duckdb-ui.html — browser UI for local DuckDB (`duckdb -ui`), built by MotherDuck; notebooks, column explorer, table summaries; fully local unless you opt into MotherDuck. Extension repo MIT, but "most of the user interface code is not yet publicly available." Instant SQL (results update as you type; public preview in both DuckDB Local UI and MotherDuck; date unverified). Verdict: inspiration + adjacent. Steal: column explorer profiles on every result; Instant SQL's run-as-you-type feel maps to Brightflow's sub-100ms chain preview. Sources: duckdb.org, github.com/duckdb/duckdb-ui, motherduck.com/blog/introducing-instant-sql.

**Tad** — https://www.tadviewer.com — MIT pivot-table viewer on DuckDB; last release v0.14.0 (2024-06-21). Dormant. Verdict: adjacent. **VisiData** — GPLv3 terminal spreadsheet, v3.4 (2026-07-01), 9.3k stars; keyboard-only model with hundreds of commands. Steal: every action is a keystroke and a replayable command log (.vd files) — a spiritual cousin of Brightflow's typed operation chain. **Datasette** — Apache-2.0, 11.5k stars, still 0.65.x (0.65.4 on 2026-09-11; 1.0 not yet final). Steal: every view is a URL and a JSON API. **Beekeeper Studio** — GPLv3 + commercial, 23.6k stars, v6.0.5 (2026-08-24); no AI in README (an "AI Shell" in Ultimate is unverified). **TablePlus** — no AI in product; sibling "NotePlus" LLM client. **DBeaver AI** — page 404; (unverified). **DataGrip** — 2026.1 (Mar 26 2026) "AI agents in the AI chat"; 2026.2 (Jul 16 2026) AI agent skills, MCP tools, CLI for data sources; Aug 26 2026 post on Claude Code/Codex/Junie inside DataGrip. Steal: shipping "skills" for external coding agents alongside the GUI. **DataZen** — https://flyxl.github.io/datazen/ — GPLv3 desktop DB workspace under 15 MB, NL→SQL, MCP server *and* client, local-first keychain creds (Show HN Aug 29 2026). **Duckle** — github.com/SouravRoy-ETL/duckle — OSS (Apache/MIT) Rust + DuckDB ETL/ELT studio, 385+ connectors, dbt, CDC, MCP, 1.3k stars (May 2026). Verdict: competitor on the ingestion half. **ColumnLens** — Mac local query app (Show HN Mar 2026; site cert error, details unverified). **Bauplan** — https://docs.bauplanlabs.com — Iceberg lakehouse with git-for-data branches, Rust CLI/SDK, MCP server + agent skills: "data infrastructure where agents build safely on production data." Verdict: inspiration. Steal: branch → agent works → human merges as the undo/approval model for data. **Ryft** — Iceberg table-maintenance vendor, acquired by Cyera (Apr 2026); not relevant. **Kestrel** — could not identify a data tool by that name; HN only surfaces **Kestra** (orchestrator) (unverified). **Rill** and **Evidence** — Rill's site returned 403 (unverified; DuckDB-based BI shipped as a single binary from memory); Evidence: BI-as-code, OSS core (6.8k stars), Analytics Agent with Slack/MCP and "skills." Verdict: both competitors for the code-first crowd.
- Sources: respective sites/GitHub API; HN Algolia.

##### 2025–2026 entrants worth knowing (from HN discovery)
- **BitBoard (YC)** — "analytics workspace for agents"; Ask (beta) + dashboards; agents use "your team's definitions, trusted queries, and corrections" — accurate/traceable/repeatable (Launch HN Jun 12 2026, 58 pts). Competitor.
- **Nao Labs (YC X25)** — "Cursor for data"/"analytics agent built for context engineering"; OSS, BYO API key, context as a file tree (DBs, docs, queries, definitions), unit tests from question→SQL; self-host (Launch HN May 9 2025, 158 pts). Both — strongest new inspiration for the semantic model as agent context.
- **ktx (YC P25)** — Apache-2.0 "executable context layer" for Claude Code/Codex over warehouses; 1.6k stars (May 2026).
- **DAC (Bruin)** — AGPL "dashboard-as-code for agents and humans": YAML/TSX + semantic layer, ships Claude/Codex skills; 730 stars (Apr 2026, 119 pts).
- **Livedocs (YC W22)** — AI-native notebook (Feb 2026). **Shadowfax AI** — auditable graph-based analyses for analysts (public beta). **Eatmydata.ai** — "local-first question-to-SQL-to-dashboard" (Jun 2026; page empty, unverified). **Anypanel** — founder "one dashboard instead of five" (Jan 2026; 403, unverified).
- Sources: hn.algolia.com API; bitboard.work; getnao.io; github.com/Kaelio/ktx; github.com/bruin-data/dac; livedocs.com; shadowfax.ai.

---

#### HALF 2 — Interaction-model inspirations outside analytics

**Linear** — https://linear.app — issue tracker; the reference for command-bar + keyboard UX and, since 2025, for agents-as-teammates.
- 2025–26: agent framework with Agent Sessions and typed activities (thoughts, actions, elicitation, responses); delegation makes the agent a *delegate*, humans stay assignee. Integrated agents: Cursor, Codex, Devin, Sentry, ChatPRD, Oz by Warp, Factory, Charlie, Ranger, Tembo. Changelog: Coding Sessions via Claude Code/Codex on AI credits (Jun 11 2026); Loops — recurring agent workflows (Jul 20 2026; enhanced Sep 14 2026 with "Continue any run"); agent-assisted editing with author indicators so you can tell "if text was written by a colleague or added by a loop" (Jul 23 2026); Cmd+Z restores closed tabs (Jul 30 2026); enterprise MCP auth (Aug 13 2026). "Linear for Agents" initial launch date (unverified; 2025).
- Steal: (1) the four-activity vocabulary for agent entries in the activity feed; (2) "delegate, not assignee" — humans own, agents contribute; (3) author indicators on agent-written text; (4) "continue any run" — resume a finished agent run in place.
- Verdict: inspiration (strongest for (a) one feed, humans + agents with provenance).
- Sources: linear.app/developers/agents, linear.app/agents, linear.app/changelog.

**Raycast** — v2.3 rebuilt AI core as agentic (plans, uses extensions, writes/executes code) on usage-based credits; BYO models (v2.2); Windows and iOS; Pro $10/Plus $20/Max $50 (500/3,000/7,500 credits). Steal: launcher-as-command-bar with AI extensions as tools; credits per tier. Verdict: inspiration. Sources: raycast.com/changelog, /pricing.

**Superhuman** — domain blocked; only fact verified is that Rows joined it. (unverified beyond that.)

**Warp** — pivoted to "Warp Factories" (Aug 18 2026): open infrastructure for agent fleets wired to the SDLC; Warp Agent CLI (Aug 4 2026); "Oz by Warp" is a Linear-integrated agent; usage-based per agent run; agents capture screenshot/video for human verification before PR. Steal: verification artefact attached to every agent action. Verdict: inspiration. Sources: warp.dev, warp.dev/blog.

**Cursor** — checkpoints auto-created before significant changes, restore files only (conversation kept), local and separate from git; Plan Mode (Shift+Tab) asks clarifying questions and produces a reviewable plan before acting; Sep 2026 "Cursor Projects" (months of context, thousands of subagents, recurring work), self-hosted machines; Aug 2026 cloud agents, `/goal`, Origin code hosting. Steal: file-only restore vs conversation restore as separate undo axes. Verdict: inspiration. Sources: cursor.com/changelog, cursor.com/docs.

**Claude Code** — checkpoint per prompt (100 kept), `/rewind` offers restore code / conversation / both / summarize; bash-made changes and subagent edits not tracked; tiered permissions (read-only never asks; bash asks except a read-only set; "don't ask again" scoped per repo+command; auto mode with a classifier; plan mode); surfaces: terminal, IDE, desktop, web, Slack, Routines (cloud schedules), Remote Control, Channels. Steal: (1) undo axes split (state vs. conversation); (2) permission tiers by tool class with per-scope "don't ask again"; (3) explicit documentation of what undo does *not* cover. Verdict: inspiration (b: undo over approval). Sources: code.claude.com/docs/en/checkpointing, /permissions, /overview.

**Devin** — docs: autonomous engineer with Shell/IDE/Browser, follow work in an embedded IDE in real time; Slack/Teams entry; Linear-integrated. Pricing page rate-limited (unverified). Verdict: adjacent. Source: docs.devin.ai.

**GitHub Copilot coding agent** — assign an issue or @copilot in a PR; works on a branch, every step in commits and viewable logs; draft PR you review; costs = Actions minutes + AI credits/premium requests. Steal: "every step is a commit" as the audit trail. Verdict: inspiration. Source: docs.github.com.

**Notion** — Notion Agent + Custom Agents (scheduled/triggered); credits from May 4 2026; agents "propose changes via suggest edits" so you approve individual modifications (Aug 28 2026); model picker with speed/intelligence/cost scorecards (Aug 14 2026); workers usage in credits dashboard (Jul 24 2026); per-agent model controls (Sep 9 2026). Charts: five types, one free chart, drilldown to rows. Steal: agent output as *suggestions* you accept per change; cost scorecards on model choice. Verdict: inspiration + adjacent (databases/charts). Sources: notion.com/product/ai, notion.com/releases, notion.com/help/charts.

**Figma** — agent chat panel as separate window (Aug 26 2026); prompt-built generative plugins (Sep 1 2026); MCP enterprise auth (Aug 24 2026). Verdict: adjacent. Source: figma.com/release-notes.

**Obsidian** — 1.14.0 (Sep 2 2026): Bases gains kanban; 1.13 settings redesign with keyboard nav; explicitly no AI; local files. Steal: Bases = database views over plain files (Brightflow: views over plain Parquet). Verdict: inspiration (local-first). Sources: obsidian.md/changelog.

**Arc / Dia** — Atlassian acquired The Browser Company (Sep 4 2025); Dia in beta on macOS with Morning Brief, cross-tool synthesis; Arc's future unstated. Verdict: adjacent. Sources: atlassian.com blog, diabrowser.com.

**Height** — site unreachable in three attempts; status (unverified).

**Attio** — CRM: Ask Attio (Feb 4 2026) → can take actions (Apr 29 2026); MCP server (Feb 19 2026); web-research AI attribute upgrade (Mar 24 2026); AI-agent workflows (Jun 9 2026). Pricing Free/Plus €39/Pro €93 per user with *two* credit pools (seat credits + workspace credits). Steal: AI attributes = LLM columns on records with a research agent behind them; two-pool credits. Verdict: inspiration (c). Sources: attio.com/changelog, /pricing.

**Clay** — the reference for LLM-column enrichment UX. Free / Launch $167 / Growth $446; "Actions" (<$0.01) vs "Data Credits" ($0.05+); no charge when no result; BYO key skips data credits; variable-token models show "~" estimates; Claygent has 1/2/3-credit tiers shown before running; guidance "start with 10 rows"; optional confidence/reasoning columns; $5B secondary valuation (NYT, Jan 28 2026). Steal: (1) cost shown per row *before* running; (2) test on 10 rows then "run all"; (3) confidence + reasoning as sibling columns; (4) free when the model returns nothing. Verdict: inspiration (c) — Brightflow's enrichment should copy this flow outright. Sources: clay.com/pricing, university.clay.com/lessons/enriching-with-claygent.

**Airtable AI fields / Omni** — pricing Team $20, Business $45 per user; AI App Building and AI Agents marked "NEW"; AI-field preview/credit docs returned 404 (unverified). Verdict: adjacent.

**Google Sheets `AI()`** — `AI("prompt",[range])`; 350 cells per run; text-only; results static until "Refresh and Insert"; "you can't undo or redo… regenerate instead"; attributed to the user in version history. Steal: cap per run + explicit regenerate semantics; attribute AI output to the invoking human in history. **Excel `COPILOT()`** — (unverified; Excel blog shows only general Copilot posts Jun–Sep 2026). Sources: support.google.com/docs/answer/15877199; techcommunity.microsoft.com.

**Hex** — Notebook Agent (pair programmer aware of upstream/downstream deps), Threads in Slack/Claude/Cursor/Codex, "every conversation becomes an inspectable project"; Professional $36, Team $75 per editor + credits. Steal: agent chats materialise as inspectable projects. Verdict: inspiration/competitor (upmarket). Sources: hex.tech/product/magic-ai, /pricing.

**Observable** — rate-limited on every attempt (unverified). **Count** — canvas analytics; AI agent on all tiers under fair use; Free (3 editors) / Pro $49 / Scale $69 per editor. Steal: canvas cells as a visible operation graph. **tldraw** — "tldraw computer" status unclear from its page; tldraw shipped an MCP app (Mar 2026), "tldraw offline — the local whiteboard for you and your agents" (Aug 2026), Flash (Sep 10 2026). Verdict: adjacent. Sources: count.co, /pricing; tldraw.dev/blog.

**Windmill** — OSS self-host orchestrator with AI-agent steps. **Retool Agents** — agentic loop, HITL "consent mechanisms for tool execution," Monitor page with run replay, tool chips, token usage, estimated cost, status; evals with LLM-as-judge and side-by-side run compare. Steal: run replay with cost per run. **Zapier Agents** — add-on metered in "activities" (400 free / 1,500 Pro). **Lindy** — $29.99–$199.99 per user with credits; asks approval before consequential actions, read-only auto-executes. **Relay.app** — shutting down: free accounts Aug 15 2026, paid access ends Sep 14 2026 (today). Steal from Lindy: approval tiered by consequence, not by tool. Sources: docs.retool.com/agents/*, zapier.com/l/agents-pricing, lindy.ai, relay.app.

**cmdk / kbar / Nuxt UI 4 / shadcn** — cmdk (Paco Coursey) 13k stars; kbar 5.3k. Nuxt UI v4 unified free + Pro into one open-source package, 125+ components, requires Nuxt ≥4.1; v4.11.1 published 2026-09-07 (fixes to CommandPalette, Table, ChatMessages); v4.11 added Splitter/ProgressGroup, v4.9 `useTour`. shadcn: Base UI as default, private registries (Aug 2026), "Questionnaire" component for agent clarification, chat components for agent UIs, MCP server, `cn` package (Sep 2026). Steal: shadcn's Questionnaire = a UI primitive for Linear-style "elicitation." Sources: github.com/dip/cmdk, github.com/timc1/kbar, GitHub API (nuxt/ui), ui.nuxt.com migration/v4, ui.shadcn.com/docs/changelog.

---

#### Patterns across this slice

1. **Agents became workspace members with a typed activity vocabulary.** Linear (thoughts/actions/elicitation/responses, delegate-not-assignee), Notion (suggest-edits per change, author indicators), Copilot (every step a commit), Retool (run replay with tool chips). Brightflow's single action bus with one feed is *ahead* in unifying human and agent actions under one primitive; it is *behind* on a published activity taxonomy and on "who wrote this" markers on artefacts.
2. **Undo beat approval, but only for state you control.** Claude Code and Cursor both ship checkpoints with separate axes (files vs. conversation) and are explicit about what undo cannot cover (shell side-effects, subagents). Lindy and Retool keep approval only for consequential/external actions. Brightflow's one-step undo over the action bus is the right default; it should document its non-undoable set the way Claude Code does and gate only external writes (connector write-back, sends) behind consent.
3. **LLM columns converged on Clay's flow: price per row before running, test on ~10 rows, then run all, with reasoning/confidence as sibling columns.** Sheets `AI()` adds a hard per-run cap (350 cells) and "regenerate, don't undo." Attio adds a research agent behind an attribute. Brightflow's vocabulary-based classification is *ahead* on determinism/curation but *behind* on pre-run cost estimate, sample-run, and free-on-empty semantics.
4. **Metering split into two currencies.** Rows (AI tasks vs enrichment tasks), Clay (actions vs data credits), Attio (seat vs workspace credits), Definite (compute vs tokens in one credit). Even self-hosted Brightflow should surface token spend per action and per model in the feed.
5. **The semantic layer is now "agent context," and evals are its test suite.** Lightdash (verified answers, eval suites, self-improving layer), Omni (AI Evals, routines that "can't invent a definition"), Nao/ktx/DAC (context as files with unit tests, skills for Claude Code/Codex). Brightflow's per-column provenance is a good base; missing pieces are verified answers and question→result eval fixtures.
6. **Push-mode insights are standard: scheduled agent runs to Slack/email.** Omni Routines, Lightdash scheduled answers, Databox routines, Linear Loops, Claude Code Routines, Notion Custom Agents. Brightflow's post-sync deterministic insight engine is *ahead* (deterministic first, LLM narrates), but delivery to chat and "continue this run" follow-ups are table stakes now.
7. **MCP is the universal side door.** Nearly every product here (Definite, Keboola, Peliqan, Basedash, Lightdash, Databox, Geckoboard, Draxlr, Luzmo, Attio, DataGrip, DataZen, Bauplan, Duckle, tldraw, Figma, Linear) ships an MCP server, and enterprise SSO for MCP appeared in Aug 2026 (Linear, Figma). Brightflow's action bus is a natural MCP surface; not having one is now a visible gap.
8. **The low end vacated, the high end consolidated.** Basedash to $1,000/mo, Equals $24K/yr, Lightdash Cloud $3,000/mo; Explo→Omni, Rows→Superhuman, Outerbase→Cloudflare, Causal→Lucanet, Browser Co→Atlassian, Relay shut down, Obviously AI→Zams. The self-hosted single-binary indie tier is held only by OSS (Chartbrew, Evidence, Duckle, DataZen, DuckDB UI) — Brightflow's positioning is uncontested by funded players.
9. **Command bars are assumed; the new UI primitive is the "clarification" step.** Nuxt UI 4 ships CommandPalette/ChatMessages free; shadcn added Questionnaire for agent elicitation; Cursor and Claude Code added Plan mode. Brightflow's "GUI first, always keyboard" is aligned; adding an elicitation surface (agent asks, human answers in the feed) is the next step.
10. **Local-first tooling is real but thin.** DuckDB UI (frontend still closed), DataZen, Duckle, tldraw offline, Obsidian Bases — all emphasise "data never leaves your machine" with optional cloud. Brightflow's Parquet-under-SQLite architecture is the same shape; Instant SQL's run-as-you-type is the speed bar to match.

---

## Pass 8: Human–agent collaboration patterns, 2025–2026: a survey for Brightflow


Research note: the web-search budget for this session was exhausted before the first query, so everything below comes from direct fetches of primary pages (docs, changelogs, specs). Where a page 404'd or was blocked (OpenHands docs, Codex docs behind `learn.chatgpt.com`, Rovo, Adobe, Google Workspace, Zendesk auto-assist, Devin billing, Amplitude MCP) I say so and mark claims **(unverified)**.

---

#### A. Coding agents (the most mature interaction models)

##### Claude Code — code.claude.com/docs
Terminal/IDE agent with the richest published permission model.
**Status 2026:** six permission modes; **auto mode** (a second "classifier" model reviews each action instead of the human) is now the default starting mode on Pro/Max/Team; checkpointing/rewind; 30+ hook events; Agent SDK; Managed Agents beta (`managed-agents-2026-04-01`).
**Interaction model:**
- *Trust dial as modes:* `default` (Manual: reads only), `acceptEdits`, `plan` (read + propose, edits blocked until plan approved), `auto` (classifier reviews), `dontAsk` (auto-deny anything that would prompt; for CI), `bypassPermissions`. Cycle with Shift+Tab. Modes are the baseline; allow/ask/deny **rules** layer on top, and deny rules win in every mode. Orgs can disable bypass/auto via managed settings. Some actions are "no mode auto-approves" (rm on critical paths, protected `.git`/`.claude` paths).
- *Hooks as policy bus:* `PreToolUse` can `allow`/`deny`/`ask` and even rewrite `updatedInput`; `PermissionRequest` overrides the classifier verdict; hook types include command, HTTP, MCP tool, prompt, and agent. Rules are enforced by the harness, not the model ("Instructions in your prompt or CLAUDE.md … don't change what Claude Code allows").
- *Undo:* checkpoints per user prompt; `/rewind` restores code, conversation, or both; 100 checkpoints kept; explicitly "not a replacement for version control"; bash-made changes and subagent edits not tracked.
- *Plan mode approval UX:* "Yes, and use auto mode / Yes, auto-accept edits / No, keep planning" — approval of a plan *switches trust level* for the rest of the session.
**Worth stealing:** (1) the ask-rule that "still prompts even in auto mode" — a per-action user opt-in exactly like Brightflow's "Ask me first"; (2) hook decision JSON with `updatedInput` (edit-before-execute at the bus, not the UI); (3) "actions no mode auto-approves" list — a named irreversible set.
**Verdict:** validates undo-over-approval + trust dial; adopt the *layered* rules-over-modes structure and the named irreversible list.
Sources: [permissions](https://code.claude.com/docs/en/permissions), [permission modes](https://code.claude.com/docs/en/permission-modes), [checkpointing](https://code.claude.com/docs/en/checkpointing), [hooks](https://code.claude.com/docs/en/hooks), [Agent SDK](https://code.claude.com/docs/en/agent-sdk/overview), [Managed Agents](https://platform.claude.com/docs/en/managed-agents/overview).

##### Cursor — cursor.com
IDE agent, cloud agents, and (Sep 2026) "Projects".
**Status 2026:** Aug 19 cloud agents/harness (`/goal` long-lived objectives, subscriptions to PRs and Slack threads, subagents on isolated VMs, mid-run steering); Aug 17 "Origin" code hosting; Sep 2 self-hosted machines and computer use; Sep 10 **Projects** — coordinator agents that plan and delegate to implementing agents.
**Interaction model:** checkpoints snapshot modified files per agent request; restore reverts files but "does not remove messages from the conversation"; stored locally, separate from Git. Hooks (`hooks.json`) with `beforeShellExecution`, `beforeMCPExecution`, `preToolUse`, `afterFileEdit`, etc., returning allow/deny/ask; `failClosed` option; documented uses include "enforce least-privilege policies on AI agent actions".
**Worth stealing:** code-vs-conversation split on restore; ask-decision from hooks for specific tools; `failClosed` semantics for policy hooks.
**Verdict:** validates checkpoints as session-level undo distinct from history; cautionary that Cursor keeps checkpoints per *file*, not per *action*, so Brightflow's action-level undo is finer.
Sources: [changelog](https://cursor.com/changelog), [checkpoints](https://cursor.com/docs/agent/chat/checkpoints), [hooks](https://cursor.com/docs/agent/hooks).

##### GitHub Copilot coding agent ("cloud agent") — docs.github.com
Assign an issue, get a draft PR from a GitHub Actions run.
**Status 2026:** Sep 1 "Copilot code review can now approve pull requests"; Sep 9 enterprise-managed permissions for agent operations; Sep 1 per-user budgets with expiry; Copilot Workspace is no longer a product page I could find (superseded by the cloud agent) **(unverified)**.
**Interaction model:** the agent acts *through the same objects humans use* — branches, commits, PRs, Actions. "Every step happening in a commit and being viewable in logs." **Agent identity:** commits are authored by Copilot with the initiating user as co-author, each linked to a session log. Session logs show "internal reasoning and the tools it used". Approval = PR review; branch rulesets can block it unless Copilot is a bypass actor. Cloud sessions shared by default; "Stop session" preserves pushed commits. Steering messages consume credits.
**Worth stealing:** co-author attribution (human who triggered + agent that acted) on every mutation; a session log link on each mutation.
**Verdict:** validates "same actions, one feed"; adopt dual attribution (initiator + actor).
Sources: [about coding agent](https://docs.github.com/en/copilot/concepts/agents/coding-agent/about-coding-agent), [track sessions](https://docs.github.com/en/copilot/how-tos/use-copilot-agents/coding-agent/track-copilot-sessions), [changelog](https://github.blog/changelog/label/copilot/).

##### Devin (Cognition) — docs.devin.ai
Cloud software engineer; web app, Slack, Teams, API, CLI.
**Interaction model:** conversational session with a live Shell/IDE/Browser the human can take over ("take over in Devin's IDE once it gives you a good first draft"). Enterprise: admins review GitHub permissions granted to Devin; secrets store; dedicated/on-prem tenants. ACU-based usage metering exists but the billing page 404'd **(unverified)**.
**Worth stealing:** "take over mid-session in the same workspace" — the human and agent share one editor, not a review pane.
**Verdict:** validates GUI-first shared workspace; usage accounting (ACUs) is a model for Brightflow's missing per-user limits.
Sources: [intro](https://docs.devin.ai/get-started/devin-intro), [enterprise](https://docs.devin.ai/enterprise/overview).

##### OpenHands (formerly All-Hands) — github.com/OpenHands
Open-source agent; now positioned as "Agent Canvas", a self-hosted control center that also runs third-party agents (Claude Code, Codex) via an Agent-Client Protocol, with an Agent Server REST API and an Automation Server for scheduled/event-triggered runs. Docs domain was blocked in this session; the event-stream (action/observation) architecture, confirmation mode and per-action security-analyzer risk levels are from prior knowledge **(unverified)**.
**Worth stealing:** exposing *other vendors' agents* through your own control plane and activity log.
**Verdict:** pattern to watch — Brightflow could be a canvas for external agents via MCP rather than only hosting its own.
Source: [repo](https://github.com/OpenHands/OpenHands), [SDK](https://github.com/OpenHands/software-agent-sdk).

##### OpenAI Codex (CLI + cloud) — github.com/openai/codex
Docs moved behind `learn.chatgpt.com` (blocked here). The repo config page confirms a managed-hooks policy (`allow_managed_hooks_only = true` in `requirements.toml`) so admins can force org hooks over user hooks. Approval policies (`untrusted` / `on-request` / `on-failure` / `never`) and sandbox modes (`read-only` / `workspace-write` / `danger-full-access`) are from prior knowledge **(unverified this session)**.
**Worth stealing:** `on-failure` — run sandboxed, only ask when the sandbox blocks something. Managed-hooks-only for admins.
**Verdict:** validates trust dial; the "ask only on sandbox failure" step is a neat intermediate notch.
Source: [config.md](https://github.com/openai/codex/blob/main/docs/config.md).

##### Google Jules — jules.google
**Status 2026:** REST API (Jan 26: repoless sessions, git-patch outputs, timestamp-filtered activity), scheduled tasks (edit/pause/resume), **suggested tasks** (scans TODOs, proposes for approval), and a **Planning Critic** (Jan 26) — a second agent reviews auto-approved plans, cutting failures 9.5%.
**Interaction model:** plan shown and approved before code changes; VM sandbox; PR output.
**Worth stealing:** a deterministic/second-model critic on *auto-approved* work — the same "stochastic proposes, deterministic grounds" idea applied to plans.
**Verdict:** validates Brightflow's engine-grounds-LLM stance; approval-before-act is the cautionary opposite of undo-over-approval, but Jules is moving toward auto-approve + critic.
Sources: [docs](https://jules.google/docs), [changelog](https://jules.google/docs/changelog/).

##### Replit Agent — docs.replit.com
**Interaction model:** Plan mode (read-only) then Build mode; automatic checkpoints per completed task with rollback; **task board** (Drafts → Active → Ready → Done) where background tasks run isolated and "the main project remains unchanged until you explicitly approve changes" — "Apply changes to main version" or dismiss; Agent auto-merges conflicts. Effort-based pricing with confirmation before paid actions; Free/Power/Max modes; spend limits and budgets.
**Worth stealing:** the task card with apply/dismiss is nearly Brightflow's "run reviewed as one card"; per-task isolation before merge.
**Verdict:** validates run-as-card; the spend controls are a template for usage accounting.
Sources: [agent](https://docs.replit.com/replitai/agent), [task system](https://docs.replit.com/core-concepts/agent/task-system.md), [plan vs build](https://docs.replit.com/learn/plan-vs-build-mode.md).

##### Lovable / Bolt
**Lovable:** Plan mode vs Build mode (default, applies directly); a details timeline shows every tool call and a changes tab shows file diffs; an undo button reverts to the previous state. **Bolt:** automatic version history with preview-before-restore; GitHub sync for real history.
**Verdict:** both validate "apply immediately, show the diff, one-click undo" for consumer builders.
Sources: [Lovable modes](https://docs.lovable.dev/features/modes), [Bolt version history](https://support.bolt.new/concepts/version-history-github).

---

#### B. Work tools where agents are members

##### Linear for Agents — linear.app/developers/agents
The clearest "agents use the same actions as humans" implementation.
**Status 2026:** Coding Sessions (Jun 11, Claude and Codex from an issue); "Write with Agent" project updates (Jun 18); agent document editing with **"author name indicators let you check if text was written by a colleague or added by a loop"** (Jul 23); **Loops** (Sep 14) — recurring agent workflows that react to workspace activity, edit docs, post to Slack, and "continue any run"; third-party app approvals by admins (Sep 3); MCP with enterprise-managed auth via Okta.
**Interaction model:** agents install as workspace members via OAuth `actor=app` (cannot take `admin` scope); they are **delegates, not assignees**, so a human keeps ownership; an *Agent Session* is created on mention/delegation and the agent must emit a `thought` activity within 10 s; activities are typed (thought, action, elicitation, response, error). Everything an agent does is a normal Linear object visible in the issue's activity.
**Worth stealing:** delegate-vs-assignee (human stays accountable); typed session activities including *elicitation* (agent asks a question inline); author-name indicators on agent-written prose.
**Verdict:** strongest validation of principles 1 and 2; adopt typed activities and delegate semantics.
Sources: [agents](https://linear.app/developers/agents), [changelog](https://linear.app/changelog), [MCP](https://linear.app/docs/mcp).

##### Notion agents — notion.com
**Interaction model:** Custom Agents run on schedules or triggers (Notion events, Slack, mail, calendar); they edit pages/databases, file bugs, post to Slack, hand off to other agents, and use MCP connections (Linear, Figma, GitHub…). Governance: "full audit trails showing what triggered it, what it did, and why"; edits reversible via version history; admins restrict who can create agents, which models, see all agents and their access levels, and **track real-time credit usage per agent**. Three permission levels per agent (Full access / Can edit / Can view and interact). Hosted MCP with OAuth and admin allowlists.
**Worth stealing:** per-agent credit metering; "what triggered it, what it did, and why" as the audit triple; per-agent sharing levels.
**Verdict:** validates undo-over-approval (version history is the safety net, not a gate); per-agent usage accounting is exactly Brightflow's gap.
Sources: [product](https://www.notion.com/product/agents), [help](https://www.notion.com/help/custom-agents), [MCP](https://developers.notion.com/docs/mcp).

##### Atlassian Rovo — (all Rovo pages 404'd this session)
Prior knowledge: Rovo agents act in Jira/Confluence on behalf of the user, Rovo Dev generates plans then PRs, admins control data access **(unverified)**. Marketing page only confirms "admins have clear control over what AI accesses".
**Verdict:** no verifiable interaction-model detail; treat as parity with Linear but less transparent.
Source: [rovo](https://www.atlassian.com/software/rovo).

##### Slack — docs.slack.dev/ai
Agents are a first-class app type with dedicated surfaces (split view, app threads, streaming). Two MCP directions: **Slackbot as MCP client** (invokes remote servers from prompts) and **Slack MCP server** (search, post, canvases) for Cursor/Claude.
**Verdict:** validates outward doors in both directions.
Source: [docs](https://docs.slack.dev/ai).

##### Salesforce Agentforce — salesforce.com/agentforce
Agents act via Flows, MuleSoft connectors, Apex/JS — i.e., the same automation primitives humans build; escalation to humans; Agentforce Observability; 2026 adds verified MCP server support, multi-agent orchestration, Agent Script.
**Verdict:** "same primitives" but at the workflow layer, not the action layer; observability, not undo.
Source: [agentforce](https://www.salesforce.com/agentforce/).

##### Microsoft Copilot Studio governance — learn.microsoft.com (updated 2026-08-04)
The most complete governance checklist: **Agent 365** as a control plane where agents get Entra identities governed by Conditional Access and RBAC; DLP policies over knowledge/actions/triggers; Copilot credit caps; maker audit logs in Purview and Sentinel; "run tools with user credentials" by default; real-time risk assessment before publishing.
**Worth stealing:** agents as identities with the same access-control machinery as users; credit caps.
**Verdict:** the enterprise reference for roles/limits/usage — Brightflow's declared gap.
Source: [security and governance](https://learn.microsoft.com/en-us/microsoft-copilot-studio/security-and-governance).

##### Zapier Agents, Lindy, Relay.app, Windmill, n8n, Retool
- **Zapier Agents:** act through 9,000+ integrations; activity monitoring; "ask before acting" framing. ([zapier.com/agents](https://zapier.com/agents))
- **Lindy:** "write actions always wait for your approval, so nothing ships without you"; runs on user credentials. ([docs](https://docs.lindy.ai/)) — approval-first, the cautionary opposite.
- **Relay.app:** **shut down** — its site is now a wind-down notice offering export of workflows, sequences, MCP servers and run history. ([relay.app](https://www.relay.app/)) Cautionary: approval-centric automation as a standalone product did not survive.
- **Windmill:** approval steps suspend a flow; approval pages carry schema forms whose values feed later steps; enterprise options "require approvers to be logged in", "disable self-approval", "require group membership"; the `approvers` array is recorded. ([approval flows](https://www.windmill.dev/docs/flows/flow_approval)) — worth stealing: approval records *who* approved as data downstream steps can read.
- **n8n:** MCP Server Trigger exposes workflows as tools (bearer/header auth); human-in-the-loop send-and-wait nodes exist but the page moved **(unverified)**. ([MCP trigger](https://docs.n8n.io/integrations/builtin/core-nodes/n8n-nodes-langchain.mcptrigger/))
- **Retool Agents:** tools = core, custom, workflows/other agents, MCP; a run shows "Queued" when "a tool requires approval"; step-by-step logs with reasoning, inputs, run IDs; A2A as a trigger. ([quickstart](https://docs.retool.com/agents/quickstart))

---

#### C. Support and analytics products

##### Intercom Fin — fin.ai
Fin Agent executes "Procedures" that read/write third-party systems; **Fin Operator** autonomously updates knowledge and scales automations; humans review via a testing suite (simulations, regression) and an observability/QA suite with "one-click recommendations". The dedicated Fin AI Analyst page 404'd **(analyst review flow unverified)**.
**Verdict:** review happens *before deploy* (tests) and *after* (QA), not per action — a different axis Brightflow's engine could add (regression tests for agent skills).
Source: [fin.ai](https://fin.ai/).

##### Zendesk AI agents — zendesk.com
Autonomous multi-step resolution; routing to humans with context; automated QA "to audit outcomes, enforce policies"; a "Resolution Learning Loop". The auto-assist approve/edit article 404'd **(unverified)**.
Source: [ai agents](https://www.zendesk.com/service/ai/ai-agents/).

##### Amplitude AI Agents — amplitude.com/blog/ai-agents
Goal-driven agents (conversion, onboarding, monetization) that monitor data and propose changes; **"Agents won't make customer-facing changes without your OK"**; "decide how much autonomy to give Agents. Start small, add guardrails, and scale up." Beta at time of the post. MCP server page 404'd **(unverified)**.
**Verdict:** analytics peer converging on a trust dial; approval only at the customer-facing boundary — an "irreversible = external" definition Brightflow can reuse.
Source: [blog](https://amplitude.com/blog/ai-agents).

##### Metabase — metabase.com
**Status 2026:** v58 (Jan) Metabot GA; v59 (Mar) AI SQL in OSS; v60 (Apr) "AI just went open source", official MCP server, Metabot in Slack; **v61 (May) AI governance layer** — AI access controls by group, token limits per group/tenant/instance, usage analytics "down to the questions asked and tokens used"; v62 (Jun) charts as results in MCP clients; v63 (Jul) more LLM providers. Metabot sees only what the user sees; MCP server lacks the granular tool controls Metabot has; agent-driven workflows Pro/Enterprise only.
**Worth stealing:** per-group token budgets and question-level usage analytics; "agent sees what the user sees" as the default authorization rule.
**Verdict:** the direct analytics comparable for Brightflow's roles/limits/usage gap.
Sources: [AI overview](https://www.metabase.com/docs/latest/ai/overview), [releases](https://www.metabase.com/releases).

##### PostHog AI (Max) — posthog.com
**Status 2026:** aggressive MCP expansion (32 tools Aug 3, 44 Aug 6, 21 Aug 27, 19 Sep 3); "Self-driving" **Scouts/Signals** — background agents that file reports; team-wide **notes** to steer scouts without editing skills (Jul 24); scout feedback becomes future context (Aug 6); **external MCP agents can claim/release Signal reports and attach PRs** (Sep 10); draft mode requiring confirmation before support replies go out (Jul 24); auto-refund of Signals PR charges.
**Interaction model:** creates insights/dashboards/flags/surveys directly; per-tool LLM spend opt-in at org level; hosted MCP with auth routing per region.
**Worth stealing:** "feedback on an agent report becomes context for the next run" — Brightflow's edit-as-training-signal, productised; external agents claiming internal work items via MCP.
**Verdict:** validates principle 4 and the outward door; PostHog is the closest analytics peer on "findings leave the app".
Sources: [PostHog AI](https://posthog.com/docs/max-ai), [MCP](https://posthog.com/docs/model-context-protocol), [changelog](https://posthog.com/changelog).

##### Databricks Unity Catalog AI comments — docs.databricks.com
Per-object "AI generate" button; **"Click Accept to accept the comment as-is, or Edit to modify it before you save it"**; inline assistant to shorten/translate; no workspace toggle; docs say comments "must be reviewed prior to saving". Once saved they are ordinary comments — **no provenance marker**.
**Verdict:** the canonical edit-before-accept UI; cautionary in that provenance is lost on save — Brightflow's layered detected→connector→agent→user model is strictly better.
Source: [AI comments](https://docs.databricks.com/aws/en/comments/ai-comments).

##### Design tools: Figma, Canva, Adobe
- **Figma:** remote MCP at `mcp.figma.com/mcp` with write-to-canvas (frames, components, variables) in beta; version history autosaves every 30 min, named versions, each shows "the main contributor" — AI edits are not separately attributed. ([MCP](https://help.figma.com/hc/en-us/articles/32132100833559-Guide-to-the-Dev-Mode-MCP-Server), [history](https://help.figma.com/hc/en-us/articles/360038006754-View-a-file-s-version-history))
- **Canva:** MCP with create/edit/export/comment tools; auth via **Client ID Metadata Documents** (the newest MCP registration method); permissions equal the user's. ([canva.dev](https://www.canva.dev/docs/mcp/))
- **Adobe:** pages 404'd; Agent Orchestrator and creative-tool agents **(unverified)**.
**Verdict:** design tools give agents write access through the same document model, but attribution lags (Figma's "main contributor" hides agent edits) — cautionary.

##### Google Workspace
Blog index shows a 2026 post "5 new agentic capabilities across Google Workspace apps" but the article 404'd; Gems/Workspace Studio details **(unverified)**. Source: [blog index](https://workspace.google.com/blog/product-announcements).

---

#### D. Protocols and conventions

##### MCP — modelcontextprotocol.io (spec 2026-07-28)
**Status:** the July 2026 revision makes MCP **stateless** (no session id, no initialize handshake; `server/discover`), replaces server-initiated requests with **Multi Round-Trip Requests** (`resultType: "input_required"` + `inputRequests`, then client retries with `inputResponses`) — this is how a tool now *elicits* a human decision; Tasks moved to an official extension (polling, mid-flight input, durable handles); **MCP Apps** extension for inline UI (charts, forms); Skills-over-MCP; Roots/Sampling/Logging deprecated; Dynamic Client Registration deprecated in favour of Client ID Metadata Documents; OpenTelemetry trace context in `_meta`; tool lists may vary by the caller's granted scopes.
**Human-in-the-loop in the spec:** "there SHOULD always be a human in the loop with the ability to deny tool invocations"; clients SHOULD show tool inputs before calling, prompt on sensitive ops, and "log tool usage for audit purposes"; tool `annotations` are untrusted unless the server is trusted. Authorization: OAuth 2.1 resource server, RFC 9728 metadata, RFC 8707 resource indicators, PKCE.
**Analytics-vendor MCP servers (verified):** Metabase, PostHog, dbt (semantic layer/discovery/SQL remote; CLI write commands self-hosted only), Snowflake managed MCP (RBAC per tool, allowlists, max 50 tools), Grafana (`--disable-write`, `--disable-query`, per-category flags), Linear (`/mcp/readonly` endpoint; read scope = no write APIs), Notion, Slack, Figma, Canva. MotherDuck and Amplitude pages 404'd **(unverified)**.
**Worth stealing:** an action manifest that maps 1:1 onto MCP tools with `readOnlyHint`/`destructiveHint` and an `input_required` path for "Ask me first"; a `/readonly` endpoint variant; OpenTelemetry trace ids on every action for the activity feed.
**Verdict:** adopt — Brightflow's JSON-schema action manifest is already the shape of a `tools/list`; exposing it as a stateless MCP server is the cheapest outward door.
Sources: [changelog](https://modelcontextprotocol.io/specification/latest/changelog), [tools](https://modelcontextprotocol.io/specification/2026-07-28/server/tools), [authorization](https://modelcontextprotocol.io/specification/2025-06-18/basic/authorization), [extensions](https://modelcontextprotocol.io/docs/extensions/overview), [dbt](https://docs.getdbt.com/docs/dbt-ai/about-mcp), [Snowflake](https://docs.snowflake.com/en/user-guide/snowflake-cortex/cortex-agents-mcp), [Grafana](https://github.com/grafana/mcp-grafana).

##### OpenAI Apps SDK / Agents SDK
Apps SDK is now a "Plugins" SDK: MCP servers + skills + optional UI resources. Agents SDK HITL: `needs_approval=True` (bool or async fn) on tools including MCP; run pauses with `ToolApprovalItem`s; `RunState.to_json()` for durable approvals; `approve()/reject()`, `always_approve` sticky decisions; custom rejection messages fed back to the model.
**Worth stealing:** serialisable interrupted state so an approval can wait days; `always_approve` as the "don't ask again" notch.
Sources: [Apps SDK](https://developers.openai.com/apps-sdk), [HITL](https://openai.github.io/openai-agents-python/human_in_the_loop/).

##### Google A2A — a2a-protocol.org
v1.0; Linux Foundation TSC (AWS, Cisco, Google, IBM, Microsoft, Salesforce, SAP, ServiceNow); joined the **Agentic AI Foundation**; task states include `input-required` and `auth-required`. Retool accepts A2A as a trigger.
**Verdict:** watch; MCP first.
Source: [a2a](https://a2a-protocol.org/latest/).

##### AGENTS.md / llms.txt
AGENTS.md: 60k+ repos, backed by Codex, Jules, Cursor, Amp, Factory; stewarded by the Agentic AI Foundation; nested files in monorepos. llms.txt: adopted by OpenAI, Anthropic, Google docs; Mintlify/GitBook auto-generate; Chrome Lighthouse audits it. Both Anthropic and MCP docs serve `llms.txt`.
**Verdict:** adopt — ship `AGENTS.md` in the repo and `llms.txt` from the API server (generated from the manifest, per the repo's "generated, never hand-written" rule).
Sources: [agents.md](https://agents.md/), [llmstxt.org](https://llmstxt.org/).

---

#### E. Frameworks, undo systems, and writing

- **LangGraph interrupts:** `interrupt(payload)` suspends at a checkpoint; `Command(resume=value)` re-runs the node from the top; documented patterns for approve, *edit state before continuing*, and tool-parameter editing; time travel via checkpointer + `thread_id`. ([docs](https://docs.langchain.com/oss/python/langgraph/interrupts)) — the framework-level edit-before-accept.
- **Inngest** `step.waitForEvent` for approvals, events stored for audit; **Temporal** signals/updates with an "Approval design pattern", cites Lindy and Dust as agent users. ([Inngest](https://www.inngest.com/docs/features/inngest-functions/steps-workflows/wait-for-event), [Temporal](https://docs.temporal.io/evaluate/use-cases-design-patterns))
- **HumanLayer:** pivoted from an approval SDK to a "multiplayer coding agent IDE and cloud"; "comments and decisions feed straight back to the agents". ([humanlayer.dev](https://www.humanlayer.dev/)) Cautionary: the pure approval-SDK category shrank (Relay shut down, HumanLayer pivoted).
- **gotoHuman:** review templates with **editable fields before approval**, retry loops, webhook resume; integrates with LangGraph and MCP. ([gotohuman.com](https://www.gotohuman.com/)) — the surviving standalone edit-before-accept product.
- **Yjs UndoManager:** `trackedOrigins` scopes undo to a chosen origin so a user undoes only their own edits in a shared doc; `captureTimeout` batches. ([docs](https://docs.yjs.dev/api/undo-manager)) — worth stealing: **origin-scoped undo** so "undo the agent's run" never undoes a colleague's concurrent edit.
- **Anthropic engineering:** "Building effective agents" (ACI deserves HCI-level effort; poka-yoke tool design; agents "pause for human feedback at checkpoints"); "Writing tools for agents" (namespacing, `response_format` verbosity, actionable errors, consolidate workflows into one tool like `schedule_event`, evaluation-driven design). ([1](https://www.anthropic.com/engineering/building-effective-agents), [2](https://www.anthropic.com/engineering/writing-tools-for-agents))
- **Vercel AI SDK 5:** tool lifecycle states and error re-submission, no approval primitive documented on that page. ([blog](https://vercel.com/blog/ai-sdk-5)) Maggie Appleton's site lists no 2024–26 agent-UX essay ([site](https://maggieappleton.com/)). Amershi et al. "Guidelines for Human-AI Interaction" (CHI 2019) remains the cited source for "efficient correction / efficient dismissal" ([MSR](https://www.microsoft.com/en-us/research/publication/guidelines-for-human-ai-interaction/)); no 2025–26 paper titled "undo over approval" was locatable without search **(unverified)**.

##### Who explicitly claims "humans and agents use the same actions"
Verified in substance, if not in those words: **Linear** (agents are OAuth workspace members using the same GraphQL API and objects), **GitHub Copilot** (commits/PRs/Actions, Copilot as author + human co-author), **Notion** (agents edit the same pages, reversible via the same version history), **PostHog** (external MCP agents claim the same Signal reports humans do), **Metabase** ("Metabot will only see what the person using it can see"), **Windmill/Retool** (approval and agent steps are ordinary scripts/tools). No product I could fetch uses the phrase "actions manifest" or "everything is a command" for agent parity **(unverified absence)**.

---

#### Patterns across this slice

1. **Permission modes are now table stakes, and the industry converged on 4–6 notches:** read-only/plan → auto-accept edits → ask-on-failure/classifier-reviewed → bypass. Claude Code, Codex, Replit, Lovable, and Cursor all ship a plan/read-only mode and an "apply directly" mode. Brightflow's binary "undo or Ask me first" is *simpler than* the field; that is fine for a few-person workspace but a middle notch — "run, but a deterministic check (the engine) reviews before apply" à la Claude's classifier and Jules' Planning Critic — is the obvious next step.
2. **Checkpoints replaced approval as the default safety net in coding tools** (Claude Code, Cursor, Replit, Lovable, Bolt) and in knowledge tools (Notion: "reversible through version history"). Approval-first products (Lindy) survive in narrow niches; Relay.app shut down and HumanLayer pivoted. Brightflow's undo-over-approval is now mainstream, not contrarian.
3. **Agent identity in activity feeds is standard; *dual* attribution is the emerging refinement.** GitHub records Copilot as author + human co-author; Linear marks agent-written text with author indicators; Notion logs "what triggered it, what it did, and why". Brightflow's actor+provenance feed matches; add the *trigger* (which human, which run, which rule) as a third field.
4. **Edit-before-accept is common in metadata/catalog tools (Databricks, gotoHuman, LangGraph edit-state) but provenance is usually lost on save.** Brightflow's layered provenance is ahead here; its gap (no edit-before-accept on proposals) is the one place it is behind Databricks.
5. **"Feedback becomes context" is the productised form of "edit is training signal":** PostHog scout feedback, HumanLayer comments, Jules suggested-task approvals. Brightflow already logs overriding edits; feed them back into the next narration prompt.
6. **MCP is the outward door everyone built in 2026** — Metabase, PostHog, dbt, Snowflake, Grafana, Linear, Notion, Slack, Figma, Canva, Salesforce. Common safeguards: a read-only endpoint/scope, per-tool RBAC, tool-category disable flags, "agent sees what the user sees". The 2026-07-28 spec's stateless model plus `input_required` maps directly onto Brightflow's dispatch + "Ask me first".
7. **Governance (roles, budgets, usage) arrived in analytics this year:** Metabase v61 per-group token limits and usage analytics; Notion per-agent credits; Copilot per-user budgets with expiry; Copilot Studio credit caps and agent identities. This is Brightflow's biggest declared gap and the peers have now defined the minimum: per-actor spend, per-group limits, question-level usage log.
8. **Of Brightflow's six principles:** (1) one action bus — rare as an explicit claim, common in substance at Linear/GitHub; (2) one activity feed with actor+provenance — industry standard; (3) undo over approval — now standard in coding/knowledge tools, still rare in support/automation; (4) editable AI output as logged override — rare and Brightflow's differentiator; (5) GUI-first, keyboard rewarded — universal; (6) WebSocket push — universal.
9. **Adopt next, in order:** (a) expose the action manifest as a stateless MCP server with a `/readonly` variant and `input_required` for "Ask me first" — this closes both "no outward door" and "findings do not leave the app" (PostHog's claim-a-report model shows the shape); (b) origin-scoped undo (Yjs `trackedOrigins`) plus dual attribution (initiator + actor + trigger) so run-level undo-all is safe in a shared workspace; (c) per-actor usage accounting and per-group budgets in the Metabase v61 / Notion style, which also makes the "Ask me first" dial a governable admin setting rather than only a personal one.

---

