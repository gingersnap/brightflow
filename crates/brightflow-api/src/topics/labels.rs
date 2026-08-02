//! Curated row labels: SQLite → engine.
//!
//! The engine is deliberately storage-agnostic, so it never reads the database.
//! This module is the bridge: it loads curated `document_labels` and aligns
//! them with a DataFrame's row order, producing the [`LabelTargets`] that
//! `fit_topics` trains the classifier head on.
//!
//! Alignment is by row id, not position — parquet row order is not stable
//! across syncs, so a positional join would silently attach each label to some
//! other ticket and train a head on noise.

use std::collections::HashMap;

use brightflow_engine::enrichment::LabelTargets;
use polars::prelude::*;

use crate::topics::display::DocDisplay;

/// Read a table's id column as strings, coercing numeric ids.
pub fn read_row_ids(df: &DataFrame, id_column: &str) -> Option<Vec<String>> {
    let series = df.column(id_column).ok()?.as_materialized_series().clone();
    let str_series = if series.dtype() == &DataType::String {
        series
    } else {
        // `issues.id` is an integer while `posts.uri` is a string — coerce
        // rather than fail, since the id is only ever used as a join key.
        series.cast(&DataType::String).ok()?
    };
    let ca = str_series.str().ok()?.clone();
    Some(
        ca.into_iter()
            .map(|o| o.unwrap_or_default().to_string())
            .collect(),
    )
}

/// Load curated labels for `table_name`, aligned with `df`'s rows.
///
/// Returns `None` when nothing is curated yet — the caller then falls back to
/// the table's own `label_names` column, preserving pre-taxonomy behaviour.
pub async fn load_label_targets(
    store: &brightflow_store::ParquetStore,
    source_id: &str,
    table_name: &str,
    df: &DataFrame,
) -> Option<LabelTargets> {
    let table_row = store.db().get_table(source_id, table_name).await.ok()??;
    let labels = store.db().get_document_labels(&table_row.id).await.ok()?;
    if labels.is_empty() {
        return None;
    }

    let mut by_row: HashMap<String, Vec<String>> = HashMap::new();
    for label in labels {
        by_row.entry(label.row_id).or_default().push(label.name);
    }

    let display = DocDisplay::for_table(table_name);
    let ids = read_row_ids(df, display.id_column)?;

    let rows: Vec<Vec<String>> = ids
        .iter()
        .map(|id| by_row.get(id).cloned().unwrap_or_default())
        .collect();

    let targets = LabelTargets::from_row_labels(&rows);
    if targets.is_empty() {
        None
    } else {
        Some(targets)
    }
}

#[cfg(test)]
mod tests {
    use super::read_row_ids;
    use polars::prelude::*;

    #[test]
    fn reads_string_ids() {
        let df = DataFrame::new(vec![Column::new("id".into(), vec!["at://a", "at://b"])]).unwrap();
        assert_eq!(
            read_row_ids(&df, "id").unwrap(),
            vec!["at://a".to_string(), "at://b".to_string()]
        );
    }

    #[test]
    fn coerces_numeric_ids() {
        // `issues.id` is an integer column; labels store row ids as text.
        let df = DataFrame::new(vec![Column::new("id".into(), vec![101i64, 102])]).unwrap();
        assert_eq!(
            read_row_ids(&df, "id").unwrap(),
            vec!["101".to_string(), "102".to_string()]
        );
    }

    #[test]
    fn missing_column_yields_none() {
        let df = DataFrame::new(vec![Column::new("other".into(), vec![1i64])]).unwrap();
        assert!(read_row_ids(&df, "id").is_none());
    }
}
