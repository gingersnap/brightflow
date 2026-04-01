use axum::extract::{Path, Query, State};
use axum::Json;

use brightflow_ingest::models::{BreakdownRow, DashboardStats, TimeseriesPoint};

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

fn get_events_path(_state: &AppState) -> std::path::PathBuf {
    brightflow_core::WorkspacePaths::from_env().events_store()
}

/// Run an analytics query in a blocking task with proper error logging.
async fn run_query<T: Send + 'static>(
    label: &str,
    f: impl FnOnce() -> brightflow_ingest::error::IngestResult<T> + Send + 'static,
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

/// GET /api/analytics/:source_id/stats
pub async fn stats(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
    Query(params): Query<AnalyticsParams>,
) -> AppResult<Json<DashboardStats>> {
    let events_path = get_events_path(&state);
    let (start, end) = resolve_dates(&params);

    let result = run_query("stats", move || {
        queries::query_stats(&events_path, &source_id, &start, &end)
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
    let events_path = get_events_path(&state);
    let (start, end) = resolve_dates(&params);

    let result = run_query("timeseries", move || {
        queries::query_timeseries(&events_path, &source_id, &start, &end)
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
    let events_path = get_events_path(&state);
    let (start, end) = resolve_dates(&params);

    let result = run_query("top-pages", move || {
        queries::query_breakdown(&events_path, &source_id, &start, &end, "pathname", 20)
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
    let events_path = get_events_path(&state);
    let (start, end) = resolve_dates(&params);

    let result = run_query("referrers", move || {
        queries::query_breakdown(
            &events_path,
            &source_id,
            &start,
            &end,
            "referrer_source",
            20,
        )
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
    let events_path = get_events_path(&state);
    let (start, end) = resolve_dates(&params);

    let result = run_query("utm", move || {
        queries::query_breakdown(&events_path, &source_id, &start, &end, "utm_source", 20)
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
    let events_path = get_events_path(&state);
    let (start, end) = resolve_dates(&params);

    let result = run_query("devices", move || {
        queries::query_breakdown(&events_path, &source_id, &start, &end, "browser", 20)
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
    let events_path = get_events_path(&state);
    let (start, end) = resolve_dates(&params);

    let result = run_query("geo", move || {
        queries::query_breakdown(&events_path, &source_id, &start, &end, "country", 20)
    })
    .await?;

    Ok(Json(result))
}
