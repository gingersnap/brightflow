# Supervised topics: from format clusters to problem intent

## The problem

The Topics UI used to group issues by **surface format**, not by what a ticket is
about — clusters like `./backport.sh, force push, cherry picking`.

This is **structural, not a tuning bug**. Model2Vec `potion-base-32M` produces
static token-averaged embeddings whose dominant axis of variance is
format/vocabulary. Any *unsupervised* consumer of that geometry — k-means,
HDBSCAN, nearest-centroid, retrieval — follows that dominant axis. No choice of
`k`, cleaning profile, or distance metric fixes it.

Two things follow, and both were counter-intuitive enough to be worth recording:

**Better LLM naming cannot fix it.** The `auto_label` agent already produces the
polished names in the UI, and it works. "Automated Backport Commits" is an
*accurate* label for a cluster of backport commits. The LLM is faithfully naming
format clusters; the clustering is upstream of the problem.

**The old label loop was circular.** `refresh_label_artifact` built each label's
centroid from `clustering.centroids[i]` — the *cluster centroid*, not row
embeddings. So `predicted_label` was the cluster assignment wearing a nicer name,
and since clusters are format-shaped, cluster labels re-taught the format bias by
construction. **This is why labels attach to ROWS, not clusters.**

## The fix

Supervision is the one mode that defeats the format axis: labels let a trained
head *downweight* format-correlated dimensions and *upweight* content ones.
Nearest-centroid cannot — it weights every dimension equally by construction, so
it inherits the bias no matter how good the labels are.

It is also the one task family where static embeddings reach near-LLM quality
(MTEB Classification: potion-base-32M **71.70** vs MiniLM 69.25, within ~7 pts of
a 7B LLM embedder — against a ~25-pt deficit on retrieval).

### The invariant

> **LLM cost is O(taxonomy + seed sample), never O(rows).**

The LLM proposes an intent taxonomy and labels a stratified ~1–2k-ticket seed.
Humans correct both the category names and the per-ticket assignments. A
deterministic head trains on the curated result and labels all 50–100k rows
forever at matrix-multiply cost.

This upholds the standing stance in `research_insights_engine.md` (*"LLM is off
the path for analysis… narration only"*): the LLM never analyses. It proposes a
vocabulary that a human ratifies; the hot path stays deterministic.

## Hot path vs cold path

|                | Hot path (every row, every sync)                | Cold path (rare, sampled)                        |
| -------------- | ----------------------------------------------- | ------------------------------------------------ |
| **Cost**       | ~Free (static, in-process, cached)              | Bounded LLM, amortized                            |
| **Code**       | `enrich_with_topics` → `apply_topics`           | agent runs; actions; `fit_topics`                 |
| **Does**       | embed → **classify (trained head)** → cluster   | propose taxonomy → label seed → curate → **train** |

`maybe_enrich_parquet` never fits — it only applies artifacts. Once `apply_topics`
consumes a trained head, inference reaches every synced row automatically, free.

## Column contract

| Column                        | Value                                          |
| ----------------------------- | ---------------------------------------------- |
| `predicted_label` (String)    | argmax label — backward compatible              |
| `confidence` (f32)            | argmax **sigmoid score**                        |
| `predicted_labels` (String)   | comma-joined labels over threshold, best-first  |

`predicted_labels` reuses `label_names`' comma encoding so existing tag UI works.
Empty → **null**, never `""`.

Two sharp edges:

- **`confidence` changed meaning.** It was a raw cosine (~0.3–0.8); it is now an
  uncalibrated sigmoid (0–1). Neither is a probability — it is a ranking signal.
  Do not read 0.7 as "70% likely".
- **Under the centroid fallback `predicted_labels` is null.** A centroid model has
  no multi-label decision rule; faking one would make the column lie.

## Clusters are discovery, not the answer

Clusters still earn their place, but not as labels:

- **Discovery** — surfacing themes the taxonomy has not named yet.
- **Stratification** — drawing the seed sample evenly across the corpus. Format
  clusters are useless as labels but useful as a sampling device: we want
  *diversity* from them, not correctness.
- **The tail** — low-confidence rows and cluster outliers feed back into
  `propose_taxonomy`, closing the loop.

`predicted_labels` is the answer.

## Verification

The thesis is enforced as a test: `crates/brightflow-engine/tests/classifier_quality.rs`
plants intent labels crossed orthogonally with format strata, then asserts a
**fair** nearest-centroid baseline still loses. Fair means the same train/val
split, the same retained label set, and per-label thresholds tuned by the
identical protocol — it calls `nlp::fit_centroid_baseline`, the same function
`topics eval-classifier` runs on real data.

Current measured values (deterministic):

```
centroid cosine  shared-format=0.680  different-format=0.500
cohesion  by-intent=0.610  by-format=0.809
macro-F1  centroid-baseline=0.551  trained-head=1.000
```

The **vacuity guard** (`base_f1 < 0.60`) is the assertion most worth keeping: if
the baseline scores well, the corpus failed to reproduce the format confound and
the rest of the test proves nothing. It has already earned its keep — it caught
two corpus designs that looked right on paper and scored 0.99 and 0.76.

**Go/no-go on real data:** `topics fit` → `topics eval-classifier` prints head vs
baseline macro-F1. If the head does not win there, stop — the thesis is wrong for
that corpus.

## How many labels are enough?

Two thresholds gate training, and **both are counted against the training split,
not the full labelled set** — the head holds out `VAL_FRACTION` (15%) to tune
per-label thresholds and report F1. Quoting the raw constants at a curator would
tell them they were done while the head silently trained nothing.

- `MIN_TRAIN_ROWS = 50` → **59 labelled rows** before training starts at all.
- `MIN_LABEL_SUPPORT = 20` → **~24 examples per category** before that category
  survives; under-supported categories are dropped from the head (but *not* from
  the centroid fallback, which is why `fit_centroid_baseline` takes the head's
  `retained_labels`).

Never hardcode 59 or 24. Call `nlp::linear::min_labelled_rows_to_train()` and
`nlp::linear::min_examples_per_label()` — they derive from the constants and the
split's rounding, so they cannot drift. The curation UI and
`topics eval-classifier` both report these numbers.

## Known sharp edges

- **Seed quality caps everything.** The head can only be as good as ~1–2k curated
  labels. That human review time is the real cost of this feature.
- **Undo of `define_taxonomy_category` is refused once the category has labels.**
  The undo op is captured at define time, before any labels exist, so it has no
  snapshot to restore — and the FK cascade would take the labels with it.
  `delete_taxonomy_category` is the deliberate path; it snapshots the labels and
  restores them under the original `category_id` (the id matters — labels
  reference categories by it).
- **`refresh_label_artifact` still writes cluster centroids to `labels.bin`,**
  while the engine's `build_dense_label_centroids` writes row centroids to the
  same filename. Two writers, semantically different objects, one file. The
  classifier supersedes both as the labeler; the centroid path survives only as
  the fallback that keeps pre-classifier artifact dirs working.
- **`topic_cluster` drifts.** It is written to parquet but never read back — the
  API resolves names from `clusters.bin` via the curation overlay, keyed on
  `topic_cluster_id`. A rename updates the UI but not the column. Treat
  `topic_cluster_id` as the identity.
- **`DocDisplay::for_table` hardcodes `issues→label_names`** — a second source of
  truth for the label column. Not unified; recorded here.
- **Near-duplicate detection is quadratic and its cap is not a latency bound.**
  `MAX_NEAR_DUP_ROWS = 100_000`, but measured on a real corpus: 2.3k rows ≈ 6s,
  which extrapolates to ~45 min at 71k rows and ~85 min at the cap itself. A
  71k-issue table therefore runs for the better part of an hour *without*
  tripping the cap. Run it on a filtered slice, or lower the cap to ~25k
  (≈6 min) if the current value ever bites.
