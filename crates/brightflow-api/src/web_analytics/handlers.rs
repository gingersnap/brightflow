use axum::extract::{Path, Query, State};
use axum::Json;

use crate::ingest::models::{BreakdownRow, DashboardStats, TimeseriesPoint};
use brightflow_store::ScanFilter;

use crate::shared::{AppError, AppResult};
use crate::state::AppState;

use super::queries;

/// Query parameters for analytics endpoints.
#[derive(Debug, serde::Deserialize)]
pub struct AnalyticsParams {
    /// Period shorthand: "7d", "30d", "month", "12m"
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

/// Resolve period to (start, end) date strings.
fn resolve_dates(params: &AnalyticsParams) -> (String, String) {
    if let (Some(start), Some(end)) = (&params.start, &params.end) {
        return (start.clone(), end.clone());
    }

    let now = chrono::Utc::now();
    let end = now.format("%Y-%m-%dT23:59:59").to_string();

    let start = match params.period.as_str() {
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

    (start, end)
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

/// Run an analytics query in a blocking task with proper error logging.
async fn run_query<T: Send + 'static>(
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

/// Scan events from the store for a source with date filters.
/// Returns None if the store isn't available or the table doesn't exist.
async fn scan_source_events(
    state: &AppState,
    source_id: &str,
    start: &str,
    end: &str,
) -> AppResult<Option<polars::prelude::LazyFrame>> {
    let Some(store) = state.store() else {
        return Ok(None);
    };
    let table_name = format!("events_{source_id}");
    let filters = date_filters(start, end);
    let lf = store
        .scan_table(&table_name, &filters)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(lf.map(queries::scan_events_from_store))
}

/// GET /api/analytics/:source_id/stats
pub async fn stats(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
    Query(params): Query<AnalyticsParams>,
) -> AppResult<Json<DashboardStats>> {
    let (start, end) = resolve_dates(&params);

    let lf = scan_source_events(&state, &source_id, &start, &end).await?;

    let result = run_query("stats", move || {
        let Some(lf) = lf else {
            return Ok(DashboardStats {
                visitors: 0,
                pageviews: 0,
                bounce_rate: 0.0,
                avg_visit_duration: 0.0,
                prev_visitors: None,
                prev_pageviews: None,
            });
        };
        queries::query_stats(lf, &start, &end)
    })
    .await?;

    Ok(Json(result))
}

/// GET /api/analytics/:source_id/timeseries
pub async fn timeseries(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
    Query(params): Query<AnalyticsParams>,
) -> AppResult<Json<Vec<TimeseriesPoint>>> {
    let (start, end) = resolve_dates(&params);

    let lf = scan_source_events(&state, &source_id, &start, &end).await?;

    let result = run_query("timeseries", move || {
        let Some(lf) = lf else {
            return Ok(vec![]);
        };
        queries::query_timeseries(lf, &start, &end)
    })
    .await?;

    Ok(Json(result))
}

/// GET /api/analytics/:source_id/top-pages
pub async fn top_pages(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
    Query(params): Query<AnalyticsParams>,
) -> AppResult<Json<Vec<BreakdownRow>>> {
    let (start, end) = resolve_dates(&params);

    let lf = scan_source_events(&state, &source_id, &start, &end).await?;

    let result = run_query("top-pages", move || {
        let Some(lf) = lf else {
            return Ok(vec![]);
        };
        queries::query_breakdown(lf, &start, &end, "pathname", 20)
    })
    .await?;

    Ok(Json(result))
}

/// GET /api/analytics/:source_id/referrers
pub async fn referrers(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
    Query(params): Query<AnalyticsParams>,
) -> AppResult<Json<Vec<BreakdownRow>>> {
    let (start, end) = resolve_dates(&params);

    let lf = scan_source_events(&state, &source_id, &start, &end).await?;

    let result = run_query("referrers", move || {
        let Some(lf) = lf else {
            return Ok(vec![]);
        };
        queries::query_breakdown(lf, &start, &end, "referrer_source", 20)
    })
    .await?;

    Ok(Json(result))
}

/// GET /api/analytics/:source_id/utm
pub async fn utm(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
    Query(params): Query<AnalyticsParams>,
) -> AppResult<Json<Vec<BreakdownRow>>> {
    let (start, end) = resolve_dates(&params);

    let lf = scan_source_events(&state, &source_id, &start, &end).await?;

    let result = run_query("utm", move || {
        let Some(lf) = lf else {
            return Ok(vec![]);
        };
        queries::query_breakdown(lf, &start, &end, "utm_source", 20)
    })
    .await?;

    Ok(Json(result))
}

/// GET /api/analytics/:source_id/devices
pub async fn devices(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
    Query(params): Query<AnalyticsParams>,
) -> AppResult<Json<Vec<BreakdownRow>>> {
    let (start, end) = resolve_dates(&params);

    let lf = scan_source_events(&state, &source_id, &start, &end).await?;

    let result = run_query("devices", move || {
        let Some(lf) = lf else {
            return Ok(vec![]);
        };
        queries::query_breakdown(lf, &start, &end, "browser", 20)
    })
    .await?;

    Ok(Json(result))
}

/// GET /api/analytics/:source_id/geo
pub async fn geo(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
    Query(params): Query<AnalyticsParams>,
) -> AppResult<Json<Vec<BreakdownRow>>> {
    let (start, end) = resolve_dates(&params);

    let lf = scan_source_events(&state, &source_id, &start, &end).await?;

    let result = run_query("geo", move || {
        let Some(lf) = lf else {
            return Ok(vec![]);
        };
        queries::query_breakdown(lf, &start, &end, "country", 20)
    })
    .await?;

    Ok(Json(result))
}
