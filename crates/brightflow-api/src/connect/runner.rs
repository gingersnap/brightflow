use std::collections::HashMap;
use std::path::{Path, PathBuf};

use brightflow_connect::{get_builtin_connector_path, RunOptions};
use chrono::Utc;
use uuid::Uuid;

use crate::shared::AppError;
use crate::state::AppState;

use super::types::{ConnectorRun, RunStatus};

/// Spawn a background connector run.
///
/// Returns the run ID immediately; the actual work happens in a spawned task.
pub fn spawn_connector_run(
    state: &AppState,
    connector_name: &str,
    config_path: &Path,
    only: Option<String>,
) -> Result<Uuid, AppError> {
    // Validate delta store exists
    let _store = state
        .delta_store()
        .ok_or_else(|| AppError::BadRequest("No Delta store configured".to_string()))?;

    // Validate the .lua connector file exists
    let lua_path = get_builtin_connector_path(connector_name).ok_or_else(|| {
        AppError::NotFound(format!(
            "Connector '{connector_name}' not found (no matching .lua file)"
        ))
    })?;

    let run_id = Uuid::new_v4();
    let run = ConnectorRun {
        id: run_id,
        connector: connector_name.to_string(),
        config_name: connector_name.to_string(),
        status: RunStatus::Pending,
        started_at: Utc::now(),
        finished_at: None,
        endpoints_synced: Vec::new(),
        tables_ingested: Vec::new(),
        error: None,
    };

    state.connector_runs.insert(run_id, run);

    let state = state.clone();
    let config_path = config_path.to_path_buf();
    let connector_name = connector_name.to_string();

    tokio::spawn(async move {
        execute_run(
            &state,
            run_id,
            &connector_name,
            &lua_path,
            &config_path,
            only,
        )
        .await;
    });

    Ok(run_id)
}

async fn execute_run(
    state: &AppState,
    run_id: Uuid,
    connector_name: &str,
    lua_path: &Path,
    config_path: &Path,
    only: Option<String>,
) {
    // Set status to Running
    if let Some(mut run) = state.connector_runs.get_mut(&run_id) {
        run.status = RunStatus::Running;
    }

    let options = RunOptions {
        only,
        dry_run: false,
        cursor_values: HashMap::new(),
    };

    // Execute the connector
    let result = brightflow_connect::run_connector(lua_path, config_path, &options).await;

    match result {
        Ok(connector_result) => {
            let endpoints = connector_result.endpoints_synced.clone();
            let output_path = PathBuf::from(&connector_result.output_path);

            // Ingest parquet files into Delta store
            let mut tables_ingested = Vec::new();
            if let Some(store) = state.delta_store() {
                if output_path.exists() && output_path.is_dir() {
                    if let Ok(entries) = std::fs::read_dir(&output_path) {
                        for dir_entry in entries.flatten() {
                            let path = dir_entry.path();
                            if path.extension().is_some_and(|ext| ext == "parquet") {
                                let table_name = path
                                    .file_stem()
                                    .and_then(|s| s.to_str())
                                    .unwrap_or("unknown")
                                    .to_string();

                                tracing::info!(
                                    "[{}] Ingesting {} into table '{}'",
                                    connector_name,
                                    path.display(),
                                    table_name
                                );

                                // Use merge (upsert by id) instead of append
                                let pks = vec!["id".to_string()];
                                match store.merge_parquet(&table_name, &path, &pks).await {
                                    Ok(metrics) => {
                                        tracing::info!(
                                            "[{}] Table '{}': {} inserted, {} updated",
                                            connector_name,
                                            table_name,
                                            metrics.rows_inserted,
                                            metrics.rows_updated,
                                        );
                                        tables_ingested.push(table_name);
                                    },
                                    Err(e) => {
                                        tracing::error!(
                                            "[{}] Failed to ingest {}: {}",
                                            connector_name,
                                            path.display(),
                                            e
                                        );
                                    },
                                }
                            }
                        }
                    }
                }
            }

            // Refresh table index so new tables are visible
            state.refresh_table_index().await;

            // Mark completed
            if let Some(mut run) = state.connector_runs.get_mut(&run_id) {
                run.status = RunStatus::Completed;
                run.finished_at = Some(Utc::now());
                run.endpoints_synced = endpoints;
                run.tables_ingested = tables_ingested;
            }

            tracing::info!("[{}] Run {} completed", connector_name, run_id);
        },
        Err(e) => {
            let msg = format!("{e}");
            tracing::error!("[{}] Run {} failed: {}", connector_name, run_id, msg);

            if let Some(mut run) = state.connector_runs.get_mut(&run_id) {
                run.status = RunStatus::Failed;
                run.finished_at = Some(Utc::now());
                run.error = Some(msg);
            }
        },
    }
}
