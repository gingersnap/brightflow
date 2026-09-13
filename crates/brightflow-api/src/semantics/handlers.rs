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
    ColumnSemanticsResponse, DeclarationChangesResponse, ImportQuery, ImportedTable, LayersQuery,
    SemanticModelImportResponse, SemanticModelResponse, TableSettings, TableSettingsResponse,
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

/// POST /api/sources/{source_id}/semantic-model/import — apply a pasted model.
///
/// An Ossie document or a bare model, strict or short form, becomes
/// `declared`-layer rows under `document:{model name}`, one table at a
/// time. Datasets with no table here are reported, not an error; a model
/// that does not parse or validate is a 400 naming the problem. With
/// `?dry_run=true` nothing is written.
pub async fn import_semantic_model(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
    Query(query): Query<ImportQuery>,
    Json(body): Json<serde_json::Value>,
) -> AppResult<Json<SemanticModelImportResponse>> {
    use crate::shared::AppError;
    let store = state.require_store()?;
    let model = brightflow_types::parse_model(body).map_err(AppError::BadRequest)?;
    if let Err(violations) = model.validate() {
        let list: Vec<String> = violations.iter().map(ToString::to_string).collect();
        return Err(AppError::BadRequest(format!(
            "the model is not well formed: {}",
            list.join("; ")
        )));
    }
    let producer = format!("document:{}", model.name);
    let provenance = brightflow_types::Provenance::declared(producer.clone());
    let dry_run = query.dry_run.unwrap_or(false);
    let mut applied = Vec::new();
    let mut missing_tables = Vec::new();
    for decl in brightflow_types::declarations_of(&model) {
        let Some(table) = store.db().get_table(&source_id, &decl.name).await? else {
            missing_tables.push(decl.name.clone());
            continue;
        };
        if let Err(violations) = decl.validate() {
            let list: Vec<String> = violations.iter().map(ToString::to_string).collect();
            return Err(AppError::BadRequest(format!(
                "dataset `{}` is not well formed: {}",
                decl.name,
                list.join("; ")
            )));
        }
        if dry_run {
            let known = table.column_names();
            let columns_without_data = decl
                .dataset
                .as_ref()
                .map(|d| {
                    d.fields
                        .iter()
                        .filter(|f| !known.contains(&f.name))
                        .map(|f| f.name.clone())
                        .collect()
                })
                .unwrap_or_default();
            applied.push(ImportedTable {
                table: decl.name.clone(),
                columns: decl.dataset.as_ref().map_or(0, |d| d.fields.len()),
                relationships: decl.relationships.len(),
                metrics: decl.metrics.len(),
                columns_without_data,
                metrics_skipped: decl
                    .metrics
                    .iter()
                    .filter(|m| m.structured_expr().is_none())
                    .map(|m| m.name.clone())
                    .collect(),
                relationships_skipped: Vec::new(),
            });
            continue;
        }
        let outcome = store
            .apply_declaration(&source_id, &decl, &provenance)
            .await?;
        applied.push(ImportedTable {
            table: decl.name.clone(),
            columns: outcome.columns,
            relationships: outcome.relationships,
            metrics: outcome.metrics,
            columns_without_data: outcome.columns_without_data,
            metrics_skipped: outcome.metrics_skipped,
            relationships_skipped: outcome.relationships_skipped,
        });
    }
    Ok(Json(SemanticModelImportResponse {
        producer,
        dry_run,
        applied,
        missing_tables,
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
