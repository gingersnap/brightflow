# Insights Engine — Research Notes

Background research synthesized while designing and building out the Brightflow insights engine. Captures: what we have, what the field looks like (commercial + research + OSS), where Brightflow sits in that landscape, and the design choices that emerged.

---

## 1. Brightflow's insights engine — current state

### Architecture

Three surfaces driven by one engine:

- **CLI** — `crates/brightflow-cli/src/main.rs` `insights` subcommand (review / trends / drivers / all). Currently CSV-only via `--input`.
- **API** — `crates/brightflow-api/src/insights/handlers.rs` reads from the parquet store via `resolve_dataset()`, runs the engine in a `spawn_blocking`, returns JSON.
- **Frontend** — `brightflow-app/src/components/insights/` (`InsightsView`, `InsightsPanel`, `InsightCard`) backed by `stores/insights.ts`.

Engine layout in `crates/brightflow-engine/src/analysis/`:

```
analysis/
├── tree.rs         — AnalysisTree, AnalysisNode, AnalysisType (13 variants)
├── engine.rs       — AnalysisEngine, ColumnCache, task queue
├── scoring.rs      — calibrated interestingness (W_SIG/EFFECT/SURPRISE × kpi_boost)
├── dedup.rs        — lattice collapse + story grouping
├── anomaly.rs      — z-score
├── trend.rs        — linear regression
├── correlation.rs  — Pearson
├── seasonality.rs  — autocorrelation at fixed lags
├── period.rs       — period-over-period + period anomaly
├── segment.rs      — Welch's t-test attribution
├── outlier_cluster.rs — multi-column simultaneous outliers
├── forecast.rs     — linear extrapolation + 95% prediction interval
├── concentration.rs — HHI / Gini / top-N share
├── distribution_shift.rs — two-sample KS test
├── membership.rs   — added/removed dimension values between periods
└── change_point.rs — CUSUM-based level shift detection
```

### Detector taxonomy (13 types)

| Type | Method | Effect-size proxy |
|---|---|---|
| Anomaly | z-score on latest value vs historical | `\|z\| / 5` |
| PeriodAnomaly | Welch's t-test, period vs others | `\|change_pct\| / 100` |
| PeriodComparison | Welch's t-test, current vs previous | `\|change_pct\| / 100` |
| Trend | Linear regression, slope + R² | `R² × 0.7 + tanh(\|slope\|) × 0.3` |
| Seasonality | Autocorrelation at 7/14/30/90/365 lags | `\|autocorrelation\|` |
| Correlation | Pearson r between numeric columns | `\|r\|` |
| Forecast Deviation | Linear fit + 95% PI extrapolation | `\|deviation_pct\| / 100` |
| Outlier Cluster | 2+ columns with same-direction outliers in same period | `cluster_size / 5` |
| Segment | Welch's t-test per dimension value, ranked by contribution | `max(contribution_pct, change_pct/2) / 100` |
| Concentration | HHI / Gini / top-N share per (measure, dimension) | `max(HHI, top_share/100)` |
| Distribution Shift | Two-sample KS test between consecutive periods | `KS_statistic` |
| Membership Change | Set diff of dimension values between periods | `(added + removed) / 10` |
| Change Point | CUSUM with relative-shift threshold | `\|after - before\| / before` |

### Calibrated scoring

Single-scalar interestingness combining three normalized components:

```
score = (W_SIG · sig + W_EFFECT · effect + W_SURPRISE · surprise) × kpi_boost
```

Constants (calibration knobs, hardcoded today in `scoring.rs`):

- `W_SIG = 0.30`, `W_EFFECT = 0.45`, `W_SURPRISE = 0.25`
- `KPI_MULTIPLIER = 2.5`
- `DEFAULT_MIN_EFFECT = 0.05` (effect-size floor, drops findings below this)

`ScoringContext` has a `kpi_columns: HashSet<String>` field that's currently always empty — the per-column `is_kpi` flag exists in the semantic layer but isn't wired through.

### Dedup architecture

Two passes after scoring, before final ranking (`dedup.rs`):

1. **Lattice collapse** — child finding survives only if its score ≥ 1.2× parent's (Adtributor-style succinctness penalty). Multiplier is a calibration knob.
2. **Story grouping** — roots sharing a target column merge under the highest-scoring root; others become children. Result: only one root per metric per run.

### UI surface

Recursive root-cause drill-down tree (the differentiator) with per-type ECharts renderers in `brightflow-app/src/components/insights/renderers/`:

- `AnomalyRenderer` — line + ±2σ band + marker
- `PeriodAnomalyRenderer` — bar series, anomalous period highlighted
- `TrendRenderer` — line + regression overlay + R² annotation (also reused for ChangePoint)
- `SeasonalityRenderer` — smoothed line
- `PeriodComparisonRenderer` — paired bars
- `SegmentRenderer` — horizontal contribution bar with sign-aware color
- `CorrelationRenderer` — scatter + fit line + r annotation
- `ForecastRenderer` — line + prediction interval cone + actual marker
- `OutlierClusterRenderer` — sparkline grid (one per affected column)
- `ConcentrationRenderer` — Lorenz curve vs equality reference
- `DistributionShiftRenderer` — overlaid histograms (previous vs current)
- `MembershipRenderer` — added / removed value lists

`InsightsPanel` shows top 5 + "show more" reveal, faceted filters (type chips, direction, measure dropdown, min-score slider). Each card has "Why this finding?" expander showing the 4-component score breakdown, and "Open in Explore" that translates the node's `filter_chain` into the query store's `addFilter` operations and routes to the explore view.

### Configuration layers (current)

- **Pipeline-level**: hardcoded constants in `scoring.rs` + per-detector module thresholds
- **Per-table semantic layer**: `crates/brightflow-api/src/semantics/` — per-column role / is_kpi / label, per-table display_name / time_granularity / comparison_periods. Stored in SQLite, loaded into in-memory caches on startup.
- **Per-source / per-connector-type**: nothing exists yet.
- **Request-level overrides**: API accepts `EngineConfig { z_threshold, p_threshold, min_effect_size, max_results, max_depth }`. Scoring weights are not yet exposed.

---

## 2. The 2026 landscape — commercial augmented analytics

### Architectural convergence

Almost every serious 2026 product uses the same three-layer pattern:

1. **Curated semantic layer** (LookML, Spotter Semantics, Cortex Semantic Views, Genie spaces, Pulse metric definitions). Looker reports two-thirds LLM error reduction from grounding alone.
2. **Deterministic statistical detection** of typed insights.
3. **LLM-as-narrator** over the stats — sometimes LLM-as-router picking which analysis to run.

[Tableau Pulse states this explicitly](https://www.tableau.com/blog/tableau-pulse-and-tableau-ai): stats are ground truth, LLM only writes prose. Almost nobody is letting the LLM do the actual analysis.

### The reference taxonomy

Microsoft Research's [QuickInsights paper (SIGMOD 2019)](https://www.microsoft.com/en-us/research/publication/quickinsights-quick-and-automatic-discovery-of-insights-from-multi-dimensional-data/) defined the canonical 8 types — **Top one, Attribution, Change point, Outlier, Trend, Correlation, Cross-measure correlation, Clustering** — and Power BI ships them.

[Tableau Pulse](https://help.tableau.com/current/online/en-us/pulse_insights_platform_insight_types.htm) extended it with the most interesting additions: **Pace to Goal, Concentrated Contribution Alert (when few members make up >50%), Trend Change Alert, Top Detractors** (dimensions moving *against* the metric), **Goal/Threshold Breakdown**.

Mapped to Brightflow's 13 types: heavy overlap on Outlier, Attribution, Trend, Correlation, Period comparison, Forecast, Concentration. Missing: **Top one, formal Clustering as insight type, Pace to Goal, Top Detractors as a distinct ranking dimension**.

### Commercial product deep-dives worth reading

**ThoughtSpot SpotIQ + Spotter** ([SpotIQ glossary](https://www.thoughtspot.com/glossary/SpotIQ); [Introducing Spotter](https://www.thoughtspot.com/blog/introducing-spotter-ai-analyst))
- SpotIQ runs ensemble of z-scores, cross-correlation, linear regression, k-means; ranks with a usage-based ML model that ingests user thumbs-up/down feedback
- Spotter's "BARQ" pattern: LLM doesn't write SQL, it translates NL into ThoughtSpot's "search tokens" (a controlled taxonomy derived from the data); the relational engine executes the analysis

**Tableau Pulse + Explain Data + Einstein Discovery**
- [Pulse insight types](https://help.tableau.com/current/online/en-us/pulse_insights_platform_insight_types.htm) — most clearly documented modern insights platform
- Pulse explicitly restricts analysis to dimensions/measures referenced in the analyst-curated metric definition
- [Tableau Explain Data](https://help.tableau.com/current/server/en-us/explain_data_explained.htm) uses Bayesian models, trades off complexity vs variance explained: "better explanations are simpler than the variation they explain"
- [Einstein Discovery Stories](https://www.biztory.com/blog/understanding-einstein-discovery-stories-in-einstein-analytics) structure: **what happened / why / what will happen / what to do**. Uses linear regression by segment with explicit second-order interaction terms

**Power BI** (most transparent commercial implementation)
- [Key Influencers](https://learn.microsoft.com/en-us/power-bi/visuals/power-bi-visualization-influencers) — L-BFGS logistic / SDCA linear regression via ML.NET, Wald test with p<0.05
- [Anomaly Detection](https://learn.microsoft.com/en-us/power-bi/visuals/power-bi-visualization-anomaly-detection) — uses SR-CNN (Spectral Residual + CNN), originally a computer vision algorithm
- [Decomposition Tree](https://learn.microsoft.com/en-us/power-bi/visuals/power-bi-visualization-decomposition-tree) — interactive drill where AI suggests next dimension to split

**Snowflake Cortex Analyst** ([behind the scenes](https://www.snowflake.com/en/engineering-blog/snowflake-cortex-analyst-behind-the-scenes/))
- Multi-LLM ensemble for SQL generation + error-correction loop + synthesizer agent
- >90% SQL accuracy reported

**Microsoft InsightPilot (EMNLP 2023)** ([paper](https://ar5iv.labs.arxiv.org/html/2304.00477))
- LLM-as-router picking among Understand / Summarize / Compare / Explain — dispatches to QuickInsight / MetaInsight / XInsight engines
- Explicit prior art for "LLM orchestrates over deterministic engines"

### Anomaly-detection-first products

| Product | Method | Notable |
|---|---|---|
| [Anodot](https://www.anodot.com/blog/building-time-series-anomaly-detection/) | Per-series model classification + ensemble | ACF eats 66% of compute, XGBoost 50% of forecasting |
| [Anomalo](https://www.anomalo.com/blog/unsupervised-data-monitoring/) | Unsupervised ML on table representations | Two algorithm variants: NULL-rate-constrained vs full distributional |
| [Bigeye Autothresholds](https://docs.bigeye.com/docs/autothresholds) | Classical statistical forecasting | Most openly documented; requires 3+ cycles of seasonality |
| [Metaplane](https://www.metaplane.dev/platform/anomaly-detection) | Metadata-only — never reads raw data | Now part of Datadog |
| [Lightup](https://lightup.ai/anomaly-detection) | Pushdown architecture, warehouse runs the checks | Auto-selects algorithm per indicator |
| [Monte Carlo](https://www.montecarlodata.com/blog-data-quality-anomaly-detection-everything-you-need-to-know/) | ML-derived dynamic thresholds on metadata only | Lineage-aware root cause |

**Critical observation**: none of these do segment-level attribution the way Brightflow does (Welch's t-test on dimension cuts). They answer "did this metric move?" but punt "why" to lineage graphs and column-level metadata, not to dimensional segmentation. The "why" side is dominated by Microsoft Research's Adtributor lineage.

### The Adtributor → HotSpot → Squeeze research lineage

Most directly relevant body of work for multi-dimensional root cause attribution. Almost no commercial BI product implements it properly.

- **[Adtributor (NSDI 2014)](https://www.usenix.org/conference/nsdi14/technical-sessions/presentation/bhagwan)** — seminal paper. Three criteria: **Explanatory Power × Succinctness × Surprise** (JS divergence). Adding "surprise" lifted accuracy 20% → 95%.
- **[HotSpot (IEEE Access 2018)](https://netman.aiops.org/wp-content/uploads/2018/12/sunyq_IEEEAccess2018_HotSpot.pdf)** — extends Adtributor to additive KPIs with combinatorial dimension interactions via Monte Carlo Tree Search.
- **[Squeeze (ISSRE 2019)](https://netman.aiops.org/~peidan/ANM2022/8.AnomalyLocalization/LectureCoverage/2019ISSRE_Squeeze.pdf)** — currently best-in-class on F1.
- **[CMMD (KDD 2022)](https://arxiv.org/abs/2203.16280)** — extends to cross-metric (metric A moved because metric B moved).
- **[RiskLoc (2022)](https://arxiv.org/pdf/2205.10004)** — weighted-risk reformulation that's faster and more accurate on complex anomalies.

Brightflow's current "biggest contribution × Welch t-test" is closer to the pre-Adtributor baseline. The **succinctness penalty** in our `dedup.rs` is the closest borrow.

### Open-source statistical libraries

Most of the OSS in this space is libraries, not products:

- **[Merlion (Salesforce)](https://arxiv.org/abs/2109.09265)** — 5-layer modular: data → modeling → post-processing → ensembling → evaluation. Score calibration as an explicit stage.
- **[Kats (Meta)](https://engineering.fb.com/2021/06/21/open-source/kats/)** — has meta-learning that recommends an algorithm for your dataset.
- **[PyOD](https://github.com/yzhao062/pyod)** — 60+ outlier detectors, unified API.
- **[Twitter S-H-ESD](https://blog.x.com/engineering/en_us/a/2015/introducing-practical-and-robust-anomaly-detection-in-a-time-series)** — canonical seasonal+trend baseline.
- **[Prophet](https://facebook.github.io/prophet/)** — additive trend + Fourier seasonality + holidays.

---

## 3. LLM-augmented analytics (2024–2026 patterns)

Three patterns observed:

1. **LLM-as-narrator over deterministic analysis** — Tableau Pulse, Power BI Smart Narratives, Sisense Narratives. Stats are ground truth, LLM writes prose. Safest, most common.

2. **LLM-as-router / agent picking which analysis to run** — Microsoft InsightPilot (Understand/Summarize/Compare/Explain), Snowflake Cortex Analyst (multi-LLM SQL ensemble), Databricks Genie (compound AI system, semantic-layer-grounded), ThoughtSpot Spotter (BARQ + agentic MCP server).

3. **Fully agentic data exploration** — mostly research:
   - ["LLM/Agent-as-Data-Analyst: A Survey" (2025)](https://arxiv.org/abs/2509.23988)
   - ["LLMs on Tabular Data" (Amazon, 2024)](https://arxiv.org/html/2402.17944v1) — best overview of tabular+LLM intersection
   - ["An LLM-Based Approach for Insight Generation" (2025)](https://arxiv.org/abs/2503.11664) — Hypothesis Generator + Query Agent + Summarization

**Brightflow's stance**: LLM is off the path for analysis — the philosophy is *thousands of cheap deterministic analyses ranked by interestingness, beats LLM-as-analyst*. The Rust+Polars compute model makes this competitive. LLM may return later for narration only.

---

## 4. Brightflow vs the field

### Engine capability vs commercial baselines

Roughly **40-50% of competitor capability** on the analysis side. Strengths and gaps:

| Strength | Where Brightflow is competitive or ahead |
|---|---|
| Welch's t-test segment attribution | More rigorous than the "biggest delta = top driver" most commercial products ship |
| Multivariate outlier clusters | Unusual — most products do per-metric anomalies and stop there |
| Forecast deviation as first-class insight type | Rare; competitors mostly show forecasts as charts, not ranked findings |
| Calibrated cross-type ranking | No OSS reference exists; commercial products don't expose their formula |
| Recursive root-cause drill-down tree | The chosen differentiator; no OSS equivalent |

| Gap | What's missing |
|---|---|
| LLM narration | None today (deliberate) — every commercial product has this |
| Pre-Adtributor attribution | No surprise-weighting or formal succinctness; the lineage has moved on |
| Per-series adaptive anomaly | Hardcoded z=2.0 vs Anodot's per-series model classification or Power BI's SR-CNN |
| Score calibration as explicit stage | Merlion separates this; Brightflow inlines it |
| Personalization / feedback loop | SpotIQ, Pulse, Anodot all bias rankings on user thumbs |
| Persistence | Every run ephemeral — no scheduled generation, no diff-vs-last-run, no surprise baseline beyond within-run |
| Streaming | Full DataFrame in memory |

### UI capability vs commercial baselines

Started at **~10% of competitors** (just text cards), now at **~50%** after the build (per-type renderers, top-N feed, filters, drill-down with score breakdown, Open in Explore bridge).

Competitor reference: Tableau Pulse, Power BI Decomposition Tree, Einstein Discovery Stories, ThoughtSpot SpotIQ boards. The remaining gap is mostly polish (per-renderer iteration on real data), persistence-driven UX (subscriptions, digests, "what changed since I looked"), and narrative paragraphs.

---

## 5. Open-source landscape (the real gap)

Targeted research after noticing earlier passes focused on commercial tools and libraries.

### The closest OSS equivalent is template-driven, not statistical

**[Metabase X-Rays](https://github.com/metabase/metabase/tree/master/src/metabase/xrays/automagic_dashboards)** — only OSS tool that auto-generates analytics from a table. But it's **YAML template-driven** with hand-coded `score: 0–100` per template entry. No statistical interestingness. Worth studying for the clean 4-stage pipeline (templates → grounding → combination → population) and for `interesting.clj` / `combination.clj` / `populate.clj`. Auto-generated dashboard, not ranked feed. AGPL-3.0.

### Newer OSS BI tools have all picked the LLM-on-semantic-layer pattern

[Lightdash](https://www.lightdash.com/ai-agents), [Evidence](https://github.com/evidence-dev/evidence), [Rill](https://docs.rilldata.com/notes/0.77) ("Explain Anomalies"), [Cube](https://cube.dev/blog/semantic-layer-and-ai-the-future-of-data-querying-with-natural-language), [Briefer](https://github.com/briefercloud/briefer) — all converged on LLM-agent-on-semantic-layer. None do deterministic statistical fan-out. Brightflow's direction is contrarian to the entire new-OSS-BI cohort.

[Apache Superset](https://github.com/apache/superset) has nothing in this space — pure user-driven dashboarding + SQL-condition alerts.

### The gold find for algorithms

**[shaido987/riskloc](https://github.com/shaido987/riskloc)** — single MIT repo with clean Python implementations of **Adtributor, R-Adtributor, HotSpot, Squeeze, RobustSpot, AutoRoot, RiskLoc** all behind one `run.py`. Reference implementation to read for next-iteration of dedup work.

Also: [NetManAIOps/Squeeze](https://github.com/NetManAIOps/Squeeze) (canonical Tsinghua impl) and [PSqueeze](https://github.com/NetManAIOps/PSqueeze) (probabilistic extension), and [Salesforce PyRCA](https://github.com/salesforce/PyRCA) (broader RCA, more focused on causal-graph methods).

### The polished single-type OSS UI

**[OpenSearch Anomaly Detection](https://github.com/opensearch-project/anomaly-detection)** + [dashboards plugin](https://github.com/opensearch-project/anomaly-detection-dashboards-plugin) — Random Cut Forest with feature-contribution attribution. Most polished single-finding-type UI in OSS. Worth studying:

- Live-strip + history-overlay pattern
- Confidence ribbons separate from score (don't collapse)
- Per-feature contribution sparkbars
- `anomaly_grade` and `confidence` exposed as separate visual layers

Apache-2.0.

### Distribution-shift detection with a real UI

**[EvidentlyAI](https://github.com/evidentlyai/evidently)** — Apache-2.0. Implements Brightflow's distribution-shift detector almost exactly (KS / chi-squared / Wasserstein / KL) but with much more polished per-feature interactive HTML reports.

### Other notable OSS

- **[Yahoo Sherlock](https://github.com/yahoo/sherlock) + [EGADS](https://github.com/yahoo/egads)** — anomaly detection on Druid. Per-series model-selection registry pattern.
- **[Soda Core](https://docs.soda.io/soda-cl/anomaly-detection.html)** — uses Prophet under the hood.
- **[Kanaries RATH](https://github.com/Kanaries/Rath)** (AGPL) — closest "augmented analytics"-branded OSS tool. Their **Data Painter** UI (paint a region of a chart to define a segment, then auto-compare) is genuinely novel UX worth borrowing.
- **[Great Expectations](https://docs.greatexpectations.io/docs/reference/learn/data_quality_use_cases/distribution/)** — KL-divergence and z-score expectations.
- **[grafana/promql-anomaly-detection](https://github.com/grafana/promql-anomaly-detection)** — recording-rules-only. Real Grafana ML is cloud-only / closed.

### The genuine OSS white space

After the dive, the gap is precise — **none of these exist openly**:

1. A deterministic, multi-detector, calibrated-ranked, feed-first auto-insight engine
2. Multi-detector ensemble orchestration with a shared scoring scale
3. Functional-dependency-based EII elimination (described in QuickInsights, implemented openly nowhere)
4. Interactive recursive root-cause drill-down (all OSS RCA libraries are batch CLIs)
5. Calibration across detector families — turning p-values, effect sizes, KL divergences, CUSUM scores into one ranking

The deterministic-fan-out + calibrated-ranking + feed-first mental model has no OSS reference implementation. Brightflow is building in genuine OSS white space.

---

## 6. Cross-cutting UI patterns worth considering

Patterns that show up in multiple OSS tools and are worth borrowing:

1. **"Live" strip + history overlay** (OpenSearch) — short rolling window prominent at top, full history below
2. **Feature-contribution sparkbars** (OpenSearch) — stacked horizontal bar of % contribution per dimension when a finding is multi-feature
3. **Auto-grouped sections, not flat list** (Metabase, RATH) — findings clustered into named sections (Overview / ByTime / Geographic) before ranking
4. **"Compare to the rest" inline action** (Metabase) — one-click pivot from any value to its same-dimension peers
5. **Per-feature interactive plot** (EvidentlyAI) — each finding gets an expandable plot, not a static thumbnail
6. **"Zoom in / Zoom out / Related" navigation** (Metabase) — findings as nodes in a graph the user walks
7. **Confidence ribbons over time-series anomalies** (OpenSearch) — show grade *and* confidence as separate visual layers
8. **Painted segments** (RATH Data Painter) — manual segment definition by selecting points on a chart

---

## 7. Cross-cutting algorithms worth considering

Deployed in OSS that Brightflow isn't using yet:

- **Random Cut Forest** (OpenSearch) — streaming sketch-based outlier detection with built-in feature attribution
- **Prophet** for time-series anomaly baselines (Soda, Sherlock-on-Druid) — handles holidays natively
- **Wasserstein distance** for distribution shift on large samples (EvidentlyAI) — often more stable than KS
- **KL divergence** for distribution comparison (Great Expectations) — sensitive to tail differences
- **Functional-dependency-based EII elimination** (QuickInsights) — pre-prune findings whose dimensions are functionally determined by other dimensions. The most underrated trick in the literature for taming "too many obvious findings"
- **PC / GES / LiNGAM causal discovery** (PyRCA, RATH) — produces a graph rather than a feed
- **Score calibration as explicit post-processing stage** (Merlion) — separate from detection

### Causal RCA — the genuine differentiating territory

Mature open-source causal stack exists ([DoWhy](https://github.com/py-why/dowhy), [EconML](https://medium.com/data-science-at-microsoft/causal-inference-in-practice-methodological-lessons-from-dowhy-fixed-effects-and-econml-f11f47129735), [Budhathoki et al.'s counterfactual approach](https://www.amazon.science/blog/new-method-identifies-the-root-causes-of-statistical-outliers)) but **no commercial BI product uses formal causal inference under the hood**. They all stick with regression / correlation / Shapley-without-causal-graph. Recent extensions: [RCA with Missing Structural Knowledge (2024)](https://arxiv.org/html/2406.05014), [Counterfactual RCA for Dynamical Systems (2024)](https://arxiv.org/html/2406.08106).

For SHAP applied to time-series specifically: **TSHAP** (sliding-window grouping), **ShaTS** (temporal-dependency-preserving), [**OmniXAI** (Salesforce)](https://opensource.salesforce.com/OmniXAI/latest/tutorials/timeseries/shap.html) wraps SHAP for time-series anomaly detection.

---

## 8. The training-data asymmetry (why "okay v1" is structural)

Honest framing of why polishing the engine to "great" requires real-data iteration, not better research or smarter LLM:

**What's in OSS / public corpus** (well-represented in training):
- The math: Adtributor lineage, QuickInsights, individual detector algorithms
- Statistical libraries: PyOD, Merlion, Kats, Prophet, riskloc
- Vendor-facing UX descriptions: Power BI tutorials, Anodot blog posts, Tableau Pulse architecture posts
- Adjacent integrated UIs: observability tools (Grafana, OpenSearch) — but only single-detector-type at a time

**What's closed** (absent from training):
- Integration glue: how Anodot calibrates scores across detectors, how Anomalo dedups, how Pulse picks insight types per metric
- Tuned constants: years of calibration on real customer data
- UI component code that renders these specific visualizations as a coherent product
- Story-organization heuristics that turn 8000 candidates into a digestible feed

**Implication**: Brightflow is being built from research papers + vendor docs + screenshots + first principles, not from "this is how everyone does it." The math is reliable; the integration choices and calibration constants are guesses informed by research that need real-data tuning.

---

## 9. Three-layer abstraction (proposed model)

Emerged from discussion about how to organize "what's interesting":

| Layer | Carries | Where |
|---|---|---|
| **Pipeline** (universal stats) | Scoring weights, KPI multiplier, effect-size floor, per-detector hard thresholds | `scoring.rs` constants |
| **Connector-type** (per-data-shape domain knowledge) | "For any GitHub source: `pull_requests.merged_at - created_at` is a derived `review_latency_hours` measure"; weekend dampening; release-cadence as KPI | **Doesn't exist yet** |
| **Source-instance** (this dataset's baselines) | "claude-code's normal PR throughput is 50/week"; per-source KPI flags; expected cadences | Partial (per-table semantics only) |
| **Per-table semantic** (column roles, is_kpi) | column role / is_kpi / label | Exists in DB, not wired to scoring |
| **Request override** | EngineConfig | Exists for thresholds, not scoring weights |

Resolution order: each layer overrides the previous. The genuinely new piece is **connector-type config** — knowledge that's true for *all* GitHub sources but isn't statistical-universal.

### What "interesting" means for GitHub specifically

- **`pull_requests`**: throughput trends, review latency distribution shifts, merge-rate anomalies, additions/deletions ratios
- **`issues`**: open vs close rate balance (backlog), time-to-first-response, label distribution shifts
- **`contributors`**: HHI/Gini concentration (top-3 share), new vs returning, churn between periods
- **`issue_comments`**: response latency, engagement spikes
- **`repository`**: snapshot table, less insight-rich on its own

These map to existing detectors but require **derived measures** that don't exist as raw columns (`review_latency_hours`, `backlog_delta`, `weekly_active_contributors`). Either the connector computes them at sync time (pure but inflexible) or the engine has a derived-measures pre-step driven by connector-type config (flexible, new architecture).

---

## 10. CLI iteration loop (proposed)

For a learning loop where Claude can iterate on the engine without touching the GUI:

1. **Store-aware CLI** — `brightflow insights run --source <id> --table <name>` reading from parquet store like the API. Foundational gap; everything else depends on it.
2. **All scoring constants as flags** — `--w-sig`, `--w-effect`, `--w-surprise`, `--kpi-multiplier`, `--min-effect`, plus per-detector thresholds.
3. **JSON output with run metadata** — `run_id`, `timestamp`, snapshot of `config_used`, dataset fingerprint.
4. **A diff command** — `brightflow insights diff a.json b.json` — appearing/disappearing findings, rank changes, score component deltas.
5. **A sweep command** — runs N configurations, emits summary CSV.
6. **Layered config files** — `pipeline_defaults → connector_type → source_instance → table_semantics → request_overrides`.
7. **Synthetic-data harness** — `brightflow insights synth` with known-signal datasets and assertions. Regression net so tuning doesn't kill recall.

### What Claude can/can't judge

- **Can**: implausibility (noisy p-values, trivial effect sizes, dedup over-collapsing parents that explain less than children, redundant findings about derived columns), before/after deltas when tuning a constant
- **Can't**: whether a "Sales dropped 18% in EU" finding actually matters to the business

So Claude calibrates toward *statistical and structural* sensibility, not domain relevance. User stays in the loop for substantive constraints ("anything correlating X and Y is uninteresting because they're the same metric in different units").

---

## 11. References — primary sources to revisit

### Research papers (foundational)

- [QuickInsights (SIGMOD 2019)](https://www.microsoft.com/en-us/research/publication/quickinsights-quick-and-automatic-discovery-of-insights-from-multi-dimensional-data/) — closest published description of what Brightflow is building
- [MetaInsight (SIGMOD 2021)](https://www.microsoft.com/en-us/research/publication/metainsight-automatic-discovery-of-structured-knowledge-for-exploratory-data-analysis/) — story-organization layer above raw insights
- [Adtributor (NSDI 2014)](https://www.usenix.org/conference/nsdi14/technical-sessions/presentation/bhagwan) — multi-dim root cause foundation
- [HotSpot (IEEE Access 2018)](https://netman.aiops.org/wp-content/uploads/2018/12/sunyq_IEEEAccess2018_HotSpot.pdf) — combinatorial dimension search via MCTS
- [Squeeze (ISSRE 2019)](https://netman.aiops.org/~peidan/ANM2022/8.AnomalyLocalization/LectureCoverage/2019ISSRE_Squeeze.pdf) — current SOTA F1
- [RiskLoc (2022)](https://arxiv.org/pdf/2205.10004) — weighted-risk reformulation
- [InsightPilot (EMNLP 2023)](https://ar5iv.labs.arxiv.org/html/2304.00477) — LLM-as-router architecture
- [LLMs on Tabular Data survey (2024)](https://arxiv.org/html/2402.17944v1) — best LLM+tabular overview

### OSS implementations to read

- [shaido987/riskloc](https://github.com/shaido987/riskloc) — Adtributor/HotSpot/Squeeze/RiskLoc reference implementations (MIT)
- [OpenSearch AD dashboards plugin](https://github.com/opensearch-project/anomaly-detection-dashboards-plugin) — best single-detector OSS UI
- [evidentlyai/evidently](https://github.com/evidentlyai/evidently) — distribution-shift UI patterns
- [Metabase automagic_dashboards](https://github.com/metabase/metabase/tree/master/src/metabase/xrays/automagic_dashboards) — template/grounding/combination/population pipeline
- [Salesforce PyRCA](https://github.com/salesforce/PyRCA) — broader RCA patterns

### Commercial deep-dives

- [Tableau Pulse architecture](https://www.tableau.com/blog/tableau-pulse-and-tableau-ai)
- [Tableau Pulse insight types](https://help.tableau.com/current/online/en-us/pulse_insights_platform_insight_types.htm)
- [Tableau Explain Data](https://help.tableau.com/current/server/en-us/explain_data_explained.htm)
- [Power BI Key Influencers tutorial](https://learn.microsoft.com/en-us/power-bi/visuals/power-bi-visualization-influencers)
- [Power BI Anomaly Detection (SR-CNN)](https://learn.microsoft.com/en-us/power-bi/visuals/power-bi-visualization-anomaly-detection)
- [Snowflake Cortex Analyst behind-the-scenes](https://www.snowflake.com/en/engineering-blog/snowflake-cortex-analyst-behind-the-scenes/)
- [Anodot anomaly detection architecture](https://www.anodot.com/blog/building-time-series-anomaly-detection/)
- [Bigeye Autothresholds](https://docs.bigeye.com/docs/autothresholds)
- [ThoughtSpot Spotter introduction](https://www.thoughtspot.com/blog/introducing-spotter-ai-analyst)
- [Einstein Discovery Stories](https://www.biztory.com/blog/understanding-einstein-discovery-stories-in-einstein-analytics)
