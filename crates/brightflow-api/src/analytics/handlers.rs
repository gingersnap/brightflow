//! HTTP and WebSocket handlers for the query surface: list and load tables,
//! upload CSVs, run queries.
//!
//! Queries have a WebSocket path as well as a REST one because interactive
//! exploration re-queries on every builder tweak, and per-query connection setup
//! was the dominant latency. Both paths run the same executor.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Multipart, Path, State,
    },
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use brightflow_store::TableInfo;
use futures::{SinkExt, StreamExt};
use polars::prelude::*;
use std::io::Cursor;

use crate::analytics::session::{DatasetData, DatasetInfo, DatasetSource};
use crate::analytics::types::{
    DatasetMetadataResponse, LoadTableResponse, Query, QueryResponse, UploadResponse,
    WsClientMessage, WsServerMessage,
};
use crate::analytics::{executor, session};
use crate::shared::{AppError, AppResult};
use crate::state::AppState;
use tracing::instrument;

const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

// ============================================================================
// REST Handlers
// ============================================================================

/// List all loaded datasets
pub async fn list_datasets(State(state): State<AppState>) -> Json<Vec<DatasetInfo>> {
    Json(state.datasets.list_datasets())
}

/// List available Parquet tables (metadata only, no data loaded)
pub async fn list_available_tables(State(state): State<AppState>) -> Json<Vec<TableInfo>> {
    Json(state.get_available_tables().await)
}

/// Load a specific Parquet table (unloads previously loaded tables)
#[instrument(skip(state))]
pub async fn load_table(
    State(state): State<AppState>,
    Path((source_id, name)): Path<(String, String)>,
) -> AppResult<Json<LoadTableResponse>> {
    // Load the table — the store is authoritative about existence.
    // A stale in-memory table_index can lag behind fresh sync output.
    let id = state.load_table(&source_id, &name).await?;

    // Get the loaded dataset info
    let dataset = state
        .datasets
        .get_dataset(&id)
        .ok_or_else(|| AppError::Internal("Failed to retrieve loaded dataset".into()))?;

    // Overlay the store's resolved semantics so Explore, the palette and the
    // settings UIs show role/KPI/label/polarity, the declared datatype and
    // who said so, without a second fetch.
    let mut columns = dataset.columns();
    let table_display: Option<brightflow_types::ResolvedTable> = match state.store() {
        Some(store) => {
            let resolved = store.resolved_columns(&source_id, &name).await?;
            for col in &mut columns {
                if let Some(r) = resolved.iter().find(|r| r.name == col.name) {
                    col.apply_resolved(r);
                }
            }
            store.resolved_table(&source_id, &name).await?
        },
        None => None,
    };
    let key = crate::state::cache_key(&source_id, &name);
    let time_granularity = state
        .settings_overrides
        .get(&key)
        .and_then(|s| s.time_granularity);

    Ok(Json(LoadTableResponse {
        id,
        name: dataset.name.clone(),
        row_count: dataset.row_count(),
        column_count: dataset.column_count(),
        columns,
        time_granularity,
        display_name: table_display.as_ref().and_then(|t| t.display_name.clone()),
        description: table_display.and_then(|t| t.description),
    }))
}

/// Get metadata for a specific dataset
pub async fn get_dataset(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<DatasetMetadataResponse>> {
    let dataset = state
        .datasets
        .get_dataset(&id)
        .ok_or_else(|| AppError::NotFound(format!("Dataset '{id}' not found")))?;

    Ok(Json(DatasetMetadataResponse {
        id: id.clone(),
        name: dataset.name.clone(),
        row_count: dataset.row_count(),
        column_count: dataset.column_count(),
        columns: dataset.columns(),
    }))
}

/// Delete a dataset
pub async fn delete_dataset(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    if id == "default" {
        return Err(AppError::BadRequest(
            "Cannot delete the default dataset".into(),
        ));
    }

    if state.datasets.delete_dataset(&id) {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound(format!("Dataset '{id}' not found")))
    }
}

/// Read the `file` (and optional `table`) fields out of an upload multipart.
async fn read_upload_multipart(
    multipart: &mut Multipart,
) -> AppResult<(Vec<u8>, String, Option<String>)> {
    let mut file_data: Option<Vec<u8>> = None;
    let mut file_name: Option<String> = None;
    let mut table_override: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to read multipart field: {e}")))?
    {
        let name = field.name().unwrap_or_default().to_string();
        match name.as_str() {
            "file" => {
                file_name = field.file_name().map(String::from);
                file_data = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| {
                            AppError::BadRequest(format!("Failed to read file data: {e}"))
                        })?
                        .to_vec(),
                );
            },
            "table" => {
                let value = field
                    .text()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("Failed to read table name: {e}")))?;
                if !value.trim().is_empty() {
                    table_override = Some(value.trim().to_string());
                }
            },
            _ => {},
        }
    }

    let data = file_data.ok_or_else(|| AppError::BadRequest("No file provided".into()))?;
    let name = file_name.unwrap_or_else(|| "uploaded.csv".to_string());
    Ok((data, name, table_override))
}

/// Derive a legal table name from a filename ("Q3 Leads.csv" → "q3_leads").
fn slugify_table_name(filename: &str) -> String {
    let stem = std::path::Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("table");
    let mut slug: String = stem
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    slug = slug.trim_matches('_').to_string();
    while slug.contains("__") {
        slug = slug.replace("__", "_");
    }
    if slug.is_empty() {
        slug = "table".to_string();
    }
    if slug.starts_with(|c: char| c.is_ascii_digit()) {
        slug = format!("t_{slug}");
    }
    slug
}

/// Parse CSV bytes and persist them as a new `upload:{uuid}` source in the
/// store. Returns (source_id, table, row_count, column names).
async fn ingest_csv_as_source(
    state: &AppState,
    data: Vec<u8>,
    filename: &str,
    table_override: Option<&str>,
) -> AppResult<(String, String, usize, Vec<String>)> {
    let store = state.require_store()?;
    let paths = state
        .paths
        .as_ref()
        .ok_or_else(|| AppError::Internal("workspace paths unavailable".to_string()))?;

    let mut df = tokio::task::spawn_blocking(move || -> Result<DataFrame, PolarsError> {
        let cursor = Cursor::new(data);
        CsvReadOptions::default()
            .with_infer_schema_length(Some(10000))
            .into_reader_with_file_handle(cursor)
            .finish()
    })
    .await??;
    if df.height() == 0 {
        return Err(AppError::BadRequest("CSV contains no data rows".into()));
    }

    let table = table_override.map_or_else(|| slugify_table_name(filename), slugify_table_name);
    let source_id = brightflow_core::upload_source_id(&uuid::Uuid::now_v7().to_string());

    // Stage a temp parquet under the workspace so ingest can copy it in.
    let tmp_dir = paths.base().join("tmp");
    std::fs::create_dir_all(&tmp_dir).map_err(AppError::Io)?;
    let tmp_path = tmp_dir.join(format!("{}.parquet", uuid::Uuid::now_v7()));
    {
        let tmp_for_write = tmp_path.clone();
        let mut frame = std::mem::take(&mut df);
        df = tokio::task::spawn_blocking(move || -> Result<DataFrame, PolarsError> {
            let file = std::fs::File::create(&tmp_for_write)
                .map_err(|e| PolarsError::ComputeError(e.to_string().into()))?;
            ParquetWriter::new(file).finish(&mut frame)?;
            Ok(frame)
        })
        .await??;
    }

    let ingest_result = store
        .ingest_parquet(
            &source_id,
            &table,
            &tmp_path,
            Some(brightflow_store::IngestOptions {
                mode: brightflow_store::IngestMode::ErrorIfExists,
                ..Default::default()
            }),
        )
        .await;
    if tmp_path.exists() {
        drop(std::fs::remove_file(&tmp_path));
    }
    ingest_result?;

    let meta = serde_json::json!({ "filename": filename }).to_string();
    store
        .db()
        .register_source(&source_id, "upload", filename, Some(&meta))
        .await?;
    // Nobody describes an upload, so the detector gives it its base layer.
    if let Err(e) = crate::semantics::detect::declare_detected(store, &source_id, &table).await {
        tracing::warn!("detection for '{source_id}/{table}' failed: {e}");
    }
    state.refresh_overrides_from_store(&source_id, &table).await;
    state.refresh_table_index().await;

    let columns: Vec<String> = df
        .get_column_names()
        .iter()
        .map(ToString::to_string)
        .collect();
    Ok((source_id, table, df.height(), columns))
}

/// `POST /api/sources/upload` — persist a CSV as a first-class source.
pub async fn upload_source(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> AppResult<(
    StatusCode,
    Json<crate::sources::types::UploadSourceResponse>,
)> {
    let (data, filename, table_override) = read_upload_multipart(&mut multipart).await?;
    let (source_id, table, row_count, columns) =
        ingest_csv_as_source(&state, data, &filename, table_override.as_deref()).await?;
    Ok((
        StatusCode::CREATED,
        Json(crate::sources::types::UploadSourceResponse {
            source_id,
            table,
            row_count,
            columns,
        }),
    ))
}

/// Upload a CSV file as a new dataset (legacy shape).
///
/// Now delegates to the persistent upload flow — the data lands in the store
/// and survives restarts — then bridges into the DatasetManager so existing
/// explore flows keep working unchanged.
pub async fn upload_dataset(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> AppResult<(StatusCode, Json<UploadResponse>)> {
    let (data, name, table_override) = read_upload_multipart(&mut multipart).await?;

    // No store configured (bare dev mode): keep the old in-memory behavior.
    if state.store().is_none() {
        let df = tokio::task::spawn_blocking(move || -> Result<DataFrame, PolarsError> {
            let cursor = Cursor::new(data);
            CsvReadOptions::default()
                .with_infer_schema_length(Some(10000))
                .into_reader_with_file_handle(cursor)
                .finish()
        })
        .await??;
        let row_count = df.height();
        let column_count = df.width();
        let columns = column_infos(&df);
        let id = state.datasets.add_dataset(
            name.clone(),
            DatasetData::Uploaded(df),
            DatasetSource::Upload {
                filename: name.clone(),
            },
        );
        return Ok((
            StatusCode::CREATED,
            Json(UploadResponse {
                id,
                name,
                row_count,
                column_count,
                columns,
            }),
        ));
    }

    let (source_id, table, row_count, _columns) =
        ingest_csv_as_source(&state, data, &name, table_override.as_deref()).await?;
    let id = state.load_table(&source_id, &table).await?;
    let dataset = state
        .datasets
        .get_dataset(&id)
        .ok_or_else(|| AppError::Internal("Failed to retrieve uploaded dataset".into()))?;

    Ok((
        StatusCode::CREATED,
        Json(UploadResponse {
            id,
            name,
            row_count,
            column_count: dataset.column_count().unwrap_or(0),
            columns: dataset.columns(),
        }),
    ))
}

fn column_infos(df: &DataFrame) -> Vec<session::ColumnInfo> {
    df.get_columns()
        .iter()
        .map(|col| session::ColumnInfo::plain(col.name(), col.dtype()))
        .collect()
}

/// Execute a query via REST (HTTP fallback)
#[instrument(skip(state, query))]
pub async fn execute_query(
    State(state): State<AppState>,
    Json(query): Json<Query>,
) -> AppResult<Json<QueryResponse>> {
    let dataset_id = query.dataset_id.clone();

    let data = state
        .datasets
        .get_data(&dataset_id)
        .ok_or_else(|| AppError::NotFound(format!("Dataset '{dataset_id}' not found")))?;

    let response =
        tokio::task::spawn_blocking(move || executor::execute_query(&data, query)).await??;

    Ok(Json(response))
}

// ============================================================================
// WebSocket Handler
// ============================================================================

/// WebSocket upgrade handler
pub async fn ws_handler(State(state): State<AppState>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// Handle WebSocket connection
async fn handle_socket(socket: WebSocket, state: AppState) {
    tracing::info!("WebSocket client connected");
    let (mut sender, mut receiver) = socket.split();

    // Send connected message
    let connected = WsServerMessage::Connected {
        server_version: SERVER_VERSION.to_string(),
    };
    if let Ok(json) = serde_json::to_string(&connected) {
        tracing::debug!("Sending connected message: {}", json);
        if sender.send(Message::Text(json.into())).await.is_err() {
            tracing::warn!("Failed to send connected message, client disconnected");
            return;
        }
    }

    // Curation events are pushed to every client unconditionally
    // (single-tenant — see `actions::events`).
    let mut events = state.curation_events.subscribe();
    let mut events_open = true;

    loop {
        tokio::select! {
            msg = receiver.next() => {
                let Some(Ok(msg)) = msg else { break };
                tracing::debug!("Received WebSocket message: {:?}", msg);
                let response = match msg {
                    Message::Text(text) => Some(handle_ws_message(&state, &text).await),
                    Message::Ping(data) => {
                        if sender.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                        None
                    },
                    Message::Close(_) => break,
                    _ => None,
                };

                if let Some(response) = response {
                    let json = match serde_json::to_string(&response) {
                        Ok(j) => j,
                        Err(e) => {
                            tracing::error!("Failed to serialize response: {}", e);
                            let err = WsServerMessage::Error {
                                code: "SERIALIZATION_ERROR".into(),
                                message: format!("Failed to serialize response: {e}"),
                                request_id: None,
                            };
                            serde_json::to_string(&err).unwrap_or_default()
                        },
                    };

                    tracing::debug!("Sending response: {} bytes", json.len());
                    if sender.send(Message::Text(json.into())).await.is_err() {
                        tracing::warn!("Failed to send response, client disconnected");
                        break;
                    }
                }
            }
            event = events.recv(), if events_open => {
                let outbound = match event {
                    Ok(ev) => WsServerMessage::from(ev),
                    // Fell behind the broadcast buffer: tell the client to
                    // refetch rather than replaying a gap.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::debug!("WS curation subscriber lagged by {n} events");
                        WsServerMessage::ActionResync
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        events_open = false;
                        continue;
                    }
                };
                let Ok(json) = serde_json::to_string(&outbound) else {
                    continue;
                };
                if sender.send(Message::Text(json.into())).await.is_err() {
                    break;
                }
            }
        }
    }
    tracing::info!("WebSocket client disconnected");
}

/// Process a WebSocket message and return a response
async fn handle_ws_message(state: &AppState, text: &str) -> WsServerMessage {
    tracing::info!("Processing message: {}", text);
    let msg: WsClientMessage = match serde_json::from_str(text) {
        Ok(m) => m,
        Err(e) => {
            tracing::error!("Failed to parse message: {}", e);
            return WsServerMessage::Error {
                code: "PARSE_ERROR".into(),
                message: format!("Invalid JSON: {e}"),
                request_id: None,
            };
        },
    };

    match msg {
        WsClientMessage::Query(query) => execute_ws_query(state, query).await,
        WsClientMessage::Ping => WsServerMessage::Pong,
    }
}

/// Execute a query via WebSocket
#[instrument(skip(state, query))]
async fn execute_ws_query(state: &AppState, query: Query) -> WsServerMessage {
    let dataset_id = query.dataset_id.clone();
    let request_id = query.request_id.clone();

    let Some(data) = state.datasets.get_data(&dataset_id) else {
        return WsServerMessage::Error {
            code: "NOT_FOUND".into(),
            message: format!("Dataset '{dataset_id}' not found"),
            request_id,
        };
    };

    match tokio::task::spawn_blocking(move || executor::execute_query(&data, query)).await {
        Ok(Ok(mut response)) => {
            response.request_id = request_id;
            WsServerMessage::QueryResult(response)
        },
        Ok(Err(e)) => WsServerMessage::Error {
            code: e.error_code().into(),
            message: e.to_string(),
            request_id,
        },
        Err(e) => WsServerMessage::Error {
            code: "TASK_ERROR".into(),
            message: format!("Task execution failed: {e}"),
            request_id,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_lowercases_and_replaces_separators() {
        assert_eq!(slugify_table_name("Q3 Leads.csv"), "q3_leads");
        assert_eq!(slugify_table_name("my--weird  file.csv"), "my_weird_file");
    }

    #[test]
    fn slugify_edge_cases() {
        // Leading digit gets a prefix so the name stays a legal identifier.
        assert_eq!(slugify_table_name("2024 report.csv"), "t_2024_report");
        // Nothing usable in the stem falls back to a generic name.
        assert_eq!(slugify_table_name("!!!.csv"), "table");
        assert_eq!(slugify_table_name(""), "table");
        // Underscore runs collapse and ends are trimmed.
        assert_eq!(slugify_table_name("__a__b__.csv"), "a_b");
    }
}
