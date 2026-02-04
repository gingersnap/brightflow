use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ColumnRole {
    /// Primary business outcomes - what leadership monitors
    Kpi,
    /// Supporting metrics that help explain KPIs
    Metric,
    /// Categorical columns for slicing/segmentation
    Dimension,
    /// Temporal columns for time-series analysis
    Time,
    /// Columns to skip (IDs, internal fields, PII)
    Ignored,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
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
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read schema config from {}", path.display()))?;
        let config: Self = serde_yaml::from_str(&content)
            .with_context(|| format!("Failed to parse schema config from {}", path.display()))?;
        Ok(config)
    }

    pub fn to_role_map(&self) -> HashMap<String, ColumnRole> {
        self.columns
            .iter()
            .map(|c| (c.name.clone(), c.role.clone()))
            .collect()
    }

    pub fn kpi_columns(&self) -> Vec<String> {
        self.columns
            .iter()
            .filter(|c| c.role == ColumnRole::Kpi)
            .map(|c| c.name.clone())
            .collect()
    }

    pub fn metric_columns(&self) -> Vec<String> {
        self.columns
            .iter()
            .filter(|c| c.role == ColumnRole::Metric)
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
