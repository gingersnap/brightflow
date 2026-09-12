//! Structural validation of a semantic model and a declaration.
//!
//! These are the checks a JSON Schema cannot express: names unique within
//! their scope, relationships that point at datasets and fields that exist
//! with matching column counts, metrics whose structured column exists, keys
//! that name declared fields. Pure functions over the structs; the store calls
//! them before applying and the export test calls them before serialising.
//! A declaration is validated against itself only — cross-table references
//! are resolved by the store, which knows what else exists.

use std::collections::HashSet;

use crate::declaration::TableDeclaration;
use crate::semantic::{Dataset, Relationship, SemanticModel};

/// One reason a model or declaration is not well formed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Violation {
    #[error("duplicate dataset `{0}`")]
    DuplicateDataset(String),
    #[error("duplicate field `{field}` in dataset `{dataset}`")]
    DuplicateField { dataset: String, field: String },
    #[error("duplicate relationship `{0}`")]
    DuplicateRelationship(String),
    #[error("duplicate metric `{0}`")]
    DuplicateMetric(String),
    #[error("relationship `{relationship}` refers to unknown dataset `{dataset}`")]
    UnknownDataset {
        relationship: String,
        dataset: String,
    },
    #[error("relationship `{relationship}` refers to unknown field `{field}` in `{dataset}`")]
    UnknownField {
        relationship: String,
        dataset: String,
        field: String,
    },
    #[error("relationship `{0}` has {1} from_columns but {2} to_columns")]
    ColumnCountMismatch(String, usize, usize),
    #[error("relationship `{0}` has no columns")]
    EmptyRelationship(String),
    #[error("metric `{metric}` refers to unknown field `{field}` in `{dataset}`")]
    MetricUnknownField {
        metric: String,
        dataset: String,
        field: String,
    },
    #[error("key on `{dataset}` names unknown field `{field}`")]
    KeyUnknownField { dataset: String, field: String },
    #[error("declaration is from contract version {0}; this build knows {1}")]
    ContractTooNew(u32, u32),
}

impl SemanticModel {
    /// Every violation, or `Ok` when there are none.
    pub fn validate(&self) -> Result<(), Vec<Violation>> {
        let mut out = Vec::new();
        let mut names = HashSet::new();
        for ds in &self.datasets {
            if !names.insert(ds.name.as_str()) {
                out.push(Violation::DuplicateDataset(ds.name.clone()));
            }
            out.extend(validate_dataset(ds));
        }
        out.extend(self.validate_relationships());
        out.extend(self.validate_metrics());
        if out.is_empty() {
            Ok(())
        } else {
            Err(out)
        }
    }

    fn dataset(&self, name: &str) -> Option<&Dataset> {
        self.datasets.iter().find(|d| d.name == name)
    }

    fn validate_relationships(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        let mut rel_names = HashSet::new();
        for rel in &self.relationships {
            if !rel_names.insert(rel.name.as_str()) {
                out.push(Violation::DuplicateRelationship(rel.name.clone()));
            }
            out.extend(relationship_arity(rel));
            for (dataset, columns) in [(&rel.from, &rel.from_columns), (&rel.to, &rel.to_columns)] {
                match self.dataset(dataset) {
                    None => out.push(Violation::UnknownDataset {
                        relationship: rel.name.clone(),
                        dataset: dataset.clone(),
                    }),
                    Some(ds) => out.extend(unknown_fields(ds, columns).map(|field| {
                        Violation::UnknownField {
                            relationship: rel.name.clone(),
                            dataset: dataset.clone(),
                            field,
                        }
                    })),
                }
            }
        }
        out
    }

    fn validate_metrics(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        let mut metric_names = HashSet::new();
        for metric in &self.metrics {
            if !metric_names.insert(metric.name.as_str()) {
                out.push(Violation::DuplicateMetric(metric.name.clone()));
            }
            let Some(expr) = metric.structured_expr() else {
                continue;
            };
            let Some(ds) = expr.dataset.as_deref().and_then(|d| self.dataset(d)) else {
                continue;
            };
            out.extend(metric_unknown_field(ds, &metric.name, &expr.column));
        }
        out
    }
}

/// Empty or mismatched column lists on a relationship.
fn relationship_arity(rel: &Relationship) -> Option<Violation> {
    if rel.from_columns.is_empty() || rel.to_columns.is_empty() {
        Some(Violation::EmptyRelationship(rel.name.clone()))
    } else if rel.from_columns.len() != rel.to_columns.len() {
        Some(Violation::ColumnCountMismatch(
            rel.name.clone(),
            rel.from_columns.len(),
            rel.to_columns.len(),
        ))
    } else {
        None
    }
}

/// Columns not declared on `ds`. A dataset with no declared fields cannot be
/// checked and yields nothing.
fn unknown_fields<'a>(ds: &'a Dataset, columns: &'a [String]) -> impl Iterator<Item = String> + 'a {
    columns
        .iter()
        .filter(move |c| !ds.fields.is_empty() && ds.field(c).is_none())
        .cloned()
}

fn metric_unknown_field(ds: &Dataset, metric: &str, column: &str) -> Option<Violation> {
    (column != "*" && !ds.fields.is_empty() && ds.field(column).is_none()).then(|| {
        Violation::MetricUnknownField {
            metric: metric.to_string(),
            dataset: ds.name.clone(),
            field: column.to_string(),
        }
    })
}

fn validate_dataset(ds: &Dataset) -> Vec<Violation> {
    let mut out = Vec::new();
    let mut fields = HashSet::new();
    for f in &ds.fields {
        if !fields.insert(f.name.as_str()) {
            out.push(Violation::DuplicateField {
                dataset: ds.name.clone(),
                field: f.name.clone(),
            });
        }
    }
    if !ds.fields.is_empty() {
        for key in ds.primary_key.iter().chain(ds.unique_keys.iter().flatten()) {
            if !fields.contains(key.as_str()) {
                out.push(Violation::KeyUnknownField {
                    dataset: ds.name.clone(),
                    field: key.clone(),
                });
            }
        }
    }
    out
}

impl TableDeclaration {
    /// Checks that need nothing but the declaration: contract version, the
    /// dataset's own consistency, and that its own relationships and metrics
    /// name columns it declares. The `to` side of a relationship is another
    /// table and is the store's to resolve.
    pub fn validate(&self) -> Result<(), Vec<Violation>> {
        let mut out = Vec::new();
        if self.contract_version > crate::declaration::CONTRACT_VERSION {
            out.push(Violation::ContractTooNew(
                self.contract_version,
                crate::declaration::CONTRACT_VERSION,
            ));
        }
        if let Some(ds) = &self.dataset {
            out.extend(validate_dataset(ds));
            for rel in &self.relationships {
                out.extend(relationship_arity(rel));
                if rel.from == ds.name {
                    out.extend(unknown_fields(ds, &rel.from_columns).map(|field| {
                        Violation::UnknownField {
                            relationship: rel.name.clone(),
                            dataset: ds.name.clone(),
                            field,
                        }
                    }));
                }
            }
            for metric in &self.metrics {
                if let Some(expr) = metric.structured_expr() {
                    if expr.dataset.as_deref() == Some(ds.name.as_str()) {
                        out.extend(metric_unknown_field(ds, &metric.name, &expr.column));
                    }
                }
            }
        }
        if out.is_empty() {
            Ok(())
        } else {
            Err(out)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declaration::CONTRACT_VERSION;
    use crate::ext::{Aggregation, MetricExpr, MetricExt};
    use crate::semantic::{Field, Metric, Relationship};

    fn model() -> SemanticModel {
        let mut issues = Dataset::new("issues", "s/issues");
        issues.primary_key = vec!["id".into()];
        issues.fields = vec![
            Field::column("id"),
            Field::column("number"),
            Field::column("n"),
        ];
        let mut comments = Dataset::new("issue_comments", "s/issue_comments");
        comments.fields = vec![Field::column("id"), Field::column("issue_number")];
        SemanticModel {
            name: "github".into(),
            datasets: vec![issues, comments],
            relationships: vec![Relationship {
                name: "comment_issue".into(),
                from: "issue_comments".into(),
                to: "issues".into(),
                from_columns: vec!["issue_number".into()],
                to_columns: vec!["number".into()],
                ai_context: None,
                custom_extensions: Vec::new(),
            }],
            metrics: vec![Metric::structured(
                "total_n",
                &MetricExt::new(MetricExpr::new("n", Aggregation::Sum).in_dataset("issues")),
            )],
            ..Default::default()
        }
    }

    #[test]
    fn a_well_formed_model_validates() {
        assert_eq!(model().validate(), Ok(()));
    }

    #[test]
    fn duplicates_are_reported() {
        let mut m = model();
        m.datasets.push(Dataset::new("issues", "s/issues"));
        m.datasets[0].fields.push(Field::column("id"));
        m.relationships.push(m.relationships[0].clone());
        m.metrics.push(m.metrics[0].clone());
        let errs = m.validate().unwrap_err();
        assert!(errs.contains(&Violation::DuplicateDataset("issues".into())));
        assert!(errs.contains(&Violation::DuplicateField {
            dataset: "issues".into(),
            field: "id".into()
        }));
        assert!(errs.contains(&Violation::DuplicateRelationship("comment_issue".into())));
        assert!(errs.contains(&Violation::DuplicateMetric("total_n".into())));
    }

    #[test]
    fn dangling_references_are_reported() {
        let mut m = model();
        m.relationships[0].to = "repos".into();
        m.relationships[0].from_columns = vec!["nope".into()];
        m.metrics[0] = Metric::structured(
            "bad",
            &MetricExt::new(MetricExpr::new("missing", Aggregation::Sum).in_dataset("issues")),
        );
        m.datasets[0].primary_key = vec!["ghost".into()];
        let errs = m.validate().unwrap_err();
        assert!(errs.contains(&Violation::UnknownDataset {
            relationship: "comment_issue".into(),
            dataset: "repos".into()
        }));
        assert!(errs.contains(&Violation::UnknownField {
            relationship: "comment_issue".into(),
            dataset: "issue_comments".into(),
            field: "nope".into()
        }));
        assert!(errs.contains(&Violation::MetricUnknownField {
            metric: "bad".into(),
            dataset: "issues".into(),
            field: "missing".into()
        }));
        assert!(errs.contains(&Violation::KeyUnknownField {
            dataset: "issues".into(),
            field: "ghost".into()
        }));
    }

    #[test]
    fn relationship_arity_is_checked() {
        let mut m = model();
        m.relationships[0].to_columns = vec!["number".into(), "id".into()];
        let errs = m.validate().unwrap_err();
        assert!(errs.contains(&Violation::ColumnCountMismatch(
            "comment_issue".into(),
            1,
            2
        )));
        m.relationships[0].from_columns.clear();
        let empty_errs = m.validate().unwrap_err();
        assert!(empty_errs.contains(&Violation::EmptyRelationship("comment_issue".into())));
    }

    #[test]
    fn a_dataset_without_fields_is_not_checked_for_columns() {
        let mut m = model();
        m.datasets[0].fields.clear();
        assert_eq!(m.validate(), Ok(()));
    }

    #[test]
    fn declaration_checks_itself_only() {
        let decl = TableDeclaration::from_endpoint_json(
            "issue_comments",
            "s/issue_comments",
            vec!["id".into()],
            None,
            serde_json::json!({
                "columns": {"id": {}, "issue_number": {}},
                "relationships": [{"to": "issues", "from_columns": ["issue_number"], "to_columns": ["number"]}],
                "metrics": {"n": {"expression": {"column": "id", "aggregation": "count"}}}
            }),
        )
        .unwrap();
        assert_eq!(decl.validate(), Ok(()));

        let bad = TableDeclaration::from_endpoint_json(
            "issue_comments",
            "s/issue_comments",
            vec!["uuid".into()],
            None,
            serde_json::json!({
                "columns": {"id": {}},
                "relationships": [{"to": "issues", "from_columns": ["issue_number"], "to_columns": ["number"]}],
                "metrics": {"n": {"expression": {"column": "ghost", "aggregation": "count"}}}
            }),
        )
        .unwrap();
        let errs = bad.validate().unwrap_err();
        assert!(errs
            .iter()
            .any(|e| matches!(e, Violation::KeyUnknownField { field, .. } if field == "uuid")));
        assert!(errs.iter().any(
            |e| matches!(e, Violation::UnknownField { field, .. } if field == "issue_number")
        ));
        assert!(errs
            .iter()
            .any(|e| matches!(e, Violation::MetricUnknownField { field, .. } if field == "ghost")));
    }

    #[test]
    fn a_newer_contract_is_refused() {
        let mut decl = TableDeclaration::new("t");
        decl.contract_version = CONTRACT_VERSION + 1;
        assert_eq!(
            decl.validate(),
            Err(vec![Violation::ContractTooNew(
                CONTRACT_VERSION + 1,
                CONTRACT_VERSION
            )])
        );
    }
}
