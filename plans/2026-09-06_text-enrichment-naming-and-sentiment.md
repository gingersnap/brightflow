# Plan: Text enrichment — naming, visible `other`, one sentiment field

**Status:** Proposal — decisions marked *confirmed* are settled with the author
on this date; *default* are my assumptions until reviewed.
**Date:** 2026-09-06
**Context:** Follows a review of the enrichment code against the author's
first use of the Text analytics tool on real data (two GitHub-issue tables,
~92k classified rows). Builds on
[`2026-08-30_two-call-ticket-enrichment.md`](2026-08-30_two-call-ticket-enrichment.md)
and the same-day removal plan. Assumes the uncommitted work in the tree at
the time of writing (sentiment `none` folded into `neutral`, cache bypass for
sample runs, the Mistral-safe re-ask) lands first — see §0.

---

## Context

What the review found, in the order the author raised it:

- **Naming.** The UI says *Text analytics*; every crate, module, CLI
  subcommand and API client says *enrichment*. The two functions are shown as
  *Classification* and *Mention extraction*; the first also writes the
  summary, the second is the only extraction there is. User-facing prose says
  *ticket* in several places (setup-pane header, editor copy, kind names),
  although the same tool runs on Bluesky posts.
- **Subcategories are built but hidden.** `propose_subcategories` exists end
  to end (agent kind, per-parent sampling, executor pinning, the panel button)
  but the button is an icon-only ghost on each category row, and the run's
  precondition — at least 50 tickets already classified under that parent —
  is stated nowhere in the UI. The author could not find it.
- **The cap is a maximum.** The induction prompt says "AT MOST {cap} … if the
  corpus needs more, group". Three entries is a legal outcome. No change
  needed beyond copy.
- **`other` is implicit.** Present in the prompt, the schema and the
  validator at both levels; never shown as a row; excluded from the cap count.
  The author wants it visible.
- **Sentiment is two fields that carry one signal.** Of ~92k cached cells,
  neutral is always `low` and negative is mostly `strong`; positive is mostly
  `low`. The author's stronger argument: a model cannot hold a consistent
  line on *low vs strong* across a corpus, so the field looks like signal and
  is not.
- **Mention polarity is a second copy of the same enum.** The extraction
  call gives every mention its own polarity ("support great, export broken":
  service positive, feedback negative). The results pane shows it as a
  per-subject negative share. The uncommitted work cut the shared constant to
  four values, but the extraction *prompt* still offers five — schema and
  prompt disagree in the working tree today.

## Decisions reached

- **Confirmed — the tool is *Text enrichment*.** Not *ticket*: the tool runs
  on any text table, and "enrichment" is the codebase's word. *Text* leaves
  room for a future sibling — rule enrichment (threshold labels, buckets,
  derived flags computed by Polars with no LLM and no cache) — which is
  enrichment but not text enrichment.
- **Default — the tool *id* is renamed too** (`textanalytics` →
  `textenrichment`): backend enum variant, route path and name, palette
  regex, generated type, component folder and API client file. Nothing
  persists the id, so there is no migration; only bookmarked URLs break, and
  the product has no users to break them for. Veto this and §1 becomes a
  label-and-folder rename only.
- **Confirmed — the two functions are *Summary and classification* and
  *Extraction*.** Display only. The kind ids `ticket_classify` /
  `ticket_extract` sit inside the `enrichment_functions.kind` CHECK
  (migration 025) and stay.
- **Confirmed — user-facing prose stops saying *ticket*.** "Text functions",
  "one row", "a text". Internal identifiers keep `ticket_` where a rename
  would touch a CHECK constraint or the cache.
- **Confirmed — `other` is shown** as a fixed row at every induced level
  (categories, each category's subcategories, feedback categories), with its
  definition and the level's other-rate from the health endpoint. Not
  counted in `n / 10`. No actions. Products and competitors have no `other`
  (an unlisted name becomes an unresolved subject) and are unchanged.
- **Confirmed — one sentiment field.** `sentiment` ∈ {`neutral`, `mixed`,
  `positive`, `negative`}. `sentiment_strength` is deleted, not deprecated.
- **Confirmed — mentions use the same four values,** from the same constant.
- **Default — the mention column is renamed `polarity` → `sentiment`** so the
  two grains use one word. The mentions table is rebuilt wholesale on every
  materialisation, so this is a rename in the builder, not a migration.
- **Default — the per-subject results table shows negative *and* positive
  share.** Today it computes negative only; with `mixed` a real value at
  mention grain, one column no longer tells the story. The author did not
  decide this; it is the one place §4 adds rather than removes.
- **Default — the subcategory button becomes discoverable**: a labelled
  button per category, disabled with a stated reason when fewer than 50 rows
  are classified under that parent. The 50-row minimum is exposed on the
  taxonomy overview so the panel can say it without a failed run.
- **Confirmed — no change to the cap.** Copy says "up to 10".

## §0 — Land the in-flight work first (1 commit, already written)

The working tree carries three unrelated fixes: sentiment `none` folded into
`neutral` (engine constants, prompt, validator, tests), `CacheMode::Bypass`
for sample runs, and the fresh-single-turn re-ask that Mistral accepts.
Commit them as they are, *plus* the one-line fix they missed: the extraction
prompt in `mentions.rs` still lists `none` while its schema (which imports
`POLARITY_VALUES`) no longer offers it. That line is corrected here so §4
starts from a consistent prompt.

## §1 — Rename the tool (2 commits, no behaviour change)

### 1a. Backend id and generated type

- `SourceTool::Textanalytics` → `Textenrichment` in `sources/types.rs`
  (serde kebab-case makes the wire value `textenrichment`), both profile
  lists in `sources/profiles.rs`, the doc comment reworded to "summary,
  classification and extraction over an LLM".
- Regenerate `brightflow-app/src/types/generated/SourceTool.ts`.
- Integration tests in `crates/brightflow-api/tests/` that assert the tool
  list, if any, follow.

### 1b. Frontend id, label, folder, copy

- `types/index.ts`: tool entry `textenrichment: { label: 'Text enrichment', … }`.
- `router.ts`: path `/:sourceId/textenrichment/:table`, name
  `textenrichment-table`; the editor's `RouterLink` (touched in the working
  tree) follows.
- `useCommandPalette.ts`: the route regex and `isTextAnalytics` → the new
  name; the fuse-key comment.
- `components/textanalytics/` → `components/textenrichment/`;
  `TextAnalyticsView.vue` → `TextEnrichmentView.vue`; `SourceLayout.vue`
  import and comment; `components.d.ts` regenerates.
- `services/api/textanalytics.ts` → `textenrichment.ts`; the barrel and the
  integration test import.
- Copy: `FunctionCard.vue` title map → *Summary and classification* /
  *Extraction*; `SetupPane.vue` header *Text functions* and the one-paragraph
  explainer ("Two LLM calls per row…"); `TicketFunctionEditor.vue` prose
  ("Summarises and classifies every row…", "Extracts every mention…");
  `ResultsPane.vue` empty states ("run Summary and classification first",
  "run Extraction first"); module headers on every file touched.

No unit test: `.vue` files and re-export edits are carve-outs. `npm run
check` and the integration tier are the gate.

## §2 — Visible `other` and a findable subcategory button (2 commits)

### 2a. `other` as a row

Frontend only. `VocabularyPanel.vue` already reads the taxonomy overview;
it gains the health query (`vocabularyApi.health`, same key as
`ResultsPane`) and renders one fixed trailing row per induced level:

- name `other`, description "none of the above — always available, never
  proposed", a muted badge with the level's `otherRate` (warning colour past
  0.15, matching the results pane), no action buttons, not counted in the
  `n / cap` badge.
- Under a category: the same row inside its subcategory list, using that
  parent's `VocabularyParentHealth`.
- Under feedback categories: the root-level feedback row.
- Products and competitors: nothing.

The `is_other` / cap logic in the engine is untouched; the panel mirrors the
rule the prompt already states ("available at both levels").

### 2b. Subcategory induction you can find

- **Backend:** `TaxonomyOverviewResponse` (or the health response — whichever
  the panel already holds; default the overview) gains, per root category,
  `classifiedRows: usize` — rows whose materialised `category` equals that
  entry's name — and the top-level constant
  `inductionMinRows: 50` exported from the runner's
  `MIN_SUMMARIES_FOR_INDUCTION`. Pure counting goes through `value_counts`
  in `health.rs`, which has tests; the new field gets one.
- **Frontend:** the ghost icon becomes a labelled `Propose subcategories`
  button on each category row; disabled with a tooltip "Needs 50 rows
  classified as *{name}* — has {n}" when under the minimum; the root
  "Propose" copy says "up to 10". The empty-state line under a category
  reads "No subcategories — propose from the {n} rows classified here, or add
  by hand".

## §3 — One sentiment field on the classify call (3 commits)

### 3a. Engine

`ticket_classify.rs`:

- `POLARITY_VALUES` → `SENTIMENT_VALUES: [&str; 4] = ["neutral", "mixed",
  "positive", "negative"]`; delete `STRENGTH_VALUES`.
- `ClassifyCell { …, sentiment: String }`; `sentiment_polarity` and
  `sentiment_strength` gone.
- `OUTPUT_COLUMNS: [&str; 5]` — `summary, language, category, subcategory,
  sentiment`.
- Prompt step 4 becomes the only sentiment step: "sentiment — neutral |
  mixed | positive | negative", keeping the two clarifications that earned
  their place (no evaluative content → neutral; never collapse mixed to
  neutral). Step 5 deleted.
- `tool_schema`: one `sentiment` enum property; `required` follows.
- `validate`: one `pick`.
- Tests: the fixture's `args()` takes one sentiment; the cross-product test
  becomes "every value is legal, unknown values and the old field names are
  rejected"; the prompt test asserts step 5 is gone.

Module doc: the two paragraphs justifying the `none`-less pair are replaced
by one sentence on why there is one field ("strength was not a judgement a
model holds consistently across a corpus").

The fingerprint hashes the rendered system prompt, so every existing
classify cache row misses automatically. No migration; a full re-run is the
cost, and it is the author's call when to pay it.

### 3b. API, CLI, tests

- `runner.rs` materialise builder: one `sentiment` column; the `value_json`
  test fixture; the `reask` test's error string can stay.
- `health.rs` `ticket_summary`: reads `OUTPUT_COLUMNS[4]` — still index 4,
  now `sentiment`; rename the local and the doc line.
- `materialize`'s stale-column drop only knows the *current* output names, so
  tables enriched before this change keep `sentiment_polarity` and
  `sentiment_strength` forever. Add a one-line list of *retired* output
  columns next to `OUTPUT_COLUMNS` in the engine (`RETIRED_COLUMNS`) and drop
  those too; test it with a frame that carries one.
- `brightflow-cli` `enrich eval`: `expected_polarity` → `expected_sentiment`
  in the row struct, the CSV reader, the score line; the three fixture CSVs
  under `testdata/eval/{sv,fi,en}/` rename the column; `README.md` there is
  dated prose and is updated in place only if it lists columns.
- Integration tests `ticket_enrichment.rs` and `enrichment_materialize.rs`:
  fake-provider payloads and column assertions.

### 3c. Frontend

- `TicketFunctionEditor.vue`: `previewNames` and `outputs` lists.
- `ResultsPane.vue`: the sentiment strip already iterates `tickets.sentiment`
  by value; no change beyond the comment.
- `brightflow-app/src/types/enrichment.ts` if it names the columns.

## §4 — Mentions: same values, same word (2 commits)

### 4a. Engine

`mentions.rs`:

- Import `SENTIMENT_VALUES`; the prompt bullet reads "`sentiment`: neutral |
  mixed | positive | negative, as the customer expressed it about THAT
  subject"; example 3 unchanged in meaning.
- `MentionCell.polarity` → `sentiment`; the schema property, `required` list
  and the validator's field name and error string follow.
- `MENTION_COLUMNS[9]` `"polarity"` → `"sentiment"`. The child table is
  rebuilt wholesale from the cache on every materialisation (module doc), so
  the old column disappears on the next run without a drop list.
- Tests: the builder fixture and the validation error test.

Cache: the extraction fingerprint hashes the prompt, so existing extract
cells miss. The default workspace has none; the eval fixtures' expected
mentions carry no polarity.

### 4b. API and results table

- `mentions_api.rs` `summarize_mentions`: `col("sentiment")`; add
  `positive_share` beside `negative_share` (same `eq(lit(..)).mean()`
  shape); the unit test frame renames its column and asserts both shares.
- `MentionSubjectStats` gains `positive_share`; regenerate the TS type.
- `ResultsPane.vue`: columns *negative* and *positive*.
- Integration test `ticket_enrichment.rs`: fake payloads use `sentiment`.

## Execution order & sizing

| Step | Commits | Depends on | Note |
|---|---|---|---|
| §0 | 1 | — | already written; add the prompt fix |
| §1 | 2 | §0 | pure rename, biggest diff, zero risk |
| §3 | 3 | §0 | invalidates the classify cache |
| §4 | 2 | §3 (shared constant) | invalidates the extract cache |
| §2 | 2 | §1 (folder) | frontend plus one overview field |

§3 and §4 land together in one push so the shared constant is never half
renamed. §2 is last because it is the only step with UI design in it and
the only one the author may want to see running before it is final.

Each commit: `cargo fmt --check`, `cargo clippy`, `cargo test -p <crate>`,
`npm run check`, `npm run test`; `cargo build --release` at the end of the
Rust work per `CLAUDE.md`.

## Verification

- `cargo test -p brightflow-engine` — new sentiment tests, retired-column
  drop, mention builder.
- `cargo test -p brightflow-api` — `ticket_enrichment`,
  `enrichment_materialize`, `action_log`, the mentions summary unit test.
- `./scripts/build-test-template.sh --check` — the committed template holds
  one `ticket_classify` function and no cached cells, so it should not
  drift; if it does, rebuild and commit the template in the §3 commit.
- Manual, on the default workspace: rerun Summary and classification on the
  smaller table (4.3k rows), confirm the five columns and no
  `sentiment_strength`; the vocabulary panel shows `other` with the level's
  rate; every category with ≥ 50 rows has an enabled *Propose
  subcategories*; run one and approve the diff; then run Extraction on the
  same table and confirm the mentions table shows both shares.
- `git grep -i "text analytics\|textanalytics\|sentiment_polarity\|
  sentiment_strength"` returns only dated plans.

## Out of scope

- **Rule enrichment** (Polars-computed threshold labels and buckets). Named
  here only to justify *Text* over *Ticket*; it is its own plan, and the
  function framework's cache and re-ask loop do not apply to it.
- **Renaming the kind ids** `ticket_classify` / `ticket_extract` and the
  agent kinds. They live in CHECK constraints and cache keys; the user never
  sees them.
- **Re-running the 88k-row table.** Its rows are all `other` because it was
  classified before any categories existed. Rerunning is the author's call
  and costs money; nothing here changes that.
- **The 55 % other-rate on the smaller table.** Possibly stale spec hashes,
  possibly a real vocabulary problem; diagnosing it is a data question, not
  a code change in this plan.

## Open questions

1. Rename the tool *id*, or label and folder only? (Default: id too.)
2. Results table: negative and positive share, or negative only? (Default:
   both.)
3. Should the `other` row show the rate from health, or just exist? (Default:
   show it — it is the one number that says whether the vocabulary is wrong.)
