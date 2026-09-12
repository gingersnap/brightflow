//! Brightflow's extension vocabulary: what Ossie's core does not say.
//!
//! Column role, polarity, KPI flag and display label; per-table analysis
//! defaults and the document-display columns; the structured metric
//! expression. None of these exist in Ossie `0.2.0.dev0` (display name,
//! polarity and default aggregation are an open proposal there), so they ride
//! in a `custom_extensions` entry with `vendor_name = "BRIGHTFLOW"`. Each struct
//! here is the JSON inside that entry. When the spec grows a field, the value
//! moves out of the bag and the key here is retired.
//!
//! The three enums are the one vocabulary for role, polarity and granularity
//! everywhere they cross a boundary: stored strings (`as_str`/`parse`), the
//! typed frontend (`ts-rs`), the LLM action manifest (`schemars`), and a
//! connector's declaration. No other crate defines a mirror of them; the SQL
//! `CHECK` constraints in the store's migrations list the same literals and a
//! store test asserts the two lists agree.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The vendor name under which Brightflow's extension is filed.
pub const BRIGHTFLOW_VENDOR: &str = "BRIGHTFLOW";

/// What a column is for in analysis.
#[derive(Debug, Clone, Copy, Serialize, TS, JsonSchema, PartialEq, Eq, Hash)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum ColumnRole {
    /// Numeric columns for analysis (both KPIs and supporting metrics)
    Measure,
    /// Categorical columns for slicing/segmentation
    Dimension,
    /// Temporal columns for time-series analysis
    Time,
    /// Entity/identifier columns (user, org, etc.)
    Entity,
    /// Columns to skip (IDs, internal fields, PII)
    Ignored,
}

impl ColumnRole {
    /// Every value — for validation messages and the `CHECK` agreement test.
    pub const ALL: [Self; 5] = [
        Self::Measure,
        Self::Dimension,
        Self::Time,
        Self::Entity,
        Self::Ignored,
    ];

    /// The stored/wire string form — the inverse of `parse`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Measure => "measure",
            Self::Dimension => "dimension",
            Self::Time => "time",
            Self::Entity => "entity",
            Self::Ignored => "ignored",
        }
    }

    /// Parse the stored/wire string form ("measure", "dimension", ...).
    /// Strict — legacy aliases are only accepted by the serde deserializer.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.as_str() == s)
    }
}

/// Custom deserializer that accepts legacy "kpi"/"metric" as aliases for
/// "measure". The derived JSON Schema and TypeScript type describe only the
/// five canonical names; the aliases exist for old stored payloads, not for
/// new writers.
impl<'de> Deserialize<'de> for ColumnRole {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "measure" | "kpi" | "metric" => Ok(Self::Measure),
            "dimension" => Ok(Self::Dimension),
            "time" => Ok(Self::Time),
            "entity" => Ok(Self::Entity),
            "ignored" => Ok(Self::Ignored),
            other => Err(serde::de::Error::unknown_variant(
                other,
                &["measure", "dimension", "time", "entity", "ignored"],
            )),
        }
    }
}

/// Which direction of movement in a measure is good news.
///
/// Rising churn is not rising revenue; display code keys off this.
#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, TS, JsonSchema, PartialEq, Eq, Hash, Default,
)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum Polarity {
    HigherIsBetter,
    LowerIsBetter,
    #[default]
    Neutral,
}

impl Polarity {
    pub const ALL: [Self; 3] = [Self::HigherIsBetter, Self::LowerIsBetter, Self::Neutral];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HigherIsBetter => "higher_is_better",
            Self::LowerIsBetter => "lower_is_better",
            Self::Neutral => "neutral",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.as_str() == s)
    }
}

/// The period a time column is bucketed into. `Week` is the default because
/// it is the coarsest unit that still shows a month's shape.
#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, TS, JsonSchema, PartialEq, Eq, Hash, Default,
)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum TimeGranularity {
    Day,
    #[default]
    Week,
    Month,
    Quarter,
    Year,
}

impl TimeGranularity {
    /// Every value, in coarsening order — for validation messages and pickers.
    pub const ALL: [Self; 5] = [
        Self::Day,
        Self::Week,
        Self::Month,
        Self::Quarter,
        Self::Year,
    ];

    /// The stored/wire string form — the inverse of `parse`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
            Self::Quarter => "quarter",
            Self::Year => "year",
        }
    }

    /// Parse the stored/wire string form ("day", "week", ...). Strict.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|g| g.as_str() == s)
    }
}

/// Brightflow's per-column additions. Every field is optional because a
/// layer may have an opinion about one field and none about the rest.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export)]
#[serde(deny_unknown_fields)]
pub struct ColumnExt {
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub role: Option<ColumnRole>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub is_kpi: Option<bool>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub polarity: Option<Polarity>,
    /// The display name. Ossie's `label` is a categorisation tag, not this.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub label: Option<String>,
}

impl ColumnExt {
    pub fn role(role: ColumnRole) -> Self {
        Self {
            role: Some(role),
            ..Self::default()
        }
    }

    #[must_use]
    pub const fn with_kpi(mut self, is_kpi: bool) -> Self {
        self.is_kpi = Some(is_kpi);
        self
    }

    #[must_use]
    pub const fn with_polarity(mut self, polarity: Polarity) -> Self {
        self.polarity = Some(polarity);
        self
    }

    #[must_use]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub const fn is_empty(&self) -> bool {
        self.role.is_none()
            && self.is_kpi.is_none()
            && self.polarity.is_none()
            && self.label.is_none()
    }
}

/// Which columns of a table make one row readable as a document.
///
/// The id to key on, the title and body to show, the timestamp to order by,
/// and how to link back to the source. `url_template` uses `{column}`
/// placeholders.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export)]
#[serde(deny_unknown_fields)]
pub struct DocFields {
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub id: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub number: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub body: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub timestamp: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub url_template: Option<String>,
}

impl DocFields {
    /// Render `url_template` against one row's values. A placeholder with no
    /// value renders empty; a missing template renders `None`.
    pub fn render_url<'a>(&self, value_of: impl Fn(&str) -> Option<&'a str>) -> Option<String> {
        let template = self.url_template.as_deref()?;
        let mut out = String::with_capacity(template.len());
        let mut rest = template;
        while let Some(start) = rest.find('{') {
            out.push_str(rest.get(..start).unwrap_or_default());
            let after = rest.get(start + 1..).unwrap_or_default();
            if let Some(end) = after.find('}') {
                let key = after.get(..end).unwrap_or_default();
                out.push_str(value_of(key).unwrap_or_default());
                rest = after.get(end + 1..).unwrap_or_default();
            } else {
                out.push_str(rest.get(start..).unwrap_or_default());
                rest = "";
            }
        }
        out.push_str(rest);
        Some(out)
    }
}

/// Brightflow's per-dataset additions.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export)]
#[serde(deny_unknown_fields)]
pub struct DatasetExt {
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub display_name: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub time_granularity: Option<TimeGranularity>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub comparison_periods: Option<u32>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub doc: Option<DocFields>,
}

/// Aggregation functions. Shared with the operations language.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq, Hash)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum Aggregation {
    Count,
    Sum,
    Avg,
    Min,
    Max,
    Median,
    Std,
    First,
    Last,
}

impl Aggregation {
    /// The ANSI SQL function name used when rendering for export.
    pub const fn sql_name(self) -> &'static str {
        match self {
            Self::Count => "COUNT",
            Self::Sum => "SUM",
            Self::Avg => "AVG",
            Self::Min => "MIN",
            Self::Max => "MAX",
            Self::Median => "MEDIAN",
            Self::Std => "STDDEV",
            Self::First => "FIRST_VALUE",
            Self::Last => "LAST_VALUE",
        }
    }
}

/// Filter comparison operators. Shared with the operations language.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq, Hash)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum FilterOp {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
    Contains,
    In,
    IsNull,
    IsNotNull,
}

/// One predicate inside a metric.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export)]
#[serde(deny_unknown_fields)]
pub struct MetricFilter {
    pub column: String,
    pub op: FilterOp,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "unknown")]
    pub value: Option<serde_json::Value>,
}

/// The executable form of a metric: one aggregation over one column, with
/// optional filters. This is what the engine compiles; the SQL in the Ossie
/// field is rendered from it for export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export)]
#[serde(deny_unknown_fields)]
pub struct MetricExpr {
    /// The dataset the column belongs to. Optional in a per-dataset
    /// declaration, where the context supplies it.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub dataset: Option<String>,
    pub column: String,
    pub aggregation: Aggregation,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<MetricFilter>,
}

impl MetricExpr {
    pub fn new(column: impl Into<String>, aggregation: Aggregation) -> Self {
        Self {
            dataset: None,
            column: column.into(),
            aggregation,
            filters: Vec::new(),
        }
    }

    #[must_use]
    pub fn in_dataset(mut self, dataset: impl Into<String>) -> Self {
        self.dataset = Some(dataset.into());
        self
    }

    #[must_use]
    pub fn with_filter(mut self, filter: MetricFilter) -> Self {
        self.filters.push(filter);
        self
    }

    fn qualified(&self, column: &str) -> String {
        match &self.dataset {
            Some(ds) => format!("{ds}.{column}"),
            None => column.to_string(),
        }
    }

    /// Render as one ANSI SQL aggregate expression. Filters become a
    /// `CASE WHEN ... THEN ... END` inside the aggregate.
    pub fn render_sql(&self) -> String {
        let target = if self.column == "*" {
            "*".to_string()
        } else {
            self.qualified(&self.column)
        };
        let inner = if self.filters.is_empty() {
            target
        } else {
            let predicate = self
                .filters
                .iter()
                .map(|f| render_predicate(&self.qualified(&f.column), f))
                .collect::<Vec<_>>()
                .join(" AND ");
            let then = if target == "*" {
                "1".to_string()
            } else {
                target
            };
            format!("CASE WHEN {predicate} THEN {then} END")
        };
        format!("{}({inner})", self.aggregation.sql_name())
    }
}

fn render_predicate(column: &str, filter: &MetricFilter) -> String {
    let value = filter.value.as_ref();
    match filter.op {
        FilterOp::Eq => format!("{column} = {}", sql_literal(value)),
        FilterOp::Ne => format!("{column} <> {}", sql_literal(value)),
        FilterOp::Gt => format!("{column} > {}", sql_literal(value)),
        FilterOp::Gte => format!("{column} >= {}", sql_literal(value)),
        FilterOp::Lt => format!("{column} < {}", sql_literal(value)),
        FilterOp::Lte => format!("{column} <= {}", sql_literal(value)),
        FilterOp::Contains => {
            let text = value
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            format!("{column} LIKE '%{}%'", text.replace('\'', "''"))
        },
        FilterOp::In => {
            let items = value
                .and_then(serde_json::Value::as_array)
                .map(|a| a.iter().map(|v| sql_literal(Some(v))).collect::<Vec<_>>())
                .unwrap_or_default();
            format!("{column} IN ({})", items.join(", "))
        },
        FilterOp::IsNull => format!("{column} IS NULL"),
        FilterOp::IsNotNull => format!("{column} IS NOT NULL"),
    }
}

fn sql_literal(value: Option<&serde_json::Value>) -> String {
    match value {
        Some(serde_json::Value::String(s)) => format!("'{}'", s.replace('\'', "''")),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        Some(serde_json::Value::Bool(b)) => if *b { "TRUE" } else { "FALSE" }.to_string(),
        Some(serde_json::Value::Null) | None => "NULL".to_string(),
        Some(other) => format!("'{}'", other.to_string().replace('\'', "''")),
    }
}

/// Brightflow's per-metric additions: the executable expression plus the
/// same display facts a measure column carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export)]
#[serde(deny_unknown_fields)]
pub struct MetricExt {
    pub expr: MetricExpr,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub is_kpi: Option<bool>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub polarity: Option<Polarity>,
    /// A display format hint (`percent`, `currency:SEK`, `integer`).
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub format: Option<String>,
}

impl MetricExt {
    pub const fn new(expr: MetricExpr) -> Self {
        Self {
            expr,
            is_kpi: None,
            polarity: None,
            format: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn column_role_parse_round_trips_the_wire_names() {
        for role in ColumnRole::ALL {
            assert_eq!(ColumnRole::parse(role.as_str()), Some(role));
            let json = serde_json::to_string(&role).unwrap();
            assert_eq!(json, format!("\"{}\"", role.as_str()));
            assert_eq!(serde_json::from_str::<ColumnRole>(&json).unwrap(), role);
        }
        // Legacy aliases are serde-only, not part of the stored format.
        assert_eq!(ColumnRole::parse("kpi"), None);
        assert_eq!(
            serde_json::from_str::<ColumnRole>("\"kpi\"").unwrap(),
            ColumnRole::Measure
        );
        assert!(serde_json::from_str::<ColumnRole>("\"colour\"").is_err());
    }

    #[test]
    fn polarity_and_granularity_round_trip() {
        for p in Polarity::ALL {
            assert_eq!(Polarity::parse(p.as_str()), Some(p));
            assert_eq!(
                serde_json::to_string(&p).unwrap(),
                format!("\"{}\"", p.as_str())
            );
        }
        for g in TimeGranularity::ALL {
            assert_eq!(TimeGranularity::parse(g.as_str()), Some(g));
            assert_eq!(
                serde_json::to_string(&g).unwrap(),
                format!("\"{}\"", g.as_str())
            );
        }
        assert_eq!(TimeGranularity::parse("fortnight"), None);
        assert_eq!(Polarity::default(), Polarity::Neutral);
        assert_eq!(TimeGranularity::default(), TimeGranularity::Week);
    }

    #[test]
    fn column_ext_serialises_only_what_is_set() {
        let ext = ColumnExt::role(ColumnRole::Measure).with_kpi(true);
        assert_eq!(
            serde_json::to_value(&ext).unwrap(),
            json!({"role": "measure", "is_kpi": true})
        );
        assert!(ColumnExt::default().is_empty());
        assert!(!ext.is_empty());
        assert!(serde_json::from_value::<ColumnExt>(json!({"colour": "red"})).is_err());
    }

    #[test]
    fn metric_sql_rendering() {
        let plain = MetricExpr::new("amount", Aggregation::Sum).in_dataset("orders");
        assert_eq!(plain.render_sql(), "SUM(orders.amount)");
        let unqualified = MetricExpr::new("id", Aggregation::Count);
        assert_eq!(unqualified.render_sql(), "COUNT(id)");
        let star = MetricExpr::new("*", Aggregation::Count).in_dataset("events");
        assert_eq!(star.render_sql(), "COUNT(*)");
        let filtered = MetricExpr::new("id", Aggregation::Count)
            .in_dataset("events")
            .with_filter(MetricFilter {
                column: "event_type".into(),
                op: FilterOp::Eq,
                value: Some(json!("pageview")),
            });
        assert_eq!(
            filtered.render_sql(),
            "COUNT(CASE WHEN events.event_type = 'pageview' THEN events.id END)"
        );
        let star_filtered = MetricExpr::new("*", Aggregation::Count).with_filter(MetricFilter {
            column: "n".into(),
            op: FilterOp::In,
            value: Some(json!([1, 2])),
        });
        assert_eq!(
            star_filtered.render_sql(),
            "COUNT(CASE WHEN n IN (1, 2) THEN 1 END)"
        );
        let escaped = MetricExpr::new("x", Aggregation::Max).with_filter(MetricFilter {
            column: "who".into(),
            op: FilterOp::Contains,
            value: Some(json!("O'Neil")),
        });
        assert_eq!(
            escaped.render_sql(),
            "MAX(CASE WHEN who LIKE '%O''Neil%' THEN x END)"
        );
        let null = MetricExpr::new("x", Aggregation::Min).with_filter(MetricFilter {
            column: "closed_at".into(),
            op: FilterOp::IsNull,
            value: None,
        });
        assert_eq!(
            null.render_sql(),
            "MIN(CASE WHEN closed_at IS NULL THEN x END)"
        );
    }

    #[test]
    fn doc_fields_render_url_template() {
        let doc = DocFields {
            url_template: Some("https://bsky.app/profile/{author_handle}/post/{rkey}".into()),
            ..Default::default()
        };
        let url = doc.render_url(|k| match k {
            "author_handle" => Some("jens.bsky.social"),
            "rkey" => Some("3kabc"),
            _ => None,
        });
        assert_eq!(
            url.as_deref(),
            Some("https://bsky.app/profile/jens.bsky.social/post/3kabc")
        );
        let missing = doc.render_url(|_| None);
        assert_eq!(missing.as_deref(), Some("https://bsky.app/profile//post/"));
        assert_eq!(DocFields::default().render_url(|_| Some("x")), None);
        let unclosed = DocFields {
            url_template: Some("a{b".into()),
            ..Default::default()
        };
        assert_eq!(unclosed.render_url(|_| None).as_deref(), Some("a{b"));
    }

    #[test]
    fn aggregation_and_filter_op_wire_names_are_camel_case() {
        assert_eq!(
            serde_json::to_string(&Aggregation::Count).unwrap(),
            "\"count\""
        );
        assert_eq!(
            serde_json::to_string(&FilterOp::IsNotNull).unwrap(),
            "\"isNotNull\""
        );
        assert_eq!(
            serde_json::from_str::<FilterOp>("\"gte\"").unwrap(),
            FilterOp::Gte
        );
    }
}
