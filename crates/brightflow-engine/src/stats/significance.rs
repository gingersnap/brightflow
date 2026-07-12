use statrs::distribution::{ContinuousCDF, Normal, StudentsT};

/// Calculate two-tailed p-value for a z-score using normal distribution
pub fn p_value_from_z(z_score: f64) -> f64 {
    let Ok(normal) = Normal::new(0.0, 1.0) else {
        return 1.0;
    };
    2.0 * (1.0 - normal.cdf(z_score.abs()))
}

/// Calculate p-value for correlation coefficient using t-distribution
pub fn p_value_for_correlation(r: f64, n: usize) -> f64 {
    if n <= 2 {
        return 1.0;
    }

    // Perfect correlation → t = ∞ → p = 0; feeding ∞ to the CDF panics in
    // statrs' incomplete-beta.
    let one_minus_r2 = r.mul_add(-r, 1.0);
    if one_minus_r2 <= f64::EPSILON {
        return 0.0;
    }

    let df = n as f64 - 2.0;
    let t = r * (df / one_minus_r2).sqrt();
    if !t.is_finite() {
        return 0.0;
    }

    let Ok(t_dist) = StudentsT::new(0.0, 1.0, df) else {
        return 1.0;
    };
    2.0 * (1.0 - t_dist.cdf(t.abs()))
}

/// Calculate p-value for t-test (two-sample, unequal variance - Welch's t-test)
pub fn p_value_welch_t_test(
    mean1: f64,
    std1: f64,
    n1: usize,
    mean2: f64,
    std2: f64,
    n2: usize,
) -> f64 {
    // Need at least 2 samples in each group for valid t-test
    if n1 < 2 || n2 < 2 {
        return 1.0;
    }

    let var1 = std1 * std1;
    let var2 = std2 * std2;

    let se = (var1 / n1 as f64 + var2 / n2 as f64).sqrt();
    if se == 0.0 || !se.is_finite() {
        return 1.0;
    }

    let t = (mean1 - mean2) / se;
    if !t.is_finite() {
        return 1.0;
    }

    let df_num = (var1 / n1 as f64 + var2 / n2 as f64).powi(2);
    let df_denom = (var1 / n1 as f64).powi(2) / (n1 as f64 - 1.0)
        + (var2 / n2 as f64).powi(2) / (n2 as f64 - 1.0);

    if df_denom == 0.0 || !df_denom.is_finite() {
        return 1.0;
    }

    let df = df_num / df_denom;
    if df <= 0.0 || !df.is_finite() {
        return 1.0;
    }

    match StudentsT::new(0.0, 1.0, df) {
        Ok(t_dist) => 2.0 * (1.0 - t_dist.cdf(t.abs())),
        Err(_) => 1.0,
    }
}

/// Calculate z-score
pub fn z_score(value: f64, mean: f64, std_dev: f64) -> f64 {
    if std_dev == 0.0 {
        return 0.0;
    }
    (value - mean) / std_dev
}

/// Calculate mean of a slice
pub fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

/// Calculate standard deviation of a slice
pub fn std_dev(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let m = mean(values);
    let variance = values.iter().map(|v| (v - m).powi(2)).sum::<f64>() / (values.len() - 1) as f64;
    variance.sqrt()
}

/// Calculate Pearson correlation coefficient
#[allow(clippy::suspicious_operation_groupings)] // sum_x * sum_x is correct (variance formula)
pub fn pearson_correlation(x: &[f64], y: &[f64]) -> Option<f64> {
    if x.len() != y.len() || x.len() < 3 {
        return None;
    }

    let n = x.len() as f64;
    let sum_x: f64 = x.iter().sum();
    let sum_y: f64 = y.iter().sum();
    let sum_xy: f64 = x.iter().zip(y.iter()).map(|(xi, yi)| xi * yi).sum();
    let sum_x2: f64 = x.iter().map(|xi| xi * xi).sum();
    let sum_y2: f64 = y.iter().map(|yi| yi * yi).sum();

    let numerator = n.mul_add(sum_xy, -(sum_x * sum_y));
    let sum_x_squared = sum_x * sum_x;
    let sum_y_squared = sum_y * sum_y;
    let denominator =
        (n.mul_add(sum_x2, -sum_x_squared) * n.mul_add(sum_y2, -sum_y_squared)).sqrt();

    if denominator == 0.0 {
        return None;
    }

    Some(numerator / denominator)
}

/// Simple linear regression returning (slope, intercept, r_squared)
#[allow(clippy::suspicious_operation_groupings)] // sum_x * sum_x is correct (variance formula)
pub fn linear_regression(x: &[f64], y: &[f64]) -> Option<(f64, f64, f64)> {
    if x.len() != y.len() || x.len() < 3 {
        return None;
    }

    let n = x.len() as f64;
    let sum_x: f64 = x.iter().sum();
    let sum_y: f64 = y.iter().sum();
    let sum_xy: f64 = x.iter().zip(y.iter()).map(|(xi, yi)| xi * yi).sum();
    let sum_x2: f64 = x.iter().map(|xi| xi * xi).sum();

    let sum_x_squared = sum_x * sum_x;
    let denominator = n.mul_add(sum_x2, -sum_x_squared);
    if denominator == 0.0 {
        return None;
    }

    let slope = n.mul_add(sum_xy, -(sum_x * sum_y)) / denominator;
    let intercept = slope.mul_add(-sum_x, sum_y) / n;

    let y_mean = sum_y / n;
    let ss_tot: f64 = y.iter().map(|yi| (yi - y_mean).powi(2)).sum();
    let ss_res: f64 = x
        .iter()
        .zip(y.iter())
        .map(|(xi, yi)| (yi - (slope * xi + intercept)).powi(2))
        .sum();

    let r_squared = if ss_tot == 0.0 {
        0.0
    } else {
        1.0 - ss_res / ss_tot
    };

    Some((slope, intercept, r_squared))
}

/// Calculate autocorrelation at a given lag
/// Returns the Pearson correlation between the series and a lagged version of itself
pub fn autocorrelation(values: &[f64], lag: usize) -> Option<f64> {
    if values.len() <= lag + 2 {
        return None;
    }

    let n = values.len() - lag;
    let original: Vec<f64> = values[..n].to_vec();
    let lagged: Vec<f64> = values[lag..].to_vec();

    pearson_correlation(&original, &lagged)
}

/// Calculate p-value for autocorrelation using Fisher's z-transformation
/// This accounts for the reduced degrees of freedom due to lag
pub fn p_value_for_autocorrelation(r: f64, n: usize, lag: usize) -> f64 {
    // Effective sample size is reduced by the lag
    let effective_n = n.saturating_sub(lag);
    if effective_n <= 3 {
        return 1.0;
    }

    // Fisher's z-transformation
    if r.abs() >= 1.0 {
        return if r.abs() > 0.99 { 0.0 } else { 1.0 };
    }

    let z = 0.5 * ((1.0 + r) / (1.0 - r)).ln();
    let se = 1.0 / ((effective_n - 3) as f64).sqrt();

    if se == 0.0 || !se.is_finite() {
        return 1.0;
    }

    let z_stat = z / se;
    p_value_from_z(z_stat)
}

/// Linear regression with prediction interval for extrapolation.
///
/// Returns (predicted_value, lower_bound, upper_bound, p_value).
/// The p_value indicates whether the actual value falls outside the prediction interval.
pub fn prediction_interval(
    x: &[f64],
    y: &[f64],
    x_new: f64,
    y_actual: f64,
    confidence: f64,
) -> Option<(f64, f64, f64, f64)> {
    if x.len() != y.len() || x.len() < 4 {
        return None;
    }

    let (slope, intercept, _) = linear_regression(x, y)?;
    let y_pred = slope.mul_add(x_new, intercept);

    let n = x.len() as f64;
    let x_mean = mean(x);

    // Calculate residual standard error
    let ss_res: f64 = x
        .iter()
        .zip(y.iter())
        .map(|(xi, yi)| (yi - (slope * xi + intercept)).powi(2))
        .sum();

    let mse = ss_res / (n - 2.0);
    let se_residual = mse.sqrt();

    if !se_residual.is_finite() {
        return None;
    }

    // Handle perfect fit case (zero residual)
    if se_residual == 0.0 {
        let deviation = (y_actual - y_pred).abs();
        if deviation < 1e-10 {
            // Actual matches prediction exactly
            return Some((y_pred, y_pred, y_pred, 1.0));
        }
        // Any deviation from perfect prediction is highly significant
        return Some((y_pred, y_pred, y_pred, 0.0));
    }

    // Sum of squared deviations of x from its mean
    let ss_x: f64 = x.iter().map(|xi| (xi - x_mean).powi(2)).sum();

    if ss_x == 0.0 {
        return None;
    }

    // Standard error of prediction (includes both model uncertainty and individual variation)
    let se_pred = se_residual * (1.0 + 1.0 / n + (x_new - x_mean).powi(2) / ss_x).sqrt();

    // t-critical value for the given confidence level
    let df = n - 2.0;
    let Ok(t_dist) = StudentsT::new(0.0, 1.0, df) else {
        return None;
    };

    // For a two-tailed interval
    let alpha = 1.0 - confidence;
    let t_crit = t_dist.inverse_cdf(1.0 - alpha / 2.0);

    let margin = t_crit * se_pred;
    let lower = y_pred - margin;
    let upper = y_pred + margin;

    // Calculate p-value for the deviation
    let t_stat = (y_actual - y_pred) / se_pred;
    let p_value = 2.0 * (1.0 - t_dist.cdf(t_stat.abs()));

    Some((y_pred, lower, upper, p_value))
}
