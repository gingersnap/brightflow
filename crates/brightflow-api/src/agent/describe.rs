//! The `describe_table` agent: what the model sees when it is asked to write
//! the `agent` layer of one table's semantics.
//!
//! The runner appends the table's resolved semantics to every system prompt;
//! this module adds what that block cannot say — a profile of each column
//! (distinct count, null share, a few values, the range) and a handful of
//! rows spread over the table — and marks which semantic fields no layer has
//! filled. The system prompt asks the model to fill those through the
//! semantic actions, to leave what a producer or a person already set unless
//! it is plainly wrong, and to name whatever it overrules in its closing
//! note. Every call lands on the action bus at the agent layer, so approval,
//! provenance and undo are the bus's, not this module's.

use polars::prelude::*;
use serde::Serialize;
use serde_json::{json, Value};

use brightflow_types::{ColumnRole, LogicalType, Polarity, ResolvedColumn, ResolvedTable};

use crate::shared::AppResult;
use crate::state::AppState;

/// Rows shown to the model, spread evenly over the table.
pub const SAMPLE_ROWS: usize = 15;
/// Distinct values shown per column.
pub const SAMPLE_VALUES: usize = 5;
/// Longest value shown, in characters; longer ones end in an ellipsis.
pub const MAX_VALUE_CHARS: usize = 120;
/// Columns profiled and sampled; the rest are named only.
pub const MAX_PROFILED_COLUMNS: usize = 60;

/// The action kinds the run may call, in the order the prompt asks for them.
pub const TOOLS: &[&str] = &[
    "set_table_settings",
    "set_column_description",
    "set_column_label",
    "set_column_polarity",
    "set_kpi",
    "set_column_role",
];

/// The run's system prompt, before the runner appends the table's semantics.
pub const SYSTEM_PROMPT: &str = "\
You are a data documentation assistant. You are given one table: its current \
semantics (under ABOUT THE DATA below), a profile of each column with a few of \
its values, and some sample rows. Fill in what nobody has said yet, through the \
tools, so the table reads well to people and to other models. Each column's \
`missing` list names the fields no one has filled.

Propose, in this order:
1. set_table_settings with a `description` (one or two sentences: what one row \
is and where the data comes from) when the table's `missing` lists description; \
add a `display_name` only when the table name is cryptic.
2. set_column_description for every column whose `missing` lists description: \
one or two sentences on what the value means, with its unit or range when the \
samples show one.
3. set_column_label only for a column whose name is an abbreviation or code a \
reader would not understand.
4. set_column_polarity for measures whose good direction is clear (revenue: \
higher_is_better; errors, latency, churn: lower_is_better); leave the rest alone.
5. set_kpi for at most three measures that are this table's headline quantities.
6. set_column_role only when the current role is plainly wrong: an identifier \
counted as a measure (role entity), a code with a handful of values counted as \
a measure (role dimension), a date stored as text left as a dimension (role time).

Rules: a value that is already set stays unless it is wrong. Do not state what \
the samples do not support; when unsure, say less. Columns listed as not \
analysed need no description. Batch several tool calls per turn. When you are \
finished, reply with a short plain-text note (no tool call) that names each \
existing value you overruled and why, or says \"Nothing overruled.\"";

/// One column as the model sees it: the resolved semantics beside the
/// data's own profile, and the semantic fields still empty.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnProfile {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub datatype: Option<LogicalType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<ColumnRole>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub polarity: Option<Polarity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_kpi: Option<bool>,
    pub distinct: usize,
    pub null_share: f64,
    pub samples: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<String>,
    /// Semantic fields no layer has an opinion on.
    pub missing: Vec<&'static str>,
}

/// The semantic fields nobody filled for a column. An `ignored` column
/// needs nothing beyond its role; polarity and KPI only matter on measures.
pub fn missing_fields(resolved: Option<&ResolvedColumn>) -> Vec<&'static str> {
    let mut missing = Vec::new();
    let role = resolved.and_then(|c| c.role);
    if role.is_none() {
        missing.push("role");
    }
    if role == Some(ColumnRole::Ignored) {
        return missing;
    }
    if resolved
        .and_then(|c| c.description.as_deref())
        .is_none_or(str::is_empty)
    {
        missing.push("description");
    }
    if role == Some(ColumnRole::Measure) {
        if resolved.and_then(|c| c.polarity).is_none() {
            missing.push("polarity");
        }
        if resolved.and_then(|c| c.is_kpi).is_none() {
            missing.push("kpi");
        }
    }
    missing
}

/// The table-level fields nobody filled: only the description is asked for.
pub fn missing_table_fields(table: Option<&ResolvedTable>) -> Vec<&'static str> {
    if table
        .and_then(|t| t.description.as_deref())
        .is_none_or(str::is_empty)
    {
        vec!["description"]
    } else {
        Vec::new()
    }
}

/// A value as prompt text: `None` for null, otherwise cut to
/// `MAX_VALUE_CHARS` with an ellipsis. Strings are bare, not quoted.
fn value_text(value: &AnyValue<'_>) -> Option<String> {
    let text = match value {
        AnyValue::Null => return None,
        AnyValue::String(s) => (*s).to_string(),
        AnyValue::StringOwned(s) => s.to_string(),
        other => other.to_string(),
    };
    let mut chars = text.chars();
    let head: String = chars.by_ref().take(MAX_VALUE_CHARS).collect();
    Some(if chars.next().is_some() {
        format!("{head}…")
    } else {
        head
    })
}

/// The first `SAMPLE_VALUES` distinct non-null values, in first-seen order.
/// Falls back to the head of the column when its dtype cannot be
/// deduplicated (nested types).
fn sample_values(series: &Series) -> Vec<String> {
    let distinct = series
        .unique_stable()
        .unwrap_or_else(|_| series.head(Some(SAMPLE_VALUES)));
    distinct
        .iter()
        .filter_map(|v| value_text(&v))
        .take(SAMPLE_VALUES)
        .collect()
}

/// Min and max for numeric and temporal columns; nothing for the rest,
/// where a range means little.
fn range_of(series: &Series) -> (Option<String>, Option<String>) {
    let dtype = series.dtype();
    if !(dtype.is_primitive_numeric() || dtype.is_temporal()) {
        return (None, None);
    }
    let min = series.min_reduce().ok().and_then(|s| value_text(s.value()));
    let max = series.max_reduce().ok().and_then(|s| value_text(s.value()));
    (min, max)
}

/// Profile the first `MAX_PROFILED_COLUMNS` columns of the frame, each
/// beside its resolved semantics.
pub fn profile_columns(df: &DataFrame, resolved: &[ResolvedColumn]) -> Vec<ColumnProfile> {
    let height = df.height();
    df.get_columns()
        .iter()
        .take(MAX_PROFILED_COLUMNS)
        .map(|col| {
            let name = col.name().to_string();
            let semantics = resolved.iter().find(|c| c.name == name);
            let series = col.as_materialized_series();
            let (min, max) = range_of(series);
            #[allow(clippy::cast_precision_loss)]
            let null_share = if height == 0 {
                0.0
            } else {
                col.null_count() as f64 / height as f64
            };
            ColumnProfile {
                name,
                datatype: semantics.and_then(|c| c.datatype),
                role: semantics.and_then(|c| c.role),
                label: semantics.and_then(|c| c.label.clone()),
                description: semantics.and_then(|c| c.description.clone()),
                polarity: semantics.and_then(|c| c.polarity),
                is_kpi: semantics.and_then(|c| c.is_kpi),
                distinct: col.n_unique().unwrap_or(0),
                null_share,
                samples: sample_values(series),
                min,
                max,
                missing: missing_fields(semantics),
            }
        })
        .collect()
}

/// `n` rows spread evenly over the frame (the first row always among
/// them), over the profiled columns only; nulls are JSON null.
pub fn sample_rows(df: &DataFrame, n: usize) -> Vec<serde_json::Map<String, Value>> {
    let height = df.height();
    if height == 0 || n == 0 {
        return Vec::new();
    }
    let n = n.min(height);
    let columns: Vec<&Column> = df.get_columns().iter().take(MAX_PROFILED_COLUMNS).collect();
    (0..n)
        .map(|i| {
            let row = i * height / n;
            columns
                .iter()
                .map(|col| {
                    let value = col
                        .get(row)
                        .ok()
                        .and_then(|v| value_text(&v))
                        .map_or(Value::Null, Value::String);
                    (col.name().to_string(), value)
                })
                .collect()
        })
        .collect()
}

/// The run's (system, user) messages for one table.
///
/// The user message is JSON: the table's own fields and what is missing,
/// the column profiles, the names of any columns past the profiling cap,
/// and the sample rows.
pub async fn context(
    state: &AppState,
    source_id: &str,
    table: &str,
) -> AppResult<(String, String)> {
    let store = state.require_store()?;
    let df = store.read_table(source_id, table).await?;
    let resolved = store.resolved_columns(source_id, table).await?;
    let settings = store.resolved_table(source_id, table).await?;
    let columns_not_shown: Vec<&str> = df
        .get_column_names()
        .into_iter()
        .skip(MAX_PROFILED_COLUMNS)
        .map(PlSmallStr::as_str)
        .collect();
    let user = json!({
        "table": {
            "name": table,
            "rows": df.height(),
            "displayName": settings.as_ref().and_then(|t| t.display_name.clone()),
            "description": settings.as_ref().and_then(|t| t.description.clone()),
            "missing": missing_table_fields(settings.as_ref()),
        },
        "columns": profile_columns(&df, &resolved),
        "columnsNotShown": columns_not_shown,
        "sampleRows": sample_rows(&df, SAMPLE_ROWS),
    });
    Ok((
        SYSTEM_PROMPT.to_string(),
        serde_json::to_string_pretty(&user).unwrap_or_default(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved(name: &str) -> ResolvedColumn {
        ResolvedColumn {
            name: name.to_string(),
            datatype: None,
            is_time: None,
            role: None,
            is_kpi: None,
            polarity: None,
            label: None,
            description: None,
            ai_context: None,
            resolved_by: None,
        }
    }

    #[test]
    fn missing_fields_follow_the_role() {
        // Nothing known: role and description.
        assert_eq!(missing_fields(None), vec!["role", "description"]);
        // An ignored column needs nothing more.
        let mut ignored = resolved("x");
        ignored.role = Some(ColumnRole::Ignored);
        assert!(missing_fields(Some(&ignored)).is_empty());
        // A measure also wants polarity and a KPI decision.
        let mut measure = resolved("revenue");
        measure.role = Some(ColumnRole::Measure);
        assert_eq!(
            missing_fields(Some(&measure)),
            vec!["description", "polarity", "kpi"]
        );
        // A blank description counts as missing; a filled one does not.
        measure.description = Some(String::new());
        measure.polarity = Some(Polarity::HigherIsBetter);
        measure.is_kpi = Some(false);
        assert_eq!(missing_fields(Some(&measure)), vec!["description"]);
        measure.description = Some("Net revenue in EUR".to_string());
        assert!(missing_fields(Some(&measure)).is_empty());
        // A dimension with a description is complete.
        let mut dimension = resolved("region");
        dimension.role = Some(ColumnRole::Dimension);
        dimension.description = Some("Sales region".to_string());
        assert!(missing_fields(Some(&dimension)).is_empty());
    }

    #[test]
    fn missing_table_fields_wants_only_a_description() {
        assert_eq!(missing_table_fields(None), vec!["description"]);
        let mut table = ResolvedTable {
            display_name: Some("Orders".to_string()),
            ..ResolvedTable::default()
        };
        assert_eq!(missing_table_fields(Some(&table)), vec!["description"]);
        table.description = Some("One row per order".to_string());
        assert!(missing_table_fields(Some(&table)).is_empty());
    }

    #[test]
    fn profiles_count_distinct_nulls_range_and_cut_long_values() {
        let long = "x".repeat(MAX_VALUE_CHARS + 10);
        let df = df!(
            "id" => &[1_i64, 2, 3, 4],
            "name" => &[Some("a"), None, Some(long.as_str()), Some("a")],
            "score" => &[0.5_f64, 1.5, 2.5, 3.5],
        )
        .unwrap();
        let mut id = resolved("id");
        id.role = Some(ColumnRole::Entity);
        id.description = Some("Row id".to_string());
        let mut score = resolved("score");
        score.role = Some(ColumnRole::Measure);
        let profiles = profile_columns(&df, &[id, score]);
        assert_eq!(profiles.len(), 3);

        assert_eq!(profiles[0].name, "id");
        assert_eq!(profiles[0].distinct, 4);
        assert_eq!(profiles[0].min.as_deref(), Some("1"));
        assert_eq!(profiles[0].max.as_deref(), Some("4"));
        assert!(profiles[0].missing.is_empty(), "{:?}", profiles[0].missing);

        assert_eq!(profiles[1].name, "name");
        assert!((profiles[1].null_share - 0.25).abs() < f64::EPSILON);
        // Distinct non-null values in first-seen order; the long one is cut.
        assert_eq!(profiles[1].samples.len(), 2);
        assert_eq!(profiles[1].samples[0], "a");
        assert!(profiles[1].samples[1].ends_with('…'));
        assert_eq!(profiles[1].samples[1].chars().count(), MAX_VALUE_CHARS + 1);
        // Strings have no range.
        assert_eq!(profiles[1].min, None);
        assert_eq!(profiles[1].missing, vec!["role", "description"]);

        assert_eq!(profiles[2].role, Some(ColumnRole::Measure));
        assert_eq!(profiles[2].min.as_deref(), Some("0.5"));
        assert_eq!(profiles[2].missing, vec!["description", "polarity", "kpi"]);
    }

    #[test]
    fn sample_rows_spread_over_the_table_with_nulls_as_null() {
        let values: Vec<Option<i64>> = (0..10).map(|i| (i != 5).then_some(i)).collect();
        let df = df!("n" => values).unwrap();
        let rows = sample_rows(&df, 4);
        let picked: Vec<Value> = rows.iter().map(|r| r["n"].clone()).collect();
        assert_eq!(
            picked,
            vec![json!("0"), json!("2"), Value::Null, json!("7")]
        );
        // Never more rows than the table has; an empty frame samples nothing.
        assert_eq!(sample_rows(&df, 50).len(), 10);
        assert!(sample_rows(&df.head(Some(0)), 4).is_empty());
    }

    /// The prompt must ask for every tool the runner hands out, by name.
    #[test]
    fn system_prompt_names_every_tool() {
        for tool in TOOLS {
            assert!(
                SYSTEM_PROMPT.contains(tool),
                "prompt does not mention {tool}"
            );
        }
        assert!(SYSTEM_PROMPT.contains("Nothing overruled."));
    }
}
