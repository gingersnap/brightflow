use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;

use brightflow_ingest::models::{
    EventListRow, FunnelRequest, FunnelResult, FunnelStepResult, RetentionRequest, RetentionResult,
    UserProfile, UserTimelineEvent,
};
use brightflow_ingest::IngestState;

use crate::shared::{AppError, AppResult};
use crate::state::AppState;
use crate::web_analytics::handlers::AnalyticsParams;

use super::queries;

fn get_ingest(state: &AppState) -> AppResult<&Arc<IngestState>> {
    state
        .ingest
        .as_ref()
        .ok_or_else(|| AppError::Internal("Ingest engine not initialized".to_string()))
}

/// Resolve period to (start, end) date strings.
fn resolve_dates(period: &str, start: Option<&str>, end: Option<&str>) -> (String, String) {
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

/// Run a product analytics query in a blocking task.
async fn run_query<T: Send + 'static>(
    label: &str,
    f: impl FnOnce() -> brightflow_ingest::error::IngestResult<T> + Send + 'static,
) -> AppResult<T> {
    let label = label.to_string();
    match tokio::task::spawn_blocking(f).await {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(e)) => {
            tracing::error!("Product analytics {label} query error: {e}");
            Err(AppError::Internal(e.to_string()))
        },
        Err(e) => {
            tracing::error!("Product analytics {label} task error: {e}");
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
    let date_start = &start[..10.min(start.len())];
    let date_end = &end[..10.min(end.len())];
    let filters = vec![
        brightflow_store::ScanFilter::PartitionRange {
            key: "date".into(),
            min: Some(date_start.into()),
            max: Some(date_end.into()),
        },
        brightflow_store::ScanFilter::ColumnRange {
            column: "timestamp".into(),
            min: Some(start.into()),
            max: Some(end.into()),
        },
    ];
    let lf = store
        .scan_table(&table_name, &filters)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(lf.map(crate::web_analytics::queries::scan_events_from_store))
}

/// Scan all events for a source (no date filter — used for user timeline).
/// Returns None if the store isn't available.
async fn scan_source_events_all(
    state: &AppState,
    source_id: &str,
) -> AppResult<Option<polars::prelude::LazyFrame>> {
    let Some(store) = state.store() else {
        return Ok(None);
    };
    let table_name = format!("events_{source_id}");
    let lf = store
        .scan_table(&table_name, &[])
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(lf.map(crate::web_analytics::queries::scan_events_from_store))
}

/// GET /api/analytics/:source_id/events
pub async fn event_list(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
    Query(params): Query<AnalyticsParams>,
) -> AppResult<Json<Vec<EventListRow>>> {
    let (start, end) = resolve_dates(
        &params.period,
        params.start.as_deref(),
        params.end.as_deref(),
    );

    let lf = scan_source_events(&state, &source_id, &start, &end).await?;

    let result = run_query("events", move || {
        let Some(lf) = lf else {
            return Ok(vec![]);
        };
        queries::query_event_list(lf, &start, &end)
    })
    .await?;

    Ok(Json(result))
}

/// POST /api/analytics/:source_id/funnel
pub async fn funnel(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
    Json(req): Json<FunnelRequest>,
) -> AppResult<Json<FunnelResult>> {
    if req.steps.is_empty() {
        return Err(AppError::BadRequest(
            "At least one funnel step is required".to_string(),
        ));
    }

    let (start, end) = resolve_dates(&req.period, req.start.as_deref(), req.end.as_deref());
    let steps: Vec<String> = req.steps.into_iter().map(|s| s.name).collect();
    let window = req.window_seconds;

    let lf = scan_source_events(&state, &source_id, &start, &end).await?;

    let result = run_query("funnel", move || {
        let Some(lf) = lf else {
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
        queries::query_funnel(lf, &start, &end, &steps, window)
    })
    .await?;

    Ok(Json(result))
}

/// POST /api/analytics/:source_id/retention
pub async fn retention(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
    Json(req): Json<RetentionRequest>,
) -> AppResult<Json<RetentionResult>> {
    let (start, end) = resolve_dates(&req.period, req.start.as_deref(), req.end.as_deref());
    let cohort_event = req.cohort_event;
    let return_event = req.return_event;
    let period_type = req.period_type;
    let num_periods = req.num_periods;

    let lf = scan_source_events(&state, &source_id, &start, &end).await?;

    let result = run_query("retention", move || {
        let Some(lf) = lf else {
            return Ok(RetentionResult {
                period_type: period_type.clone(),
                rows: vec![],
            });
        };
        queries::query_retention(
            lf,
            &start,
            &end,
            &cohort_event,
            &return_event,
            &period_type,
            num_periods,
        )
    })
    .await?;

    Ok(Json(result))
}

/// Query params for user search.
#[derive(Debug, serde::Deserialize)]
pub struct UserSearchParams {
    #[serde(default)]
    pub q: String,
    #[serde(default = "default_user_limit")]
    pub limit: u32,
}

fn default_user_limit() -> u32 {
    20
}

/// GET /api/analytics/:source_id/users
pub async fn search_users(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
    Query(params): Query<UserSearchParams>,
) -> AppResult<Json<Vec<UserProfile>>> {
    let ingest = get_ingest(&state)?;
    let profiles = ingest
        .db
        .search_user_profiles(&source_id, &params.q, params.limit)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Json(profiles))
}

/// GET /api/analytics/:source_id/users/:user_id/timeline
pub async fn user_timeline(
    State(state): State<AppState>,
    Path((source_id, user_id)): Path<(String, String)>,
) -> AppResult<Json<Vec<UserTimelineEvent>>> {
    let lf = scan_source_events_all(&state, &source_id).await?;

    let result = run_query("user-timeline", move || {
        let Some(lf) = lf else {
            return Ok(vec![]);
        };
        queries::query_user_timeline(lf, &user_id)
    })
    .await?;

    Ok(Json(result))
}

/// GET /api/analytics/:source_id/users/:user_id/profile
pub async fn user_profile(
    State(state): State<AppState>,
    Path((source_id, user_id)): Path<(String, String)>,
) -> AppResult<Json<UserProfile>> {
    let ingest = get_ingest(&state)?;
    let profile = ingest
        .db
        .get_user_profile(&user_id, &source_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("User {user_id} not found")))?;
    Ok(Json(profile))
}
