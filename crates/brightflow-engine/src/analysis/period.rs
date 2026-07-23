use anyhow::Result;
use chrono::{Datelike, NaiveDate, NaiveDateTime};
use polars::prelude::*;

use crate::data::config::TimeGranularity;
use crate::stats::significance::{mean, p_value_welch_t_test, std_dev};

/// Public function to get period labels from a time column
pub fn get_period_labels(
    series: &Column,
    granularity: TimeGranularity,
) -> Result<Vec<Option<String>>> {
    extract_period_labels(series, granularity)
}

#[derive(Debug, Clone)]
pub struct PeriodComparisonResult {
    pub column: String,
    pub current_period: String,
    pub previous_period: String,
    pub current_value: f64,
    pub previous_value: f64,
    pub change_absolute: f64,
    pub change_percent: f64,
    pub p_value: f64,
}

#[derive(Debug, Clone)]
pub struct PeriodStats {
    pub period_label: String,
    pub count: usize,
    pub sum: f64,
    pub mean: f64,
    pub std_dev: f64,
}

/// Group by time periods using pre-cached data
pub fn aggregate_by_period_cached(
    metric_values: &[f64],
    period_labels: &[Option<String>],
) -> Vec<PeriodStats> {
    let mut period_data: std::collections::HashMap<String, Vec<f64>> =
        std::collections::HashMap::new();

    for (period, value) in period_labels.iter().zip(metric_values.iter()) {
        if let Some(p) = period {
            period_data.entry(p.clone()).or_default().push(*value);
        }
    }

    let mut stats: Vec<PeriodStats> = period_data
        .into_iter()
        .map(|(period_label, values)| {
            let sum: f64 = values.iter().sum();
            let m = mean(&values);
            let s = std_dev(&values);
            PeriodStats {
                period_label,
                count: values.len(),
                sum,
                mean: m,
                std_dev: s,
            }
        })
        .collect();

    stats.sort_by(|a, b| a.period_label.cmp(&b.period_label));
    stats
}

/// Group a DataFrame by time periods and compute aggregated stats for a metric
pub fn aggregate_by_period(
    df: &DataFrame,
    time_col: &str,
    metric_col: &str,
    granularity: TimeGranularity,
) -> Result<Vec<PeriodStats>> {
    let time_series = df.column(time_col)?;
    let metric_series = df.column(metric_col)?;

    // Convert metric to f64
    let metric_values: Vec<Option<f64>> = metric_series
        .cast(&DataType::Float64)?
        .f64()?
        .into_iter()
        .collect();

    // Parse time column and extract period labels
    let period_labels = extract_period_labels(time_series, granularity)?;

    // Group by period
    let mut period_data: std::collections::HashMap<String, Vec<f64>> =
        std::collections::HashMap::new();

    for (period, value) in period_labels.iter().zip(metric_values.iter()) {
        if let (Some(p), Some(v)) = (period, value) {
            period_data.entry(p.clone()).or_default().push(*v);
        }
    }

    // Convert to sorted stats
    let mut stats: Vec<PeriodStats> = period_data
        .into_iter()
        .map(|(period_label, values)| {
            let sum: f64 = values.iter().sum();
            let m = mean(&values);
            let s = std_dev(&values);
            PeriodStats {
                period_label,
                count: values.len(),
                sum,
                mean: m,
                std_dev: s,
            }
        })
        .collect();

    // Sort by period label (chronological)
    stats.sort_by(|a, b| a.period_label.cmp(&b.period_label));

    Ok(stats)
}

/// Compare the most recent period to the previous period (cached version)
pub fn compare_periods_cached(
    metric_col: &str,
    metric_values: &[f64],
    period_labels: &[Option<String>],
) -> Option<PeriodComparisonResult> {
    let stats = aggregate_by_period_cached(metric_values, period_labels);

    if stats.len() < 2 {
        return None;
    }

    let current = &stats[stats.len() - 1];
    let previous = &stats[stats.len() - 2];

    if previous.mean == 0.0 {
        return None;
    }

    let change_absolute = current.mean - previous.mean;
    let change_percent = (change_absolute / previous.mean) * 100.0;

    let p_value = p_value_welch_t_test(
        current.mean,
        current.std_dev,
        current.count,
        previous.mean,
        previous.std_dev,
        previous.count,
    );

    Some(PeriodComparisonResult {
        column: metric_col.to_string(),
        current_period: current.period_label.clone(),
        previous_period: previous.period_label.clone(),
        current_value: current.mean,
        previous_value: previous.mean,
        change_absolute,
        change_percent,
        p_value,
    })
}

/// Compare the most recent period to the previous period
pub fn compare_periods(
    df: &DataFrame,
    time_col: &str,
    metric_col: &str,
    granularity: TimeGranularity,
) -> Result<Option<PeriodComparisonResult>> {
    let stats = aggregate_by_period(df, time_col, metric_col, granularity)?;

    if stats.len() < 2 {
        return Ok(None);
    }

    let current = &stats[stats.len() - 1];
    let previous = &stats[stats.len() - 2];

    if previous.mean == 0.0 {
        return Ok(None);
    }

    let change_absolute = current.mean - previous.mean;
    let change_percent = (change_absolute / previous.mean) * 100.0;

    // Calculate p-value using Welch's t-test
    let p_value = p_value_welch_t_test(
        current.mean,
        current.std_dev,
        current.count,
        previous.mean,
        previous.std_dev,
        previous.count,
    );

    Ok(Some(PeriodComparisonResult {
        column: metric_col.to_string(),
        current_period: current.period_label.clone(),
        previous_period: previous.period_label.clone(),
        current_value: current.mean,
        previous_value: previous.mean,
        change_absolute,
        change_percent,
        p_value,
    }))
}

/// Find the period with the largest deviation (cached version)
pub fn find_anomalous_period_cached(
    metric_col: &str,
    metric_values: &[f64],
    period_labels: &[Option<String>],
) -> Option<PeriodComparisonResult> {
    let stats = aggregate_by_period_cached(metric_values, period_labels);

    if stats.len() < 3 {
        return None;
    }

    let all_means: Vec<f64> = stats.iter().map(|s| s.mean).collect();
    let overall_mean = mean(&all_means);
    let overall_std = std_dev(&all_means);

    if overall_std == 0.0 {
        return None;
    }

    let mut max_z = 0.0f64;
    let mut anomalous_idx = None;

    for (i, stat) in stats.iter().enumerate() {
        let z = ((stat.mean - overall_mean) / overall_std).abs();
        if z > max_z {
            max_z = z;
            anomalous_idx = Some(i);
        }
    }

    let idx = anomalous_idx?;
    let anomalous = &stats[idx];

    let other_means: Vec<f64> = stats
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != idx)
        .map(|(_, s)| s.mean)
        .collect();
    let other_mean = mean(&other_means);
    let other_std = std_dev(&other_means);

    let change_absolute = anomalous.mean - other_mean;
    let change_percent = if other_mean == 0.0 {
        0.0
    } else {
        (change_absolute / other_mean) * 100.0
    };

    let p_value = p_value_welch_t_test(
        anomalous.mean,
        anomalous.std_dev,
        anomalous.count,
        other_mean,
        other_std,
        other_means.len(),
    );

    Some(PeriodComparisonResult {
        column: metric_col.to_string(),
        current_period: anomalous.period_label.clone(),
        previous_period: "other periods".to_string(),
        current_value: anomalous.mean,
        previous_value: other_mean,
        change_absolute,
        change_percent,
        p_value,
    })
}

/// Find the period with the largest deviation from the overall mean
pub fn find_anomalous_period(
    df: &DataFrame,
    time_col: &str,
    metric_col: &str,
    granularity: TimeGranularity,
) -> Result<Option<PeriodComparisonResult>> {
    let stats = aggregate_by_period(df, time_col, metric_col, granularity)?;

    if stats.len() < 3 {
        return Ok(None);
    }

    // Calculate overall stats
    let all_means: Vec<f64> = stats.iter().map(|s| s.mean).collect();
    let overall_mean = mean(&all_means);
    let overall_std = std_dev(&all_means);

    if overall_std == 0.0 {
        return Ok(None);
    }

    // Find period with largest z-score
    let mut max_z = 0.0f64;
    let mut anomalous_idx = None;

    for (i, stat) in stats.iter().enumerate() {
        let z = ((stat.mean - overall_mean) / overall_std).abs();
        if z > max_z {
            max_z = z;
            anomalous_idx = Some(i);
        }
    }

    let Some(idx) = anomalous_idx else {
        return Ok(None);
    };
    let anomalous = &stats[idx];

    // Compare to all other periods
    let other_means: Vec<f64> = stats
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != idx)
        .map(|(_, s)| s.mean)
        .collect();
    let other_mean = mean(&other_means);
    let other_std = std_dev(&other_means);

    let change_absolute = anomalous.mean - other_mean;
    let change_percent = if other_mean == 0.0 {
        0.0
    } else {
        (change_absolute / other_mean) * 100.0
    };

    let p_value = p_value_welch_t_test(
        anomalous.mean,
        anomalous.std_dev,
        anomalous.count,
        other_mean,
        other_std,
        other_means.len(),
    );

    Ok(Some(PeriodComparisonResult {
        column: metric_col.to_string(),
        current_period: anomalous.period_label.clone(),
        previous_period: "other periods".to_string(),
        current_value: anomalous.mean,
        previous_value: other_mean,
        change_absolute,
        change_percent,
        p_value,
    }))
}

/// Extract period labels from a time series based on granularity
fn extract_period_labels(
    series: &Column,
    granularity: TimeGranularity,
) -> Result<Vec<Option<String>>> {
    let dtype = series.dtype();

    match dtype {
        DataType::Date => {
            let date_series = series.date()?;
            Ok(date_series
                .into_iter()
                .map(|opt_days| {
                    opt_days.and_then(|days| {
                        NaiveDate::from_num_days_from_ce_opt(days + 719163)
                            .map(|date| format_period(date, granularity))
                    })
                })
                .collect())
        },
        DataType::Datetime(unit, _) => {
            let dt_series = series.datetime()?;
            let divisor = time_unit_divisor(*unit);
            Ok(dt_series
                .into_iter()
                .map(|opt_ticks| {
                    opt_ticks.and_then(|ticks| {
                        let secs = ticks / divisor;
                        chrono::DateTime::from_timestamp(secs, 0).map(
                            |dt: chrono::DateTime<chrono::Utc>| {
                                format_period(dt.date_naive(), granularity)
                            },
                        )
                    })
                })
                .collect())
        },
        DataType::String => {
            // Try to parse string dates
            let str_series = series.str()?;
            Ok(str_series
                .into_iter()
                .map(|opt_s| {
                    opt_s.and_then(|s| {
                        parse_date_string(s).map(|date| format_period(date, granularity))
                    })
                })
                .collect())
        },
        DataType::Int64 | DataType::Float64 => {
            // Assume Unix timestamp (seconds or milliseconds)
            let values: Vec<Option<i64>> = if matches!(dtype, DataType::Float64) {
                series
                    .f64()?
                    .into_iter()
                    .map(|v| v.map(|f| f as i64))
                    .collect()
            } else {
                series.i64()?.into_iter().collect()
            };

            Ok(values
                .into_iter()
                .map(|opt_ts| {
                    opt_ts.and_then(|ts| {
                        // Detect if milliseconds (> year 2100 in seconds)
                        let secs = if ts > 4_102_444_800 { ts / 1000 } else { ts };
                        chrono::DateTime::from_timestamp(secs, 0).map(
                            |dt: chrono::DateTime<chrono::Utc>| {
                                format_period(dt.date_naive(), granularity)
                            },
                        )
                    })
                })
                .collect())
        },
        _ => Ok(vec![None; series.len()]),
    }
}

/// Ticks-per-second for a polars datetime unit. Assuming microseconds for
/// millisecond columns collapsed years of data into a single 1970 period.
fn time_unit_divisor(unit: TimeUnit) -> i64 {
    match unit {
        TimeUnit::Nanoseconds => 1_000_000_000,
        TimeUnit::Microseconds => 1_000_000,
        TimeUnit::Milliseconds => 1_000,
    }
}

fn format_period(date: NaiveDate, granularity: TimeGranularity) -> String {
    match granularity {
        TimeGranularity::Day => date.format("%Y-%m-%d").to_string(),
        TimeGranularity::Week => {
            let iso_week = date.iso_week();
            format!("{}-W{:02}", iso_week.year(), iso_week.week())
        },
        TimeGranularity::Month => date.format("%Y-%m").to_string(),
        TimeGranularity::Quarter => {
            let quarter = (date.month() - 1) / 3 + 1;
            format!("{}-Q{}", date.year(), quarter)
        },
        TimeGranularity::Year => date.format("%Y").to_string(),
    }
}

/// Extract Unix timestamps (in seconds) from a time series
pub fn extract_timestamps(series: &Column) -> Result<Vec<Option<i64>>> {
    let dtype = series.dtype();

    match dtype {
        DataType::Date => {
            let date_series = series.date()?;
            Ok(date_series
                .into_iter()
                .map(|opt_days| {
                    opt_days.and_then(|days| {
                        NaiveDate::from_num_days_from_ce_opt(days + 719_163).and_then(|date| {
                            date.and_hms_opt(0, 0, 0).map(|dt| dt.and_utc().timestamp())
                        })
                    })
                })
                .collect())
        },
        DataType::Datetime(unit, _) => {
            let dt_series = series.datetime()?;
            let divisor = time_unit_divisor(*unit);
            Ok(dt_series
                .into_iter()
                .map(|opt_ticks| opt_ticks.map(|ticks| ticks / divisor))
                .collect())
        },
        DataType::String => {
            let str_series = series.str()?;
            Ok(str_series
                .into_iter()
                .map(|opt_s| {
                    opt_s.and_then(|s| {
                        parse_date_string(s).and_then(|date| {
                            date.and_hms_opt(0, 0, 0).map(|dt| dt.and_utc().timestamp())
                        })
                    })
                })
                .collect())
        },
        DataType::Int64 | DataType::Float64 => {
            let values: Vec<Option<i64>> = if matches!(dtype, DataType::Float64) {
                series
                    .f64()?
                    .into_iter()
                    .map(|v| v.map(|f| f as i64))
                    .collect()
            } else {
                series.i64()?.into_iter().collect()
            };

            Ok(values
                .into_iter()
                .map(|opt_ts| {
                    opt_ts.map(|ts| {
                        // Detect if milliseconds (> year 2100 in seconds)
                        if ts > 4_102_444_800 {
                            ts / 1000
                        } else {
                            ts
                        }
                    })
                })
                .collect())
        },
        _ => Ok(vec![None; series.len()]),
    }
}

fn parse_date_string(s: &str) -> Option<NaiveDate> {
    // Try common date formats
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(s, "%Y/%m/%d"))
        .or_else(|_| NaiveDate::parse_from_str(s, "%d-%m-%Y"))
        .or_else(|_| NaiveDate::parse_from_str(s, "%d/%m/%Y"))
        .or_else(|_| NaiveDate::parse_from_str(s, "%m/%d/%Y"))
        .or_else(|_| NaiveDate::parse_from_str(s, "%m-%d-%Y"))
        .or_else(|_| {
            // Try datetime formats and extract date
            NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
                .map(|dt: NaiveDateTime| dt.date())
                .or_else(|_| {
                    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S")
                        .map(|dt: NaiveDateTime| dt.date())
                })
                .or_else(|_| {
                    // Handle ISO8601 with Z/timezone suffix
                    // e.g., "2024-03-15T14:30:00Z", "2024-03-15T14:30:00+05:30"
                    // Strip Z, then strip +/- timezone offset after T
                    let s_stripped = s.trim_end_matches('Z');
                    let s_clean = if let Some(t_pos) = s_stripped.find('T') {
                        let time_part = &s_stripped[t_pos + 1..];
                        // Strip timezone offset (+HH:MM or -HH:MM) from time portion
                        if let Some(plus_pos) = time_part.rfind('+') {
                            &s_stripped[..t_pos + 1 + plus_pos]
                        } else if let Some(minus_pos) = time_part.rfind('-') {
                            &s_stripped[..t_pos + 1 + minus_pos]
                        } else {
                            s_stripped
                        }
                    } else {
                        s_stripped
                    };
                    NaiveDateTime::parse_from_str(s_clean, "%Y-%m-%dT%H:%M:%S")
                        .map(|dt: NaiveDateTime| dt.date())
                })
                .or_else(|_| {
                    // Handle space-separated datetime with timezone suffix
                    let s_clean = s.split('+').next().unwrap_or(s);
                    NaiveDateTime::parse_from_str(s_clean, "%Y-%m-%d %H:%M:%S")
                        .map(|dt: NaiveDateTime| dt.date())
                })
        })
        .ok()
        .or_else(|| {
            // Handle US format with variable-width month/day: M/D/YYYY or MM/DD/YYYY
            let parts: Vec<&str> = s.split('/').collect();
            if parts.len() == 3 {
                let month: u32 = parts[0].parse().ok()?;
                let day: u32 = parts[1].parse().ok()?;
                let year: i32 = parts[2].parse().ok()?;
                return NaiveDate::from_ymd_opt(year, month, day);
            }
            None
        })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // ── parse_date_string ─────────────────────────────────────────────────

    #[test]
    fn parses_iso_dates_and_datetimes() {
        assert_eq!(
            parse_date_string("2024-03-15"),
            NaiveDate::from_ymd_opt(2024, 3, 15)
        );
        assert_eq!(
            parse_date_string("2024-03-15 14:30:00"),
            NaiveDate::from_ymd_opt(2024, 3, 15)
        );
        assert_eq!(
            parse_date_string("2024-03-15T14:30:00"),
            NaiveDate::from_ymd_opt(2024, 3, 15)
        );
    }

    #[test]
    fn parses_timezone_suffixes() {
        assert_eq!(
            parse_date_string("2024-03-15T14:30:00Z"),
            NaiveDate::from_ymd_opt(2024, 3, 15)
        );
        assert_eq!(
            parse_date_string("2024-03-15T14:30:00+05:30"),
            NaiveDate::from_ymd_opt(2024, 3, 15)
        );
        assert_eq!(
            parse_date_string("2024-03-15 14:30:00+02:00"),
            NaiveDate::from_ymd_opt(2024, 3, 15)
        );
    }

    /// Ambiguous slash dates parse DAY-FIRST: "%d/%m/%Y" is tried before
    /// "%m/%d/%Y", so 03/04/2023 is April 3rd, not March 4th. Documented
    /// behavior — US-format data with day ≤ 12 will be misread; unambiguous
    /// values (13/04/2023, 4/13/2023) land correctly either way.
    #[test]
    fn slash_dates_prefer_day_first() {
        assert_eq!(
            parse_date_string("03/04/2023"),
            NaiveDate::from_ymd_opt(2023, 4, 3)
        );
        // Day > 12 forces day-first unambiguously.
        assert_eq!(
            parse_date_string("13/04/2023"),
            NaiveDate::from_ymd_opt(2023, 4, 13)
        );
        // Month > 12 in slot one falls through to the US fallback (M/D/YYYY).
        assert_eq!(
            parse_date_string("4/13/2023"),
            NaiveDate::from_ymd_opt(2023, 4, 13)
        );
    }

    #[test]
    fn garbage_dates_parse_to_none() {
        assert_eq!(parse_date_string("not a date"), None);
        assert_eq!(parse_date_string(""), None);
        assert_eq!(parse_date_string("99/99/9999"), None);
    }

    // ── format_period ─────────────────────────────────────────────────────

    #[test]
    fn format_period_per_granularity() {
        let date = NaiveDate::from_ymd_opt(2023, 4, 3).unwrap();
        assert_eq!(format_period(date, TimeGranularity::Day), "2023-04-03");
        assert_eq!(format_period(date, TimeGranularity::Week), "2023-W14");
        assert_eq!(format_period(date, TimeGranularity::Month), "2023-04");
        assert_eq!(format_period(date, TimeGranularity::Quarter), "2023-Q2");
        assert_eq!(format_period(date, TimeGranularity::Year), "2023");
    }

    /// ISO-week edge: Jan 1st 2021 belongs to ISO week 53 of 2020.
    #[test]
    fn format_period_iso_week_year_boundary() {
        let date = NaiveDate::from_ymd_opt(2021, 1, 1).unwrap();
        assert_eq!(format_period(date, TimeGranularity::Week), "2020-W53");
    }

    // ── extract_period_labels over polars Columns ─────────────────────────

    #[test]
    fn extracts_labels_from_string_column() {
        let col = Column::new("d".into(), &["2024-01-05", "2024-02-10", "garbage"]);
        let labels = extract_period_labels(&col, TimeGranularity::Month).unwrap();
        assert_eq!(
            labels,
            vec![
                Some("2024-01".to_string()),
                Some("2024-02".to_string()),
                None
            ]
        );
    }

    /// The seconds/milliseconds threshold sits at 4_102_444_800 (year 2100):
    /// larger values are treated as milliseconds.
    #[test]
    fn extracts_labels_from_unix_timestamps_seconds_and_millis() {
        // 2024-03-15 00:00:00 UTC in seconds and milliseconds.
        let secs: i64 = 1_710_460_800;
        let col_secs = Column::new("t".into(), &[secs, secs]);
        let col_millis = Column::new("t".into(), &[secs * 1000, secs * 1000]);
        let from_secs = extract_period_labels(&col_secs, TimeGranularity::Day).unwrap();
        let from_millis = extract_period_labels(&col_millis, TimeGranularity::Day).unwrap();
        assert_eq!(from_secs, from_millis);
        assert_eq!(from_secs[0], Some("2024-03-15".to_string()));
    }

    #[test]
    fn unsupported_dtype_yields_all_none() {
        let col = Column::new("b".into(), &[true, false]);
        let labels = extract_period_labels(&col, TimeGranularity::Day).unwrap();
        assert_eq!(labels, vec![None, None]);
    }
}
