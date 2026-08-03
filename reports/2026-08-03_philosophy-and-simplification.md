# Deep Research — Philosophy, Adherence, and Simplification

**Date:** 2026-08-03
**Scope:** Strengths and weaknesses of the codebase; whether the project follows
its own stated philosophy; a concrete catalog of simplification opportunities
(deletions, duplication collapse, library defaults over hand-written code).
**Method:** Five parallel research passes (backend architecture, Rust
simplification sweep, frontend deep dive, conventions-adherence audit, git
history/trajectory), each grounded in grep-verified usage checks. Headline
claims (dead code, the NUL byte, dead debug facility) independently re-verified
before inclusion. No code was changed.
**Baseline:** HEAD `505218d`, 126 commits since 2026-02-03. Prior report:
`reports/2026-07-25_codebase-assessment.md`.

---

## 1. What happened since the last report

The 2026-07-25 assessment was committed into the repo (`d2c2a30`) and then
systematically executed: 25 commits across 2026-08-02/03, zero new product
features. All six `plans/2026-08-02_*.md` plans landed and their verification
claims hold up (tests 406 → 457/458; two latent bugs found *by writing the
tests*; auth hardening; supply-chain gate; docs-as-code inlining; dataset
identity fix). Status of the prior findings:

| 07-25 finding | Status today |
|---|---|
| longbow path dep | Unfixed, now documented in README (build prerequisite) |
| Frontend zero tests | Partially fixed: 3 files / 50 tests; runner wired into pre-commit |
| Uneven crate coverage | Partially fixed: api 85→118, store 17→32, scheduler 0→7; cli/connect/core still 0 |
| `docs/supervised_topics.md` missing | Fixed by inlining into module docs |
| Single-dataset claim in app CLAUDE.md | Fixed |
| engine/api large & flat | Unfixed (api grew slightly) |
| Single embedding backend | Unfixed, deliberately deferred |
| No CI | Unfixed by choice; pre-commit hook strengthened instead (repo is on Codeberg, not GitHub) |
| `insights_limitations.md` untriaged | Fixed by deletion — all 9 limitations inlined next to the code |

Git history shows essentially no instability: zero reverts, zero fixup/WIP
commits, no force-push artifacts, and a healthy self-audit pattern (the
module-doc backfill was followed by four "verify the headers, fix N" commits
and produced a *new rule* rather than more patching).

---

## 2. The philosophy, and whether the project follows it

Two philosophies coexist: a **product** philosophy
(`docs/human_ai_interaction.md`, `docs/ux-principles.md`) and a **development**
philosophy (root `CLAUDE.md` conventions: inline comments as primary docs,
diff-scoped unit tests, the prose staleness test).

### Product philosophy scorecard

| Claim | Verdict |
|---|---|
| "The engine is read-only, never mutates state" | **Substantially true.** 0 SQL writes in the engine (verified). The analysis half is pure; `AnalysisEngine::with_history` takes history as an argument. Exception the doc doesn't name: the *fitting* half writes versioned model artifacts to disk (`engine/src/enrichment/artifacts.rs:184-263`, `topic_enricher.rs:1095`). Defensible (refit-able caches), but not literally "never." |
| "One action bus; exceptions are explicit hard rules, not drift" | **True for the interpretation layer; drifting at the edges.** 17 action kinds, well enforced. But ~40 mutating routes exist and two bypasses hit state the bus also owns: `POST .../topics/recluster` performs the identical mutation as `Action::Recluster` with no log entry (`api/src/topics/handlers.rs:333` vs `:293`), and `PUT .../semantics/{col}` writes the same `column_semantics` table as `Action::SetKpi`/`SetColumnPolarity`, unlogged and non-undoable (`semantics/handlers.rs:139` vs `actions/handlers.rs:1257`). These are drift, by the doc's own definition. |
| "Everything logged, one feed — human, AI, background job" | **Partially true.** Curation, agent runs, and insights emit to the feed (11 emit sites). Connector syncs and enrichment runs live in their own tables with their own polling endpoints and never reach it. "Background job" is not yet on the feed. |
| "Rank by effect size with false-discovery control" | **Mismatch.** The engine uses Bonferroni (FWER) throughout (`null_models.rs:82`, `drivers.rs:8,151`); zero references to Benjamini–Hochberg/FDR. Bonferroni is stricter and defensible — but it isn't what the normative doc says. One of the two should change. |
| "Single typed Action = REST + TS + LLM schema" | **True and unusually well-enforced.** Adding an action has 5 edit sites; 3 are compiler-checked, a 4th is checked by a schemars-derived test (`actions/types.rs:648`) that derives ground truth from the enum itself. |
| "Offload UX details to Nuxt UI" | **Split.** Excellent where it matters most: zero hand-rolled overlays, `UCommandPalette` + `defineShortcuts` in a 51-line component, 5 `UContextMenu` sites with keyboard parity, 95% adherence to the hand-enforced UButton size rule. But forms and tables route around it: 4 raw `<select>`, 7 raw `<input>` (5 sharing a byte-identical 130-char class string with a hardcoded *blue* focus ring while the app's primary is purple), 45 raw `<button>`, 12 raw `<table>` including 8 clones of the same breakdown table. |

### Development philosophy scorecard

The clean result of the audit: **the conventions hold exactly as far as
mechanical enforcement reaches, and no further.**

| Rule | Adherence |
|---|---|
| `//!` module doc on every Rust file | **100%** (181/181) — and mostly substantive *why* docs, some exceptional (`ingest/identity.rs`, `auth/rate_limit.rs`, `topic_enricher.rs`) |
| `/** */` header on every frontend `.ts` | **100%** (46/46) |
| `/** */` header on `.vue` files | **13–18%** (16/91). `check-conventions.sh` greps `\.ts$` only; `.vue` files — 13.8k of the app's 19.7k hand-written lines — are invisible to it. The stated rule covers them; the script doesn't. |
| Comment truthfulness | Mostly high, with sharp failures: `stores/connect.ts:3-6` claims "deliberately holds no fetching logic" directly above a fetch function; `brightflow-app/CLAUDE.md:115` claims "No UBadge usages exist" while 9 files use it (one of which the same document cites); `path_guard.rs:8` asserts an absence ("nothing downstream re-checks it") — committed *hours after* the rule banning absence assertions. Store/connect crates never got a verification wave: 5 of 8 store module docs are one-line WHAT restatements, including the 2,188-line `db.rs`. |
| Unit tests co-located on pure logic | **~55%** of eligible Rust files. 458 tests total, deep where it matters (engine 289). Untested-and-pure standouts: `stats/significance.rs` (301 lines of p-values/correlation, zero tests), `analysis/tree.rs` (1,253), `state.rs` `cache_key()` (the dataset-identity fix itself). Frontend: 1 of 13 stores, 1 of 9 composables. |
| Prose staleness test | **Holds for plans/reports perfectly** (9/9 files have exactly one commit each — never edited after creation). The violations are precisely the three files the script exempts by name: `CLAUDE.md` (cli command list stale; core described as holding `TenantId`/`DatasetId`, which exist nowhere), `brightflow-app/CLAUDE.md` (actively stale, see above), `README.md` (currently accurate). The carve-out is exactly coextensive with the violations. |

**The structural lesson** (which the project has half-learned already — the
diff-scoped checker, the schemars manifest test): rules enforced by machine
hold at 100%; rules enforced by discipline decay within hours. The
absence-assertion committed one commit after the absence-assertion ban is the
cleanest possible demonstration.

---

## 3. Strengths

1. **Layering is real.** No cycles; engine has zero references to store/api/sqlx
   (verified by grep); api→engine coupling is thin and type-level (15 of 87
   files). `PostSyncHook` (`scheduler/lib.rs:36`) keeps the LLM dependency out
   of the scheduler via inverted control.
2. **The action bus is the best code in the repo.** Single enum → serde REST
   body + ts-rs frontend union + schemars LLM tool schema; reversibility
   tiering in 9 lines; idempotency via stored-outcome replay; completeness
   checked by a test that derives ground truth from the schema.
3. **Discipline that survives measurement.** `unwrap`/`expect`/`panic` denied
   workspace-wide with only 18 justified production exceptions (15 of them
   static regex initializers). Zero TODO/FIXME in 52k lines, backed by
   `todo = "deny"`. Zero reverts in 126 commits. Two latent bugs found by
   writing tests, not by users.
4. **Documented limitations instead of silent ones.** Statistical edges live
   next to the code that has them (`null_models.rs:10-17`: "design intent, not
   a measured guarantee"). Very few codebases write down where their statistics
   are weakest.
5. **The self-correction loop works.** Prior report → committed → six plans →
   executed → verified, in 26 hours of wall clock, with a follow-up self-audit
   that corrected 17 of its own doc headers.
6. **Pragmatic hand-rolls where justified.** `nlp/rng.rs` (deterministic by
   design, avoids `rand`), `system/proc.rs` (`sysinfo` doesn't expose
   `RssAnon`), `auth/rate_limit.rs` (token-bucket rationale documented),
   `brightflow-llm` (~310 impl lines; `async-openai` would add a large dep tree
   to replace code that deliberately omits 90% of that API). These were checked
   and should stay.

## 4. Weaknesses

1. **`longbow = { path = "../../../longbow" }`** — the workspace still does not
   build from a fresh clone. Highest-severity structural issue, unchanged since
   07-25.
2. **Enforcement gaps map exactly onto quality gaps** (§2). `.vue` headers,
   store/connect doc quality, the CLAUDE.md carve-outs — each unenforced zone
   is a decayed zone.
3. **Engine→api error boundary is stringly-typed.** `From<anyhow::Error> for
   AppError` maps everything to `ANALYSIS_ERROR` 500 (`shared/error.rs:141`);
   the engine's five typed error enums all arrive as strings. User-input errors
   are indistinguishable from server faults at the API. Also: `into_response()`
   and `error_code()` are two hand-maintained 13-arm matches nothing keeps in
   sync.
4. **Persistence is not where the map says it is.** Four SQLite databases,
   migrations in three crates, api owning three of five persistence surfaces,
   plus a hand-rolled `CREATE TABLE` outside any migration system
   (`ingest/buffer.rs:22`).
5. **The complexity lints are off where the code is densest.**
   `cognitive_complexity` + `too_many_lines` are crate-root-disabled in 5 of 8
   crates; `clippy.toml`'s thresholds are aspirational. Densest offenders:
   `analysis/tree.rs` (443 lines across two render functions), `cli/main.rs`
   (1,629-line single file, 30 touches, 0 tests), `store/db.rs` (2,188 lines,
   113 methods), `state.rs` (22-field AppState, four hand-copied constructors).
6. **Frontend data layer has two competing paradigms.** @pinia/colada (38
   `useQuery`) vs six stores hand-rolling `loading`/`error` refs; worst case is
   `AppSidebar.vue:27-39`, where a colada query function mutates a Pinia store,
   so the source list lives in two places.
7. **A NUL byte is committed in `useTextExplore.ts:80`** (`.join('\x00')` as a
   cache-key separator, presumably meant to be a visible sentinel). Functionally
   harmless — but grep/ripgrep classify the file as *binary and skip it*, so
   every text-search-based audit of this repo has silently ignored that file.
8. **A permanent phantom diff.** `vp dev` regenerates `auto-imports.d.ts` /
   `components.d.ts` without semicolons; `vp check --fix` adds them back;
   `fmt.ignorePatterns` covers neither file. 72 lines of churn on every dev
   session (this is the current `git status` noise).

---

## 5. Simplification catalog

All items grep-verified; the highest-impact ones re-verified independently.
"Risk" reflects blast radius, not confidence.

### 5.1 Verified-dead code — delete (~2,550 lines total)

**Backend (~1,245 lines):**

| What | Where | Lines | Notes |
|---|---|---|---|
| Entire `DebugLog` facility | `engine/src/debug.rs` + 31 call sites + 6 `debug:` params | ~250 | `to_file()` (the only enabling constructor) has zero callers; all 9 sites pass `disabled()`. `tracing` covers the need if it returns. |
| Dead sparse-TF-IDF enrichment path | `engine/src/nlp/polars/{enrichment,transform,centroids,serde_utils}.rs` | ~968 | Every export has zero references outside the subtree; superseded by dense Model2Vec in `topic_enricher.rs`. Note: overlaps partially with the CLI `insights` CSV path — verify `tfidf` exports aren't used there before deleting wholesale. |
| 4 dead period-comparison fns (+1 orphaned) | `engine/src/analysis/period.rs:133,176,221,292` | ~280 | `_cached`/non-cached pairs are line-for-line duplicates; none called. |
| Dead ASCII-tree renderer | `engine/src/output/markdown.rs:133-274` | ~145 | `write_markdown_compact` + its only helper. |
| Dead AppState surface | `api/src/state.rs:118,211,310,472,491,522` | ~130 | 2 of 4 constructors, `get_or_build_schema`, `load_all_store_tables`, `unload_store_tables`, `table_exists` — all zero callers. |
| Legacy TOML schema-config island | `engine/src/data/config.rs` (`SchemaConfig` etc.) + `schema.rs:34` | ~110 | Closed island; engine doesn't even depend on `toml`, so the config can't be loaded. Keep `ColumnRole`/`Polarity`/`TimeGranularity`. |
| Dead DB methods & HTTP handlers | see below | ~340 | |
| Dead error variants | `BrightflowError::{DatasetNotFound,Storage,Serialization,Io}`, `StoreError::{InvalidTableName,SchemaMismatch}`, `IngestError::UnknownDomain`, `SubtextError::{NotFitted,InvalidParameter}`, `TopicError::{Embedder,Artifact}` | ~40 | Same class as the already-removed `GenerationMismatch`/`UserNotFound`. `TopicError` variants are `#[from]` — confirm no `?` conversion first. |

Dead DB methods / handlers detail: `StoreDb::{delete_table_settings, get_all_enrichment_settings, delete_enrichment_settings, get_enrichment_function_by_name}`, `IngestDb::cleanup_old_salts` (dead — or a missing-feature bug: salts currently grow forever), `SchedulerDb::get_sync_state`, api scheduler handlers `list_sync_runs`/`get_sync_run`/`get_sync_state`/`create_job`/`list_jobs`/`get_job`/`trigger_run`, connect handlers `list_connectors`/`run_preset`/`schedule_preset` (the `api.ts:209` comment calling name-based endpoints "legacy" has it backwards — the preset ones are dead), `cache.rs:52 dimension_coverage_in_period`, `classification_metrics.rs` `macro_precision`/`macro_recall`, `drivers.rs:97`, `null_models.rs:185,221`, `FilePartitionRow`, `RunRequest`, CLI `Commands::Schedule` (prints "moved to API server"). *Caveat: route removals assume no external script hits them — the frontend doesn't.*

**Frontend (~1,000 lines):**

| What | Where | Lines |
|---|---|---|
| `AnalyticsView.vue` (pre-SourceLayout monolith; its web tab duplicates `WebDashboard.vue` query-for-query) | `components/analytics/` | 455 |
| `ConnectorCard.vue` | `components/connect/` | 231 |
| `DatasetPickerModal.vue` | `components/layout/` | 166 |
| `stores/connect.ts` (dead store — deleting it also removes the repo's one outright-false module header) | `stores/` | 54 |
| `semanticsApi` (6 methods, all `Promise<unknown>`, zero callers) + `datasetApi.upload` | `services/api.ts:141-170,287-305` | ~51 |
| ~40 dead store members (getters/actions returned but read by nothing) across `dataset`, `results`, `pivot`, `connection`, `query` (the never-built `aggregations` UI), `insightsActivity`, `ui`, `source`, `insights` | stores | ~150 |
| `execute()` legacy alias; unreachable `messageQueue` in `websocket.ts`; empty `components/inputs/` dir | misc | ~20 |

### 5.2 Duplication to collapse (~475 lines)

Backend: `dtype_to_string` ×3 in one module tree (`analytics/{handlers,session,executor}.rs`); `now_epoch()` ×4 (or just `chrono::Utc::now().timestamp()` — chrono is already a dep); dense `dot()` ×4 in the engine (`nlp/similarity.rs` is the natural home; pick `mul_add` deliberately — FP-observable); `web_analytics`/`product_analytics` share ~80 lines (`resolve_dates`, `default_period`, `scan_source_events`); `topics/handlers.rs:683-712` ≡ `textexplore/handlers.rs:202-231` byte-identical (`read_*_at`, `derive_title`); `ParquetStore` repeats an 8× table-resolution preamble (`store/lib.rs`); `agent/runner.rs:205` hand-rolls a second retry when `brightflow-llm` already exports `chat_with_backoff`; `AppError::into_response`/`error_code` twin 13-arm matches → one `parts()` fn; `Action::scope()` 54-line match over 17 variants all carrying identical `source_id`/`table` fields → flattened `Scope` struct; `ColumnRole` string→enum mapping ×3 (`state.rs:554`, `actions/handlers.rs:1316`, `semantics/handlers.rs:325`); the 25-line semantics cache-invalidation block ×3.

Frontend: `stores/system.ts:34-135` is a second from-scratch WebSocket client (same URL derivation, same backoff formula inlined, no heartbeat, magic-number stop) — `WebSocketClient` needs only a `path` option (−90 lines, gains heartbeat); identical 300ms debounce blocks in `QueryBuilder.vue:119-141` and `FilterBar.vue:36-51` (neither clears its timer on unmount); `isActionEvent` defined byte-identically twice (`curation.ts:28`, `insights.ts:101` — the latter's "(same as curation store)" comment is itself a rule-1 violation); filter→operation mapping ×3 (only the `query.ts` copy is tested); two multipart-upload fns bypassing `request()` and its 401 handling; 4 ECharts registration sites; `DataTable.vue` `formatCell` re-implements the tested `formatNumber`; 8 clones of a 2-column breakdown table in `AnalyticsView`/`WebDashboard` → one `BreakdownTable.vue` (~160 lines).

### 5.3 Library defaults over hand-written code

| Hand-rolled | Replace with | Where |
|---|---|---|
| RFC3339 timezone stripping via `rfind` + byte slicing (panic risk on multi-byte input) | `chrono::DateTime::parse_from_rfc3339` (already used correctly at `product_analytics/queries.rs:416`) | `engine/analysis/period.rs:551-606`, −25 lines |
| IPv4 parsing via `split('.').filter_map` ×2 (accepts `"1.2.3.4.5.6"`) | `str::parse::<Ipv4Addr>()` | `api/lib.rs:91`, `cli/main.rs:752` |
| `once_cell` crate (3 uses) | `std::sync::LazyLock`/`OnceLock` (already used in 5 places) | drop the dep |
| localStorage get/validate/watch boilerplate (~40 lines in `ui.ts` alone), debounce watchers, 2s/3s polling loops, clipboard calls, `getComputedStyle` caching | `@vueuse/core` — **already in node_modules** as a Nuxt UI dependency; declaring it costs zero bundle weight | ~120 lines across 10 files; `useCssVar` also fixes `useChartColors`' theme-reactivity gap |
| Raw `<select>` ×4, `<input>` ×7, password-reveal widget, duplicated inline-rename widget, segmented-control `<button>` strips | `USelect`/`UInput`/`UTabs`/`UButtonGroup` (all already in use elsewhere) | fixes the blue-focus-ring-on-purple-theme inconsistency for free |
| Inline `text-red-500` error divs (88 raw palette classes total) | `useToast` (used once today) + the semantic tokens `miami.css` already defines | |

**Checked and deliberately kept:** `brightflow-llm` (vs `async-openai`), `nlp/rng.rs` (vs `rand`), `auth/rate_limit.rs` (vs `tower-governor`), `system/proc.rs` (vs `sysinfo`), the scheduler's `${VAR}` expander (self-reference termination `shellexpand` doesn't give), `EmbedderBackend` trait (one impl, but `EmbedderId::name()` is persisted in settings and the module header argues its case — collapsing saves only ~70 lines).

### 5.4 Dependency and config trimming

- `brightflow-core` declared but **never referenced** by `store` and `engine` — delete both edges; the true graph shows engine/store as leaves.
- `BrightflowError`: one consumer (`connect`), one variant used (`Other`), always via `map_err`. Delete it; `brightflow-core` becomes a zero-dep paths crate (drops `serde_json` + `thiserror`). CLAUDE.md's description of core (`TenantId`, `DatasetId`) describes types that don't exist — fix the sentence.
- `dotenvy` unused in `api`; `password-hash` redundant (argon2 re-exports it with the needed feature); polars `json` feature has no reader/writer usage.
- Frontend: `@fontsource-variable/outfit` — zero references, delete. `@lucide/vue` — fully redundant with `@iconify-json/lucide` + `UIcon`, removable after a mechanical 25-file rewrite. `--font-mono: 'JetBrains Mono'` names a font that is never loaded.
- `types/generated/index.ts` is **hand-written inside the generated dir** (non-ts-rs banner, manual ordering): 21 generated types unexported, and `types/index.ts:45-73` + `types/enrichment.ts` hand-mirror types that sit generated-but-unexported on disk. Generate the barrel; delete the mirrors; `ToolId` becomes `SourceTool | 'settings'`.

### 5.5 Rollup

| Category | Est. lines | Risk |
|---|---|---|
| Verified-dead backend code | ~1,245 | low |
| Verified-dead frontend code | ~1,000 | low |
| Duplication collapse | ~475 | low |
| Library-default replacements | ~170 | low |
| Deps removed | 6 (+1 polars feature, 2 phantom crate edges) | low |
| **Total** | **~2,900 lines (~4% of hand-written code)** | **nothing above low** |

---

## 6. Prioritized discussion list

Quick wins (mechanical, near-zero risk):
1. Fix the NUL byte (`useTextExplore.ts:80`) — one character; restores the file to searchability.
2. Add `auto-imports.d.ts`/`components.d.ts` to `fmt.ignorePatterns` — kills the permanent phantom diff.
3. The dead-code deletions (§5.1) — ~2,250 lines, all grep-verified.
4. Remove the 2 phantom `brightflow-core` edges + unused deps.

Structural (each a real decision):
5. Close the two action-bus bypasses (recluster, semantics PUT) — this is philosophy enforcement, not just cleanup; also removes the extractor-manufacturing layering inversion inside `execute_action`.
6. Extend `check-conventions.sh` to `.vue` — the audit shows enforcement is the only thing that holds a rule here.
7. Reconcile Bonferroni vs the "false-discovery control" claim in the normative doc — change the doc or the method.
8. Typed engine→api errors (or at minimum stop mapping all `anyhow` to 500).
9. Pick one frontend data paradigm (colada everywhere, or stores everywhere) — the AppSidebar dual-ownership is the forcing case.
10. Decide `longbow`: vendor, publish, or accept the documented sibling-checkout forever.

Explicitly not recommended: replacing `brightflow-llm`, the rate limiter, the RNG, or `proc.rs` with dependencies (all four hand-rolls are justified and documented); collapsing the `EmbedderBackend` trait (persisted-name coupling makes it not-free, savings trivial).
