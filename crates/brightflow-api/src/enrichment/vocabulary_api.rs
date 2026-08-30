//! Read endpoint for a table's vocabularies. Writes go through
//! `POST /api/actions` so a human edit and an agent proposal share one
//! execution path, one audit log and one undo story.

use axum::extract::{Path, State};
use axum::Json;

use super::types::{TaxonomyCategory, TaxonomyOverview};
use crate::shared::{AppError, AppResult};
use crate::state::AppState;

/// `GET /api/sources/{source_id}/tables/{table}/taxonomy`
pub async fn get_taxonomy(
    State(state): State<AppState>,
    Path((source_id, table)): Path<(String, String)>,
) -> AppResult<Json<TaxonomyOverview>> {
    let store = state.require_store()?;
    let row = store
        .db()
        .get_table(&source_id, &table)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Table '{table}' not found")))?;
    let categories = store
        .db()
        .get_taxonomy_categories(&row.id)
        .await?
        .into_iter()
        .map(|c| TaxonomyCategory {
            id: c.id,
            kind: c.kind,
            parent_id: c.parent_id,
            name: c.name,
            description: c.description,
            frozen: c.frozen,
        })
        .collect();
    Ok(Json(TaxonomyOverview { categories }))
}
