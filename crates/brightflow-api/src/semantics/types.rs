//! Wire types for the semantics endpoints.
//!
//! The column shape on the wire is the contract crate's `ResolvedColumn`,
//! the same type `ColumnInfo` embeds, so the two routes that describe a
//! column cannot spell it differently.

use brightflow_types::{
    ColumnOpinion, OssieDocument, ResolvedColumn, ResolvedTable, SemanticModel, TimeGranularity,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// `?layers=1` on the semantics listing.
#[derive(Debug, Default, Deserialize)]
pub struct LayersQuery {
    pub layers: Option<bool>,
}

/// The resolved columns of a table, and the opinion rows behind them when
/// asked for.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ColumnSemanticsResponse {
    pub table_name: String,
    pub columns: Vec<ResolvedColumn>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub layers: Option<Vec<ColumnOpinion>>,
}

/// The resolved table settings. Writes go through `set_table_settings` on
/// the action bus.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TableSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub time_granularity: Option<TimeGranularity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub comparison_periods: Option<u32>,
}

impl From<ResolvedTable> for TableSettings {
    fn from(r: ResolvedTable) -> Self {
        Self {
            display_name: r.display_name,
            description: r.description,
            time_granularity: r.time_granularity,
            comparison_periods: r.comparison_periods,
        }
    }
}

/// Response for table analysis settings
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TableSettingsResponse {
    pub table_name: String,
    #[serde(flatten)]
    pub settings: TableSettings,
}

/// One source as an Ossie document.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct SemanticModelResponse {
    #[serde(flatten)]
    pub document: OssieDocument,
}

impl SemanticModelResponse {
    pub fn new(model: SemanticModel) -> Self {
        Self {
            document: OssieDocument::new(model),
        }
    }
}
