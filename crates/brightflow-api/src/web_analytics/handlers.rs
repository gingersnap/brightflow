//! HTTP handlers for the web-analytics dashboard.
//!
//! Auth posture: session-authenticated, read-only.

use axum::extract::{Path, Query, State};
use axum::Json;

use crate::analytics::events_scan::{run_query, scan_source_events, AnalyticsParams};
use crate::ingest::models::{BreakdownRow, DashboardStats, TimeseriesPoint};

use crate::shared::AppResult;
use crate::state::AppState;

use super::queries;

/// GET /api/analytics/:source_id/stats
pub async fn stats(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
    Query(params): Query<AnalyticsParams>,
) -> AppResult<Json<DashboardStats>> {
    let (start, end) = params.resolve_dates();

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
    let (start, end) = params.resolve_dates();

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
    let (start, end) = params.resolve_dates();

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
    let (start, end) = params.resolve_dates();

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
    let (start, end) = params.resolve_dates();

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
    let (start, end) = params.resolve_dates();

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
    let (start, end) = params.resolve_dates();

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
