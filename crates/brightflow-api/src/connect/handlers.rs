//! HTTP handlers for connector configuration, scheduling, and run history.
//!
//! Auth posture: session-authenticated. These endpoints write scheduler config
//! and can trigger syncs, so they are administrative rather than read-only.

use axum::extract::{Path, State};
use axum::Json;

use crate::shared::{AppError, AppResult};
use crate::state::AppState;

use super::types::{
    AvailableConnectorResponse, EnrichedSyncRun, PresetInfo, RunTriggerResponse, ScheduleRequest,
    ScheduleResponse, UnifiedConnector, UnifiedJob, UnifiedSyncRun, UpdateTokenRequest,
};

/// GET /api/connectors/available — discover all available connectors + their presets
pub async fn list_available_connectors(
    State(state): State<AppState>,
) -> AppResult<Json<Vec<AvailableConnectorResponse>>> {
    let custom_dir = state
        .paths
        .as_ref()
        .map(brightflow_core::WorkspacePaths::connector_configs);
    let discovered = brightflow_connect::discover_connectors(custom_dir.as_deref());

    // Load presets from DB if available
    let presets = if let Some(db) = state.scheduler_db.as_ref() {
        db.list_connector_configs()
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?
    } else {
        Vec::new()
    };

    let results = discovered
        .into_iter()
        .map(|c| {
            let matching_presets: Vec<PresetInfo> = presets
                .iter()
                .filter(|p| p.connector_path == c.name)
                .map(|p| PresetInfo {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    has_token: p.token.is_some(),
                })
                .collect();
            AvailableConnectorResponse {
                name: c.name,
                version: c.version,
                description: c.description,
                source_type: c.source_type,
                source_hash: c.source_hash,
                presets: matching_presets,
            }
        })
        .collect();

    Ok(Json(results))
}

/// GET /api/connectors/unified — unified view: config + schedule + last run per connector
///
/// Now starts from discovery so connectors always appear even without a DB preset.
pub async fn list_unified_connectors(
    State(state): State<AppState>,
) -> AppResult<Json<Vec<UnifiedConnector>>> {
    let custom_dir = state
        .paths
        .as_ref()
        .map(brightflow_core::WorkspacePaths::connector_configs);
    let discovered = brightflow_connect::discover_connectors(custom_dir.as_deref());

    let db = state.require_scheduler_db()?;

    let configs = db
        .list_connector_configs()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // Load all jobs once for lookup
    let all_jobs = db
        .list_scheduler_jobs()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let mut results = Vec::new();

    // Start from discovered connectors so builtins always appear
    for connector in &discovered {
        // Find matching preset in DB
        let config = configs.iter().find(|c| c.connector_path == connector.name);

        let has_token = config.is_some_and(|c| {
            c.token.is_some()
                || serde_json::from_str::<serde_json::Value>(&c.config_json)
                    .ok()
                    .and_then(|v| v.get("token")?.as_str().map(|s| !s.is_empty()))
                    .unwrap_or(false)
        });

        let (job, last_run) = if let Some(cfg) = config {
            let job = all_jobs
                .iter()
                .find(|j| j.connector_id == cfg.id)
                .map(|j| UnifiedJob {
                    id: j.id.clone(),
                    interval_secs: j.interval_secs,
                    enabled: j.enabled,
                });

            let latest = db
                .get_latest_sync_run_for_connector(&cfg.id)
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
            connector: connector.name.clone(),
            name: config.map_or_else(|| connector.name.clone(), |c| c.name.clone()),
            valid: true,
            has_token,
            job,
            last_run,
        });
    }

    // Also include any DB configs that don't match discovered connectors
    for config in &configs {
        if !discovered.iter().any(|d| d.name == config.connector_path) {
            let job = all_jobs
                .iter()
                .find(|j| j.connector_id == config.id)
                .map(|j| UnifiedJob {
                    id: j.id.clone(),
                    interval_secs: j.interval_secs,
                    enabled: j.enabled,
                });

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

            results.push(UnifiedConnector {
                connector: config.connector_path.clone(),
                name: config.name.clone(),
                valid: false,
                has_token: config.token.is_some(),
                job,
                last_run,
            });
        }
    }

    Ok(Json(results))
}

/// GET /api/connectors/:name/runs — run history for a specific connector
pub async fn list_connector_runs(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> AppResult<Json<Vec<brightflow_scheduler::SyncRun>>> {
    let db = state.require_scheduler_db()?;

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
) -> AppResult<Json<RunTriggerResponse>> {
    let db = state.require_scheduler_db()?;

    let scheduler = state
        .scheduler
        .as_ref()
        .ok_or_else(|| AppError::Internal("Scheduler not configured".to_string()))?;

    let connector_config = db
        .get_connector_config_by_name(&name)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("Connector config not found: '{name}'")))?;

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

/// POST /api/connectors/:name/schedule — create or update a schedule for a connector
pub async fn schedule_connector(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<ScheduleRequest>,
) -> AppResult<Json<ScheduleResponse>> {
    let db = state.require_scheduler_db()?;

    let connector_config = db
        .get_connector_config_by_name(&name)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("Connector config not found: '{name}'")))?;

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
///
/// If no preset exists yet for this connector, auto-creates one.
pub async fn update_connector_token(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<UpdateTokenRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let db = state.require_scheduler_db()?;

    let connector_config = db
        .get_connector_config_by_name(&name)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    match connector_config {
        Some(config) => {
            // Update existing preset's dedicated token column
            db.update_connector_token(&config.id, &body.token)
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;
        },
        None => {
            // Auto-create a preset for this connector with the token
            db.create_connector_config(&name, &name, "{}", Some(&body.token))
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;
        },
    }

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// GET /api/connectors/runs — enriched run history with connector names
pub async fn list_enriched_runs(
    State(state): State<AppState>,
) -> AppResult<Json<Vec<EnrichedSyncRun>>> {
    let db = state.require_scheduler_db()?;

    let runs = db
        .list_sync_runs(50)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let configs = db
        .list_connector_configs()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let enriched = runs
        .into_iter()
        .map(|r| {
            let connector_name = configs
                .iter()
                .find(|c| c.id == r.connector_id)
                .map_or_else(|| r.connector_id.clone(), |c| c.name.clone());
            EnrichedSyncRun {
                id: r.id,
                connector_name,
                status: r.status,
                started_at: r.started_at,
                finished_at: r.finished_at,
                rows_synced: r.rows_synced,
                error: r.error,
            }
        })
        .collect();

    Ok(Json(enriched))
}

/// DELETE /api/schedules/:id — delete a schedule
pub async fn delete_schedule(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let db = state.require_scheduler_db()?;

    let deleted = db
        .delete_scheduler_job(&id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    if deleted {
        Ok(Json(serde_json::json!({ "deleted": true })))
    } else {
        Err(AppError::NotFound(format!("Schedule {id} not found")))
    }
}
