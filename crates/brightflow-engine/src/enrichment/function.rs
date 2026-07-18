//! Enrichment functions: the stored, versioned owners of derived columns.
//!
//! Everything here is synchronous and DB-free — pure helpers over the spec
//! (`config_json` in `enrichment_function_versions` is the serde form of
//! [`FunctionSpec`]). The async LLM batch runner lives in brightflow-api;
//! topic_model/classifier fit + apply stay in `topic_enricher`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::config::{EnrichmentConfig, EnrichmentOverrides};
use crate::embedding::EmbedderId;
use crate::nlp::CleaningProfile;

/// One versioned enrichment-function config, tagged on `kind`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FunctionSpec {
    LlmPrompt(LlmPromptSpec),
    TopicModel(TopicModelSpec),
    Classifier(ClassifierSpec),
}

impl FunctionSpec {
    pub fn kind_str(&self) -> &'static str {
        match self {
            Self::LlmPrompt(_) => "llm_prompt",
            Self::TopicModel(_) => "topic_model",
            Self::Classifier(_) => "classifier",
        }
    }
}

/// Ad-hoc per-column LLM enrichment: a prompt template over row values,
/// producing one or more typed output columns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmPromptSpec {
    /// Columns the template references (validated at save + run time).
    pub input_columns: Vec<String>,
    /// Template with `{{col:Name}}` references.
    pub prompt_template: String,
    pub outputs: Vec<OutputField>,
    /// LLM provider registry id.
    pub provider_id: String,
    /// Model override; None = the provider's configured default.
    #[serde(default)]
    pub model: Option<String>,
}

/// One typed output column.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputField {
    pub name: String,
    pub dtype: OutputType,
    #[serde(default)]
    pub description: String,
}

/// Output column type. Enum values are free-form labels.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutputType {
    String,
    Number,
    Bool,
    Json,
    Enum { values: Vec<String> },
}

/// Topic-model config: the same optional overrides that used to live in
/// `table_enrichment_settings`, now snapshotted per version.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TopicModelSpec {
    #[serde(flatten)]
    pub overrides: EnrichmentOverrides,
}

impl TopicModelSpec {
    /// Resolve to an effective `EnrichmentConfig`. Known table names start
    /// from their builtin default; any other table starts from a plain-profile
    /// base — this is what makes arbitrary (e.g. uploaded) tables enrichable.
    pub fn to_config(&self, table_name: &str) -> EnrichmentConfig {
        let mut config =
            EnrichmentConfig::builtin_default(table_name).unwrap_or_else(|| EnrichmentConfig {
                text_columns: Vec::new(),
                cleaning_profile: CleaningProfile::Plain,
                language_column: None,
                embedder: EmbedderId::default(),
                min_cluster_size: None,
                algorithm: "kmeans".to_string(),
            });
        config.apply(&self.overrides);
        config
    }
}

/// Supervised classifier head config. Training inputs (taxonomy, labels) live
/// in their own tables; nothing to configure yet.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClassifierSpec {}

/// Extract `{{col:Name}}` references from a template, in order of first
/// appearance, deduplicated. Names are trimmed.
pub fn extract_column_refs(template: &str) -> Vec<String> {
    let mut refs: Vec<String> = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{col:") {
        let Some(after) = rest.get(start + 6..) else {
            break;
        };
        let Some(end) = after.find("}}") else {
            break;
        };
        let name = after.get(..end).unwrap_or("").trim();
        if !name.is_empty() && !refs.iter().any(|r| r == name) {
            refs.push(name.to_string());
        }
        rest = after.get(end + 2..).unwrap_or("");
    }
    refs
}

/// Render a template by substituting `{{col:Name}}` with row values.
/// Unknown references render as empty strings (validation should have
/// caught them earlier; rendering never fails).
pub fn render_prompt(template: &str, values: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{col:") {
        out.push_str(rest.get(..start).unwrap_or(""));
        let Some(after) = rest.get(start + 6..) else {
            rest = "";
            break;
        };
        let Some(end) = after.find("}}") else {
            // Unterminated ref: emit literally.
            out.push_str("{{col:");
            rest = after;
            break;
        };
        let name = after.get(..end).unwrap_or("").trim();
        if let Some(value) = values.get(name) {
            out.push_str(value);
        }
        rest = after.get(end + 2..).unwrap_or("");
    }
    out.push_str(rest);
    out
}

/// Content hash of the spec parts that determine an LLM output.
///
/// Covers template, outputs, provider and model — NOT the version: draft
/// edits must recompute without a bump, and reverting a prompt re-hits old
/// cache for free.
pub fn spec_hash(spec: &LlmPromptSpec) -> String {
    let canonical = serde_json::json!({
        "template": spec.prompt_template,
        "outputs": spec.outputs,
        "provider": spec.provider_id,
        "model": spec.model,
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

/// JSON Schema for the forced `set_values` tool call, hand-built from the
/// output fields. All fields are required.
pub fn output_tool_schema(outputs: &[OutputField]) -> serde_json::Value {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();
    for field in outputs {
        let mut prop = serde_json::Map::new();
        match &field.dtype {
            OutputType::String => {
                prop.insert("type".to_string(), serde_json::json!("string"));
            },
            OutputType::Number => {
                prop.insert("type".to_string(), serde_json::json!("number"));
            },
            OutputType::Bool => {
                prop.insert("type".to_string(), serde_json::json!("boolean"));
            },
            OutputType::Json => {
                prop.insert(
                    "description".to_string(),
                    serde_json::json!("Any JSON value"),
                );
            },
            OutputType::Enum { values } => {
                prop.insert("type".to_string(), serde_json::json!("string"));
                prop.insert("enum".to_string(), serde_json::json!(values));
            },
        }
        if !field.description.is_empty() {
            prop.insert(
                "description".to_string(),
                serde_json::json!(field.description),
            );
        }
        properties.insert(field.name.clone(), serde_json::Value::Object(prop));
        required.push(field.name.clone());
    }
    serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    })
}

/// Validate (and gently coerce) a model-produced arguments object against the
/// output fields. Returns the canonical object keyed by output name; `Err`
/// carries a message suitable for the one re-ask.
pub fn validate_output(
    outputs: &[OutputField],
    value: &serde_json::Value,
) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    let obj = value
        .as_object()
        .ok_or_else(|| "arguments must be a JSON object".to_string())?;
    let mut out = serde_json::Map::new();
    for field in outputs {
        let raw = obj
            .get(&field.name)
            .ok_or_else(|| format!("missing output field '{}'", field.name))?;
        let coerced = coerce_value(&field.dtype, raw)
            .map_err(|why| format!("field '{}': {why}", field.name))?;
        out.insert(field.name.clone(), coerced);
    }
    Ok(out)
}

fn coerce_value(dtype: &OutputType, raw: &serde_json::Value) -> Result<serde_json::Value, String> {
    use serde_json::Value;
    match dtype {
        OutputType::String => match raw {
            Value::String(s) => Ok(Value::String(s.clone())),
            Value::Number(n) => Ok(Value::String(n.to_string())),
            Value::Bool(b) => Ok(Value::String(b.to_string())),
            Value::Null => Err("value is null".to_string()),
            other => Ok(Value::String(other.to_string())),
        },
        OutputType::Number => match raw {
            Value::Number(n) => Ok(Value::Number(n.clone())),
            Value::String(s) => {
                let parsed: f64 = s
                    .trim()
                    .parse()
                    .map_err(|_| format!("'{s}' is not a number"))?;
                serde_json::Number::from_f64(parsed)
                    .map(Value::Number)
                    .ok_or_else(|| format!("'{s}' is not a finite number"))
            },
            _ => Err("expected a number".to_string()),
        },
        OutputType::Bool => match raw {
            Value::Bool(b) => Ok(Value::Bool(*b)),
            Value::String(s) => match s.trim().to_ascii_lowercase().as_str() {
                "true" | "yes" => Ok(Value::Bool(true)),
                "false" | "no" => Ok(Value::Bool(false)),
                _ => Err(format!("'{s}' is not a boolean")),
            },
            _ => Err("expected a boolean".to_string()),
        },
        OutputType::Json => Ok(raw.clone()),
        OutputType::Enum { values } => {
            let s = raw
                .as_str()
                .ok_or_else(|| "expected an enum string".to_string())?;
            let trimmed = s.trim();
            if let Some(exact) = values.iter().find(|v| v.as_str() == trimmed) {
                return Ok(Value::String(exact.clone()));
            }
            let lowered = trimmed.to_ascii_lowercase();
            let ci: Vec<&String> = values
                .iter()
                .filter(|v| v.to_ascii_lowercase() == lowered)
                .collect();
            match ci.as_slice() {
                [only] => Ok(Value::String((*only).clone())),
                _ => Err(format!("'{trimmed}' is not one of: {}", values.join(", "))),
            }
        },
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::shadow_unrelated, clippy::panic)]
mod tests {
    use super::*;

    fn spec(template: &str) -> LlmPromptSpec {
        LlmPromptSpec {
            input_columns: vec!["Title".to_string()],
            prompt_template: template.to_string(),
            outputs: vec![OutputField {
                name: "sentiment".to_string(),
                dtype: OutputType::Enum {
                    values: vec!["pos".to_string(), "neg".to_string()],
                },
                description: String::new(),
            }],
            provider_id: "p1".to_string(),
            model: None,
        }
    }

    #[test]
    fn extracts_refs_in_order_deduplicated() {
        let refs = extract_column_refs("{{col:Title}} and {{col: Body }} then {{col:Title}}");
        assert_eq!(refs, vec!["Title", "Body"]);
        assert!(extract_column_refs("no refs {{col:unterminated").is_empty());
        assert!(extract_column_refs("").is_empty());
    }

    #[test]
    fn renders_with_values_and_empty_for_unknown() {
        let mut values = BTreeMap::new();
        values.insert("Title".to_string(), "Login broken".to_string());
        let out = render_prompt("Summarize: {{col:Title}} / {{col:Missing}}!", &values);
        assert_eq!(out, "Summarize: Login broken / !");
        // Whitespace inside the ref is tolerated.
        let out = render_prompt("{{col: Title }}", &values);
        assert_eq!(out, "Login broken");
    }

    #[test]
    fn spec_hash_is_stable_and_content_sensitive() {
        let a = spec("classify {{col:Title}}");
        let b = spec("classify {{col:Title}}");
        assert_eq!(spec_hash(&a), spec_hash(&b));

        let mut c = spec("classify {{col:Title}}");
        c.prompt_template = "judge {{col:Title}}".to_string();
        assert_ne!(spec_hash(&a), spec_hash(&c));

        // input_columns is NOT part of the hash (refs drive inputs).
        let mut d = spec("classify {{col:Title}}");
        d.input_columns.push("Body".to_string());
        assert_eq!(spec_hash(&a), spec_hash(&d));

        let mut e = spec("classify {{col:Title}}");
        e.model = Some("gpt-x".to_string());
        assert_ne!(spec_hash(&a), spec_hash(&e));
    }

    #[test]
    fn input_hash_is_length_prefixed() {
        let a = input_hash(&[("a".to_string(), "bc".to_string())]);
        let b = input_hash(&[("ab".to_string(), "c".to_string())]);
        assert_ne!(a, b);
        let c = input_hash(&[("a".to_string(), "bc".to_string())]);
        assert_eq!(a, c);
    }

    #[test]
    fn tool_schema_covers_all_types() {
        let outputs = vec![
            OutputField {
                name: "s".to_string(),
                dtype: OutputType::String,
                description: "text".to_string(),
            },
            OutputField {
                name: "n".to_string(),
                dtype: OutputType::Number,
                description: String::new(),
            },
            OutputField {
                name: "b".to_string(),
                dtype: OutputType::Bool,
                description: String::new(),
            },
            OutputField {
                name: "j".to_string(),
                dtype: OutputType::Json,
                description: String::new(),
            },
            OutputField {
                name: "e".to_string(),
                dtype: OutputType::Enum {
                    values: vec!["a".to_string(), "b".to_string()],
                },
                description: String::new(),
            },
        ];
        let schema = output_tool_schema(&outputs);
        assert_eq!(schema["properties"]["s"]["type"], "string");
        assert_eq!(schema["properties"]["s"]["description"], "text");
        assert_eq!(schema["properties"]["n"]["type"], "number");
        assert_eq!(schema["properties"]["b"]["type"], "boolean");
        assert_eq!(
            schema["properties"]["e"]["enum"],
            serde_json::json!(["a", "b"])
        );
        assert_eq!(
            schema["required"],
            serde_json::json!(["s", "n", "b", "j", "e"])
        );
    }

    #[test]
    fn validate_coerces_and_rejects() {
        let outputs = vec![
            OutputField {
                name: "score".to_string(),
                dtype: OutputType::Number,
                description: String::new(),
            },
            OutputField {
                name: "urgent".to_string(),
                dtype: OutputType::Bool,
                description: String::new(),
            },
            OutputField {
                name: "mood".to_string(),
                dtype: OutputType::Enum {
                    values: vec!["Happy".to_string(), "Sad".to_string()],
                },
                description: String::new(),
            },
        ];

        // Coercions: "3" → 3, "true" → true, " happy " → "Happy"
        let ok = validate_output(
            &outputs,
            &serde_json::json!({"score": "3", "urgent": "true", "mood": " happy "}),
        )
        .unwrap();
        assert_eq!(ok["score"], serde_json::json!(3.0));
        assert_eq!(ok["urgent"], serde_json::json!(true));
        assert_eq!(ok["mood"], serde_json::json!("Happy"));

        // Native types pass through.
        let ok = validate_output(
            &outputs,
            &serde_json::json!({"score": 7, "urgent": false, "mood": "Sad"}),
        )
        .unwrap();
        assert_eq!(ok["score"], serde_json::json!(7));

        // Failures name the field.
        let err = validate_output(
            &outputs,
            &serde_json::json!({"score": "many", "urgent": true, "mood": "Sad"}),
        )
        .unwrap_err();
        assert!(err.contains("score"), "{err}");

        let err = validate_output(
            &outputs,
            &serde_json::json!({"urgent": true, "mood": "Sad"}),
        )
        .unwrap_err();
        assert!(err.contains("missing"), "{err}");

        let err = validate_output(
            &outputs,
            &serde_json::json!({"score": 1, "urgent": true, "mood": "angry"}),
        )
        .unwrap_err();
        assert!(err.contains("Happy"), "enum errors list options: {err}");

        let err = validate_output(&outputs, &serde_json::json!("not an object")).unwrap_err();
        assert!(err.contains("object"), "{err}");
    }

    #[test]
    fn function_spec_round_trips_tagged_json() {
        let json = r#"{"kind":"topic_model","text_columns":["title","body"],"cleaning_profile":"plain","language_column":null,"embedder":null,"min_cluster_size":25,"algorithm":"hdbscan"}"#;
        let parsed: FunctionSpec = serde_json::from_str(json).unwrap();
        match &parsed {
            FunctionSpec::TopicModel(tm) => {
                assert_eq!(
                    tm.overrides.text_columns,
                    Some(vec!["title".to_string(), "body".to_string()])
                );
                assert_eq!(tm.overrides.min_cluster_size, Some(25));
            },
            other => panic!("wrong variant: {other:?}"),
        }
        assert_eq!(parsed.kind_str(), "topic_model");

        let llm = FunctionSpec::LlmPrompt(LlmPromptSpec {
            input_columns: vec!["t".to_string()],
            prompt_template: "x {{col:t}}".to_string(),
            outputs: vec![],
            provider_id: "p".to_string(),
            model: None,
        });
        let text = serde_json::to_string(&llm).unwrap();
        assert!(text.contains(r#""kind":"llm_prompt""#), "{text}");
    }

    #[test]
    fn topic_model_to_config_falls_back_to_plain_for_unknown_tables() {
        // Known table: builtin default + overrides.
        let tm: TopicModelSpec = serde_json::from_str(
            r#"{"min_cluster_size": 30, "text_columns": null, "cleaning_profile": null,
                "language_column": null, "embedder": null, "algorithm": null}"#,
        )
        .unwrap();
        let config = tm.to_config("issues");
        assert_eq!(config.text_columns, vec!["title", "body"]);
        assert_eq!(config.min_cluster_size, Some(30));

        // Unknown table: plain-profile base, overrides supply the columns.
        let tm: TopicModelSpec = serde_json::from_str(
            r#"{"text_columns": ["message"], "cleaning_profile": null, "language_column": null,
                "embedder": null, "min_cluster_size": null, "algorithm": null}"#,
        )
        .unwrap();
        let config = tm.to_config("uploaded_feedback");
        assert_eq!(config.text_columns, vec!["message"]);
        assert_eq!(config.cleaning_profile, CleaningProfile::Plain);
        assert_eq!(config.algorithm, "kmeans");
    }
}
