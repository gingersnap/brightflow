//! Column semantics: role, polarity, and time granularity.
//!
//! This is what turns a table of numbers into something analyzable — which
//! columns are measures, which are dimensions to slice by, and whether "up" is
//! good. Without polarity the engine can detect a change but cannot say whether
//! it is good news, which is most of what a reader wants.
//!
//! The three enums are defined in `brightflow-types`, the contract crate every
//! producer, the store and the API share; this module re-exports them so the
//! engine's own paths (`data::config::ColumnRole`) keep working. Nothing here
//! is a mirror — a second definition would be exactly the drift the contract
//! crate exists to prevent.

pub use brightflow_types::{ColumnRole, Polarity, TimeGranularity};

#[cfg(test)]
mod tests {
    use super::*;

    /// The engine's paths resolve to the contract crate's types: one
    /// vocabulary, re-exported, not copied.
    #[test]
    fn re_exports_are_the_contract_types() {
        assert_eq!(ColumnRole::parse("measure"), Some(ColumnRole::Measure));
        assert_eq!(Polarity::default(), Polarity::Neutral);
        assert_eq!(TimeGranularity::default(), TimeGranularity::Week);
        assert_eq!(
            serde_json::from_str::<ColumnRole>("\"kpi\"").unwrap(),
            ColumnRole::Measure
        );
    }
}
