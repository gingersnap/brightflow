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

use brightflow_engine::embedding::EmbedderId;
use brightflow_engine::enrichment::{extract_column_refs, FunctionSpec, LlmPromptSpec, OutputType};

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
        FunctionSpec::LlmPrompt(llm) => validate_llm_spec(llm, columns),
        FunctionSpec::TopicModel(tm) => {
            let o = &tm.overrides;
            if let Some(profile) = o.cleaning_profile.as_deref() {
                if brightflow_engine::nlp::CleaningProfile::parse(profile).is_none() {
                    return Err(AppError::BadRequest(format!(
                        "Unknown cleaning profile '{profile}'"
                    )));
                }
            }
            if let Some(embedder) = o.embedder.as_deref() {
                if EmbedderId::parse(embedder).is_none() {
                    return Err(AppError::BadRequest(format!(
                        "Unknown embedder '{embedder}'"
                    )));
                }
            }
            if let Some(algo) = o.algorithm.as_deref() {
                if algo != "kmeans" && algo != "hdbscan" {
                    return Err(AppError::BadRequest(format!(
                        "Unknown algorithm '{algo}' (kmeans | hdbscan)"
                    )));
                }
            }
            if let Some(cols) = &o.text_columns {
                for col in cols {
                    if !columns.iter().any(|c| c == col) {
                        return Err(AppError::BadRequest(format!(
                            "text column '{col}' not found in table"
                        )));
                    }
                }
            }
            Ok(())
        },
        FunctionSpec::Classifier(_) => Ok(()),
    }
}

pub(crate) fn validate_llm_spec(spec: &LlmPromptSpec, columns: &[String]) -> AppResult<()> {
    if spec.prompt_template.trim().is_empty() {
        return Err(AppError::BadRequest("prompt template is empty".to_string()));
    }
    let refs = extract_column_refs(&spec.prompt_template);
    if refs.is_empty() {
        return Err(AppError::BadRequest(
            "prompt template references no columns — insert at least one {{col:…}}".to_string(),
        ));
    }
    for r in &refs {
        if !columns.iter().any(|c| c == r) {
            return Err(AppError::BadRequest(format!(
                "template references unknown column '{r}'"
            )));
        }
    }
    if spec.outputs.is_empty() {
        return Err(AppError::BadRequest(
            "at least one output field is required".to_string(),
        ));
    }
    let mut seen = std::collections::HashSet::new();
    for field in &spec.outputs {
        if !valid_name(&field.name) {
            return Err(AppError::BadRequest(format!(
                "output name '{}' is not a legal column name",
                field.name
            )));
        }
        if !seen.insert(field.name.as_str()) {
            return Err(AppError::BadRequest(format!(
                "duplicate output name '{}'",
                field.name
            )));
        }
        if columns.iter().any(|c| c == &field.name) {
            return Err(AppError::BadRequest(format!(
                "output name '{}' collides with an existing table column",
                field.name
            )));
        }
        if let OutputType::Enum { values } = &field.dtype {
            if values.is_empty() || values.iter().any(|v| v.trim().is_empty()) {
                return Err(AppError::BadRequest(format!(
                    "enum output '{}' needs at least one non-empty value",
                    field.name
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use brightflow_engine::enrichment::OutputField;

    fn cols(names: &[&str]) -> Vec<String> {
        names.iter().map(ToString::to_string).collect()
    }

    fn llm_spec(template: &str, outputs: Vec<OutputField>) -> LlmPromptSpec {
        LlmPromptSpec {
            input_columns: Vec::new(),
            prompt_template: template.to_string(),
            outputs,
            provider_id: "p1".to_string(),
            model: None,
        }
    }

    fn out(name: &str) -> OutputField {
        OutputField {
            name: name.to_string(),
            dtype: OutputType::String,
            description: String::new(),
        }
    }

    #[test]
    fn valid_name_accepts_identifiers_and_rejects_the_rest() {
        assert!(valid_name("summary"));
        assert!(valid_name("_private"));
        assert!(valid_name("col_2"));
        assert!(valid_name(&"a".repeat(64)));
        assert!(!valid_name(""));
        assert!(!valid_name(&"a".repeat(65)));
        assert!(!valid_name("2fast"));
        assert!(!valid_name("has-dash"));
        assert!(!valid_name("has space"));
        assert!(!valid_name("åccent"));
    }

    #[test]
    fn table_columns_reads_field_names_and_defaults_to_empty() {
        let schema = r#"{"fields":[{"name":"title"},{"name":"body"}]}"#;
        assert_eq!(table_columns(Some(schema)), cols(&["title", "body"]));
        assert!(table_columns(None).is_empty());
        assert!(table_columns(Some("not json")).is_empty());
        assert!(table_columns(Some(r#"{"no_fields":true}"#)).is_empty());
    }

    #[test]
    fn parse_spec_merges_kind_and_rejects_non_objects() {
        let config = serde_json::json!({
            "input_columns": [],
            "prompt_template": "{{col:title}}",
            "outputs": [],
            "provider_id": "p1",
        });
        let spec = parse_spec("llm_prompt", &config).expect("valid llm_prompt config");
        assert!(matches!(spec, FunctionSpec::LlmPrompt(_)));

        let non_object = parse_spec("llm_prompt", &serde_json::json!([1, 2])).unwrap_err();
        assert!(matches!(non_object, AppError::BadRequest(_)));

        let unknown_kind = parse_spec("no_such_kind", &serde_json::json!({})).unwrap_err();
        assert!(matches!(unknown_kind, AppError::BadRequest(_)));
    }

    #[test]
    fn llm_spec_happy_path_passes() {
        let spec = llm_spec("Summarize {{col:body}}", vec![out("summary")]);
        assert!(validate_llm_spec(&spec, &cols(&["body"])).is_ok());
    }

    #[test]
    fn llm_spec_rejects_empty_and_reference_free_templates() {
        let empty = llm_spec("   ", vec![out("x")]);
        assert!(validate_llm_spec(&empty, &cols(&["body"])).is_err());
        let no_refs = llm_spec("no references here", vec![out("x")]);
        assert!(validate_llm_spec(&no_refs, &cols(&["body"])).is_err());
    }

    #[test]
    fn llm_spec_rejects_unknown_column_reference() {
        let spec = llm_spec("{{col:missing}}", vec![out("x")]);
        assert!(validate_llm_spec(&spec, &cols(&["body"])).is_err());
    }

    #[test]
    fn llm_spec_output_rules() {
        // No outputs at all.
        let no_outputs = llm_spec("{{col:body}}", vec![]);
        assert!(validate_llm_spec(&no_outputs, &cols(&["body"])).is_err());
        // Illegal output name.
        let bad_name = llm_spec("{{col:body}}", vec![out("2bad")]);
        assert!(validate_llm_spec(&bad_name, &cols(&["body"])).is_err());
        // Duplicate output names.
        let duplicate = llm_spec("{{col:body}}", vec![out("x"), out("x")]);
        assert!(validate_llm_spec(&duplicate, &cols(&["body"])).is_err());
        // Collision with an existing table column.
        let collision = llm_spec("{{col:body}}", vec![out("body")]);
        assert!(validate_llm_spec(&collision, &cols(&["body"])).is_err());
    }

    #[test]
    fn llm_spec_enum_outputs_need_non_empty_values() {
        let empty_values = OutputField {
            name: "sentiment".to_string(),
            dtype: OutputType::Enum { values: vec![] },
            description: String::new(),
        };
        let empty_spec = llm_spec("{{col:body}}", vec![empty_values]);
        assert!(validate_llm_spec(&empty_spec, &cols(&["body"])).is_err());

        let blank_value = OutputField {
            name: "sentiment".to_string(),
            dtype: OutputType::Enum {
                values: vec!["pos".to_string(), "  ".to_string()],
            },
            description: String::new(),
        };
        let blank_spec = llm_spec("{{col:body}}", vec![blank_value]);
        assert!(validate_llm_spec(&blank_spec, &cols(&["body"])).is_err());
    }

    #[test]
    fn topic_model_overrides_are_validated_individually() {
        let make = |overrides: serde_json::Value| -> FunctionSpec {
            parse_spec("topic_model", &overrides).expect("topic_model config parses")
        };
        let columns = cols(&["body"]);

        assert!(validate_spec(&make(serde_json::json!({})), &columns).is_ok());
        assert!(validate_spec(&make(serde_json::json!({"algorithm": "kmeans"})), &columns).is_ok());
        assert!(
            validate_spec(&make(serde_json::json!({"algorithm": "dbscan"})), &columns).is_err()
        );
        assert!(validate_spec(
            &make(serde_json::json!({"cleaning_profile": "no_such_profile"})),
            &columns
        )
        .is_err());
        assert!(validate_spec(
            &make(serde_json::json!({"embedder": "no-such-embedder"})),
            &columns
        )
        .is_err());
        assert!(validate_spec(
            &make(serde_json::json!({"text_columns": ["missing"]})),
            &columns
        )
        .is_err());
        assert!(validate_spec(
            &make(serde_json::json!({"text_columns": ["body"]})),
            &columns
        )
        .is_ok());
    }
}
