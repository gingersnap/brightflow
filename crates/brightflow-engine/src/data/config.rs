//! Column semantics: role, polarity, and time granularity.
//!
//! This is what turns a table of numbers into something analyzable — which
//! columns are measures, which are dimensions to slice by, and whether "up" is
//! good. Without polarity the engine can detect a change but cannot say whether
//! it is good news, which is most of what a reader wants.

use serde::{Deserialize, Serialize};

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

impl ColumnRole {
    /// Parse the stored/wire string form ("measure", "dimension", ...).
    /// Strict — legacy aliases are only accepted by the serde deserializer.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "measure" => Some(Self::Measure),
            "dimension" => Some(Self::Dimension),
            "time" => Some(Self::Time),
            "entity" => Some(Self::Entity),
            "ignored" => Some(Self::Ignored),
            _ => None,
        }
    }
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

/// Which direction of movement in a measure is good news.
///
/// Rising churn is not rising revenue; display code keys off this.
/// Display-only in v1 — scoring deliberately ignores it (hook point:
/// `scoring::kpi_boost_for`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Polarity {
    HigherIsBetter,
    LowerIsBetter,
    #[default]
    Neutral,
}

impl Polarity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HigherIsBetter => "higher_is_better",
            Self::LowerIsBetter => "lower_is_better",
            Self::Neutral => "neutral",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "higher_is_better" => Some(Self::HigherIsBetter),
            "lower_is_better" => Some(Self::LowerIsBetter),
            "neutral" => Some(Self::Neutral),
            _ => None,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_role_parse_round_trips_the_wire_names() {
        assert_eq!(ColumnRole::parse("measure"), Some(ColumnRole::Measure));
        assert_eq!(ColumnRole::parse("dimension"), Some(ColumnRole::Dimension));
        assert_eq!(ColumnRole::parse("time"), Some(ColumnRole::Time));
        assert_eq!(ColumnRole::parse("entity"), Some(ColumnRole::Entity));
        assert_eq!(ColumnRole::parse("ignored"), Some(ColumnRole::Ignored));
        // Legacy aliases are serde-only, not part of the stored format.
        assert_eq!(ColumnRole::parse("kpi"), None);
        assert_eq!(ColumnRole::parse(""), None);
    }
}
