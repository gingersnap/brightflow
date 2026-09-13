//! What changed between two declarations from the same producer.
//!
//! When a connector re-declares a table under a new version, the store keeps
//! the old rows only long enough to diff them against the new ones; the diff
//! is what a person can look at to see what the upgrade changed. One entry
//! per (column, field) that differs, plus one per table-level field; a
//! column that appears or disappears is one entry on the `column` field.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::resolved::{ColumnOpinion, TableOpinion};

/// One field that differs between two declarations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DeclarationChange {
    /// The column, or `None` for a table-level field.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub column: Option<String>,
    /// `datatype`, `role`, `label`, … or `column` for an added or removed
    /// column.
    pub field: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub from: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub to: Option<String>,
}

/// A producer's re-declaration, summarised.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DeclarationDiff {
    pub producer: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub from_version: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub to_version: Option<String>,
    pub changes: Vec<DeclarationChange>,
}

impl DeclarationDiff {
    /// The columns touched, in first-seen order, table-level entries left out.
    pub fn changed_columns(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for change in &self.changes {
            if let Some(column) = &change.column {
                if !out.contains(column) {
                    out.push(column.clone());
                }
            }
        }
        out
    }
}

fn text<T: std::fmt::Display>(value: Option<T>) -> Option<String> {
    value.map(|inner| inner.to_string())
}

fn push_if_changed(
    out: &mut Vec<DeclarationChange>,
    column: Option<&str>,
    field: &str,
    from: Option<String>,
    to: Option<String>,
) {
    if from != to {
        out.push(DeclarationChange {
            column: column.map(str::to_string),
            field: field.to_string(),
            from,
            to,
        });
    }
}

/// Field-by-field differences between two column opinions with the same
/// name.
fn diff_column(prev: &ColumnOpinion, next: &ColumnOpinion) -> Vec<DeclarationChange> {
    let column = Some(next.column.as_str());
    let mut out = Vec::new();
    push_if_changed(
        &mut out,
        column,
        "datatype",
        prev.datatype.map(|d| d.as_str().to_string()),
        next.datatype.map(|d| d.as_str().to_string()),
    );
    push_if_changed(
        &mut out,
        column,
        "is_time",
        text(prev.is_time),
        text(next.is_time),
    );
    push_if_changed(
        &mut out,
        column,
        "role",
        prev.ext.role.map(|r| r.as_str().to_string()),
        next.ext.role.map(|r| r.as_str().to_string()),
    );
    push_if_changed(
        &mut out,
        column,
        "is_kpi",
        text(prev.ext.is_kpi),
        text(next.ext.is_kpi),
    );
    push_if_changed(
        &mut out,
        column,
        "polarity",
        prev.ext.polarity.map(|p| p.as_str().to_string()),
        next.ext.polarity.map(|p| p.as_str().to_string()),
    );
    push_if_changed(
        &mut out,
        column,
        "label",
        prev.ext.label.clone(),
        next.ext.label.clone(),
    );
    push_if_changed(
        &mut out,
        column,
        "description",
        prev.description.clone(),
        next.description.clone(),
    );
    out
}

/// Every difference between a producer's previous and next declaration.
/// Column order follows `next`, then columns only `prev` had.
pub fn diff_declarations(
    prev_columns: &[ColumnOpinion],
    next_columns: &[ColumnOpinion],
    prev_table: Option<&TableOpinion>,
    next_table: Option<&TableOpinion>,
) -> Vec<DeclarationChange> {
    let mut out = Vec::new();
    for next in next_columns {
        match prev_columns.iter().find(|p| p.column == next.column) {
            Some(prev) => out.extend(diff_column(prev, next)),
            None => out.push(DeclarationChange {
                column: Some(next.column.clone()),
                field: "column".to_string(),
                from: None,
                to: Some("declared".to_string()),
            }),
        }
    }
    for prev in prev_columns {
        if !next_columns.iter().any(|n| n.column == prev.column) {
            out.push(DeclarationChange {
                column: Some(prev.column.clone()),
                field: "column".to_string(),
                from: Some("declared".to_string()),
                to: None,
            });
        }
    }
    let empty = TableOpinion::empty(crate::provenance::Provenance::detected());
    let prev_t = prev_table.unwrap_or(&empty);
    let next_t = next_table.unwrap_or(&empty);
    push_if_changed(
        &mut out,
        None,
        "display_name",
        prev_t.display_name.clone(),
        next_t.display_name.clone(),
    );
    push_if_changed(
        &mut out,
        None,
        "description",
        prev_t.description.clone(),
        next_t.description.clone(),
    );
    push_if_changed(
        &mut out,
        None,
        "time_granularity",
        prev_t.time_granularity.map(|g| g.as_str().to_string()),
        next_t.time_granularity.map(|g| g.as_str().to_string()),
    );
    push_if_changed(
        &mut out,
        None,
        "comparison_periods",
        text(prev_t.comparison_periods),
        text(next_t.comparison_periods),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ext::{ColumnExt, ColumnRole};
    use crate::provenance::Provenance;

    fn col(name: &str, role: ColumnRole, description: Option<&str>) -> ColumnOpinion {
        let mut o = ColumnOpinion::empty(name, Provenance::declared("connector:github"));
        o.ext = ColumnExt::role(role);
        o.description = description.map(str::to_string);
        o
    }

    #[test]
    fn diff_names_changed_fields_added_and_removed_columns() {
        let prev = vec![
            col("a", ColumnRole::Dimension, Some("old")),
            col("gone", ColumnRole::Ignored, None),
        ];
        let next = vec![
            col("a", ColumnRole::Measure, Some("old")),
            col("new", ColumnRole::Dimension, None),
        ];
        let mut prev_t = TableOpinion::empty(Provenance::declared("connector:github"));
        prev_t.display_name = Some("Issues".into());
        let mut next_t = prev_t.clone();
        next_t.display_name = Some("Issues and PRs".into());
        let changes = diff_declarations(&prev, &next, Some(&prev_t), Some(&next_t));
        assert_eq!(
            changes,
            vec![
                DeclarationChange {
                    column: Some("a".into()),
                    field: "role".into(),
                    from: Some("dimension".into()),
                    to: Some("measure".into()),
                },
                DeclarationChange {
                    column: Some("new".into()),
                    field: "column".into(),
                    from: None,
                    to: Some("declared".into()),
                },
                DeclarationChange {
                    column: Some("gone".into()),
                    field: "column".into(),
                    from: Some("declared".into()),
                    to: None,
                },
                DeclarationChange {
                    column: None,
                    field: "display_name".into(),
                    from: Some("Issues".into()),
                    to: Some("Issues and PRs".into()),
                },
            ]
        );
        let diff = DeclarationDiff {
            producer: "connector:github".into(),
            from_version: Some("0.2.0".into()),
            to_version: Some("0.3.0".into()),
            changes,
        };
        assert_eq!(diff.changed_columns(), ["a", "new", "gone"]);
    }

    #[test]
    fn identical_declarations_have_no_diff() {
        let cols = vec![col("a", ColumnRole::Dimension, Some("x"))];
        assert!(diff_declarations(&cols, &cols, None, None).is_empty());
    }
}
