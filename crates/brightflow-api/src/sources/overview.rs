//! Per-table signals for a source's Overview page: what would make a person
//! open a table, beside what the table is.
//!
//! The unified source list already carries a table's name, display name,
//! description and row count. This adds the signals that change: when the
//! table last changed, how much of it is described, how many proposals wait
//! on it, and its latest insight run. Fetched only for the Overview, so the
//! source list stays cheap.

use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;
use ts_rs::TS;

use brightflow_types::ResolvedColumn;

use crate::insights::types::InsightRunResponse;
use crate::shared::AppResult;
use crate::state::AppState;

/// How much of a table's columns the semantics cover.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SemanticCoverage {
    /// Columns in the stored schema.
    pub columns: usize,
    /// Columns some layer gave a role.
    pub with_role: usize,
    /// Columns some layer described.
    pub described: usize,
}

/// One table's signals, keyed by name to the source list's table.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TableSignals {
    pub name: String,
    /// When the table's data last changed, as the catalog records it.
    pub updated_at: String,
    pub coverage: SemanticCoverage,
    /// Proposals awaiting review that name this table.
    pub pending_proposals: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_insight_run: Option<InsightRunResponse>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SourceOverviewResponse {
    pub tables: Vec<TableSignals>,
}

/// Coverage over the schema's columns.
///
/// A column counts as described when a resolved row has a non-blank
/// description, and as placed when it has a role. Columns the semantics
/// know but the schema does not are ignored; they are declarations for
/// data that has not arrived.
pub fn coverage(schema_columns: &[String], resolved: &[ResolvedColumn]) -> SemanticCoverage {
    let mut with_role = 0;
    let mut described = 0;
    for name in schema_columns {
        let Some(column) = resolved.iter().find(|c| &c.name == name) else {
            continue;
        };
        if column.role.is_some() {
            with_role += 1;
        }
        if column
            .description
            .as_deref()
            .is_some_and(|d| !d.trim().is_empty())
        {
            described += 1;
        }
    }
    SemanticCoverage {
        columns: schema_columns.len(),
        with_role,
        described,
    }
}

/// The (source, table) a stored action names, from its params JSON. Every
/// scoped action carries both at the top level; an action without them
/// belongs to no table.
pub fn action_scope(params_json: &str) -> Option<(String, String)> {
    let value: serde_json::Value = serde_json::from_str(params_json).ok()?;
    let source = value.get("source_id")?.as_str()?.to_string();
    let table = value.get("table")?.as_str()?.to_string();
    Some((source, table))
}

/// `GET /api/sources/{source_id}/overview` — the signals for every table
/// of the source.
pub async fn source_overview(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
) -> AppResult<Json<SourceOverviewResponse>> {
    let store = state.require_store()?;
    let db = store.db();

    // Pending proposals per table, from one scan of the (bounded) queue.
    let mut pending: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for row in db.list_proposed_actions().await? {
        if let Some((source, table)) = action_scope(&row.params_json) {
            if source == source_id {
                *pending.entry(table).or_insert(0) += 1;
            }
        }
    }
    let latest_runs: std::collections::HashMap<String, InsightRunResponse> = db
        .latest_insight_runs_for_source(&source_id)
        .await?
        .into_iter()
        .map(InsightRunResponse::from_row)
        .map(|run| (run.table.clone(), run))
        .collect();

    let mut tables = Vec::new();
    for table in store.list_tables_by_source(&source_id).await? {
        let resolved = db.resolved_columns(&table.id).await?;
        tables.push(TableSignals {
            coverage: coverage(&table.column_names(), &resolved),
            pending_proposals: pending.get(&table.name).copied().unwrap_or(0),
            last_insight_run: latest_runs.get(&table.name).cloned(),
            updated_at: table.updated_at.clone(),
            name: table.name,
        });
    }
    Ok(Json(SourceOverviewResponse { tables }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use brightflow_types::ColumnRole;

    fn resolved(name: &str, role: Option<ColumnRole>, description: Option<&str>) -> ResolvedColumn {
        ResolvedColumn {
            name: name.to_string(),
            datatype: None,
            is_time: None,
            role,
            is_kpi: None,
            polarity: None,
            label: None,
            description: description.map(str::to_string),
            ai_context: None,
            resolved_by: None,
        }
    }

    #[test]
    fn coverage_counts_schema_columns_only_and_ignores_blank_descriptions() {
        let schema = vec!["id".to_string(), "title".to_string(), "body".to_string()];
        let rows = vec![
            resolved("id", Some(ColumnRole::Entity), Some("Row id")),
            resolved("title", Some(ColumnRole::Dimension), Some("  ")),
            resolved("ghost", Some(ColumnRole::Measure), Some("not in the data")),
        ];
        assert_eq!(
            coverage(&schema, &rows),
            SemanticCoverage {
                columns: 3,
                with_role: 2,
                described: 1,
            }
        );
        assert_eq!(
            coverage(&[], &rows),
            SemanticCoverage {
                columns: 0,
                with_role: 0,
                described: 0,
            }
        );
    }

    #[test]
    fn action_scope_reads_the_flattened_scope_fields() {
        assert_eq!(
            action_scope(r#"{"kind":"set_column_role","source_id":"s","table":"t","column":"c"}"#),
            Some(("s".to_string(), "t".to_string()))
        );
        assert_eq!(action_scope(r#"{"kind":"x"}"#), None);
        assert_eq!(action_scope("not json"), None);
    }
}
