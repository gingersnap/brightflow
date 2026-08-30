//! Enrichment functions: the stored, versioned owners of derived columns.
//!
//! Everything here is synchronous and DB-free — pure helpers over the spec
//! (`config_json` in `enrichment_function_versions` is the serde form of
//! [`FunctionSpec`]). Running a function is out of scope for this module by
//! design; it only defines specs and their cache identities.

use serde::{Deserialize, Serialize};

/// One versioned enrichment-function config, tagged on `kind`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FunctionSpec {
    TicketClassify(TicketClassifySpec),
    TicketExtract(TicketExtractSpec),
}

impl FunctionSpec {
    pub fn kind_str(&self) -> &'static str {
        match self {
            Self::TicketClassify(_) => "ticket_classify",
            Self::TicketExtract(_) => "ticket_extract",
        }
    }
}

/// Built-in mention extraction (Call B).
///
/// Every product, competitor, pricing, service or feedback mention in a
/// ticket, 0..n rows per ticket. Same snapshot rule as
/// [`TicketClassifySpec`]: ids and definitions, never names.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketExtractSpec {
    pub text_columns: Vec<String>,
    #[serde(default)]
    pub language_column: Option<String>,
    pub provider_id: String,
    #[serde(default)]
    pub model: Option<String>,
    /// Product areas (root) and components (children), kind `product`.
    #[serde(default)]
    pub products: Vec<VocabEntry>,
    /// Kind `competitor`, roots only.
    #[serde(default)]
    pub competitors: Vec<VocabEntry>,
    /// Kind `feedback_category`, roots only.
    #[serde(default)]
    pub feedback_categories: Vec<VocabEntry>,
}

impl TicketExtractSpec {
    pub fn vocabulary_matches(
        &self,
        products: &[VocabEntry],
        competitors: &[VocabEntry],
        feedback_categories: &[VocabEntry],
    ) -> bool {
        sorted(&self.products) == sorted(products)
            && sorted(&self.competitors) == sorted(competitors)
            && sorted(&self.feedback_categories) == sorted(feedback_categories)
    }
}

/// Content hash of an extraction spec — see [`ticket_classify_hash`].
pub fn ticket_extract_hash(spec: &TicketExtractSpec, prompt_fingerprint: &str) -> String {
    let canonical = serde_json::json!({
        "text_columns": spec.text_columns,
        "language_column": spec.language_column,
        "provider": spec.provider_id,
        "model": spec.model,
        "prompt_fingerprint": prompt_fingerprint,
        "products": sorted(&spec.products),
        "competitors": sorted(&spec.competitors),
        "feedback_categories": sorted(&spec.feedback_categories),
    });
    let bytes = serde_json::to_vec(&canonical).unwrap_or_default();
    blake3::hash(&bytes).to_hex().to_string()
}

/// One vocabulary entry as the classifier prompt sees it.
///
/// The *name* is not here: names are labels, looked up live at render and
/// materialise time, so a rename never changes this snapshot. The
/// description is the definition the model classifies against; changing it
/// is a recalibration event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VocabEntry {
    pub id: i64,
    /// 0 = root.
    pub parent_id: i64,
    #[serde(default)]
    pub description: Option<String>,
}

/// Built-in ticket classification (Call A): summary, language, category,
/// subcategory, sentiment. The prompt is owned by the runner; only the
/// customer-specific parts are config, snapshotted per version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketClassifySpec {
    /// Text columns rendered into the prompt, in order; first = headline.
    pub text_columns: Vec<String>,
    /// Source-provided language column; None = detect pre-call.
    #[serde(default)]
    pub language_column: Option<String>,
    pub provider_id: String,
    #[serde(default)]
    pub model: Option<String>,
    /// Root categories (kind `category`).
    #[serde(default)]
    pub categories: Vec<VocabEntry>,
    /// Subcategories; each `parent_id` names one of `categories`.
    #[serde(default)]
    pub subcategories: Vec<VocabEntry>,
}

impl TicketClassifySpec {
    /// Snapshot equality for the parts that reach the hash — used to decide
    /// whether a vocabulary edit needs a new function version.
    pub fn vocabulary_matches(
        &self,
        categories: &[VocabEntry],
        subcategories: &[VocabEntry],
    ) -> bool {
        sorted(&self.categories) == sorted(categories)
            && sorted(&self.subcategories) == sorted(subcategories)
    }
}

fn sorted(entries: &[VocabEntry]) -> Vec<VocabEntry> {
    let mut v = entries.to_vec();
    v.sort_by_key(|e| (e.parent_id, e.id));
    v
}

/// Content hash of a ticket-classification spec: inputs, provider, model,
/// the runner's prompt fingerprint, and the vocabulary as (id, parent,
/// description) — never names (see [`VocabEntry`]).
pub fn ticket_classify_hash(spec: &TicketClassifySpec, prompt_fingerprint: &str) -> String {
    let canonical = serde_json::json!({
        "text_columns": spec.text_columns,
        "language_column": spec.language_column,
        "provider": spec.provider_id,
        "model": spec.model,
        "prompt_fingerprint": prompt_fingerprint,
        "categories": sorted(&spec.categories),
        "subcategories": sorted(&spec.subcategories),
    });
    let bytes = serde_json::to_vec(&canonical).unwrap_or_default();
    blake3::hash(&bytes).to_hex().to_string()
}

/// Content hash of one row's rendered input values. Length-prefixed so
/// `["a","bc"]` and `["ab","c"]` cannot collide.
pub fn input_hash(rendered_inputs: &[(String, String)]) -> String {
    let mut hasher = blake3::Hasher::new();
    for (name, value) in rendered_inputs {
        hasher.update(&(name.len() as u64).to_le_bytes());
        hasher.update(name.as_bytes());
        hasher.update(&(value.len() as u64).to_le_bytes());
        hasher.update(value.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
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

    fn classify_spec() -> TicketClassifySpec {
        TicketClassifySpec {
            text_columns: vec!["title".to_string(), "body".to_string()],
            language_column: None,
            provider_id: "p1".to_string(),
            model: None,
            categories: vec![entry(1, 0, "charges"), entry(2, 0, "sign-in")],
            subcategories: vec![entry(3, 1, "vat")],
        }
    }

    /// Order of entries never matters; a description does; a fingerprint
    /// does. Names are not in the spec at all, so a rename cannot reach it.
    #[test]
    fn ticket_classify_hash_ignores_order_and_tracks_definitions() {
        let a = classify_spec();
        let mut b = classify_spec();
        b.categories.reverse();
        assert_eq!(
            ticket_classify_hash(&a, "fp"),
            ticket_classify_hash(&b, "fp")
        );
        assert!(a.vocabulary_matches(&b.categories, &b.subcategories));

        let mut c = classify_spec();
        c.categories[0].description = Some("charges and refunds".to_string());
        assert_ne!(
            ticket_classify_hash(&a, "fp"),
            ticket_classify_hash(&c, "fp")
        );
        assert!(!a.vocabulary_matches(&c.categories, &c.subcategories));

        assert_ne!(
            ticket_classify_hash(&a, "fp"),
            ticket_classify_hash(&a, "fp2")
        );
        let json = serde_json::to_string(&FunctionSpec::TicketClassify(a)).unwrap();
        assert!(json.contains("\"kind\":\"ticket_classify\""));
    }

    #[test]
    fn input_hash_is_length_prefixed() {
        let a = input_hash(&[("a".to_string(), "bc".to_string())]);
        let b = input_hash(&[("ab".to_string(), "c".to_string())]);
        assert_ne!(a, b);
        assert_eq!(a, input_hash(&[("a".to_string(), "bc".to_string())]));
    }
}
