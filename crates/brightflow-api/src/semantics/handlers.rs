//! HTTP handlers for per-table semantics: the resolved view, the layers
//! behind it, and the table's own settings.
//!
//! Auth posture: session-authenticated. Column writes go through the action
//! bus, not here. The one write left on this router, table settings, files
//! a `user`-layer opinion and refreshes the in-memory override so the next
//! analysis run sees it without a restart; it is the last semantic write not
//! on the bus and is slated to move there.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use brightflow_types::{Layer, Provenance, TableOpinion};

use crate::semantics::types::{
    ColumnSemanticsResponse, LayersQuery, SemanticModelResponse, TableSettings,
    TableSettingsResponse,
};
use crate::shared::AppResult;
use crate::state::{cache_key, settings_from_resolved, AppState};

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

/// PUT /api/sources/{source_id}/tables/{name}/settings — write the caller's
/// table settings as one `user`-layer opinion.
pub async fn upsert_table_settings(
    State(state): State<AppState>,
    Path((source_id, name)): Path<(String, String)>,
    Json(req): Json<TableSettings>,
) -> AppResult<Json<TableSettingsResponse>> {
    let store = state.require_store()?;
    let table = store
        .db()
        .get_table(&source_id, &name)
        .await?
        .ok_or_else(|| crate::shared::AppError::NotFound(format!("Table '{name}' not found")))?;
    let opinion = TableOpinion {
        provenance: Provenance {
            layer: Layer::User,
            producer: "user:settings".to_string(),
            version: None,
            hash: None,
        },
        updated_at: 0,
        display_name: req.display_name,
        description: req.description,
        time_granularity: req.time_granularity,
        comparison_periods: req.comparison_periods,
        doc: None,
        ai_context: None,
        custom_extensions: Vec::new(),
    };
    store.db().write_table_opinion(&table.id, &opinion).await?;

    let resolved = store.db().resolved_table(&table.id).await?;
    let key = cache_key(&source_id, &name);
    match resolved.as_ref().and_then(settings_from_resolved) {
        Some(settings) => {
            state.settings_overrides.insert(key, settings);
        },
        None => {
            state.settings_overrides.remove(&key);
        },
    }
    Ok(Json(TableSettingsResponse {
        table_name: name,
        settings: resolved.map(TableSettings::from).unwrap_or_default(),
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
