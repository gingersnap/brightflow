# Connector Candidates

Research notes on potential new data connectors for Brightflow, ranked by fit with the platform.

## Constraints

- **Open data**: publicly accessible, not user-generated on the source's side. A token is fine (like GitHub), but the data must exist independently of the Brightflow user.
- **Free**: no paid plans required.
- **Event/time-series preferred**: fits the Parquet/Polars storage and the analysis engine (anomaly, trend, seasonality, forecast, correlation).
- **General interest**: technical is fine if broadly interesting (GitHub-level appeal).

## Platform fit criteria

What Brightflow actually rewards in a connector:

- Event/time-series shape with a monotonic cursor field (`updated_at` style) → incremental sync is the path of least resistance.
- Numeric measures + categorical dimensions → the Polars analysis engine bites.
- Text fields → NLP enrichment (TF-IDF + k-means) pays off.
- Lua/Longbow DSL connectors are cheap to author and revise.
- Generic table-based UI → any connector "just works" in the frontend; no source-specific views required.

---

## Tier 1 — Start here

### 1. Hacker News (Firebase API)

- **Why top**: Trivial API (no auth, no key, just HTTPS JSON), very general interest, text-heavy → perfect for NLP clustering. Data shape is ideal — stories and comments are append-only events with a monotonic ID and timestamp.
- **Data**: ~30k new items/day. Stories (title, url, score, descendants, time), comments (parent, text, time), job posts, Ask/Show HN. `/maxitem.json` gives the latest ID, then walk backwards.
- **Analysis that pops**: topic trends over time, score anomalies, author clustering, domain popularity. Natural pair with the GitHub connector (same audience discussing the same things).
- **Risk**: none real. API has been unchanged for a decade.

### 2. Wikipedia Pageviews + Recent Changes

- **Why**: Two complementary firehoses from one source, both free, no auth. Pageviews API gives hourly/daily views per article (pure time-series). Recent Changes gives an SSE/EventStream of every edit — a massive event log with timestamps, user, size delta, comment.
- **Data shape**: Pageviews = textbook time-series with article as dimension. Edits = event stream with multiple dimensions + a text comment field.
- **Analysis that pops**: trending articles, edit-war anomalies, seasonality (holidays, news events), bot vs human ratio.
- **Risk**: Recent Changes is high-volume (tens of edits/sec). Need to sample or filter by language/namespace.

### 3. NPM Registry (download stats + registry events)

- **Why**: Perfect complement to the GitHub connector — same audience, different signal. Download counts are the *business metric* for open-source packages.
- **Data**: `api.npmjs.org/downloads/range/...` gives per-package daily downloads (time-series). Registry changes feed `replicate.npmjs.com/_changes` is a CouchDB changes stream of every publish event.
- **Analysis that pops**: release cadence vs downloads, version adoption curves, package ecosystem growth, dependency trends.
- **Risk**: No auth needed, but download stats API has per-call package limits (one package per request for daily breakdown) — need to batch sensibly. PyPI (via BigQuery) and crates.io offer equivalents.

### 4. USGS Earthquakes

- **Why**: Underrated demo-factor. Real-time feed of a *physical-world event stream* — totally different from the tech-centric stuff. Magnitude is a clean numeric measure; place/depth/time are dimensions. Free, no token, GeoJSON endpoints.
- **Data**: Rolling feeds (past hour/day/week/month) + historical query API back to 1900s.
- **Analysis that pops**: rate of M4+ events by region, aftershock clustering, seasonality, correlation with reported felt reports.
- **Risk**: Small domain — cool but narrow. Great as a "breadth" connector to prove the platform handles non-software data.

### 5. Stack Exchange API

- **Why**: Same audience as GitHub, different angle. Questions/answers/votes/tags are event-ish with tag-as-dimension and score-as-measure. Text-heavy. Free; key optional for higher quota.
- **Data**: Questions (created, tagged, scored, view_count), answers, comments, edits. Incremental via `fromdate`/`todate` + `sort=activity`.
- **Analysis that pops**: tag popularity over time, technology decline/rise, answerer burnout signals, unanswered-question trends.
- **Risk**: Quota (300/day unauthed, 10k with key) — manageable with good cursoring.

---

## Tier 2 — Strong, with caveats

### 6. Reddit (public API)

- **Why**: Huge general appeal, text-heavy, subreddit = natural dimension, score + comments = measures, events over time.
- **Caveat**: Their 2023 API tightening changed the vibe. Free tier still exists via OAuth app, but rate limits are tight and rules can shift. Would be Tier 1 if not for the governance risk.

### 7. NIST NVD (CVEs)

- **Why**: Security vulnerabilities as events, growing strategic importance, CVSS scores as measures, vendor/product/CWE as dimensions. Free API. Natural pair with GitHub (repo deps → vulnerabilities).
- **Caveat**: Backlog/enrichment lag issues at NVD have been well documented in 2024–2025. Data quality caveats, but the shape fits the platform beautifully.

### 8. GHArchive

- **Why**: Every public GitHub event, ever, as hourly JSON dumps (or via BigQuery). Orthogonal to the existing per-repo GitHub connector — this is the firehose view across all of GitHub.
- **Caveat**: Volume. One hour = ~100MB compressed. Would want to either scope to event types or accept it as a heavy connector. But it is *the* demo for time-series analytics on dev activity.

### 9. arXiv + OpenAlex / Crossref

- **Why**: Academic publishing as event stream. Citations, authors, institutions, fields = rich dimensions. Free.
- **Caveat**: Narrower audience than HN/Wikipedia. Best if Brightflow has any research/policy angle.

### 10. GDELT 2.0

- **Why**: Global news event database, updated every 15 minutes. Every named event with tone, actors, locations. Free.
- **Caveat**: Messy. Requires real filtering and the schema is famously wide. High ceiling, high effort.

---

## Tier 3 — Worth mentioning, not where to start

### FRED (Federal Reserve Economic Data)

Pure time-series, but thousands of disconnected series rather than a unified event stream; slightly off-shape. Free with API key. Great for macro-economic analysis if that's a direction.

### World Bank Open Data

Similar story to FRED — annual/quarterly cadence, not a firehose. Global development indicators, free, no key.

### Mastodon / Bluesky public firehose

Open and technically fascinating (Bluesky's Jetstream in particular), but operationally heavy and moderation noise is real. Better once there's a content-moderation/filtering story.

### NASA NEO (near-earth objects)

Fun novelty — asteroid close approaches as events with distance/velocity/size. Tiny data volume. Demo-grade rather than analytical-grade.

### OpenStreetMap changesets

Geo-heavy event stream. Would need spatial tooling the platform doesn't have yet.

### Product Hunt

Launches as events, decent general interest, but smaller catalog and the API is less loved than it used to be.

### MusicBrainz / OpenLibrary

Mostly entity snapshots rather than event streams. Rich metadata, but doesn't play to the platform's time-series strengths.

### PyPI / crates.io download stats

Equivalents to the NPM recommendation above — if NPM lands well, these are near-copies for the Python and Rust ecosystems. PyPI stats live in Google BigQuery (public dataset); crates.io exposes a direct API.

### Twitch public API

Stream/channel metrics, requires app auth but free. Entertainment-industry angle. Live-viewer counts as time-series, but the interesting data (chat) needs separate IRC plumbing.

### RSS / Atom feeds (generic)

Not a single connector but a category — any blog or news site with a feed becomes an event stream. Worth considering as a meta-connector once the core patterns are solid.

---

## Recommendation for what to build next

**Hacker News first.** Lowest-friction path to prove the platform handles a second source end-to-end — no auth plumbing, text-heavy so the NLP stack lights up, and the "HN + GitHub" pairing tells a coherent story (discussion about the code).

**Wikipedia pageviews second** — shifts shape (pure numeric time-series, no text) and will expose any assumptions the schema/analysis baked in from GitHub/HN. If both land cleanly, adding connectors #4, #5, #6 becomes mechanical.
