//! Multi-label classification metrics.
//!
//! Sibling to [`super::cluster_metrics`], which scores *unsupervised* structure.
//! These score a *supervised* head against known truth.
//!
//! Every function takes predictions and truth as per-row label-index sets and
//! is total: degenerate input yields 0.0 rather than NaN, so a metric never
//! poisons a comparison with a silent NaN (`NaN > x` is always false, which
//! would make an assertion pass or fail for the wrong reason).

/// Per-label true positives, false positives and false negatives.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub tp: usize,
    pub fp: usize,
    pub fn_: usize,
}

impl Counts {
    /// Precision — 0.0 when nothing was predicted.
    pub fn precision(self) -> f64 {
        let denom = self.tp + self.fp;
        if denom == 0 {
            0.0
        } else {
            self.tp as f64 / denom as f64
        }
    }

    /// Recall — 0.0 when there was nothing to find.
    pub fn recall(self) -> f64 {
        let denom = self.tp + self.fn_;
        if denom == 0 {
            0.0
        } else {
            self.tp as f64 / denom as f64
        }
    }

    /// F1 — harmonic mean of precision and recall; 0.0 when both are 0.
    pub fn f1(self) -> f64 {
        let (p, r) = (self.precision(), self.recall());
        if p + r <= 0.0 {
            0.0
        } else {
            2.0 * p * r / (p + r)
        }
    }
}

/// Per-label confusion counts over a multi-label prediction set.
///
/// `predicted` and `truth` are parallel per-row label-index sets. Indices at or
/// above `n_labels` are ignored rather than panicking.
pub fn per_label_counts(
    predicted: &[Vec<usize>],
    truth: &[Vec<usize>],
    n_labels: usize,
) -> Vec<Counts> {
    let mut counts = vec![Counts::default(); n_labels];
    for (pred, gold) in predicted.iter().zip(truth.iter()) {
        for &p in pred {
            if p >= n_labels {
                continue;
            }
            if gold.contains(&p) {
                if let Some(c) = counts.get_mut(p) {
                    c.tp += 1;
                }
            } else if let Some(c) = counts.get_mut(p) {
                c.fp += 1;
            }
        }
        for &g in gold {
            if g >= n_labels {
                continue;
            }
            if !pred.contains(&g) {
                if let Some(c) = counts.get_mut(g) {
                    c.fn_ += 1;
                }
            }
        }
    }
    counts
}

/// Macro-averaged F1 — the unweighted mean of per-label F1.
///
/// Every label counts equally regardless of support, so a head that nails the
/// common labels and fails the rare ones is penalised. This is the metric to
/// assert on when the question is "did it learn the distinction?" — micro-F1
/// would let a dominant label mask the confusion under test.
pub fn macro_f1(predicted: &[Vec<usize>], truth: &[Vec<usize>], n_labels: usize) -> f64 {
    if n_labels == 0 {
        return 0.0;
    }
    let counts = per_label_counts(predicted, truth, n_labels);
    counts.iter().map(|c| c.f1()).sum::<f64>() / n_labels as f64
}

/// Micro-averaged F1 — pools every label's counts before computing F1, so it
/// weights by support.
pub fn micro_f1(predicted: &[Vec<usize>], truth: &[Vec<usize>], n_labels: usize) -> f64 {
    let counts = per_label_counts(predicted, truth, n_labels);
    let pooled = counts.iter().fold(Counts::default(), |mut acc, c| {
        acc.tp += c.tp;
        acc.fp += c.fp;
        acc.fn_ += c.fn_;
        acc
    });
    pooled.f1()
}

/// Macro-averaged precision.
/// Macro-averaged recall.
/// Binary F1 for one label index — the objective the threshold sweep maximises.
pub fn binary_f1(scores: &[f32], positives: &[bool], threshold: f32) -> f64 {
    let mut counts = Counts::default();
    for (&s, &is_pos) in scores.iter().zip(positives.iter()) {
        match (s >= threshold, is_pos) {
            (true, true) => counts.tp += 1,
            (true, false) => counts.fp += 1,
            (false, true) => counts.fn_ += 1,
            (false, false) => {},
        }
    }
    counts.f1()
}

#[cfg(test)]
#[expect(clippy::float_cmp, reason = "tests assert exact expected values")]
mod tests {
    use super::{binary_f1, macro_f1, micro_f1, per_label_counts, Counts};

    #[test]
    fn perfect_prediction_scores_one() {
        let truth = vec![vec![0], vec![1], vec![0, 1]];
        let pred = truth.clone();
        assert_eq!(macro_f1(&pred, &truth, 2), 1.0);
        assert_eq!(micro_f1(&pred, &truth, 2), 1.0);
    }

    #[test]
    fn empty_prediction_scores_zero_not_nan() {
        let truth = vec![vec![0], vec![1]];
        let pred = vec![vec![], vec![]];
        let m = macro_f1(&pred, &truth, 2);
        assert!(m.is_finite(), "must not be NaN");
        assert_eq!(m, 0.0);
    }

    #[test]
    fn zero_labels_scores_zero_not_nan() {
        assert_eq!(macro_f1(&[], &[], 0), 0.0);
        assert!(macro_f1(&[], &[], 0).is_finite());
    }

    #[test]
    fn counts_are_correct() {
        // row0: predicted 0, truth 0      -> tp[0]
        // row1: predicted 0, truth 1      -> fp[0], fn[1]
        let pred = vec![vec![0], vec![0]];
        let truth = vec![vec![0], vec![1]];
        let counts = per_label_counts(&pred, &truth, 2);
        assert_eq!(
            counts[0],
            Counts {
                tp: 1,
                fp: 1,
                fn_: 0
            }
        );
        assert_eq!(
            counts[1],
            Counts {
                tp: 0,
                fp: 0,
                fn_: 1
            }
        );
    }

    #[test]
    fn macro_penalises_rare_label_failure_more_than_micro() {
        // Label 0 has 9 rows and is perfect; label 1 has 1 row and is missed.
        let mut pred: Vec<Vec<usize>> = vec![vec![0]; 9];
        let mut truth: Vec<Vec<usize>> = vec![vec![0]; 9];
        pred.push(vec![]);
        truth.push(vec![1]);
        let macro_score = macro_f1(&pred, &truth, 2);
        let micro_score = micro_f1(&pred, &truth, 2);
        assert!(
            macro_score < micro_score,
            "macro ({macro_score}) must punish the rare-label miss harder than micro ({micro_score})"
        );
        assert_eq!(macro_score, 0.5, "one perfect label, one dead label");
    }

    #[test]
    fn out_of_range_indices_are_ignored_not_panicking() {
        let pred = vec![vec![0, 99]];
        let truth = vec![vec![0, 42]];
        let counts = per_label_counts(&pred, &truth, 1);
        assert_eq!(
            counts[0],
            Counts {
                tp: 1,
                fp: 0,
                fn_: 0
            }
        );
    }

    #[test]
    fn binary_f1_respects_threshold() {
        let scores = [0.9f32, 0.4, 0.8, 0.1];
        let positives = [true, false, true, false];
        assert_eq!(binary_f1(&scores, &positives, 0.5), 1.0);
        // A threshold above every score predicts nothing -> F1 0, not NaN.
        assert_eq!(binary_f1(&scores, &positives, 0.99), 0.0);
    }
}
