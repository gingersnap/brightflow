//! Drivers decomposition: which segments drove a measure's latest change?
//!
//! Pure core: takes per-slice two-period aggregates (sum, sum of squares,
//! count — enough for mean/variance without a second row pass) and ranks
//! slices by their contribution to the total period-over-period delta.
//!
//! Statistical honesty:
//! - Per-slice Welch test of current vs previous period values, Bonferroni-
//!   corrected across the slices tested for the same measure. (Per-measure
//!   correction only — see `docs/insights_limitations.md`.)
//! - Offsetting-drivers rule: when the total barely moved but segments moved
//!   hard in opposite directions (`|Δtotal| < 0.25 × Σ|Δslice|`), the story is
//!   the cancellation itself; contributions are then reported against gross
//!   movement instead of the near-zero net.

use std::collections::HashMap;

use crate::analysis::candidates::DimensionIndex;
use crate::analysis::null_models::{comparison_null, Significance};

/// Net delta below this share of gross slice movement → offsetting story.
const OFFSETTING_NET_SHARE: f64 = 0.25;
/// Diversity: keep at most this many drivers total…
const MAX_DRIVERS: usize = 8;
/// …and at most this many per dimension.
const MAX_PER_DIMENSION: usize = 3;

/// Two-period aggregates for one (dimension, value) slice.
#[derive(Debug, Clone)]
pub struct DriverSlice {
    pub dimension: String,
    pub value: String,
    pub prev_sum: f64,
    pub curr_sum: f64,
    pub prev_sum_squares: f64,
    pub curr_sum_squares: f64,
    pub prev_n: usize,
    pub curr_n: usize,
}

impl DriverSlice {
    pub fn delta(&self) -> f64 {
        self.curr_sum - self.prev_sum
    }
}

/// One ranked driver in a decomposition.
#[derive(Debug, Clone)]
pub struct RankedDriver {
    pub dimension: String,
    pub value: String,
    pub prev_value: f64,
    pub curr_value: f64,
    pub delta: f64,
    /// Share of the reference movement (net Δtotal, or gross when offsetting),
    /// signed, in percent.
    pub contribution_pct: f64,
    /// Slice's own period-over-period change, in percent (0 when prev = 0).
    pub change_percent: f64,
    /// Bonferroni-corrected Welch p of current vs previous slice values.
    pub p_value: f64,
    pub significance: Significance,
}

/// Decomposition of one measure's latest period-over-period delta.
#[derive(Debug, Clone)]
pub struct DeltaDecomposition {
    pub total_prev: f64,
    pub total_curr: f64,
    /// Σ|Δslice| over the tested slices of the best-covering dimension set.
    pub gross_movement: f64,
    /// True when segments moved hard in opposite directions and mostly
    /// cancelled — contributions are then relative to gross movement.
    pub offsetting: bool,
    /// Ranked, diversity-capped drivers (see `select_diverse`).
    pub drivers: Vec<RankedDriver>,
}

impl DeltaDecomposition {
    pub fn delta_total(&self) -> f64 {
        self.total_curr - self.total_prev
    }
}

/// Welch significance of one slice's current vs previous period values —
/// exposed for report roots that test the whole-table total the same way.
pub fn two_period_significance(slice: &DriverSlice) -> Significance {
    welch_from_moments(slice)
}

fn welch_from_moments(slice: &DriverSlice) -> Significance {
    if slice.prev_n < 2 || slice.curr_n < 2 {
        return Significance::none();
    }
    let mean_var = |sum: f64, sumsq: f64, n: usize| -> (f64, f64) {
        let nf = n as f64;
        let mean = sum / nf;
        // Sample variance from raw moments; clamp tiny negatives from
        // floating-point cancellation.
        let var = ((nf * mean).mul_add(-mean, sumsq) / (nf - 1.0)).max(0.0);
        (mean, var)
    };
    let (prev_mean, prev_var) = mean_var(slice.prev_sum, slice.prev_sum_squares, slice.prev_n);
    let (curr_mean, curr_var) = mean_var(slice.curr_sum, slice.curr_sum_squares, slice.curr_n);
    comparison_null(
        curr_mean,
        curr_var.sqrt(),
        slice.curr_n,
        prev_mean,
        prev_var.sqrt(),
        slice.prev_n,
    )
}

/// Rank slices by contribution to the total delta. `total_prev`/`total_curr`
/// are the whole-table sums for the same two periods.
pub fn rank_segment_drivers(
    slices: &[DriverSlice],
    total_prev: f64,
    total_curr: f64,
) -> DeltaDecomposition {
    let delta_total = total_curr - total_prev;
    let gross: f64 = slices.iter().map(|s| s.delta().abs()).sum();
    let offsetting = gross > 0.0 && delta_total.abs() < OFFSETTING_NET_SHARE * gross;
    let denom = if offsetting {
        gross
    } else {
        delta_total.abs().max(1e-12)
    };
    let n_tested = slices
        .iter()
        .filter(|s| s.delta().abs() > 0.0)
        .count()
        .max(1) as f64;

    let mut drivers: Vec<RankedDriver> = slices
        .iter()
        .filter(|s| s.delta().abs() > 0.0)
        .map(|s| {
            let mut sig = welch_from_moments(s);
            // Bonferroni across the slices tested for this measure.
            sig.p_value = (sig.p_value * n_tested).clamp(0.0, 1.0);
            sig.score = (1.0 - sig.p_value).clamp(0.0, sig.score);
            let change_percent = if s.prev_sum.abs() > 1e-12 {
                (s.curr_sum - s.prev_sum) / s.prev_sum.abs() * 100.0
            } else {
                0.0
            };
            RankedDriver {
                dimension: s.dimension.clone(),
                value: s.value.clone(),
                prev_value: s.prev_sum,
                curr_value: s.curr_sum,
                delta: s.delta(),
                contribution_pct: s.delta() / denom * 100.0,
                change_percent,
                p_value: sig.p_value,
                significance: sig,
            }
        })
        .collect();

    drivers.sort_by(|a, b| {
        b.delta
            .abs()
            .partial_cmp(&a.delta.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    DeltaDecomposition {
        total_prev,
        total_curr,
        gross_movement: gross,
        offsetting,
        drivers: select_diverse(drivers),
    }
}

/// Cap the ranked list at `MAX_DRIVERS`, no more than `MAX_PER_DIMENSION`
/// from any one dimension — one high-cardinality dimension must not crowd
/// out every other angle on the same delta.
fn select_diverse(ranked: Vec<RankedDriver>) -> Vec<RankedDriver> {
    let mut per_dim: HashMap<String, usize> = HashMap::new();
    let mut out = Vec::with_capacity(MAX_DRIVERS);
    for driver in ranked {
        if out.len() >= MAX_DRIVERS {
            break;
        }
        let seen = per_dim.entry(driver.dimension.clone()).or_insert(0);
        if *seen >= MAX_PER_DIMENSION {
            continue;
        }
        *seen += 1;
        out.push(driver);
    }
    out
}

/// Adapter: decompose the latest period-over-period delta of `measure`
/// ("rows" = row count) using a built [`DimensionIndex`].
///
/// Returns `None` when there are fewer than two periods or the totals carry
/// no movement at all.
pub fn decompose_latest_delta(
    index: &DimensionIndex,
    measure: &str,
) -> Option<(String, String, DeltaDecomposition)> {
    let n = index.periods.len();
    if n < 2 {
        return None;
    }
    let (pi, ci) = (n - 2, n - 1);
    let prev_period = index.periods[pi].clone();
    let curr_period = index.periods[ci].clone();

    let sums_at = |agg: &crate::analysis::candidates::SliceAgg, idx: usize| -> (f64, f64, usize) {
        if measure == "rows" {
            let c = agg.row_counts[idx];
            // Row counts: each row contributes 1, so Σx = Σx² = n.
            (c as f64, c as f64, c)
        } else {
            (
                agg.sums.get(measure).map_or(0.0, |v| v[idx]),
                agg.sum_squares.get(measure).map_or(0.0, |v| v[idx]),
                agg.row_counts[idx],
            )
        }
    };

    let (total_prev, _, _) = sums_at(&index.total, pi);
    let (total_curr, _, _) = sums_at(&index.total, ci);
    if total_prev == 0.0 && total_curr == 0.0 {
        return None;
    }

    let slices: Vec<DriverSlice> = index
        .slices
        .iter()
        .map(|s| {
            let (prev_sum, prev_sq, prev_n) = sums_at(&s.agg, pi);
            let (curr_sum, curr_sq, curr_n) = sums_at(&s.agg, ci);
            DriverSlice {
                dimension: s.dimension.clone(),
                value: s.value.clone(),
                prev_sum,
                curr_sum,
                prev_sum_squares: prev_sq,
                curr_sum_squares: curr_sq,
                prev_n,
                curr_n,
            }
        })
        .collect();

    Some((
        prev_period,
        curr_period,
        rank_segment_drivers(&slices, total_prev, total_curr),
    ))
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::suboptimal_flops)]
mod tests {
    use super::*;

    fn slice(dim: &str, value: &str, prev: f64, curr: f64, n: usize) -> DriverSlice {
        // Synthesize moments as if every row carried mean value with a bit of
        // spread: sumsq = n·mean² + n (unit variance mass).
        let nf = n as f64;
        DriverSlice {
            dimension: dim.to_string(),
            value: value.to_string(),
            prev_sum: prev,
            curr_sum: curr,
            prev_sum_squares: (prev / nf).powi(2) * nf + nf,
            curr_sum_squares: (curr / nf).powi(2) * nf + nf,
            prev_n: n,
            curr_n: n,
        }
    }

    #[test]
    fn top_driver_carries_the_delta() {
        // Total went 1000 → 600; EU alone dropped 380 of the 400.
        let slices = vec![
            slice("region", "EU", 500.0, 120.0, 50),
            slice("region", "NA", 400.0, 390.0, 50),
            slice("region", "APAC", 100.0, 90.0, 20),
        ];
        let d = rank_segment_drivers(&slices, 1000.0, 600.0);
        assert!(!d.offsetting);
        let top = &d.drivers[0];
        assert_eq!(top.value, "EU");
        assert!(
            top.contribution_pct < -50.0,
            "EU explains most of the drop: {}",
            top.contribution_pct
        );
    }

    #[test]
    fn offsetting_movements_are_flagged() {
        // Net ≈ 0 but two segments each moved 400 in opposite directions.
        let slices = vec![
            slice("region", "EU", 500.0, 100.0, 50),
            slice("region", "NA", 100.0, 500.0, 50),
        ];
        let d = rank_segment_drivers(&slices, 600.0, 600.0);
        assert!(d.offsetting, "sign-cancellation must be flagged");
        // Contributions vs gross: each side ≈ ±50%.
        assert!(d.drivers.iter().all(|dr| dr.contribution_pct.abs() < 60.0));
        assert!(d.drivers.iter().any(|dr| dr.contribution_pct > 40.0));
        assert!(d.drivers.iter().any(|dr| dr.contribution_pct < -40.0));
    }

    #[test]
    fn diversity_caps_per_dimension() {
        let mut slices = Vec::new();
        for i in 0..10 {
            slices.push(slice(
                "city",
                &format!("c{i}"),
                100.0,
                200.0 + f64::from(i),
                20,
            ));
        }
        slices.push(slice("region", "EU", 100.0, 250.0, 20));
        let d = rank_segment_drivers(&slices, 1100.0, 2300.0);
        let city_count = d.drivers.iter().filter(|x| x.dimension == "city").count();
        assert!(city_count <= MAX_PER_DIMENSION);
        assert!(d.drivers.iter().any(|x| x.dimension == "region"));
        assert!(d.drivers.len() <= MAX_DRIVERS);
    }

    #[test]
    fn bonferroni_scales_p_by_slice_count() {
        let one = vec![slice("region", "EU", 500.0, 100.0, 50)];
        let solo = rank_segment_drivers(&one, 500.0, 100.0);
        let many: Vec<DriverSlice> = (0..10)
            .map(|i| {
                if i == 0 {
                    slice("region", "EU", 500.0, 100.0, 50)
                } else {
                    slice("region", &format!("r{i}"), 100.0, 101.0, 50)
                }
            })
            .collect();
        let crowd = rank_segment_drivers(&many, 1400.0, 1009.0);
        let solo_p = solo.drivers[0].p_value;
        let crowd_p = crowd
            .drivers
            .iter()
            .find(|d| d.value == "EU")
            .unwrap()
            .p_value;
        assert!(
            crowd_p >= solo_p,
            "correction must not shrink p: solo={solo_p} crowd={crowd_p}"
        );
    }
}
