//! Concentration alert: detects when a metric's value is dominated by a small
//! number of segment values. Reports the Herfindahl-Hirschman Index (HHI) and
//! the top-N share.
//!
//! Triggers when HHI > 0.25 (moderately concentrated) or top-3 share > 60%.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ConcentrationResult {
    pub column: String,
    pub segment_column: String,
    pub hhi: f64,
    pub top_n: usize,
    pub top_share: f64,
    pub lorenz_population: Vec<f64>,
    pub lorenz_share: Vec<f64>,
    pub gini: f64,
}

const HHI_THRESHOLD: f64 = 0.25;
const TOP_N: usize = 3;
const TOP_SHARE_THRESHOLD: f64 = 60.0;

pub fn detect_concentration(
    column: &str,
    segment_column: &str,
    target_values: &[f64],
    segment_values: &[String],
) -> Option<ConcentrationResult> {
    if target_values.len() != segment_values.len() || target_values.is_empty() {
        return None;
    }

    // Sum metric per segment value
    let mut by_segment: HashMap<&str, f64> = HashMap::new();
    let mut total: f64 = 0.0;
    for (val, seg) in target_values.iter().zip(segment_values.iter()) {
        if val.is_finite() && *val > 0.0 {
            *by_segment.entry(seg.as_str()).or_insert(0.0) += val;
            total += val;
        }
    }

    if total <= 0.0 || by_segment.is_empty() {
        return None;
    }

    // Compute shares (sorted desc)
    let mut shares: Vec<f64> = by_segment.values().map(|v| v / total).collect();
    shares.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));

    let hhi: f64 = shares.iter().map(|s| s * s).sum();
    let top_share: f64 = shares.iter().take(TOP_N).sum::<f64>() * 100.0;

    // Lorenz curve: cumulative share by cumulative population (sorted ascending)
    let mut sorted_asc = shares.clone();
    sorted_asc.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted_asc.len() as f64;
    let mut cum_share: Vec<f64> = Vec::with_capacity(sorted_asc.len() + 1);
    let mut cum_pop: Vec<f64> = Vec::with_capacity(sorted_asc.len() + 1);
    cum_share.push(0.0);
    cum_pop.push(0.0);
    let mut acc = 0.0;
    for (i, s) in sorted_asc.iter().enumerate() {
        acc += s;
        cum_share.push(acc);
        cum_pop.push((i as f64 + 1.0) / n);
    }

    // Gini = 1 - 2 * area_under_lorenz; trapezoidal
    let mut area = 0.0;
    for i in 1..cum_pop.len() {
        let dx = cum_pop[i] - cum_pop[i - 1];
        area += dx * (cum_share[i] + cum_share[i - 1]) / 2.0;
    }
    let gini = 2.0_f64.mul_add(-area, 1.0).clamp(0.0, 1.0);

    if hhi < HHI_THRESHOLD && top_share < TOP_SHARE_THRESHOLD {
        return None;
    }

    Some(ConcentrationResult {
        column: column.to_string(),
        segment_column: segment_column.to_string(),
        hhi,
        top_n: TOP_N,
        top_share,
        lorenz_population: cum_pop,
        lorenz_share: cum_share,
        gini,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn detects_high_concentration() {
        // 90% in one segment
        let values = vec![100.0, 5.0, 5.0];
        let segs = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let result = detect_concentration("revenue", "region", &values, &segs);
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(r.hhi > 0.8);
        assert!(r.top_share > 90.0);
    }

    #[test]
    fn no_alert_for_uniform() {
        // Equal across 10 segments
        let values: Vec<f64> = (0..10).map(|_| 100.0).collect();
        let segs: Vec<String> = (0..10).map(|i| format!("s{i}")).collect();
        let result = detect_concentration("revenue", "region", &values, &segs);
        assert!(result.is_none());
    }
}
