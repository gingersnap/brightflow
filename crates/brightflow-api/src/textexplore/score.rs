//! Words-widget scoring over doc frequencies.
//!
//! Distinctiveness is count-based weighted log-odds with an informative
//! Dirichlet prior (Monroe et al. 2008), comparing the filtered subset
//! against the rest of the corpus. Doc-frequency counting (a term counts
//! once per row) keeps one spammy row from dominating and matches the
//! "rows mentioning X" mental model of click-to-filter.
#![allow(clippy::cast_precision_loss)]

/// Prior concentration. Higher = more shrinkage toward corpus rates, which
/// suppresses low-count flukes.
const A0: f64 = 500.0;

/// Weighted log-odds z-score per term: subset vs rest-of-corpus.
///
/// `subset_df[t]` = rows in the subset containing term t; `corpus_df[t]` =
/// rows in the whole corpus containing t. Terms absent from the subset get
/// z = 0 (never "distinctively absent" — this widget only surfaces presence).
pub(crate) fn log_odds_z(subset_df: &[u32], corpus_df: &[u32]) -> Vec<f64> {
    debug_assert_eq!(subset_df.len(), corpus_df.len());
    let n_subset: f64 = subset_df.iter().map(|&c| f64::from(c)).sum();
    let corpus_total: f64 = corpus_df.iter().map(|&c| f64::from(c)).sum();
    let n_bg = corpus_total - n_subset;
    if n_subset <= 0.0 || n_bg <= 0.0 || corpus_total <= 0.0 {
        return vec![0.0; subset_df.len()];
    }

    subset_df
        .iter()
        .zip(corpus_df.iter())
        .map(|(&y_i_raw, &y_all)| {
            if y_i_raw == 0 {
                return 0.0;
            }
            let y_i = f64::from(y_i_raw);
            let y_bg = (f64::from(y_all) - y_i).max(0.0);
            // Informative prior scaled to the term's corpus share.
            let a_w = A0 * f64::from(y_all) / corpus_total;
            let delta = ((y_i + a_w) / (n_subset + A0 - y_i - a_w)).ln()
                - ((y_bg + a_w) / (n_bg + A0 - y_bg - a_w)).ln();
            let variance = 1.0 / (y_i + a_w) + 1.0 / (y_bg + a_w);
            if variance <= 0.0 {
                return 0.0;
            }
            delta / variance.sqrt()
        })
        .collect()
}

/// Term id + subset count + score, sorted, as raw building blocks for the
/// response (the handler maps ids to strings).
pub(crate) struct RankedTerm {
    pub term_id: u32,
    pub count: u32,
    pub score: f64,
}

/// Terms surfaced per widget section.
pub(crate) const TOP_TERMS: usize = 30;
/// Significance floor for the distinctive list (two-sided 95%).
const MIN_Z: f64 = 1.96;
/// Minimum subset rows containing a term before it can rank as distinctive.
const MIN_DISTINCTIVE_COUNT: u32 = 3;

/// Top terms by subset doc frequency; score = share of subset rows.
pub(crate) fn common_terms(subset_df: &[u32], subset_rows: usize) -> Vec<RankedTerm> {
    if subset_rows == 0 {
        return Vec::new();
    }
    let mut ranked: Vec<RankedTerm> = subset_df
        .iter()
        .enumerate()
        .filter(|&(_, &c)| c > 0)
        .map(|(id, &c)| RankedTerm {
            term_id: u32::try_from(id).unwrap_or(u32::MAX),
            count: c,
            score: f64::from(c) / subset_rows as f64,
        })
        .collect();
    ranked.sort_by(|a, b| b.count.cmp(&a.count).then(a.term_id.cmp(&b.term_id)));
    ranked.truncate(TOP_TERMS);
    ranked
}

/// Top terms by log-odds z-score, thresholded on significance and support.
pub(crate) fn distinctive_terms(subset_df: &[u32], corpus_df: &[u32]) -> Vec<RankedTerm> {
    let z = log_odds_z(subset_df, corpus_df);
    let mut ranked: Vec<RankedTerm> = z
        .iter()
        .enumerate()
        .filter(|&(id, &score)| {
            score > MIN_Z && subset_df.get(id).copied().unwrap_or(0) >= MIN_DISTINCTIVE_COUNT
        })
        .map(|(id, &score)| RankedTerm {
            term_id: u32::try_from(id).unwrap_or(u32::MAX),
            count: subset_df.get(id).copied().unwrap_or(0),
            score,
        })
        .collect();
    ranked.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.term_id.cmp(&b.term_id))
    });
    ranked.truncate(TOP_TERMS);
    ranked
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::cast_possible_truncation)]
mod tests {
    use super::*;

    #[test]
    fn uniform_subset_nothing_significant() {
        // Subset is a uniform 50% sample of every term: no term should
        // stand out as distinctive.
        let corpus: Vec<u32> = vec![40, 60, 100, 80];
        let subset: Vec<u32> = corpus.iter().map(|c| c / 2).collect();
        let ranked = distinctive_terms(&subset, &corpus);
        assert!(
            ranked.is_empty(),
            "uniform sample produced distinctive terms: {:?}",
            ranked
                .iter()
                .map(|r| (r.term_id, r.score))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn planted_term_ranks_top() {
        // Term 2 is massively enriched in the subset vs the corpus.
        let corpus: Vec<u32> = vec![500, 500, 60, 500, 500];
        let mut subset: Vec<u32> = vec![50, 50, 50, 50, 50];
        subset[2] = 50; // 50 of 60 corpus occurrences fall in the subset
        let ranked = distinctive_terms(&subset, &corpus);
        assert!(!ranked.is_empty());
        assert_eq!(ranked[0].term_id, 2);
    }

    #[test]
    fn zero_subset_count_never_distinctive() {
        let corpus: Vec<u32> = vec![100, 100];
        let subset: Vec<u32> = vec![0, 90];
        let z = log_odds_z(&subset, &corpus);
        assert!(z[0].abs() < f64::EPSILON);
    }

    #[test]
    fn common_is_ordered_by_count_with_share_score() {
        let subset: Vec<u32> = vec![5, 20, 10];
        let ranked = common_terms(&subset, 40);
        let ids: Vec<u32> = ranked.iter().map(|r| r.term_id).collect();
        assert_eq!(ids, vec![1, 2, 0]);
        assert!((ranked[0].score - 0.5).abs() < 1e-9);
    }

    #[test]
    fn empty_inputs_are_safe() {
        assert!(common_terms(&[], 0).is_empty());
        assert!(distinctive_terms(&[], &[]).is_empty());
        assert!(log_odds_z(&[0, 0], &[0, 0])
            .iter()
            .all(|z| z.abs() < f64::EPSILON));
    }
}
