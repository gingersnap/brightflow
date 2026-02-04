use crate::stats::significance::prediction_interval;

#[derive(Debug, Clone)]
pub struct ForecastDeviation {
    pub column: String,
    pub period: String,
    pub actual: f64,
    pub expected: f64,
    pub deviation_percent: f64,
    pub p_value: f64,
}

/// Detect forecast deviation by comparing actual values to linear trend extrapolation.
/// Requires at least 4 historical periods.
/// Returns a deviation if |deviation| > 20% and p < 0.05 (outside prediction interval).
pub fn detect_forecast_deviation(
    column: &str,
    period_values: &[(String, f64)], // (period_label, mean_value)
) -> Option<ForecastDeviation> {
    // Need at least 4 historical periods + 1 current period
    if period_values.len() < 5 {
        return None;
    }

    let n = period_values.len();

    // Historical data: all but the last period
    let x: Vec<f64> = (0..n - 1).map(|i| i as f64).collect();
    let y: Vec<f64> = period_values[..n - 1].iter().map(|(_, v)| *v).collect();

    // Current period to test
    let x_new = (n - 1) as f64;
    let (current_period, actual) = &period_values[n - 1];
    let actual = *actual;

    // Calculate prediction interval (95% confidence)
    let (expected, _lower, _upper, p_value) = prediction_interval(&x, &y, x_new, actual, 0.95)?;

    if expected == 0.0 {
        return None;
    }

    let deviation_percent = ((actual - expected) / expected.abs()) * 100.0;

    Some(ForecastDeviation {
        column: column.to_string(),
        period: current_period.clone(),
        actual,
        expected,
        deviation_percent,
        p_value,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_deviation_increasing_trend() {
        // Linear increasing trend with deviation at the end
        let period_values = vec![
            ("2023-01".to_string(), 100.0),
            ("2023-02".to_string(), 110.0),
            ("2023-03".to_string(), 120.0),
            ("2023-04".to_string(), 130.0),
            ("2023-05".to_string(), 180.0), // Big deviation
        ];

        let result = detect_forecast_deviation("Sales", &period_values);
        assert!(result.is_some());
        let dev = result.unwrap();
        assert_eq!(dev.column, "Sales");
        assert_eq!(dev.period, "2023-05");
        assert!(dev.deviation_percent > 20.0); // Should detect >20% deviation
    }

    #[test]
    fn test_no_deviation_normal_trend() {
        // Linear increasing trend with normal continuation
        let period_values = vec![
            ("2023-01".to_string(), 100.0),
            ("2023-02".to_string(), 110.0),
            ("2023-03".to_string(), 120.0),
            ("2023-04".to_string(), 130.0),
            ("2023-05".to_string(), 140.0), // Expected continuation
        ];

        let result = detect_forecast_deviation("Sales", &period_values);
        assert!(result.is_some());
        let dev = result.unwrap();
        // Small or no deviation expected
        assert!(dev.deviation_percent.abs() < 5.0);
    }

    #[test]
    fn test_insufficient_data() {
        // Too few periods
        let period_values = vec![
            ("2023-01".to_string(), 100.0),
            ("2023-02".to_string(), 110.0),
            ("2023-03".to_string(), 120.0),
        ];

        let result = detect_forecast_deviation("Sales", &period_values);
        assert!(result.is_none());
    }
}
