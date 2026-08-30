# Plan: Remove the embedding / clustering / classifier path

**Status:** Executed the same day — recorded so the decision and its scope
are dated, not reconstructed from a diff.
**Date:** 2026-08-30
**Context:** Pulls Wave E of
[`2026-08-30_two-call-ticket-enrichment.md`](2026-08-30_two-call-ticket-enrichment.md)
forward. That plan gated deletion on the two calls running on a real corpus;
the author decided the old path is not a fallback worth keeping and asked for
the product to go fully to the new approach.

---

## Decisions reached

- **Confirmed — remove:** Model2Vec embeddings, k-means/HDBSCAN clustering,
  the linear classifier head and its curation (cluster edits, excluded terms,
  row-level labels, centroid reconciliation), the Topics tool, the Enrich tool,
  and the ad-hoc `llm_prompt` column function ("we add that later when we
  know where it makes sense").
- **Confirmed — keep:** Text Explorer, the insights engine, the Bluesky
  `posts` table as a plain table, the action bus and activity feed, the LLM
  provider settings.
- **Confirmed — build:** one *Text analytics* tool with three panes — Setup
  (vocabularies + the classify/extract functions with sample test and scoped
  run), Results (ticket grain, mention grain, unresolved-subject queue,
  vocabulary health), Activity (the action feed).

## What was removed

Engine: `embedding/`, `enrichment/{artifacts,config,curation,labels_io,
topic_enricher}.rs`, `nlp/{classification_metrics,clean,cluster_eval,
cluster_metrics,dense_clustering,density,linear,near_dup,reduce,similarity,
sparse,tfidf}.rs`, the `llm_prompt`/`topic_model`/`classifier` specs and
their helpers, tests `classifier_quality.rs` and `topics_quality.rs`,
dependencies `model2vec-rs`, `hdbscan`, `linfa`, `linfa-logistic`, `ndarray`,
`bincode`. `english_stopwords` moved to `nlp/stopwords.rs` for Text Explorer.

Store: migration 026 drops `document_labels`, `cluster_edits`,
`excluded_terms`, `table_enrichment_settings`; their rows, CRUD and the
label-carrying undo shapes are gone.

Scheduler: the pre-merge topic enrichment step.

API: `topics/` (display schema moved to `enrichment/display.rs`, the
vocabulary read endpoint to `enrichment/vocabulary_api.rs`), the cluster
actions and `label_document`, agent kinds `auto_label` / `propose_merges` /
`propose_taxonomy` / `label_documents`, the `llm_prompt` runner branch,
promote/demote, `enrichment_overrides` on `AppState`; `SourceTool` gains
`textanalytics` in place of `topics` and `enrich`.

CLI: `topics` subcommands. Template builder: the embedder prerequisite and
topic fit; the fixture ships a `ticket_classify` function instead.

Frontend: `components/topics/`, `components/enrich/` (the run bar, run modal,
version panel and ticket editor moved to `components/textanalytics/`), the
Topics and Enrich routes and tools, the cluster palette flows.

## What was added

`GET …/tickets/summary` (ticket grain), `components/textanalytics/`
(`TextAnalyticsView`, `SetupPane`, `FunctionCard`, `ResultsPane`, plus the
moved `VocabularyPanel` and `TicketFunctionEditor`), `services/api/
textanalytics.ts` (Text Explorer, vocabulary, mentions, tickets clients).

## Out of scope

Deleting the migrations that created the dropped tables (history stays
replayable); re-adding an ad-hoc column tool; a time axis on the ticket
grain (the insights engine's job once the columns exist).
