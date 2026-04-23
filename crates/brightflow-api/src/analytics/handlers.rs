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

    Ok(Json(LoadTableResponse {
        id,
        name: dataset.name.clone(),
        row_count: dataset.row_count(),
        column_count: dataset.column_count(),
        columns: dataset.columns(),
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

/// Upload a CSV file as a new dataset
pub async fn upload_dataset(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> AppResult<(StatusCode, Json<UploadResponse>)> {
    let mut file_data: Option<Vec<u8>> = None;
    let mut file_name: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to read multipart field: {e}")))?
    {
        let name = field.name().unwrap_or_default().to_string();

        if name == "file" {
            file_name = field.file_name().map(String::from);
            file_data = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("Failed to read file data: {e}")))?
                    .to_vec(),
            );
        }
    }

    let data = file_data.ok_or_else(|| AppError::BadRequest("No file provided".into()))?;
    let name = file_name.unwrap_or_else(|| "uploaded.csv".to_string());

    // Parse CSV in blocking task
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
    let columns: Vec<session::ColumnInfo> = df
        .get_columns()
        .iter()
        .map(|col| session::ColumnInfo {
            name: col.name().to_string(),
            dtype: dtype_to_string(col.dtype()),
            role: None,
            is_kpi: None,
            label: None,
        })
        .collect();

    let id = state.datasets.add_dataset(
        name.clone(),
        DatasetData::Uploaded(df),
        DatasetSource::Upload {
            filename: name.clone(),
        },
    );

    Ok((
        StatusCode::CREATED,
        Json(UploadResponse {
            id,
            name,
            row_count,
            column_count,
            columns,
        }),
    ))
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

    // Handle incoming messages
    while let Some(Ok(msg)) = receiver.next().await {
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

    let Some(data) = state.datasets.get_data(&dataset_id) else {
        return WsServerMessage::Error {
            code: "NOT_FOUND".into(),
            message: format!("Dataset '{dataset_id}' not found"),
        };
    };

    match tokio::task::spawn_blocking(move || executor::execute_query(&data, query)).await {
        Ok(Ok(response)) => WsServerMessage::QueryResult(response),
        Ok(Err(e)) => WsServerMessage::Error {
            code: e.error_code().into(),
            message: e.to_string(),
        },
        Err(e) => WsServerMessage::Error {
            code: "TASK_ERROR".into(),
            message: format!("Task execution failed: {e}"),
        },
    }
}

/// Convert Polars DataType to a display string
fn dtype_to_string(dtype: &DataType) -> String {
    match dtype {
        DataType::Boolean => "bool",
        DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64 => "int",
        DataType::UInt8 | DataType::UInt16 | DataType::UInt32 | DataType::UInt64 => "uint",
        DataType::Float32 | DataType::Float64 => "float",
        DataType::String => "string",
        DataType::Datetime(_, _) => "datetime",
        DataType::Date => "date",
        DataType::Time => "time",
        DataType::Duration(_) => "duration",
        DataType::Null => "null",
        DataType::List(_) => "list",
        DataType::Struct(_) => "struct",
        _ => "unknown",
    }
    .to_string()
}
