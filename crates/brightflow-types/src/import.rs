//! A semantic model handed in as a document, turned into per-table
//! declarations the store can apply.
//!
//! Input is either a full Ossie document (`{ version, semantic_model: [..] }`)
//! or one bare model, in the strict Ossie shape or the short hand-authored
//! form the model types already accept. One model is one source; each of its
//! datasets becomes a `TableDeclaration` named after the dataset, carrying
//! the relationships that start at it and the metrics whose expression names
//! it. A metric without a structured expression is carried too, and the
//! store reports it as skipped, since only structured metrics execute.

use crate::declaration::TableDeclaration;
use crate::semantic::{OssieDocument, SemanticModel};

/// Parse a document or a bare model. A document with several models is
/// refused: one source holds one model.
pub fn parse_model(value: serde_json::Value) -> Result<SemanticModel, String> {
    let is_document = value.get("semantic_model").is_some();
    if is_document {
        let document: OssieDocument =
            serde_json::from_value(value).map_err(|e| format!("not an Ossie document: {e}"))?;
        let mut models = document.semantic_model;
        return match models.len() {
            1 => Ok(models.remove(0)),
            0 => Err("the document has no semantic model".to_string()),
            n => Err(format!(
                "the document has {n} semantic models; a source takes one"
            )),
        };
    }
    serde_json::from_value(value).map_err(|e| format!("not a semantic model: {e}"))
}

/// One declaration per dataset, each with its own relationships and metrics.
pub fn declarations_of(model: &SemanticModel) -> Vec<TableDeclaration> {
    model
        .datasets
        .iter()
        .map(|dataset| {
            let name = dataset.name.clone();
            let relationships = model
                .relationships
                .iter()
                .filter(|r| r.from == name)
                .cloned()
                .collect();
            let metrics = model
                .metrics
                .iter()
                .filter(|m| {
                    m.structured_expr()
                        .and_then(|e| e.dataset)
                        .is_some_and(|d| d == name)
                })
                .cloned()
                .collect();
            TableDeclaration {
                dataset: Some(dataset.clone()),
                relationships,
                metrics,
                ..TableDeclaration::new(name)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model_json() -> serde_json::Value {
        serde_json::json!({
            "name": "github",
            "datasets": [
                {"name": "issues", "source": "issues", "primary_key": ["id"],
                 "fields": {"id": {"datatype": "Integer"}, "number": {"datatype": "Integer"},
                            "reactions_total": {"datatype": "Integer", "brightflow": {"role": "measure"}}}},
                {"name": "issue_comments", "source": "issue_comments",
                 "fields": {"id": {"datatype": "Integer"}, "issue_number": {"datatype": "Integer"}}}
            ],
            "relationships": {"issue": {"from": "issue_comments", "to": "issues",
                                        "from_columns": ["issue_number"], "to_columns": ["number"]}},
            "metrics": {"reactions": {"expression": {"dataset": "issues", "column": "reactions_total", "aggregation": "sum"}}}
        })
    }

    #[test]
    fn a_bare_model_and_a_document_both_parse() {
        let bare = parse_model(model_json()).expect("bare");
        assert_eq!(bare.name, "github");
        let document = serde_json::json!({"version": "0.2.0", "semantic_model": [model_json()]});
        let from_document = parse_model(document).expect("document");
        assert_eq!(from_document.datasets.len(), 2);
        let empty = serde_json::json!({"version": "0.2.0", "semantic_model": []});
        assert!(parse_model(empty)
            .unwrap_err()
            .contains("no semantic model"));
        let two =
            serde_json::json!({"version": "0.2.0", "semantic_model": [model_json(), model_json()]});
        assert!(parse_model(two).unwrap_err().contains("2 semantic models"));
        assert!(parse_model(serde_json::json!({"datasets": 3})).is_err());
    }

    #[test]
    fn each_dataset_becomes_a_declaration_with_its_own_relationships_and_metrics() {
        let model = parse_model(model_json()).expect("model");
        let decls = declarations_of(&model);
        assert_eq!(decls.len(), 2);
        let issues = decls.iter().find(|d| d.name == "issues").expect("issues");
        assert_eq!(issues.dataset.as_ref().unwrap().primary_key, ["id"]);
        assert_eq!(issues.metrics.len(), 1);
        assert!(issues.relationships.is_empty());
        let comments = decls
            .iter()
            .find(|d| d.name == "issue_comments")
            .expect("comments");
        assert_eq!(comments.relationships.len(), 1);
        assert_eq!(comments.relationships[0].to, "issues");
        assert!(comments.metrics.is_empty());
        for d in &decls {
            assert_eq!(d.validate(), Ok(()), "{}", d.name);
        }
    }
}
