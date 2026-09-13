//! Call A — ticket classification: the prompt, the forced-tool schema, and
//! the validation of one model answer into a cell.
//!
//! Pure: no I/O, no DB. The caller supplies the vocabulary snapshot (ids and
//! definitions, from the spec) and the current names (looked up live, so a
//! rename never touches the cache). The prompt is ordered for prefix caching:
//! everything byte-identical across rows — instructions, definitions, the
//! vocabulary block — comes first; the ticket is the trailing user turn.
//!
//! What a cell records is *ids*, never names: `category_id`/`subcategory_id`
//! with 0 meaning the reserved `other`. Materialisation resolves ids to
//! whatever the entries are called at write time.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use super::function::{TicketClassifySpec, VocabEntry};
use super::vocabulary::{is_other, OTHER, OTHER_PARENT};
use super::OutputSemantic;
use crate::data::config::ColumnRole;
use brightflow_types::LogicalType;

/// Upper bound the prompt states for the summary; validation trims, never
/// rejects, so a verbose model degrades to a truncated summary rather than a
/// failed cell.
pub const SUMMARY_MAX_WORDS: usize = 15;

/// The one sentiment field, shared with mention extraction so both grains
/// speak the same four values.
///
/// One field, not direction plus strength: "how forcefully" is not a
/// judgement a model holds consistently across a corpus, so a strength
/// column looked like signal and was not. "No evaluative content" is
/// `neutral`; `mixed` is both directions at once and is never collapsed.
pub const SENTIMENT_VALUES: [&str; 4] = ["neutral", "mixed", "positive", "negative"];

/// Materialised columns, in table order. `language` is written from the
/// pre-call detector even when the LLM call fails.
pub const OUTPUT_COLUMNS: [&str; 5] = [
    "summary",
    "language",
    "category",
    "subcategory",
    "sentiment",
];

/// The meaning of each output column, in `OUTPUT_COLUMNS` order. `summary`
/// is free text and so `ignored` for analysis; the rest are dimensions.
pub const OUTPUT_SEMANTICS: [OutputSemantic; 5] = [
    OutputSemantic {
        name: "summary",
        datatype: LogicalType::String,
        role: ColumnRole::Ignored,
        label: "Summary",
        description: "One-sentence summary of the ticket, written by the model.",
    },
    OutputSemantic {
        name: "language",
        datatype: LogicalType::String,
        role: ColumnRole::Dimension,
        label: "Language",
        description: "Language of the ticket text, detected before the model call.",
    },
    OutputSemantic {
        name: "category",
        datatype: LogicalType::String,
        role: ColumnRole::Dimension,
        label: "Category",
        description: "What is wrong for the user, from the table's category vocabulary; \
                      'other' when no entry fits.",
    },
    OutputSemantic {
        name: "subcategory",
        datatype: LogicalType::String,
        role: ColumnRole::Dimension,
        label: "Subcategory",
        description: "The finer entry under the category, from the table's subcategory \
                      vocabulary; 'other' when no entry fits.",
    },
    OutputSemantic {
        name: "sentiment",
        datatype: LogicalType::String,
        role: ColumnRole::Dimension,
        label: "Sentiment",
        description: "How the customer feels about the matter: neutral (no evaluative \
                      content), mixed (both directions at once), positive, or negative.",
    },
];

/// Output columns this call used to write and no longer does. Materialisation
/// drops these alongside the current names, so a table enriched under an
/// older column set does not keep stale columns forever.
pub const RETIRED_COLUMNS: [&str; 2] = ["sentiment_polarity", "sentiment_strength"];

/// Key under which the runner stores the detected language in a row's
/// rendered inputs. Deterministic from the text, so it is safe inside the
/// input hash.
pub const LANGUAGE_INPUT: &str = "__language";

/// Tool name for the forced structured-output call.
pub const TOOL_NAME: &str = "classify_ticket";

/// Live id → name lookup for the vocabulary the spec snapshots.
pub type VocabNames = HashMap<i64, String>;

/// One validated classification. Serialised as the cache's `value_json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassifyCell {
    pub summary: String,
    /// 0 = `other`.
    pub category_id: i64,
    /// 0 = `other`.
    pub subcategory_id: i64,
    /// One of [`SENTIMENT_VALUES`].
    pub sentiment: String,
}

/// Append formatted text; writing into a `String` cannot fail.
fn push_fmt(out: &mut String, args: std::fmt::Arguments<'_>) {
    if out.write_fmt(args).is_err() {
        unreachable!("fmt::Write for String is infallible");
    }
}

fn name_of(names: &VocabNames, id: i64) -> String {
    if id == 0 {
        return OTHER.to_string();
    }
    names.get(&id).cloned().unwrap_or_else(|| format!("#{id}"))
}

/// Resolve a stored id to its current display name (`other` for 0, `#id`
/// for an entry that has since been deleted — visible, never silent).
pub fn resolve_name(names: &VocabNames, id: i64) -> String {
    name_of(names, id)
}

fn entries_sorted(entries: &[VocabEntry]) -> Vec<&VocabEntry> {
    let mut v: Vec<&VocabEntry> = entries.iter().collect();
    v.sort_by_key(|e| e.id);
    v
}

/// The byte-identical prefix: role, field definitions, and the vocabulary
/// block. Deterministic ordering (by id) so two rows share the prefix exactly.
pub fn system_prompt(spec: &TicketClassifySpec, names: &VocabNames) -> String {
    let mut out = String::with_capacity(4096);
    out.push_str(
        "You classify one customer ticket. Record the result by calling the `classify_ticket` \
         tool — always call the tool, never reply with prose.\n\n\
         Fill the fields in this order, because later fields depend on the summary:\n\
         1. summary — ONE short clause in English (at most 15 words) saying what the ticket \
         is about, whatever language the ticket is written in. Never quote the ticket; \
         paraphrase.\n\
         2. category — exactly one entry from CATEGORIES, chosen by WHAT IS WRONG for the \
         user, never by how the ticket is written. Use `other` only when no entry fits.\n\
         3. subcategory — exactly one entry from the chosen category's SUBCATEGORIES, or \
         `other` when none fits or the category is `other`.\n\
         4. sentiment — neutral | mixed | positive | negative.\n\
         \x20  neutral: flat or balanced, and also the answer when there is no evaluative \
         content at all (machine-generated text, log dumps, one-line issues).\n\
         \x20  mixed: both directions at once (\"support was great but the product is still \
         broken\"). Never collapse mixed to neutral.\n\n",
    );
    out.push_str("CATEGORIES (name — definition):\n");
    for c in entries_sorted(&spec.categories) {
        push_fmt(
            &mut out,
            format_args!(
                "- {} — {}\n",
                name_of(names, c.id),
                c.description.as_deref().unwrap_or("(no definition)")
            ),
        );
        let mut children: Vec<&VocabEntry> = spec
            .subcategories
            .iter()
            .filter(|s| s.parent_id == c.id)
            .collect();
        children.sort_by_key(|e| e.id);
        if !children.is_empty() {
            out.push_str("  SUBCATEGORIES:\n");
            for s in children {
                push_fmt(
                    &mut out,
                    format_args!(
                        "  - {} — {}\n",
                        name_of(names, s.id),
                        s.description.as_deref().unwrap_or("(no definition)")
                    ),
                );
            }
        }
    }
    push_fmt(
        &mut out,
        format_args!("- {OTHER} — none of the above (available at both levels)\n"),
    );
    // `other` can have subcategories of its own: what the vocabulary does
    // not cover yet, grouped. They hang off the root sentinel.
    let mut orphans: Vec<&VocabEntry> = spec
        .subcategories
        .iter()
        .filter(|s| s.parent_id == OTHER_PARENT)
        .collect();
    orphans.sort_by_key(|e| e.id);
    if !orphans.is_empty() {
        out.push_str("  SUBCATEGORIES:\n");
        for s in orphans {
            push_fmt(
                &mut out,
                format_args!(
                    "  - {} — {}\n",
                    name_of(names, s.id),
                    s.description.as_deref().unwrap_or("(no definition)")
                ),
            );
        }
    }
    out
}

/// The trailing, per-row turn: the ticket text and its detected language.
pub fn user_prompt(
    text_columns: &[String],
    rendered: &BTreeMap<String, String>,
    language: Option<&str>,
) -> String {
    let mut out = String::new();
    match language {
        Some(l) => push_fmt(
            &mut out,
            format_args!("Ticket language: {l}. Write the summary in English regardless.\n\n"),
        ),
        None => out.push_str("Ticket language: unknown. Write the summary in English.\n\n"),
    }
    for col in text_columns {
        let value = rendered.get(col).map_or("", String::as_str);
        if value.trim().is_empty() {
            continue;
        }
        push_fmt(&mut out, format_args!("[{col}]\n{value}\n\n"));
    }
    out
}

/// JSON Schema for the forced tool.
///
/// `summary` is first so the model fills it first. Subcategory offers every
/// subcategory name plus `other`; the parent–child rule is enforced by
/// `validate`, which re-asks on violation.
pub fn tool_schema(spec: &TicketClassifySpec, names: &VocabNames) -> serde_json::Value {
    let mut category_names: Vec<String> = entries_sorted(&spec.categories)
        .into_iter()
        .map(|c| name_of(names, c.id))
        .collect();
    category_names.push(OTHER.to_string());
    let mut subcategory_names: Vec<String> = entries_sorted(&spec.subcategories)
        .into_iter()
        .map(|s| name_of(names, s.id))
        .collect();
    subcategory_names.sort();
    subcategory_names.dedup();
    subcategory_names.push(OTHER.to_string());
    serde_json::json!({
        "type": "object",
        "properties": {
            "summary": {
                "type": "string",
                "description": format!("One short English clause, at most {SUMMARY_MAX_WORDS} words. Never a quote.")
            },
            "category": { "type": "string", "enum": category_names },
            "subcategory": { "type": "string", "enum": subcategory_names },
            "sentiment": { "type": "string", "enum": SENTIMENT_VALUES }
        },
        "required": ["summary", "category", "subcategory", "sentiment"],
        "additionalProperties": false
    })
}

fn pick<'a>(
    values: &'a [&'a str],
    raw: &serde_json::Value,
    field: &str,
) -> Result<&'a str, String> {
    let s = raw
        .as_str()
        .ok_or_else(|| format!("field '{field}' must be a string"))?
        .trim();
    values
        .iter()
        .find(|v| v.eq_ignore_ascii_case(s))
        .copied()
        .ok_or_else(|| {
            format!(
                "field '{field}': '{s}' is not one of: {}",
                values.join(", ")
            )
        })
}

fn find_entry<'a>(
    entries: impl Iterator<Item = &'a VocabEntry>,
    names: &VocabNames,
    wanted: &str,
) -> Option<&'a VocabEntry> {
    entries.into_iter().find(|e| {
        names
            .get(&e.id)
            .is_some_and(|n| n.eq_ignore_ascii_case(wanted))
    })
}

/// Trim a summary to the word cap and single-line it.
fn normalize_summary(raw: &str) -> String {
    let words: Vec<&str> = raw.split_whitespace().collect();
    let kept = &words[..words.len().min(SUMMARY_MAX_WORDS)];
    kept.join(" ").trim_matches('"').to_string()
}

/// Validate one tool-call arguments object into a cell. Errors are worded
/// for the one re-ask.
pub fn validate(
    spec: &TicketClassifySpec,
    names: &VocabNames,
    args: &serde_json::Value,
) -> Result<ClassifyCell, String> {
    let obj = args
        .as_object()
        .ok_or_else(|| "arguments must be a JSON object".to_string())?;
    let get = |k: &str| obj.get(k).ok_or_else(|| format!("missing field '{k}'"));

    let summary = normalize_summary(
        get("summary")?
            .as_str()
            .ok_or_else(|| "field 'summary' must be a string".to_string())?,
    );
    if summary.is_empty() {
        return Err("field 'summary' is empty".to_string());
    }

    let category_raw = get("category")?
        .as_str()
        .ok_or_else(|| "field 'category' must be a string".to_string())?
        .trim();
    let category_id = if is_other(category_raw) {
        0
    } else {
        find_entry(spec.categories.iter(), names, category_raw)
            .map(|e| e.id)
            .ok_or_else(|| format!("field 'category': '{category_raw}' is not a listed category"))?
    };

    let subcategory_raw = get("subcategory")?
        .as_str()
        .ok_or_else(|| "field 'subcategory' must be a string".to_string())?
        .trim();
    // `category_id == 0` is `other`, whose children sit at OTHER_PARENT (also
    // 0), so one filter serves both a listed category and `other`.
    let subcategory_id = if is_other(subcategory_raw) {
        0
    } else {
        let children = spec
            .subcategories
            .iter()
            .filter(|s| s.parent_id == category_id);
        find_entry(children, names, subcategory_raw)
            .map(|e| e.id)
            .ok_or_else(|| {
                format!(
                    "field 'subcategory': '{subcategory_raw}' is not a subcategory of '{}'",
                    name_of(names, category_id)
                )
            })?
    };

    let sentiment = pick(&SENTIMENT_VALUES, get("sentiment")?, "sentiment")?;
    Ok(ClassifyCell {
        summary,
        category_id,
        subcategory_id,
        sentiment: sentiment.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: i64, parent_id: i64, description: &str) -> VocabEntry {
        VocabEntry {
            id,
            parent_id,
            description: Some(description.to_string()),
        }
    }

    fn fixture() -> (TicketClassifySpec, VocabNames) {
        let spec = TicketClassifySpec {
            text_columns: vec!["title".to_string(), "body".to_string()],
            language_column: None,
            provider_id: "p".to_string(),
            model: None,
            categories: vec![entry(1, 0, "charges and invoices"), entry(2, 0, "sign-in")],
            subcategories: vec![
                entry(3, 1, "missing VAT line"),
                entry(4, 2, "password reset"),
                // A subcategory of `other`: parent is the root sentinel.
                entry(5, OTHER_PARENT, "feature wishes with no home yet"),
            ],
        };
        let names: VocabNames = [
            (1, "Billing".to_string()),
            (2, "Authentication".to_string()),
            (3, "VAT".to_string()),
            (4, "Password".to_string()),
            (5, "Wishes".to_string()),
        ]
        .into_iter()
        .collect();
        (spec, names)
    }

    fn args(cat: &str, sub: &str, sentiment: &str) -> serde_json::Value {
        serde_json::json!({
            "summary": "Invoice omits the VAT line for EU customers",
            "category": cat,
            "subcategory": sub,
            "sentiment": sentiment,
        })
    }

    #[test]
    fn system_prompt_is_byte_identical_across_rows_and_lists_other() {
        let (spec, names) = fixture();
        let a = system_prompt(&spec, &names);
        let b = system_prompt(&spec, &names);
        assert_eq!(a, b);
        assert!(a.contains("- Billing — charges and invoices"));
        assert!(a.contains("  - VAT — missing VAT line"));
        assert!(a.contains("- other — none of the above"));
        // `other`'s own subcategories are listed right under it.
        assert!(a.contains("- other — none of the above (available at both levels)\n  SUBCATEGORIES:\n  - Wishes — feature wishes with no home yet"), "{a}");
        // One sentiment step and nothing after it: strength is gone.
        assert!(a.contains("4. sentiment — neutral | mixed | positive | negative"));
        assert!(!a.contains("5. "), "{a}");
        assert!(!a.contains("strength"), "{a}");
        // Names are display only: a rename shows up without touching the spec.
        let mut renamed = names;
        renamed.insert(1, "Payments".to_string());
        assert!(system_prompt(&spec, &renamed).contains("- Payments — charges and invoices"));
    }

    #[test]
    fn schema_puts_summary_first_and_offers_other_at_both_levels() {
        let (spec, names) = fixture();
        let schema = tool_schema(&spec, &names);
        // Object key order is not preserved by serde_json; the ordered
        // `required` list and the prompt text carry the fill order.
        assert_eq!(schema["required"][0], "summary");
        let cats = schema["properties"]["category"]["enum"].as_array().unwrap();
        assert_eq!(cats.last().unwrap(), "other");
        assert!(cats.iter().any(|v| v == "Billing"));
        let subs = schema["properties"]["subcategory"]["enum"]
            .as_array()
            .unwrap();
        assert!(subs.iter().any(|v| v == "Password"));
        assert_eq!(subs.last().unwrap(), "other");
    }

    #[test]
    fn validates_names_case_insensitively_into_ids() {
        let (spec, names) = fixture();
        let cell = validate(&spec, &names, &args("billing", "vat", "Negative")).unwrap();
        assert_eq!(cell.category_id, 1);
        assert_eq!(cell.subcategory_id, 3);
        assert_eq!(cell.sentiment, "negative");
        assert_eq!(resolve_name(&names, cell.category_id), "Billing");
        assert_eq!(resolve_name(&names, 0), "other");
        assert_eq!(resolve_name(&names, 99), "#99");
    }

    #[test]
    fn subcategory_must_belong_to_the_chosen_category() {
        let (spec, names) = fixture();
        let err = validate(&spec, &names, &args("Billing", "Password", "negative")).unwrap_err();
        assert!(err.contains("not a subcategory of 'Billing'"), "{err}");
        // other at both levels is fine; `other` also has children of its own.
        let cell = validate(&spec, &names, &args("other", "Other", "neutral")).unwrap();
        assert_eq!((cell.category_id, cell.subcategory_id), (0, 0));
        let billing_other = validate(&spec, &names, &args("Billing", "other", "neutral")).unwrap();
        assert_eq!(
            (billing_other.category_id, billing_other.subcategory_id),
            (1, 0)
        );
        let other_wish = validate(&spec, &names, &args("other", "wishes", "neutral")).unwrap();
        assert_eq!((other_wish.category_id, other_wish.subcategory_id), (0, 5));
        // A child of `other` is not a child of a listed category, and vice versa.
        assert!(validate(&spec, &names, &args("other", "VAT", "neutral")).is_err());
        assert!(validate(&spec, &names, &args("Billing", "Wishes", "neutral")).is_err());
    }

    #[test]
    fn every_sentiment_value_is_legal_and_old_fields_are_rejected() {
        let (spec, names) = fixture();
        for sentiment in SENTIMENT_VALUES {
            let cell = validate(&spec, &names, &args("Billing", "VAT", sentiment))
                .unwrap_or_else(|e| panic!("{sentiment} must be legal: {e}"));
            assert_eq!(cell.sentiment, sentiment);
        }
        // Values from the retired two-field shape are not sentiments.
        for old in ["none", "low", "strong", "angry"] {
            assert!(
                validate(&spec, &names, &args("Billing", "VAT", old)).is_err(),
                "{old}"
            );
        }
        // The old field names are not accepted in place of `sentiment`.
        let mut two_field = args("Billing", "VAT", "negative");
        two_field.as_object_mut().unwrap().remove("sentiment");
        two_field["sentiment_polarity"] = serde_json::json!("negative");
        two_field["sentiment_strength"] = serde_json::json!("strong");
        let err = validate(&spec, &names, &two_field).unwrap_err();
        assert!(err.contains("missing field 'sentiment'"), "{err}");
        assert!(validate(&spec, &names, &args("Shipping", "VAT", "negative")).is_err());
    }

    #[test]
    fn retired_columns_never_overlap_current_ones() {
        for retired in RETIRED_COLUMNS {
            assert!(!OUTPUT_COLUMNS.contains(&retired), "{retired}");
        }
        assert_eq!(OUTPUT_COLUMNS[4], "sentiment");
    }

    #[test]
    fn summary_is_trimmed_to_the_word_cap_and_never_empty() {
        let (spec, names) = fixture();
        let mut a = args("Billing", "VAT", "negative");
        a["summary"] = serde_json::json!(format!("\"{}\"", "word ".repeat(40)));
        let cell = validate(&spec, &names, &a).unwrap();
        assert_eq!(cell.summary.split_whitespace().count(), SUMMARY_MAX_WORDS);
        a["summary"] = serde_json::json!("   ");
        assert!(validate(&spec, &names, &a).is_err());
    }

    #[test]
    fn user_prompt_carries_language_and_skips_empty_columns() {
        let rendered: BTreeMap<String, String> = [
            ("title".to_string(), "Faktura saknar moms".to_string()),
            ("body".to_string(), String::new()),
        ]
        .into_iter()
        .collect();
        let cols = vec!["title".to_string(), "body".to_string()];
        let p = user_prompt(&cols, &rendered, Some("sv"));
        assert!(p.starts_with("Ticket language: sv."));
        assert!(p.contains("[title]\nFaktura saknar moms"));
        assert!(!p.contains("[body]"));
        assert!(user_prompt(&cols, &rendered, None).contains("unknown"));
    }

    /// The semantics list and the column list are two views of one thing.
    #[test]
    fn output_semantics_name_exactly_the_output_columns() {
        let declared: Vec<&str> = OUTPUT_SEMANTICS.iter().map(|s| s.name).collect();
        assert_eq!(declared, OUTPUT_COLUMNS.to_vec());
        for s in &OUTPUT_SEMANTICS {
            assert!(!s.label.trim().is_empty() && !s.description.trim().is_empty());
        }
    }
}
