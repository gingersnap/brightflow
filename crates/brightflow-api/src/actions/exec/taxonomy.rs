//! Vocabulary curation: define/rename/redefine/freeze/delete entries across
//! every kind (induced categories and imported catalogs alike) — the ratified
//! vocabulary the LLM must stay inside.
//!
//! Contracts the executors hold: an entry's *name* is a label and may change
//! freely; its *description* is the definition and changing it is a
//! recalibration event; a frozen entry changes in neither way; a parent with
//! children cannot be deleted; the reserved `other` is never a row; and no
//! level exceeds its cap — grouping, not deletion, is the answer over it.

use serde_json::json;

use brightflow_engine::enrichment::{check_cap, VocabKind};

use super::{table_ctx, StoreHandle};
use crate::actions::types::UndoOp;
use crate::shared::{AppError, AppResult};
use crate::state::AppState;

/// Borrowed `Action::DefineTaxonomyCategory` fields.
pub(crate) struct DefineArgs<'a> {
    pub name: &'a str,
    pub description: Option<&'a str>,
    /// None = `category`.
    pub kind: Option<&'a str>,
    /// 0 = root.
    pub parent_id: i64,
    /// Surface forms to append (imported kinds).
    pub aliases: &'a [String],
}

fn parse_kind(raw: Option<&str>) -> AppResult<VocabKind> {
    match raw {
        None => Ok(VocabKind::Category),
        Some(k) => VocabKind::parse(k).ok_or_else(|| {
            AppError::BadRequest(format!(
                "unknown vocabulary kind '{k}' — expected one of: {}",
                VocabKind::ALL
                    .iter()
                    .map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        }),
    }
}

pub(crate) async fn execute_define_taxonomy_category(
    state: &AppState,
    source_id: &str,
    table: &str,
    args: &DefineArgs<'_>,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    let name = args.name.trim();
    let kind = parse_kind(args.kind)?;
    let (store, table_id) = table_ctx(state, source_id, table).await?;

    // Placement: a root must be allowed at the root, a child must sit under a
    // parent of the right kind in the same table.
    if args.parent_id == 0 {
        if !kind.allows_root() {
            return Err(AppError::BadRequest(format!(
                "a {kind} needs a parent_id — it never sits at the root"
            )));
        }
    } else {
        let parent = owned_category(&store, &table_id, args.parent_id).await?;
        let Some(expected) = kind.parent_kind() else {
            return Err(AppError::BadRequest(format!(
                "a {kind} has no hierarchy — omit parent_id"
            )));
        };
        if parent.kind != expected.as_str() {
            return Err(AppError::BadRequest(format!(
                "parent {} is a {} — a {kind} must sit under a {expected}",
                parent.id, parent.kind
            )));
        }
    }

    let siblings = store
        .db()
        .list_vocabulary(&table_id, kind.as_str(), args.parent_id)
        .await?;
    check_cap(kind, name, siblings.iter().map(|r| r.name.as_str()))
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    let previous = siblings.iter().find(|r| r.name.eq_ignore_ascii_case(name));
    // Case-insensitive re-definition targets the existing row's exact name so
    // the upsert hits it instead of creating a near-duplicate.
    let stored_name = previous.map_or(name, |r| r.name.as_str());
    let mut row = store
        .db()
        .upsert_taxonomy_category(
            &table_id,
            kind.as_str(),
            args.parent_id,
            stored_name,
            args.description,
            None,
            chrono::Utc::now().timestamp(),
        )
        .await?;
    for alias in args.aliases.iter().filter(|a| !a.trim().is_empty()) {
        if let Some(updated) = store.db().append_taxonomy_alias(row.id, alias).await? {
            row = updated;
        }
    }
    let bumped = crate::enrichment::vocab::refresh_snapshots(state, &table_id).await;
    Ok((
        json!({
            "categoryId": row.id,
            "kind": row.kind,
            "parentId": row.parent_id,
            "name": row.name,
            "description": row.description,
            "created": previous.is_none(),
            "functionsBumped": bumped,
        }),
        Some(UndoOp::RestoreTaxonomyCategory {
            category_id: row.id,
            delete_row: previous.is_none(),
            name: previous.map(|p| p.name.clone()),
            description: previous.and_then(|p| p.description.clone()),
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
        return Err(AppError::BadRequest("name cannot be empty".to_string()));
    }
    if brightflow_engine::enrichment::is_other(name) {
        return Err(AppError::BadRequest(
            "'other' is reserved and implicit at every level".to_string(),
        ));
    }
    let (store, table_id) = table_ctx(state, source_id, table).await?;
    let previous = owned_category(&store, &table_id, category_id).await?;
    refuse_frozen(&previous, "rename")?;
    // The level-scoped UNIQUE would otherwise surface as an opaque 500.
    if let Some(clash) = store
        .db()
        .get_taxonomy_category_by_name(&table_id, &previous.kind, previous.parent_id, name)
        .await?
    {
        if clash.id != category_id {
            return Err(AppError::BadRequest(format!(
                "'{name}' is already a {} at this level",
                previous.kind
            )));
        }
    }
    let row = store
        .db()
        .rename_taxonomy_category(category_id, name)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("entry {category_id} not found")))?;
    Ok((
        json!({ "categoryId": row.id, "name": row.name, "affectsCache": false }),
        Some(UndoOp::RestoreTaxonomyCategory {
            category_id: row.id,
            delete_row: false,
            name: Some(previous.name),
            description: previous.description,
        }),
    ))
}

pub(crate) async fn execute_redefine_taxonomy_category(
    state: &AppState,
    source_id: &str,
    table: &str,
    category_id: i64,
    description: Option<&str>,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    let (store, table_id) = table_ctx(state, source_id, table).await?;
    let previous = owned_category(&store, &table_id, category_id).await?;
    refuse_frozen(&previous, "redefine")?;
    let description = description.map(str::trim).filter(|d| !d.is_empty());
    let row = store
        .db()
        .set_taxonomy_description(category_id, description)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("entry {category_id} not found")))?;
    let bumped = crate::enrichment::vocab::refresh_snapshots(state, &table_id).await;
    Ok((
        json!({
            "categoryId": row.id,
            "description": row.description,
            // The definition changed: every cell classified against the
            // old one recomputes on the next run of the owning function.
            "affectsCache": true,
            "functionsBumped": bumped,
        }),
        Some(UndoOp::RestoreTaxonomyCategory {
            category_id: row.id,
            delete_row: false,
            name: Some(previous.name),
            description: previous.description,
        }),
    ))
}

pub(crate) async fn execute_freeze_taxonomy_category(
    state: &AppState,
    source_id: &str,
    table: &str,
    category_id: i64,
    frozen: bool,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    let (store, table_id) = table_ctx(state, source_id, table).await?;
    let previous = owned_category(&store, &table_id, category_id).await?;
    let row = store
        .db()
        .set_taxonomy_frozen(category_id, frozen)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("entry {category_id} not found")))?;
    Ok((
        json!({ "categoryId": row.id, "frozen": row.frozen }),
        Some(UndoOp::RestoreTaxonomyFrozen {
            category_id: row.id,
            frozen: previous.frozen,
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
    refuse_frozen(&previous, "delete")?;
    let children = store.db().count_vocabulary_children(category_id).await?;
    if children > 0 {
        return Err(AppError::BadRequest(format!(
            "'{}' still has {children} child entr{} — delete or move them first",
            previous.name,
            if children == 1 { "y" } else { "ies" }
        )));
    }
    let deleted = store.db().delete_taxonomy_category(category_id).await?;
    if !deleted {
        return Err(AppError::NotFound(format!("entry {category_id} not found")));
    }
    let bumped = crate::enrichment::vocab::refresh_snapshots(state, &table_id).await;
    Ok((
        json!({ "categoryId": category_id, "functionsBumped": bumped }),
        Some(UndoOp::RecreateTaxonomyCategory {
            table_id,
            category_id,
            name: previous.name,
            description: previous.description,
            created_at: previous.created_at,
            kind: previous.kind,
            parent_id: previous.parent_id,
            frozen: previous.frozen,
            aliases_json: previous.aliases_json,
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
        // Undoing a *define* deletes the entry. Children added since the
        // define were not part of what is being undone; refuse rather than
        // orphan them — `delete_taxonomy_category` is the deliberate path.
        let children = store.db().count_vocabulary_children(category_id).await?;
        if children > 0 {
            return Err(AppError::BadRequest(format!(
                "cannot undo: {children} child entr{} now sit under this entry. Delete \
                 them first.",
                if children == 1 { "y" } else { "ies" }
            )));
        }
        let row = store.db().get_taxonomy_category(category_id).await?;
        store.db().delete_taxonomy_category(category_id).await?;
        if let Some(row) = row {
            crate::enrichment::vocab::refresh_snapshots(state, &row.table_id).await;
        }
    } else if let Some(name) = name {
        let row = store
            .db()
            .update_taxonomy_category(category_id, name, description)
            .await?;
        if let Some(row) = row {
            crate::enrichment::vocab::refresh_snapshots(state, &row.table_id).await;
        }
    }
    Ok(())
}

pub(crate) async fn undo_restore_taxonomy_frozen(
    state: &AppState,
    category_id: i64,
    frozen: bool,
) -> AppResult<()> {
    let store = state.require_store()?;
    store.db().set_taxonomy_frozen(category_id, frozen).await?;
    Ok(())
}

pub(crate) async fn undo_recreate_taxonomy_category(
    state: &AppState,
    row: &brightflow_store::TaxonomyCategoryRow,
) -> AppResult<()> {
    let store = state.require_store()?;
    store.db().recreate_taxonomy_category(row).await?;
    crate::enrichment::vocab::refresh_snapshots(state, &row.table_id).await;
    Ok(())
}

fn refuse_frozen(row: &brightflow_store::TaxonomyCategoryRow, verb: &str) -> AppResult<()> {
    if row.frozen {
        return Err(AppError::BadRequest(format!(
            "'{}' is frozen — unfreeze it before you {verb} it",
            row.name
        )));
    }
    Ok(())
}

/// Fetch an entry, verifying it belongs to `table_id`.
///
/// The ownership check is the authorization boundary: `category_id` arrives
/// from the client while the table scope comes from the URL, so without this a
/// caller could rename or delete another table's entries by guessing ids.
async fn owned_category(
    store: &StoreHandle,
    table_id: &str,
    category_id: i64,
) -> AppResult<brightflow_store::TaxonomyCategoryRow> {
    let row = store
        .db()
        .get_taxonomy_category(category_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("entry {category_id} not found")))?;
    if row.table_id != table_id {
        return Err(AppError::NotFound(format!("entry {category_id} not found")));
    }
    Ok(row)
}
