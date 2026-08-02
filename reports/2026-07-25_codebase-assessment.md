# Codebase Assessment — Brightflow

**Date:** 2026-07-25
**Scope:** Overall strengths, weaknesses, capabilities, and recommended next steps.
**Method:** Static review of repo structure, git history (101 commits since 2026-02-03),
dependencies, docs, tests, and key source files. No coding performed.

---

## 1. What Brightflow is

A self-hosted **analytics platform** with a Rust backend (Axum + Polars over a
SQLite-metadata + Parquet-data lakehouse) and a Vue 3 frontend. It positions
itself not as a BI dashboard but as a **"pair-analysis" tool**: a deterministic
statistical engine surfaces findings, an LLM agent narrates/curates, and a human
co-edits an interpretation layer (labels, clusters, taxonomies) on top of
immutable data. See `docs/human_ai_interaction.md` for the guiding philosophy
("stochastic proposes, deterministic grounds").

### Capabilities (by surface)

| Surface | What it does | Key files |
|---|---|---|
| **Explore** | Visual query builder over any loaded Parquet table; WebSocket-backed Polars execution | `brightflow-app/src/components/query/`, `stores/query.ts`, `composables/useWsQuery.ts`, `crates/brightflow-api/src/analytics/` |
| **Insights engine** | Autonomous read-only statistical analysis: trends, anomalies, change-points, seasonality, concentration, correlation, distribution shift, outliers, forecast, drivers (delta decomposition), period comparison | `crates/brightflow-engine/src/analysis/*` (21 detector modules), `analysis/engine/` (pipeline + scoring + review + trends + drivers) |
| **Topics / NLP** | TF-IDF + Model2Vec embeddings, HDBSCAN clustering, c-TF-IDF naming, supervised intent classifier (linfa-logistic), near-duplicate detection | `crates/brightflow-engine/src/nlp/*`, `embedding/`, `enrichment/topic_enricher.rs` |
| **Enrichment** | Versioned derived columns; LLM-prompt functions + deterministic transforms; sample-run, promote/demote, full runs | `crates/brightflow-engine/src/enrichment/`, `crates/brightflow-api/src/enrichment/` |
| **Text Explorer** | Term/phrase search with scoring + highlighting across any text column | `crates/brightflow-api/src/textexplore/`, `brightflow-app/src/components/textexplore/` |
| **Product / Web analytics** | Event ingestion (script.js + `/event`, `/track`, `/identify`), GeoIP (maxminddb), UA parsing, funnels, retention, user explorer | `crates/brightflow-api/src/ingest/`, `product_analytics/`, `web_analytics/` |
| **Connectors** | Lua-based via sibling `longbow` crate; builtins: github, bluesky; CSV upload sources; incremental cursor sync | `crates/brightflow-connect/`, `connectors/*.lua` |
| **LLM agent** | OpenAI-compatible chat client; 6 run kinds (auto_label, propose_merges, narrate_insights, triage_insights, propose_taxonomy, label_documents); tool-calling; bounded iterations/tokens | `crates/brightflow-llm/`, `crates/brightflow-api/src/agent/` |
| **Action bus / curation** | Single typed `Action` enum = REST body + ts-rs frontend union + LLM tool schema. Undo-first, tiered approval, run-level bulk undo. Everything logged to one feed. | `crates/brightflow-api/src/actions/`, `crates/brightflow-store/migrations/010_action_log.sql` |
| **Auth** | axum-login + argon2 + tower-sessions (SQLite) | `crates/brightflow-api/src/auth/` |
| **Scheduler** | sqlx-based job runner for connector syncs + post-sync auto-insights | `crates/brightflow-scheduler/` |
| **System** | Live log streaming, proc metrics, process sampler | `crates/brightflow-api/src/system/` |

---

## 2. Strengths

1. **Cohesive architecture around one idea.** The "stochastic proposes,
   deterministic grounds" principle is consistently enforced: the engine is
   read-only and never mutates state; all mutations (human or AI) go through
   the same `Action` bus; outputs are editable/undoable. This is rare and
   well-executed.

2. **Strong engineering discipline.**
   - Workspace-level clippy `pedantic`/`nursery`/`cargo` lints at `warn`,
     `unwrap_used`/`expect_used`/`panic` denied at the *workspace* level
     (`Cargo.toml`). `unsafe_code = "deny"` globally.
   - **Zero** `TODO`/`FIXME`/`HACK` markers in `crates/`.
   - jemalloc tuned for short-lived datasets (dirty_decay_ms=0).
   - Frontend: strict TS (`noUncheckedIndexedAccess`, `exactOptionalPropertyTypes`),
     Oxlint type-aware, `no-console`/`no-debugger` as errors.

3. **Type-safe full-stack contract.** `ts-rs` generates TS types from Rust
   structs; `schemars` derives JSON Schema for the same `Action` enum used as
   LLM tool definitions. One source of truth across REST / frontend / agent.

4. **Real test coverage on the hard part.** ~402 `#[test]`/`#[tokio::test]`;
   289 in the engine (the statistical core), plus integration tests
   (`tests/drivers_report.rs`, `tests/classifier_quality.rs`,
   `tests/topics_quality.rs`, `tests/insights_pipeline.rs`, `tests/insight_runs.rs`).
   The NLP/stats code — where bugs are subtle — is where the tests live.

5. **Honest self-documentation.** `docs/insights_limitations.md` enumerates the
   statistical edges (Bonferroni scope, synthetic null-model calibration,
   irregular-sampling seasonality, single-dimension drivers, day-first date
   parsing) with *why* and *what would lift it*. This is the mark of a mature
   codebase.

6. **Pragmatic tech choices.** Polars + Parquet + SQLite ("Litehouse") is a
   genuinely good fit for single-node analytics. Model2Vec (static, ~ms) for
   embeddings instead of a transformer — explicitly chosen so the hot path
   stays cheap and quality comes from over-clustering + curation.

7. **Fast iteration cadence with coherent themes.** 101 commits in ~6 months,
   each recent commit is a complete vertical slice ("insights engine product
   overhaul", "supervised intent classification", "live WS action event stream",
   "reversibility-tiered approval"). No reverted/re-rolled work in git history.

---

## 3. Weaknesses & Risks

1. **External path dependency on a sibling repo.** `crates/brightflow-connect`
   depends on `longbow = { path = "../../../longbow" }` — a local sibling crate
   at `/home/jens/dev/longbow`. This is not published to a registry, so the
   workspace is not independently buildable/cloneable. (It *does* build —
   `cargo check` passes — but only on this machine layout.)

2. **Frontend has zero automated tests.** No vitest/jest/playwright in
   `package.json`; no `*.test.*`/`*.spec.*` files. The frontend is ~20.6k lines
   across 24 component directories with non-trivial state (15 Pinia stores,
   9 composables, WebSocket connection logic). All verification is manual via
   `npm run dev`.

3. **Test coverage is uneven across crates.**
   | Crate | Test files | Tests |
   |---|---|---|
   | engine | 50 | 289 |
   | api | 16 | 85 |
   | store | 3 | 17 |
   | llm | 2 | 11 |
   | cli | 0 | 0 |
   | connect | 0 | 0 |
   | core | 0 | 0 |
   | scheduler | 0 | 0 |

   `scheduler` (1k lines, runs connector syncs + auto-insights) and `connect`
   (the Longbow integration surface) have no tests. `store` has only 17 tests
   for 4.9k lines including 32 migrations.

4. **Doc drift.** `CLAUDE.md` instructs contributors to read
   `docs/supervised_topics.md` before touching `topic_enricher.rs`, `nlp/linear.rs`,
   or `predicted_label*` — **that file does not exist** in `docs/`. The
   institutional knowledge it's meant to encode is now only recoverable from
   git history (commit `4d9adcd "supervised intent classification"`). The
   README is also stale (lists `brightflow-insights` crate that doesn't exist;
   says scheduler is "not yet implemented" though it is integrated).

5. **Single-dataset assumption is leaking.** Frontend `CLAUDE.md` states
   "single dataset focus, one data source at a time," but recent work
   (sources, multi-table, `/:sourceId/:table` routes, per-source schedules)
   has clearly moved past this. The mental model in docs no longer matches the
   product, which will confuse new contributors.

6. **`brightflow-engine` and `brightflow-api` are large and flat.** Engine is
   22k lines; API is 19.4k lines. Many modules sit at the crate root with broad
   `#![allow]` blocks for cognitive_complexity/too_many_lines/indexing_slicing.
   These allows are reasonable per-module but accumulate — the lints that would
   force decomposition are disabled where the code is densest.

7. **Embedding backend is a single point of quality ceiling.** Only
   `PotionBase32M` (Model2Vec). The `EmbedderBackend` trait is plumbed for
   alternatives but none ship. Topic quality on long/semantic text may suffer
   and there's no fallback path short of code.

8. **No CI observable in-repo.** No `.github/workflows`, no `.gitlab-ci.yml`.
   Pre-commit hook exists (`scripts/install-hooks.sh`) but `cargo audit`,
   `cargo clippy`, frontend `check` are only run if a human remembers. The
   `.claude/scheduled_tasks.lock` suggests an external scheduler, not CI.

9. **Insights limitations are all *known unknowns* with no owners/dates.**
   `docs/insights_limitations.md` is excellent at listing them but there's no
   tracking of which are slated for resolution vs. accepted-as-is.

---

## 4. Options / Best-Practice Conventions Observed

- **Lakehouse-on-one-node:** SQLite (metadata, via sqlx+migrations) + Parquet
  (data, via Polars lazy) is an increasingly common Rust pattern; aligns with
  DataFusion/LanceDB-era thinking but stays simpler. Trade-off: no
  concurrency-safe multi-writer Parquet; ingestion is serialized through the
  store. Acceptable for the single-user/single-node thesis.
- **Action-bus curation model** mirrors Linear's AIG and is, as far as the
  review found, an original synthesis. The `Action` enum-as-single-source is
  the recommended pattern for human/AI parity.
- **ts-rs + schemars from one enum** is the current best practice for
  Rust↔TS↔LLM-schema and is correctly applied here.
- **Strict clippy at workspace level** is the modern Rust convention; this
  repo is on the stricter end (good).

---

## 5. Open Questions

1. **Is `longbow` intended to stay a local sibling, or be published/ vendored?**
   This blocks reproducible builds and any external contributor.
2. **What happened to `docs/supervised_topics.md`?** Was it deleted, or never
   written? The classifier head (`nlp/linear.rs`) is load-bearing for topic
   intent and the doc is the only onboarding path.
3. **Is the "single dataset" model in frontend `CLAUDE.md` still the goal, or
   has multi-source/multi-table won?** Docs should reflect reality.
4. **What is the deployment target?** Local single-user? Self-hosted
   multi-tenant? The auth + ingest + product-analytics modules suggest
   something bigger than "single dataset," but there's no deployment doc.
5. **Is there CI anywhere external** (the lock file hints at a scheduler), or
   is it all local hooks?
6. **Embedding strategy:** is a heavier backend (e.g. `fastembed-rs`/`ort`)
   planned, or is Model2Vec + curation the committed direction?
7. **Scheduler untested** — is it considered stable, or still experimental?

---

## 6. Recommended Next Steps (prioritized)

These are discussion points, not a plan — no work has been started.

1. **Restore/repair the documentation.** Re-create
   `docs/supervised_topics.md` (mine commit `4d9adcd`) or remove the
   `CLAUDE.md` reference. Refresh `README.md` (crate list, scheduler) and the
   frontend `CLAUDE.md` (single-dataset claim). Low effort, high leverage for
   any future contributor — including the agent itself.
2. **Add CI.** A GitHub Actions workflow running `cargo fmt --check`,
   `cargo clippy --workspace`, `cargo test --workspace`, `cargo audit`, and
   `npm run check`. Even a minimal one prevents regression of the discipline
   that currently lives only in a local pre-commit hook.
3. **Resolve the `longbow` dependency.** Either publish/vendor it, or document
   the sibling-repo checkout as a build prerequisite in the README.
4. **Frontend test foundation.** Introduce vitest + a thin smoke test per
   store (`stores/query.ts`, `stores/connection.ts`) and one Playwright
   happy-path. The WebSocket connection and query-builder state are the
   highest-risk untested surfaces.
5. **Tests for `scheduler` and `store` ingest paths.** These run unattended
   (post-sync auto-insights, incremental cursors) and silently corrupt data if
   wrong — exactly where regression tests pay off most.
6. **Decide on the embedding ceiling.** If Model2Vec is the committed
   direction, document the trade-off in `docs/insights_limitations.md` or a
   topics equivalent. If a heavier backend is wanted, the `EmbedderBackend`
   trait already anticipates it — it's a contained addition.
7. **Triage `docs/insights_limitations.md`.** Tag each item as accept / fix /
   investigate so the list becomes a roadmap rather than a graveyard.
