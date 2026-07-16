//! **The thesis, as a test.**
//!
//! The claim under test is the whole reason the supervised head exists:
//!
//! > Static token-averaged embeddings put their dominant axis of variance on
//! > surface *format*, not problem *intent*. Any unsupervised consumer of that
//! > geometry — including nearest-centroid over perfect labels — follows that
//! > axis and confuses intents that share a format. A *trained* head can
//! > downweight the format dimensions and recover the intent.
//!
//! # Corpus design (load-bearing — do not "simplify")
//!
//! Intent is planted **crossed orthogonally with format strata**:
//!
//! Each doc is 14 tokens: **10 format + 2 generic + 2 intent**, so ~70% of the
//! vector's mass is format and only ~14% carries any intent signal at all.
//!
//! * Format correlates with intent only **partially** (55/45), and two intents
//!   share the same dominant format. That drags their centroids toward
//!   collinearity, which is exactly what nearest-centroid cannot undo.
//! * **Format vocabulary is dense; intent vocabulary is sparse.** A doc draws 10
//!   of a 12-token format pool (same-format docs overlap ~83%) but only 2 of a
//!   36-token intent pool (same-intent docs overlap ~6%). Faithful: backport
//!   boilerplate is the same handful of words every time, while there are many
//!   ways to describe an auth failure.
//! * **Most non-format words are generic** ("error", "failed", "broken"). They
//!   are drawn identically for every intent, so they carry no signal — they just
//!   consume mass, exactly as real complaint language does.
//!
//! Every one of those properties was arrived at by measurement, and the earlier
//! failures are worth remembering because each looks fine on paper:
//! * 70% format *by mass*, small disjoint intent vocabulary → baseline **0.99**.
//!   Mass is not enough: a clean, cross-talk-free intent signal decides every
//!   comparison once the shared format terms cancel.
//! * Add sparsity (4-of-20) → **0.76**. Better, but individually perfect
//!   diagnostic tokens still accumulate into a decisive signal.
//! * Add generic filler (2 intent tokens of 14) → **0.67**.
//! * Widen the intent pool to 36 and relax the mix to 55/45 → **0.55**.
//!
//! The lesson: what reproduces the bug is *diluting the diagnostic signal* —
//! few intent tokens, rarely repeated, buried in generic filler — not simply
//! piling on more format text.
//!
//! The failure mode this produces is concrete: a *billing* doc that happens to
//! be *template*-formatted scores higher against the *perf* centroid (whose
//! dominant format is template) than against its own. Format wins. That is the
//! bug, reproduced.
//!
//! # Fairness (mandatory — otherwise this test is theater)
//!
//! The baseline is not a strawman. It comes from `nlp::fit_centroid_baseline` —
//! the very function `topics eval-classifier` runs on real data — which gives it
//! the **same train/val split**, the **same retained label set**
//! (`MIN_LABEL_SUPPORT` drops rare labels from the head but not from centroids,
//! so both must score the same label set), and **per-label thresholds tuned by
//! the identical protocol**. Sharing the implementation rather than
//! reimplementing it here is what stops this test's notion of "fair" from
//! drifting away from production's.
//!
//! The real claim is that a *fair* baseline still loses.
//!
//! Assertions are on **macro**-F1. Micro would pool the labels and hide the
//! very confusion under test behind the labels that work.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_precision_loss,
    clippy::indexing_slicing,
    clippy::print_stderr,
    clippy::cast_possible_truncation
)]

use brightflow_engine::nlp::classification_metrics::macro_f1;
use brightflow_engine::nlp::{fit_centroid_baseline, fit_multilabel_linear, train_val_split};

/// Matches the real embedder's width. Kept modest on purpose: the head trains
/// C one-vs-rest L-BFGS fits over this many features, and a wide dim makes the
/// test unusably slow in a debug build.
const EMBED_DIM: usize = 512;
const DOCS_PER_INTENT: usize = 250;
/// Dense: 10 drawn from a 12-token pool => same-format docs overlap ~83%.
const FORMAT_TOKENS_PER_DOC: usize = 10;
/// Generic complaint words, shared by every intent. They occupy half the
/// non-format budget and carry ZERO intent signal — real tickets are mostly
/// "error / failed / broken", not diagnostic vocabulary.
const GENERIC_TOKENS_PER_DOC: usize = 2;
/// The only genuinely diagnostic tokens: 2 drawn from a 36-token per-intent
/// pool (so same-intent docs overlap only ~6%). Sparse AND few — this is the
/// thin thread the head must find.
const INTENT_TOKENS_PER_DOC: usize = 2;
const INTENT_POOL: usize = 36;

/// Dominant format share, as `DOM_NUM / DOM_DEN` — 55/45.
///
/// Counter-intuitively a *weaker* correlation is more adversarial. Measured
/// sweep of the centroid baseline over this parameter:
///
/// ```text
///   55/45 -> 0.551      65/35 -> 0.672
///   60/40 -> 0.575      70/30 -> 0.677
/// ```
///
/// Sharpening the mix leaves *fewer* docs sitting in a format that some other
/// intent dominates, so the baseline recovers. 55/45 maximises the confusable
/// fraction while still giving each pair of intents a shared dominant format.
const DOM_NUM: usize = 11;
const DOM_DEN: usize = 20;

/// Shared across all intents, so it cancels in any centroid comparison while
/// still consuming vector mass.
const GENERIC_VOCAB: &[&str] = &[
    "error",
    "failed",
    "broken",
    "issue",
    "unexpected",
    "regression",
    "wrong",
    "problem",
];

/// Deterministic hash-bucket bag-of-tokens embedding, L2-normalized — the same
/// hermetic stand-in `topics_quality.rs` uses. It shares Model2Vec's defining
/// failure mode: every token pulls the vector, so the most numerous tokens win.
fn hash_embed(text: &str) -> Vec<f32> {
    let mut v = vec![0.0f32; EMBED_DIM];
    for token in text.to_lowercase().split_whitespace() {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in token.bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        v[(h % EMBED_DIM as u64) as usize] += 1.0;
    }
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
    for x in &mut v {
        *x /= norm;
    }
    v
}

const INTENT_NAMES: [&str; 4] = [
    "billing-error",
    "data-loss",
    "auth-failure",
    "perf-regression",
];
const N_INTENTS: usize = INTENT_NAMES.len();

/// One token from an intent's large, sparse vocabulary.
///
/// Generated rather than hand-listed because the pool needs to be big (40 per
/// intent) for the sparsity that creates the confound. Each token is exclusive
/// to its intent, so the signal is *learnable* — just rarely *repeated*.
fn intent_word(intent: usize, k: usize) -> String {
    format!("{}_{k:02}", INTENT_NAMES[intent].replace('-', "_"))
}

/// How the ticket is *written*. This is the confound — it dominates the vector
/// and must be learned away.
const FORMAT_VOCAB: [&[&str]; 3] = [
    // 0: backport / release-engineering chatter
    &[
        "backport", "cherry", "pick", "force", "push", "rebase", "branch", "release", "merge",
        "conflict", "revert", "upstream",
    ],
    // 1: stack traces
    &[
        "traceback",
        "exception",
        "thread",
        "frame",
        "caused",
        "unwind",
        "line",
        "nullpointer",
        "stack",
        "panic",
        "abort",
        "segfault",
    ],
    // 2: issue templates
    &[
        "steps",
        "reproduce",
        "expected",
        "actual",
        "behavior",
        "environment",
        "version",
        "checklist",
        "describe",
        "screenshot",
        "additional",
        "context",
    ],
];

/// Dominant and secondary format per intent (see [`DOM_NUM`] for the split).
///
/// Intents 0 and 1 SHARE a dominant format (backport). That shared mass is what
/// drags their centroids together. Intent 3's dominant format is `template`,
/// which is intent 0's secondary — so intent 0's template-formatted docs get
/// pulled toward intent 3.
const FORMAT_MIX: [(usize, usize); 4] = [
    (0, 2), // billing-error:    backport / template
    (0, 1), // data-loss:        backport / stacktrace
    (1, 2), // auth-failure:     stacktrace / template
    (2, 0), // perf-regression:  template / backport
];

/// Which format stratum doc `i` of `intent` lands in. Shared by the corpus
/// builder and the geometry guard so they can never disagree about the strata.
fn format_for(intent: usize, i: usize) -> usize {
    let (dominant, secondary) = FORMAT_MIX[intent];
    if i % DOM_DEN < DOM_NUM {
        dominant
    } else {
        secondary
    }
}

/// Build the crossed corpus. Returns (texts, per-row intent label sets).
fn build_corpus() -> (Vec<String>, Vec<Vec<usize>>) {
    let mut texts = Vec::with_capacity(DOCS_PER_INTENT * N_INTENTS);
    let mut truth = Vec::with_capacity(DOCS_PER_INTENT * N_INTENTS);

    for intent in 0..N_INTENTS {
        for i in 0..DOCS_PER_INTENT {
            let format = format_for(intent, i);
            let fmt_words = FORMAT_VOCAB[format];

            let mut parts: Vec<String> = Vec::with_capacity(
                FORMAT_TOKENS_PER_DOC + GENERIC_TOKENS_PER_DOC + INTENT_TOKENS_PER_DOC,
            );
            // Format: 10 of a 12-token pool. Strides are coprime with the pool
            // size so each doc draws 10 DISTINCT tokens -> dense overlap.
            for j in 0..FORMAT_TOKENS_PER_DOC {
                parts.push(fmt_words[(i * 5 + j * 7 + format) % fmt_words.len()].to_string());
            }
            // Generic: identical distribution for every intent -> no signal.
            for j in 0..GENERIC_TOKENS_PER_DOC {
                parts.push(GENERIC_VOCAB[(i * 3 + j * 5) % GENERIC_VOCAB.len()].to_string());
            }
            // Intent: the only diagnostic tokens. Strides coprime with the pool
            // so the whole vocabulary gets exercised -> sparse overlap.
            for j in 0..INTENT_TOKENS_PER_DOC {
                parts.push(intent_word(
                    intent,
                    (i * 7 + j * 5 + intent * 3) % INTENT_POOL,
                ));
            }
            texts.push(parts.join(" "));
            truth.push(vec![intent]);
        }
    }
    (texts, truth)
}

/// L2-normalized mean of the given rows' vectors.
fn centroid(vectors: &[Vec<f32>], rows: &[usize]) -> Vec<f32> {
    let mut sum = vec![0.0f32; EMBED_DIM];
    for &r in rows {
        for (s, v) in sum.iter_mut().zip(vectors[r].iter()) {
            *s += v;
        }
    }
    let norm = sum.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
    for x in &mut sum {
        *x /= norm;
    }
    sum
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .fold(0.0f32, |acc, (x, y)| x.mul_add(*y, acc))
}

#[test]
fn trained_head_recovers_intent_where_nearest_centroid_follows_format() {
    let (texts, truth) = build_corpus();
    let vectors: Vec<Vec<f32>> = texts.iter().map(|t| hash_embed(t)).collect();

    let outcome = fit_multilabel_linear(&vectors, &truth, N_INTENTS, EMBED_DIM)
        .expect("corpus is large and balanced enough to train");

    // Same split the head used — fairness is structural, not re-derived here.
    let (val_idx, train_idx) = train_val_split(vectors.len());

    // Head, scored on validation in its own retained label space.
    let head_predicted: Vec<Vec<usize>> = val_idx
        .iter()
        .map(|&r| {
            outcome
                .head
                .predict(&vectors[r])
                .into_iter()
                .map(|(i, _)| i)
                .collect()
        })
        .collect();
    let head_gold: Vec<Vec<usize>> = val_idx
        .iter()
        .map(|&r| {
            outcome
                .retained_labels
                .iter()
                .enumerate()
                .filter(|(_, &label)| truth[r].contains(&label))
                .map(|(i, _)| i)
                .collect()
        })
        .collect();
    let head_f1 = macro_f1(&head_predicted, &head_gold, outcome.head.len());

    // The SAME baseline `topics eval-classifier` runs in production.
    let base_f1 = f64::from(
        fit_centroid_baseline(&vectors, &truth, &outcome.retained_labels, EMBED_DIM)
            .expect("baseline fits on the same split the head used"),
    );

    let retained_names: Vec<&str> = outcome
        .retained_labels
        .iter()
        .map(|&i| INTENT_NAMES[i])
        .collect();
    eprintln!(
        "macro-F1  centroid-baseline={base_f1:.3}  trained-head={head_f1:.3}  \
         (labels: {retained_names:?}, train={} val={})",
        train_idx.len(),
        val_idx.len()
    );

    // VACUITY GUARD. If the baseline does fine, the corpus failed to reproduce
    // the format confound and the rest of this test proves nothing. Do not
    // "fix" a failure here by loosening it — fix the corpus.
    assert!(
        base_f1 < 0.60,
        "baseline must fall for the format confound, else this test proves nothing (got {base_f1:.3})"
    );
    assert!(
        head_f1 > base_f1 + 0.15,
        "trained head must beat nearest-centroid by a real margin ({head_f1:.3} vs {base_f1:.3})"
    );
    assert!(
        head_f1 > 0.75,
        "trained head must recover the planted intent (got {head_f1:.3})"
    );
}

/// Guards the corpus itself: format really must dominate the geometry, or the
/// headline test is passing for the wrong reason.
#[test]
fn corpus_geometry_is_format_dominated() {
    let (texts, truth) = build_corpus();
    let vectors: Vec<Vec<f32>> = texts.iter().map(|t| hash_embed(t)).collect();

    // Recover each doc's format stratum via the same function build_corpus used.
    let mut format_of = Vec::with_capacity(texts.len());
    for intent in 0..N_INTENTS {
        for i in 0..DOCS_PER_INTENT {
            format_of.push(format_for(intent, i));
        }
    }

    let rows_by = |pred: &dyn Fn(usize) -> bool| -> Vec<usize> {
        (0..vectors.len()).filter(|&r| pred(r)).collect()
    };

    // Mean within-group cosine for grouping by intent vs. grouping by format.
    let mut intent_cohesion = 0.0f64;
    for intent in 0..N_INTENTS {
        let rows = rows_by(&|r| truth[r].contains(&intent));
        let c = centroid(&vectors, &rows);
        intent_cohesion += rows
            .iter()
            .map(|&r| f64::from(cosine(&vectors[r], &c)))
            .sum::<f64>()
            / rows.len() as f64;
    }
    intent_cohesion /= N_INTENTS as f64;

    let mut format_cohesion = 0.0f64;
    for format in 0..FORMAT_VOCAB.len() {
        let rows = rows_by(&|r| format_of[r] == format);
        let c = centroid(&vectors, &rows);
        format_cohesion += rows
            .iter()
            .map(|&r| f64::from(cosine(&vectors[r], &c)))
            .sum::<f64>()
            / rows.len() as f64;
    }
    format_cohesion /= FORMAT_VOCAB.len() as f64;

    eprintln!("cohesion  by-intent={intent_cohesion:.3}  by-format={format_cohesion:.3}");
    assert!(
        format_cohesion > intent_cohesion,
        "the corpus must be format-dominated for the thesis test to mean anything \
         (format={format_cohesion:.3} intent={intent_cohesion:.3})"
    );
}

/// Two intents that share a dominant format must have near-collinear centroids.
/// This is the specific geometric fact that defeats nearest-centroid.
#[test]
fn intents_sharing_a_format_have_collinear_centroids() {
    let (texts, truth) = build_corpus();
    let vectors: Vec<Vec<f32>> = texts.iter().map(|t| hash_embed(t)).collect();

    let centroid_for = |intent: usize| -> Vec<f32> {
        let rows: Vec<usize> = (0..vectors.len())
            .filter(|&r| truth[r].contains(&intent))
            .collect();
        centroid(&vectors, &rows)
    };

    // Intents 0 and 1 both sit 60% on the backport format.
    let shared = cosine(&centroid_for(0), &centroid_for(1));
    // Intents 0 and 2 share no dominant format.
    let unshared = cosine(&centroid_for(0), &centroid_for(2));

    eprintln!("centroid cosine  shared-format={shared:.3}  different-format={unshared:.3}");
    assert!(
        shared > unshared,
        "intents sharing a dominant format must have closer centroids than those that don't \
         ({shared:.3} vs {unshared:.3}) — this is the confound the head has to defeat"
    );
}
