//! Call B — mention extraction.
//!
//! The prompt, the forced-tool schema, the validation of one answer into
//! mention rows, and the pure builders for the `{table}_mentions` child
//! table and the parent row's flags.
//!
//! Mentions are a *table*, not a nested column: one row per mention keyed
//! `(ticket_id, mention_idx)`, rebuilt wholesale from the cache on every
//! materialisation so a re-enriched ticket with fewer mentions can never
//! leave stale rows behind. Subjects are stored as vocabulary ids; an
//! unresolved subject keeps the model's normalised *name* for the review
//! queue — never a quote from the ticket.
//!
//! The prompt leads with an empty-output example on purpose: incidental
//! feedback is 5–15 % of tickets and a model asked to fill a list will fill
//! it. Exhaustiveness is asked for instead of suppression — the contact
//! reason is extracted too and `incidental=false` marks it.

use std::collections::HashMap;
use std::fmt::Write as _;

use polars::prelude::*;
use serde::{Deserialize, Serialize};

use super::function::{TicketExtractSpec, VocabEntry};
use super::ticket_classify::{VocabNames, SENTIMENT_VALUES};
use super::vocabulary::{is_other, OTHER};
use super::OutputSemantic;
use crate::data::config::ColumnRole;
use brightflow_types::LogicalType;

/// Child-table name for a parent `table`.
pub fn mentions_table_name(table: &str) -> String {
    format!("{table}_mentions")
}

pub const TOOL_NAME: &str = "extract_mentions";
/// Engineering limit on mentions per ticket (schema `maxItems`).
pub const MAX_MENTIONS: usize = 50;
/// Word cap for `feedback_summary`: a normalised noun phrase, near a label.
pub const FEEDBACK_SUMMARY_MAX_WORDS: usize = 6;

pub const MENTION_TYPES: [&str; 5] = ["product", "competitor", "pricing", "service", "feedback"];

/// Columns of the child table, in order. `ticket_id` takes the parent id
/// column's dtype.
pub const MENTION_COLUMNS: [&str; 11] = [
    "ticket_id",
    "mention_idx",
    "type",
    "subject_id",
    "subject",
    "subject_surface",
    "feedback_summary",
    "feedback_category",
    "incidental",
    "sentiment",
    "confidence",
];

/// Derived columns written on the parent row.
pub const FLAG_COLUMNS: [&str; 4] = [
    "has_feedback",
    "has_incidental_feedback",
    "has_competitor_mention",
    "mention_count",
];

/// The meaning of each parent-row flag, in `FLAG_COLUMNS` order.
pub const FLAG_SEMANTICS: [OutputSemantic; 4] = [
    OutputSemantic {
        name: "has_feedback",
        datatype: LogicalType::Boolean,
        role: ColumnRole::Dimension,
        label: "Has feedback",
        description: "Whether the ticket contains at least one piece of feedback about \
                      the product or service.",
    },
    OutputSemantic {
        name: "has_incidental_feedback",
        datatype: LogicalType::Boolean,
        role: ColumnRole::Dimension,
        label: "Has incidental feedback",
        description: "Whether the ticket contains feedback that is not the reason the \
                      customer wrote in.",
    },
    OutputSemantic {
        name: "has_competitor_mention",
        datatype: LogicalType::Boolean,
        role: ColumnRole::Dimension,
        label: "Mentions a competitor",
        description: "Whether the ticket names a competitor.",
    },
    OutputSemantic {
        name: "mention_count",
        datatype: LogicalType::Integer,
        role: ColumnRole::Measure,
        label: "Mention count",
        description: "How many products, competitors, prices, services or pieces of \
                      feedback the ticket mentions.",
    },
];

/// One validated mention. The cache's `value_json` is `{"mentions": [..]}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MentionCell {
    pub mention_type: String,
    /// Resolved vocabulary id; None when unresolved or not applicable.
    pub subject_id: Option<i64>,
    /// The model's normalised name for an unresolved subject.
    pub subject_surface: Option<String>,
    pub feedback_summary: Option<String>,
    /// 0 = `other`; None when the mention is not feedback.
    pub feedback_category_id: Option<i64>,
    pub incidental: bool,
    /// The customer's attitude toward THIS subject, one of
    /// `ticket_classify::SENTIMENT_VALUES` — the same four values as the
    /// row-level field, so both grains read alike.
    pub sentiment: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractCell {
    pub mentions: Vec<MentionCell>,
}

/// Name/alias → id resolution for imported vocabularies, case-insensitive.
/// Built by the caller from the live table; the spec carries only ids.
#[derive(Debug, Default, Clone)]
pub struct SubjectResolver {
    by_surface: HashMap<String, i64>,
}

impl SubjectResolver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a canonical name or alias for an entry.
    pub fn add(&mut self, surface: &str, id: i64) {
        let key = surface.trim().to_lowercase();
        if !key.is_empty() {
            self.by_surface.entry(key).or_insert(id);
        }
    }

    pub fn resolve(&self, surface: &str) -> Option<i64> {
        self.by_surface.get(&surface.trim().to_lowercase()).copied()
    }
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

fn sorted(entries: &[VocabEntry]) -> Vec<&VocabEntry> {
    let mut v: Vec<&VocabEntry> = entries.iter().collect();
    v.sort_by_key(|e| e.id);
    v
}

fn push_level(out: &mut String, heading: &str, entries: &[VocabEntry], names: &VocabNames) {
    out.push_str(heading);
    out.push('\n');
    if entries.is_empty() {
        out.push_str("- (none defined)\n");
    }
    let roots: Vec<&VocabEntry> = sorted(entries)
        .into_iter()
        .filter(|e| e.parent_id == 0)
        .collect();
    for root in roots {
        push_fmt(
            out,
            format_args!(
                "- {} — {}\n",
                name_of(names, root.id),
                root.description.as_deref().unwrap_or("(no definition)")
            ),
        );
        for child in sorted(entries)
            .into_iter()
            .filter(|e| e.parent_id == root.id)
        {
            push_fmt(
                out,
                format_args!(
                    "  - {} — {}\n",
                    name_of(names, child.id),
                    child.description.as_deref().unwrap_or("(no definition)")
                ),
            );
        }
    }
}

/// The byte-identical prefix: vocabulary block first, then the rules, then
/// the few-shot block whose first example is the empty output.
pub fn system_prompt(spec: &TicketExtractSpec, names: &VocabNames) -> String {
    let mut out = String::with_capacity(6144);
    push_level(
        &mut out,
        "PRODUCTS (area, then components):",
        &spec.products,
        names,
    );
    push_level(&mut out, "\nCOMPETITORS:", &spec.competitors, names);
    push_level(
        &mut out,
        "\nFEEDBACK CATEGORIES:",
        &spec.feedback_categories,
        names,
    );
    push_fmt(
        &mut out,
        format_args!("- {OTHER} — feedback that fits no category above\n\n"),
    );
    out.push_str(
        "You extract every distinct mention from one customer ticket and record them by \
         calling the `extract_mentions` tool — always call the tool, never reply with prose.\n\n\
         A mention is one of:\n\
         - product: a product area or component from PRODUCTS. `subject` = its name.\n\
         - competitor: a company or product from COMPETITORS, or an unlisted one — then \
         `subject` = the name as the customer would say it, normalised (never a quote).\n\
         - pricing: cost, plans, discounts, invoicing terms. `subject` may name a product.\n\
         - service: the support interaction itself (response time, agent, process).\n\
         - feedback: an opinion, complaint, wish or praise. `feedback_summary` = 3–6 English \
         words, a normalised noun phrase or imperative (\"export fails on large files\", \
         \"wants dark mode\"); `feedback_category` = one of FEEDBACK CATEGORIES or `other`.\n\n\
         Rules:\n\
         - List everything mentioned, including the reason for contact. Set `incidental` \
         true only for mentions unrelated to why the customer wrote in.\n\
         - Feedback is its own row. \"The invoice screen is confusing\" is two mentions: a \
         product row for the invoice screen and a feedback row for what was said.\n\
         - Several distinct feedback points are several rows.\n\
         - A comparison (\"X is better than Y\") is two rows with opposite sentiment.\n\
         - `sentiment`: neutral | mixed | positive | negative, as the customer \
         expressed it about THAT subject. A subject named without an opinion is \
         neutral.\n\
         - `confidence`: 0 to 1.\n\
         - Most tickets contain NO incidental feedback and few mentions. An empty list is \
         a correct and common answer. Never invent a mention to fill the list.\n\n\
         Examples (ticket → mentions):\n\
         1. \"Password reset link expired, can you send a new one?\" → []  (nothing beyond \
         the contact reason once no product/component is named)\n\
         2. \"Password reset link expired. Also the invoice screen is really confusing.\" → \
         [{type: feedback, feedback_summary: \"invoice screen confusing\", incidental: true, \
         sentiment: negative}] plus a product row for the invoice component when PRODUCTS \
         lists one.\n\
         3. \"Your support was great but the export is still broken and CompetitorCo \
         handles it fine.\" → a service row (positive, not incidental), a feedback row \
         \"export broken\" (negative), a competitor row (positive, incidental).\n",
    );
    out
}

/// JSON Schema for the forced tool: one array of uniform mention objects.
pub fn tool_schema(spec: &TicketExtractSpec, names: &VocabNames) -> serde_json::Value {
    let mut feedback_names: Vec<String> = sorted(&spec.feedback_categories)
        .into_iter()
        .map(|e| name_of(names, e.id))
        .collect();
    feedback_names.push(OTHER.to_string());
    serde_json::json!({
        "type": "object",
        "properties": {
            "mentions": {
                "type": "array",
                "maxItems": MAX_MENTIONS,
                "items": {
                    "type": "object",
                    "properties": {
                        "type": { "type": "string", "enum": MENTION_TYPES },
                        "subject": { "type": ["string", "null"],
                            "description": "Product/competitor name; null for feedback and service" },
                        "feedback_summary": { "type": ["string", "null"],
                            "description": format!("{FEEDBACK_SUMMARY_MAX_WORDS} English words at most; only for type=feedback") },
                        "feedback_category": { "type": ["string", "null"], "enum": feedback_names_with_null(&feedback_names) },
                        "incidental": { "type": "boolean" },
                        "sentiment": { "type": "string", "enum": SENTIMENT_VALUES },
                        "confidence": { "type": "number", "minimum": 0, "maximum": 1 }
                    },
                    "required": ["type", "subject", "feedback_summary", "feedback_category", "incidental", "sentiment", "confidence"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["mentions"],
        "additionalProperties": false
    })
}

fn feedback_names_with_null(names: &[String]) -> Vec<serde_json::Value> {
    let mut v: Vec<serde_json::Value> = names
        .iter()
        .map(|n| serde_json::Value::String(n.clone()))
        .collect();
    v.push(serde_json::Value::Null);
    v
}

fn normalize_phrase(raw: &str, max_words: usize) -> String {
    let words: Vec<&str> = raw.split_whitespace().collect();
    words[..words.len().min(max_words)]
        .join(" ")
        .trim_matches(|c: char| c == '"' || c == '.')
        .to_string()
}

fn opt_str(v: Option<&serde_json::Value>) -> Result<Option<String>, String> {
    match v {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(s)) => {
            let t = s.trim();
            Ok((!t.is_empty()).then(|| t.to_string()))
        },
        Some(_) => Err("expected a string or null".to_string()),
    }
}

/// Validate one tool-call arguments object into mention rows.
///
/// Errors are worded for the one re-ask. `resolver` maps product/competitor
/// names and aliases to ids; `names` maps feedback-category ids to names.
pub fn validate(
    spec: &TicketExtractSpec,
    names: &VocabNames,
    resolver: &SubjectResolver,
    args: &serde_json::Value,
) -> Result<ExtractCell, String> {
    let obj = args
        .as_object()
        .ok_or_else(|| "arguments must be a JSON object".to_string())?;
    let list = obj
        .get("mentions")
        .and_then(|m| m.as_array())
        .ok_or_else(|| "missing 'mentions' array".to_string())?;
    if list.len() > MAX_MENTIONS {
        return Err(format!("at most {MAX_MENTIONS} mentions per ticket"));
    }
    let mut mentions = Vec::with_capacity(list.len());
    for (i, item) in list.iter().enumerate() {
        let m = item
            .as_object()
            .ok_or_else(|| format!("mention {i}: must be an object"))?;
        let mention_type = m
            .get("type")
            .and_then(|t| t.as_str())
            .map(str::trim)
            .and_then(|t| MENTION_TYPES.iter().find(|k| k.eq_ignore_ascii_case(t)))
            .ok_or_else(|| {
                format!(
                    "mention {i}: 'type' must be one of {}",
                    MENTION_TYPES.join(", ")
                )
            })?;
        let subject =
            opt_str(m.get("subject")).map_err(|e| format!("mention {i}: subject: {e}"))?;
        let feedback_summary = opt_str(m.get("feedback_summary"))
            .map_err(|e| format!("mention {i}: feedback_summary: {e}"))?;
        let feedback_category = opt_str(m.get("feedback_category"))
            .map_err(|e| format!("mention {i}: feedback_category: {e}"))?;
        let incidental = m
            .get("incidental")
            .and_then(serde_json::Value::as_bool)
            .ok_or_else(|| format!("mention {i}: 'incidental' must be a boolean"))?;
        let sentiment = m
            .get("sentiment")
            .and_then(|p| p.as_str())
            .map(str::trim)
            .and_then(|p| SENTIMENT_VALUES.iter().find(|k| k.eq_ignore_ascii_case(p)))
            .ok_or_else(|| {
                format!(
                    "mention {i}: 'sentiment' must be one of {}",
                    SENTIMENT_VALUES.join(", ")
                )
            })?;
        let confidence = m
            .get("confidence")
            .and_then(serde_json::Value::as_f64)
            .map_or(0.5, |c| c.clamp(0.0, 1.0)) as f32;

        let (subject_id, subject_surface) = match (*mention_type, subject.as_deref()) {
            ("product" | "competitor", None) => {
                return Err(format!(
                    "mention {i}: a {mention_type} mention needs a 'subject'"
                ))
            },
            (_, Some(s)) => match resolver.resolve(s) {
                Some(id) => (Some(id), None),
                None => (None, Some(normalize_phrase(s, 6))),
            },
            (_, None) => (None, None),
        };

        let (feedback_summary, feedback_category_id) = if *mention_type == "feedback" {
            let summary = feedback_summary
                .as_deref()
                .map(|s| normalize_phrase(s, FEEDBACK_SUMMARY_MAX_WORDS))
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    format!("mention {i}: a feedback mention needs a 'feedback_summary'")
                })?;
            let category_id = match feedback_category.as_deref() {
                None => 0,
                Some(c) if is_other(c) => 0,
                Some(c) => spec
                    .feedback_categories
                    .iter()
                    .find(|e| names.get(&e.id).is_some_and(|n| n.eq_ignore_ascii_case(c)))
                    .map(|e| e.id)
                    .ok_or_else(|| {
                        format!("mention {i}: '{c}' is not a listed feedback category")
                    })?,
            };
            (Some(summary), Some(category_id))
        } else {
            (None, None)
        };

        mentions.push(MentionCell {
            mention_type: (*mention_type).to_string(),
            subject_id,
            subject_surface,
            feedback_summary,
            feedback_category_id,
            incidental,
            sentiment: (*sentiment).to_string(),
            confidence,
        });
    }
    Ok(ExtractCell { mentions })
}

/// Build the child-table frame: one row per mention.
///
/// `ticket_id` is gathered from the parent id column so it keeps that
/// column's dtype. `cells[i]` is parent row `i`'s validated mentions (None =
/// not yet extracted).
pub fn build_mention_rows(
    ticket_ids: &Column,
    cells: &[Option<ExtractCell>],
    names: &VocabNames,
) -> PolarsResult<DataFrame> {
    let mut parent_idx: Vec<IdxSize> = Vec::new();
    let mut mention_idx: Vec<i32> = Vec::new();
    let mut types: Vec<String> = Vec::new();
    let mut subject_ids: Vec<Option<i64>> = Vec::new();
    let mut subjects: Vec<Option<String>> = Vec::new();
    let mut surfaces: Vec<Option<String>> = Vec::new();
    let mut summaries: Vec<Option<String>> = Vec::new();
    let mut categories: Vec<Option<String>> = Vec::new();
    let mut incidental: Vec<bool> = Vec::new();
    let mut sentiment: Vec<String> = Vec::new();
    let mut confidence: Vec<f32> = Vec::new();
    for (row, cell) in cells.iter().enumerate() {
        let Some(cell) = cell else { continue };
        for (i, m) in cell.mentions.iter().enumerate() {
            parent_idx.push(IdxSize::try_from(row).unwrap_or(IdxSize::MAX));
            mention_idx.push(i32::try_from(i).unwrap_or(i32::MAX));
            types.push(m.mention_type.clone());
            subject_ids.push(m.subject_id);
            subjects.push(m.subject_id.map(|id| name_of(names, id)));
            surfaces.push(m.subject_surface.clone());
            summaries.push(m.feedback_summary.clone());
            categories.push(m.feedback_category_id.map(|id| name_of(names, id)));
            incidental.push(m.incidental);
            sentiment.push(m.sentiment.clone());
            confidence.push(m.confidence);
        }
    }
    let idx = IdxCa::from_vec("idx".into(), parent_idx);
    let ticket_id = ticket_ids.take(&idx)?.with_name(MENTION_COLUMNS[0].into());
    DataFrame::new(vec![
        ticket_id,
        Column::new(MENTION_COLUMNS[1].into(), mention_idx),
        Column::new(MENTION_COLUMNS[2].into(), types),
        Column::new(MENTION_COLUMNS[3].into(), subject_ids),
        Column::new(MENTION_COLUMNS[4].into(), subjects),
        Column::new(MENTION_COLUMNS[5].into(), surfaces),
        Column::new(MENTION_COLUMNS[6].into(), summaries),
        Column::new(MENTION_COLUMNS[7].into(), categories),
        Column::new(MENTION_COLUMNS[8].into(), incidental),
        Column::new(MENTION_COLUMNS[9].into(), sentiment),
        Column::new(MENTION_COLUMNS[10].into(), confidence),
    ])
}

/// The parent row's derived flags, in `FLAG_COLUMNS` order. Null for rows
/// not yet extracted, so "no mentions" and "not run" stay distinguishable.
pub fn derive_ticket_flags(cells: &[Option<ExtractCell>]) -> Vec<Column> {
    let has_feedback: Vec<Option<bool>> = cells
        .iter()
        .map(|c| {
            c.as_ref()
                .map(|c| c.mentions.iter().any(|m| m.mention_type == "feedback"))
        })
        .collect();
    let has_incidental: Vec<Option<bool>> = cells
        .iter()
        .map(|c| {
            c.as_ref().map(|c| {
                c.mentions
                    .iter()
                    .any(|m| m.mention_type == "feedback" && m.incidental)
            })
        })
        .collect();
    let has_competitor: Vec<Option<bool>> = cells
        .iter()
        .map(|c| {
            c.as_ref()
                .map(|c| c.mentions.iter().any(|m| m.mention_type == "competitor"))
        })
        .collect();
    let count: Vec<Option<i32>> = cells
        .iter()
        .map(|c| {
            c.as_ref()
                .map(|c| i32::try_from(c.mentions.len()).unwrap_or(i32::MAX))
        })
        .collect();
    vec![
        Column::new(FLAG_COLUMNS[0].into(), has_feedback),
        Column::new(FLAG_COLUMNS[1].into(), has_incidental),
        Column::new(FLAG_COLUMNS[2].into(), has_competitor),
        Column::new(FLAG_COLUMNS[3].into(), count),
    ]
}

/// Unresolved subject surfaces with their mention counts, for the review
/// queue: `(type, surface) → count`.
pub fn unresolved_surfaces(cells: &[Option<ExtractCell>]) -> Vec<((String, String), usize)> {
    let mut counts: HashMap<(String, String), usize> = HashMap::new();
    for cell in cells.iter().flatten() {
        for m in &cell.mentions {
            if let (None, Some(surface)) = (m.subject_id, &m.subject_surface) {
                *counts
                    .entry((m.mention_type.clone(), surface.clone()))
                    .or_insert(0) += 1;
            }
        }
    }
    let mut out: Vec<_> = counts.into_iter().collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    out
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

    fn fixture() -> (TicketExtractSpec, VocabNames, SubjectResolver) {
        let spec = TicketExtractSpec {
            text_columns: vec!["body".to_string()],
            language_column: None,
            provider_id: "p".to_string(),
            model: None,
            products: vec![
                entry(10, 0, "billing area"),
                entry(11, 10, "the invoice screen"),
            ],
            competitors: vec![entry(20, 0, "CompetitorCo")],
            feedback_categories: vec![entry(30, 0, "usability friction")],
        };
        let names: VocabNames = [
            (10, "Billing".to_string()),
            (11, "Invoice screen".to_string()),
            (20, "CompetitorCo".to_string()),
            (30, "Usability".to_string()),
        ]
        .into_iter()
        .collect();
        let mut resolver = SubjectResolver::new();
        resolver.add("Billing", 10);
        resolver.add("Invoice screen", 11);
        resolver.add("invoices page", 11);
        resolver.add("CompetitorCo", 20);
        (spec, names, resolver)
    }

    fn mention(t: &str, subject: Option<&str>, summary: Option<&str>) -> serde_json::Value {
        serde_json::json!({
            "type": t,
            "subject": subject,
            "feedback_summary": summary,
            "feedback_category": if t == "feedback" { Some("usability") } else { None },
            "incidental": true,
            "sentiment": "negative",
            "confidence": 0.9,
        })
    }

    #[test]
    fn prompt_leads_with_vocabulary_and_offers_the_empty_example_first() {
        let (spec, names, _) = fixture();
        let p = system_prompt(&spec, &names);
        assert!(p.starts_with("PRODUCTS"));
        assert!(p.contains("  - Invoice screen — the invoice screen"));
        assert!(p.contains("- CompetitorCo — CompetitorCo"));
        let empty = p.find("→ []").unwrap();
        let second = p.find("2. ").unwrap();
        assert!(empty < second, "empty output must be the first example");
        let schema = tool_schema(&spec, &names);
        assert_eq!(schema["properties"]["mentions"]["maxItems"], MAX_MENTIONS);
    }

    #[test]
    fn empty_output_is_valid_and_flags_are_false_not_null() {
        let (spec, names, resolver) = fixture();
        let cell = validate(
            &spec,
            &names,
            &resolver,
            &serde_json::json!({"mentions": []}),
        )
        .unwrap();
        assert!(cell.mentions.is_empty());
        let flags = derive_ticket_flags(&[Some(cell), None]);
        let hf = flags[0].as_materialized_series().bool().unwrap();
        assert_eq!(hf.get(0), Some(false));
        assert_eq!(hf.get(1), None);
        let n = flags[3].as_materialized_series().i32().unwrap();
        assert_eq!(n.get(0), Some(0));
    }

    #[test]
    fn product_and_feedback_rows_resolve_and_normalise() {
        let (spec, names, resolver) = fixture();
        let args = serde_json::json!({ "mentions": [
            mention("product", Some("invoices page"), None),
            mention("feedback", None, Some("\"The invoice screen is really quite confusing to use.\"")),
            mention("competitor", Some("Unknown Corp"), None),
        ]});
        let cell = validate(&spec, &names, &resolver, &args).unwrap();
        assert_eq!(cell.mentions[0].subject_id, Some(11));
        assert_eq!(cell.mentions[0].subject_surface, None);
        let fb = &cell.mentions[1];
        assert_eq!(
            fb.feedback_summary.as_deref(),
            Some("The invoice screen is really quite")
        );
        assert_eq!(fb.feedback_category_id, Some(30));
        assert_eq!(cell.mentions[2].subject_id, None);
        assert_eq!(
            cell.mentions[2].subject_surface.as_deref(),
            Some("Unknown Corp")
        );

        let unresolved = unresolved_surfaces(&[Some(cell.clone())]);
        assert_eq!(
            unresolved[0].0,
            ("competitor".to_string(), "Unknown Corp".to_string())
        );
        assert_eq!(unresolved[0].1, 1);

        let flags = derive_ticket_flags(&[Some(cell)]);
        assert_eq!(
            flags[1].as_materialized_series().bool().unwrap().get(0),
            Some(true)
        );
        assert_eq!(
            flags[2].as_materialized_series().bool().unwrap().get(0),
            Some(true)
        );
    }

    #[test]
    fn rejects_shape_errors_with_reask_wording() {
        let (spec, names, resolver) = fixture();
        let missing_subject = serde_json::json!({ "mentions": [mention("product", None, None)] });
        assert!(validate(&spec, &names, &resolver, &missing_subject)
            .unwrap_err()
            .contains("needs a 'subject'"));
        let missing_summary = serde_json::json!({ "mentions": [mention("feedback", None, None)] });
        assert!(validate(&spec, &names, &resolver, &missing_summary)
            .unwrap_err()
            .contains("feedback_summary"));
        let mut bad_cat = mention("feedback", None, Some("slow export"));
        bad_cat["feedback_category"] = serde_json::json!("Speed");
        let err = validate(
            &spec,
            &names,
            &resolver,
            &serde_json::json!({ "mentions": [bad_cat] }),
        )
        .unwrap_err();
        assert!(err.contains("not a listed feedback category"));
        let mut other = mention("feedback", None, Some("slow export"));
        other["feedback_category"] = serde_json::json!("Other");
        let cell = validate(
            &spec,
            &names,
            &resolver,
            &serde_json::json!({ "mentions": [other] }),
        )
        .unwrap();
        assert_eq!(cell.mentions[0].feedback_category_id, Some(0));
    }

    /// A comparative is two rows sharing a ticket; the child frame gathers
    /// the parent id with its dtype, and a ticket with no cell contributes
    /// nothing.
    #[test]
    fn mention_rows_gather_parent_ids_with_dtype() {
        let (_, names, _) = fixture();
        let ids = Column::new("id".into(), &[101_i64, 102, 103]);
        let pos = MentionCell {
            mention_type: "competitor".to_string(),
            subject_id: Some(20),
            subject_surface: None,
            feedback_summary: None,
            feedback_category_id: None,
            incidental: true,
            sentiment: "positive".to_string(),
            confidence: 0.8,
        };
        let neg = MentionCell {
            mention_type: "product".to_string(),
            subject_id: Some(11),
            sentiment: "negative".to_string(),
            ..pos.clone()
        };
        let cells = vec![
            Some(ExtractCell {
                mentions: vec![pos, neg],
            }),
            None,
            Some(ExtractCell { mentions: vec![] }),
        ];
        let df = build_mention_rows(&ids, &cells, &names).unwrap();
        assert_eq!(df.height(), 2);
        assert_eq!(df.column("ticket_id").unwrap().dtype(), &DataType::Int64);
        let tids = df.column("ticket_id").unwrap().i64().unwrap();
        assert_eq!(tids.get(0), Some(101));
        assert_eq!(tids.get(1), Some(101));
        let idx = df.column("mention_idx").unwrap().i32().unwrap();
        assert_eq!((idx.get(0), idx.get(1)), (Some(0), Some(1)));
        let subj = df.column("subject").unwrap().str().unwrap();
        assert_eq!(subj.get(1), Some("Invoice screen"));
        assert_eq!(df.get_column_names().len(), MENTION_COLUMNS.len());
        assert_eq!(mentions_table_name("issues"), "issues_mentions");
    }

    /// The semantics list and the flag list are two views of one thing.
    #[test]
    fn flag_semantics_name_exactly_the_flag_columns() {
        let declared: Vec<&str> = FLAG_SEMANTICS.iter().map(|s| s.name).collect();
        assert_eq!(declared, FLAG_COLUMNS.to_vec());
        for s in &FLAG_SEMANTICS {
            assert!(!s.label.trim().is_empty() && !s.description.trim().is_empty());
        }
    }
}
