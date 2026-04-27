//! Change-point detection via simple CUSUM (cumulative sum). Looks for the
//! single most likely level shift in a time series.

#[derive(Debug, Clone)]
pub struct ChangePointResult {
    pub column: String,
    pub period: String,
    pub before_mean: f64,
    pub after_mean: f64,
    pub cusum: f64,
    pub p_value: f64,
}

const MIN_RELATIVE_SHIFT: f64 = 0.15;

pub fn detect_change_point(
    column: &str,
    period_means: &[(String, f64)],
) -> Option<ChangePointResult> {
    if period_means.len() < 6 {
        return None;
    }
    let n = period_means.len();
    let values: Vec<f64> = period_means.iter().map(|(_, v)| *v).collect();
    let global_mean: f64 = values.iter().sum::<f64>() / n as f64;
    let global_std: f64 = {
        let sum_sq: f64 = values.iter().map(|v| (v - global_mean).powi(2)).sum();
        (sum_sq / (n as f64 - 1.0)).sqrt()
    };
    if global_std == 0.0 {
        return None;
    }

    // Compute CUSUM, find argmax of |cusum|
    let mut cusum: Vec<f64> = Vec::with_capacity(n);
    let mut acc = 0.0;
    for v in &values {
        acc += v - global_mean;
        cusum.push(acc);
    }
    let mut max_abs = 0.0;
    let mut idx = 0;
    for (i, &c) in cusum.iter().enumerate() {
        if c.abs() > max_abs {
            max_abs = c.abs();
            idx = i;
        }
    }
    // Avoid endpoints
    if idx < 2 || idx > n - 3 {
        return None;
    }

    let before: &[f64] = &values[..=idx];
    let after: &[f64] = &values[idx + 1..];
    let before_mean: f64 = before.iter().sum::<f64>() / before.len() as f64;
    let after_mean: f64 = after.iter().sum::<f64>() / after.len() as f64;
    let denom = before_mean.abs().max(1e-6);
    let rel_shift = (after_mean - before_mean).abs() / denom;
    if rel_shift < MIN_RELATIVE_SHIFT {
        return None;
    }

    // Synthetic p-value: convert |cusum|/σ to a normal-tail probability
    let z = max_abs / (global_std * (n as f64).sqrt());
    let p_value = 2.0 * (1.0 - phi_approx(z.abs())).clamp(0.0, 1.0);

    Some(ChangePointResult {
        column: column.to_string(),
        period: period_means[idx].0.clone(),
        before_mean,
        after_mean,
        cusum: max_abs,
        p_value,
    })
}

fn phi_approx(z: f64) -> f64 {
    // Φ(z) using Abramowitz & Stegun
    let abs_z = z.abs();
    if abs_z > 8.0 {
        return if z > 0.0 { 1.0 } else { 0.0 };
    }
    let pdf = (-0.5 * abs_z * abs_z).exp() / (2.0 * std::f64::consts::PI).sqrt();
    let t = 1.0 / 0.231_641_9_f64.mul_add(abs_z, 1.0);
    let poly = 0.319_381_530
        + t * (-0.356_563_782 + t * (1.781_477_937 + t * (-1.821_255_978 + t * 1.330_274_429)));
    let cdf = (pdf * t).mul_add(-poly, 1.0);
    if z > 0.0 {
        cdf
    } else {
        1.0 - cdf
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn detects_step_shift() {
        let mut data: Vec<(String, f64)> = Vec::new();
        for i in 0..10 {
            data.push((format!("p{i:02}"), 100.0));
        }
        for i in 10..20 {
            data.push((format!("p{i:02}"), 150.0));
        }
        let r = detect_change_point("x", &data).unwrap();
        assert!(r.before_mean > 90.0 && r.before_mean < 110.0);
        assert!(r.after_mean > 140.0 && r.after_mean < 160.0);
    }

    #[test]
    fn no_change_for_flat_series() {
        let data: Vec<(String, f64)> = (0..10).map(|i| (format!("p{i}"), 100.0)).collect();
        assert!(detect_change_point("x", &data).is_none());
    }
}
