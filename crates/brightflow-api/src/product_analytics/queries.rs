//! Polars implementations of the product-analytics questions.
//!
//! All of these coalesce `user_id` and `visitor_id` into one identity column
//! first: an anonymous visitor who later identifies must count as the same person
//! on both sides of that boundary, or every funnel silently drops its converters.

use std::collections::HashMap;

use chrono::Datelike;
use polars::prelude::*;

use crate::ingest::error::IngestResult;
use crate::ingest::models::{
    EventListRow, FunnelResult, FunnelStepResult, RetentionResult, RetentionRow, UserTimelineEvent,
};

use crate::web_analytics::queries::filter_date_range;

/// Polars expression: coalesce user_id and visitor_id into a single identity column.
fn user_or_visitor() -> Expr {
    when(col("user_id").neq(lit("")))
        .then(col("user_id"))
        .otherwise(col("visitor_id"))
        .alias("effective_user")
}

/// Query the top events by count and unique users.
pub fn query_event_list(lf: LazyFrame, start: &str, end: &str) -> IngestResult<Vec<EventListRow>> {
    let lf = filter_date_range(lf, start, end);

    let result = lf
        .with_column(user_or_visitor())
        .group_by([col("event_name")])
        .agg([
            col("id").len().cast(DataType::UInt32).alias("count"),
            col("effective_user")
                .n_unique()
                .cast(DataType::UInt32)
                .alias("unique_users"),
        ])
        .sort(
            ["count"],
            SortMultipleOptions::default().with_order_descending(true),
        )
        .limit(50)
        .collect()?;

    let names = result
        .column("event_name")
        .ok()
        .and_then(|c| c.str().ok())
        .map(|c| c.into_iter().collect::<Vec<_>>())
        .unwrap_or_default();
    let counts = result
        .column("count")
        .ok()
        .and_then(|c| c.u32().ok())
        .map(|c| c.into_iter().collect::<Vec<_>>())
        .unwrap_or_default();
    let users = result
        .column("unique_users")
        .ok()
        .and_then(|c| c.u32().ok())
        .map(|c| c.into_iter().collect::<Vec<_>>())
        .unwrap_or_default();

    let rows: Vec<EventListRow> = names
        .into_iter()
        .zip(counts)
        .zip(users)
        .map(|((name, count), users)| EventListRow {
            name: name.unwrap_or("").to_string(),
            count: u64::from(count.unwrap_or(0)),
            unique_users: u64::from(users.unwrap_or(0)),
        })
        .collect();

    Ok(rows)
}

/// Funnel analysis: for each step, count how many users reached it within the time window.
pub fn query_funnel(
    lf: LazyFrame,
    start: &str,
    end: &str,
    steps: &[String],
    window_seconds: u64,
) -> IngestResult<FunnelResult> {
    if steps.is_empty() {
        return Ok(FunnelResult { steps: vec![] });
    }

    // Filter by date range and only events that appear in the funnel steps
    let lf = filter_date_range(lf, start, end);

    // Build an OR expression for event name matching
    let event_filter = steps.iter().fold(lit(false), |acc, name| {
        acc.or(col("event_name").eq(lit(name.as_str())))
    });

    let df = lf
        .with_column(user_or_visitor())
        .filter(event_filter)
        .select([col("effective_user"), col("event_name"), col("timestamp")])
        .sort(["timestamp"], SortMultipleOptions::default())
        .collect()?;

    // Collect events per user: HashMap<user, Vec<(event_name, timestamp_str)>>
    let users_col = df.column("effective_user").ok().and_then(|c| c.str().ok());
    let events_col = df.column("event_name").ok().and_then(|c| c.str().ok());
    let ts_col = df.column("timestamp").ok().and_then(|c| c.str().ok());

    let (Some(users_col), Some(events_col), Some(ts_col)) = (users_col, events_col, ts_col) else {
        return Ok(FunnelResult {
            steps: steps
                .iter()
                .map(|name| FunnelStepResult {
                    name: name.clone(),
                    count: 0,
                    conversion_rate: 0.0,
                    dropoff_rate: 0.0,
                })
                .collect(),
        });
    };

    let mut user_events: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for i in 0..df.height() {
        if let (Some(user), Some(event), Some(ts)) =
            (users_col.get(i), events_col.get(i), ts_col.get(i))
        {
            user_events
                .entry(user.to_string())
                .or_default()
                .push((event.to_string(), ts.to_string()));
        }
    }

    // For each user, greedily match funnel steps
    #[allow(clippy::cast_possible_wrap)]
    let window = chrono::Duration::seconds(window_seconds as i64);
    let mut step_counts = vec![0u64; steps.len()];

    for events in user_events.values() {
        let mut step_idx = 0;
        let mut step_start_time: Option<chrono::DateTime<chrono::FixedOffset>> = None;

        for (event_name, ts_str) in events {
            if step_idx >= steps.len() {
                break;
            }
            if *event_name != steps[step_idx] {
                continue;
            }

            let Ok(ts) = chrono::DateTime::parse_from_rfc3339(ts_str) else {
                continue;
            };

            if step_idx == 0 {
                step_start_time = Some(ts);
                step_counts[step_idx] += 1;
                step_idx += 1;
            } else if let Some(start_ts) = step_start_time {
                if ts.signed_duration_since(start_ts) <= window {
                    step_counts[step_idx] += 1;
                    step_idx += 1;
                }
                // If outside window, this user fails the funnel from here
            }
        }
    }

    // Build results
    let first_count = step_counts.first().copied().unwrap_or(0);
    let results: Vec<FunnelStepResult> = steps
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let count = step_counts[i];
            #[allow(clippy::cast_precision_loss)]
            let conversion_rate = if first_count > 0 {
                count as f64 / first_count as f64
            } else {
                0.0
            };
            #[allow(clippy::cast_precision_loss)]
            let dropoff_rate = if i == 0 {
                0.0
            } else {
                let prev = step_counts[i - 1];
                if prev > 0 {
                    (prev - count) as f64 / prev as f64
                } else {
                    0.0
                }
            };
            FunnelStepResult {
                name: name.clone(),
                count,
                conversion_rate,
                dropoff_rate,
            }
        })
        .collect();

    Ok(FunnelResult { steps: results })
}

/// Retention analysis: cohort users by when they first did `cohort_event`, then
/// measure how many returned to do `return_event` in subsequent periods.
pub fn query_retention(
    lf: LazyFrame,
    start: &str,
    end: &str,
    cohort_event: &str,
    return_event: &str,
    period_type: &str,
    num_periods: usize,
) -> IngestResult<RetentionResult> {
    let lf = filter_date_range(lf, start, end).with_column(user_or_visitor());

    // Step 1: Find first cohort_event per user → assign cohort period
    let cohort_df = lf
        .clone()
        .filter(col("event_name").eq(lit(cohort_event)))
        .group_by([col("effective_user")])
        .agg([col("timestamp").min().alias("first_ts")])
        .collect()?;

    if cohort_df.height() == 0 {
        return Ok(RetentionResult {
            period_type: period_type.to_string(),
            rows: vec![],
        });
    }

    // Build user → first_ts map and assign cohort periods
    let user_col = cohort_df
        .column("effective_user")
        .ok()
        .and_then(|c| c.str().ok());
    let first_ts_col = cohort_df.column("first_ts").ok().and_then(|c| c.str().ok());

    let (Some(user_col), Some(first_ts_col)) = (user_col, first_ts_col) else {
        return Ok(RetentionResult {
            period_type: period_type.to_string(),
            rows: vec![],
        });
    };

    let mut user_cohort: HashMap<String, String> = HashMap::new();
    for i in 0..cohort_df.height() {
        if let (Some(user), Some(ts)) = (user_col.get(i), first_ts_col.get(i)) {
            let cohort_key = ts_to_period_key(ts, period_type);
            user_cohort.insert(user.to_string(), cohort_key);
        }
    }

    // Step 2: Find all return_event occurrences per user
    let return_df = lf
        .filter(col("event_name").eq(lit(return_event)))
        .select([col("effective_user"), col("timestamp")])
        .collect()?;

    let ret_user_col = return_df
        .column("effective_user")
        .ok()
        .and_then(|c| c.str().ok());
    let ret_ts_col = return_df
        .column("timestamp")
        .ok()
        .and_then(|c| c.str().ok());

    // user → set of active period keys
    let mut user_active_periods: HashMap<String, std::collections::HashSet<String>> =
        HashMap::new();
    if let (Some(ret_user_col), Some(ret_ts_col)) = (ret_user_col, ret_ts_col) {
        for i in 0..return_df.height() {
            if let (Some(user), Some(ts)) = (ret_user_col.get(i), ret_ts_col.get(i)) {
                let period_key = ts_to_period_key(ts, period_type);
                user_active_periods
                    .entry(user.to_string())
                    .or_default()
                    .insert(period_key);
            }
        }
    }

    // Step 3: Build cohort matrix
    // cohort_period → Vec<user_id>
    let mut cohorts: HashMap<String, Vec<String>> = HashMap::new();
    for (user, cohort_key) in &user_cohort {
        cohorts
            .entry(cohort_key.clone())
            .or_default()
            .push(user.clone());
    }

    let mut sorted_cohorts: Vec<String> = cohorts.keys().cloned().collect();
    sorted_cohorts.sort();

    // Only take the last `num_periods` cohorts
    if sorted_cohorts.len() > num_periods {
        sorted_cohorts = sorted_cohorts[sorted_cohorts.len() - num_periods..].to_vec();
    }

    // For each cohort, compute retention percentages
    let all_period_keys = generate_period_keys(num_periods);

    let rows: Vec<RetentionRow> = sorted_cohorts
        .iter()
        .map(|cohort_key| {
            let users = cohorts.get(cohort_key).cloned().unwrap_or_default();
            let cohort_size = users.len() as u64;

            // Generate target period keys relative to cohort start
            let periods: Vec<f64> = (0..num_periods)
                .map(|offset| {
                    let target_key = offset_period_key(cohort_key, offset, period_type);
                    let active = users
                        .iter()
                        .filter(|u| {
                            user_active_periods
                                .get(*u)
                                .is_some_and(|ps| ps.contains(&target_key))
                        })
                        .count();
                    #[allow(clippy::cast_precision_loss)]
                    if cohort_size > 0 {
                        active as f64 / cohort_size as f64
                    } else {
                        0.0
                    }
                })
                .collect();

            RetentionRow {
                cohort: cohort_key.clone(),
                cohort_size,
                periods,
            }
        })
        .collect();

    // Drop unused variable
    drop(all_period_keys);

    Ok(RetentionResult {
        period_type: period_type.to_string(),
        rows,
    })
}

/// Query a user's event timeline.
pub fn query_user_timeline(lf: LazyFrame, user_id: &str) -> IngestResult<Vec<UserTimelineEvent>> {
    // Find events where user_id matches or visitor_id matches
    let df = lf
        .with_column(user_or_visitor())
        .filter(col("effective_user").eq(lit(user_id)))
        .select([
            col("timestamp"),
            col("event_name"),
            col("page_url"),
            col("properties"),
        ])
        .sort(
            ["timestamp"],
            SortMultipleOptions::default().with_order_descending(true),
        )
        .limit(200)
        .collect()?;

    let timestamps = df
        .column("timestamp")
        .ok()
        .and_then(|c| c.str().ok())
        .map(|c| c.into_iter().collect::<Vec<_>>())
        .unwrap_or_default();
    let event_names = df
        .column("event_name")
        .ok()
        .and_then(|c| c.str().ok())
        .map(|c| c.into_iter().collect::<Vec<_>>())
        .unwrap_or_default();
    let page_urls = df
        .column("page_url")
        .ok()
        .and_then(|c| c.str().ok())
        .map(|c| c.into_iter().collect::<Vec<_>>())
        .unwrap_or_default();
    let properties = df
        .column("properties")
        .ok()
        .and_then(|c| c.str().ok())
        .map(|c| c.into_iter().collect::<Vec<_>>())
        .unwrap_or_default();

    let events: Vec<UserTimelineEvent> = timestamps
        .into_iter()
        .zip(event_names)
        .zip(page_urls)
        .zip(properties)
        .map(|(((ts, name), url), props)| UserTimelineEvent {
            timestamp: ts.unwrap_or("").to_string(),
            event_name: name.unwrap_or("").to_string(),
            page_url: url.unwrap_or("").to_string(),
            properties: props.unwrap_or("{}").to_string(),
        })
        .collect();

    Ok(events)
}

// ── Period helpers ────────────────────────────────────────────────

/// Convert a timestamp string to a period key (e.g. "2026-W14" or "2026-04").
fn ts_to_period_key(ts: &str, period_type: &str) -> String {
    let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) else {
        // Fall back to date string prefix (whole string if too short or the
        // boundary is mid-codepoint)
        return ts.get(..10).unwrap_or(ts).to_string();
    };
    let date = dt.date_naive();
    match period_type {
        "week" => {
            let iso = date.iso_week();
            format!("{}-W{:02}", iso.year(), iso.week())
        },
        _ => format!("{}-{:02}", date.year(), date.month()),
    }
}

/// Offset a period key by N periods. Keys that don't parse as "YYYY-Www" /
/// "YYYY-MM" (including out-of-range months like "2026-99") are returned
/// unchanged, matching the malformed-shape policy of the week arm.
fn offset_period_key(base_key: &str, offset: usize, period_type: &str) -> String {
    let Ok(offset) = u32::try_from(offset) else {
        return base_key.to_string();
    };
    if period_type == "week" {
        let Some((year, week)) = base_key.split_once("-W") else {
            return base_key.to_string();
        };
        let (Ok(year), Ok(week)) = (year.parse::<i32>(), week.parse::<u32>()) else {
            return base_key.to_string();
        };
        let Some(date) = chrono::NaiveDate::from_isoywd_opt(year, week, chrono::Weekday::Mon)
        else {
            return base_key.to_string();
        };
        let target = date + chrono::Duration::weeks(i64::from(offset));
        let iso = target.iso_week();
        format!("{}-W{:02}", iso.year(), iso.week())
    } else {
        let Some((year, month)) = base_key.split_once('-') else {
            return base_key.to_string();
        };
        let (Ok(year), Ok(month)) = (year.parse::<i32>(), month.parse::<u32>()) else {
            return base_key.to_string();
        };
        let Some(target) = chrono::NaiveDate::from_ymd_opt(year, month, 1)
            .and_then(|d| d.checked_add_months(chrono::Months::new(offset)))
        else {
            return base_key.to_string();
        };
        format!("{}-{:02}", target.year(), target.month())
    }
}

/// Generate a sequence of period keys for column headers.
fn generate_period_keys(count: usize) -> Vec<String> {
    (0..count).map(|i| format!("P{i}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ts_to_period_key_month_and_week() {
        assert_eq!(ts_to_period_key("2026-04-15T12:00:00Z", "month"), "2026-04");
        assert_eq!(ts_to_period_key("2026-04-15T12:00:00Z", "week"), "2026-W16");
        // ISO week-year boundary: 2025-12-29 belongs to 2026-W01
        assert_eq!(ts_to_period_key("2025-12-29T00:00:00Z", "week"), "2026-W01");
    }

    #[test]
    fn ts_to_period_key_non_rfc3339_falls_back_to_prefix() {
        assert_eq!(
            ts_to_period_key("2026-04-15 12:00:00", "month"),
            "2026-04-15"
        );
        assert_eq!(ts_to_period_key("2026-04", "month"), "2026-04");
        assert_eq!(ts_to_period_key("", "month"), "");
    }

    #[test]
    fn offset_period_key_months() {
        assert_eq!(offset_period_key("2026-04", 2, "month"), "2026-06");
        assert_eq!(offset_period_key("2026-11", 3, "month"), "2027-02");
        assert_eq!(offset_period_key("2026-04", 0, "month"), "2026-04");
    }

    #[test]
    fn offset_period_key_weeks() {
        assert_eq!(offset_period_key("2026-W16", 2, "week"), "2026-W18");
        // Crosses into the next ISO week-year
        assert_eq!(offset_period_key("2026-W52", 2, "week"), "2027-W01");
    }

    #[test]
    fn offset_period_key_malformed_passthrough() {
        assert_eq!(offset_period_key("garbage", 3, "month"), "garbage");
        assert_eq!(offset_period_key("garbage", 3, "week"), "garbage");
        // Out-of-range month is passed through, not silently remapped
        assert_eq!(offset_period_key("2026-99", 1, "month"), "2026-99");
        assert_eq!(offset_period_key("20xx-04", 1, "month"), "20xx-04");
    }

    #[test]
    fn generate_period_keys_shape() {
        assert_eq!(generate_period_keys(3), vec!["P0", "P1", "P2"]);
        assert!(generate_period_keys(0).is_empty());
    }
}
