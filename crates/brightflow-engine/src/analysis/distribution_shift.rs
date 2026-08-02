//! Distribution shift between two periods using the two-sample
//! Kolmogorov-Smirnov test. Triggers when KS statistic is significant.

#[derive(Debug, Clone)]
pub struct DistributionShiftResult {
    pub column: String,
    pub previous_period: String,
    pub current_period: String,
    pub ks_statistic: f64,
    pub p_value: f64,
    pub bin_edges: Vec<f64>,
    pub previous_hist: Vec<f64>,
    pub current_hist: Vec<f64>,
}

const KS_THRESHOLD: f64 = 0.2;
const NUM_BINS: usize = 20;

pub fn detect_distribution_shift(
    column: &str,
    values: &[f64],
    period_labels: &[Option<String>],
    previous_period: &str,
    current_period: &str,
) -> Option<DistributionShiftResult> {
    if values.len() != period_labels.len() {
        return None;
    }
    let mut prev: Vec<f64> = Vec::new();
    let mut curr: Vec<f64> = Vec::new();
    for (val, p) in values.iter().zip(period_labels.iter()) {
        match p {
            Some(s) if s == previous_period => prev.push(*val),
            Some(s) if s == current_period => curr.push(*val),
            _ => {},
        }
    }
    if prev.len() < 5 || curr.len() < 5 {
        return None;
    }

    // KS statistic
    let mut combined: Vec<f64> = prev.iter().chain(curr.iter()).copied().collect();
    combined.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let mut sorted_prev = prev.clone();
    sorted_prev.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut sorted_curr = curr.clone();
    sorted_curr.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let n_prev = sorted_prev.len() as f64;
    let n_curr = sorted_curr.len() as f64;

    let mut ks: f64 = 0.0;
    for v in &combined {
        let cdf_prev = sorted_prev.iter().filter(|x| *x <= v).count() as f64 / n_prev;
        let cdf_curr = sorted_curr.iter().filter(|x| *x <= v).count() as f64 / n_curr;
        let d = (cdf_prev - cdf_curr).abs();
        if d > ks {
            ks = d;
        }
    }

    if ks < KS_THRESHOLD {
        return None;
    }

    // Asymptotic two-sided p-value (Kolmogorov distribution approx)
    let n_eff = (n_prev * n_curr / (n_prev + n_curr)).sqrt();
    let lambda = (n_eff + 0.12 + 0.11 / n_eff) * ks;
    // Q_KS(λ) ≈ 2 * Σ (-1)^(k-1) exp(-2 k² λ²); use first 5 terms
    let mut p = 0.0;
    for k in 1_i32..=5 {
        let kf = f64::from(k);
        let sign = if k % 2 == 0 { -1.0 } else { 1.0 };
        p += sign * (-2.0 * kf * kf * lambda * lambda).exp();
    }
    let p_value = (2.0 * p).clamp(0.0, 1.0);

    // Build histograms over the combined range
    let v_min = combined.first().copied().unwrap_or(0.0);
    let v_max = combined.last().copied().unwrap_or(1.0);
    let span = (v_max - v_min).max(f64::EPSILON);
    let bin_w = span / NUM_BINS as f64;
    let mut edges = Vec::with_capacity(NUM_BINS + 1);
    for i in 0..=NUM_BINS {
        edges.push((i as f64).mul_add(bin_w, v_min));
    }
    let mut prev_hist = vec![0.0; NUM_BINS];
    let mut curr_hist = vec![0.0; NUM_BINS];
    for v in &prev {
        let idx = (((v - v_min) / bin_w).floor() as usize).min(NUM_BINS - 1);
        prev_hist[idx] += 1.0;
    }
    for v in &curr {
        let idx = (((v - v_min) / bin_w).floor() as usize).min(NUM_BINS - 1);
        curr_hist[idx] += 1.0;
    }
    // Normalize
    if n_prev > 0.0 {
        for h in &mut prev_hist {
            *h /= n_prev;
        }
    }
    if n_curr > 0.0 {
        for h in &mut curr_hist {
            *h /= n_curr;
        }
    }

    Some(DistributionShiftResult {
        column: column.to_string(),
        previous_period: previous_period.to_string(),
        current_period: current_period.to_string(),
        ks_statistic: ks,
        p_value,
        bin_edges: edges,
        previous_hist: prev_hist,
        current_hist: curr_hist,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_obvious_shift() {
        let mut vals: Vec<f64> = Vec::new();
        let mut periods: Vec<Option<String>> = Vec::new();
        for i in 0_i32..50 {
            vals.push(f64::from(i));
            periods.push(Some("p1".to_string()));
        }
        for i in 100_i32..150 {
            vals.push(f64::from(i));
            periods.push(Some("p2".to_string()));
        }
        let r = detect_distribution_shift("x", &vals, &periods, "p1", "p2");
        assert!(r.is_some());
        let r = r.unwrap();
        assert!(r.ks_statistic > 0.9);
    }

    #[test]
    fn no_shift_when_same() {
        // Overlapping uniform distributions in two periods (interleaved)
        let mut vals: Vec<f64> = Vec::new();
        let mut periods: Vec<Option<String>> = Vec::new();
        for i in 0_i32..50 {
            vals.push(f64::from(i));
            periods.push(Some("p1".to_string()));
            vals.push(f64::from(i));
            periods.push(Some("p2".to_string()));
        }
        let r = detect_distribution_shift("x", &vals, &periods, "p1", "p2");
        // Identical distributions → either None or KS near 0
        if let Some(r) = r {
            assert!(r.ks_statistic < 0.3);
        }
    }
}
