//! Column semantics: role, polarity, and time granularity.
//!
//! This is what turns a table of numbers into something analyzable — which
//! columns are measures, which are dimensions to slice by, and whether "up" is
//! good. Without polarity the engine can detect a change but cannot say whether
//! it is good news, which is most of what a reader wants.
//!
//! These three enums are the one vocabulary for role, polarity and granularity
//! everywhere they cross a boundary: stored strings (`as_str`/`parse`), the
//! typed frontend (`ts-rs`), and the LLM action manifest (`schemars`). No
//! other crate defines a mirror of them.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Serialize, TS, JsonSchema, PartialEq, Eq)]
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
/// Display-only in v1 — scoring deliberately ignores it (hook point:
/// `scoring::kpi_boost_for`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, JsonSchema, PartialEq, Eq, Default)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum Polarity {
    HigherIsBetter,
    LowerIsBetter,
    #[default]
    Neutral,
}

impl Polarity {
    pub const fn as_str(self) -> &'static str {
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

/// The period a time column is bucketed into. `Week` is the default because
/// it is the coarsest unit that still shows a month's shape.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, JsonSchema, PartialEq, Eq, Default)]
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

    #[test]
    fn as_str_and_parse_are_inverses_for_every_enum() {
        for role in [
            ColumnRole::Measure,
            ColumnRole::Dimension,
            ColumnRole::Time,
            ColumnRole::Entity,
            ColumnRole::Ignored,
        ] {
            assert_eq!(ColumnRole::parse(role.as_str()), Some(role));
        }
        for polarity in [
            Polarity::HigherIsBetter,
            Polarity::LowerIsBetter,
            Polarity::Neutral,
        ] {
            assert_eq!(Polarity::parse(polarity.as_str()), Some(polarity));
        }
        for granularity in TimeGranularity::ALL {
            assert_eq!(
                TimeGranularity::parse(granularity.as_str()),
                Some(granularity)
            );
        }
        assert_eq!(TimeGranularity::parse("fortnight"), None);
    }

    /// The serde names are the stored names: a row written by `as_str` must
    /// deserialize, and a value serialized by serde must `parse`.
    #[test]
    fn serde_names_match_the_stored_names() {
        for role in [ColumnRole::Measure, ColumnRole::Time, ColumnRole::Ignored] {
            let json = serde_json::to_string(&role).unwrap();
            assert_eq!(json, format!("\"{}\"", role.as_str()));
            assert_eq!(serde_json::from_str::<ColumnRole>(&json).unwrap(), role);
        }
        assert_eq!(
            serde_json::to_string(&Polarity::HigherIsBetter).unwrap(),
            "\"higher_is_better\""
        );
        assert_eq!(
            serde_json::to_string(&TimeGranularity::Quarter).unwrap(),
            "\"quarter\""
        );
        // Legacy aliases still deserialize to Measure.
        assert_eq!(
            serde_json::from_str::<ColumnRole>("\"kpi\"").unwrap(),
            ColumnRole::Measure
        );
    }
}
