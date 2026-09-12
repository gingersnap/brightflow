//! Layered opinions and how they resolve to one answer per field.
//!
//! The store keeps one row per (column, layer, producer): a *opinion*. A
//! producer's declaration, the detector's guess, an agent's edit and a
//! person's edit about the same column are four rows. Readers want one
//! value per field, so [`resolve_columns`] coalesces per field across
//! opinions in [`Layer::PRECEDENCE`] order, most recent first within a
//! layer. A field an opinion leaves `None` is "no opinion here", and the
//! next layer down answers; an empty string in a text field is "cleared",
//! and resolves to `None` without falling through. That is what lets a
//! person change only a label and inherit the connector's description, and
//! what lets a connector re-declare on every sync without touching a
//! person's edit.
//!
//! `resolved_by` on the result is the highest-precedence opinion that said
//! anything at all, which is what the UI shows as "from GitHub connector
//! 0.2.0" or "edited by you".

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::datatype::LogicalType;
use crate::ext::{ColumnExt, ColumnRole, DocFields, Polarity, TimeGranularity};
use crate::provenance::{Layer, Provenance};
use crate::semantic::{AiContext, CustomExtension, Field};

/// One layer's statement about one column. Every field is optional; `None`
/// is "no opinion".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export)]
pub struct ColumnOpinion {
    pub column: String,
    pub provenance: Provenance,
    /// Ordering key within a layer: a later opinion from the same layer
    /// wins. Unix seconds; `0` when unknown.
    #[serde(default)]
    pub updated_at: i64,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub datatype: Option<LogicalType>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub is_time: Option<bool>,
    #[serde(default)]
    pub ext: ColumnExt,
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
    pub custom_extensions: Vec<CustomExtension>,
}

impl ColumnOpinion {
    /// An opinion with nothing in it yet.
    pub fn empty(column: impl Into<String>, provenance: Provenance) -> Self {
        Self {
            column: column.into(),
            provenance,
            updated_at: 0,
            datatype: None,
            is_time: None,
            ext: ColumnExt::default(),
            description: None,
            ai_context: None,
            custom_extensions: Vec::new(),
        }
    }

    /// A declared field, as the producer stated it. The `BRIGHTFLOW`
    /// extension is lifted into `ext`; every other extension stays in the
    /// bag.
    pub fn from_field(field: &Field, provenance: &Provenance) -> Self {
        Self {
            column: field.name.clone(),
            provenance: provenance.clone(),
            updated_at: 0,
            datatype: field.datatype,
            is_time: field.dimension.and_then(|d| d.is_time),
            ext: field.brightflow().unwrap_or_default(),
            description: field.description.clone(),
            ai_context: field.ai_context.clone(),
            custom_extensions: field
                .custom_extensions
                .iter()
                .filter(|e| e.vendor_name != crate::ext::BRIGHTFLOW_VENDOR)
                .cloned()
                .collect(),
        }
    }

    /// `true` when this opinion states nothing.
    pub fn is_empty(&self) -> bool {
        self.datatype.is_none()
            && self.is_time.is_none()
            && self.ext.is_empty()
            && self.description.is_none()
            && self.ai_context.is_none()
            && self.custom_extensions.is_empty()
    }
}

/// One layer's statement about a table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export)]
pub struct TableOpinion {
    pub provenance: Provenance,
    #[serde(default)]
    pub updated_at: i64,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub display_name: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
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
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ai_context: Option<AiContext>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub custom_extensions: Vec<CustomExtension>,
}

impl TableOpinion {
    pub fn empty(provenance: Provenance) -> Self {
        Self {
            provenance,
            updated_at: 0,
            display_name: None,
            description: None,
            time_granularity: None,
            comparison_periods: None,
            doc: None,
            ai_context: None,
            custom_extensions: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.display_name.is_none()
            && self.description.is_none()
            && self.time_granularity.is_none()
            && self.comparison_periods.is_none()
            && self.doc.is_none()
            && self.ai_context.is_none()
            && self.custom_extensions.is_empty()
    }
}

/// One column as every reader sees it: the winning value per field.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export)]
pub struct ResolvedColumn {
    pub name: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub datatype: Option<LogicalType>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub is_time: Option<bool>,
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
    pub ai_context: Option<AiContext>,
    /// The highest-precedence opinion that said anything about this column.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub resolved_by: Option<Provenance>,
}

impl ResolvedColumn {
    /// Ossie's `is_time` rule over the resolved values.
    pub fn resolved_is_time(&self) -> bool {
        match self.is_time {
            Some(explicit) => explicit,
            None => self.datatype.is_some_and(LogicalType::is_temporal),
        }
    }

    /// The Brightflow extension view of the resolved values.
    pub fn ext(&self) -> ColumnExt {
        ColumnExt {
            role: self.role,
            is_kpi: self.is_kpi,
            polarity: self.polarity,
            label: self.label.clone(),
        }
    }

    /// Back to a declarable field, for export.
    pub fn to_field(&self) -> Field {
        let mut field = Field::column(self.name.clone());
        field.datatype = self.datatype;
        field.dimension = self.is_time.map(|is_time| crate::semantic::Dimension {
            is_time: Some(is_time),
        });
        field.description.clone_from(&self.description);
        field.ai_context.clone_from(&self.ai_context);
        let ext = self.ext();
        if !ext.is_empty() {
            field.set_brightflow(&ext);
        }
        field
    }
}

/// One table as every reader sees it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export)]
pub struct ResolvedTable {
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub display_name: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
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
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ai_context: Option<AiContext>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub resolved_by: Option<Provenance>,
}

/// Sort opinions into resolution order: highest layer first, most recent
/// first within a layer.
fn precedence_order<T>(items: &mut [T], layer_of: impl Fn(&T) -> Layer, at: impl Fn(&T) -> i64) {
    let rank = |l: Layer| {
        Layer::PRECEDENCE
            .iter()
            .position(|p| *p == l)
            .unwrap_or(usize::MAX)
    };
    items.sort_by(|a, b| {
        rank(layer_of(a))
            .cmp(&rank(layer_of(b)))
            .then_with(|| at(b).cmp(&at(a)))
    });
}

/// First opinion with a value; an empty string means "cleared" and stops
/// the search with `None`.
fn first_text<'a, T>(items: &'a [T], get: impl Fn(&'a T) -> Option<&'a str>) -> Option<String> {
    items
        .iter()
        .find_map(get)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn first_value<'a, T, V: Copy + 'a>(items: &'a [T], get: impl Fn(&'a T) -> Option<V>) -> Option<V> {
    items.iter().find_map(get)
}

/// Resolve every column that has at least one opinion. Output is ordered by
/// column name.
pub fn resolve_columns(opinions: &[ColumnOpinion]) -> Vec<ResolvedColumn> {
    let mut names: Vec<&str> = opinions.iter().map(|o| o.column.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    names
        .into_iter()
        .map(|name| {
            let mut mine: Vec<&ColumnOpinion> =
                opinions.iter().filter(|o| o.column == name).collect();
            precedence_order(&mut mine, |o| o.provenance.layer, |o| o.updated_at);
            ResolvedColumn {
                name: name.to_string(),
                datatype: first_value(&mine, |o| o.datatype),
                is_time: first_value(&mine, |o| o.is_time),
                role: first_value(&mine, |o| o.ext.role),
                is_kpi: first_value(&mine, |o| o.ext.is_kpi),
                polarity: first_value(&mine, |o| o.ext.polarity),
                label: first_text(&mine, |o| o.ext.label.as_deref()),
                description: first_text(&mine, |o| o.description.as_deref()),
                ai_context: mine.iter().find_map(|o| o.ai_context.clone()),
                resolved_by: mine
                    .iter()
                    .find(|o| !o.is_empty())
                    .map(|o| o.provenance.clone()),
            }
        })
        .collect()
}

/// Resolve a table's opinions, or `None` when there are none.
pub fn resolve_table(opinions: &[TableOpinion]) -> Option<ResolvedTable> {
    if opinions.is_empty() {
        return None;
    }
    let mut mine: Vec<&TableOpinion> = opinions.iter().collect();
    precedence_order(&mut mine, |o| o.provenance.layer, |o| o.updated_at);
    Some(ResolvedTable {
        display_name: first_text(&mine, |o| o.display_name.as_deref()),
        description: first_text(&mine, |o| o.description.as_deref()),
        time_granularity: first_value(&mine, |o| o.time_granularity),
        comparison_periods: first_value(&mine, |o| o.comparison_periods),
        doc: mine.iter().find_map(|o| o.doc.clone()),
        ai_context: mine.iter().find_map(|o| o.ai_context.clone()),
        resolved_by: mine
            .iter()
            .find(|o| !o.is_empty())
            .map(|o| o.provenance.clone()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declared(column: &str) -> ColumnOpinion {
        let mut o = ColumnOpinion::empty(
            column,
            Provenance::declared("connector:github").with_version("0.2.0"),
        );
        o.datatype = Some(LogicalType::Integer);
        o.ext = ColumnExt::role(ColumnRole::Measure).with_kpi(true);
        o.description = Some("Reactions on the issue".into());
        o
    }

    fn user(column: &str) -> ColumnOpinion {
        ColumnOpinion::empty(
            column,
            Provenance {
                layer: Layer::User,
                producer: "user:1".into(),
                version: None,
                hash: None,
            },
        )
    }

    #[test]
    fn a_user_label_wins_and_the_rest_falls_through() {
        let mut edit = user("reactions_total");
        edit.ext.label = Some("Reactions".into());
        let resolved = resolve_columns(&[declared("reactions_total"), edit]);
        assert_eq!(resolved.len(), 1);
        let r = &resolved[0];
        assert_eq!(r.label.as_deref(), Some("Reactions"));
        assert_eq!(r.role, Some(ColumnRole::Measure));
        assert_eq!(r.is_kpi, Some(true));
        assert_eq!(r.description.as_deref(), Some("Reactions on the issue"));
        assert_eq!(r.datatype, Some(LogicalType::Integer));
        assert_eq!(r.resolved_by.as_ref().map(|p| p.layer), Some(Layer::User));
    }

    #[test]
    fn an_empty_string_clears_a_lower_layers_text() {
        let mut edit = user("reactions_total");
        edit.description = Some(String::new());
        let resolved = resolve_columns(&[declared("reactions_total"), edit]);
        assert_eq!(resolved[0].description, None);
    }

    #[test]
    fn an_empty_user_row_does_not_claim_resolution() {
        let resolved = resolve_columns(&[declared("c"), user("c")]);
        assert_eq!(
            resolved[0]
                .resolved_by
                .as_ref()
                .map(|p| p.producer.as_str()),
            Some("connector:github")
        );
    }

    #[test]
    fn within_a_layer_the_most_recent_wins() {
        let mut a = declared("c");
        a.provenance.producer = "enrichment:a".into();
        a.updated_at = 10;
        a.description = Some("older".into());
        let mut b = declared("c");
        b.provenance.producer = "enrichment:b".into();
        b.updated_at = 20;
        b.description = Some("newer".into());
        let resolved = resolve_columns(&[a, b]);
        assert_eq!(resolved[0].description.as_deref(), Some("newer"));
    }

    #[test]
    fn layers_rank_user_agent_declared_detected() {
        let mut detected = ColumnOpinion::empty("c", Provenance::detected());
        detected.ext.role = Some(ColumnRole::Dimension);
        let mut agent = ColumnOpinion::empty(
            "c",
            Provenance {
                layer: Layer::Agent,
                producer: "agent:7".into(),
                version: None,
                hash: None,
            },
        );
        agent.ext.role = Some(ColumnRole::Entity);
        let mut declared_row = declared("c");
        declared_row.ext.role = Some(ColumnRole::Measure);
        let all_three = resolve_columns(&[detected.clone(), declared_row.clone(), agent]);
        assert_eq!(all_three[0].role, Some(ColumnRole::Entity));
        let two = resolve_columns(&[detected.clone(), declared_row]);
        assert_eq!(two[0].role, Some(ColumnRole::Measure));
        let one = resolve_columns(&[detected]);
        assert_eq!(one[0].role, Some(ColumnRole::Dimension));
    }

    #[test]
    fn columns_come_out_sorted_and_deduplicated() {
        let resolved = resolve_columns(&[declared("b"), declared("a"), user("b")]);
        assert_eq!(
            resolved.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            ["a", "b"]
        );
    }

    #[test]
    fn from_field_lifts_the_brightflow_extension() {
        let field = Field::column("n")
            .with_datatype(LogicalType::Float)
            .with_is_time(false)
            .with_brightflow(&ColumnExt::role(ColumnRole::Measure).with_label("N"));
        let o = ColumnOpinion::from_field(&field, &Provenance::declared("p"));
        assert_eq!(o.datatype, Some(LogicalType::Float));
        assert_eq!(o.is_time, Some(false));
        assert_eq!(o.ext.label.as_deref(), Some("N"));
        assert!(o.custom_extensions.is_empty());
        assert!(!o.is_empty());
        assert!(ColumnOpinion::empty("x", Provenance::detected()).is_empty());
    }

    #[test]
    fn resolved_column_round_trips_to_a_field() {
        let resolved = resolve_columns(&[declared("reactions_total")]);
        let field = resolved[0].to_field();
        assert_eq!(field.name, "reactions_total");
        assert_eq!(field.datatype, Some(LogicalType::Integer));
        assert_eq!(field.brightflow().unwrap().is_kpi, Some(true));
        assert_eq!(field.description.as_deref(), Some("Reactions on the issue"));
        assert!(resolve_columns(&[])[..].is_empty());
    }

    #[test]
    fn table_resolution_follows_the_same_rules() {
        assert_eq!(resolve_table(&[]), None);
        let mut declared_t = TableOpinion::empty(Provenance::declared("connector:github"));
        declared_t.display_name = Some("Issues".into());
        declared_t.time_granularity = Some(TimeGranularity::Week);
        declared_t.comparison_periods = Some(4);
        let mut user_t = TableOpinion::empty(Provenance {
            layer: Layer::User,
            producer: "user:1".into(),
            version: None,
            hash: None,
        });
        user_t.time_granularity = Some(TimeGranularity::Month);
        let r = resolve_table(&[declared_t, user_t]).unwrap();
        assert_eq!(r.display_name.as_deref(), Some("Issues"));
        assert_eq!(r.time_granularity, Some(TimeGranularity::Month));
        assert_eq!(r.comparison_periods, Some(4));
        assert_eq!(r.resolved_by.map(|p| p.layer), Some(Layer::User));
    }
}
