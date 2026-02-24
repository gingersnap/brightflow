use axum::extract::{Path, State};
use axum::Json;
use uuid::Uuid;

use crate::shared::{AppError, AppResult};
use crate::state::AppState;

use super::runner::spawn_connector_run;
use super::types::{
    ConnectorInfo, ConnectorRun, RunRequest, RunResponse, ScheduleRequest, ScheduleResponse,
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

            let valid = brightflow_connect::get_builtin_connector_path(&name).is_some();

            connectors.push(ConnectorInfo {
                connector: name.clone(),
                name,
                valid,
            });
        }
    }

    Ok(Json(connectors))
}

/// POST /api/connectors/:name/run — trigger a connector run
pub async fn run_connector(
    State(state): State<AppState>,
    Path(name): Path<String>,
    body: Option<Json<RunRequest>>,
) -> AppResult<Json<RunResponse>> {
    let config_dir = state.connector_config_dir.as_ref().ok_or_else(|| {
        AppError::BadRequest("No connector config directory configured".to_string())
    })?;

    let config_path = find_config_file(config_dir, &name)?;

    let only = body.and_then(|b| b.0.only);

    let run_id = spawn_connector_run(&state, &name, &config_path, only)?;

    Ok(Json(RunResponse {
        run_id,
        connector: name,
        status: super::types::RunStatus::Pending,
    }))
}

/// GET /api/connectors/runs — list all runs
pub async fn list_runs(State(state): State<AppState>) -> Json<Vec<ConnectorRun>> {
    let mut runs: Vec<ConnectorRun> = state
        .connector_runs
        .iter()
        .map(|entry| entry.value().clone())
        .collect();

    // Sort by started_at descending (newest first)
    runs.sort_by(|a, b| b.started_at.cmp(&a.started_at));

    Json(runs)
}

/// GET /api/connectors/runs/:id — get a specific run
pub async fn get_run(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ConnectorRun>> {
    let run = state
        .connector_runs
        .get(&id)
        .map(|entry| entry.value().clone())
        .ok_or_else(|| AppError::NotFound(format!("Run {id} not found")))?;

    Ok(Json(run))
}

/// POST /api/connectors/:name/schedule — create or update a schedule for a file-based connector
pub async fn schedule_connector(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<ScheduleRequest>,
) -> AppResult<Json<ScheduleResponse>> {
    let db = state
        .auth_db
        .as_ref()
        .ok_or_else(|| AppError::Internal("No database configured".to_string()))?;

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
