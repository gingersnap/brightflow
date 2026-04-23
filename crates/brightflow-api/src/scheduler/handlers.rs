use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::shared::{AppError, AppResult};
use crate::state::AppState;

// =====================================================
// Request/Response types
// =====================================================

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CreateConnectorConfigRequest {
    pub name: String,
    pub connector_path: String,
    #[ts(type = "unknown")]
    pub config_json: serde_json::Value,
    pub token: Option<String>,
}

#[derive(Debug, Default, Deserialize, TS)]
#[ts(export)]
#[serde(default, rename_all = "camelCase")]
pub struct UpdateConnectorConfigRequest {
    pub name: Option<String>,
    pub connector_path: Option<String>,
    #[ts(type = "unknown", optional)]
    pub config_json: Option<serde_json::Value>,
    pub token: Option<String>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CreateJobRequest {
    pub name: String,
    pub connector_id: String,
    pub interval_secs: i64,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct UpdateJobRequest {
    pub interval_secs: Option<i64>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TriggerRunResponse {
    pub run_id: String,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorConfigResponse {
    pub id: String,
    pub name: String,
    pub connector_path: String,
    pub config_json: String,
    pub has_token: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl From<brightflow_scheduler::ConnectorConfig> for ConnectorConfigResponse {
    fn from(c: brightflow_scheduler::ConnectorConfig) -> Self {
        Self {
            has_token: c.token.is_some(),
            id: c.id,
            name: c.name,
            connector_path: c.connector_path,
            config_json: c.config_json,
            created_at: c.created_at,
            updated_at: c.updated_at,
        }
    }
}

// =====================================================
// Connector Config CRUD
// =====================================================

/// POST /api/connectors — create connector config
pub async fn create_connector_config(
    State(state): State<AppState>,
    Json(body): Json<CreateConnectorConfigRequest>,
) -> AppResult<Json<brightflow_scheduler::ConnectorConfig>> {
    let db = state
        .scheduler_db
        .as_ref()
        .ok_or_else(|| AppError::Internal("No scheduler database configured".to_string()))?;

    let config_json = serde_json::to_string(&body.config_json)
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    let config = db
        .create_connector_config(
            &body.name,
            &body.connector_path,
            &config_json,
            body.token.as_deref(),
        )
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(config))
}

/// GET /api/connectors — list connector configs (from DB)
pub async fn list_connector_configs(
    State(state): State<AppState>,
) -> AppResult<Json<Vec<brightflow_scheduler::ConnectorConfig>>> {
    let db = state
        .scheduler_db
        .as_ref()
        .ok_or_else(|| AppError::Internal("No scheduler database configured".to_string()))?;

    let configs = db
        .list_connector_configs()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(configs))
}

/// GET /api/connectors/:id — get connector config (without raw token)
pub async fn get_connector_config(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<ConnectorConfigResponse>> {
    let db = state
        .scheduler_db
        .as_ref()
        .ok_or_else(|| AppError::Internal("No scheduler database configured".to_string()))?;

    let config = db
        .get_connector_config(&id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("Connector config {id} not found")))?;

    Ok(Json(ConnectorConfigResponse::from(config)))
}

/// PUT /api/connectors/:id — update connector config (partial updates supported)
pub async fn update_connector_config(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateConnectorConfigRequest>,
) -> AppResult<Json<ConnectorConfigResponse>> {
    let db = state
        .scheduler_db
        .as_ref()
        .ok_or_else(|| AppError::Internal("No scheduler database configured".to_string()))?;

    let existing = db
        .get_connector_config(&id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("Connector config {id} not found")))?;

    let name = body.name.unwrap_or(existing.name);
    let connector_path = body.connector_path.unwrap_or(existing.connector_path);
    let config_json = match body.config_json {
        Some(v) => serde_json::to_string(&v).map_err(|e| AppError::BadRequest(e.to_string()))?,
        None => existing.config_json,
    };

    let config = db
        .update_connector_config(
            &id,
            &name,
            &connector_path,
            &config_json,
            body.token.as_deref(),
        )
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("Connector config {id} not found")))?;

    Ok(Json(ConnectorConfigResponse::from(config)))
}

/// DELETE /api/connectors/:id — delete connector config
pub async fn delete_connector_config(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let db = state
        .scheduler_db
        .as_ref()
        .ok_or_else(|| AppError::Internal("No scheduler database configured".to_string()))?;

    let deleted = db
        .delete_connector_config(&id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    if !deleted {
        return Err(AppError::NotFound(format!(
            "Connector config {id} not found"
        )));
    }

    // Cascade: purge this preset's tables, parquet files, and connector-output dir.
    if let Some(store) = state.store() {
        let source_key = format!("connector:{id}");
        if let Err(e) = store.delete_source_data(&source_key).await {
            tracing::warn!("Failed to purge store data for source '{source_key}': {e}");
        }
    }

    if let Some(paths) = &state.paths {
        let output_dir = paths.connector_output_for_preset(&id);
        if output_dir.exists() {
            if let Err(e) = std::fs::remove_dir_all(&output_dir) {
                tracing::warn!(
                    "Failed to remove connector output {}: {e}",
                    output_dir.display()
                );
            }
        }
    }

    state.refresh_table_index().await;

    Ok(Json(serde_json::json!({"deleted": true})))
}

// =====================================================
// Scheduler Job CRUD
// =====================================================

/// POST /api/scheduler/jobs — create job
pub async fn create_job(
    State(state): State<AppState>,
    Json(body): Json<CreateJobRequest>,
) -> AppResult<Json<brightflow_scheduler::SchedulerJob>> {
    let db = state
        .scheduler_db
        .as_ref()
        .ok_or_else(|| AppError::Internal("No scheduler database configured".to_string()))?;

    let job = db
        .create_scheduler_job(&body.name, &body.connector_id, body.interval_secs)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(job))
}

/// GET /api/scheduler/jobs — list jobs
pub async fn list_jobs(
    State(state): State<AppState>,
) -> AppResult<Json<Vec<brightflow_scheduler::SchedulerJob>>> {
    let db = state
        .scheduler_db
        .as_ref()
        .ok_or_else(|| AppError::Internal("No scheduler database configured".to_string()))?;

    let jobs = db
        .list_scheduler_jobs()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(jobs))
}

/// GET /api/scheduler/jobs/:id — get job details
pub async fn get_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<brightflow_scheduler::SchedulerJob>> {
    let db = state
        .scheduler_db
        .as_ref()
        .ok_or_else(|| AppError::Internal("No scheduler database configured".to_string()))?;

    let job = db
        .get_scheduler_job(&id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("Job {id} not found")))?;

    Ok(Json(job))
}

/// PUT /api/scheduler/jobs/:id — update job (interval, enabled)
pub async fn update_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateJobRequest>,
) -> AppResult<Json<brightflow_scheduler::SchedulerJob>> {
    let db = state
        .scheduler_db
        .as_ref()
        .ok_or_else(|| AppError::Internal("No scheduler database configured".to_string()))?;

    let job = db
        .update_scheduler_job(&id, body.interval_secs, body.enabled)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("Job {id} not found")))?;

    Ok(Json(job))
}

/// DELETE /api/scheduler/jobs/:id — delete job
pub async fn delete_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let db = state
        .scheduler_db
        .as_ref()
        .ok_or_else(|| AppError::Internal("No scheduler database configured".to_string()))?;

    let deleted = db
        .delete_scheduler_job(&id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    if deleted {
        Ok(Json(serde_json::json!({"deleted": true})))
    } else {
        Err(AppError::NotFound(format!("Job {id} not found")))
    }
}

/// POST /api/scheduler/jobs/:id/run — trigger immediate run
pub async fn trigger_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<TriggerRunResponse>> {
    let scheduler = state
        .scheduler
        .as_ref()
        .ok_or_else(|| AppError::Internal("Scheduler not configured".to_string()))?;

    let run_id = scheduler
        .trigger_job(&id)
        .await
        .map_err(AppError::Internal)?;

    Ok(Json(TriggerRunResponse { run_id }))
}

// =====================================================
// Sync Runs & State
// =====================================================

/// GET /api/sync/runs — list recent runs
pub async fn list_sync_runs(
    State(state): State<AppState>,
) -> AppResult<Json<Vec<brightflow_scheduler::SyncRun>>> {
    let db = state
        .scheduler_db
        .as_ref()
        .ok_or_else(|| AppError::Internal("No scheduler database configured".to_string()))?;

    let runs = db
        .list_sync_runs(100)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(runs))
}

/// GET /api/sync/runs/:id — get run details
pub async fn get_sync_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<brightflow_scheduler::SyncRun>> {
    let db = state
        .scheduler_db
        .as_ref()
        .ok_or_else(|| AppError::Internal("No scheduler database configured".to_string()))?;

    let run = db
        .get_sync_run(&id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("Sync run {id} not found")))?;

    Ok(Json(run))
}

/// GET /api/sync/state/:connector_id — get cursor state per endpoint
pub async fn get_sync_state(
    State(state): State<AppState>,
    Path(connector_id): Path<String>,
) -> AppResult<Json<Vec<brightflow_scheduler::SyncState>>> {
    let db = state
        .scheduler_db
        .as_ref()
        .ok_or_else(|| AppError::Internal("No scheduler database configured".to_string()))?;

    let states = db
        .list_sync_states(&connector_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(states))
}
