//! Model actions: create, update, delete, rebuild, and their inverses.
//!
//! A model is created under the source of its input, named as an ordinary
//! table, and built at once; a first build that fails takes the model with
//! it, so a bad recipe never leaves a half-made table. Update appends a
//! version and rebuilds; delete drops the output table (the model rows
//! cascade) after capturing them for the undo. Who dispatches is the bus's
//! concern; `created_by` records the person or run the same way saved
//! views do.

use serde_json::json;

use brightflow_store::{ModelRow, ModelVersionRow};
use brightflow_types::ModelRecipe;

use super::table_ctx;
use crate::actions::types::UndoOp;
use crate::actions::Actor;
use crate::models::{context, rebuild, rebuild_dependents};
use crate::shared::{AppError, AppResult};
use crate::state::AppState;

fn author(actor: &Actor) -> String {
    match actor {
        Actor::Human { user_id } => format!("user:{user_id}"),
        Actor::Agent { run_id, .. } => format!("agent:{run_id}"),
    }
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// A model's name is its output table's name, so it must be a name the
/// store can put on disk and one the source does not already use.
pub(crate) fn checked_name(name: &str, input: &str) -> AppResult<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("a model needs a name".into()));
    }
    if name.starts_with('.') || name.contains('/') || name.contains('\\') {
        return Err(AppError::BadRequest(format!(
            "'{name}' cannot name a table: no leading dot, no slashes"
        )));
    }
    if name == input {
        return Err(AppError::BadRequest(
            "a model cannot be named after its own input".into(),
        ));
    }
    Ok(name.to_string())
}

/// The recipe must be one this build can run and must read only columns
/// the input has.
fn checked_recipe(recipe: &ModelRecipe, input_columns: &[String]) -> AppResult<String> {
    recipe
        .validate(input_columns)
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    serde_json::to_string(recipe)
        .map_err(|e| AppError::BadRequest(format!("recipe is not JSON: {e}")))
}

fn spec_json(client_spec: Option<&serde_json::Value>) -> AppResult<Option<String>> {
    client_spec
        .map(|v| {
            serde_json::to_string(v)
                .map_err(|e| AppError::BadRequest(format!("client spec is not JSON: {e}")))
        })
        .transpose()
}

pub(crate) async fn execute_create_model(
    state: &AppState,
    actor: &Actor,
    source_id: &str,
    input: &str,
    name: &str,
    recipe: &ModelRecipe,
    client_spec: Option<&serde_json::Value>,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    let (store, input_id) = table_ctx(state, source_id, input).await?;
    let name = checked_name(name, input)?;
    if store.db().get_table(source_id, &name).await?.is_some() {
        return Err(AppError::Conflict(format!(
            "source already has a table named '{name}'"
        )));
    }
    let input_row = store
        .db()
        .get_table_by_id(&input_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("table '{input}' not found")))?;
    let recipe_json = checked_recipe(recipe, &input_row.column_names())?;

    let output = store.db().create_table(&name, source_id).await?;
    let model_id = uuid::Uuid::now_v7().to_string();
    let created_by = Some(author(actor));
    let ts = now();
    store
        .db()
        .insert_model(
            &ModelRow {
                id: model_id.clone(),
                output_table_id: output.id,
                input_table_id: Some(input_id),
                current_version: 1,
                created_by: created_by.clone(),
                created_at: ts,
                updated_at: ts,
            },
            &ModelVersionRow {
                model_id: model_id.clone(),
                version: 1,
                recipe_json,
                client_spec: spec_json(client_spec)?,
                created_by,
                created_at: ts,
            },
        )
        .await?;

    // A model that cannot build is not a model: drop it again and say why.
    let outcome = match rebuild(state, &model_id, "create").await {
        Ok(outcome) => outcome,
        Err(e) => {
            if let Err(drop) = store.delete_table(source_id, &name).await {
                tracing::warn!("could not remove failed model '{name}': {drop}");
            }
            state.refresh_table_index().await;
            return Err(e);
        },
    };
    Ok((
        json!({ "model_id": model_id, "table": name, "rows": outcome.rows, "created": true }),
        Some(UndoOp::DeleteModel { model_id }),
    ))
}

/// The model whose output is `table`, checked against the id the caller
/// named so a stale id cannot touch another table's model.
async fn scoped_model(
    state: &AppState,
    source_id: &str,
    table: &str,
    model_id: &str,
) -> AppResult<(super::StoreHandle, ModelRow)> {
    let (store, table_id) = table_ctx(state, source_id, table).await?;
    let model = store
        .db()
        .get_model_by_output(&table_id)
        .await?
        .filter(|m| m.id == model_id)
        .ok_or_else(|| AppError::NotFound(format!("'{table}' is not the model '{model_id}'")))?;
    Ok((store, model))
}

pub(crate) async fn execute_update_model(
    state: &AppState,
    actor: &Actor,
    source_id: &str,
    table: &str,
    model_id: &str,
    recipe: &ModelRecipe,
    client_spec: Option<&serde_json::Value>,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    let (store, model) = scoped_model(state, source_id, table, model_id).await?;
    let ctx = context(&store, model_id).await?;
    let recipe_json = checked_recipe(recipe, &ctx.input.column_names())?;
    let previous = model.current_version;
    let version = store
        .db()
        .append_model_version(
            model_id,
            &recipe_json,
            spec_json(client_spec)?.as_deref(),
            Some(&author(actor)),
            now(),
        )
        .await?;
    let outcome = match rebuild(state, model_id, "update").await {
        Ok(outcome) => outcome,
        Err(e) => {
            // The recipe stays on record as a version, but the model keeps
            // building what it built before.
            store
                .db()
                .set_model_current_version(model_id, previous, now())
                .await?;
            return Err(e);
        },
    };
    rebuild_dependents(state, source_id, table).await;
    Ok((
        json!({ "model_id": model_id, "table": table, "version": version, "rows": outcome.rows }),
        Some(UndoOp::RestoreModelVersion {
            model_id: model_id.to_string(),
            version: previous,
        }),
    ))
}

pub(crate) async fn execute_delete_model(
    state: &AppState,
    source_id: &str,
    table: &str,
    model_id: &str,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    let (store, model) = scoped_model(state, source_id, table, model_id).await?;
    let versions = store.db().list_model_versions(model_id).await?;
    store.delete_table(source_id, table).await?;
    state.refresh_table_index().await;
    Ok((
        json!({ "model_id": model_id, "table": table, "deleted": true }),
        Some(UndoOp::RecreateModel {
            source_id: source_id.to_string(),
            table: table.to_string(),
            model,
            versions,
        }),
    ))
}

pub(crate) async fn execute_rebuild_model(
    state: &AppState,
    source_id: &str,
    table: &str,
    model_id: &str,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    scoped_model(state, source_id, table, model_id).await?;
    let outcome = rebuild(state, model_id, "manual").await?;
    rebuild_dependents(state, source_id, table).await;
    Ok((
        json!({ "model_id": model_id, "table": table, "rows": outcome.rows, "build_id": outcome.build_id }),
        None,
    ))
}

pub(crate) async fn undo_delete_model(state: &AppState, model_id: &str) -> AppResult<()> {
    let store = state.require_store()?;
    let Some(model) = store.db().get_model(model_id).await? else {
        return Ok(());
    };
    if let Some(output) = store.db().get_table_by_id(&model.output_table_id).await? {
        store.delete_table(&output.source_id, &output.name).await?;
    }
    state.refresh_table_index().await;
    Ok(())
}

pub(crate) async fn undo_restore_model_version(
    state: &AppState,
    model_id: &str,
    version: i64,
) -> AppResult<()> {
    let store = state.require_store()?;
    store
        .db()
        .set_model_current_version(model_id, version, now())
        .await?;
    let outcome = rebuild(state, model_id, "undo").await?;
    rebuild_dependents(state, &outcome.source_id, &outcome.table).await;
    Ok(())
}

pub(crate) async fn undo_recreate_model(
    state: &AppState,
    source_id: &str,
    table: &str,
    model: &ModelRow,
    versions: &[ModelVersionRow],
) -> AppResult<()> {
    let store = state.require_store()?;
    if store.db().get_table(source_id, table).await?.is_some() {
        return Err(AppError::Conflict(format!(
            "cannot restore model '{table}': a table with that name exists again"
        )));
    }
    let output = store.db().create_table(table, source_id).await?;
    let restored = ModelRow {
        output_table_id: output.id,
        ..model.clone()
    };
    store.db().restore_model(&restored, versions).await?;
    rebuild(state, &model.id, "undo").await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_trimmed_and_must_be_storable_and_not_the_input() {
        assert_eq!(checked_name("  by_month ", "orders").unwrap(), "by_month");
        assert!(checked_name("   ", "orders").is_err());
        assert!(checked_name(".hidden", "orders").is_err());
        assert!(checked_name("a/b", "orders").is_err());
        assert!(checked_name("orders", "orders").is_err());
    }

    #[test]
    fn a_recipe_reading_a_missing_column_is_refused_before_anything_is_written() {
        let recipe = ModelRecipe::new(vec![brightflow_types::Operation::Sort {
            by: "nope".into(),
            descending: false,
        }]);
        let err = checked_recipe(&recipe, &["region".to_string()]).unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)), "{err}");
        let ok = ModelRecipe::new(vec![brightflow_types::Operation::Limit { n: 5 }]);
        assert!(checked_recipe(&ok, &["region".to_string()]).is_ok());
    }
}
