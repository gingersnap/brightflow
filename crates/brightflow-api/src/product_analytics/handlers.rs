use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;

use brightflow_ingest::models::{
    EventListRow, FunnelRequest, FunnelResult, RetentionRequest, RetentionResult, UserProfile,
    UserTimelineEvent,
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

fn get_events_path(_state: &AppState) -> std::path::PathBuf {
    brightflow_core::WorkspacePaths::from_env().events_store()
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

/// GET /api/analytics/:source_id/events
pub async fn event_list(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
    Query(params): Query<AnalyticsParams>,
) -> AppResult<Json<Vec<EventListRow>>> {
    let events_path = get_events_path(&state);
    let (start, end) = resolve_dates(
        &params.period,
        params.start.as_deref(),
        params.end.as_deref(),
    );

    let result = run_query("events", move || {
        queries::query_event_list(&events_path, &source_id, &start, &end)
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

    let events_path = get_events_path(&state);
    let (start, end) = resolve_dates(&req.period, req.start.as_deref(), req.end.as_deref());
    let steps: Vec<String> = req.steps.into_iter().map(|s| s.name).collect();
    let window = req.window_seconds;

    let result = run_query("funnel", move || {
        queries::query_funnel(&events_path, &source_id, &start, &end, &steps, window)
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
    let events_path = get_events_path(&state);
    let (start, end) = resolve_dates(&req.period, req.start.as_deref(), req.end.as_deref());
    let cohort_event = req.cohort_event;
    let return_event = req.return_event;
    let period_type = req.period_type;
    let num_periods = req.num_periods;

    let result = run_query("retention", move || {
        queries::query_retention(
            &events_path,
            &source_id,
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
    let events_path = get_events_path(&state);

    let result = run_query("user-timeline", move || {
        queries::query_user_timeline(&events_path, &source_id, &user_id)
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
