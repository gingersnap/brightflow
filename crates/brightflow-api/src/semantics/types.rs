//! Wire types for the semantics endpoints.
//!
//! The column shape on the wire is the contract crate's `ResolvedColumn`,
//! the same type `ColumnInfo` embeds, so the two routes that describe a
//! column cannot spell it differently.

use brightflow_types::{
    ColumnOpinion, DeclarationDiff, OssieDocument, ResolvedColumn, ResolvedTable, SemanticModel,
    TimeGranularity,
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

/// What a producer's re-declarations changed, newest first.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DeclarationChangesResponse {
    pub table_name: String,
    pub changes: Vec<DeclarationDiff>,
}

/// `?dry_run=true` on the import: validate and report, write nothing.
#[derive(Debug, Default, Deserialize)]
pub struct ImportQuery {
    pub dry_run: Option<bool>,
}

/// What importing a model did, or would do, per table.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ImportedTable {
    pub table: String,
    pub columns: usize,
    pub relationships: usize,
    pub metrics: usize,
    /// Declared columns the table's data does not have.
    pub columns_without_data: Vec<String>,
    /// Metrics with no structured expression, which the store cannot run.
    pub metrics_skipped: Vec<String>,
    /// Relationships whose target table this source does not have.
    pub relationships_skipped: Vec<String>,
}

/// The result of importing a semantic model into a source.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SemanticModelImportResponse {
    /// The producer the rows were filed under: `document:{model name}`.
    pub producer: String,
    pub dry_run: bool,
    pub applied: Vec<ImportedTable>,
    /// Datasets in the model with no table in this source.
    pub missing_tables: Vec<String>,
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
