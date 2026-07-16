//! Supervised linear classification head over dense embeddings.
//!
//! # Why this exists
//!
//! Static token-averaged embeddings (Model2Vec `potion-base-32M`) have a
//! dominant axis of variance that tracks **surface format and vocabulary**, not
//! problem intent. Any *unsupervised* consumer of that geometry — k-means,
//! nearest-centroid, retrieval — follows that dominant axis and yields
//! format clusters ("backport scripts", "stack traces") rather than intent
//! ("authentication failure", "data loss on sync").
//!
//! Supervision is the mode that defeats it: labels let a trained head
//! *downweight* format-correlated dimensions and *upweight* content ones.
//! Nearest-centroid cannot do this — it weights every dimension equally by
//! construction, so it inherits the format bias no matter how good the labels
//! are. This is also the one task family where static embeddings reach
//! near-LLM quality (MTEB Classification).
//!
//! # Division of labour
//!
//! *Training* is cold-path (rare, correctness matters) → `linfa-logistic`'s
//! L-BFGS. *Inference* is hot-path (every row, every sync) → the five
//! dependency-free lines in [`MultiLabelLinear::scores`].
//!
//! The artifact stores **plain `Vec<Vec<f32>>` weights**, never linfa's
//! `FittedLogisticRegression`. Storing their type would make our artifact
//! version hostage to their private field layout, silently orphaning every fit
//! on a patch bump — and would weld us to that optimizer forever.

use linfa::prelude::Fit;
use linfa::Dataset;
use linfa_logistic::LogisticRegression;
use ndarray::{Array1, Array2};
use serde::{Deserialize, Serialize};

use super::classification_metrics::binary_f1;
use super::rng::SplitMix64;

/// Fraction of labelled rows held out to tune thresholds and report F1.
pub const VAL_FRACTION: f32 = 0.15;
/// A label needs this many positive rows to get a head; rarer labels are dropped.
pub const MIN_LABEL_SUPPORT: usize = 20;
/// Below this many labelled rows, training is refused outright.
pub const MIN_TRAIN_ROWS: usize = 50;
/// L2 regularisation strength.
pub const L2_ALPHA: f32 = 1e-4;
/// L-BFGS iteration cap.
pub const MAX_ITERATIONS: u64 = 200;
/// L-BFGS convergence tolerance.
pub const GRADIENT_TOLERANCE: f32 = 1e-6;
/// Fixed seed — training must be reproducible across builds and runs.
pub const CLASSIFIER_SEED: u64 = 0x00c1_a551_f1e5_0000;

/// Candidate decision thresholds swept per label on the validation split.
pub const THRESHOLD_GRID: &[f32] = &[
    0.05, 0.10, 0.15, 0.20, 0.25, 0.30, 0.35, 0.40, 0.45, 0.50, 0.55, 0.60, 0.65, 0.70, 0.75, 0.80,
    0.85, 0.90, 0.95,
];

/// Threshold used for a label with no validation positives to tune against.
const DEFAULT_THRESHOLD: f32 = 0.5;

/// C independent one-vs-rest logistic heads over a shared dense input.
///
/// Scores are `sigmoid(w·x + b)`. They are **uncalibrated** — a monotone
/// ranking signal, not a probability. Do not read 0.7 as "70% likely".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiLabelLinear {
    /// C × dim weight matrix.
    pub weights: Vec<Vec<f32>>,
    /// C biases.
    pub biases: Vec<f32>,
    /// C per-label decision thresholds, tuned on validation.
    pub thresholds: Vec<f32>,
}

impl MultiLabelLinear {
    /// Sigmoid score per label. Length always equals `weights.len()`; a
    /// dimension mismatch yields 0.0 for that label rather than panicking.
    pub fn scores(&self, x: &[f32]) -> Vec<f32> {
        self.weights
            .iter()
            .zip(self.biases.iter())
            .map(|(w, &b)| {
                if w.len() != x.len() {
                    return 0.0;
                }
                let z = w
                    .iter()
                    .zip(x.iter())
                    .fold(b, |acc, (wi, xi)| wi.mul_add(*xi, acc));
                sigmoid(z)
            })
            .collect()
    }

    /// Highest-scoring label and its score, ignoring thresholds.
    ///
    /// This backs the backward-compatible `predicted_label` column, which has
    /// always been an unconditional argmax.
    pub fn argmax(&self, x: &[f32]) -> Option<(usize, f32)> {
        self.scores(x)
            .into_iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.total_cmp(b))
    }

    /// Every label scoring at or above its own threshold, best-first.
    ///
    /// May be empty — that is a real answer ("no intent confidently applies"),
    /// not a failure.
    pub fn predict(&self, x: &[f32]) -> Vec<(usize, f32)> {
        let mut hits: Vec<(usize, f32)> = self
            .scores(x)
            .into_iter()
            .enumerate()
            .filter(|(i, s)| {
                *s >= self
                    .thresholds
                    .get(*i)
                    .copied()
                    .unwrap_or(DEFAULT_THRESHOLD)
            })
            .collect();
        hits.sort_by(|(_, a), (_, b)| b.total_cmp(a));
        hits
    }

    /// Number of labels this head scores.
    pub fn len(&self) -> usize {
        self.weights.len()
    }

    pub fn is_empty(&self) -> bool {
        self.weights.is_empty()
    }
}

/// Numerically stable logistic sigmoid.
fn sigmoid(z: f32) -> f32 {
    if z >= 0.0 {
        1.0 / (1.0 + (-z).exp())
    } else {
        let e = z.exp();
        e / (1.0 + e)
    }
}

/// Deterministic train/validation split over `n` rows, as `(val, train)` index
/// lists.
///
/// Public on purpose: any baseline being compared against the trained head must
/// use the *same* split, or the comparison is rigged. Exposing the real function
/// makes that fairness structural rather than a copy-paste that silently drifts.
///
/// Shuffling before splitting matters: labelled corpora usually arrive grouped
/// by cluster or import order, so an unshuffled tail split would hand back a
/// validation set covering only the last few labels and a meaningless F1.
pub fn train_val_split(n: usize) -> (Vec<usize>, Vec<usize>) {
    if n < 2 {
        return (Vec::new(), (0..n).collect());
    }
    let mut order: Vec<usize> = (0..n).collect();
    SplitMix64::new(CLASSIFIER_SEED).shuffle(&mut order);
    let n_val = ((n as f32) * VAL_FRACTION).round() as usize;
    let n_val = n_val.clamp(1, n - 1);
    let (val, train) = order.split_at(n_val);
    (val.to_vec(), train.to_vec())
}

/// Smallest number of labelled rows for which training can actually start.
///
/// NOT [`MIN_TRAIN_ROWS`]. That constant gates the *training split*, which is
/// only `1 - VAL_FRACTION` of the labelled rows — so a corpus with exactly
/// `MIN_TRAIN_ROWS` labelled rows still trains nothing. Reporting the raw
/// constant to a curator tells them they are done when they are not, and the
/// only symptom is `predicted_labels` silently staying null.
///
/// Derived by asking [`train_val_split`] rather than hardcoding a number, so it
/// tracks the constants and the split's rounding automatically.
pub fn min_labelled_rows_to_train() -> usize {
    // Bounded: the split keeps ~85% of rows, so the answer is near
    // MIN_TRAIN_ROWS / 0.85. The cap is a backstop, not an expected path.
    for n in MIN_TRAIN_ROWS..=(MIN_TRAIN_ROWS * 4) {
        let (_, train) = train_val_split(n);
        if train.len() >= MIN_TRAIN_ROWS {
            return n;
        }
    }
    MIN_TRAIN_ROWS * 4
}

/// Labelled examples a single category needs before the head will keep it.
///
/// Same trap as [`min_labelled_rows_to_train`]: [`MIN_LABEL_SUPPORT`] is counted
/// against the *training split*, so a category needs proportionally more
/// examples overall to survive. Approximate — which rows land in validation
/// depends on the shuffle — but it is the right number to show a curator, and
/// erring high is the safe direction.
pub fn min_examples_per_label() -> usize {
    let train_fraction = 1.0 - f64::from(VAL_FRACTION);
    if train_fraction <= 0.0 {
        return MIN_LABEL_SUPPORT;
    }
    (f64::from(u32::try_from(MIN_LABEL_SUPPORT).unwrap_or(u32::MAX)) / train_fraction).ceil()
        as usize
}

/// Result of a training run.
#[derive(Debug, Clone)]
pub struct TrainOutcome {
    pub head: MultiLabelLinear,
    /// Macro-F1 on the held-out split — the claim, checkable in production.
    pub val_macro_f1: f32,
    /// Rows used for training (excludes the validation split).
    pub train_rows: usize,
    /// Indices into the caller's label space that survived `MIN_LABEL_SUPPORT`,
    /// parallel to `head.weights`. The caller needs this to map head outputs
    /// back to label names.
    pub retained_labels: Vec<usize>,
    /// Positive-row count per retained label.
    pub support: Vec<usize>,
}

/// Fit C one-vs-rest binary logistic models and extract plain weights.
///
/// `features` are per-row dense vectors; `targets` are per-row label-index sets
/// (multi-label — a row may carry several intents, or none). `n_labels` is the
/// caller's full label-space size; labels with fewer than
/// [`MIN_LABEL_SUPPORT`] positives are dropped and reported via
/// [`TrainOutcome::retained_labels`].
///
/// Deterministic: fixed seed, fixed row order.
///
/// Returns `None` when the signal is too thin to train on — too few rows, no
/// features, or no label clearing support. That is graceful degradation, not an
/// error: the caller keeps its previous behaviour.
pub fn fit_multilabel_linear(
    features: &[Vec<f32>],
    targets: &[Vec<usize>],
    n_labels: usize,
    dim: usize,
) -> Option<TrainOutcome> {
    if features.len() != targets.len()
        || features.len() < MIN_TRAIN_ROWS
        || dim == 0
        || n_labels == 0
        || features.iter().any(|f| f.len() != dim)
    {
        return None;
    }

    let (val_idx, train_idx) = train_val_split(features.len());
    let (val_idx, train_idx) = (val_idx.as_slice(), train_idx.as_slice());
    if train_idx.len() < MIN_TRAIN_ROWS.min(features.len()) {
        return None;
    }

    // Keep only labels with enough positives in the TRAINING split. Support in
    // the full set is not enough: a label whose positives all land in
    // validation cannot be learned.
    let retained: Vec<usize> = (0..n_labels)
        .filter(|label| {
            train_idx
                .iter()
                .filter(|&&r| targets.get(r).is_some_and(|t| t.contains(label)))
                .count()
                >= MIN_LABEL_SUPPORT
        })
        .collect();
    if retained.is_empty() {
        return None;
    }

    let x_train = stack_rows(features, train_idx, dim)?;
    let mut weights = Vec::with_capacity(retained.len());
    let mut biases = Vec::with_capacity(retained.len());
    let mut thresholds = Vec::with_capacity(retained.len());
    let mut support = Vec::with_capacity(retained.len());
    let mut kept: Vec<usize> = Vec::with_capacity(retained.len());

    for &label in &retained {
        let y_train: Vec<usize> = train_idx
            .iter()
            .map(|&r| usize::from(targets.get(r).is_some_and(|t| t.contains(&label))))
            .collect();
        let n_pos = y_train.iter().filter(|&&v| v == 1).count();

        let Some((w, b)) = fit_binary(&x_train, &y_train) else {
            continue;
        };

        // Tune this label's threshold on validation.
        let val_scores: Vec<f32> = val_idx
            .iter()
            .filter_map(|&r| features.get(r))
            .map(|x| {
                sigmoid(
                    w.iter()
                        .zip(x.iter())
                        .fold(b, |acc, (wi, xi)| wi.mul_add(*xi, acc)),
                )
            })
            .collect();
        let val_pos: Vec<bool> = val_idx
            .iter()
            .map(|&r| targets.get(r).is_some_and(|t| t.contains(&label)))
            .collect();

        thresholds.push(best_threshold(&val_scores, &val_pos));
        weights.push(w);
        biases.push(b);
        support.push(n_pos);
        kept.push(label);
    }

    if weights.is_empty() {
        return None;
    }

    let head = MultiLabelLinear {
        weights,
        biases,
        thresholds,
    };

    // Score the head on validation, in the head's own (retained) label space.
    let predicted: Vec<Vec<usize>> = val_idx
        .iter()
        .filter_map(|&r| features.get(r))
        .map(|x| head.predict(x).into_iter().map(|(i, _)| i).collect())
        .collect();
    let truth: Vec<Vec<usize>> = val_idx
        .iter()
        .map(|&r| {
            kept.iter()
                .enumerate()
                .filter(|(_, &label)| targets.get(r).is_some_and(|t| t.contains(&label)))
                .map(|(i, _)| i)
                .collect()
        })
        .collect();
    let val_macro_f1 =
        super::classification_metrics::macro_f1(&predicted, &truth, head.len()) as f32;

    Some(TrainOutcome {
        head,
        val_macro_f1,
        train_rows: train_idx.len(),
        retained_labels: kept,
        support,
    })
}

/// Held-out macro-F1 of a **fair** nearest-centroid baseline.
///
/// "Fair" is the whole point, and it is what makes a head-vs-baseline number
/// mean anything. The baseline gets:
/// * the same train/val split ([`train_val_split`]),
/// * the same retained label set (pass the head's `retained_labels` —
///   [`MIN_LABEL_SUPPORT`] drops rare labels from the head but not from
///   centroids, and letting the baseline keep them would compare two different
///   problems),
/// * per-label thresholds tuned by the identical protocol ([`best_threshold`]
///   over [`THRESHOLD_GRID`] on the same validation rows).
///
/// Centroids are built from **training rows only** — the head never sees
/// validation either. The claim worth making is that a fair baseline still
/// loses; a rigged one proves nothing.
///
/// Returns `None` when the split is degenerate.
pub fn fit_centroid_baseline(
    features: &[Vec<f32>],
    targets: &[Vec<usize>],
    retained_labels: &[usize],
    dim: usize,
) -> Option<f32> {
    if features.len() != targets.len() || features.len() < 2 || retained_labels.is_empty() {
        return None;
    }
    let (val_idx, train_idx) = train_val_split(features.len());
    if val_idx.is_empty() || train_idx.is_empty() {
        return None;
    }

    // One L2-normalized centroid per retained label, from training rows.
    let centroids: Vec<Vec<f32>> = retained_labels
        .iter()
        .map(|&label| {
            let mut sum = vec![0.0f32; dim];
            let mut n = 0u32;
            for &r in &train_idx {
                if targets.get(r).is_some_and(|t| t.contains(&label)) {
                    if let Some(f) = features.get(r).filter(|f| f.len() == dim) {
                        for (s, v) in sum.iter_mut().zip(f.iter()) {
                            *s += v;
                        }
                        n += 1;
                    }
                }
            }
            if n > 0 {
                let norm = sum.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
                for x in &mut sum {
                    *x /= norm;
                }
            }
            sum
        })
        .collect();

    let cos = |a: &[f32], b: &[f32]| -> f32 {
        if a.len() != b.len() {
            return 0.0;
        }
        a.iter()
            .zip(b.iter())
            .fold(0.0f32, |acc, (x, y)| x.mul_add(*y, acc))
    };

    let thresholds: Vec<f32> = retained_labels
        .iter()
        .enumerate()
        .map(|(i, &label)| {
            let scores: Vec<f32> = val_idx
                .iter()
                .filter_map(|&r| features.get(r))
                .map(|f| centroids.get(i).map_or(0.0, |c| cos(f, c)))
                .collect();
            let positives: Vec<bool> = val_idx
                .iter()
                .map(|&r| targets.get(r).is_some_and(|t| t.contains(&label)))
                .collect();
            best_threshold(&scores, &positives)
        })
        .collect();

    let predicted: Vec<Vec<usize>> = val_idx
        .iter()
        .filter_map(|&r| features.get(r))
        .map(|f| {
            (0..retained_labels.len())
                .filter(|&i| {
                    centroids.get(i).is_some_and(|c| {
                        cos(f, c) >= thresholds.get(i).copied().unwrap_or(DEFAULT_THRESHOLD)
                    })
                })
                .collect()
        })
        .collect();
    let gold: Vec<Vec<usize>> = val_idx
        .iter()
        .map(|&r| {
            retained_labels
                .iter()
                .enumerate()
                .filter(|(_, &label)| targets.get(r).is_some_and(|t| t.contains(&label)))
                .map(|(i, _)| i)
                .collect()
        })
        .collect();

    Some(super::classification_metrics::macro_f1(&predicted, &gold, retained_labels.len()) as f32)
}

/// Pick the threshold maximising binary F1 on validation.
///
/// Per-label thresholds are the highest-leverage part of this module: for
/// imbalanced multi-label data they outweigh any optimizer choice, and they
/// handle imbalance without sample weights. With no validation positives there
/// is nothing to tune against, so fall back to 0.5 rather than fitting noise.
///
/// Ties are broken toward the threshold **closest to 0.5**. On an easy or small
/// validation split many thresholds tie at the same F1, and the tied region is
/// a plateau whose edges are artefacts of where the split's scores happen to
/// fall. Taking an edge (as a plain `max_by` would — it keeps the *last*
/// maximum, i.e. 0.95) bakes that artefact into production. Staying near the
/// natural decision boundary unless the data actually argues for moving is the
/// conservative read of the same evidence.
///
/// Public for the same reason as [`train_val_split`]: a baseline compared
/// against the head must be tuned by the identical protocol to be fair.
pub fn best_threshold(scores: &[f32], positives: &[bool]) -> f32 {
    if !positives.iter().any(|&p| p) {
        return DEFAULT_THRESHOLD;
    }
    THRESHOLD_GRID
        .iter()
        .map(|&t| (t, binary_f1(scores, positives, t)))
        .fold(None::<(f32, f64)>, |best, (t, f1)| match best {
            None => Some((t, f1)),
            Some((bt, bf1)) => {
                let better = f1 > bf1;
                let tied_and_closer = (f1 - bf1).abs() < f64::EPSILON
                    && (t - DEFAULT_THRESHOLD).abs() < (bt - DEFAULT_THRESHOLD).abs();
                if better || tied_and_closer {
                    Some((t, f1))
                } else {
                    Some((bt, bf1))
                }
            },
        })
        .map_or(DEFAULT_THRESHOLD, |(t, _)| t)
}

/// Stack selected rows into an ndarray matrix.
fn stack_rows(features: &[Vec<f32>], idx: &[usize], dim: usize) -> Option<Array2<f32>> {
    let mut flat = Vec::with_capacity(idx.len() * dim);
    for &r in idx {
        flat.extend_from_slice(features.get(r)?);
    }
    Array2::from_shape_vec((idx.len(), dim), flat).ok()
}

/// Fit one binary logistic model and return plain `(weights, bias)` oriented so
/// that `sigmoid(w·x + b)` is the score for **class 1**.
///
/// The orientation step is not cosmetic. linfa fits with a ±1 internal encoding
/// and binds `pos` to whichever class it decides — empirically the *majority*
/// class, so for a rare label `pos` is class 0 and the raw `params()` are
/// sign-inverted relative to what we want. Extracting them naively produces a
/// head that is confidently, silently backwards: it compiles, it converges, and
/// every score is upside down. We therefore read `labels().pos.class` and flip.
///
/// Returns `None` when linfa refuses the fit — most commonly
/// "fewer than two classes", i.e. this label is all-positive or all-negative in
/// the training split and there is nothing to separate.
fn fit_binary(x: &Array2<f32>, y: &[usize]) -> Option<(Vec<f32>, f32)> {
    let targets: Array1<usize> = Array1::from_vec(y.to_vec());
    let dataset = Dataset::new(x.clone(), targets);
    if dataset.records().nrows() == 0 {
        return None;
    }

    let model = LogisticRegression::default()
        .alpha(L2_ALPHA)
        .max_iterations(MAX_ITERATIONS)
        .gradient_tolerance(GRADIENT_TOLERANCE)
        .fit(&dataset)
        .ok()?;

    let sign = if model.labels().pos.class == 1 {
        1.0f32
    } else {
        -1.0f32
    };
    let weights: Vec<f32> = model.params().iter().map(|w| w * sign).collect();
    let bias = model.intercept() * sign;
    if weights.iter().any(|w| !w.is_finite()) || !bias.is_finite() {
        return None;
    }
    Some((weights, bias))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::indexing_slicing,
    clippy::cast_precision_loss
)]
mod tests {
    use super::super::classification_metrics::binary_f1;
    use super::{
        fit_multilabel_linear, sigmoid, MultiLabelLinear, MIN_LABEL_SUPPORT, MIN_TRAIN_ROWS,
    };

    /// Two linearly separable label directions in a small dense space.
    fn toy_corpus(n: usize) -> (Vec<Vec<f32>>, Vec<Vec<usize>>) {
        let mut features = Vec::with_capacity(n);
        let mut targets = Vec::with_capacity(n);
        for i in 0..n {
            let label = i % 2;
            let jitter = (i % 7) as f32 * 0.01;
            let v = if label == 0 {
                vec![1.0 + jitter, 0.0, jitter]
            } else {
                vec![0.0, 1.0 + jitter, jitter]
            };
            features.push(v);
            targets.push(vec![label]);
        }
        (features, targets)
    }

    #[test]
    fn separable_toy_converges() {
        let (features, targets) = toy_corpus(200);
        let out = fit_multilabel_linear(&features, &targets, 2, 3).expect("should fit");
        assert_eq!(out.head.len(), 2);
        assert_eq!(out.retained_labels, vec![0, 1]);
        assert!(
            out.val_macro_f1 > 0.95,
            "separable data must be nearly perfect, got {}",
            out.val_macro_f1
        );

        // The head must actually point the right way — the regression guard for
        // the linfa pos-class orientation trap.
        let pred0 = out.head.argmax(&[1.0, 0.0, 0.0]).unwrap();
        let pred1 = out.head.argmax(&[0.0, 1.0, 0.0]).unwrap();
        assert_eq!(pred0.0, 0, "label-0 direction must score label 0 highest");
        assert_eq!(pred1.0, 1, "label-1 direction must score label 1 highest");
    }

    #[test]
    fn deterministic_across_runs() {
        let (features, targets) = toy_corpus(200);
        let a = fit_multilabel_linear(&features, &targets, 2, 3).expect("fit a");
        let b = fit_multilabel_linear(&features, &targets, 2, 3).expect("fit b");
        // Exact f32 equality: same seed, same order, same result. Anything less
        // means a refit silently changes production labels.
        assert_eq!(a.head.weights, b.head.weights);
        assert_eq!(a.head.biases, b.head.biases);
        assert_eq!(a.head.thresholds, b.head.thresholds);
        assert_eq!(a.val_macro_f1, b.val_macro_f1);
    }

    #[test]
    fn empty_input_returns_none() {
        assert!(fit_multilabel_linear(&[], &[], 2, 3).is_none());
    }

    /// The reported minimum must be the number that ACTUALLY trains — one row
    /// fewer must fail. This is what the curation UI promises a human.
    #[test]
    fn min_labelled_rows_to_train_is_the_real_threshold() {
        let n = super::min_labelled_rows_to_train();
        assert!(
            n > MIN_TRAIN_ROWS,
            "the holdout must push it above the raw constant"
        );

        let (_, train_at) = super::train_val_split(n);
        assert!(train_at.len() >= MIN_TRAIN_ROWS, "n must actually train");

        let (_, train_below) = super::train_val_split(n - 1);
        assert!(
            train_below.len() < MIN_TRAIN_ROWS,
            "n-1 must NOT train, else the reported minimum is too high"
        );
    }

    #[test]
    fn min_examples_per_label_accounts_for_the_holdout() {
        let m = super::min_examples_per_label();
        assert!(
            m > MIN_LABEL_SUPPORT,
            "must exceed the raw constant, since support is counted on the train split"
        );
        // A label with `m` examples should keep >= MIN_LABEL_SUPPORT after the
        // ~15% holdout.
        let kept = (m as f64) * (1.0 - f64::from(super::VAL_FRACTION));
        assert!(
            kept >= MIN_LABEL_SUPPORT as f64,
            "kept {kept} < {MIN_LABEL_SUPPORT}"
        );
    }

    #[test]
    fn too_few_rows_returns_none() {
        let (features, targets) = toy_corpus(MIN_TRAIN_ROWS - 1);
        assert!(fit_multilabel_linear(&features, &targets, 2, 3).is_none());
    }

    #[test]
    fn mismatched_lengths_return_none() {
        let (features, targets) = toy_corpus(100);
        assert!(fit_multilabel_linear(&features, &targets[..50], 2, 3).is_none());
    }

    #[test]
    fn ragged_features_return_none() {
        let (mut features, targets) = toy_corpus(100);
        features[3] = vec![1.0, 0.0];
        assert!(fit_multilabel_linear(&features, &targets, 2, 3).is_none());
    }

    #[test]
    fn rare_label_is_dropped_below_min_support() {
        let (mut features, mut targets) = toy_corpus(200);
        // Label 2 appears far fewer times than MIN_LABEL_SUPPORT.
        for _ in 0..(MIN_LABEL_SUPPORT / 4) {
            features.push(vec![0.0, 0.0, 1.0]);
            targets.push(vec![2]);
        }
        let out = fit_multilabel_linear(&features, &targets, 3, 3).expect("should fit");
        assert!(
            !out.retained_labels.contains(&2),
            "under-supported label must be dropped, got {:?}",
            out.retained_labels
        );
    }

    /// Rare label 1 that genuinely OVERLAPS the majority: feature x1 fires for
    /// every label-1 row, but also for ~110 label-0 rows, so
    /// `P(label 1 | x1=1) ~= 50/160 ~= 0.31`. No decision rule can separate them
    /// — the best any model can do is score ~0.31 on x1=1.
    ///
    /// That is exactly the case where a fixed 0.5 cut is worthless: it never
    /// fires, so recall is 0 and F1 is 0. Recovering the label REQUIRES dropping
    /// the threshold below the class-conditional rate. Separable toy data cannot
    /// test this — it ties across the whole grid and proves nothing.
    fn overlapping_rare_label_corpus() -> (Vec<Vec<f32>>, Vec<Vec<usize>>) {
        let n = 600;
        let mut features = Vec::with_capacity(n);
        let mut targets = Vec::with_capacity(n);
        for i in 0..n {
            let jitter = (i % 7) as f32 * 0.001;
            if i % 12 == 0 {
                // Rare positive: x1 fires.
                features.push(vec![1.0, 1.0, jitter]);
                targets.push(vec![0, 1]);
            } else if i % 5 == 1 {
                // Confuser: x1 also fires, but the row is NOT label 1.
                features.push(vec![1.0, 1.0, jitter]);
                targets.push(vec![0]);
            } else {
                features.push(vec![1.0, 0.0, jitter]);
                targets.push(vec![0]);
            }
        }
        (features, targets)
    }

    #[test]
    fn imbalanced_label_gets_threshold_below_half() {
        let (features, targets) = overlapping_rare_label_corpus();
        let out = fit_multilabel_linear(&features, &targets, 2, 3).expect("should fit");
        let i = out
            .retained_labels
            .iter()
            .position(|&l| l == 1)
            .expect("rare label has enough support to be retained");
        assert!(
            out.head.thresholds[i] < 0.5,
            "an overlapping rare label must drop its threshold below 0.5 to buy recall, got {}",
            out.head.thresholds[i]
        );
    }

    #[test]
    fn tuned_threshold_beats_a_fixed_half_on_the_rare_label() {
        // The payoff test: at 0.5 the rare label is never predicted at all.
        // This is what per-label thresholds actually buy, stated as a number.
        let (features, targets) = overlapping_rare_label_corpus();
        let out = fit_multilabel_linear(&features, &targets, 2, 3).expect("should fit");
        let i = out
            .retained_labels
            .iter()
            .position(|&l| l == 1)
            .expect("retained");

        let scores: Vec<f32> = features.iter().map(|x| out.head.scores(x)[i]).collect();
        let positives: Vec<bool> = targets.iter().map(|t| t.contains(&1)).collect();

        let f1_at_half = binary_f1(&scores, &positives, 0.5);
        let f1_tuned = binary_f1(&scores, &positives, out.head.thresholds[i]);

        assert_eq!(
            f1_at_half, 0.0,
            "a fixed 0.5 cut should never fire on this label"
        );
        assert!(
            f1_tuned > 0.4,
            "the tuned threshold should recover the label (F1 {f1_tuned} at t={})",
            out.head.thresholds[i]
        );
    }

    #[test]
    fn tied_thresholds_break_toward_the_natural_boundary() {
        // Perfectly separable scores tie at F1=1.0 across most of the grid. A
        // plain max_by would keep the LAST tie (0.95) and ship a brittle
        // edge-of-plateau threshold; we want the tie broken toward 0.5.
        let scores = [0.99f32, 0.99, 0.01, 0.01];
        let positives = [true, true, false, false];
        let t = super::best_threshold(&scores, &positives);
        assert_eq!(
            t, 0.5,
            "tied plateau must resolve to the closest-to-0.5 threshold, got {t}"
        );
    }

    #[test]
    fn no_validation_positives_falls_back_to_half() {
        let scores = [0.9f32, 0.2, 0.4];
        let positives = [false, false, false];
        assert_eq!(super::best_threshold(&scores, &positives), 0.5);
    }

    #[test]
    fn predict_respects_per_label_thresholds() {
        let head = MultiLabelLinear {
            weights: vec![vec![10.0, 0.0], vec![0.0, 10.0]],
            biases: vec![0.0, 0.0],
            thresholds: vec![0.5, 0.99],
        };
        // x favours label 0 strongly, label 1 weakly.
        let hits = head.predict(&[1.0, 0.05]);
        assert_eq!(hits.len(), 1, "only label 0 clears its threshold");
        assert_eq!(hits[0].0, 0);
    }

    #[test]
    fn predict_returns_best_first() {
        let head = MultiLabelLinear {
            weights: vec![vec![1.0, 0.0], vec![0.0, 1.0]],
            biases: vec![0.0, 0.0],
            thresholds: vec![0.0, 0.0],
        };
        let hits = head.predict(&[0.1, 5.0]);
        assert_eq!(hits[0].0, 1, "highest score must come first");
        assert!(hits[0].1 >= hits[1].1);
    }

    #[test]
    fn dimension_mismatch_scores_zero_not_panic() {
        let head = MultiLabelLinear {
            weights: vec![vec![1.0, 2.0, 3.0]],
            biases: vec![0.0],
            thresholds: vec![0.5],
        };
        assert_eq!(head.scores(&[1.0]), vec![0.0]);
        assert!(head.predict(&[1.0]).is_empty());
    }

    #[test]
    fn sigmoid_is_stable_at_extremes() {
        assert!(sigmoid(-1000.0).is_finite());
        assert!(sigmoid(1000.0).is_finite());
        assert!(sigmoid(-1000.0) >= 0.0);
        assert!(sigmoid(1000.0) <= 1.0);
        assert!((sigmoid(0.0) - 0.5).abs() < 1e-6);
    }
}
