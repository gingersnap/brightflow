//! The operations language: the typed chain a query or a model recipe is
//! made of.
//!
//! A chain is a *sequence* of operations rather than a fixed struct of
//! optional clauses, because the query builder lets people compose filter,
//! group, sort and limit in any order and the result depends on that order.
//! It crosses three boundaries: the query API (a session runs it), the store
//! (a model's recipe is one, stored as JSON and versioned), and the action
//! manifest (an agent emits one through a JSON Schema). That is why it lives
//! here with `Serialize` and `JsonSchema` and not in the API crate.
//!
//! Derived columns (`WithColumns`) are a typed expression tree, not text:
//! each `DerivedExpr` variant names what it computes and the API's executor
//! is its only compiler. The tree grows by adding variants; it never gains a
//! raw expression-string escape hatch.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ext::{Aggregation, FilterOp, TimeGranularity};

/// Each operation transforms the frame it receives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Operation {
    /// Filter rows by column condition
    Filter {
        column: String,
        op: FilterOp,
        #[serde(default)]
        #[ts(type = "unknown")]
        value: serde_json::Value,
    },

    /// Select specific columns
    Select { columns: Vec<String> },

    /// Group by columns with aggregations
    GroupBy { by: Vec<String>, aggs: Vec<AggSpec> },

    /// Pivot table transformation
    Pivot {
        index: Vec<String>,
        columns: String,
        values: String,
        #[serde(default)]
        agg: Option<Aggregation>,
    },

    /// Sort by column(s)
    Sort {
        by: String,
        #[serde(default)]
        descending: bool,
    },

    /// Limit number of rows returned
    Limit { n: u32 },

    /// Add derived columns computed from existing ones. Placed before a
    /// `GroupBy` or `Pivot`, the derived names can be grouped on like any
    /// other column.
    WithColumns { columns: Vec<DerivedColumn> },
}

/// One derived column: its output name and how it is computed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DerivedColumn {
    pub name: String,
    pub expr: DerivedExpr,
}

/// The expression behind a derived column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export)]
#[serde(tag = "fn", rename_all = "camelCase")]
pub enum DerivedExpr {
    /// The period label of a time column at a granularity, as a string in the
    /// engine's format (`2024-03-01`, `2024-W11`, `2024-03`, `2024-Q1`,
    /// `2024`), so Explore and the insights feed name the same week the same
    /// way and label order is chronological. Accepts `Date`, `Datetime`, and
    /// `String` columns holding ISO dates (the first ten characters are
    /// parsed; anything else buckets to null).
    Period {
        column: String,
        granularity: TimeGranularity,
    },
}

/// Aggregation specification for GroupBy
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export)]
pub struct AggSpec {
    /// Column to aggregate ("*" for count)
    pub column: String,
    /// Aggregation function
    pub function: Aggregation,
    /// Optional output column name
    #[serde(default)]
    pub alias: Option<String>,
}

/// The recipe format this crate writes. A newer recipe deserialises (the
/// fields are additive) but `validate` refuses it, so an older binary never
/// runs a chain it may misread.
pub const RECIPE_VERSION: u32 = 1;

/// A model's recipe: the chain that builds its output from its input. The
/// input is the model's scope, not part of the recipe, so an agent's tool
/// schema is the name and the chain and nothing else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ModelRecipe {
    #[serde(default = "default_recipe_version")]
    pub version: u32,
    pub operations: Vec<Operation>,
}

const fn default_recipe_version() -> u32 {
    RECIPE_VERSION
}

/// Why a recipe cannot be run as it stands.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RecipeError {
    #[error("recipe version {0} is newer than this build understands ({RECIPE_VERSION})")]
    VersionTooNew(u32),
    #[error("a recipe needs at least one operation")]
    Empty,
    #[error("recipe references column '{0}', which the input does not have")]
    UnknownColumn(String),
}

impl ModelRecipe {
    pub fn new(operations: Vec<Operation>) -> Self {
        Self {
            version: RECIPE_VERSION,
            operations,
        }
    }

    /// Structural checks that need no data: version, non-empty, and every
    /// column the chain reads exists in the input or is derived earlier in
    /// the chain. After a `GroupBy` or `Pivot` the frame is a new shape whose
    /// columns depend on data, so reads past that point are not checked.
    pub fn validate(&self, input_columns: &[String]) -> Result<(), RecipeError> {
        if self.version > RECIPE_VERSION {
            return Err(RecipeError::VersionTooNew(self.version));
        }
        if self.operations.is_empty() {
            return Err(RecipeError::Empty);
        }
        let mut known: Vec<String> = input_columns.to_vec();
        let mut reshaped = false;
        for op in &self.operations {
            if !reshaped {
                for column in op.reads() {
                    if !known.iter().any(|k| k == column) {
                        return Err(RecipeError::UnknownColumn(column.to_string()));
                    }
                }
            }
            if let Operation::WithColumns { columns } = op {
                known.extend(columns.iter().map(|c| c.name.clone()));
            }
            reshaped |= op.reshapes();
        }
        Ok(())
    }
}

impl Operation {
    /// The input columns this operation reads by name. `GroupBy` aggregates
    /// over `"*"` read no column; a `Pivot` and `GroupBy` change the frame's
    /// shape, so nothing after them is checked against the input.
    pub fn reads(&self) -> Vec<&str> {
        match self {
            Self::Filter { column, .. } => vec![column.as_str()],
            Self::Select { columns } => columns.iter().map(String::as_str).collect(),
            Self::GroupBy { by, aggs } => by
                .iter()
                .map(String::as_str)
                .chain(aggs.iter().map(|a| a.column.as_str()).filter(|c| *c != "*"))
                .collect(),
            Self::Pivot {
                index,
                columns,
                values,
                ..
            } => index
                .iter()
                .map(String::as_str)
                .chain([columns.as_str(), values.as_str()])
                .collect(),
            Self::Sort { by, .. } => vec![by.as_str()],
            Self::Limit { .. } => Vec::new(),
            Self::WithColumns { columns } => columns
                .iter()
                .map(|c| match &c.expr {
                    DerivedExpr::Period { column, .. } => column.as_str(),
                })
                .collect(),
        }
    }

    /// Whether the operation replaces the frame's columns with new ones.
    pub const fn reshapes(&self) -> bool {
        matches!(self, Self::GroupBy { .. } | Self::Pivot { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain() -> Vec<Operation> {
        vec![
            Operation::Filter {
                column: "region".into(),
                op: FilterOp::Eq,
                value: serde_json::json!("EU"),
            },
            Operation::WithColumns {
                columns: vec![DerivedColumn {
                    name: "order_date__month".into(),
                    expr: DerivedExpr::Period {
                        column: "order_date".into(),
                        granularity: TimeGranularity::Month,
                    },
                }],
            },
            Operation::GroupBy {
                by: vec!["order_date__month".into()],
                aggs: vec![AggSpec {
                    column: "revenue".into(),
                    function: Aggregation::Sum,
                    alias: Some("sum".into()),
                }],
            },
            Operation::Sort {
                by: "sum".into(),
                descending: true,
            },
            Operation::Limit { n: 10 },
        ]
    }

    #[test]
    fn a_chain_round_trips_through_json_with_its_tags() {
        let recipe = ModelRecipe::new(chain());
        let json = serde_json::to_value(&recipe).unwrap();
        assert_eq!(json["version"], 1);
        assert_eq!(json["operations"][0]["type"], "filter");
        assert_eq!(json["operations"][1]["columns"][0]["expr"]["fn"], "period");
        let back: ModelRecipe = serde_json::from_value(json).unwrap();
        assert_eq!(back, recipe);
    }

    #[test]
    fn the_schema_names_every_operation() {
        let schema = serde_json::to_value(schemars::schema_for!(Operation)).unwrap();
        let tags: Vec<&str> = schema["oneOf"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["properties"]["type"]["const"].as_str().unwrap())
            .collect();
        assert_eq!(
            tags,
            [
                "filter",
                "select",
                "groupBy",
                "pivot",
                "sort",
                "limit",
                "withColumns"
            ]
        );
    }

    #[test]
    fn validate_checks_version_emptiness_and_columns() {
        let input = ["region", "order_date", "revenue"].map(String::from);
        assert_eq!(ModelRecipe::new(chain()).validate(&input), Ok(()));

        let newer: ModelRecipe =
            serde_json::from_value(serde_json::json!({ "version": 2, "operations": [] })).unwrap();
        assert_eq!(newer.validate(&input), Err(RecipeError::VersionTooNew(2)));

        assert_eq!(
            ModelRecipe::new(vec![]).validate(&input),
            Err(RecipeError::Empty)
        );

        let bad = ModelRecipe::new(vec![Operation::Sort {
            by: "nope".into(),
            descending: false,
        }]);
        assert_eq!(
            bad.validate(&input),
            Err(RecipeError::UnknownColumn("nope".into()))
        );

        // A derived column is readable after it is introduced, and anything
        // goes after an aggregate reshapes the frame.
        let after_group = ModelRecipe::new(vec![
            Operation::GroupBy {
                by: vec!["region".into()],
                aggs: vec![AggSpec {
                    column: "*".into(),
                    function: Aggregation::Count,
                    alias: None,
                }],
            },
            Operation::Sort {
                by: "count".into(),
                descending: true,
            },
        ]);
        assert_eq!(after_group.validate(&input), Ok(()));
    }

    #[test]
    fn a_version_field_defaults_when_absent() {
        let recipe: ModelRecipe = serde_json::from_value(
            serde_json::json!({ "operations": [{ "type": "limit", "n": 5 }] }),
        )
        .unwrap();
        assert_eq!(recipe.version, RECIPE_VERSION);
    }
}
