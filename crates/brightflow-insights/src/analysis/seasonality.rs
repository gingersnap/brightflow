use crate::stats::significance::{autocorrelation, p_value_for_autocorrelation};

#[derive(Debug, Clone)]
pub struct SeasonalityResult {
    pub column: String,
    pub period_days: u32,     // 7=weekly, 30=monthly, 365=yearly
    pub period_name: String,  // "weekly", "monthly", "yearly"
    pub autocorrelation: f64, // strength of pattern (-1 to 1)
    pub p_value: f64,
}

/// Standard lags to test for seasonality (in days)
const SEASONALITY_LAGS: &[(u32, &str)] = &[
    (7, "weekly"),
    (14, "biweekly"),
    (30, "monthly"),
    (90, "quarterly"),
    (365, "yearly"),
];

/// Detect seasonality in time-series data using autocorrelation.
///
/// # Arguments
/// * `column` - Name of the column being analyzed
/// * `values` - Time-series values (assumed to be daily or regularly spaced)
/// * `timestamps` - Unix timestamps for each value (used to determine actual spacing)
///
/// # Returns
/// The strongest significant seasonality pattern, if any.
///
/// # Trigger criteria
/// - Autocorrelation |r| > 0.5
/// - p-value < 0.05
/// - Minimum 2 full cycles of data
pub fn detect_seasonality(
    column: &str,
    values: &[f64],
    timestamps: &[i64],
) -> Option<SeasonalityResult> {
    if values.len() < 14 || timestamps.len() != values.len() {
        return None;
    }

    // Determine the average spacing between data points (in days)
    let avg_spacing_days = calculate_avg_spacing_days(timestamps);
    if avg_spacing_days <= 0.0 {
        return None;
    }

    let mut best_result: Option<SeasonalityResult> = None;
    let mut best_score: f64 = 0.0;

    for &(period_days, period_name) in SEASONALITY_LAGS {
        // Convert period from days to data points based on actual data spacing
        let lag = (period_days as f64 / avg_spacing_days).round() as usize;

        // Need at least 2 full cycles
        if values.len() < lag * 2 + 3 {
            continue;
        }

        // Calculate autocorrelation at this lag
        if let Some(r) = autocorrelation(values, lag) {
            let p_value = p_value_for_autocorrelation(r, values.len(), lag);

            // Check trigger criteria
            if r.abs() > 0.5 && p_value < 0.05 {
                // Score by both strength and significance
                let score = r.abs() / (p_value + 0.001);

                if score > best_score {
                    best_score = score;
                    best_result = Some(SeasonalityResult {
                        column: column.to_string(),
                        period_days,
                        period_name: period_name.to_string(),
                        autocorrelation: r,
                        p_value,
                    });
                }
            }
        }
    }

    best_result
}

/// Calculate average spacing between timestamps in days
fn calculate_avg_spacing_days(timestamps: &[i64]) -> f64 {
    if timestamps.len() < 2 {
        return 0.0;
    }

    let mut total_diff: i64 = 0;
    let mut count = 0;

    for i in 1..timestamps.len() {
        let diff = timestamps[i] - timestamps[i - 1];
        if diff > 0 {
            total_diff += diff;
            count += 1;
        }
    }

    if count == 0 {
        return 0.0;
    }

    // Convert from seconds to days
    let avg_seconds = total_diff as f64 / count as f64;
    avg_seconds / 86400.0
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    #[test]
    fn test_detect_weekly_seasonality() {
        // Generate synthetic weekly pattern (7 days)
        let mut values = Vec::new();
        let mut timestamps = Vec::new();

        let start_ts: i64 = 1609459200; // 2021-01-01 00:00:00 UTC

        for i in 0..100 {
            // 100 days of data
            let day_of_week = (i % 7) as f64;
            // Weekly pattern: peaks on day 3 (Wednesday)
            let seasonal = 100.0 + 30.0 * (2.0 * PI * day_of_week / 7.0).sin();
            values.push(seasonal);
            timestamps.push(start_ts + i * 86400);
        }

        let result = detect_seasonality("Sales", &values, &timestamps);
        assert!(result.is_some());
        let seasonality = result.unwrap();
        assert_eq!(seasonality.period_name, "weekly");
        assert!(seasonality.autocorrelation.abs() > 0.5);
    }

    #[test]
    fn test_no_seasonality_random() {
        // Random data should not show seasonality
        let values: Vec<f64> = (0..100)
            .map(|i| 100.0 + ((i * 17) % 23) as f64 - 11.0)
            .collect();
        let timestamps: Vec<i64> = (0..100).map(|i| 1609459200 + i * 86400).collect();

        let result = detect_seasonality("Random", &values, &timestamps);
        // Should be None or have weak autocorrelation
        if let Some(r) = result {
            assert!(r.autocorrelation.abs() <= 0.5 || r.p_value >= 0.05);
        }
    }

    #[test]
    fn test_insufficient_data() {
        let values = vec![1.0, 2.0, 3.0];
        let timestamps = vec![1609459200, 1609545600, 1609632000];

        let result = detect_seasonality("Test", &values, &timestamps);
        assert!(result.is_none());
    }
}
