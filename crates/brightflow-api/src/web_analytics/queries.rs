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
