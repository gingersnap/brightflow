//! Row-id alignment for curated labels.
//!
//! The engine never reads the database; callers (API, CLI) fetch the curated
//! `document_labels` rows and hand them to [`align_label_targets`], which
//! aligns them with a DataFrame's row order into the [`LabelTargets`] that
//! `fit_topics` trains the classifier head on.
//!
//! Alignment is by row id, not position — parquet row order is not stable
//! across syncs, so a positional join would silently attach each label to
//! some other ticket and train a head on noise.

use std::collections::HashMap;

use polars::prelude::*;

use super::topic_enricher::LabelTargets;

/// The id column labels join on, per table type. Must agree with how the
/// labelling agents recorded `row_id` for that table.
#[must_use]
pub fn label_join_id_column(table_name: &str) -> &'static str {
    match table_name {
        "posts" => "uri",
        _ => "id",
    }
}

/// Read a table's id column as strings, coercing numeric ids.
#[must_use]
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

/// Align curated `(row_id, label_name)` pairs with `df`'s rows.
///
/// Returns `None` when nothing aligns — the caller then falls back to the
/// table's own label column, preserving pre-taxonomy behaviour.
#[must_use]
pub fn align_label_targets(
    df: &DataFrame,
    table_name: &str,
    labels: &[(String, String)],
) -> Option<LabelTargets> {
    if labels.is_empty() {
        return None;
    }
    let mut by_row: HashMap<&str, Vec<String>> = HashMap::new();
    for (row_id, name) in labels {
        by_row.entry(row_id).or_default().push(name.clone());
    }

    let ids = read_row_ids(df, label_join_id_column(table_name))?;
    let rows: Vec<Vec<String>> = ids
        .iter()
        .map(|id| by_row.get(id.as_str()).cloned().unwrap_or_default())
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
    use super::*;

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

    #[test]
    fn join_column_per_table() {
        assert_eq!(label_join_id_column("issues"), "id");
        assert_eq!(label_join_id_column("posts"), "uri");
        assert_eq!(label_join_id_column("anything_else"), "id");
    }

    #[test]
    fn align_joins_by_id_not_position() {
        let df = DataFrame::new(vec![Column::new("id".into(), vec![7i64, 8, 9])]).unwrap();
        // Labels arrive in a different order than the rows.
        let labels = vec![
            ("9".to_string(), "bug".to_string()),
            ("7".to_string(), "feature".to_string()),
            ("9".to_string(), "urgent".to_string()),
        ];
        let targets = align_label_targets(&df, "issues", &labels).unwrap();
        assert_eq!(targets.per_row.len(), 3);
        assert!(targets.per_row[1].is_empty(), "row 8 has no labels");
        assert_eq!(targets.per_row[2].len(), 2, "row 9 has two labels");
    }

    #[test]
    fn align_returns_none_for_empty_or_unmatched() {
        let df = DataFrame::new(vec![Column::new("id".into(), vec![1i64])]).unwrap();
        assert!(align_label_targets(&df, "issues", &[]).is_none());
        let unmatched = vec![("999".to_string(), "bug".to_string())];
        assert!(align_label_targets(&df, "issues", &unmatched).is_none());
    }
}
