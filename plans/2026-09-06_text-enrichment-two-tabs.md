# Plan: Text enrichment — two tabs, one tree, undo over approval

**Status:** Proposal — decisions marked *confirmed* are settled with the author
on this date; *default* are my assumptions until reviewed.
**Date:** 2026-09-06
**Context:** Follows the author's first end-to-end use of the Text enrichment
tool on the neofetch issues table (~2.2k rows) after
[`2026-09-06_text-enrichment-naming-and-sentiment.md`](2026-09-06_text-enrichment-naming-and-sentiment.md)
landed. This round is about the **category flow**; the extraction flow is
moved, not redesigned, and gets its own round later.

---

## Context

What the review found, in the order the author raised it:

- **Three panes, wrong seams.** `TextEnrichmentView.vue` splits the tool into
  Setup / Results / Activity. Setup holds both function cards and the
  vocabulary tree; Results holds row counts per category, the mention table,
  the unresolved-subject queue and a "vocabulary health" list; Activity is
  the global `ActivityFeed`, which takes no props and shows every action on
  every source.
- **Categories are shown three times.** The vocabulary tree (definitions,
  actions), the "Rows" list (counts per category and subcategory, from
  `ticketsApi.summary`) and "Vocabulary health" (entries, rows, other-rate,
  out-of-band shares, from `vocabularyApi.health`) are one tree with
  different numbers attached.
- **Proposals detour through approval.** `VocabularyPanel.induce()` sends
  `mode: 'propose'`. The backend defaults to `auto_apply`
  (`agent/types.rs`: "every action the agent runner hands out is undoable —
  reversibility, not pre-approval, is the safety mechanism"), the Insights
  view runs its agents auto-apply with an "Undo all" button, and
  `docs/human_ai_interaction.md` principle 3 is "Undo over approval". The
  panel is the outlier.
- **Subcategories are proposed one parent at a time.** The backend locks
  runs per `(kind, source, table, parent)`, so parents can run concurrently;
  nothing in the UI does.
- **Tiny subcategories.** After a full run, some subcategories hold one or
  two rows — items, not groups. The balance band in
  `engine/enrichment/vocabulary.rs` is relative (2 %–40 % of the level), so on
  a 60-row category a 1-row entry passes. The induction prompt has no floor
  either.
- **Induction reads 200 summaries.** `agent/sampling.rs::INDUCTION_SAMPLE`.
  At ~20 tokens per summary (id + ~12-token clause + JSON framing) that is a
  ~4k-token prompt; the author wants a wider read.

The two calls split the vocabularies cleanly, which is what makes the tab
split a data boundary rather than a grouping: `vocab.rs::load` gives the
classify call categories and subcategories only; the extract call gets
products, competitors and feedback categories, and feedback categories are
induced from *extracted mentions* (`feedback_summary_sample`), not from row
summaries.

Two facts that shape the design and are stated so nobody rediscovers them:

- **A vocabulary edit misses the whole classify cache.** `define` calls
  `vocab::refresh_snapshots`, which writes a new function version whose spec
  hash differs, so `staleRowCount` jumps to the table size. The full tree
  therefore costs three classification runs (seed → after categories → after
  subcategories). Fine on 2.2k rows; the per-run row roof that makes it fine
  on 92k is the next round, not this one.
- **Right after a proposal, every new subcategory has zero rows.** Health
  computes from materialised columns, so "too small" and "not run yet" look
  identical until the next run. The UI must tell them apart.

## Decisions reached

- **Confirmed — two tabs, *Classification* and *Extraction*.** Classification
  = the classify function, the category/subcategory tree, the sentiment and
  language line. Extraction = the extract function, products, competitors,
  feedback categories, the mention table, the unresolved queue. Results and
  Activity panes go away.
- **Confirmed — the function card collapses to a header once set up.** One
  line: name · version · text columns · provider · rows computed of total ·
  stale note · *Run* · *Edit*. Edit expands the current editor (sample test
  included). A missing function shows the current create form.
- **Confirmed — one tree.** Each entry row: name, definition, rows classified,
  share of its level, out-of-band / too-small badge, audit line, actions. The
  fixed `other` row per induced level keeps its rate. The separate "Rows" and
  "Vocabulary health" sections are deleted, not moved.
- **Confirmed — proposals auto-apply.** The panel drops `mode: 'propose'`.
  After a run finishes the panel shows a result line — "Run #12 · 7 applied ·
  *Undo run*" — backed by the existing undo-all endpoint. No per-entry undo
  this round (simplest to build and to use).
- **Confirmed — Activity becomes a page under Settings** in the sidebar,
  beside Sources / Schedules / System. The Activity tab here and the Insights
  slideover are both removed. Everything is still logged; it is just not
  shown inside the tools.
- **Confirmed — *Propose all subcategories* is a frontend fan-out.** One run
  per eligible parent (≥ 50 classified rows), started in parallel; the panel
  shows "3 of 7 done" and one result line per run. No backend change; no
  skipping of parents that already have subcategories (define is an upsert by
  name, and the prompt already says not to re-propose).
- **Confirmed — induction sample 200 → 500.** ~10k-token prompt. One
  constant, no cache impact (induction prompts are never cached).
- **Confirmed — minimum five rows per entry, enforced twice, never pruned.**
  In the induction prompt ("do not propose an entry that would hold fewer
  than five of these summaries; fold it into a broader one") and in health
  as an absolute floor beside the relative band. Nothing deletes
  automatically: a delete is a vocabulary edit and misses the cache.
- **Default — the too-small badge is suppressed while the function is
  stale** (`staleRowCount > 0`). That is the one signal that says the counts
  were computed against the current vocabulary.
- **Default — the tab is remembered per session only** (a `ref`, as today's
  pane is). No route change.
- **Default — the extraction tab is a straight move.** No CSV import button,
  no new copy, no layout work beyond what the shared components force.

## §0 — Backend: sample, prompt floor, health floor (2 commits)

### 0a. Sample and prompt

- `agent/sampling.rs`: `INDUCTION_SAMPLE = 500`. Module doc's "200 of them"
  becomes "500 of them" with the same cost argument.
- `agent/runner.rs::induction_system`: one sentence after the cap paragraph —
  "Do not propose an entry that would hold fewer than five of these
  summaries; fold it into a broader one." The `MIN_ROWS_PER_ENTRY` constant
  from 0b is formatted in so the two floors cannot drift.
- Test: a unit test in `runner.rs` asserting the rendered system prompt
  contains the floor sentence with the constant's value (the file already
  has a `tests` module; `induction_system` is pure).

### 0b. Absolute floor in health

- `brightflow-engine/src/enrichment/vocabulary.rs`: `pub const
  MIN_ROWS_PER_ENTRY: usize = 5`; `LevelHealth` gains `too_small: Vec<(String,
  usize)>` — entries (not `other`) with `0 < rows < MIN_ROWS_PER_ENTRY`.
  Zero-row entries stay in `unbalanced` only: an entry nothing has been
  classified into is "unused", which the band already reports, and after a
  proposal every new entry is zero until the next run.
- `brightflow-api/src/enrichment/types.rs`: `VocabularyLevelHealth` gains
  `too_small: Vec<TooSmallEntry { name, rows }>`; `VocabularyHealthResponse`
  gains `min_rows_per_entry: usize` next to `induction_min_rows`, same
  rationale (the UI states the rule instead of guessing it). The `From` impl
  in `types.rs` maps the new field.
- Regenerate `brightflow-app/src/types/generated/` (pre-commit regen).
- Tests: extend `health_reports_other_rate_and_balance_band` with a 3-row
  entry that lands in `too_small` and not (only) in `unbalanced`; assert a
  0-row entry is not in `too_small`; `health_on_empty_data_is_all_zero`
  asserts the new vec is empty.
- Integration tests in `crates/brightflow-api/tests/` do not assert the
  health shape (checked); the field is additive.

## §1 — Activity page (1 commit, frontend only)

- `router.ts`: `{ path: '/activity', name: 'activity', component: () =>
  import('./components/actions/ActivityView.vue') }` beside `/system`.
- `components/actions/ActivityView.vue` (new): the page shell the other
  Settings pages use, wrapping the unchanged `ActivityFeed`. Header comment
  states the contract: the feed is global, not scoped to a source.
- `AppSidebar.vue`: an *Activity* item (`i-lucide-history`) in the Settings
  group after System.
- `InsightsView.vue`: remove the floating Activity button, the slideover,
  `activityOpen` and the `ActivityFeed` import; re-read the module header.
- `TextEnrichmentView.vue`: the `activity` pane and its import go (the rest
  of the file is rewritten in §2 anyway; this commit only removes).
- `VocabularyPanel.vue` delete confirm: "Undoable from the activity feed" →
  "Undoable from Activity under Settings".
- `useCommandPalette.ts`: the Settings commands list (Schedules, System, …)
  gains *Activity* with the same `go('activity')` shape.

Carve-out: `.vue` and route wiring, no unit test. `npm run check` is the
gate.

## §2 — Classification tab (4 commits)

### 2a. View and panes

- `TextEnrichmentView.vue`: `type Pane = 'classification' | 'extraction'`,
  labels *Classification* / *Extraction*, default `classification`. Header
  comment rewritten: two tabs, one per text function, each holding its
  function, the vocabularies that function reads, and what it materialised.
- `ClassificationPane.vue` (new): `FunctionCard kind="ticket_classify"`,
  then `VocabularyTree` (2c) for categories, then the sentiment / language
  line lifted from `ResultsPane.vue`.
- `ExtractionPane.vue` (new, §3).
- `SetupPane.vue` and `ResultsPane.vue` deleted once §3 has taken the
  mention table and the unresolved queue.

### 2b. Collapsed function header

`FunctionCard.vue`:

- `expanded = ref(false)` when `fn != null`, `true` while creating. The
  header row gains the summary: `fn.name · v{version} · {text_columns} ·
  {provider label} · {total − staleRowCount} of {total} rows`, plus "· N rows
  to recompute" when `staleRowCount > 0`. `total` comes from the
  `tables-index` query `SetupPane` already fetched for column names — the
  same query moves into the card (or stays in the pane and is passed down;
  default: passed down as `totalRows`).
- Buttons in the header: *Run* (opens `RunAllModal`), *Edit* (toggles
  `expanded`). *History*, *Delete* and *Save* move inside the expanded body.
  `ActiveRunBar` renders in both states.
- The provider label needs the provider list; `TicketFunctionEditor` fetches
  it under key `['llm-providers']`, so the card reuses the same key.
- Header comment re-read: the card now owns expand state; say so.

Carve-out; no unit test.

### 2c. One tree

`VocabularyPanel.vue` becomes `VocabularyTree.vue` with a `kinds: VocabKind[]`
prop selecting which levels to render (Classification passes `['category']`,
Extraction `['feedback_category', 'product', 'competitor']`). Rendering
stays as it is except:

- **Counts and shares.** The panel gains the `tickets-summary` query
  (`ticketsApi.summary`, same key `ResultsPane` used). Each category row
  shows `rows` and `share` of classified rows; each subcategory row shows
  `rows` and share of its parent. Feedback / product / competitor levels
  have no row grain and show nothing extra.
- **Badges.** Per entry: `warning` when its name is in the level's
  `unbalanced` ("share out of band") or `too_small` ("N rows — too small to
  be a subcategory"); the latter only when the classify function's
  `staleRowCount === 0`, which the pane passes down as `stale: boolean`.
  The `other` rows keep the other-rate badge.
- **Pure helpers in a sibling module** `vocabularyTree.ts` (new, with
  `vocabularyTree.test.ts`): `buildLevelRows(entries, summary, health,
  opts)` merging taxonomy entries with counts, shares and flags into the
  row shape the template iterates, and `eligibleParents(categories, health,
  minRows)` for 2d. This is the one piece of real logic in the round and the
  place rule 2 applies.
- The "Vocabulary health" and "Rows" sections in `ResultsPane` are deleted
  in this commit; nothing they showed is lost.
- Module header rewritten: no "reviewable diff" (it never was one; a run is a
  set of defines), auto-apply stated as the panel's contract, `other` and
  floor rules kept.

### 2d. Auto-apply, run result line, propose all

`VocabularyTree.vue`:

- `induce()` drops `mode: 'propose'` (backend default is `auto_apply`).
- Subscribe to the pushed `agentRun` frame (`connection.onMessage('agentRun',
  …)`, the guard `AgentActions.vue` uses) for run ids this panel started.
  On `completed` / `failed`: refresh taxonomy + health, and append a result
  line `Run #{id} · {detail head} · Undo run` (`detail` is the runner's
  "{n} applied" summary; the head is its first line). *Undo run* calls the
  API client's `undoAll(runId)` and refreshes. Lines clear on the next
  proposal from the same level.
- *Propose all subcategories* button on the Categories level header,
  enabled when `eligibleParents(...)` is non-empty, disabled with "No
  category has 50 classified rows yet" otherwise. Click: `Promise.all` over
  `agentApi.start` per parent; a `pending: Set<number>` of run ids drives a
  "3 of 7 done" badge; each completion produces its own result line as
  above. A 409 (run already active for that parent) is shown on that line
  and skipped.
- The per-category *Propose subcategories* button stays.
- `lastRunNote` copy no longer mentions the activity feed.

Unit test: `eligibleParents` in `vocabularyTree.test.ts` (below minimum,
at minimum, already has children → still eligible). The WS wiring and the
buttons are carve-outs.

## §3 — Extraction tab (1 commit, move only)

- `ExtractionPane.vue`: `FunctionCard kind="ticket_extract"`,
  `VocabularyTree :kinds="['feedback_category','product','competitor']"`,
  then the mention table and the unresolved-subject queue moved verbatim from
  `ResultsPane.vue` (their queries, `resolve`, `applyMappings` included).
- Delete `ResultsPane.vue` and `SetupPane.vue`; `components.d.ts`
  regenerates.
- No copy or layout changes beyond the move. Header comment states the
  contract: everything the extract call reads or writes, and nothing the
  classify call does.

## Execution order & sizing

| Step | Commits | Depends on | Note |
|---|---|---|---|
| §0 | 2 | — | backend; regen types before §2c |
| §1 | 1 | — | frontend; independent of §0 |
| §2a–b | 2 | §1 | view + header; `ResultsPane` still mounted nowhere after 2a — keep the file until §3 |
| §2c | 1 | §0b, §2a | tree with counts and badges; the only unit-tested code |
| §2d | 1 | §2c | auto-apply, undo line, propose all |
| §3 | 1 | §2c | move; deletes the old panes |

Each commit: `cargo fmt --check`, `cargo clippy`, `cargo test -p
brightflow-engine`, `cargo test -p brightflow-api`, `npm run check`, `npm run
test`; `./scripts/check-conventions.sh` via the pre-commit hook;
`cargo build --release` at the end of §0.

## Verification

Testing follows the project's own tiers and nothing else — no browser
automation, no E2E runner. The tiers are:

- **Rust unit** — `cargo test -p brightflow-engine` (health floor),
  `cargo test -p brightflow-api --lib agent::runner` (prompt floor).
- **Rust integration** — `cargo test -p brightflow-api` as a whole;
  `ticket_enrichment.rs` and `action_log.rs` must stay green (nothing here
  changes their contracts).
- **Frontend unit** — `npm run test`: `vocabularyTree.test.ts` for
  `buildLevelRows` (counts, shares, `too_small` suppressed when stale,
  `other` never flagged) and `eligibleParents`; `stores/curation.test.ts`
  untouched.
- **Frontend integration** — the `enrichment.integration.test.ts` tier
  against the committed test workspace stays green unchanged; it does not
  read the health endpoint (checked), and the new field is additive.
- **Template drift** — `./scripts/build-test-template.sh --check`; the
  committed template holds one classify function and no cached cells, so
  no drift is expected.
- **Manual, on the neofetch workspace with backend and frontend running as
  background tasks** (the workflow in `CLAUDE.md`): open Text enrichment →
  Classification; the header is collapsed and shows rows computed; *Propose*
  categories runs, the tree fills without a visit to Activity, the result
  line offers *Undo run*, and undo empties it again; run classification;
  *Propose all subcategories* starts one run per category with ≥ 50 rows
  and reports "n of m done"; re-run classification; entries under 5 rows
  carry the badge, and none did while the header said rows to recompute.
  Settings → Activity lists every action above. Extraction shows the extract
  card, the three vocabularies, mentions and the queue exactly as before.

## Out of scope

- **Per-run row roof** for classification / extraction (the "seed run").
  Next round; the collapsed header already shows computed-of-total so the
  roof drops in without a redesign.
- **The extraction flow** beyond the move: CSV import UI for products and
  competitors, mention-table design, unresolved-queue ergonomics.
- **Per-entry undo** in the tree.
- **A backend "propose all" run kind** with one progress row and one undo.
  The fan-out is enough until it is not.
- **Automatic pruning** of too-small entries.
- **Route-level tab state** (deep links to a tab).

## Open questions

1. Header summary source: pass `totalRows` from the pane, or move the
   `tables-index` query into the card? (Default: pass down.)
2. Should the *Propose all* button also appear on the feedback-category level
   in Extraction? It has no parents, so no. Confirming the reading.
3. When several parents' runs finish, one result line per run can stack to
   ten lines. Collapse into one line with a count once all are done, or leave
   the stack? (Default: leave; they clear on the next proposal.)
