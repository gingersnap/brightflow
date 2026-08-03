//! Shared plumbing for the web- and product-analytics handlers: period
//! resolution, store scans over a source's event table, and the blocking-task
//! wrapper.
//!
//! Both handler families query the same `events_{source_id}` tables with the
//! same date filters; keeping one copy here is what guarantees a "7d" period
//! or a partition-range filter means the same thing on a web dashboard and in
//! a funnel.

use polars::prelude::LazyFrame;
use serde::Deserialize;

use crate::shared::{AppError, AppResult};
use crate::state::AppState;
use brightflow_store::ScanFilter;

/// Common query params for period-scoped analytics endpoints.
#[derive(Debug, Deserialize)]
pub struct AnalyticsParams {
    /// Period preset: today | 7d | 30d | month | 12m
    #[serde(default = "default_period")]
    pub period: String,
    /// Custom start date (YYYY-MM-DD), overrides period
    pub start: Option<String>,
    /// Custom end date (YYYY-MM-DD), overrides period
    pub end: Option<String>,
}

fn default_period() -> String {
    "30d".to_string()
}

impl AnalyticsParams {
    /// Resolve to (start, end) datetime strings.
    pub fn resolve_dates(&self) -> (String, String) {
        resolve_dates(&self.period, self.start.as_deref(), self.end.as_deref())
    }
}

/// Resolve a period preset (or explicit bounds) to (start, end) datetime strings.
pub fn resolve_dates(period: &str, start: Option<&str>, end: Option<&str>) -> (String, String) {
    if let (Some(s), Some(e)) = (start, end) {
        return (s.to_string(), e.to_string());
    }

    let now = chrono::Utc::now();
    let end_str = now.format("%Y-%m-%dT23:59:59").to_string();

    let start_str = match period {
        "today" => now.format("%Y-%m-%dT00:00:00").to_string(),
        "7d" => (now - chrono::Duration::days(7))
            .format("%Y-%m-%dT00:00:00")
            .to_string(),
        "month" => now.format("%Y-%m-01T00:00:00").to_string(),
        "12m" => (now - chrono::Duration::days(365))
            .format("%Y-%m-%dT00:00:00")
            .to_string(),
        _ => (now - chrono::Duration::days(30))
            .format("%Y-%m-%dT00:00:00")
            .to_string(),
    };

    (start_str, end_str)
}

/// Build scan filters for a date range query.
fn date_filters(start: &str, end: &str) -> Vec<ScanFilter> {
    let date_start = &start[..10.min(start.len())];
    let date_end = &end[..10.min(end.len())];
    vec![
        ScanFilter::PartitionRange {
            key: "date".into(),
            min: Some(date_start.into()),
            max: Some(date_end.into()),
        },
        ScanFilter::ColumnRange {
            column: "timestamp".into(),
            min: Some(start.into()),
            max: Some(end.into()),
        },
    ]
}

/// Scan events from the store for a source with date filters.
/// Returns None if the store isn't available or the table doesn't exist.
pub async fn scan_source_events(
    state: &AppState,
    source_id: &str,
    start: &str,
    end: &str,
) -> AppResult<Option<LazyFrame>> {
    scan_with_filters(state, source_id, &date_filters(start, end)).await
}

/// Scan all events for a source (no date filter — used for user timeline).
/// Returns None if the store isn't available.
pub async fn scan_source_events_all(
    state: &AppState,
    source_id: &str,
) -> AppResult<Option<LazyFrame>> {
    scan_with_filters(state, source_id, &[]).await
}

async fn scan_with_filters(
    state: &AppState,
    source_id: &str,
    filters: &[ScanFilter],
) -> AppResult<Option<LazyFrame>> {
    let Some(store) = state.store() else {
        return Ok(None);
    };
    let store_source_id = format!("web:{source_id}");
    let table_name = format!("events_{source_id}");
    let lf = store
        .scan_table(&store_source_id, &table_name, filters)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(lf.map(crate::web_analytics::queries::scan_events_from_store))
}

/// Run an analytics query in a blocking task with proper error logging.
pub async fn run_query<T: Send + 'static>(
    label: &str,
    f: impl FnOnce() -> crate::ingest::error::IngestResult<T> + Send + 'static,
) -> AppResult<T> {
    let label = label.to_string();
    match tokio::task::spawn_blocking(f).await {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(e)) => {
            tracing::error!("Analytics {label} query error: {e}");
            Err(AppError::Internal(e.to_string()))
        },
        Err(e) => {
            tracing::error!("Analytics {label} task error: {e}");
            Err(AppError::Internal(e.to_string()))
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_bounds_override_period() {
        let (s, e) = resolve_dates("7d", Some("2026-01-01"), Some("2026-01-31"));
        assert_eq!(s, "2026-01-01");
        assert_eq!(e, "2026-01-31");
    }

    #[test]
    fn presets_produce_day_bounded_datetimes() {
        let (s, e) = resolve_dates("7d", None, None);
        assert!(s.ends_with("T00:00:00"), "start: {s}");
        assert!(e.ends_with("T23:59:59"), "end: {e}");
    }

    #[test]
    fn unknown_period_falls_back_to_30d() {
        // Same shape as the known presets — the fallback must not panic or
        // produce a different format.
        let (s, _) = resolve_dates("bogus", None, None);
        assert!(s.ends_with("T00:00:00"));
    }
}
