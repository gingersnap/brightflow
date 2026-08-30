//! Chart-data payload builders attached to analysis nodes.
//!
//! Every builder here is pure — slices in, `NodeData` out — so the chart
//! payload for a finding is reproducible from the finding's inputs alone.
//! Long raw series are stride-downsampled to a glance-chart budget
//! ([`MAX_POINTS`]) before fit lines and bands are computed, so those overlays
//! always have the same length as the points actually drawn. Overlays are
//! expressed in *sampled-index* space, not original-row space: anything
//! carrying a per-row rate (a trend slope) is rescaled by the stride on the way
//! in, so the overlay lines up with the points rather than with the raw rows.

use std::collections::HashMap;

use crate::analysis::tree::{NamedSeries, NodeData};

use super::cache::ColumnCache;

/// Downsample to ≤MAX_POINTS via stride sampling — good enough for
/// glance-charts; replace with LTTB if precision matters later. Labels are the
/// original indices, so a downsampled point is still traceable to its row.
const MAX_POINTS: usize = 200;

/// Returns the labels, the sampled values, and the stride used.
///
/// The stride is part of the contract, not an implementation detail: callers
/// that fit a line expressed in *original-row* units must rescale it, because
/// sampled position `i` holds the value from original row `i * stride`.
fn downsample(values: &[f64]) -> (Vec<String>, Vec<f64>, usize) {
    let n = values.len();
    if n <= MAX_POINTS {
        let labels = (0..n).map(|i| i.to_string()).collect();
        return (labels, values.to_vec(), 1);
    }
    // Ceiling division: flooring made the bound a lie for MAX_POINTS < n <
    // 2*MAX_POINTS (stride 1 emitted every point).
    let stride = n.div_ceil(MAX_POINTS);
    let mut out = Vec::with_capacity(MAX_POINTS);
    let mut labels = Vec::with_capacity(MAX_POINTS);
    let mut i = 0;
    while i < n {
        out.push(values[i]);
        labels.push(i.to_string());
        i += stride;
    }
    (labels, out, stride)
}

pub(super) fn build_anomaly_series(values: &[f64], mean: f64, std_dev: f64) -> NodeData {
    // Stride is irrelevant here: the bands are constants, not a function of x.
    let (labels, vals, _stride) = downsample(values);
    let band_low = vec![2.0_f64.mul_add(-std_dev, mean); vals.len()];
    let band_high = vec![2.0_f64.mul_add(std_dev, mean); vals.len()];
    let marker = vals.len().saturating_sub(1);
    NodeData::Series {
        labels,
        values: vals,
        band_low: Some(band_low),
        band_high: Some(band_high),
        marker_index: Some(marker),
        y_label: None,
    }
}

/// `slope` is per *original* row. The fit is drawn against sampled positions,
/// so it is rescaled by the stride — without that the line under-slopes by
/// exactly the stride factor and visibly drifts off the points it explains.
pub(super) fn build_trend_data(values: &[f64], slope: f64) -> NodeData {
    let (labels, vals, stride) = downsample(values);
    let n = vals.len();
    if n == 0 {
        return NodeData::SeriesWithFit {
            labels,
            values: vals,
            fit: Vec::new(),
            y_label: None,
        };
    }
    // Recompute the simple linear fit in sampled-index space: one step along
    // the drawn x-axis advances `stride` original rows, hence the rescale.
    let slope = slope * stride as f64;
    let xs: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let mean_y: f64 = vals.iter().sum::<f64>() / n as f64;
    let mean_x = (n as f64 - 1.0) / 2.0;
    let intercept = slope.mul_add(-mean_x, mean_y);
    let fit: Vec<f64> = xs.iter().map(|x| slope.mul_add(*x, intercept)).collect();
    NodeData::SeriesWithFit {
        labels,
        values: vals,
        fit,
        y_label: None,
    }
}

/// Trend chart over an already-aggregated period series (labels = periods)
pub(super) fn build_trend_series_data(labels: &[String], values: &[f64], slope: f64) -> NodeData {
    let n = values.len();
    if n == 0 {
        return NodeData::SeriesWithFit {
            labels: labels.to_vec(),
            values: values.to_vec(),
            fit: Vec::new(),
            y_label: None,
        };
    }
    let mean_y: f64 = values.iter().sum::<f64>() / n as f64;
    let mean_x = (n as f64 - 1.0) / 2.0;
    let intercept = slope.mul_add(-mean_x, mean_y);
    let fit: Vec<f64> = (0..n).map(|x| slope.mul_add(x as f64, intercept)).collect();
    NodeData::SeriesWithFit {
        labels: labels.to_vec(),
        values: values.to_vec(),
        fit,
        y_label: None,
    }
}

/// Anomaly chart over an already-aggregated period series (labels = periods)
pub(super) fn build_anomaly_period_series(
    labels: &[String],
    values: &[f64],
    mean: f64,
    std_dev: f64,
) -> NodeData {
    let band_low = vec![2.0_f64.mul_add(-std_dev, mean); values.len()];
    let band_high = vec![2.0_f64.mul_add(std_dev, mean); values.len()];
    NodeData::Series {
        labels: labels.to_vec(),
        values: values.to_vec(),
        band_low: Some(band_low),
        band_high: Some(band_high),
        marker_index: Some(values.len().saturating_sub(1)),
        y_label: None,
    }
}

pub(super) fn build_period_comparison_data(
    values: &[f64],
    period_labels: &[Option<String>],
    current_period: &str,
    previous_period: &str,
) -> Option<NodeData> {
    let mut current_sum = 0.0;
    let mut current_n = 0_i32;
    let mut previous_sum = 0.0;
    let mut previous_n = 0_i32;
    for (val, period) in values.iter().zip(period_labels.iter()) {
        match period {
            Some(p) if p == current_period => {
                current_sum += val;
                current_n += 1;
            },
            Some(p) if p == previous_period => {
                previous_sum += val;
                previous_n += 1;
            },
            _ => {},
        }
    }
    if current_n == 0 || previous_n == 0 {
        return None;
    }
    Some(NodeData::PairedBars {
        labels: vec![
            crate::analysis::tree::humanize_period(previous_period),
            crate::analysis::tree::humanize_period(current_period),
        ],
        previous: vec![previous_sum / f64::from(previous_n)],
        current: vec![current_sum / f64::from(current_n)],
        y_label: None,
    })
}

pub(super) fn build_seasonality_data(
    values: &[f64],
    period_labels: &[Option<String>],
) -> Option<NodeData> {
    let mut by_period: HashMap<String, (f64, usize)> = HashMap::new();
    for (val, period) in values.iter().zip(period_labels.iter()) {
        if let Some(p) = period {
            let entry = by_period.entry(p.clone()).or_insert((0.0, 0));
            entry.0 += val;
            entry.1 += 1;
        }
    }
    if by_period.is_empty() {
        return None;
    }
    let mut sorted: Vec<(String, f64)> = by_period
        .into_iter()
        .map(|(p, (s, n))| (p, s / n as f64))
        .collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let labels: Vec<String> = sorted.iter().map(|(p, _)| p.clone()).collect();
    let vals: Vec<f64> = sorted.iter().map(|(_, v)| *v).collect();
    Some(NodeData::Series {
        labels,
        values: vals,
        band_low: None,
        band_high: None,
        marker_index: None,
        y_label: None,
    })
}

pub(super) fn build_forecast_data(
    period_values: &[(String, f64)],
    expected: f64,
    actual: f64,
) -> Option<NodeData> {
    if period_values.is_empty() {
        return None;
    }
    let labels: Vec<String> = period_values.iter().map(|(p, _)| p.clone()).collect();
    let history: Vec<f64> = period_values.iter().map(|(_, v)| *v).collect();
    // Simple PI: ±20% of expected — replace with proper PI from forecast detector
    let pi = expected.abs() * 0.2;
    Some(NodeData::Forecast {
        labels,
        history,
        expected,
        actual,
        pi_low: expected - pi,
        pi_high: expected + pi,
    })
}

pub(super) fn build_outlier_cluster_data(
    cache: &ColumnCache,
    period_labels: &[Option<String>],
    target_period: &str,
    columns: &[String],
) -> Option<NodeData> {
    let mut series_list: Vec<NamedSeries> = Vec::new();
    let mut common_labels: Vec<String> = Vec::new();
    let mut marker_idx: Option<usize> = None;

    for col in columns {
        let Some(values) = cache.numeric.get(col) else {
            continue;
        };
        let mut by_period: HashMap<String, (f64, usize)> = HashMap::new();
        for (val, period) in values.iter().zip(period_labels.iter()) {
            if let Some(p) = period {
                let entry = by_period.entry(p.clone()).or_insert((0.0, 0));
                entry.0 += val;
                entry.1 += 1;
            }
        }
        if by_period.is_empty() {
            continue;
        }
        let mut sorted: Vec<(String, f64)> = by_period
            .into_iter()
            .map(|(p, (s, n))| (p, s / n as f64))
            .collect();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        if common_labels.is_empty() {
            common_labels = sorted.iter().map(|(p, _)| p.clone()).collect();
            marker_idx = common_labels.iter().position(|p| p == target_period);
        }
        let vals: Vec<f64> = sorted.iter().map(|(_, v)| *v).collect();
        series_list.push(NamedSeries {
            name: col.clone(),
            values: vals,
        });
    }

    if series_list.is_empty() || common_labels.is_empty() {
        return None;
    }

    Some(NodeData::Multi {
        labels: common_labels,
        series: series_list,
        marker_index: marker_idx.unwrap_or(0),
        y_label: None,
    })
}

pub(super) fn build_scatter_data(
    xs: &[f64],
    ys: &[f64],
    x_label: &str,
    y_label: &str,
    r: f64,
) -> NodeData {
    // Empty input has no pairs to sample or fit; `take` would be 0 below and
    // the stride division would panic. Mirrors the `n == 0` guard in
    // `build_trend_data`.
    if xs.is_empty() || ys.is_empty() {
        return NodeData::Scatter {
            x: Vec::new(),
            y: Vec::new(),
            x_label: x_label.to_string(),
            y_label: y_label.to_string(),
            fit_slope: None,
            fit_intercept: None,
        };
    }
    // Downsample if too large. This strides over *pairs* and refits OLS from
    // the sampled pairs, so unlike `downsample`'s series fit it stays
    // self-consistent without a stride rescale — hence the separate rule
    // (floor + truncate) rather than a shared helper.
    let take = xs.len().min(ys.len()).min(MAX_POINTS);
    let stride = (xs.len() / take).max(1);
    let x: Vec<f64> = xs.iter().step_by(stride).take(take).copied().collect();
    let y: Vec<f64> = ys.iter().step_by(stride).take(take).copied().collect();
    // Compute simple OLS fit
    let n = x.len() as f64;
    let mean_x = x.iter().sum::<f64>() / n;
    let mean_y = y.iter().sum::<f64>() / n;
    let mut num = 0.0;
    let mut den = 0.0;
    for (xi, yi) in x.iter().zip(y.iter()) {
        num = (xi - mean_x).mul_add(yi - mean_y, num);
        den += (xi - mean_x).powi(2);
    }
    let (slope, intercept) = if (den - 0.0).abs() > f64::EPSILON {
        let s = num / den;
        (Some(s), Some(s.mul_add(-mean_x, mean_y)))
    } else {
        (None, None)
    };
    let _ = r; // r already part of analysis
    NodeData::Scatter {
        x,
        y,
        x_label: x_label.to_string(),
        y_label: y_label.to_string(),
        fit_slope: slope,
        fit_intercept: intercept,
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::float_cmp,
        reason = "tests compare exactly constructed values, not computed approximations"
    )]

    use super::*;

    #[test]
    fn downsample_short_series_is_identity() {
        let values: Vec<f64> = (0..MAX_POINTS).map(|i| i as f64).collect();
        let (labels, out, _stride) = downsample(&values);
        assert_eq!(out, values);
        assert_eq!(labels.first().map(String::as_str), Some("0"));
        assert_eq!(labels.last().unwrap(), &(MAX_POINTS - 1).to_string());
    }

    #[test]
    fn downsample_bound_holds_just_over_the_budget() {
        // Regression: flooring stride made 200 < n < 400 emit every point.
        let values: Vec<f64> = (0..399).map(f64::from).collect();
        let (labels, out, _stride) = downsample(&values);
        assert!(out.len() <= MAX_POINTS, "399 points emitted {}", out.len());
        assert_eq!(labels.len(), out.len());
    }

    #[test]
    fn downsample_bound_holds_across_sizes() {
        for n in [201, 400, 401, 999, 1000, 5000] {
            let values: Vec<f64> = (0..n).map(f64::from).collect();
            let (labels, out, _stride) = downsample(&values);
            assert!(out.len() <= MAX_POINTS, "n={n} emitted {}", out.len());
            // Labels are original indices: each sampled value matches its label.
            for (label, val) in labels.iter().zip(&out) {
                assert_eq!(label.parse::<f64>().unwrap(), *val);
            }
            assert_eq!(labels[0], "0", "first point always kept");
        }
    }

    #[test]
    fn anomaly_series_bands_are_mean_plus_minus_two_sigma() {
        let values = [1.0, 2.0, 3.0];
        let NodeData::Series {
            band_low,
            band_high,
            marker_index,
            values: vals,
            ..
        } = build_anomaly_series(&values, 2.0, 0.5)
        else {
            panic!("expected Series");
        };
        assert_eq!(band_low.unwrap(), vec![1.0; 3]);
        assert_eq!(band_high.unwrap(), vec![3.0; 3]);
        assert_eq!(marker_index, Some(2));
        assert_eq!(vals, values);
    }

    #[test]
    fn trend_fit_matches_series_length_and_slope() {
        let values = [10.0, 12.0, 14.0, 16.0];
        let NodeData::SeriesWithFit {
            values: vals, fit, ..
        } = build_trend_data(&values, 2.0)
        else {
            panic!("expected SeriesWithFit");
        };
        assert_eq!(fit.len(), vals.len());
        // Fit passes through the mean with the given slope: consecutive fit
        // points differ by exactly the slope.
        assert!((fit[1] - fit[0] - 2.0).abs() < 1e-9);
        // Perfectly linear input: the fit reproduces the series.
        for (f, v) in fit.iter().zip(&vals) {
            assert!((f - v).abs() < 1e-9);
        }
    }

    #[test]
    fn trend_fit_tracks_points_when_the_series_is_downsampled() {
        // Regression: the fit was computed with the per-original-row slope over
        // sampled positions, so it under-sloped by exactly the stride factor.
        // A perfectly linear series is the sharpest probe — the fit must
        // reproduce it whether or not downsampling kicked in.
        for n in [199usize, 399, 1000] {
            let values: Vec<f64> = (0..n).map(|i| i as f64).collect();
            let NodeData::SeriesWithFit {
                values: vals, fit, ..
            } = build_trend_data(&values, 1.0)
            else {
                panic!("expected SeriesWithFit");
            };
            assert_eq!(fit.len(), vals.len());
            for (i, (f, v)) in fit.iter().zip(&vals).enumerate() {
                assert!(
                    (f - v).abs() < 1e-9,
                    "n={n} i={i}: fit {f} drifted off point {v}"
                );
            }
        }
    }

    #[test]
    fn trend_data_empty_input_is_safe() {
        let NodeData::SeriesWithFit { values, fit, .. } = build_trend_data(&[], 1.0) else {
            panic!("expected SeriesWithFit");
        };
        assert!(values.is_empty());
        assert!(fit.is_empty());
    }

    #[test]
    fn period_comparison_averages_each_period() {
        let values = [10.0, 20.0, 40.0, 60.0];
        let periods = [
            Some("2025-01".to_string()),
            Some("2025-01".to_string()),
            Some("2025-02".to_string()),
            Some("2025-02".to_string()),
        ];
        let Some(NodeData::PairedBars {
            previous, current, ..
        }) = build_period_comparison_data(&values, &periods, "2025-02", "2025-01")
        else {
            panic!("expected PairedBars");
        };
        assert_eq!(previous, vec![15.0]);
        assert_eq!(current, vec![50.0]);
    }

    #[test]
    fn period_comparison_requires_both_periods() {
        let values = [1.0, 2.0];
        let periods = [Some("2025-01".to_string()), Some("2025-01".to_string())];
        assert!(build_period_comparison_data(&values, &periods, "2025-02", "2025-01").is_none());
    }

    #[test]
    fn seasonality_sorts_periods_and_averages() {
        let values = [4.0, 2.0, 6.0];
        let periods = [
            Some("2025-02".to_string()),
            Some("2025-01".to_string()),
            Some("2025-02".to_string()),
        ];
        let Some(NodeData::Series {
            labels,
            values: vals,
            ..
        }) = build_seasonality_data(&values, &periods)
        else {
            panic!("expected Series");
        };
        assert_eq!(labels, vec!["2025-01", "2025-02"]);
        assert_eq!(vals, vec![2.0, 5.0]);
    }

    #[test]
    fn seasonality_empty_periods_is_none() {
        assert!(build_seasonality_data(&[1.0], &[None]).is_none());
    }

    #[test]
    fn forecast_interval_is_twenty_percent_of_expected() {
        let history = vec![("2025-01".to_string(), 5.0)];
        let Some(NodeData::Forecast {
            pi_low,
            pi_high,
            expected,
            ..
        }) = build_forecast_data(&history, 10.0, 12.0)
        else {
            panic!("expected Forecast");
        };
        assert_eq!(expected, 10.0);
        assert_eq!(pi_low, 8.0);
        assert_eq!(pi_high, 12.0);
        assert!(build_forecast_data(&[], 1.0, 1.0).is_none());
    }

    #[test]
    fn scatter_fit_recovers_a_perfect_line() {
        let xs: Vec<f64> = (0..10).map(f64::from).collect();
        let ys: Vec<f64> = xs.iter().map(|x| 3.0f64.mul_add(*x, 1.0)).collect();
        let NodeData::Scatter {
            fit_slope,
            fit_intercept,
            ..
        } = build_scatter_data(&xs, &ys, "x", "y", 1.0)
        else {
            panic!("expected Scatter");
        };
        assert!((fit_slope.unwrap() - 3.0).abs() < 1e-9);
        assert!((fit_intercept.unwrap() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn scatter_empty_input_is_safe() {
        // Regression: `take` was 0, so the stride division panicked with
        // "attempt to divide by zero".
        let NodeData::Scatter {
            x,
            y,
            fit_slope,
            fit_intercept,
            ..
        } = build_scatter_data(&[], &[], "x", "y", 0.0)
        else {
            panic!("expected Scatter");
        };
        assert!(x.is_empty());
        assert!(y.is_empty());
        assert!(fit_slope.is_none());
        assert!(fit_intercept.is_none());

        // One side empty is equally unfittable.
        let NodeData::Scatter { x: half_empty, .. } =
            build_scatter_data(&[1.0, 2.0], &[], "x", "y", 0.0)
        else {
            panic!("expected Scatter");
        };
        assert!(half_empty.is_empty());
    }

    #[test]
    fn scatter_constant_x_has_no_fit() {
        let xs = [2.0, 2.0, 2.0];
        let ys = [1.0, 2.0, 3.0];
        let NodeData::Scatter {
            fit_slope,
            fit_intercept,
            ..
        } = build_scatter_data(&xs, &ys, "x", "y", 0.0)
        else {
            panic!("expected Scatter");
        };
        assert!(fit_slope.is_none());
        assert!(fit_intercept.is_none());
    }
}
