use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;

use brightflow_ingest::models::{
    CreateSourceRequest, RawEvent, RawIdentifyEvent, RawTrackEvent, Source, UpdateSourceRequest,
};
use brightflow_ingest::script::TRACKING_SCRIPT;
use brightflow_ingest::IngestState;

use crate::shared::{AppError, AppResult};
use crate::state::AppState;

// ── Event Ingestion (public, no auth) ──────────────────────────────

/// POST /api/event (and /api/collect alias)
///
/// Receives events from the tracking script. Public endpoint — no auth required.
/// Accepts `Content-Type: text/plain` or `application/json` (both parsed as JSON
/// to avoid CORS preflight on the tracking script's sendBeacon calls).
pub async fn ingest_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: String,
) -> AppResult<StatusCode> {
    let ingest = get_ingest(&state)?;

    // Parse body as JSON (sendBeacon sends as text/plain)
    let raw: RawEvent = serde_json::from_str(&body)
        .map_err(|e| AppError::BadRequest(format!("Invalid JSON: {e}")))?;

    // Look up source by domain
    let source = ingest
        .get_source_by_domain(&raw.domain)
        .ok_or_else(|| AppError::BadRequest(format!("Unknown domain: {}", raw.domain)))?;

    // Get today's salt
    let salt = ingest
        .get_today_salt()
        .await
        .map_err(|e| AppError::Internal(format!("Salt error: {e}")))?;

    // Extract IP from headers (X-Forwarded-For takes precedence)
    let ip = extract_ip(&headers);

    // Extract User-Agent
    let ua = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    // Process event: hash visitor ID, parse UA, lookup geo, parse URL/UTMs
    let mut event = brightflow_ingest::ingest::process_event(
        &raw,
        &ip,
        ua,
        &source.id,
        &salt,
        ingest.geo_reader.as_ref(),
        &ingest.ua_parser,
    );

    // Insert into per-source buffer (also derives session_id)
    ingest
        .buffer
        .insert(&mut event)
        .await
        .map_err(|e| AppError::Internal(format!("Buffer error: {e}")))?;

    Ok(StatusCode::ACCEPTED)
}

/// POST /api/track
///
/// Receives product analytics events with explicit user_id support.
/// Public endpoint — no auth required.
pub async fn track_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: String,
) -> AppResult<StatusCode> {
    let ingest = get_ingest(&state)?;

    let raw: RawTrackEvent = serde_json::from_str(&body)
        .map_err(|e| AppError::BadRequest(format!("Invalid JSON: {e}")))?;

    let source = ingest
        .get_source_by_domain(&raw.domain)
        .ok_or_else(|| AppError::BadRequest(format!("Unknown domain: {}", raw.domain)))?;

    let salt = ingest
        .get_today_salt()
        .await
        .map_err(|e| AppError::Internal(format!("Salt error: {e}")))?;

    let ip = extract_ip(&headers);
    let ua = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let mut event = brightflow_ingest::ingest::process_track_event(
        &raw,
        &ip,
        ua,
        &source.id,
        &salt,
        ingest.geo_reader.as_ref(),
        &ingest.ua_parser,
    );

    ingest
        .buffer
        .insert(&mut event)
        .await
        .map_err(|e| AppError::Internal(format!("Buffer error: {e}")))?;

    Ok(StatusCode::ACCEPTED)
}

/// POST /api/identify
///
/// Associates traits with a user_id. Public endpoint — called from tracking script.
pub async fn identify_user(State(state): State<AppState>, body: String) -> AppResult<StatusCode> {
    let ingest = get_ingest(&state)?;

    let raw: RawIdentifyEvent = serde_json::from_str(&body)
        .map_err(|e| AppError::BadRequest(format!("Invalid JSON: {e}")))?;

    if raw.user_id.is_empty() {
        return Err(AppError::BadRequest("user_id is required".to_string()));
    }

    let source = ingest
        .get_source_by_domain(&raw.domain)
        .ok_or_else(|| AppError::BadRequest(format!("Unknown domain: {}", raw.domain)))?;

    ingest
        .db
        .upsert_user_profile(&raw.user_id, &source.id, &raw.traits)
        .await
        .map_err(|e| AppError::Internal(format!("Profile error: {e}")))?;

    Ok(StatusCode::ACCEPTED)
}

/// GET /api/script.js
///
/// Serves the tracking script. The script reads `data-domain` from the
/// `<script>` tag that loaded it and derives the API endpoint automatically.
pub async fn serve_script() -> Response {
    (
        StatusCode::OK,
        [
            ("content-type", "application/javascript; charset=utf-8"),
            ("cache-control", "public, max-age=86400"),
        ],
        TRACKING_SCRIPT,
    )
        .into_response()
}

// ── Source Management (protected, auth required) ───────────────────

/// GET /api/sources
pub async fn list_sources(State(state): State<AppState>) -> AppResult<Json<Vec<Source>>> {
    let ingest = get_ingest(&state)?;
    let sources = ingest
        .db
        .list_sources()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Json(sources))
}

/// POST /api/sources
pub async fn create_source(
    State(state): State<AppState>,
    Json(req): Json<CreateSourceRequest>,
) -> AppResult<Json<Source>> {
    let ingest = get_ingest(&state)?;
    let source = ingest
        .db
        .create_source(&req)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // Update in-memory cache
    ingest
        .source_cache
        .insert(source.domain.clone(), source.clone());

    Ok(Json(source))
}

/// GET /api/sources/:id
pub async fn get_source(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Source>> {
    let ingest = get_ingest(&state)?;
    let source = ingest
        .db
        .get_source(&id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Json(source))
}

/// PUT /api/sources/:id
pub async fn update_source(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSourceRequest>,
) -> AppResult<Json<Source>> {
    let ingest = get_ingest(&state)?;
    let source = ingest
        .db
        .update_source(&id, &req)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // Update cache
    ingest
        .source_cache
        .insert(source.domain.clone(), source.clone());

    Ok(Json(source))
}

/// DELETE /api/sources/:id
pub async fn delete_source(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    let ingest = get_ingest(&state)?;

    // Get source for cache removal
    let source = ingest
        .db
        .get_source(&id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // Delete from DB
    ingest
        .db
        .delete_source(&id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // Remove from cache
    ingest.source_cache.remove(&source.domain);

    // Delete buffer database
    ingest
        .buffer
        .delete_source(&id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/sources/:id/snippet
pub async fn get_snippet(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> AppResult<Json<serde_json::Value>> {
    let ingest = get_ingest(&state)?;
    let source = ingest
        .db
        .get_source(&id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // Derive the script URL from the request host
    let host = headers
        .get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost:8080");

    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("http");

    let snippet = format!(
        r#"<script defer data-domain="{}" src="{}://{}/api/script.js"></script>"#,
        source.domain, scheme, host
    );

    Ok(Json(serde_json::json!({ "snippet": snippet })))
}

// ── Helpers ────────────────────────────────────────────────────────

fn get_ingest(state: &AppState) -> AppResult<&Arc<IngestState>> {
    state
        .ingest
        .as_ref()
        .ok_or_else(|| AppError::Internal("Ingest engine not initialized".to_string()))
}

/// Extract client IP from headers, preferring X-Forwarded-For.
fn extract_ip(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|s| s.trim().to_string())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(ToString::to_string)
        })
        .unwrap_or_default()
}
