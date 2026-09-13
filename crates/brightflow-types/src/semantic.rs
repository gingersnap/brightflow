//! The Ossie-shaped semantic model: datasets, fields, relationships, metrics.
//!
//! These structs *are* Apache Ossie's core model (`0.2.0.dev0`), field for
//! field, so serialising a [`SemanticModel`] with serde produces an Ossie
//! document and the vendored JSON Schema in `tests/fixtures` can check it. The
//! only liberty is on input: deserialising accepts the exact Ossie form and a
//! shorter one meant for hand-authored declarations in Lua or TOML —
//! `expression` may be a bare column string or absent (it defaults to the
//! field name), `is_time` may sit at the top level instead of under
//! `dimension`, `fields`/`metrics`/`relationships` may be a map keyed by name
//! instead of a list (order is then alphabetical), and a `brightflow` key
//! becomes the `BRIGHTFLOW` custom extension. Output is always the strict form.
//!
//! Metrics carry an ANSI SQL expression because Ossie requires one, but that
//! string is *rendered*, never parsed: the structured form lives in the
//! `BRIGHTFLOW` extension (see [`crate::ext::MetricExpr`]) and is what the
//! store keeps and the engine executes. A document whose metric has only SQL
//! and no structured form deserialises, but `Metric::structured()` is `None`
//! for it and the store refuses to apply it.

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use ts_rs::TS;

use crate::datatype::LogicalType;
use crate::ext::{ColumnExt, DatasetExt, MetricExpr, MetricExt, BRIGHTFLOW_VENDOR};

/// The Ossie spec version this crate serialises.
pub const OSSIE_VERSION: &str = "0.2.0.dev0";
/// The one expression dialect this crate writes.
pub const ANSI_SQL: &str = "ANSI_SQL";

/// A whole Ossie document: the top-level wrapper around one or more models.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export)]
pub struct OssieDocument {
    pub version: String,
    pub semantic_model: Vec<SemanticModel>,
}

impl OssieDocument {
    pub fn new(model: SemanticModel) -> Self {
        Self {
            version: OSSIE_VERSION.to_string(),
            semantic_model: vec![model],
        }
    }
}

/// One semantic model: a named set of datasets with the relationships and
/// metrics that span them. In Brightflow one model is one source.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export)]
pub struct SemanticModel {
    pub name: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ai_context: Option<AiContext>,
    pub datasets: Vec<Dataset>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(deserialize_with = "list_or_map")]
    pub relationships: Vec<Relationship>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(deserialize_with = "list_or_map")]
    pub metrics: Vec<Metric>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub custom_extensions: Vec<CustomExtension>,
}

/// Context for AI tools. Ossie allows a bare string or a structured object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export)]
#[serde(untagged)]
pub enum AiContext {
    Text(String),
    Structured(AiContextFields),
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export)]
pub struct AiContextFields {
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub instructions: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub synonyms: Vec<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<String>,
}

/// A vendor's own metadata, opaque to everyone else. Ossie types `data` as a
/// JSON *string*; the helpers below hide that.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export)]
pub struct CustomExtension {
    pub vendor_name: String,
    pub data: String,
}

impl CustomExtension {
    pub(crate) fn from_json(vendor_name: impl Into<String>, value: &serde_json::Value) -> Self {
        Self {
            vendor_name: vendor_name.into(),
            data: value.to_string(),
        }
    }

    pub fn json(&self) -> Option<serde_json::Value> {
        serde_json::from_str(&self.data).ok()
    }
}

/// Read one vendor's extension as JSON.
pub(crate) fn extension_json(exts: &[CustomExtension], vendor: &str) -> Option<serde_json::Value> {
    exts.iter()
        .find(|e| e.vendor_name == vendor)
        .and_then(CustomExtension::json)
}

/// Replace (or add) one vendor's extension.
pub fn set_extension_json(
    exts: &mut Vec<CustomExtension>,
    vendor: &str,
    value: &serde_json::Value,
) {
    exts.retain(|e| e.vendor_name != vendor);
    exts.push(CustomExtension::from_json(vendor, value));
}

/// An expression in one or more dialects. This crate writes exactly one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export)]
pub struct Expression {
    pub dialects: Vec<DialectExpression>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export)]
pub struct DialectExpression {
    pub dialect: String,
    pub expression: String,
}

impl Expression {
    pub fn ansi(expression: impl Into<String>) -> Self {
        Self {
            dialects: vec![DialectExpression {
                dialect: ANSI_SQL.to_string(),
                expression: expression.into(),
            }],
        }
    }

    /// The ANSI SQL text, if present.
    pub fn ansi_text(&self) -> Option<&str> {
        self.dialects
            .iter()
            .find(|d| d.dialect == ANSI_SQL)
            .map(|d| d.expression.as_str())
    }
}

/// Ossie's dimension marker. Only `is_time` exists today.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export)]
pub struct Dimension {
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub is_time: Option<bool>,
}

/// A row-level attribute of a dataset: a column, or an expression over columns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS, JsonSchema)]
#[ts(export)]
pub struct Field {
    pub name: String,
    pub expression: Expression,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub dimension: Option<Dimension>,
    /// Ossie's categorisation tag. Not a display name — that is
    /// `ColumnExt::label`.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub label: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub datatype: Option<LogicalType>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ai_context: Option<AiContext>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub custom_extensions: Vec<CustomExtension>,
}

impl Field {
    /// A field that is one physical column.
    pub fn column(name: impl Into<String>) -> Self {
        let owned: String = name.into();
        Self {
            expression: Expression::ansi(owned.clone()),
            name: owned,
            dimension: None,
            label: None,
            description: None,
            datatype: None,
            ai_context: None,
            custom_extensions: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_datatype(mut self, datatype: LogicalType) -> Self {
        self.datatype = Some(datatype);
        self
    }

    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    #[must_use]
    pub fn with_is_time(mut self, is_time: bool) -> Self {
        self.dimension = Some(Dimension {
            is_time: Some(is_time),
        });
        self
    }

    #[must_use]
    pub fn with_brightflow(mut self, ext: &ColumnExt) -> Self {
        self.set_brightflow(ext);
        self
    }

    /// Ossie's rule: an explicit `is_time` wins; otherwise a temporal
    /// datatype is a time dimension and anything else is not.
    pub fn resolved_is_time(&self) -> bool {
        match self.dimension.and_then(|d| d.is_time) {
            Some(explicit) => explicit,
            None => self.datatype.is_some_and(LogicalType::is_temporal),
        }
    }

    /// Brightflow's extension on this field, if any.
    pub fn brightflow(&self) -> Option<ColumnExt> {
        extension_json(&self.custom_extensions, BRIGHTFLOW_VENDOR)
            .and_then(|v| serde_json::from_value(v).ok())
    }

    pub fn set_brightflow(&mut self, ext: &ColumnExt) {
        if let Ok(value) = serde_json::to_value(ext) {
            set_extension_json(&mut self.custom_extensions, BRIGHTFLOW_VENDOR, &value);
        }
    }
}

/// Lenient input form of a field. `name` is optional because the map form
/// supplies it from the key.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FieldDe {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    expression: Option<ExprDe>,
    #[serde(default)]
    dimension: Option<Dimension>,
    #[serde(default)]
    is_time: Option<bool>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    datatype: Option<LogicalType>,
    #[serde(default)]
    ai_context: Option<AiContext>,
    #[serde(default)]
    custom_extensions: Vec<CustomExtension>,
    #[serde(default)]
    brightflow: Option<ColumnExt>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ExprDe {
    Text(String),
    Ossie(Expression),
}

impl ExprDe {
    fn into_expression(self) -> Expression {
        match self {
            Self::Text(t) => Expression::ansi(t),
            Self::Ossie(e) => e,
        }
    }
}

impl FieldDe {
    fn into_field(self, key: Option<String>) -> Result<Field, String> {
        let name = self
            .name
            .or(key)
            .ok_or_else(|| "field without a name".to_string())?;
        let dimension = match (self.dimension, self.is_time) {
            (Some(d), None) => Some(d),
            (None, Some(t)) => Some(Dimension { is_time: Some(t) }),
            (Some(d), Some(t)) => Some(Dimension {
                is_time: d.is_time.or(Some(t)),
            }),
            (None, None) => None,
        };
        let mut custom_extensions = self.custom_extensions;
        if let Some(ext) = self.brightflow {
            let value = serde_json::to_value(&ext).map_err(|e| e.to_string())?;
            set_extension_json(&mut custom_extensions, BRIGHTFLOW_VENDOR, &value);
        }
        Ok(Field {
            expression: self
                .expression
                .map_or_else(|| Expression::ansi(name.clone()), ExprDe::into_expression),
            name,
            dimension,
            label: self.label,
            description: self.description,
            datatype: self.datatype,
            ai_context: self.ai_context,
            custom_extensions,
        })
    }
}

impl<'de> Deserialize<'de> for Field {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        FieldDe::deserialize(deserializer)?
            .into_field(None)
            .map_err(serde::de::Error::custom)
    }
}

/// A logical dataset: one table and the fields over it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS, JsonSchema)]
#[ts(export)]
pub struct Dataset {
    pub name: String,
    /// Where the rows live. In Brightflow: `{source_id}/{table}`.
    pub source: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub primary_key: Vec<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unique_keys: Vec<Vec<String>>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ai_context: Option<AiContext>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<Field>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub custom_extensions: Vec<CustomExtension>,
}

impl Dataset {
    pub fn new(name: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            source: source.into(),
            primary_key: Vec::new(),
            unique_keys: Vec::new(),
            description: None,
            ai_context: None,
            fields: Vec::new(),
            custom_extensions: Vec::new(),
        }
    }

    pub fn field(&self, name: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.name == name)
    }

    pub fn brightflow(&self) -> Option<DatasetExt> {
        extension_json(&self.custom_extensions, BRIGHTFLOW_VENDOR)
            .and_then(|v| serde_json::from_value(v).ok())
    }

    pub fn set_brightflow(&mut self, ext: &DatasetExt) {
        if let Ok(value) = serde_json::to_value(ext) {
            set_extension_json(&mut self.custom_extensions, BRIGHTFLOW_VENDOR, &value);
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DatasetDe {
    #[serde(default)]
    name: Option<String>,
    source: String,
    #[serde(default)]
    primary_key: Vec<String>,
    #[serde(default)]
    unique_keys: Vec<Vec<String>>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    ai_context: Option<AiContext>,
    #[serde(default)]
    #[serde(deserialize_with = "list_or_map")]
    fields: Vec<Field>,
    #[serde(default)]
    custom_extensions: Vec<CustomExtension>,
    #[serde(default)]
    brightflow: Option<DatasetExt>,
}

impl DatasetDe {
    fn into_dataset(self, key: Option<String>) -> Result<Dataset, String> {
        let name = self
            .name
            .or(key)
            .ok_or_else(|| "dataset without a name".to_string())?;
        let mut custom_extensions = self.custom_extensions;
        if let Some(ext) = self.brightflow {
            let value = serde_json::to_value(&ext).map_err(|e| e.to_string())?;
            set_extension_json(&mut custom_extensions, BRIGHTFLOW_VENDOR, &value);
        }
        Ok(Dataset {
            name,
            source: self.source,
            primary_key: self.primary_key,
            unique_keys: self.unique_keys,
            description: self.description,
            ai_context: self.ai_context,
            fields: self.fields,
            custom_extensions,
        })
    }
}

impl<'de> Deserialize<'de> for Dataset {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        DatasetDe::deserialize(deserializer)?
            .into_dataset(None)
            .map_err(serde::de::Error::custom)
    }
}

/// A many-to-one join from one dataset to another.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS, JsonSchema)]
#[ts(export)]
pub struct Relationship {
    pub name: String,
    /// The many side.
    pub from: String,
    /// The one side.
    pub to: String,
    pub from_columns: Vec<String>,
    pub to_columns: Vec<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ai_context: Option<AiContext>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub custom_extensions: Vec<CustomExtension>,
}

/// Lenient input: `from` may be absent when the context supplies it (a
/// connector declaring a relationship on its own endpoint).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RelationshipDe {
    #[serde(default)]
    pub(crate) name: Option<String>,
    #[serde(default)]
    pub(crate) from: Option<String>,
    pub(crate) to: String,
    pub(crate) from_columns: Vec<String>,
    pub(crate) to_columns: Vec<String>,
    #[serde(default)]
    pub(crate) ai_context: Option<AiContext>,
    #[serde(default)]
    pub(crate) custom_extensions: Vec<CustomExtension>,
}

impl RelationshipDe {
    pub(crate) fn into_relationship(
        self,
        key: Option<String>,
        from_hint: Option<&str>,
    ) -> Result<Relationship, String> {
        let from = self
            .from
            .or_else(|| from_hint.map(str::to_string))
            .ok_or_else(|| "relationship without a `from` dataset".to_string())?;
        let name = self
            .name
            .or(key)
            .unwrap_or_else(|| format!("{from}_{}", self.to));
        Ok(Relationship {
            name,
            from,
            to: self.to,
            from_columns: self.from_columns,
            to_columns: self.to_columns,
            ai_context: self.ai_context,
            custom_extensions: self.custom_extensions,
        })
    }
}

impl<'de> Deserialize<'de> for Relationship {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        RelationshipDe::deserialize(deserializer)?
            .into_relationship(None, None)
            .map_err(serde::de::Error::custom)
    }
}

/// A named aggregate. The SQL is rendered from the structured expression in
/// the `BRIGHTFLOW` extension; see the module header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS, JsonSchema)]
#[ts(export)]
pub struct Metric {
    pub name: String,
    pub expression: Expression,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub datatype: Option<LogicalType>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ai_context: Option<AiContext>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub custom_extensions: Vec<CustomExtension>,
}

impl Metric {
    /// A metric from its structured form; the SQL is rendered from it.
    pub fn structured(name: impl Into<String>, ext: &MetricExt) -> Self {
        let mut metric = Self {
            name: name.into(),
            expression: Expression::ansi(ext.expr.render_sql()),
            description: None,
            datatype: None,
            ai_context: None,
            custom_extensions: Vec::new(),
        };
        metric.set_brightflow(ext);
        metric
    }

    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn brightflow(&self) -> Option<MetricExt> {
        extension_json(&self.custom_extensions, BRIGHTFLOW_VENDOR)
            .and_then(|v| serde_json::from_value(v).ok())
    }

    /// The executable form, if this metric carries one.
    pub fn structured_expr(&self) -> Option<MetricExpr> {
        self.brightflow().map(|e| e.expr)
    }

    pub fn set_brightflow(&mut self, ext: &MetricExt) {
        if let Ok(value) = serde_json::to_value(ext) {
            set_extension_json(&mut self.custom_extensions, BRIGHTFLOW_VENDOR, &value);
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MetricDe {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    expression: Option<MetricExprDe>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    datatype: Option<LogicalType>,
    #[serde(default)]
    ai_context: Option<AiContext>,
    #[serde(default)]
    custom_extensions: Vec<CustomExtension>,
    #[serde(default)]
    brightflow: Option<MetricExt>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum MetricExprDe {
    Structured(MetricExpr),
    Ossie(Expression),
    Text(String),
}

impl MetricDe {
    fn into_metric(self, key: Option<String>) -> Result<Metric, String> {
        let name = self
            .name
            .or(key)
            .ok_or_else(|| "metric without a name".to_string())?;
        let mut custom_extensions = self.custom_extensions;
        let mut ext = self.brightflow;
        let expression = match self.expression {
            Some(MetricExprDe::Structured(expr)) => {
                let sql = expr.render_sql();
                ext = Some(match ext {
                    Some(mut e) => {
                        e.expr = expr;
                        e
                    },
                    None => MetricExt::new(expr),
                });
                Expression::ansi(sql)
            },
            Some(MetricExprDe::Ossie(e)) => e,
            Some(MetricExprDe::Text(t)) => Expression::ansi(t),
            None => match &ext {
                Some(e) => Expression::ansi(e.expr.render_sql()),
                None => return Err(format!("metric `{name}` has no expression")),
            },
        };
        if let Some(resolved) = ext {
            let value = serde_json::to_value(&resolved).map_err(|e| e.to_string())?;
            set_extension_json(&mut custom_extensions, BRIGHTFLOW_VENDOR, &value);
        }
        Ok(Metric {
            name,
            expression,
            description: self.description,
            datatype: self.datatype,
            ai_context: self.ai_context,
            custom_extensions,
        })
    }
}

impl<'de> Deserialize<'de> for Metric {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        MetricDe::deserialize(deserializer)?
            .into_metric(None)
            .map_err(serde::de::Error::custom)
    }
}

/// Anything that can be built from its lenient form plus an optional key.
pub(crate) trait FromKeyed: Sized {
    type De: for<'de> Deserialize<'de>;
    fn from_keyed(de: Self::De, key: Option<String>) -> Result<Self, String>;
}

impl FromKeyed for Field {
    type De = FieldDe;
    fn from_keyed(de: FieldDe, key: Option<String>) -> Result<Self, String> {
        de.into_field(key)
    }
}

impl FromKeyed for Dataset {
    type De = DatasetDe;
    fn from_keyed(de: DatasetDe, key: Option<String>) -> Result<Self, String> {
        de.into_dataset(key)
    }
}

impl FromKeyed for Relationship {
    type De = RelationshipDe;
    fn from_keyed(de: RelationshipDe, key: Option<String>) -> Result<Self, String> {
        de.into_relationship(key, None)
    }
}

impl FromKeyed for Metric {
    type De = MetricDe;
    fn from_keyed(de: MetricDe, key: Option<String>) -> Result<Self, String> {
        de.into_metric(key)
    }
}

/// Accept a list (Ossie) or a map keyed by name (hand-authored). A map is
/// read in key order, so declared order is alphabetical in that form. Goes
/// through `serde_json::Value` so an error inside one item keeps its own
/// message and names the item, instead of serde's "did not match any
/// variant" for the whole list.
pub(crate) fn list_or_map<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: FromKeyed,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    items_from_value(value).map_err(serde::de::Error::custom)
}

/// The list-or-map rule over an already-parsed JSON value.
pub(crate) fn items_from_value<T: FromKeyed>(value: serde_json::Value) -> Result<Vec<T>, String> {
    match value {
        serde_json::Value::Array(items) => items
            .into_iter()
            .enumerate()
            .map(|(i, item)| {
                let de: T::De =
                    serde_json::from_value(item).map_err(|e| format!("item {i}: {e}"))?;
                T::from_keyed(de, None)
            })
            .collect(),
        serde_json::Value::Object(map) => map
            .into_iter()
            .map(|(key, item)| {
                let de: T::De =
                    serde_json::from_value(item).map_err(|e| format!("`{key}`: {e}"))?;
                T::from_keyed(de, Some(key))
            })
            .collect(),
        serde_json::Value::Null => Ok(Vec::new()),
        other => Err(format!(
            "expected a list or a map keyed by name, got {}",
            match other {
                serde_json::Value::String(_) => "a string",
                serde_json::Value::Number(_) => "a number",
                serde_json::Value::Bool(_) => "a boolean",
                _ => "something else",
            }
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ext::{Aggregation, ColumnRole};
    use serde_json::json;

    #[test]
    fn a_column_field_serialises_in_ossie_form() {
        let field = Field::column("created_at")
            .with_datatype(LogicalType::DateTime)
            .with_description("When it was opened");
        let json = serde_json::to_value(&field).unwrap();
        assert_eq!(
            json,
            json!({
                "name": "created_at",
                "expression": {"dialects": [{"dialect": "ANSI_SQL", "expression": "created_at"}]},
                "description": "When it was opened",
                "datatype": "DateTime"
            })
        );
    }

    #[test]
    fn field_accepts_the_strict_form_and_the_short_form() {
        let strict: Field = serde_json::from_value(json!({
            "name": "created_at",
            "expression": {"dialects": [{"dialect": "ANSI_SQL", "expression": "created_at"}]},
            "dimension": {"is_time": true},
            "datatype": "DateTime"
        }))
        .unwrap();
        let short: Field = serde_json::from_value(json!({
            "name": "created_at",
            "is_time": true,
            "datatype": "DateTime"
        }))
        .unwrap();
        assert_eq!(strict, short);
        let text_expr: Field =
            serde_json::from_value(json!({"name": "full", "expression": "first || last"})).unwrap();
        assert_eq!(text_expr.expression.ansi_text(), Some("first || last"));
    }

    #[test]
    fn brightflow_key_becomes_the_extension_and_reads_back() {
        let field: Field = serde_json::from_value(json!({
            "name": "reactions_total",
            "brightflow": {"role": "measure", "is_kpi": true}
        }))
        .unwrap();
        let ext = field.brightflow().unwrap();
        assert_eq!(ext.role, Some(ColumnRole::Measure));
        assert_eq!(ext.is_kpi, Some(true));
        let json = serde_json::to_value(&field).unwrap();
        assert_eq!(json["custom_extensions"][0]["vendor_name"], "BRIGHTFLOW");
        assert!(json["custom_extensions"][0]["data"].is_string());
        assert!(json.get("brightflow").is_none());
        // Round trip through the strict form keeps the extension.
        let back: Field = serde_json::from_value(json).unwrap();
        assert_eq!(back, field);
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let err =
            serde_json::from_value::<Field>(json!({"name": "x", "colour": "red"})).unwrap_err();
        assert!(err.to_string().contains("colour"), "{err}");
    }

    #[test]
    fn is_time_resolves_per_the_spec() {
        assert!(Field::column("d")
            .with_datatype(LogicalType::Date)
            .resolved_is_time());
        assert!(!Field::column("n")
            .with_datatype(LogicalType::Integer)
            .resolved_is_time());
        assert!(Field::column("y")
            .with_datatype(LogicalType::Integer)
            .with_is_time(true)
            .resolved_is_time());
        assert!(!Field::column("audit")
            .with_datatype(LogicalType::DateTime)
            .with_is_time(false)
            .resolved_is_time());
        assert!(!Field::column("untyped").resolved_is_time());
    }

    #[test]
    fn fields_accept_a_list_or_a_map() {
        let list: Dataset = serde_json::from_value(json!({
            "name": "issues", "source": "s/issues",
            "fields": [{"name": "b"}, {"name": "a"}]
        }))
        .unwrap();
        let map: Dataset = serde_json::from_value(json!({
            "name": "issues", "source": "s/issues",
            "fields": {"b": {}, "a": {}}
        }))
        .unwrap();
        assert_eq!(
            list.fields
                .iter()
                .map(|f| f.name.as_str())
                .collect::<Vec<_>>(),
            ["b", "a"]
        );
        assert_eq!(
            map.fields
                .iter()
                .map(|f| f.name.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
    }

    #[test]
    fn relationship_name_defaults_and_from_is_required_standalone() {
        let named: Relationship = serde_json::from_value(json!({
            "from": "issue_comments", "to": "issues",
            "from_columns": ["issue_number"], "to_columns": ["number"]
        }))
        .unwrap();
        assert_eq!(named.name, "issue_comments_issues");
        let err = serde_json::from_value::<Relationship>(json!({
            "to": "issues", "from_columns": ["a"], "to_columns": ["b"]
        }))
        .unwrap_err();
        assert!(err.to_string().contains("from"));
    }

    #[test]
    fn metric_from_structured_expression_renders_sql_and_keeps_the_form() {
        let m: Metric = serde_json::from_value(json!({
            "name": "revenue",
            "expression": {"dataset": "orders", "column": "amount", "aggregation": "sum"}
        }))
        .unwrap();
        assert_eq!(m.expression.ansi_text(), Some("SUM(orders.amount)"));
        let expr = m.structured_expr().unwrap();
        assert_eq!(expr.aggregation, Aggregation::Sum);
        // Strict round trip keeps both.
        let json = serde_json::to_value(&m).unwrap();
        let back: Metric = serde_json::from_value(json).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn a_sql_only_metric_has_no_structured_form() {
        let m: Metric = serde_json::from_value(json!({
            "name": "x", "expression": "SUM(t.c)"
        }))
        .unwrap();
        assert_eq!(m.expression.ansi_text(), Some("SUM(t.c)"));
        assert!(m.structured_expr().is_none());
        let err = serde_json::from_value::<Metric>(json!({"name": "x"})).unwrap_err();
        assert!(err.to_string().contains("no expression"));
    }

    #[test]
    fn ai_context_is_text_or_structured() {
        let text: AiContext = serde_json::from_value(json!("Use for retail")).unwrap();
        assert_eq!(text, AiContext::Text("Use for retail".into()));
        let structured: AiContext =
            serde_json::from_value(json!({"synonyms": ["tickets"]})).unwrap();
        assert_eq!(
            structured,
            AiContext::Structured(AiContextFields {
                synonyms: vec!["tickets".into()],
                ..Default::default()
            })
        );
    }

    #[test]
    fn document_wraps_one_model_with_the_spec_version() {
        let doc = OssieDocument::new(SemanticModel {
            name: "github".into(),
            datasets: vec![Dataset::new("issues", "connector:x/issues")],
            ..Default::default()
        });
        let json = serde_json::to_value(&doc).unwrap();
        assert_eq!(json["version"], OSSIE_VERSION);
        assert_eq!(json["semantic_model"][0]["datasets"][0]["name"], "issues");
        assert!(json["semantic_model"][0].get("relationships").is_none());
    }
}
