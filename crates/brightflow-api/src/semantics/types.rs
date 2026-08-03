//! Wire types for the column-semantics endpoints.
//!
//! Role and polarity cross the wire as strings rather than enums so an unknown
//! value from an older or newer client degrades to a warning at parse time
//! instead of rejecting the whole request.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A column semantic override (request/response)
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ColumnSemantic {
    pub column_name: String,
    /// One of: measure, dimension, time, entity, ignored
    pub role: String,
    #[serde(default)]
    pub is_kpi: bool,
    /// One of: higher_is_better, lower_is_better, neutral
    #[serde(default = "default_polarity")]
    pub polarity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

fn default_polarity() -> String {
    "neutral".to_string()
}

/// Response listing all column semantics for a table
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ColumnSemanticsResponse {
    pub table_name: String,
    pub columns: Vec<ColumnSemantic>,
}

/// Table analysis settings (request/response)
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TableSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_granularity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comparison_periods: Option<i32>,
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
