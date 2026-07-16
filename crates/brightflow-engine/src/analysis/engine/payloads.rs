//! Chart-data payload builders attached to analysis nodes.

use std::collections::HashMap;

use crate::analysis::tree::{NamedSeries, NodeData};

use super::cache::ColumnCache;

/// LTTB-ish downsample to ≤MAX_POINTS (simple stride sampling — good enough for
/// glance-charts; replace with LTTB if precision matters later)
const MAX_POINTS: usize = 200;

fn downsample(values: &[f64]) -> (Vec<String>, Vec<f64>) {
    let n = values.len();
    if n <= MAX_POINTS {
        let labels = (0..n).map(|i| i.to_string()).collect();
        return (labels, values.to_vec());
    }
    let stride = n / MAX_POINTS;
    let mut out = Vec::with_capacity(MAX_POINTS);
    let mut labels = Vec::with_capacity(MAX_POINTS);
    let mut i = 0;
    while i < n {
        out.push(values[i]);
        labels.push(i.to_string());
        i += stride.max(1);
    }
    (labels, out)
}

pub(super) fn build_anomaly_series(values: &[f64], mean: f64, std_dev: f64) -> NodeData {
    let (labels, vals) = downsample(values);
    let band_low = vec![2.0_f64.mul_add(-std_dev, mean); vals.len()];
    let band_high = vec![2.0_f64.mul_add(std_dev, mean); vals.len()];
    let marker = vals.len().saturating_sub(1);
    NodeData::Series {
        labels,
        values: vals,
        band_low: Some(band_low),
        band_high: Some(band_high),
        marker_index: Some(marker),
    }
}

pub(super) fn build_trend_data(values: &[f64], slope: f64) -> NodeData {
    let (labels, vals) = downsample(values);
    let n = vals.len();
    if n == 0 {
        return NodeData::SeriesWithFit {
            labels,
            values: vals,
            fit: Vec::new(),
        };
    }
    // Recompute simple linear fit on the (possibly downsampled) series
    let xs: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let mean_y: f64 = vals.iter().sum::<f64>() / n as f64;
    let mean_x = (n as f64 - 1.0) / 2.0;
    let intercept = slope.mul_add(-mean_x, mean_y);
    let fit: Vec<f64> = xs.iter().map(|x| slope.mul_add(*x, intercept)).collect();
    NodeData::SeriesWithFit {
        labels,
        values: vals,
        fit,
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
        labels: vec![previous_period.to_string(), current_period.to_string()],
        previous: vec![previous_sum / f64::from(previous_n)],
        current: vec![current_sum / f64::from(current_n)],
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
    })
}

pub(super) fn build_scatter_data(
    xs: &[f64],
    ys: &[f64],
    x_label: &str,
    y_label: &str,
    r: f64,
) -> NodeData {
    // Downsample if too large
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
