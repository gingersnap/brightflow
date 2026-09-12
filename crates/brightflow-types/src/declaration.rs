//! What a producer hands the store when it creates or refreshes a table.
//!
//! A [`TableDeclaration`] bundles the physical facts a producer knows (name,
//! column types, primary key, cursor) with the meaning it claims (one Ossie
//! dataset, plus relationships and metrics that involve it). A connector, an
//! enrichment function and the detector all build one of these; the store
//! applies it under a [`crate::Provenance`] and keeps the rows.
//!
//! Two input shapes are accepted. The canonical one is this struct's own serde
//! form. The other is the shape a connector author writes next to an endpoint
//! — `columns`, `description`, `ai_context`, `relationships`, `metrics`,
//! `brightflow` at the top level, no dataset wrapper — and
//! [`TableDeclaration::from_endpoint_json`] turns that into the canonical
//! form, filling the dataset name, the relationship `from` and the metric
//! dataset from the table name so an author never repeats it.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::datatype::TableSchema;
use crate::ext::{DatasetExt, MetricExpr, BRIGHTFLOW_VENDOR};
use crate::semantic::{
    items_from_value, set_extension_json, AiContext, CustomExtension, Dataset, Field, Metric,
    Relationship, RelationshipDe,
};

/// The version of this crate's declaration shape. Bumped when a field changes
/// meaning; the store refuses a declaration from a newer contract than it
/// knows.
pub const CONTRACT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export)]
pub struct TableDeclaration {
    #[serde(default = "default_contract_version")]
    pub contract_version: u32,
    pub name: String,
    /// The physical columns, when the producer knows them before writing.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub schema: Option<TableSchema>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub primary_key: Vec<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unique_keys: Vec<Vec<String>>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub cursor_field: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub dataset: Option<Dataset>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<Relationship>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub metrics: Vec<Metric>,
}

const fn default_contract_version() -> u32 {
    CONTRACT_VERSION
}

/// The connector-author shape of a declaration: dataset fields at the top
/// level, `columns` for fields.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EndpointDeclarationDe {
    #[serde(default)]
    #[serde(deserialize_with = "crate::semantic::list_or_map")]
    columns: Vec<Field>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    ai_context: Option<AiContext>,
    #[serde(default)]
    unique_keys: Vec<Vec<String>>,
    #[serde(default)]
    relationships: serde_json::Value,
    #[serde(default)]
    metrics: serde_json::Value,
    #[serde(default)]
    custom_extensions: Vec<CustomExtension>,
    #[serde(default)]
    brightflow: Option<DatasetExt>,
}

impl TableDeclaration {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            contract_version: CONTRACT_VERSION,
            name: name.into(),
            schema: None,
            primary_key: Vec::new(),
            unique_keys: Vec::new(),
            cursor_field: None,
            dataset: None,
            relationships: Vec::new(),
            metrics: Vec::new(),
        }
    }

    /// Build from the connector-author shape. `source` is the dataset's
    /// `source` string (`{source_id}/{table}`); `primary_key` and
    /// `cursor_field` come from the endpoint config, not from the JSON.
    pub fn from_endpoint_json(
        name: &str,
        source: &str,
        primary_key: Vec<String>,
        cursor_field: Option<String>,
        value: serde_json::Value,
    ) -> Result<Self, String> {
        let de: EndpointDeclarationDe = serde_json::from_value(value).map_err(|e| e.to_string())?;
        let mut dataset = Dataset::new(name, source);
        dataset.primary_key.clone_from(&primary_key);
        dataset.unique_keys = de.unique_keys;
        dataset.description = de.description;
        dataset.ai_context = de.ai_context;
        dataset.fields = de.columns;
        dataset.custom_extensions = de.custom_extensions;
        if let Some(ext) = de.brightflow {
            let v = serde_json::to_value(&ext).map_err(|e| e.to_string())?;
            set_extension_json(&mut dataset.custom_extensions, BRIGHTFLOW_VENDOR, &v);
        }

        let relationships = relationships_from_value(de.relationships, name)
            .map_err(|e| format!("relationships: {e}"))?;

        let metrics = items_from_value::<Metric>(de.metrics)
            .map_err(|e| format!("metrics: {e}"))?
            .into_iter()
            .map(|m| qualify_metric(m, name))
            .collect();

        Ok(Self {
            contract_version: CONTRACT_VERSION,
            name: name.to_string(),
            schema: None,
            primary_key,
            unique_keys: Vec::new(),
            cursor_field,
            dataset: Some(dataset),
            relationships,
            metrics,
        })
    }

    /// Declared fields with no column in `schema`. Empty when there is no
    /// schema to compare against.
    pub fn columns_without_data(&self) -> Vec<String> {
        let (Some(schema), Some(dataset)) = (&self.schema, &self.dataset) else {
            return Vec::new();
        };
        dataset
            .fields
            .iter()
            .filter(|f| !schema.has_column(&f.name))
            .map(|f| f.name.clone())
            .collect()
    }
}

/// Relationships in list or map form, with `from` defaulting to the
/// declaring table.
fn relationships_from_value(
    value: serde_json::Value,
    from: &str,
) -> Result<Vec<Relationship>, String> {
    let parse = |item: serde_json::Value, key: Option<String>, label: String| {
        let de: RelationshipDe =
            serde_json::from_value(item).map_err(|e| format!("{label}: {e}"))?;
        de.into_relationship(key, Some(from))
    };
    match value {
        serde_json::Value::Array(items) => items
            .into_iter()
            .enumerate()
            .map(|(i, item)| parse(item, None, format!("item {i}")))
            .collect(),
        serde_json::Value::Object(map) => map
            .into_iter()
            .map(|(key, item)| parse(item, Some(key.clone()), format!("`{key}`")))
            .collect(),
        serde_json::Value::Null => Ok(Vec::new()),
        _ => Err("expected a list or a map keyed by name".to_string()),
    }
}

/// A metric declared on an endpoint refers to that endpoint's columns; fill
/// the dataset so the rendered SQL is qualified.
fn qualify_metric(mut metric: Metric, dataset: &str) -> Metric {
    if let Some(mut ext) = metric.brightflow() {
        if ext.expr.dataset.is_none() {
            ext.expr = MetricExpr {
                dataset: Some(dataset.to_string()),
                ..ext.expr
            };
            metric.expression = crate::semantic::Expression::ansi(ext.expr.render_sql());
            metric.set_brightflow(&ext);
        }
    }
    metric
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datatype::{ColumnSchema, LogicalType};
    use crate::ext::{Aggregation, ColumnRole, TimeGranularity};
    use serde_json::json;

    fn github_issues() -> serde_json::Value {
        json!({
            "description": "Issues and pull requests, one row each",
            "columns": {
                "id": {"datatype": "Integer", "brightflow": {"role": "ignored"}},
                "created_at": {"datatype": "DateTime", "description": "Opened at"},
                "reactions_total": {"datatype": "Integer", "brightflow": {"role": "measure", "is_kpi": true}}
            },
            "relationships": [
                {"to": "repository", "from_columns": ["repository_id"], "to_columns": ["id"]}
            ],
            "metrics": {
                "reactions": {"expression": {"column": "reactions_total", "aggregation": "sum"}}
            },
            "brightflow": {"time_granularity": "week", "comparison_periods": 4}
        })
    }

    #[test]
    fn endpoint_shape_becomes_a_canonical_declaration() {
        let decl = TableDeclaration::from_endpoint_json(
            "issues",
            "connector:x/issues",
            vec!["id".into()],
            Some("updated_at".into()),
            github_issues(),
        )
        .unwrap();
        assert_eq!(decl.contract_version, CONTRACT_VERSION);
        let ds = decl.dataset.as_ref().unwrap();
        assert_eq!(ds.name, "issues");
        assert_eq!(ds.source, "connector:x/issues");
        assert_eq!(ds.primary_key, ["id"]);
        assert_eq!(
            ds.description.as_deref(),
            Some("Issues and pull requests, one row each")
        );
        assert_eq!(
            ds.brightflow().unwrap().time_granularity,
            Some(TimeGranularity::Week)
        );
        let created = ds.field("created_at").unwrap();
        assert_eq!(created.datatype, Some(LogicalType::DateTime));
        assert!(created.resolved_is_time());
        assert_eq!(
            ds.field("reactions_total")
                .unwrap()
                .brightflow()
                .unwrap()
                .role,
            Some(ColumnRole::Measure)
        );
    }

    #[test]
    fn endpoint_relationships_and_metrics_are_qualified_with_the_table() {
        let decl = TableDeclaration::from_endpoint_json(
            "issues",
            "connector:x/issues",
            vec!["id".into()],
            Some("updated_at".into()),
            github_issues(),
        )
        .unwrap();
        let rel = &decl.relationships[0];
        assert_eq!(rel.from, "issues");
        assert_eq!(rel.to, "repository");
        assert_eq!(rel.name, "issues_repository");
        let metric = &decl.metrics[0];
        assert_eq!(metric.name, "reactions");
        assert_eq!(
            metric.expression.ansi_text(),
            Some("SUM(issues.reactions_total)")
        );
        assert_eq!(
            metric.structured_expr().unwrap().aggregation,
            Aggregation::Sum
        );
        assert_eq!(decl.cursor_field.as_deref(), Some("updated_at"));
    }

    #[test]
    fn canonical_form_round_trips() {
        let decl = TableDeclaration::from_endpoint_json(
            "issues",
            "s/issues",
            vec!["id".into()],
            None,
            github_issues(),
        )
        .unwrap();
        let json = serde_json::to_value(&decl).unwrap();
        let back: TableDeclaration = serde_json::from_value(json).unwrap();
        assert_eq!(back, decl);
    }

    #[test]
    fn unknown_endpoint_keys_are_rejected() {
        let err =
            TableDeclaration::from_endpoint_json("t", "s/t", vec![], None, json!({"colums": {}}))
                .unwrap_err();
        assert!(err.contains("colums"), "{err}");
    }

    #[test]
    fn columns_without_data_compares_fields_to_schema() {
        let mut decl = TableDeclaration::from_endpoint_json(
            "t",
            "s/t",
            vec![],
            None,
            json!({"columns": {"a": {}, "b": {}}}),
        )
        .unwrap();
        assert!(decl.columns_without_data().is_empty());
        decl.schema = Some(TableSchema {
            columns: vec![ColumnSchema::new("a", LogicalType::String)],
        });
        assert_eq!(decl.columns_without_data(), ["b"]);
    }

    #[test]
    fn empty_endpoint_json_is_a_bare_dataset() {
        let decl =
            TableDeclaration::from_endpoint_json("t", "s/t", vec![], None, json!({})).unwrap();
        let ds = decl.dataset.unwrap();
        assert!(ds.fields.is_empty());
        assert!(ds.brightflow().is_none());
    }
}
