//! Spec validation for enrichment functions — the rules a function must pass
//! before it is stored or promoted.
//!
//! Everything here is pure (`&str`/JSON in, `AppResult` out) and separated
//! from the HTTP handlers so the ruleset is unit-testable on its own. The
//! rules exist because a function's outputs become real Parquet columns on
//! the table: names must be legal column identifiers, must not collide with
//! existing columns, and every `{{col:…}}` reference must resolve against the
//! table's stored schema — a spec that validates here can fail at run time
//! only for runtime reasons, not for shape reasons.

use brightflow_engine::enrichment::mentions::FLAG_COLUMNS;
use brightflow_engine::enrichment::ticket_classify::OUTPUT_COLUMNS;
use brightflow_engine::enrichment::{FunctionSpec, TicketClassifySpec};

use crate::shared::{AppError, AppResult};

/// A legal derived-column / function name: 1–64 bytes of ASCII alphanumerics
/// or underscores, not starting with a digit.
pub(crate) fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .enumerate()
            .all(|(i, c)| c == '_' || c.is_ascii_alphanumeric() && (i > 0 || !c.is_ascii_digit()))
        && !name.starts_with(|c: char| c.is_ascii_digit())
}

/// Column names from a table's stored `schema_json`, empty on any malformed
/// or absent schema — validation then rejects every column reference, which
/// is the safe direction.
pub(crate) fn table_columns(schema_json: Option<&str>) -> Vec<String> {
    schema_json
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .and_then(|v| {
            v.get("fields").and_then(|f| f.as_array()).map(|fields| {
                fields
                    .iter()
                    .filter_map(|f| f.get("name").and_then(|n| n.as_str()).map(String::from))
                    .collect()
            })
        })
        .unwrap_or_default()
}

/// Assemble a `FunctionSpec` from a kind + kind-less config payload.
pub(crate) fn parse_spec(kind: &str, config: &serde_json::Value) -> AppResult<FunctionSpec> {
    let mut merged = config.clone();
    let obj = merged
        .as_object_mut()
        .ok_or_else(|| AppError::BadRequest("config must be a JSON object".to_string()))?;
    obj.insert("kind".to_string(), serde_json::json!(kind));
    serde_json::from_value(merged)
        .map_err(|e| AppError::BadRequest(format!("invalid {kind} config: {e}")))
}

/// Validate a spec against the table it will run on.
pub(crate) fn validate_spec(spec: &FunctionSpec, columns: &[String]) -> AppResult<()> {
    match spec {
        FunctionSpec::TicketClassify(tc) => validate_classify_spec(tc, columns),
        FunctionSpec::TicketExtract(te) => validate_ticket_inputs(
            &te.text_columns,
            te.language_column.as_deref(),
            &te.provider_id,
            &FLAG_COLUMNS,
            columns,
        ),
    }
}

/// The built-in classifier: its inputs must exist and its fixed outputs must
/// not collide with the table. The vocabulary snapshot is server-injected
/// and not validated here.
pub(crate) fn validate_classify_spec(
    spec: &TicketClassifySpec,
    columns: &[String],
) -> AppResult<()> {
    validate_ticket_inputs(
        &spec.text_columns,
        spec.language_column.as_deref(),
        &spec.provider_id,
        &OUTPUT_COLUMNS,
        columns,
    )
}

/// Shared rules for the two built-in ticket kinds.
fn validate_ticket_inputs(
    text_columns: &[String],
    language_column: Option<&str>,
    provider_id: &str,
    outputs: &[&str],
    columns: &[String],
) -> AppResult<()> {
    if text_columns.is_empty() {
        return Err(AppError::BadRequest(
            "at least one text column is required".to_string(),
        ));
    }
    for col in text_columns {
        if !columns.iter().any(|c| c == col) {
            return Err(AppError::BadRequest(format!(
                "text column '{col}' not found in table"
            )));
        }
    }
    if let Some(lang) = language_column {
        if !columns.iter().any(|c| c == lang) {
            return Err(AppError::BadRequest(format!(
                "language column '{lang}' not found in table"
            )));
        }
    }
    if provider_id.trim().is_empty() {
        return Err(AppError::BadRequest("provider_id is required".to_string()));
    }
    for out in outputs {
        if columns.iter().any(|c| c == out) {
            return Err(AppError::BadRequest(format!(
                "output column '{out}' collides with an existing table column"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use brightflow_engine::enrichment::TicketExtractSpec;

    fn cols(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| (*n).to_string()).collect()
    }

    fn classify(text_columns: &[&str], language: Option<&str>) -> TicketClassifySpec {
        TicketClassifySpec {
            text_columns: text_columns.iter().map(|c| (*c).to_string()).collect(),
            language_column: language.map(str::to_string),
            provider_id: "p".to_string(),
            model: None,
            categories: vec![],
            subcategories: vec![],
        }
    }

    #[test]
    fn valid_name_accepts_identifiers_and_rejects_the_rest() {
        assert!(valid_name("classify"));
        assert!(valid_name("a_b1"));
        assert!(!valid_name(""));
        assert!(!valid_name("1abc"));
        assert!(!valid_name("a-b"));
        assert!(!valid_name(&"a".repeat(65)));
    }

    #[test]
    fn table_columns_reads_field_names_and_defaults_to_empty() {
        let schema = r#"{"fields":[{"name":"id"},{"name":"title"}]}"#;
        assert_eq!(table_columns(Some(schema)), cols(&["id", "title"]));
        assert!(table_columns(None).is_empty());
        assert!(table_columns(Some("not json")).is_empty());
    }

    #[test]
    fn parse_spec_merges_kind_and_rejects_non_objects() {
        let config = serde_json::json!({
            "text_columns": ["title"],
            "provider_id": "default",
        });
        let spec = parse_spec("ticket_classify", &config).expect("valid ticket_classify config");
        assert!(matches!(spec, FunctionSpec::TicketClassify(_)));
        let non_object = parse_spec("ticket_classify", &serde_json::json!([1, 2])).unwrap_err();
        assert!(matches!(non_object, AppError::BadRequest(_)));
        assert!(parse_spec("topic_model", &config).is_err());
    }

    #[test]
    fn classify_spec_checks_inputs_and_fixed_output_collisions() {
        let cols = cols(&["id", "title", "body"]);
        assert!(validate_classify_spec(&classify(&["title", "body"], None), &cols).is_ok());
        assert!(validate_classify_spec(&classify(&[], None), &cols).is_err());
        assert!(validate_classify_spec(&classify(&["nope"], None), &cols).is_err());
        assert!(validate_classify_spec(&classify(&["title"], Some("lang")), &cols).is_err());
        let mut clashing = cols;
        clashing.push("summary".to_string());
        let err = validate_classify_spec(&classify(&["title"], None), &clashing).unwrap_err();
        assert!(matches!(err, AppError::BadRequest(ref m) if m.contains("summary")));
    }

    #[test]
    fn extract_spec_collides_on_flag_columns() {
        let spec = TicketExtractSpec {
            text_columns: vec!["body".to_string()],
            language_column: None,
            provider_id: "p".to_string(),
            model: None,
            products: vec![],
            competitors: vec![],
            feedback_categories: vec![],
        };
        let cols = cols(&["id", "body"]);
        assert!(validate_spec(&FunctionSpec::TicketExtract(spec.clone()), &cols).is_ok());
        let mut clashing = cols;
        clashing.push("has_feedback".to_string());
        assert!(validate_spec(&FunctionSpec::TicketExtract(spec), &clashing).is_err());
    }
}
