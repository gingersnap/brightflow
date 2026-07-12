# Brightflow Bluesky Analysis — Feature Analysis & Competitive Research Report

_Date: 2026-06-01_

This report has two halves:

1. **What Brightflow does today** — a deep technical analysis of the Bluesky feature as implemented.
2. **What the market does** — research on Bluesky-native and Twitter/X analytics tools, followed by a gap analysis and prioritized opportunities.

---

## Part 1 — What Brightflow's Bluesky feature does today

### 1.1 Connector & data acquisition

`crates/brightflow-connect/connectors/bluesky.lua` (registered in `src/lib.rs`).

- **Auth:** AT Protocol `createSession` via Bluesky **App Password** (`auth.atproto_session()`). Strips leading `@`/whitespace from the identifier to avoid 401s. Uses the `atproto-proxy: did:web:api.bsky.app#bsky_appview` header so cursor pagination works through the PDS→AppView path.
- **Two ingest modes:**
  - **`keyword`** — `app.bsky.feed.searchPosts` (`q`, `sort=latest`, optional `lang`), cursor-paginated, 100/page, PK `uri`.
  - **`actor`** — `app.bsky.feed.getAuthorFeed` (`filter=posts_with_replies`) for posts, plus `app.bsky.actor.getProfile` for a **daily profile snapshot** (composite PK `{did, snapshot_date}`, idempotent per UTC day).
- **Post schema (flattened to Parquet):** `uri, cid, author_did, author_handle, text, created_at, indexed_at, lang, reply_count, repost_count, like_count, quote_count, is_reply, is_repost, hashtags, mentions, external_uri, has_image, has_video`.
- **Profile snapshot schema:** `did, snapshot_date, handle, display_name, description, followers_count, follows_count, posts_count, indexed_at, created_at`.
- **Extraction detail:** hashtags & mentions pulled from richtext facets; image/video presence detected from embed `$type`; external link from `embed.external.uri`.

### 1.2 Storage

`crates/brightflow-store/src/ingest.rs` — Parquet files under `{source_id}/{table}/{uuid}.parquet`, metadata in SQLite. `merge_parquet()` does a Polars semi/anti-join upsert on primary keys (returns `rows_updated`/`rows_inserted`), plus per-column min/max/null stats for query pruning. Store is scoped per `source_id` so presets don't collide.

### 1.3 Enrichment & analysis engine

`crates/brightflow-engine`.

- **Text enrichment** runs on every sync for enrichable tables (`ENRICHABLE_TABLES = [("issues", [title, body]), ("posts", [text])]`). For Bluesky it embeds `text` with **Model2Vec `potion-base-32M`** (512-dim, L2-normalized) and writes back columns: `embedding`, `embedding_model_id`, `topic_terms`, `topic_cluster_id`, `topic_cluster`, `predicted_label`, `confidence`.
- **Topic fitting** (`topic_enricher.rs`): K-means (default **k=12**) on embeddings + **c-TF-IDF** cluster naming (unigram+bigram, sublinear TF, min_df 2 / max_df 0.5), cluster names = representative sample title + top distinctive terms. Optional supervised label centroids → per-row predicted label + cosine confidence. Artifacts persisted under `{workspace}/topics/{source_id}/{table}/`.
- **Statistical insights** (`analysis/`): two modes — **Review** (daily/weekly/monthly cadence) and **Trends** (by dimension). Detectors: anomaly (IQR + Z-score), change-point (PELT), concentration (Herfindahl), distribution shift, seasonality, trend (linear regression + significance), segmentation, correlation. Tunables: Z=2.0, p=0.05, min-effect=0.1.

### 1.4 API & scheduler

- **Topics API** (`brightflow-api/src/topics/handlers.rs`): `GET …/topics` (overview + cluster summaries), `GET …/topics/clusters/{id}` (samples, outliers, purity, label distribution, per-day timeseries), `POST …/topics/recluster` (refit with new k).
- **Insights API:** `POST /api/insights/review`, `POST /api/insights/trends`.
- **Connector mgmt:** list/run/runs endpoints; semantics endpoints let you tag columns as measure/dimension/time/entity.
- **Scheduler** (`brightflow-scheduler`): 30s tick, due-job detection, runs connector → merges → text-enrichment → updates cursor/sync_state, with `SyncRun` status tracking.

### 1.5 Frontend (what the user sees)

`brightflow-app`. Bluesky sources expose three tools (`sources/profiles.rs`): **Explore**, **Insights**, **Topics** (no Dashboard/Funnels).

- **Connect flow:** `PresetForm.vue` + `connectorHints.ts` specialize the form for Bluesky — "App password" label with `xxxx-xxxx-xxxx-xxxx` placeholder and setup help, `identifier`, `mode` (keyword/actor), conditional `query`/`lang`/`actor` fields, masked token with reveal toggle.
- **Topics view:** pie/donut of cluster sizes + cluster cards (top terms, row counts, expandable samples) + detail drawer. Recluster control with custom k.
- **Insights view:** renders 13 finding types (anomaly, trend, period comparison, seasonality, segment, correlation, concentration, distribution shift, change-point, etc.) with narrative summary, tech detail, score breakdown, and a drill path into Explore.
- **Explore:** generic filter/select/group-by/sort/limit query builder over the raw tables.

### 1.6 ⚠️ Notable gaps & bugs found in the current implementation

- **Topics view is hardcoded to the `issues` table** (`TopicsView.vue:24`, `const ISSUES_TABLE = 'issues'`; empty-state literally says _"Topics requires an `issues` table"_). But Bluesky produces a **`posts`** table. The backend *enables* the Topics tool for Bluesky **and** enriches `posts` — so the pipeline runs, but **the UI will never render it**. This is the single most impactful gap: a built feature that's unreachable for the connector it was advertised for. Fix is small (generalize the table constant / drive it from the source's enrichable tables).
- **No engagement analytics surface.** The connector captures `like/repost/reply/quote_count`, `has_image/has_video`, `lang`, `hashtags` and a daily follower snapshot — but there's no dashboard turning these into the table-stakes metrics every competitor ships (engagement rate, top posts, best-time-to-post, follower growth curve). The raw data exists; the presentation layer doesn't.
- **Profile snapshots are collected but not visualized.** `followers_count`/`follows_count`/`posts_count` per day is exactly a growth time-series, but nothing charts it.
- **Topic naming uses a code-aware tokenizer** (camel-case splitting, code stop-words) inherited from the GitHub-issues origin — fine but not tuned for social text (emoji, @handles, #hashtags, URLs).

---

## Part 2 — Competitive landscape

### 2.1 Bluesky-native tools

Bluesky ships almost **no native analytics** (only like counts in-app), so a third-party market formed. Four clusters:

**Creator/marketer dashboards (direct competitors):** TheBlue.social, BlueSkyHunter (TechCrunch-covered, Feb 2025), Fedica, Metricool/Sprout (added Bluesky), GraphTracks, plus many lightweight free trackers (SkyKit, SkeetStats, Bluesky Meter, Dopplersky…). Common feature set: **follower growth with follow/unfollow split**, engagement rate, **best-posting-time heatmap**, top posts, posting streaks, time-range filters. Weak everywhere: reach/impressions (protocol exposes none — they estimate), demographics (only Fedica claims it).

**Open-data / network & moderation tools (the differentiated cluster):**
- **ClearSky** (clearsky.app) — block-graph transparency: who blocks you, which moderation lists you're on, network-wide block stats, trending-blocked board. Only possible because the block graph is public.
- **Stats for Bluesky / Atlas by Jaz** (bsky.jazco.dev) — network aggregates + **Atlas: engagement-based community/cluster graph** of the whole network.
- Follower-network explorers, DID/handle history trackers (portable identity is public), thread reconstructors.

**Firehose / Jetstream dev tooling:** Firesky, Firehose 3D, SkyFeed (no-code feed builder), AT Proto Explorer (repo/post analyzer, exports), Memgraph BlueJ (graph-DB social-graph demo). Mostly developer toys/infra, not productized analytics.

**Academic research on the open firehose** validates demand: polarization/topology (arXiv:2405.17571, 2506.03443), starter-pack network bootstrapping (2501.11605), news reliability (MurkySky 2501.10557).

**The AT Protocol moat — analyses impossible on X/Threads:**
- Free, permissionless, complete **firehose** (no API key, no quota, no fee) + **Jetstream** (JSON, >99% bandwidth reduction → runnable on a laptop).
- **Open, fully-queryable social graph** → real community detection, "who-blocked-whom," reciprocal-follow, centrality.
- **Custom feeds** as an analytics surface → feed-placement/feed-reach analytics (no incumbent).
- **Stackable public labels** → labeler/moderation analytics (greenfield).
- **Starter packs** as a measurable growth primitive.
- **Network-wide true percentiles** — because the whole network is queryable, you can compute real benchmarks, not samples.
- Honest limitation: **no impression/reach data exists** in-protocol; competitors fudge it.

### 2.2 Twitter/X analytics tools (mature feature taxonomy to benchmark against)

- **Engagement:** engagement rate / impressions / reach (Hootsuite, Sprout, Buffer); **best-time-to-post** (Sprout ViralPost, Buffer, Fedica per-timezone); top-post ranking; content-type performance; published benchmarks.
- **Audience:** follower growth/churn + grading (Social Blade), **demographics to city level** (Followerwonk, Tweepsmap), interests/affinities, **adjustable 2–20 cluster segmentation** (Audiense), in-audience influencer identification, bio search, audience-overlap comparison.
- **Content intelligence:** hashtag campaign tracking (Keyhole), trending topics (BuzzSumo), **share of voice** (Brandwatch/Sprout), **content theme/topic clustering** (Brandwatch/Brand24 — closest analog to Brightflow's Topics).
- **Social listening:** keyword/mention tracking at scale, **sentiment over time** (Brand24 ~95%, Talkwalker ~90% w/ sarcasm), competitor benchmarking, **spike/crisis detection**, **emotion detection** (Talkwalker 25+ emotions, Brand24 6), visual/logo recognition (Talkwalker).
- **Network/influencer:** Audiense is the standout for **commercial graph-based community detection**; most others don't expose it.
- **AI:** NL-query assistants over your own data (Talkwalker Yeti, Brand24 Assistant), predictive (best time, reach probability), content recommendations, AI-drafted automated reports.
- **Reporting/UX:** custom dashboards, scheduled reports, CSV/XLSX/PDF/PPT export, alerts.

---

## Part 3 — Gap analysis & opportunities for Brightflow

### Where Brightflow already stands out
- **A real statistical insights engine** (anomaly / change-point / seasonality / distribution-shift / trend with significance testing) — most consumer Bluesky tools have *nothing* like this; it's closer to Brandwatch/Talkwalker territory than to BlueSkyHunter.
- **Embedding + clustering Topics pipeline** (Model2Vec + c-TF-IDF) — directly comparable to the content-theme clustering only Brandwatch/Brand24/Audiense offer commercially. Brightflow has the hard part built.
- **Local/open-data architecture** (Polars + Parquet, Lua connectors) — zero data-acquisition cost via the protocol, no vendor API fees.

### Table stakes Brightflow is missing (build to be credible)
1. **Fix the Topics-table bug** so Topics actually works on Bluesky `posts`. _(near-zero effort, unblocks an already-built feature)_
2. **Engagement dashboard** from data already collected: engagement rate, top posts, like/repost/reply/quote breakdown, image-vs-text-vs-link performance, language mix.
3. **Follower-growth chart** from the daily `profile_snapshot` (already collected, unused).
4. **Best-time-to-post heatmap** from `created_at` × engagement — the single most expected feature in the creator cluster.
5. **Scheduled/exportable reports** (the engine output is report-shaped already).

### Differentiators that leverage the AT Protocol moat (where to actually win)
6. **Network / community graph analysis** — Brightflow already has embeddings + K-means; extend from *content* clustering to *audience/follow-graph* clustering (Audiense's premium feature, free to compute on Bluesky's open graph).
7. **Feed-placement analytics** — "which custom feeds is my content surfacing in, and what reach does each drive." No incumbent owns this.
8. **Network-wide true benchmarking** — real percentile ranks (engagement, growth) computed against the whole network via firehose — literally impossible on X.
9. **Moderation/label & block-graph insights** (ClearSky-style) — reputation/coverage analytics from public label and block records.
10. **Firehose/Jetstream ingestion** — current connector is poll/cursor-based per account/keyword; adding a Jetstream consumer unlocks real-time, network-scale features (trending detection, live spike alerts, whole-graph analysis) cheaply.

### Higher-end AI features to consider (match Brandwatch/Talkwalker tier)
11. **Sentiment & emotion** over post text/time (none today) — natural next enrichment alongside Topics.
12. **NL-query assistant** over the user's own Parquet/insights (à la Talkwalker Yeti) — fits the existing "drill path / Explore" model.

### Suggested priority order
**P0 (days):** #1 Topics fix.
**P1 (table stakes, weeks):** #2 engagement dashboard, #3 follower-growth chart, #4 best-time heatmap — all from data already in hand.
**P2 (differentiation):** #11 sentiment/emotion, #10 Jetstream ingestion, #6 graph/community analysis.
**P3 (moat plays):** #7 feed-placement, #8 network-wide benchmarking, #9 moderation/label insights, #12 NL assistant.

---

## Key sources
Bluesky: theblue.social, blueskyhunter.com (+ TechCrunch 2025-02-14), clearsky.app, bsky.jazco.dev (Atlas), skyfeed.app, docs.bsky.app/docs/advanced-guides/firehose, github.com/bluesky-social/jetstream, romiojoseph.github.io/atproto-explorer, github.com/fishttp/awesome-bluesky. Research: arXiv 2405.17571, 2506.03443, 2501.11605, 2501.10557.
X/Twitter: sproutsocial.com, hootsuite.com, buffer.com/analyze, followerwonk.com, audiense.com, socialblade.com, keyhole.co, buzzsumo.com, brandwatch.com, talkwalker.com, brand24.com, fedica.com.
