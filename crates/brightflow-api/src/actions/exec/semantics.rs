//! Column-semantics actions: role, KPI flag, polarity, label and description,
//! with field-preserving upserts so a mutation never wipes the fields it did
//! not touch.
//!
//! A column with no `column_semantics` row is seeded from what the engine's
//! detector says about it before the mutation applies, so flagging a string
//! column as KPI cannot silently declare it a measure.

use brightflow_engine::data::config::{ColumnRole, Polarity};
use brightflow_engine::data::merge::ColumnOverride;
use brightflow_engine::data::schema::{detect_schema, DataSchema};
use polars::prelude::*;
use serde_json::json;

use super::table_ctx;
use crate::actions::types::{ColumnSemanticSnapshot, UndoOp};
use crate::shared::{AppError, AppResult};
use crate::state::cache_key;
use crate::state::AppState;

/// Rows the role detector samples when a column has no stored semantics yet.
/// Enough for the detector's cardinality rule to settle; small enough that a
/// first-time write on a large table stays interactive.
const DETECT_SAMPLE_ROWS: u32 = 10_000;

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
            role: previous.role.as_str().to_string(),
            is_kpi: previous.is_kpi,
        }),
    ))
}

pub(crate) async fn execute_set_column_polarity(
    state: &AppState,
    source_id: &str,
    table: &str,
    column: &str,
    polarity: Polarity,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    let previous = upsert_semantic_preserving(state, source_id, table, column, |s| {
        s.polarity = polarity;
    })
    .await?;
    Ok((
        json!({ "column": column, "polarity": polarity.as_str() }),
        Some(UndoOp::RestorePolarity {
            source_id: source_id.to_string(),
            table: table.to_string(),
            column: column.to_string(),
            polarity: previous.polarity.as_str().to_string(),
        }),
    ))
}

/// A KPI is a measure by definition, so moving a KPI column to any other
/// role clears the flag rather than leaving a KPI dimension behind.
pub(crate) async fn execute_set_column_role(
    state: &AppState,
    source_id: &str,
    table: &str,
    column: &str,
    role: ColumnRole,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    let previous = upsert_semantic_preserving(state, source_id, table, column, |s| {
        s.role = role;
        if role != ColumnRole::Measure {
            s.is_kpi = false;
        }
    })
    .await?;
    let kpi_cleared = previous.is_kpi && role != ColumnRole::Measure;
    Ok((
        json!({ "column": column, "role": role.as_str(), "kpiCleared": kpi_cleared }),
        Some(restore_op(source_id, table, column, previous)),
    ))
}

pub(crate) async fn execute_set_column_label(
    state: &AppState,
    source_id: &str,
    table: &str,
    column: &str,
    label: Option<&str>,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    let label = non_blank(label);
    let previous = upsert_semantic_preserving(state, source_id, table, column, |s| {
        s.label.clone_from(&label);
    })
    .await?;
    Ok((
        json!({ "column": column, "label": label }),
        Some(restore_op(source_id, table, column, previous)),
    ))
}

pub(crate) async fn execute_set_column_description(
    state: &AppState,
    source_id: &str,
    table: &str,
    column: &str,
    description: Option<&str>,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    let description = non_blank(description);
    let previous = upsert_semantic_preserving(state, source_id, table, column, |s| {
        s.description.clone_from(&description);
    })
    .await?;
    Ok((
        json!({ "column": column, "description": description }),
        Some(restore_op(source_id, table, column, previous)),
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
    let role = ColumnRole::parse(role)
        .ok_or_else(|| AppError::BadRequest(format!("unknown stored role '{role}'")))?;
    upsert_semantic_preserving(state, source_id, table, column, |s| {
        s.role = role;
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
    let polarity = Polarity::parse(polarity)
        .ok_or_else(|| AppError::BadRequest(format!("unknown stored polarity '{polarity}'")))?;
    upsert_semantic_preserving(state, source_id, table, column, |s| {
        s.polarity = polarity;
    })
    .await?;
    Ok(())
}

pub(crate) async fn undo_restore_column_semantic(
    state: &AppState,
    source_id: &str,
    table: &str,
    column: &str,
    snapshot: &ColumnSemanticSnapshot,
) -> AppResult<()> {
    upsert_semantic_preserving(state, source_id, table, column, |s| {
        *s = snapshot.clone();
    })
    .await?;
    Ok(())
}

fn restore_op(
    source_id: &str,
    table: &str,
    column: &str,
    snapshot: ColumnSemanticSnapshot,
) -> UndoOp {
    UndoOp::RestoreColumnSemantic {
        source_id: source_id.to_string(),
        table: table.to_string(),
        column: column.to_string(),
        snapshot,
    }
}

/// Trim free text; blank means "clear".
fn non_blank(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Mutate one column's semantics while PRESERVING every field the mutation
/// does not touch, then refresh the in-memory override so the next analysis
/// run and the next `load_table` see the change without a restart.
///
/// A column with no row yet starts from the detected snapshot, not from a
/// fixed default. Every semantic execute and undo comes through here.
///
/// Returns the PREVIOUS snapshot for undo capture.
async fn upsert_semantic_preserving(
    state: &AppState,
    source_id: &str,
    table: &str,
    column: &str,
    mutate: impl FnOnce(&mut ColumnSemanticSnapshot),
) -> AppResult<ColumnSemanticSnapshot> {
    let (store, table_id) = table_ctx(state, source_id, table).await?;
    let semantics = store.db().get_column_semantics(&table_id).await?;
    let stored = semantics.iter().find(|r| r.column_name == column);
    let previous = match stored {
        Some(r) => ColumnSemanticSnapshot {
            role: ColumnRole::parse(&r.role)
                .ok_or_else(|| AppError::Internal(format!("unknown stored role '{}'", r.role)))?,
            is_kpi: r.is_kpi,
            polarity: Polarity::parse(&r.polarity).unwrap_or_default(),
            label: r.label.clone(),
            description: r.description.clone(),
        },
        None => ColumnSemanticSnapshot {
            role: detected_role(&store, source_id, table, column).await?,
            is_kpi: false,
            polarity: Polarity::Neutral,
            label: None,
            description: None,
        },
    };
    let mut next = previous.clone();
    mutate(&mut next);
    store
        .db()
        .upsert_column_semantic(
            &table_id,
            column,
            next.role.as_str(),
            next.is_kpi,
            next.polarity.as_str(),
            next.label.as_deref(),
            next.description.as_deref(),
        )
        .await?;

    state.set_column_override(
        &cache_key(source_id, table),
        ColumnOverride {
            column_name: column.to_string(),
            role: next.role,
            is_kpi: next.is_kpi,
            polarity: next.polarity,
            label: next.label.clone(),
            description: next.description.clone(),
        },
    );
    Ok(previous)
}

/// What the engine's detector would call this column, from a sample of the
/// table. The column must exist in the table's schema.
async fn detected_role(
    store: &brightflow_store::ParquetStore,
    source_id: &str,
    table: &str,
    column: &str,
) -> AppResult<ColumnRole> {
    let files = store.get_table_parquet_paths(source_id, table).await?;
    let column = column.to_string();
    tokio::task::spawn_blocking(move || {
        let df = LazyFrame::scan_parquet_files(files.into(), ScanArgsParquet::default())?
            .limit(DETECT_SAMPLE_ROWS)
            .collect()?;
        let dtype = df
            .column(&column)
            .map_err(|_| AppError::NotFound(format!("column '{column}' not in table")))?
            .dtype()
            .clone();
        let schema = detect_schema(&df).map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(role_from_detected(&schema, &column, &dtype))
    })
    .await
    .map_err(|e| AppError::Internal(format!("role detection task failed: {e}")))?
}

/// Map the detector's column lists back to one column's role. The detector
/// records a name-based time guess in `time_column` separately from the
/// dtype-based `time_columns`; both count as `Time` here. Columns the
/// detector left out of every list fall back on dtype.
fn role_from_detected(schema: &DataSchema, column: &str, dtype: &DataType) -> ColumnRole {
    let is = |names: &[String]| names.iter().any(|n| n == column);
    if schema.time_column.as_deref() == Some(column) || is(&schema.time_columns) {
        ColumnRole::Time
    } else if is(&schema.measure_columns) {
        ColumnRole::Measure
    } else if is(&schema.dimension_columns) {
        ColumnRole::Dimension
    } else if dtype.is_primitive_numeric() {
        ColumnRole::Measure
    } else if dtype.is_temporal() {
        ColumnRole::Time
    } else {
        ColumnRole::Dimension
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema(measures: &[&str], dims: &[&str], times: &[&str], time: Option<&str>) -> DataSchema {
        let v = |xs: &[&str]| xs.iter().map(|s| (*s).to_string()).collect();
        DataSchema {
            measure_columns: v(measures),
            kpi_columns: Vec::new(),
            dimension_columns: v(dims),
            time_columns: v(times),
            time_column: time.map(str::to_string),
            time_granularity: brightflow_engine::data::config::TimeGranularity::default(),
            polarity: std::collections::HashMap::default(),
        }
    }

    #[test]
    fn detected_role_follows_the_detector_lists() {
        let s = schema(
            &["revenue"],
            &["region", "order_date"],
            &[],
            Some("order_date"),
        );
        assert_eq!(
            role_from_detected(&s, "revenue", &DataType::Float64),
            ColumnRole::Measure
        );
        assert_eq!(
            role_from_detected(&s, "region", &DataType::String),
            ColumnRole::Dimension
        );
        // A string column the detector guessed as the time axis by name is
        // Time, even though the detector also lists it as a dimension.
        assert_eq!(
            role_from_detected(&s, "order_date", &DataType::String),
            ColumnRole::Time
        );
    }

    #[test]
    fn detected_role_falls_back_on_dtype_for_unlisted_columns() {
        let s = schema(&[], &[], &[], None);
        assert_eq!(
            role_from_detected(&s, "flag", &DataType::Boolean),
            ColumnRole::Dimension
        );
        assert_eq!(
            role_from_detected(&s, "n", &DataType::Int64),
            ColumnRole::Measure
        );
        assert_eq!(
            role_from_detected(&s, "when", &DataType::Date),
            ColumnRole::Time
        );
    }

    #[test]
    fn non_blank_trims_and_clears() {
        assert_eq!(non_blank(Some("  Revenue ")), Some("Revenue".to_string()));
        assert_eq!(non_blank(Some("   ")), None);
        assert_eq!(non_blank(None), None);
    }
}
