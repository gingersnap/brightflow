//! Polars implementations of the web-analytics aggregations.
//!
//! Carries a backward-compatibility shim: Parquet files written before `user_id`
//! existed lack the column entirely, so scans add or fill it rather than failing.
//! Dropping that shim means those files stop being readable.

use polars::prelude::*;

use crate::ingest::error::IngestResult;
use crate::ingest::models::{BreakdownRow, DashboardStats, TimeseriesPoint};

/// Apply user_id backward-compat fix to a store-backed LazyFrame.
///
/// Phase 1 Parquet files lack user_id entirely. Check schema and
/// either fill nulls (column exists but has nulls) or add the column.
pub fn scan_events_from_store(mut lf: LazyFrame) -> LazyFrame {
    let has_user_id = lf
        .collect_schema()
        .ok()
        .is_some_and(|s| s.contains("user_id"));
    if has_user_id {
        lf.with_column(col("user_id").fill_null(lit("")))
    } else {
        lf.with_column(lit("").alias("user_id"))
    }
}

/// Apply date range filter to a LazyFrame.
pub fn filter_date_range(lf: LazyFrame, start: &str, end: &str) -> LazyFrame {
    lf.filter(
        col("timestamp")
            .gt_eq(lit(start.to_string()))
            .and(col("timestamp").lt(lit(end.to_string()))),
    )
}

/// Query summary stats: visitors, pageviews, bounce rate, avg visit duration.
pub fn query_stats(lf: LazyFrame, start: &str, end: &str) -> IngestResult<DashboardStats> {
    let lf = filter_date_range(lf, start, end);

    // Collect the filtered data, selecting only the columns we need
    let df = lf
        .select([col("visitor_id"), col("session_id"), col("event_name")])
        .collect()?;

    let pageviews = df.height();

    // Count unique visitors
    let visitors = df
        .column("visitor_id")
        .ok()
        .and_then(|c| c.n_unique().ok())
        .unwrap_or(0);

    // Count unique sessions
    let sessions = df
        .column("session_id")
        .ok()
        .and_then(|c| c.n_unique().ok())
        .unwrap_or(1);

    // Bounce rate: count sessions with only 1 pageview
    let pageview_df = df
        .lazy()
        .filter(col("event_name").eq(lit("pageview")))
        .collect()
        .unwrap_or_default();

    // Count pageviews per session manually
    let mut session_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    if let Ok(session_col) = pageview_df.column("session_id") {
        if let Ok(str_col) = session_col.str() {
            for val in str_col.iter().flatten() {
                *session_counts.entry(val.to_string()).or_default() += 1;
            }
        }
    }
    let bounces = session_counts.values().filter(|&&c| c == 1).count();

    #[allow(clippy::cast_precision_loss)]
    let bounce_rate = if sessions > 0 {
        bounces as f64 / sessions as f64
    } else {
        0.0
    };

    Ok(DashboardStats {
        visitors: visitors as u64,
        pageviews: pageviews as u64,
        bounce_rate,
        avg_visit_duration: 0.0,
        prev_visitors: None,
        prev_pageviews: None,
    })
}

/// Query time series: visitors and pageviews per day.
pub fn query_timeseries(
    lf: LazyFrame,
    start: &str,
    end: &str,
) -> IngestResult<Vec<TimeseriesPoint>> {
    let lf = filter_date_range(lf, start, end);

    let result = lf
        .with_column(col("timestamp").str().slice(lit(0), lit(10)).alias("date"))
        .group_by([col("date")])
        .agg([
            col("visitor_id")
                .n_unique()
                .cast(DataType::UInt32)
                .alias("visitors"),
            col("id").len().cast(DataType::UInt32).alias("pageviews"),
        ])
        .sort(["date"], SortMultipleOptions::default())
        .collect()?;

    let dates = result
        .column("date")
        .ok()
        .and_then(|c| c.str().ok())
        .map(|c| c.into_iter().collect::<Vec<_>>())
        .unwrap_or_default();
    let visitors = result
        .column("visitors")
        .ok()
        .and_then(|c| c.u32().ok())
        .map(|c| c.into_iter().collect::<Vec<_>>())
        .unwrap_or_default();
    let pageviews = result
        .column("pageviews")
        .ok()
        .and_then(|c| c.u32().ok())
        .map(|c| c.into_iter().collect::<Vec<_>>())
        .unwrap_or_default();

    let points: Vec<TimeseriesPoint> = dates
        .into_iter()
        .zip(visitors)
        .zip(pageviews)
        .map(|((date, visitors), pageviews)| TimeseriesPoint {
            date: date.unwrap_or("").to_string(),
            visitors: u64::from(visitors.unwrap_or(0)),
            pageviews: u64::from(pageviews.unwrap_or(0)),
        })
        .collect();

    Ok(points)
}

/// Query a breakdown by a dimension column (top pages, referrers, etc.).
pub fn query_breakdown(
    lf: LazyFrame,
    start: &str,
    end: &str,
    dimension: &str,
    limit: u32,
) -> IngestResult<Vec<BreakdownRow>> {
    let lf = filter_date_range(lf, start, end);

    let result = lf
        .group_by([col(dimension)])
        .agg([
            col("visitor_id")
                .n_unique()
                .cast(DataType::UInt32)
                .alias("visitors"),
            col("id").len().cast(DataType::UInt32).alias("pageviews"),
        ])
        .sort(
            ["visitors"],
            SortMultipleOptions::default().with_order_descending(true),
        )
        .limit(limit)
        .collect()?;

    let names = result
        .column(dimension)
        .ok()
        .and_then(|c| c.str().ok())
        .map(|c| c.into_iter().collect::<Vec<_>>())
        .unwrap_or_default();
    let visitors = result
        .column("visitors")
        .ok()
        .and_then(|c| c.u32().ok())
        .map(|c| c.into_iter().collect::<Vec<_>>())
        .unwrap_or_default();
    let pageviews = result
        .column("pageviews")
        .ok()
        .and_then(|c| c.u32().ok())
        .map(|c| c.into_iter().collect::<Vec<_>>())
        .unwrap_or_default();

    let rows: Vec<BreakdownRow> = names
        .into_iter()
        .zip(visitors)
        .zip(pageviews)
        .map(|((name, visitors), pageviews)| BreakdownRow {
            name: name.unwrap_or("").to_string(),
            visitors: u64::from(visitors.unwrap_or(0)),
            pageviews: u64::from(pageviews.unwrap_or(0)),
        })
        .collect();

    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use polars::df;

    /// Four events over two days: session s1 has two pageviews, s2 has one
    /// pageview (a bounce), s3 has only a custom event.
    fn events() -> DataFrame {
        df!(
            "id" => &["e1", "e2", "e3", "e4"],
            "timestamp" => &[
                "2026-01-01T10:00:00Z",
                "2026-01-01T11:00:00Z",
                "2026-01-02T09:00:00Z",
                "2026-01-02T10:00:00Z",
            ],
            "visitor_id" => &["v1", "v1", "v2", "v3"],
            "session_id" => &["s1", "s1", "s2", "s3"],
            "event_name" => &["pageview", "pageview", "pageview", "click"],
            "page_url" => &["/home", "/about", "/home", "/home"],
        )
        .unwrap()
    }

    /// Zero rows but the full event schema, so every query still resolves
    /// its columns.
    fn no_events() -> DataFrame {
        df!(
            "id" => Vec::<String>::new(),
            "timestamp" => Vec::<String>::new(),
            "visitor_id" => Vec::<String>::new(),
            "session_id" => Vec::<String>::new(),
            "event_name" => Vec::<String>::new(),
            "page_url" => Vec::<String>::new(),
        )
        .unwrap()
    }

    #[test]
    fn scan_events_fills_null_user_id() {
        let df = df!(
            "id" => &["e1", "e2"],
            "user_id" => &[Some("u1"), None],
        )
        .unwrap();
        let out = scan_events_from_store(df.lazy()).collect().unwrap();
        let user_id: Vec<Option<&str>> = out
            .column("user_id")
            .unwrap()
            .str()
            .unwrap()
            .into_iter()
            .collect();
        assert_eq!(user_id, vec![Some("u1"), Some("")]);
    }

    #[test]
    fn scan_events_adds_missing_user_id() {
        let df = df!("id" => &["e1", "e2"]).unwrap();
        let out = scan_events_from_store(df.lazy()).collect().unwrap();
        let user_id: Vec<Option<&str>> = out
            .column("user_id")
            .unwrap()
            .str()
            .unwrap()
            .into_iter()
            .collect();
        assert_eq!(user_id, vec![Some(""), Some("")]);
    }

    #[test]
    fn filter_date_range_start_inclusive_end_exclusive() {
        // Start equals e1's timestamp (kept); end equals e4's (dropped).
        let df = filter_date_range(
            events().lazy(),
            "2026-01-01T10:00:00Z",
            "2026-01-02T10:00:00Z",
        )
        .collect()
        .unwrap();
        let ids: Vec<Option<&str>> = df
            .column("id")
            .unwrap()
            .str()
            .unwrap()
            .into_iter()
            .collect();
        assert_eq!(ids, vec![Some("e1"), Some("e2"), Some("e3")]);
    }

    #[test]
    fn query_stats_aggregates_and_bounce_rate() {
        let stats = query_stats(events().lazy(), "2026-01-01", "2026-01-03").unwrap();
        assert_eq!(stats.visitors, 3);
        // `pageviews` counts every event in range, custom events included
        // (e4 is a click).
        assert_eq!(stats.pageviews, 4);
        // Bounce counting, by contrast, looks only at pageview events:
        // s2 is the sole single-pageview session out of 3 total sessions.
        assert!((stats.bounce_rate - 1.0 / 3.0).abs() < 1e-12);
        assert_eq!(stats.prev_visitors, None);
        assert_eq!(stats.prev_pageviews, None);
    }

    #[test]
    fn query_stats_empty_frame_is_all_zero() {
        let stats = query_stats(no_events().lazy(), "2026-01-01", "2026-01-03").unwrap();
        assert_eq!(stats.visitors, 0);
        assert_eq!(stats.pageviews, 0);
        assert!(stats.bounce_rate.abs() < f64::EPSILON);
    }

    #[test]
    fn query_stats_out_of_range_rows_are_excluded() {
        // Range covering only day one: e3/e4 fall away.
        let stats = query_stats(events().lazy(), "2026-01-01", "2026-01-02").unwrap();
        assert_eq!(stats.visitors, 1);
        assert_eq!(stats.pageviews, 2);
    }

    #[test]
    fn query_timeseries_groups_by_day_sorted() {
        let points = query_timeseries(events().lazy(), "2026-01-01", "2026-01-03").unwrap();
        assert_eq!(points.len(), 2);
        assert_eq!(points[0].date, "2026-01-01");
        assert_eq!(points[0].visitors, 1);
        assert_eq!(points[0].pageviews, 2);
        assert_eq!(points[1].date, "2026-01-02");
        assert_eq!(points[1].visitors, 2);
        // Non-pageview events count here too (e4 is a click).
        assert_eq!(points[1].pageviews, 2);
    }

    #[test]
    fn query_timeseries_empty_frame() {
        let points = query_timeseries(no_events().lazy(), "2026-01-01", "2026-01-03").unwrap();
        assert!(points.is_empty());
    }

    #[test]
    fn query_breakdown_sorts_desc_and_limits() {
        let rows =
            query_breakdown(events().lazy(), "2026-01-01", "2026-01-03", "page_url", 10).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, "/home");
        assert_eq!(rows[0].visitors, 3);
        assert_eq!(rows[0].pageviews, 3);
        assert_eq!(rows[1].name, "/about");
        assert_eq!(rows[1].visitors, 1);
        assert_eq!(rows[1].pageviews, 1);

        let limited =
            query_breakdown(events().lazy(), "2026-01-01", "2026-01-03", "page_url", 1).unwrap();
        assert_eq!(limited.len(), 1);
        assert_eq!(limited[0].name, "/home");
    }

    #[test]
    fn query_breakdown_empty_frame() {
        let rows = query_breakdown(
            no_events().lazy(),
            "2026-01-01",
            "2026-01-03",
            "page_url",
            5,
        )
        .unwrap();
        assert!(rows.is_empty());
    }
}
