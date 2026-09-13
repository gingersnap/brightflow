//! Column-semantics actions: role, KPI flag, polarity, label and description.
//!
//! Every action writes one *opinion row* at the actor's own layer — `user`
//! for a person, `agent` for an LLM run — touching only the field it names.
//! The store resolves that row over whatever a connector, an enrichment
//! function or the detector declared beneath it, so a person who renames a
//! column keeps the connector's description, and a connector re-declaring on
//! the next sync cannot overwrite the rename. There is no seeding from
//! detection here any more: the detector is a producer with its own layer.
//!
//! Undo puts the actor's row back exactly as it was before the action — or
//! deletes it, when the action created it. Rows logged before layers existed
//! carry the old full-tuple snapshot and restore it at the user layer.

use brightflow_types::{
    ColumnOpinion, ColumnRole, Layer, Polarity, Provenance, ResolvedColumn, TableOpinion,
};
use serde_json::json;

use super::table_ctx;
use crate::actions::types::{ColumnSemanticSnapshot, TableSettingsSnapshot, UndoOp};
use crate::actions::Actor;
use crate::shared::AppResult;
use crate::state::AppState;

/// The producer a legacy (pre-layer) undo row restores into.
const LEGACY_PRODUCER: &str = "user:legacy";

/// The layer and producer an actor's edits are filed under.
pub(crate) fn provenance_for_actor(actor: &Actor) -> Provenance {
    match actor {
        Actor::Human { user_id } => Provenance {
            layer: Layer::User,
            producer: format!("user:{user_id}"),
            version: None,
            hash: None,
        },
        Actor::Agent { run_id, .. } => Provenance {
            layer: Layer::Agent,
            producer: format!("agent:{run_id}"),
            version: None,
            hash: None,
        },
    }
}

pub(crate) async fn execute_set_kpi(
    state: &AppState,
    actor: &Actor,
    source_id: &str,
    table: &str,
    column: &str,
    is_kpi: bool,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    let (previous, prov) = mutate_opinion(state, actor, source_id, table, column, |o| {
        o.ext.is_kpi = Some(is_kpi);
    })
    .await?;
    Ok((
        json!({ "column": column, "isKpi": is_kpi }),
        Some(restore_op(source_id, table, column, previous, prov)),
    ))
}

pub(crate) async fn execute_set_column_polarity(
    state: &AppState,
    actor: &Actor,
    source_id: &str,
    table: &str,
    column: &str,
    polarity: Polarity,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    let (previous, prov) = mutate_opinion(state, actor, source_id, table, column, |o| {
        o.ext.polarity = Some(polarity);
    })
    .await?;
    Ok((
        json!({ "column": column, "polarity": polarity.as_str() }),
        Some(restore_op(source_id, table, column, previous, prov)),
    ))
}

/// A KPI is a measure by definition, so moving a column to any other role
/// also says "not a KPI" at this layer, rather than leaving a lower layer's
/// KPI flag showing through on a dimension.
pub(crate) async fn execute_set_column_role(
    state: &AppState,
    actor: &Actor,
    source_id: &str,
    table: &str,
    column: &str,
    role: ColumnRole,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    let was_kpi = resolved_column(state, source_id, table, column)
        .await?
        .and_then(|c| c.is_kpi)
        .unwrap_or(false);
    let (previous, prov) = mutate_opinion(state, actor, source_id, table, column, |o| {
        o.ext.role = Some(role);
        if role != ColumnRole::Measure {
            o.ext.is_kpi = Some(false);
        }
    })
    .await?;
    let kpi_cleared = was_kpi && role != ColumnRole::Measure;
    Ok((
        json!({ "column": column, "role": role.as_str(), "kpiCleared": kpi_cleared }),
        Some(restore_op(source_id, table, column, previous, prov)),
    ))
}

pub(crate) async fn execute_set_column_label(
    state: &AppState,
    actor: &Actor,
    source_id: &str,
    table: &str,
    column: &str,
    label: Option<&str>,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    // A blank label is stored as "" — "cleared" — so it also hides a lower
    // layer's label; `None` would mean "no opinion" and let it through.
    let stored = trimmed(label);
    let (previous, prov) = mutate_opinion(state, actor, source_id, table, column, |o| {
        o.ext.label = Some(stored.clone());
    })
    .await?;
    Ok((
        json!({ "column": column, "label": non_blank(label) }),
        Some(restore_op(source_id, table, column, previous, prov)),
    ))
}

pub(crate) async fn execute_set_column_description(
    state: &AppState,
    actor: &Actor,
    source_id: &str,
    table: &str,
    column: &str,
    description: Option<&str>,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    let stored = trimmed(description);
    let (previous, prov) = mutate_opinion(state, actor, source_id, table, column, |o| {
        o.description = Some(stored.clone());
    })
    .await?;
    Ok((
        json!({ "column": column, "description": non_blank(description) }),
        Some(restore_op(source_id, table, column, previous, prov)),
    ))
}

/// Forget every edit to a column: the user and agent rows go, so whatever a
/// connector, an enrichment function or the detector declared shows again.
/// The removed rows ride on the undo op.
pub(crate) async fn execute_reset_column_semantics(
    state: &AppState,
    source_id: &str,
    table: &str,
    column: &str,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    let (store, table_id) = table_ctx(state, source_id, table).await?;
    let removed = store
        .db()
        .delete_column_opinions_at_layers(&table_id, column, &[Layer::User, Layer::Agent])
        .await?;
    state.refresh_overrides_from_store(source_id, table).await;
    let resolved = resolved_column(state, source_id, table, column).await?;
    Ok((
        json!({
            "column": column,
            "removed": removed.len(),
            "resolvedBy": resolved.and_then(|c| c.resolved_by),
        }),
        Some(UndoOp::RestoreColumnOpinions {
            source_id: source_id.to_string(),
            table: table.to_string(),
            column: column.to_string(),
            rows: removed,
        }),
    ))
}

/// Put the rows a reset removed back exactly as they were.
pub(crate) async fn undo_restore_column_opinions(
    state: &AppState,
    source_id: &str,
    table: &str,
    column: &str,
    rows: &[ColumnOpinion],
) -> AppResult<()> {
    let (store, table_id) = table_ctx(state, source_id, table).await?;
    for row in rows.iter().filter(|r| r.column == column) {
        store.db().write_column_opinion(&table_id, row).await?;
    }
    state.refresh_overrides_from_store(source_id, table).await;
    Ok(())
}

/// Set the table's own settings as one opinion row at the actor's layer.
/// Fields given replace the actor's earlier values; fields left `None` keep
/// what the actor's row already said, so a caller can change the period
/// without restating the display name. A blank text field is stored as
/// "" — "cleared" — so it hides a lower layer's value.
pub(crate) async fn execute_set_table_settings(
    state: &AppState,
    actor: &Actor,
    source_id: &str,
    table: &str,
    settings: &TableSettingsSnapshot,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    let prov = provenance_for_actor(actor);
    let (store, table_id) = table_ctx(state, source_id, table).await?;
    let previous = store
        .db()
        .table_opinions(&table_id)
        .await?
        .into_iter()
        .find(|o| o.provenance.layer == prov.layer && o.provenance.producer == prov.producer);
    let mut next = previous
        .clone()
        .unwrap_or_else(|| TableOpinion::empty(prov.clone()));
    if let Some(v) = &settings.display_name {
        next.display_name = Some(v.trim().to_string());
    }
    if let Some(v) = &settings.description {
        next.description = Some(v.trim().to_string());
    }
    if let Some(v) = settings.time_granularity {
        next.time_granularity = Some(v);
    }
    if let Some(v) = settings.comparison_periods {
        next.comparison_periods = Some(v);
    }
    store.db().write_table_opinion(&table_id, &next).await?;
    state.refresh_overrides_from_store(source_id, table).await;
    Ok((
        json!({
            "displayName": next.display_name,
            "description": next.description,
            "timeGranularity": next.time_granularity,
            "comparisonPeriods": next.comparison_periods,
        }),
        Some(UndoOp::RestoreTableSettings {
            source_id: source_id.to_string(),
            table: table.to_string(),
            snapshot: previous
                .as_ref()
                .map(TableSettingsSnapshot::from_opinion)
                .unwrap_or_default(),
            provenance: prov,
            existed: previous.is_some(),
        }),
    ))
}

/// Put the actor's table row back as it was, or delete it when the action
/// created it.
pub(crate) async fn undo_restore_table_settings(
    state: &AppState,
    source_id: &str,
    table: &str,
    snapshot: &TableSettingsSnapshot,
    provenance: &Provenance,
    existed: bool,
) -> AppResult<()> {
    let (store, table_id) = table_ctx(state, source_id, table).await?;
    if existed {
        let mut opinion = TableOpinion::empty(provenance.clone());
        snapshot.apply_to(&mut opinion);
        store.db().write_table_opinion(&table_id, &opinion).await?;
    } else {
        store
            .db()
            .delete_table_opinion(&table_id, provenance)
            .await?;
    }
    state.refresh_overrides_from_store(source_id, table).await;
    Ok(())
}

/// Legacy undo (rows logged before layers): restore role and KPI at the
/// user layer.
pub(crate) async fn undo_restore_kpi(
    state: &AppState,
    source_id: &str,
    table: &str,
    column: &str,
    role: &str,
    is_kpi: bool,
) -> AppResult<()> {
    let role = ColumnRole::parse(role);
    write_opinion(state, &legacy_provenance(), source_id, table, column, |o| {
        o.ext.role = role;
        o.ext.is_kpi = Some(is_kpi);
    })
    .await
}

/// Legacy undo: restore polarity at the user layer.
pub(crate) async fn undo_restore_polarity(
    state: &AppState,
    source_id: &str,
    table: &str,
    column: &str,
    polarity: &str,
) -> AppResult<()> {
    let polarity = Polarity::parse(polarity);
    write_opinion(state, &legacy_provenance(), source_id, table, column, |o| {
        o.ext.polarity = polarity;
    })
    .await
}

/// Put the actor's row back as it was: rewrite it from the snapshot, or
/// delete it when the action created it.
pub(crate) async fn undo_restore_column_semantic(
    state: &AppState,
    source_id: &str,
    table: &str,
    column: &str,
    snapshot: &ColumnSemanticSnapshot,
    provenance: Option<&Provenance>,
    existed: bool,
) -> AppResult<()> {
    let prov = provenance.cloned().unwrap_or_else(legacy_provenance);
    let (store, table_id) = table_ctx(state, source_id, table).await?;
    if existed {
        let mut opinion = ColumnOpinion::empty(column, prov);
        snapshot.apply_to(&mut opinion);
        store.db().write_column_opinion(&table_id, &opinion).await?;
    } else {
        store
            .db()
            .delete_column_opinion(&table_id, column, &prov)
            .await?;
    }
    state.refresh_overrides_from_store(source_id, table).await;
    Ok(())
}

fn legacy_provenance() -> Provenance {
    Provenance {
        layer: Layer::User,
        producer: LEGACY_PRODUCER.to_string(),
        version: None,
        hash: None,
    }
}

fn restore_op(
    source_id: &str,
    table: &str,
    column: &str,
    previous: Option<ColumnOpinion>,
    prov: Provenance,
) -> UndoOp {
    UndoOp::RestoreColumnSemantic {
        source_id: source_id.to_string(),
        table: table.to_string(),
        column: column.to_string(),
        snapshot: previous
            .as_ref()
            .map(ColumnSemanticSnapshot::from_opinion)
            .unwrap_or_default(),
        provenance: Some(prov),
        existed: previous.is_some(),
    }
}

/// Trim free text; blank becomes the empty string.
fn trimmed(value: Option<&str>) -> String {
    value.map(str::trim).unwrap_or_default().to_string()
}

/// Trim free text; blank means "none" in the response.
fn non_blank(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

async fn resolved_column(
    state: &AppState,
    source_id: &str,
    table: &str,
    column: &str,
) -> AppResult<Option<ResolvedColumn>> {
    let store = state.require_store()?;
    Ok(store
        .resolved_columns(source_id, table)
        .await?
        .into_iter()
        .find(|c| c.name == column))
}

/// Mutate the actor's opinion row for one column, creating it if absent,
/// then refresh the in-memory overrides so the next analysis run and the
/// next `load_table` see the change. Returns the row as it was before (for
/// undo) and the provenance it was written under.
async fn mutate_opinion(
    state: &AppState,
    actor: &Actor,
    source_id: &str,
    table: &str,
    column: &str,
    mutate: impl FnOnce(&mut ColumnOpinion),
) -> AppResult<(Option<ColumnOpinion>, Provenance)> {
    let prov = provenance_for_actor(actor);
    let (store, table_id) = table_ctx(state, source_id, table).await?;
    let previous = store.db().column_opinion(&table_id, column, &prov).await?;
    let mut next = previous
        .clone()
        .unwrap_or_else(|| ColumnOpinion::empty(column, prov.clone()));
    mutate(&mut next);
    store.db().write_column_opinion(&table_id, &next).await?;
    state.refresh_overrides_from_store(source_id, table).await;
    Ok((previous, prov))
}

async fn write_opinion(
    state: &AppState,
    prov: &Provenance,
    source_id: &str,
    table: &str,
    column: &str,
    mutate: impl FnOnce(&mut ColumnOpinion),
) -> AppResult<()> {
    let (store, table_id) = table_ctx(state, source_id, table).await?;
    let mut next = store
        .db()
        .column_opinion(&table_id, column, prov)
        .await?
        .unwrap_or_else(|| ColumnOpinion::empty(column, prov.clone()));
    mutate(&mut next);
    store.db().write_column_opinion(&table_id, &next).await?;
    state.refresh_overrides_from_store(source_id, table).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actors_map_to_their_own_layer() {
        let human = provenance_for_actor(&Actor::Human {
            user_id: "u1".into(),
        });
        assert_eq!(human.layer, Layer::User);
        assert_eq!(human.producer, "user:u1");
        let agent = provenance_for_actor(&Actor::Agent {
            run_id: 7,
            auto_apply: true,
        });
        assert_eq!(agent.layer, Layer::Agent);
        assert_eq!(agent.producer, "agent:7");
    }

    #[test]
    fn trimming_and_blank_handling() {
        assert_eq!(trimmed(Some("  Revenue ")), "Revenue");
        assert_eq!(trimmed(Some("   ")), "");
        assert_eq!(trimmed(None), "");
        assert_eq!(non_blank(Some("  Revenue ")), Some("Revenue".to_string()));
        assert_eq!(non_blank(Some("   ")), None);
    }

    #[test]
    fn restore_op_records_whether_the_row_existed() {
        let prov = provenance_for_actor(&Actor::Human {
            user_id: "u1".into(),
        });
        let created = restore_op("s", "t", "c", None, prov.clone());
        assert!(matches!(
            created,
            UndoOp::RestoreColumnSemantic { existed: false, .. }
        ));
        let mut row = ColumnOpinion::empty("c", prov.clone());
        row.ext.label = Some("L".into());
        let edited = restore_op("s", "t", "c", Some(row), prov);
        match edited {
            UndoOp::RestoreColumnSemantic {
                existed, snapshot, ..
            } => {
                assert!(existed);
                assert_eq!(snapshot.label.as_deref(), Some("L"));
            },
            _ => panic!("wrong op"),
        }
    }
}
