use axum::extract::{Path, State};
use axum::Json;
use uuid::Uuid;

use crate::shared::{AppError, AppResult};
use crate::state::AppState;

use super::runner::spawn_connector_run;
use super::types::{ConnectorInfo, ConnectorRun, RunRequest, RunResponse};

/// GET /api/connectors — list configured connectors from YAML config dir
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
        if path
            .extension()
            .is_some_and(|ext| ext == "yaml" || ext == "yml")
        {
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

    // Find the YAML config for this connector
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

/// Find a YAML config file in the config directory by connector name
fn find_config_file(
    config_dir: &std::path::Path,
    name: &str,
) -> Result<std::path::PathBuf, AppError> {
    let yaml_path = config_dir.join(format!("{name}.yaml"));
    if yaml_path.exists() {
        return Ok(yaml_path);
    }

    let yml_path = config_dir.join(format!("{name}.yml"));
    if yml_path.exists() {
        return Ok(yml_path);
    }

    Err(AppError::NotFound(format!(
        "No config file found for connector '{name}' (looked for {name}.yaml and {name}.yml in {})",
        config_dir.display()
    )))
}
