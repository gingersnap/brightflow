//! A model serialised by this crate is a valid Ossie document.
//!
//! Validates against the vendored `fixtures/ossie-schema.json` (see the README
//! beside it for the commit). The model exercises every construct the crate
//! writes: two datasets with fields carrying datatype, `is_time`, description,
//! `ai_context` and a `BRIGHTFLOW` extension; a relationship; a metric with a
//! rendered ANSI SQL expression and the structured form in its extension; and
//! a dataset-level extension.

use brightflow_types::{
    Aggregation, AiContext, AiContextFields, ColumnExt, ColumnRole, Dataset, DatasetExt, DocFields,
    Field, LogicalType, Metric, MetricExpr, MetricExt, OssieDocument, Polarity, Relationship,
    SemanticModel, TimeGranularity,
};

fn issues_dataset() -> Dataset {
    let mut issues = Dataset::new("issues", "connector:github/issues");
    issues.primary_key = vec!["id".into()];
    issues.description = Some("Issues and pull requests, one row each".into());
    issues.ai_context = Some(AiContext::Structured(AiContextFields {
        synonyms: vec!["tickets".into(), "bugs".into()],
        ..Default::default()
    }));
    issues.fields = vec![
        Field::column("id")
            .with_datatype(LogicalType::Integer)
            .with_brightflow(&ColumnExt::role(ColumnRole::Ignored)),
        Field::column("number").with_datatype(LogicalType::Integer),
        Field::column("created_at")
            .with_datatype(LogicalType::DateTime)
            .with_is_time(true)
            .with_description("When the issue was opened"),
        Field::column("updated_at")
            .with_datatype(LogicalType::DateTime)
            .with_is_time(false),
        Field::column("reactions_total")
            .with_datatype(LogicalType::Integer)
            .with_brightflow(
                &ColumnExt::role(ColumnRole::Measure)
                    .with_kpi(true)
                    .with_polarity(Polarity::HigherIsBetter)
                    .with_label("Reactions"),
            ),
    ];
    issues.set_brightflow(&DatasetExt {
        display_name: Some("Issues".into()),
        time_granularity: Some(TimeGranularity::Week),
        comparison_periods: Some(4),
        doc: Some(DocFields {
            id: Some("id".into()),
            number: Some("number".into()),
            title: Some("title".into()),
            body: Some("body".into()),
            timestamp: Some("created_at".into()),
            url_template: Some("{html_url}".into()),
        }),
    });

    issues
}

fn github_model() -> SemanticModel {
    let mut comments = Dataset::new("issue_comments", "connector:github/issue_comments");
    comments.primary_key = vec!["id".into()];
    comments.fields = vec![
        Field::column("id").with_datatype(LogicalType::Integer),
        Field::column("issue_number").with_datatype(LogicalType::Integer),
    ];

    SemanticModel {
        name: "connector:github".into(),
        description: Some("A GitHub repository".into()),
        ai_context: Some(AiContext::Text(
            "Engineering activity on one repository".into(),
        )),
        datasets: vec![issues_dataset(), comments],
        relationships: vec![Relationship {
            name: "comment_issue".into(),
            from: "issue_comments".into(),
            to: "issues".into(),
            from_columns: vec!["issue_number".into()],
            to_columns: vec!["number".into()],
            ai_context: None,
            custom_extensions: Vec::new(),
        }],
        metrics: vec![Metric::structured(
            "reactions",
            &MetricExt {
                is_kpi: Some(true),
                polarity: Some(Polarity::HigherIsBetter),
                ..MetricExt::new(
                    MetricExpr::new("reactions_total", Aggregation::Sum).in_dataset("issues"),
                )
            },
        )
        .with_description("Total reactions across issues")],
        custom_extensions: Vec::new(),
    }
}

#[test]
fn exported_document_validates_against_the_vendored_schema() {
    let model = github_model();
    assert_eq!(model.validate(), Ok(()));

    let document = serde_json::to_value(OssieDocument::new(model)).expect("serialise");
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ossie-schema.json")).expect("fixture parses");
    let validator = jsonschema::validator_for(&schema).expect("schema compiles");
    let errors: Vec<String> = validator
        .iter_errors(&document)
        .map(|e| format!("{} at {}", e, e.instance_path()))
        .collect();
    assert!(
        errors.is_empty(),
        "Ossie violations:\n{}",
        errors.join("\n")
    );
}

#[test]
fn exported_document_round_trips_through_the_strict_form() {
    let model = github_model();
    let json = serde_json::to_value(OssieDocument::new(model.clone())).expect("serialise");
    let back: OssieDocument = serde_json::from_value(json).expect("deserialise");
    assert_eq!(back.semantic_model, vec![model]);
}
