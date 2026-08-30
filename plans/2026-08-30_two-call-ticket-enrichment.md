# Plan: Two-call ticket enrichment — classification and mention extraction

**Status:** Proposal — decisions marked *confirmed* are settled with the author;
*default* are my assumptions until reviewed.
**Date:** 2026-08-30
**Context:** Replaces the embedding → cluster → classifier-head path as the way
tickets get categorised. Follows a review of the codebase against the two-call
design (Call A classification, Call B extraction) discussed with the author on
this date. Independent of
[`2026-08-30_captured-test-workspace.md`](2026-08-30_captured-test-workspace.md),
except where noted in §E4 (the captured template carries topic artifacts that
this plan deletes).

---

## Context

The principle both the existing code and the new design share: **the LLM is a
write-time dependency, never a read-time one.** Enrichment runs after sync,
materialises columns into Parquet, and analysis is Polars over those columns.

Where they part is *how much* of the write path the LLM owns. The current
architecture's thesis (`crates/brightflow-engine/src/enrichment/topic_enricher.rs:32`)
is "LLM cost is O(taxonomy + seed sample), never O(rows)": an LLM proposes a
vocabulary, a human ratifies it, and a linear head over static embeddings
labels every row for free. That head is trained on `potion-base-32M`, which is
English-only (`crates/brightflow-engine/src/embedding/backend.rs:203` asserts a
multilingual embedder is *not* an option). For an EU, multilingual corpus the
hot path therefore cannot classify most of the rows it is meant to classify.
The new design makes the LLM O(rows) — two calls per ticket — and accepts the
cost because the alternative does not work on the input.

What the review found already exists, and is kept or re-pointed rather than
rebuilt:

- **The per-row LLM machinery.** `llm_prompt` functions
  (`crates/brightflow-api/src/enrichment/runner.rs`) already do forced tool
  calls, enum coercion with one re-ask, a content-addressed cell cache
  (`enrichment_cache`, keyed `(function_id, spec_hash, input_hash)`), run rows
  with token totals, an estimate endpoint, and a full-table materialise. Calls A
  and B reuse all of it.
- **Vocabulary curation with an audit trail.** `taxonomy_categories`
  (`crates/brightflow-store/migrations/013_intent_taxonomy.sql`), the
  `propose_taxonomy` agent that can only call `define_taxonomy_category`, and
  the action bus (`crates/brightflow-api/src/actions/`) where a human click and
  an agent tool call share one dispatch, one `action_log`, and one undo story.
  This *is* "LLM suggested, human editable, logged" — it needs a hierarchy, a
  frozen flag, a user id on the log row, and a different sampling corpus.
- **Post-sync triggering.** `crates/brightflow-api/src/enrichment/mod.rs:18`
  starts a `scope=missing` run for every promoted function after each connector
  sync. Unchanged rows are free cache hits.

What does not exist: any ticket source beyond GitHub issues (connectors are
`github.lua` and `bluesky.lua`, `crates/brightflow-connect/connectors/`), an
entity catalog, a mention grain, language detection, nested Parquet dtypes
(Polars 0.48 without `dtype-struct`), cached-token accounting, or any notion of
function ordering or per-table write serialisation.

What is wrong and gets removed (Wave E): embeddings, clusters, the classifier
head, centroid reconciliation, the row-level `CurationQueue`, and the agent
kinds that serve them. Once Call A classifies every row, nothing reads them.

**Binding repo rules for every step below:** co-located unit tests for touched
pure logic (rule 2); `//!` / `/** */` docs written for new files and re-verified
on touched ones, stating contracts rather than cross-file observations (rule 1);
new prose only in the dated `plans/` class (rule 3); strict clippy stays green;
Rust work ends with `cargo build --release`.

---

## Decisions reached

- **Confirmed — the embedding/classifier path is retired, not kept alongside.**
  The vocabulary-curation half (taxonomy tables, agent proposal, action bus,
  audit) stays and is the backbone of Wave C.
- **Confirmed — categories are LLM-proposed and human-edited, with logging of
  who did what.** The action bus already records `actor_type` and
  `agent_run_id`; Wave C adds the human user id.
- **Confirmed — no verbatim text is ever produced or stored.** Every free-text
  output is a summary: a ticket `summary` (one short clause) or a mention
  `feedback_summary` (3–6 words, normalised). Always English regardless of
  source language.
- **Confirmed — no span in v1.** A mention references its ticket; opening the
  ticket is the recovery path. Spans return as turn indices when a transcript
  source exists, where they are both reliable and useful.
- **Confirmed — mentions are a child table**, `{table}_mentions`, not a
  `List<Struct>` column. Rationale in §D2.
- **Confirmed — subcategory is induced *within* its parent category**, from
  that parent's ticket summaries.
- **Confirmed — GitHub issues are the first source.** Consequences: `resolved`
  is not extracted (issues carry `state`/`state_reason`, and the design's own
  rule is "only extract what isn't already structured"); the mention table
  launches on issue bodies; per-language eval is built against issues.
- **Confirmed — cache-key refinements are deferred.** The one exception is the
  hash gap that would produce silently wrong data (§A3); everything else about
  cache economics is an open question, not a wave.
- **Default — Calls A and B are a built-in function kind, not `llm_prompt`
  templates.** `llm_prompt` is the user-authored ad-hoc column tool with a
  draft/promoted lifecycle; the two calls are a fixed-schema product feature
  whose prompt is owned by code and whose customer-specific parts are config.
  Two kinds, `ticket_classify` and `ticket_extract`, each with its own cache so
  a catalog edit never invalidates classification cells.
- **Default — the prompt sees vocabulary *names and descriptions*; Parquet
  stores *ids*, resolved to current names at materialise time.** The
  description is the definition; the name is a label. `spec_hash` therefore
  covers ids and descriptions, not names — a rename is a free re-materialise, a
  redefinition is a recalibration event (full re-call, user picks the scope).
  See Open questions if this feels too subtle.
- **Default — one vocabulary table for everything the model resolves against.**
  `taxonomy_categories` gains `kind` ∈ {`category`, `subcategory`,
  `feedback_category`, `product`, `competitor`} and `parent_id`. Products and
  competitors are imported vocabularies with a 20/level cap; the rest are
  induced with a 10/level cap; hard backstop 50 per level as an engineering
  limit. One table means one CRUD, one set of actions, one audit trail, one
  review UI.
- **Default — `language` is a deterministic pre-call input, not an LLM
  output.** Detected with `whatlang` and passed into the prompt. You cannot gate
  Call B per language on something Call A returns.
- **Default — `other` is a reserved value in every induced vocabulary level**,
  so the >15 % other-rate health signal has something to count.

---

## Wave A — foundations (4 commits, no LLM behaviour change)

### A1. Vocabulary hierarchy and provenance

Migration `020_vocabulary_hierarchy.sql` on `taxonomy_categories`:

- `kind TEXT NOT NULL DEFAULT 'category' CHECK (kind IN ('category',
  'subcategory','feedback_category','product','competitor'))`.
- `parent_id INTEGER REFERENCES taxonomy_categories(id) ON DELETE RESTRICT` —
  `RESTRICT`, not cascade: deleting a parent with children is a decision the UI
  must surface, not a silent prune.
- `frozen INTEGER NOT NULL DEFAULT 0`. A frozen row refuses rename/redefine
  through the action executors; it exists so `category` can hold the
  long-horizon trend line while `subcategory` is recalibrated.
- `aliases_json TEXT` for imported kinds (product/competitor surface forms the
  resolver accepts).
- `UNIQUE (table_id, name)` becomes `UNIQUE (table_id, kind, parent_id, name)`
  — SQLite treats NULLs as distinct in unique indexes, so top-level rows use a
  sentinel `parent_id = 0` rather than NULL. Record that in the migration
  comment; it is the kind of decision that otherwise gets rediscovered.
- Existing rows migrate as `kind='category', parent_id=0`.

`crates/brightflow-store/src/db/curation.rs` gains `list_vocabulary(table_id,
kind, parent_id)`, `count_children`, and the cap check as a pure function in
`crates/brightflow-engine/src/enrichment/vocabulary.rs` (new):
`check_cap(kind, existing_count) -> Result<(), CapError>` with the 10/20/50
rule. Unit tests: each kind at cap, at cap−1, at the hard backstop, and that
`other` does not count toward the cap.

### A2. Who did it

Migration `021_action_log_user.sql`: `ALTER TABLE action_log ADD COLUMN user_id
TEXT`. `Actor::Human` becomes `Actor::Human { user_id: String }`; the
`POST /api/actions` handler (`crates/brightflow-api/src/actions/handlers.rs:60`)
reads it from the `AuthSession` extractor already used in `routes.rs:335`.
`Actor` is `Copy` today because it is two integers; it stops being `Copy` and
the three call sites take `&Actor`. Test: `initial_status_tiers_by_actor_and_undoability`
extends to the new shape; a new test asserts the logged row carries the id.

### A3. Hash what determines the output

`spec_hash` (`crates/brightflow-engine/src/enrichment/function.rs:157`) omits
the system prompt and sampling parameters, so editing `SYSTEM_PROMPT` in
`runner.rs:31` reuses every cached cell. Add a `prompt_fingerprint: &str`
argument that the runner passes as `blake3(SYSTEM_PROMPT ‖ temperature)`. This
is the one cache change in scope: it is not economics, it is correctness — a
changed prompt with an unchanged hash is wrong data that looks fresh.

Test: two specs identical except the fingerprint hash differently; the existing
"reverting a prompt re-hits old cache" property still holds.

### A4. Serialise materialisation per table

Two functions materialising into the same table race on
`replace_table_data` with a single retry (`runner.rs:498`); a sync landing at the
same moment makes the loser fail. Add `materialize_locks: DashMap<String,
Arc<tokio::Mutex<()>>>` to `AppState` keyed by `cache_key(source_id, table)`;
`materialize` holds the lock for its read-rebuild-write. The scheduler's merge
is a different process boundary and stays on optimistic versioning — the
retry exists for it. Test in `runner.rs`: two concurrent materialisations of
different functions on one table both succeed and both columns are present.

---

## Wave B — Call A, classification (5 commits)

### B1. Language detection

New dependency `whatlang` in `brightflow-engine` (run `./scripts/audit.sh`).
`crates/brightflow-engine/src/nlp/language.rs` (new): `detect(text) ->
Option<&'static str>` returning the primary BCP-47 subtag, `None` below the
crate's reliability threshold or for text under 20 characters. If the source
already has a language column (`posts.lang`), the detector is not consulted —
`config.rs`'s `language_column` precedence applies.

Unit tests: Swedish, Finnish, English, a code-only body (→ `None`), a
20-character boundary case.

### B2. `FunctionSpec::TicketClassify`

`crates/brightflow-engine/src/enrichment/function.rs`:

```rust
TicketClassify(TicketClassifySpec {
    text_columns: Vec<String>,        // ["title", "body"] for issues
    language_column: Option<String>,  // None = detect
    provider_id: String,
    model: Option<String>,
    // Snapshotted at version time: what the prompt saw.
    categories: Vec<VocabEntry>,      // id, description; kind=category
    subcategories: Vec<VocabEntry>,   // id, parent_id, description
})
```

Names are *not* in the spec — they are looked up at render and materialise
time (Decisions). `spec_hash` for this kind covers `text_columns`, provider,
model, the fingerprint from §A3, and the `(id, parent_id, description)` triples.

Outputs are fixed by the kind, not configured:

| column | dtype | values |
|---|---|---|
| `summary` | String | one clause, ≤ 15 words, English |
| `language` | String | from §B1, written even when the LLM call fails |
| `category` | String | vocabulary name, resolved from id |
| `subcategory` | String | vocabulary name, resolved from id; must be a child of `category` |
| `sentiment_polarity` | String | `positive` `negative` `neutral` `mixed` `none` |
| `sentiment_strength` | String | `none` `low` `strong` |
| `ticket_classify__status` | String | `ok` / `error` / null |

`resolved` is absent (Decisions). Validation (`validate.rs`) rejects a spec
whose output names collide with existing columns of the table — for `issues`
none do; `posts` is not a ticket source and is out of scope.

The tool schema is built by a new `classify_tool_schema(spec)`; the `summary`
property comes first in the schema and the prompt says to fill it first, since
later fields condition on it. `subcategory` is validated *against the chosen
category's children*, not the flat list — a valid subcategory under the wrong
parent is a re-ask, then an error cell.

The prompt (owned by `runner.rs`, not stored) is ordered for prefix caching:
system instructions and the full vocabulary block (byte-identical across rows)
first, the ticket last. `neutral` vs `none` and `mixed` get one-line
definitions in the system block, verbatim from the design.

Unit tests: schema shape; parent-child validation; id→name resolution with a
renamed category; `other` accepted at every level; the prompt's byte prefix is
identical for two different tickets.

### B3. Runner dispatch on kind

`start_run_internal_scoped` (`enrichment/handlers.rs:609`) currently refuses
anything but `llm_prompt`. It dispatches on `FunctionSpec` instead:
`LlmPrompt` → existing path; `TicketClassify` → `prepare_inputs` renders
`text_columns` plus the detected language into the same `RowInput` shape
(language is a deterministic function of the text, so it adds nothing to cache
churn), `compute_cell` takes a `CellPrompt` trait object rather than an
`LlmPromptSpec`. `execute_cells`, the cache, progress, and `materialize` are
shared unchanged except that `build_output_column` resolves vocabulary ids.

`post_sync` (`enrichment/mod.rs:18`) lists promoted functions of *all* LLM
kinds. Built-in kinds are created promoted; there is no draft state for them,
which removes the "merge nulls a draft's columns" hazard for A and B.

Test: the existing mock-server integration test in
`crates/brightflow-llm/tests/mock_server.rs` gets a sibling in
`crates/brightflow-api/tests/` that runs a `ticket_classify` function over the
test workspace's `issues` table against a scripted provider and asserts the
seven columns land with the right dtypes.

### B4. Token accounting for the two things the design prices on

- `WireUsage` (`crates/brightflow-llm/src/lib.rs:363`) parses
  `prompt_tokens_details.cached_tokens` when present; `ChatOutcome` exposes it;
  `enrichment_cache` and `enrichment_runs` gain `cached_tokens` (migration
  `022`). Without this, the "cached tokens are cheaper" claim is unmeasurable.
- `GET /api/functions/{id}/usage-by-language`: `prompt_tokens`,
  `completion_tokens`, `cached_tokens`, and cell count grouped by the
  `language` column, read from the materialised table joined to the cache by
  input hash. This is the number to price Swedish and Finnish on. Currency stays
  out (Open questions).

### B5. Taxonomy health, cold-start and steady-state

`GET /api/tables/{source}/{table}/vocabulary/health` returns, per induced
level: count vs cap, `other` rate, the share of the largest and smallest value,
and the rows failing the 2 %–40 % balance band. Pure computation in
`crates/brightflow-engine/src/enrichment/vocabulary.rs` (`health(counts,
cap)`), unit-tested on hand-built histograms. Pairwise confusion needs a second
labeller and is deferred (Open questions).

---

## Wave C — vocabulary induction from summaries (4 commits)

### C1. Sample summaries, not bodies

`crates/brightflow-api/src/agent/sampling.rs` draws `{rowId, title, body}`
stratified across format clusters. It becomes `{rowId, summary}` drawn
uniformly from rows whose `ticket_classify__status = 'ok'`; the cluster
stratification goes with the clusters (Wave E). `TAXONOMY_SAMPLE` rises from 60
to 200 — summaries are ~12 tokens each, so this is cheaper than the old 60
truncated bodies and a far better induction corpus. The deterministic
`SAMPLE_SEED` stays so a re-run resumes the same queue.

`propose_taxonomy` is refused when fewer than 50 rows have a summary — inducing
a vocabulary from nothing produces a guessed list, which the design forbids.

### C2. Three induction runs, one agent shape

`agent/runner.rs` gets kinds `propose_categories`, `propose_subcategories
{ parent_id }`, and `propose_feedback_categories`, all restricted to
`define_taxonomy_category` (which now takes `kind` and `parent_id`) and `done`.
`propose_subcategories` samples only summaries whose `category` is the parent.
`propose_feedback_categories` samples `feedback_summary` values from the mention
table (Wave D) — it is wired here and refuses to run until D lands.

The system prompt keeps the existing negative instruction against
format-shaped buckets (`runner.rs:443`) and adds the cap as an instruction
("at most 10; group rather than list"), with the executor enforcing §A1's cap
regardless of what the model asks for.

Existing categories are passed in as they are today, so a recalibration run
proposes a *diff* against the current list, not a fresh list. The action bus
records each proposal as `proposed`; the human approves, renames, or rejects
per row — this is the "reviewed as a diff" step, and it already exists in
`TaxonomyPanel.vue`, which needs the hierarchy added (§C4).

### C3. Frozen and redefinition semantics in the executors

`crates/brightflow-api/src/actions/exec/taxonomy.rs`:

- `rename_taxonomy_category` on a frozen row → `BadRequest`.
- A new `redefine_taxonomy_category { id, description }` action, distinct from
  rename, because per the Decisions it is the one that invalidates cache. Its
  response carries `affects_cache: true` and the number of cells that will
  recompute, so the UI can say what approving costs.
- `freeze_taxonomy_category` / `unfreeze_taxonomy_category`, undoable.

The `TicketClassifySpec` snapshot is rebuilt (new function version) whenever a
vocabulary row of kind category/subcategory is added, redefined, or deleted;
renames do not bump it. Test: a rename leaves `spec_hash` unchanged; a
redefinition changes it.

### C4. Taxonomy panel: hierarchy and provenance

`brightflow-app/src/components/topics/TaxonomyPanel.vue` moves to
`components/enrichment/VocabularyPanel.vue`: a two-level tree per kind, cap
meter, frozen toggle, and the audit line ("proposed by run 12, approved by
jens, renamed by anna") from `action_log`. Types regenerate via `ts-rs`. This is
the only frontend work in Waves A–D beyond showing new columns, and it is
deliberately thin — the data model is the deliverable.

---

## Wave D — Call B, mentions (5 commits)

### D1. Imported vocabularies and the unresolved queue

Products and competitors are `taxonomy_categories` rows of kind `product` (two
levels: area → component, 20 each) and `competitor` (one level, 20). Import is
`POST /api/tables/{source}/{table}/vocabulary/import` taking a CSV of
`kind,parent,name,description,aliases`, executed as one `define` action per row
through the bus so it is logged and undoable as a batch.

Migration `023_unresolved_subjects.sql`: `unresolved_subjects(table_id,
kind, surface TEXT, mention_count, first_seen, last_seen, status IN
('open','mapped','ignored'), mapped_to REFERENCES taxonomy_categories(id))`.
`surface` is the model's *normalised name* for the entity ("invoice screen",
"CompetitorCo"), never a quote from the ticket. Resolution is exact or
case-insensitive match on name or alias; anything else lands here and the
mention's `subject_id` is null. "Nine mentions of an unknown competitor is a
better signal than silent absorption" is the whole reason this table exists.
Mapping an entry back-fills `subject_id` on existing mention rows via a
re-materialise, no LLM call.

### D2. `{table}_mentions` child table

Why a table and not `List<Struct>`: the co-location argument is a scan-cost
argument on a store whose `read_table` eagerly reads and concatenates every file
and whose pruning is file-level from SQLite stats. A Polars join on `ticket_id`
at single-server volumes is nothing. Meanwhile a nested column would need a
Polars feature flag plus teaching `merge_parquet`, `stats.rs` (skips lists), the
analytics executor, textexplore, and the generated TS types about nesting — and
the insights engine, which is the point of mention grain, only understands flat
tables. The `has_*` booleans the design wanted for row-group pruning are kept as
derived ticket columns because they are useful filters, not because they prune
anything today.

Schema, written by `materialize` for `ticket_extract`:

| column | dtype |
|---|---|
| `ticket_id` | same dtype as the parent's id column |
| `mention_idx` | Int32 |
| `type` | String: `product` `competitor` `pricing` `service` `feedback` |
| `subject_id` | Int64 nullable — vocabulary id |
| `subject` | String nullable — resolved name, for the analytics engine |
| `subject_surface` | String nullable — the model's name when unresolved |
| `feedback_summary` | String nullable — 3–6 words, English |
| `feedback_category` | String nullable — vocabulary name |
| `incidental` | Boolean |
| `polarity` | String — same enum as ticket sentiment; comparatives get two rows |
| `confidence` | Float32 |

Primary key `(ticket_id, mention_idx)`. The table is rebuilt wholesale from the
cache on every materialise and written with `replace_table_data`, so a
re-enriched ticket with fewer mentions cannot leave stale rows — a PK merge
would. It is registered in the catalog as an ordinary table under the same
source, so `scan_table`, the analytics executor, and the insights engine see it
with no special casing. The parent row gets `has_feedback`,
`has_incidental_feedback`, `has_competitor_mention`, `mention_count`, and
`ticket_extract__status`.

Pure builder `build_mention_rows(cells) -> DataFrame` and
`derive_ticket_flags(rows)` in a new
`crates/brightflow-engine/src/enrichment/mentions.rs`, unit-tested: empty
output, a feedback+product pair sharing an index, a comparative producing two
rows with opposite polarity, an unresolved subject.

### D3. `FunctionSpec::TicketExtract`

Mirrors §B2: `text_columns`, provider, model, and snapshots of the product,
competitor, and feedback_category vocabularies as `(id, parent_id,
description, aliases)`. Output is a JSON array of mention objects under one
forced tool; the schema caps the array at 50 entries as an engineering limit.

The prompt: vocabulary block first for prefix caching; "list everything
mentioned, including the reason for contact, and mark `incidental`"; the
few-shot block leads with an **empty-array example** because incidental
feedback is 5–15 % of tickets and a model asked to fill a field will fill it.
`feedback_summary` is instructed as a normalised noun-phrase or imperative in
English, 3–6 words. A `product` mention and a `feedback` mention about it share
`mention_idx` parity by construction — the model emits them adjacent; the
builder does not enforce adjacency, only that both are valid rows.

The consistency check the design wants — Call B's non-incidental mentions vs
Call A's category — is a health query in §B5, not a gate: disagreement is a
signal about the vocabulary, not an error on the row.

### D4. Headline metrics at mention grain

Two pure Polars queries in `crates/brightflow-api/src/enrichment/mention_stats.rs`:
per subject, `mentions`, `distinct_tickets`, negative share, incidental share.
`distinct_tickets` is the headline (long tickets and repeated complaints inflate
`mentions`). Exposed as `GET /api/tables/{source}/{table}/mentions/summary`.
Unit test on a hand-built frame where one ticket mentions a subject three times.

### D5. Per-language eval, both calls separately

`testdata/eval/{en,sv,fi}/issues.csv` with hand-labelled expected
`category`, `sentiment_polarity`, and the expected mention list — twenty rows
per language to start, author-supplied. `cargo run -- enrich eval --lang sv
--call a|b` runs the real function against the configured provider and prints
accuracy per field (A) and precision/recall on `(type, subject)` pairs (B). This
is a CLI, not a test: it costs money and needs a provider. Its output is what
decides whether the mention table launches for a language — the design's own
warning is that extraction degrades across languages far faster than
classification.

---

## Wave E — retire the embedding path (3 commits, after A–D are live)

### E1. Delete, don't deprecate

Remove: `crates/brightflow-engine/src/embedding/`, `nlp/linear.rs`,
`nlp/dense_clustering.rs`, `nlp/density.rs`, `nlp/cluster_metrics.rs`,
`enrichment/topic_enricher.rs`, `enrichment/artifacts.rs`,
`enrichment/curation.rs`, `enrichment/labels_io.rs`;
`FunctionSpec::TopicModel` and `::Classifier`; the `model2vec-rs`, `linfa`,
`linfa-logistic` dependencies; `crates/brightflow-scheduler/src/text_enrichment.rs`
and the pre-merge call at `lib.rs:341`; `crates/brightflow-api/src/topics/`
except `display.rs` (which `sampling.rs` still uses for the id column);
`agent` kinds `auto_label`, `propose_merges`, `label_documents`; CLI `topics
fit|embed|eval-classifier|near-dup`; `cluster_edits`, `excluded_terms`,
`document_labels` via a migration that drops them; the `topic_*`, `embedding*`,
`predicted_label*`, `confidence` columns via a one-off `drop_enrichment_columns`
run on affected tables. `nlp/near_dup.rs` stays if it has a non-embedding
caller; otherwise it goes too.

Frontend: `components/topics/` except what §C4 moved, `services/api/topics.ts`,
`curation.ts` (row queue), and their integration specs.

### E2. What `posts` loses

The Bluesky `posts` table was enrichable only through topics. After E1 it has no
enrichment. That is acceptable for a source that is not a ticket source; if
social posts should get Call A, that is a new decision, not a regression to
paper over.

### E3. CLAUDE.md

Project structure (engine crate description drops "NLP primitives" for what
survives), the Commands block (remove the `topics` lines, add `enrich eval`),
and the test-workspace note that the embedding model is no longer needed by
`scripts/build-test-template.sh`.

### E4. The captured test template

`testdata/workspaces/test/models/` and the topic columns in the fixture Parquet
disappear; `build-test-template.sh --check` will report drift until the
template is rebuilt. Coordinate with the captured-workspace plan's §B7 size
guard, which becomes moot.

---

## Execution order & sizing

| Wave | Commits | Depends on | Notes |
|---|---|---|---|
| A | 4 | — | Schema + hash + lock; ships with no behaviour change |
| B | 5 | A | Call A live on `issues`; usable value on its own |
| C | 4 | B | Needs summaries to induce from |
| D | 5 | A, C1–C3 | Mentions; feedback_category induction closes the loop with C2 |
| E | 3 | B, C, D live | Deletion; do not interleave with A–D |

Wave B is the first point at which a customer sees the new design. Wave E is
gated on a period of A/B running in anger, because deleting the fallback before
the replacement has classified a real corpus is the one ordering that cannot be
undone cheaply.

---

## Verification

- `cargo test --workspace`, `cargo clippy --workspace --all-targets`,
  `cargo fmt --check`, `./scripts/audit.sh` (new `whatlang` dependency),
  `cargo build --release`.
- Mock-provider integration test (§B3) for both kinds, asserting column dtypes,
  the mention table's PK uniqueness, and that a ticket re-enriched with fewer
  mentions leaves no stale rows.
- Prompt prefix stability: a test that the first N bytes of the rendered
  request are identical across two tickets for the same spec — the property
  prefix caching depends on.
- `enrich eval` on the three seed languages before enabling Call B for any of
  them; the numbers go in a dated report under `reports/`.
- `./scripts/build-test-template.sh --check` clean after E4.

---

## Out of scope

- Embedding retrieval for entity resolution — evaluated and rejected; hierarchy
  is the answer to catalog size.
- Routing to specialist queues.
- Spans (transient-quote resolution or turn indices) until a transcript source
  exists.
- Ticket/transcript connectors beyond GitHub (Zendesk, call transcripts,
  surveys) — each is a Lua connector plus a `DocDisplay` entry plus, for
  transcripts, the span decision.
- Account/segment joins (market, tier, region) — no account record exists;
  when one does it is a join key on the ticket row, not an extraction.
- Cache economics beyond §A3: model-alias drift, pruning policy, rename-free
  invalidation variants.
- Encryption at rest, erasure, and retention. Flagged, not planned here: Call A
  and B put derived personal data (summaries) in `enrichment_cache` and ship
  ticket text to a processor, so the erasure path must cover Parquet rows, the
  cache, `connector-output/`, and the provider's retention — a plan of its own.

---

## Open questions

1. **Rename-free invalidation (Decisions, default).** Hashing descriptions but
   not names means a rename never re-calls. If the author would rather treat
   every vocabulary edit as a recalibration event, drop the distinction and
   §C3's `redefine` action collapses into `rename`.
2. **Provider.** Prefix caching's discount is provider-specific (steep on some
   native APIs, partial on others, compute-only for local serving) and the
   client speaks only the OpenAI dialect. Which EU-hosted endpoint is the
   target decides whether §B4's `cached_tokens` ever reads non-zero. Verify
   current pricing before the catalog-size decision leans on it.
3. **Cost in currency.** Tokens per language are measured (§B4); a price table
   per provider/model would turn them into euros. Worth doing, but it is
   configuration that rots — should it be a settings row or left to a
   spreadsheet?
4. **Pairwise confusion.** Needs either a second labeller run or human labels.
   The retired `document_labels` table was that; is a small human-labelled set
   per customer worth keeping as a `vocabulary_eval` table?
5. **`posts` after Wave E** (§E2) — leave unenriched, or extend Call A?
6. **Recalibration cadence and budget.** A subcategory recalibration re-calls A
   for every ticket. Quarterly at N tickets is the budget line; who approves it,
   and should the `redefine` response's cell count be a hard gate above some
   threshold?
