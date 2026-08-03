//! HTTP handlers for funnels, retention, and user timelines.
//!
//! Auth posture: session-authenticated, read-only.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;

use crate::ingest::models::{
    EventListRow, FunnelRequest, FunnelResult, FunnelStepResult, RetentionRequest, RetentionResult,
    UserProfile, UserTimelineEvent,
};
use crate::ingest::IngestState;

use crate::analytics::events_scan::{
    resolve_dates, run_query, scan_source_events, scan_source_events_all, AnalyticsParams,
};
use crate::shared::{AppError, AppResult};
use crate::state::AppState;

use super::queries;

fn get_ingest(state: &AppState) -> AppResult<&Arc<IngestState>> {
    state
        .ingest
        .as_ref()
        .ok_or_else(|| AppError::Internal("Ingest engine not initialized".to_string()))
}

/// GET /api/analytics/:source_id/events
pub async fn event_list(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
    Query(params): Query<AnalyticsParams>,
) -> AppResult<Json<Vec<EventListRow>>> {
    let (start, end) = params.resolve_dates();

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
