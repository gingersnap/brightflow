use axum::extract::{Path, State};
use axum::Json;

use crate::shared::{AppError, AppResult};
use crate::state::AppState;

use super::types::{
    ConnectorInfo, RunRequest, RunTriggerResponse, ScheduleRequest, ScheduleResponse,
    UnifiedConnector, UnifiedJob, UnifiedSyncRun, UpdateTokenRequest,
};

/// GET /api/connectors — list configured connectors from config dir
pub async fn list_connectors(State(state): State<AppState>) -> AppResult<Json<Vec<ConnectorInfo>>> {
    let config_dir = state.connector_config_dir.as_ref().ok_or_else(|| {
        AppError::BadRequest("No connector config directory configured".to_string())
    })?;

    if !config_dir.exists() || !config_dir.is_dir() {
        return Ok(Json(Vec::new()));
    }

    let entries = std::fs::read_dir(config_dir)
        .map_err(|e| AppError::Internal(format!("Failed to read config dir: {e}")))?;

    let mut connectors = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "toml") {
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();

            let valid = brightflow_connect::get_builtin_connector_source(&name).is_some();

            connectors.push(ConnectorInfo {
                connector: name.clone(),
                name,
                valid,
            });
        }
    }

    Ok(Json(connectors))
}

/// GET /api/connectors/unified — unified view: config + schedule + last run per connector
pub async fn list_unified_connectors(
    State(state): State<AppState>,
) -> AppResult<Json<Vec<UnifiedConnector>>> {
    let config_dir = state.connector_config_dir.as_ref().ok_or_else(|| {
        AppError::BadRequest("No connector config directory configured".to_string())
    })?;

    if !config_dir.exists() || !config_dir.is_dir() {
        return Ok(Json(Vec::new()));
    }

    let db = state
        .scheduler_db
        .as_ref()
        .ok_or_else(|| AppError::Internal("No scheduler database configured".to_string()))?;

    let entries = std::fs::read_dir(config_dir)
        .map_err(|e| AppError::Internal(format!("Failed to read config dir: {e}")))?;

    // Load all jobs once for lookup
    let all_jobs = db
        .list_scheduler_jobs()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let mut results = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "toml") {
            continue;
        }

        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();

        let valid = brightflow_connect::get_builtin_connector_source(&name).is_some();

        // Look up DB connector config by name
        let db_config = db
            .get_connector_config_by_name(&name)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        // Check if a token is set in the DB config
        let has_token = db_config
            .as_ref()
            .and_then(|c| serde_json::from_str::<serde_json::Value>(&c.config_json).ok())
            .and_then(|v| v.get("token")?.as_str().map(|s| !s.is_empty()))
            .unwrap_or(false);

        let (job, last_run) = if let Some(config) = &db_config {
            // Find scheduler job for this connector
            let job = all_jobs
                .iter()
                .find(|j| j.connector_id == config.id)
                .map(|j| UnifiedJob {
                    id: j.id.clone(),
                    interval_secs: j.interval_secs,
                    enabled: j.enabled,
                });

            // Find latest sync run for this connector
            let latest = db
                .get_latest_sync_run_for_connector(&config.id)
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;

            let last_run = latest.map(|r| UnifiedSyncRun {
                id: r.id,
                status: r.status,
                started_at: r.started_at,
                finished_at: r.finished_at,
                rows_synced: r.rows_synced,
                error: r.error,
            });

            (job, last_run)
        } else {
            (None, None)
        };

        results.push(UnifiedConnector {
            connector: name.clone(),
            name,
            valid,
            has_token,
            job,
            last_run,
        });
    }

    Ok(Json(results))
}

/// GET /api/connectors/:name/runs — run history for a specific connector
pub async fn list_connector_runs(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> AppResult<Json<Vec<brightflow_scheduler::SyncRun>>> {
    let db = state
        .scheduler_db
        .as_ref()
        .ok_or_else(|| AppError::Internal("No scheduler database configured".to_string()))?;

    // Find DB connector config by name
    let config = db
        .get_connector_config_by_name(&name)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("No sync history for connector '{name}'")))?;

    let runs = db
        .list_sync_runs_for_connector(&config.id, 50)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(runs))
}

/// POST /api/connectors/:name/run — trigger a connector run via the scheduler
pub async fn run_connector(
    State(state): State<AppState>,
    Path(name): Path<String>,
    _body: Option<Json<RunRequest>>,
) -> AppResult<Json<RunTriggerResponse>> {
    let db = state
        .scheduler_db
        .as_ref()
        .ok_or_else(|| AppError::Internal("No scheduler database configured".to_string()))?;

    let config_dir = state.connector_config_dir.as_ref().ok_or_else(|| {
        AppError::BadRequest("No connector config directory configured".to_string())
    })?;

    let scheduler = state
        .scheduler
        .as_ref()
        .ok_or_else(|| AppError::Internal("Scheduler not configured".to_string()))?;

    // Ensure config file exists
    let config_path = find_config_file(config_dir, &name)?;

    // Find or create DB connector config (same pattern as schedule_connector)
    let connector_config = if let Some(existing) = db
        .get_connector_config_by_name(&name)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    {
        existing
    } else {
        let config_content = std::fs::read_to_string(&config_path)
            .map_err(|e| AppError::Internal(format!("Failed to read config: {e}")))?;
        let config_value: serde_json::Value = toml::from_str(&config_content)
            .map_err(|e| AppError::Internal(format!("Failed to parse config TOML: {e}")))?;
        let config_json =
            serde_json::to_string(&config_value).map_err(|e| AppError::Internal(e.to_string()))?;

        db.create_connector_config(&name, &name, &config_json)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?
    };

    // Trigger an ad-hoc run via the scheduler
    let run_id = scheduler
        .trigger_connector_run(&connector_config.id)
        .await
        .map_err(AppError::Internal)?;

    Ok(Json(RunTriggerResponse {
        run_id,
        connector: name,
        status: "running".to_string(),
    }))
}

/// POST /api/connectors/:name/schedule — create or update a schedule for a file-based connector
pub async fn schedule_connector(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<ScheduleRequest>,
) -> AppResult<Json<ScheduleResponse>> {
    let db = state
        .scheduler_db
        .as_ref()
        .ok_or_else(|| AppError::Internal("No scheduler database configured".to_string()))?;

    let config_dir = state.connector_config_dir.as_ref().ok_or_else(|| {
        AppError::BadRequest("No connector config directory configured".to_string())
    })?;

    // Read the TOML config file and convert to JSON for storage
    let config_path = find_config_file(config_dir, &name)?;
    let config_content = std::fs::read_to_string(&config_path)
        .map_err(|e| AppError::Internal(format!("Failed to read config: {e}")))?;
    let config_value: serde_json::Value = toml::from_str(&config_content)
        .map_err(|e| AppError::Internal(format!("Failed to parse config TOML: {e}")))?;
    let config_json =
        serde_json::to_string(&config_value).map_err(|e| AppError::Internal(e.to_string()))?;

    // Find or create the DB connector config
    let connector_config = if let Some(existing) = db
        .get_connector_config_by_name(&name)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    {
        existing
    } else {
        db.create_connector_config(&name, &name, &config_json)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?
    };

    // Find existing job for this connector or create one
    let jobs = db
        .list_scheduler_jobs()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let existing_job = jobs.iter().find(|j| j.connector_id == connector_config.id);

    let job = if let Some(job) = existing_job {
        // Update interval
        db.update_scheduler_job(
            &job.id,
            Some(body.interval_secs),
            Some(body.interval_secs > 0),
        )
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::Internal("Failed to update job".to_string()))?
    } else {
        // Create new job
        db.create_scheduler_job(&name, &connector_config.id, body.interval_secs)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?
    };

    Ok(Json(ScheduleResponse {
        job_id: job.id,
        connector_config_id: connector_config.id,
        interval_secs: job.interval_secs,
        enabled: job.enabled,
    }))
}

/// PUT /api/connectors/:name/token — update the auth token for a connector
pub async fn update_connector_token(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<UpdateTokenRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let db = state
        .scheduler_db
        .as_ref()
        .ok_or_else(|| AppError::Internal("No scheduler database configured".to_string()))?;

    let config_dir = state.connector_config_dir.as_ref().ok_or_else(|| {
        AppError::BadRequest("No connector config directory configured".to_string())
    })?;

    // Find or create DB connector config
    let connector_config = if let Some(existing) = db
        .get_connector_config_by_name(&name)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    {
        existing
    } else {
        // Bootstrap from TOML if it exists
        let config_path = find_config_file(config_dir, &name)?;
        let config_content = std::fs::read_to_string(&config_path)
            .map_err(|e| AppError::Internal(format!("Failed to read config: {e}")))?;
        let config_value: serde_json::Value = toml::from_str(&config_content)
            .map_err(|e| AppError::Internal(format!("Failed to parse config TOML: {e}")))?;
        let config_json =
            serde_json::to_string(&config_value).map_err(|e| AppError::Internal(e.to_string()))?;

        db.create_connector_config(&name, &name, &config_json)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?
    };

    // Parse existing config, set token, save back
    let mut config_value: serde_json::Value =
        serde_json::from_str(&connector_config.config_json)
            .map_err(|e| AppError::Internal(format!("Failed to parse config JSON: {e}")))?;

    config_value["token"] = serde_json::Value::String(body.token);

    let updated_json =
        serde_json::to_string(&config_value).map_err(|e| AppError::Internal(e.to_string()))?;

    db.update_connector_config(
        &connector_config.id,
        &connector_config.name,
        &connector_config.connector_path,
        &updated_json,
    )
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Find a TOML config file in the config directory by connector name
fn find_config_file(
    config_dir: &std::path::Path,
    name: &str,
) -> Result<std::path::PathBuf, AppError> {
    let toml_path = config_dir.join(format!("{name}.toml"));
    if toml_path.exists() {
        return Ok(toml_path);
    }

    Err(AppError::NotFound(format!(
        "No config file found for connector '{name}' (looked for {name}.toml in {})",
        config_dir.display()
    )))
}
