use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
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

/// Custom deserializer that accepts legacy "kpi"/"metric" as aliases for "measure"
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum TimeGranularity {
    Day,
    #[default]
    Week,
    Month,
    Quarter,
    Year,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnConfig {
    pub name: String,
    #[serde(rename = "type")]
    pub role: ColumnRole,
    /// Whether this measure is a KPI (only meaningful when role == Measure)
    #[serde(default)]
    pub is_kpi: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// When deserializing legacy TOML, detect "kpi" type and set is_kpi=true.
/// This is handled via a post-processing step in SchemaConfig deserialization.
impl ColumnConfig {
    /// Fix up is_kpi for legacy "kpi"/"metric" type values.
    /// Called after raw deserialization where the custom ColumnRole deserializer
    /// already mapped "kpi" → Measure, but we need to also set is_kpi.
    pub fn fixup_legacy_kpi(raw_type: &str) -> bool {
        raw_type == "kpi"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AnalysisSettings {
    /// Time granularity for period comparisons
    #[serde(default)]
    pub time_granularity: TimeGranularity,
    /// Number of periods to compare (default: 4)
    #[serde(default = "default_comparison_periods")]
    pub comparison_periods: usize,
}

fn default_comparison_periods() -> usize {
    4
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub settings: AnalysisSettings,
    pub columns: Vec<ColumnConfig>,
}

impl SchemaConfig {
    pub fn to_role_map(&self) -> HashMap<String, ColumnRole> {
        self.columns
            .iter()
            .map(|c| (c.name.clone(), c.role.clone()))
            .collect()
    }

    /// All measure columns (both KPIs and non-KPI metrics)
    pub fn measure_columns(&self) -> Vec<String> {
        self.columns
            .iter()
            .filter(|c| c.role == ColumnRole::Measure)
            .map(|c| c.name.clone())
            .collect()
    }

    /// Only KPI columns (measures with is_kpi=true)
    pub fn kpi_columns(&self) -> Vec<String> {
        self.columns
            .iter()
            .filter(|c| c.role == ColumnRole::Measure && c.is_kpi)
            .map(|c| c.name.clone())
            .collect()
    }

    pub fn dimension_columns(&self) -> Vec<String> {
        self.columns
            .iter()
            .filter(|c| c.role == ColumnRole::Dimension)
            .map(|c| c.name.clone())
            .collect()
    }

    pub fn time_columns(&self) -> Vec<String> {
        self.columns
            .iter()
            .filter(|c| c.role == ColumnRole::Time)
            .map(|c| c.name.clone())
            .collect()
    }

    pub fn ignored_columns(&self) -> Vec<String> {
        self.columns
            .iter()
            .filter(|c| c.role == ColumnRole::Ignored)
            .map(|c| c.name.clone())
            .collect()
    }
}
