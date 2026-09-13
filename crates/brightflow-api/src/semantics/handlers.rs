//! HTTP handlers for per-table semantics: the resolved view, the layers
//! behind it, and the table's own settings.
//!
//! Auth posture: session-authenticated. Read-only: every semantic write,
//! column or table, goes through the action bus so it is logged, attributed
//! to a layer and undoable.

use axum::{
    extract::{Path, Query, State},
    Json,
};

use crate::semantics::types::{
    ColumnSemanticsResponse, DeclarationChangesResponse, LayersQuery, SemanticModelResponse,
    TableSettings, TableSettingsResponse,
};
use crate::shared::AppResult;
use crate::state::AppState;

/// GET /api/sources/{source_id}/tables/{name}/semantics — the resolved
/// columns; `?layers=1` also returns every opinion row behind them.
pub async fn list_semantics(
    State(state): State<AppState>,
    Path((source_id, name)): Path<(String, String)>,
    Query(query): Query<LayersQuery>,
) -> AppResult<Json<ColumnSemanticsResponse>> {
    let store = state.require_store()?;
    let columns = store.resolved_columns(&source_id, &name).await?;
    let layers = if query.layers.unwrap_or(false) {
        Some(store.column_opinions(&source_id, &name).await?)
    } else {
        None
    };
    Ok(Json(ColumnSemanticsResponse {
        table_name: name,
        columns,
        layers,
    }))
}

/// GET /api/sources/{source_id}/tables/{name}/settings — the resolved table
/// settings.
pub async fn get_table_settings(
    State(state): State<AppState>,
    Path((source_id, name)): Path<(String, String)>,
) -> AppResult<Json<TableSettingsResponse>> {
    let store = state.require_store()?;
    let resolved = store.resolved_table(&source_id, &name).await?;
    Ok(Json(TableSettingsResponse {
        table_name: name,
        settings: resolved.map(TableSettings::from).unwrap_or_default(),
    }))
}

/// GET /api/sources/{source_id}/tables/{name}/semantics/changes — what each
/// re-declaration by a producer changed, newest first.
pub async fn list_declaration_changes(
    State(state): State<AppState>,
    Path((source_id, name)): Path<(String, String)>,
) -> AppResult<Json<DeclarationChangesResponse>> {
    let store = state.require_store()?;
    let table = store
        .db()
        .get_table(&source_id, &name)
        .await?
        .ok_or_else(|| crate::shared::AppError::NotFound(format!("Table '{name}' not found")))?;
    let changes = store.db().declaration_changes(&table.id).await?;
    Ok(Json(DeclarationChangesResponse {
        table_name: name,
        changes,
    }))
}

/// GET /api/sources/{source_id}/semantic-model — the source as one Ossie
/// document, from the resolved views. Generated, never stored.
pub async fn export_semantic_model(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
) -> AppResult<Json<SemanticModelResponse>> {
    let store = state.require_store()?;
    let model = store.export_model(&source_id).await?;
    Ok(Json(SemanticModelResponse::new(model)))
}
