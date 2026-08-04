//! Column-semantics actions: KPI flag and polarity, with field-preserving
//! upserts so a mutation never wipes the fields it did not touch.

use serde_json::json;

use super::table_ctx;
use crate::actions::types::UndoOp;
use crate::shared::AppResult;
use crate::state::cache_key;
use crate::state::AppState;

pub(crate) async fn execute_set_kpi(
    state: &AppState,
    source_id: &str,
    table: &str,
    column: &str,
    is_kpi: bool,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    let previous = upsert_semantic_preserving(state, source_id, table, column, |s| {
        s.is_kpi = is_kpi;
    })
    .await?;
    Ok((
        json!({ "column": column, "isKpi": is_kpi }),
        Some(UndoOp::RestoreKpi {
            source_id: source_id.to_string(),
            table: table.to_string(),
            column: column.to_string(),
            role: previous.role,
            is_kpi: previous.is_kpi,
        }),
    ))
}

pub(crate) async fn execute_set_column_polarity(
    state: &AppState,
    source_id: &str,
    table: &str,
    column: &str,
    polarity: &crate::actions::types::ColumnPolarity,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    let previous = upsert_semantic_preserving(state, source_id, table, column, |s| {
        s.polarity = polarity.as_str().to_string();
    })
    .await?;
    Ok((
        json!({ "column": column, "polarity": polarity.as_str() }),
        Some(UndoOp::RestorePolarity {
            source_id: source_id.to_string(),
            table: table.to_string(),
            column: column.to_string(),
            polarity: previous.polarity,
        }),
    ))
}

pub(crate) async fn undo_restore_kpi(
    state: &AppState,
    source_id: &str,
    table: &str,
    column: &str,
    role: &str,
    is_kpi: bool,
) -> AppResult<()> {
    upsert_semantic_preserving(state, source_id, table, column, |s| {
        s.role = role.to_string();
        s.is_kpi = is_kpi;
    })
    .await?;
    Ok(())
}

pub(crate) async fn undo_restore_polarity(
    state: &AppState,
    source_id: &str,
    table: &str,
    column: &str,
    polarity: &str,
) -> AppResult<()> {
    upsert_semantic_preserving(state, source_id, table, column, |s| {
        s.polarity = polarity.to_string();
    })
    .await?;
    Ok(())
}

/// One column's full semantic tuple — what `upsert_semantic_preserving`
/// snapshots and mutates.
#[derive(Debug, Clone)]
struct SemanticSnapshot {
    role: String,
    is_kpi: bool,
    polarity: String,
    label: Option<String>,
    description: Option<String>,
}

impl Default for SemanticSnapshot {
    fn default() -> Self {
        Self {
            role: "measure".to_string(),
            is_kpi: false,
            polarity: "neutral".to_string(),
            label: None,
            description: None,
        }
    }
}

/// Mutate one column's semantics while PRESERVING every field the mutation
/// does not touch, then refresh the in-memory schema overrides + cache so the
/// next analysis run sees the change without a restart.
///
/// This is the fix for the old `SetKpi` path, which wrote `label = NULL,
/// description = NULL` and never touched `state.schema_overrides` — the KPI
/// flag looked applied but the running engine kept the stale schema. Both
/// `set_kpi` and `set_column_polarity` (and their undos) come through here.
///
/// Returns the PREVIOUS snapshot for undo capture.
async fn upsert_semantic_preserving(
    state: &AppState,
    source_id: &str,
    table: &str,
    column: &str,
    mutate: impl FnOnce(&mut SemanticSnapshot),
) -> AppResult<SemanticSnapshot> {
    let (store, table_id) = table_ctx(state, source_id, table).await?;
    let semantics = store.db().get_column_semantics(&table_id).await?;
    let previous = semantics
        .iter()
        .find(|r| r.column_name == column)
        .map_or_else(SemanticSnapshot::default, |r| SemanticSnapshot {
            role: r.role.clone(),
            is_kpi: r.is_kpi,
            polarity: r.polarity.clone(),
            label: r.label.clone(),
            description: r.description.clone(),
        });
    let mut next = previous.clone();
    mutate(&mut next);
    store
        .db()
        .upsert_column_semantic(
            &table_id,
            column,
            &next.role,
            next.is_kpi,
            &next.polarity,
            next.label.as_deref(),
            next.description.as_deref(),
        )
        .await?;

    // Keep the running engine honest: update the in-memory override for this
    // column and drop the cached schema.
    let key = cache_key(source_id, table);
    state.invalidate_schema_cache(&key);
    if let Some(role) = brightflow_engine::data::config::ColumnRole::parse(&next.role) {
        let mut overrides = state
            .schema_overrides
            .get(&key)
            .map(|v| v.value().clone())
            .unwrap_or_default();
        overrides.retain(|o| o.column_name != column);
        overrides.push(brightflow_engine::data::merge::ColumnOverride {
            column_name: column.to_string(),
            role,
            is_kpi: next.is_kpi,
            polarity: brightflow_engine::data::config::Polarity::parse(&next.polarity)
                .unwrap_or_default(),
            label: next.label.clone(),
            description: next.description.clone(),
        });
        state.schema_overrides.insert(key, overrides);
    }
    Ok(previous)
}
