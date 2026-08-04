//! Taxonomy curation: category define/rename/delete and per-row document
//! labels — the ratified vocabulary the labeling agents must stay inside.

use serde_json::json;

use super::{table_ctx, StoreHandle};
use crate::actions::types::UndoOp;
use crate::shared::{AppError, AppResult};
use crate::state::AppState;

pub(crate) async fn execute_define_taxonomy_category(
    state: &AppState,
    source_id: &str,
    table: &str,
    name: &str,
    description: Option<&str>,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest(
            "category name cannot be empty".to_string(),
        ));
    }
    let (store, table_id) = table_ctx(state, source_id, table).await?;
    let previous = store
        .db()
        .get_taxonomy_category_by_name(&table_id, name)
        .await?;
    let row = store
        .db()
        .upsert_taxonomy_category(&table_id, name, description, chrono::Utc::now().timestamp())
        .await?;
    Ok((
        json!({
            "categoryId": row.id,
            "name": row.name,
            "description": row.description,
            "created": previous.is_none(),
        }),
        Some(UndoOp::RestoreTaxonomyCategory {
            category_id: row.id,
            delete_row: previous.is_none(),
            name: previous.as_ref().map(|p| p.name.clone()),
            description: previous.and_then(|p| p.description),
        }),
    ))
}

pub(crate) async fn execute_rename_taxonomy_category(
    state: &AppState,
    source_id: &str,
    table: &str,
    category_id: i64,
    name: &str,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest(
            "category name cannot be empty".to_string(),
        ));
    }
    let (store, table_id) = table_ctx(state, source_id, table).await?;
    let previous = owned_category(&store, &table_id, category_id).await?;
    // UNIQUE(table_id, name) would otherwise surface as an opaque 500.
    if let Some(clash) = store
        .db()
        .get_taxonomy_category_by_name(&table_id, name)
        .await?
    {
        if clash.id != category_id {
            return Err(AppError::BadRequest(format!(
                "'{name}' is already a category in this table"
            )));
        }
    }
    let row = store
        .db()
        .rename_taxonomy_category(category_id, name)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("category {category_id} not found")))?;
    Ok((
        json!({ "categoryId": row.id, "name": row.name }),
        Some(UndoOp::RestoreTaxonomyCategory {
            category_id: row.id,
            delete_row: false,
            name: Some(previous.name),
            description: previous.description,
        }),
    ))
}

pub(crate) async fn execute_delete_taxonomy_category(
    state: &AppState,
    source_id: &str,
    table: &str,
    category_id: i64,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    let (store, table_id) = table_ctx(state, source_id, table).await?;
    let previous = owned_category(&store, &table_id, category_id).await?;
    // Snapshot the labels BEFORE deleting — the FK cascade is about to
    // destroy them, and they are curated human work.
    let labels: Vec<(String, String, i64)> = store
        .db()
        .get_document_labels_for_category(category_id)
        .await?
        .into_iter()
        .map(|l| (l.row_id, l.source, l.created_at))
        .collect();
    let deleted = store.db().delete_taxonomy_category(category_id).await?;
    if !deleted {
        return Err(AppError::NotFound(format!(
            "category {category_id} not found"
        )));
    }
    Ok((
        json!({ "categoryId": category_id, "labelsRemoved": labels.len() }),
        Some(UndoOp::RecreateTaxonomyCategory {
            table_id,
            category_id,
            name: previous.name,
            description: previous.description,
            created_at: previous.created_at,
            labels,
        }),
    ))
}

pub(crate) async fn execute_label_document(
    state: &AppState,
    source_id: &str,
    table: &str,
    row_id: &str,
    categories: &[String],
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    let (store, table_id) = table_ctx(state, source_id, table).await?;

    // Resolve names -> ids against the APPROVED taxonomy. An unknown
    // name is an error, not an implicit create: the taxonomy is the
    // ratified vocabulary, and letting a labeling call invent
    // categories would route around human approval entirely.
    let mut category_ids = Vec::with_capacity(categories.len());
    for name in categories {
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        let row = store
            .db()
            .get_taxonomy_category_by_name(&table_id, name)
            .await?
            .ok_or_else(|| {
                AppError::BadRequest(format!(
                    "'{name}' is not a category in this table's taxonomy — \
                     define it first"
                ))
            })?;
        category_ids.push(row.id);
    }
    category_ids.sort_unstable();
    category_ids.dedup();

    let previous: Vec<(i64, String, i64)> = store
        .db()
        .get_label_rows_for_row(&table_id, row_id)
        .await?
        .into_iter()
        .map(|l| (l.category_id, l.source, l.created_at))
        .collect();

    let written = store
        .db()
        .set_document_labels(
            &table_id,
            row_id,
            &category_ids,
            "human",
            chrono::Utc::now().timestamp(),
        )
        .await?;
    Ok((
        json!({ "rowId": row_id, "categories": categories, "count": written.len() }),
        Some(UndoOp::RestoreDocumentLabels {
            table_id,
            row_id: row_id.to_string(),
            labels: previous,
        }),
    ))
}

pub(crate) async fn undo_restore_taxonomy_category(
    state: &AppState,
    category_id: i64,
    delete_row: bool,
    name: Option<&str>,
    description: Option<&str>,
) -> AppResult<()> {
    let store = state.require_store()?;
    if delete_row {
        // Undoing a *define* deletes the category, and document_labels
        // cascade off it. Between the define and the undo, rows may have
        // been labelled — the labels are curated human work, and the
        // undo op was captured before they existed, so it has no
        // snapshot to restore them from. Refuse rather than silently
        // destroy them; `delete_taxonomy_category` is the deliberate
        // path, and it DOES snapshot.
        let labels = store
            .db()
            .get_document_labels_for_category(category_id)
            .await?;
        if !labels.is_empty() {
            return Err(AppError::BadRequest(format!(
                "cannot undo: {} row label(s) now use this category. Delete the \
                 category explicitly instead — that path preserves the labels for undo.",
                labels.len()
            )));
        }
        store.db().delete_taxonomy_category(category_id).await?;
    } else if let Some(name) = name {
        store
            .db()
            .update_taxonomy_category(category_id, name, description)
            .await?;
    }
    Ok(())
}

/// Borrowed `UndoOp::RecreateTaxonomyCategory` fields — over the
/// argument-count threshold as bare arguments.
pub(crate) struct RecreateTaxonomyCategoryArgs<'a> {
    pub table_id: &'a str,
    pub category_id: i64,
    pub name: &'a str,
    pub description: Option<&'a str>,
    pub created_at: i64,
    pub labels: &'a [(String, String, i64)],
}

pub(crate) async fn undo_recreate_taxonomy_category(
    state: &AppState,
    args: &RecreateTaxonomyCategoryArgs<'_>,
) -> AppResult<()> {
    let store = state.require_store()?;
    store
        .db()
        .recreate_taxonomy_category(
            args.category_id,
            args.table_id,
            args.name,
            args.description,
            args.created_at,
            args.labels,
        )
        .await?;
    Ok(())
}

pub(crate) async fn undo_restore_document_labels(
    state: &AppState,
    table_id: &str,
    row_id: &str,
    labels: &[(i64, String, i64)],
) -> AppResult<()> {
    let store = state.require_store()?;
    store
        .db()
        .restore_document_labels(table_id, row_id, labels)
        .await?;
    Ok(())
}

/// Fetch a category, verifying it belongs to `table_id`.
///
/// The ownership check is the authorization boundary: `category_id` arrives
/// from the client while the table scope comes from the URL, so without this a
/// caller could rename or delete another table's categories by guessing ids.
async fn owned_category(
    store: &StoreHandle,
    table_id: &str,
    category_id: i64,
) -> AppResult<brightflow_store::TaxonomyCategoryRow> {
    let row = store
        .db()
        .get_taxonomy_category(category_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("category {category_id} not found")))?;
    if row.table_id != table_id {
        return Err(AppError::NotFound(format!(
            "category {category_id} not found"
        )));
    }
    Ok(row)
}
