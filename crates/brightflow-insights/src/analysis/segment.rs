use anyhow::Result;
use polars::prelude::*;

use crate::stats::significance::{mean, p_value_welch_t_test, std_dev};

#[derive(Debug, Clone)]
pub struct SegmentResult {
    pub target_column: String,
    pub segment_column: String,
    pub segment_value: String,
    pub contribution: f64,
    pub change_percent: f64,
    /// What percentage of the total change this segment explains
    pub contribution_pct: f64,
    pub p_value: f64,
}

/// Attribute target column variance to segments
pub fn attribute_segment(
    df: &DataFrame,
    target_col: &str,
    segment_col: &str,
) -> Result<Option<SegmentResult>> {
    let target = df.column(target_col)?;
    let segment = df.column(segment_col)?;

    let target_values: Vec<f64> = target
        .cast(&DataType::Float64)?
        .f64()?
        .into_iter()
        .flatten()
        .collect();

    let segment_values: Vec<String> = segment
        .cast(&DataType::String)?
        .str()?
        .into_iter()
        .map(|v| v.unwrap_or("").to_string())
        .collect();

    if target_values.len() != segment_values.len() || target_values.len() < 3 {
        return Ok(None);
    }

    let overall_mean = mean(&target_values);

    let mut segments: std::collections::HashMap<String, Vec<f64>> = std::collections::HashMap::new();
    for (val, seg) in target_values.iter().zip(segment_values.iter()) {
        segments.entry(seg.clone()).or_default().push(*val);
    }

    let mut best_segment: Option<SegmentResult> = None;
    let mut best_contribution = 0.0f64;

    for (seg_value, seg_data) in &segments {
        if seg_data.len() < 2 {
            continue;
        }

        let seg_mean = mean(seg_data);
        let seg_std = std_dev(seg_data);

        let other_data: Vec<f64> = segments
            .iter()
            .filter(|(k, _)| *k != seg_value)
            .flat_map(|(_, v)| v.iter().copied())
            .collect();

        if other_data.len() < 2 {
            continue;
        }

        let other_mean = mean(&other_data);
        let other_std = std_dev(&other_data);

        let contribution = ((seg_mean - overall_mean) * seg_data.len() as f64) / target_values.len() as f64;
        let change_percent = if other_mean != 0.0 {
            ((seg_mean - other_mean) / other_mean) * 100.0
        } else {
            0.0
        };

        let p_value = p_value_welch_t_test(
            seg_mean,
            seg_std,
            seg_data.len(),
            other_mean,
            other_std,
            other_data.len(),
        );

        if contribution.abs() > best_contribution.abs() {
            best_contribution = contribution;
            best_segment = Some(SegmentResult {
                target_column: target_col.to_string(),
                segment_column: segment_col.to_string(),
                segment_value: seg_value.clone(),
                contribution,
                change_percent,
                contribution_pct: 0.0, // Not applicable for non-period attribution
                p_value,
            });
        }
    }

    Ok(best_segment)
}

/// Attribute a period anomaly to segments using pre-cached column data
/// Returns all significant segments sorted by contribution percentage (descending)
pub fn attribute_period_segments_cached(
    target_col: &str,
    segment_col: &str,
    target_values: &[f64],
    segment_values: &[String],
    anomalous_period: &str,
    period_labels: &[Option<String>],
) -> Vec<SegmentResult> {
    if target_values.len() != segment_values.len() || target_values.len() < 3 {
        return Vec::new();
    }

    // Calculate overall total change first
    let mut period_values: Vec<f64> = Vec::new();
    let mut other_values: Vec<f64> = Vec::new();

    for (val, period) in target_values.iter().zip(period_labels.iter()) {
        if let Some(p) = period {
            if p == anomalous_period {
                period_values.push(*val);
            } else {
                other_values.push(*val);
            }
        }
    }

    if period_values.is_empty() || other_values.is_empty() {
        return Vec::new();
    }

    let overall_period_mean = mean(&period_values);
    let overall_other_mean = mean(&other_values);
    let total_change = (overall_period_mean - overall_other_mean) * period_values.len() as f64;

    if total_change.abs() < 0.0001 {
        return Vec::new();
    }

    // Group data by segment, separating anomalous period from others
    let mut segment_in_period: std::collections::HashMap<String, Vec<f64>> =
        std::collections::HashMap::new();
    let mut segment_other_periods: std::collections::HashMap<String, Vec<f64>> =
        std::collections::HashMap::new();

    for ((val, seg), period) in target_values
        .iter()
        .zip(segment_values.iter())
        .zip(period_labels.iter())
    {
        if let Some(p) = period {
            if p == anomalous_period {
                segment_in_period.entry(seg.clone()).or_default().push(*val);
            } else {
                segment_other_periods.entry(seg.clone()).or_default().push(*val);
            }
        }
    }

    // Collect all segments with their contributions
    let mut results: Vec<SegmentResult> = Vec::new();

    for (seg_value, period_data) in &segment_in_period {
        if period_data.is_empty() {
            continue;
        }

        let other_data = match segment_other_periods.get(seg_value) {
            Some(d) if !d.is_empty() => d,
            _ => continue,
        };

        let period_mean = mean(period_data);
        let other_mean = mean(other_data);
        let other_std = std_dev(other_data);

        if other_mean == 0.0 {
            continue;
        }

        // Calculate how much this segment changed in the anomalous period
        let change_percent = ((period_mean - other_mean) / other_mean) * 100.0;

        // Contribution = segment's share of the total change, weighted by segment size
        let contribution = (period_mean - other_mean) * period_data.len() as f64;

        // What percentage of the total change this segment explains
        let contribution_pct = (contribution / total_change) * 100.0;

        let p_value = if other_data.len() >= 2 && period_data.len() >= 2 {
            let period_std = std_dev(period_data);
            p_value_welch_t_test(
                period_mean,
                period_std,
                period_data.len(),
                other_mean,
                other_std,
                other_data.len(),
            )
        } else {
            1.0
        };

        results.push(SegmentResult {
            target_column: target_col.to_string(),
            segment_column: segment_col.to_string(),
            segment_value: seg_value.clone(),
            contribution,
            change_percent,
            contribution_pct,
            p_value,
        });
    }

    // Sort by contribution percentage descending (highest impact first)
    results.sort_by(|a, b| {
        b.contribution_pct
            .abs()
            .partial_cmp(&a.contribution_pct.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Return the top segment for this dimension (the one with highest contribution)
    results.into_iter().take(1).collect()
}
