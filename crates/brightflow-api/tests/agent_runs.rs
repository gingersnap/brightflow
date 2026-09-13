//! End-to-end pins on the `describe_table` agent kind: the store admits the
//! kind, the run's context carries the column profiles the prompt promises,
//! and what the run proposes lands at the agent layer through the same bus
//! a person uses — held as a proposal until approved, applied at once when
//! the run auto-applies.

#![expect(
    clippy::unwrap_used,
    clippy::too_many_lines,
    reason = "integration tests panic on failure by design and read top to bottom"
)]

use std::sync::Arc;

use axum::extract::{Path, State};
use polars::prelude::*;

use brightflow_api::actions::types::{Action, ActionStatus, Scope};
use brightflow_api::actions::{dispatch_action, Actor};
use brightflow_api::agent::describe;
use brightflow_api::state::AppState;
use brightflow_store::ParquetStore;
use brightflow_test_support::{copy_template, TestWorkspace};
use brightflow_types::Layer;

const SOURCE: &str = "test-source";
const TABLE: &str = "issues";

/// Boot the store from a copy of the committed test workspace, plant a
/// small `issues` table on it and run the detector over it, so the
/// `detected` layer is present under everything the test writes — as it is
/// for every table production creates.
async fn state_with_planted_table(ws: &TestWorkspace) -> AppState {
    let store = ParquetStore::new(ws.paths.store(), &ws.paths.litehouse_url())
        .await
        .unwrap();
    let parquet_path = ws.root().join("issues.parquet");
    let mut df = df!(
        "id" => &[1_i64, 2, 3],
        "title" => &["a", "b", "c"],
        "body" => &["text one", "text two", "text three"],
        "reactions" => &[4_i64, 0, 12],
    )
    .unwrap();
    let file = std::fs::File::create(&parquet_path).unwrap();
    ParquetWriter::new(file).finish(&mut df).unwrap();
    store
        .ingest_parquet(SOURCE, TABLE, &parquet_path, None)
        .await
        .unwrap();
    brightflow_api::semantics::detect::declare_detected(&store, SOURCE, TABLE)
        .await
        .unwrap();
    AppState::with_store(store).await
}

/// Migration 030 admits the kind; the run's context is the system prompt
/// plus a JSON profile of every column with its missing fields.
#[tokio::test]
async fn describe_table_run_is_admitted_and_sees_column_profiles() {
    let ws = copy_template().unwrap();
    let state = state_with_planted_table(&ws).await;
    let store = state.store().unwrap();

    let run = store
        .db()
        .insert_agent_run(
            "describe_table",
            "propose",
            &format!("describe_table:{SOURCE}:{TABLE}"),
            chrono::Utc::now().timestamp(),
        )
        .await
        .unwrap();
    assert_eq!(run.kind, "describe_table");

    let (system, user_text) = describe::context(&state, SOURCE, TABLE).await.unwrap();
    assert_eq!(system, describe::SYSTEM_PROMPT);
    let user: serde_json::Value = serde_json::from_str(&user_text).unwrap();
    assert_eq!(user["table"]["name"], TABLE);
    assert_eq!(user["table"]["rows"], 3);
    assert_eq!(user["table"]["missing"], serde_json::json!(["description"]));
    let columns = user["columns"].as_array().unwrap();
    let names: Vec<&str> = columns
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["id", "title", "body", "reactions"]);
    // The detector declared roles, so only the description is missing on a
    // dimension; a measure also wants polarity and a KPI decision.
    let title = columns.iter().find(|c| c["name"] == "title").unwrap();
    assert_eq!(title["role"], "dimension");
    assert_eq!(title["missing"], serde_json::json!(["description"]));
    assert_eq!(title["distinct"], 3);
    assert_eq!(title["samples"], serde_json::json!(["a", "b", "c"]));
    let reactions = columns.iter().find(|c| c["name"] == "reactions").unwrap();
    assert_eq!(reactions["min"], "0");
    assert_eq!(reactions["max"], "12");
    assert_eq!(user["sampleRows"].as_array().unwrap().len(), 3);
    assert_eq!(user["sampleRows"][0]["body"], "text one");
}

/// An agent's semantic action is a proposal until approved, then an
/// agent-layer row under `agent:{run}`; an auto-apply run writes the row at
/// once. Either way a person's row still outranks it.
#[tokio::test]
async fn describe_table_proposals_land_at_the_agent_layer() {
    let ws = copy_template().unwrap();
    let state = state_with_planted_table(&ws).await;
    let store = state.store().unwrap();
    let run = store
        .db()
        .insert_agent_run(
            "describe_table",
            "propose",
            &format!("describe_table:{SOURCE}:{TABLE}"),
            chrono::Utc::now().timestamp(),
        )
        .await
        .unwrap();
    let scope = || Scope {
        source_id: SOURCE.to_string(),
        table: TABLE.to_string(),
    };
    let resolved = |column: &str| {
        let handle = Arc::clone(store);
        let wanted = column.to_string();
        async move {
            handle
                .resolved_columns(SOURCE, TABLE)
                .await
                .unwrap()
                .into_iter()
                .find(|c| c.name == wanted)
                .unwrap()
        }
    };

    // Propose mode: recorded, not applied.
    let response = dispatch_action(
        &state,
        Action::SetColumnDescription {
            scope: scope(),
            column: "title".to_string(),
            description: Some("The issue's headline.".to_string()),
        },
        "agent-1-0-call-a",
        Actor::Agent {
            run_id: run.id,
            auto_apply: false,
        },
    )
    .await
    .unwrap();
    assert_eq!(response.status, ActionStatus::Proposed);
    assert_eq!(resolved("title").await.description, None);

    // Approval applies it at the agent layer, attributed to the run.
    let approved =
        brightflow_api::actions::handlers::approve(State(state.clone()), Path(response.log_id))
            .await
            .unwrap()
            .0;
    assert_eq!(approved.status, ActionStatus::Applied);
    let title = resolved("title").await;
    assert_eq!(title.description.as_deref(), Some("The issue's headline."));
    assert_eq!(
        title
            .resolved_by
            .as_ref()
            .map(|p| (p.layer, p.producer.clone())),
        Some((Layer::Agent, format!("agent:{}", run.id)))
    );

    // Auto-apply mode writes the row directly.
    let applied = dispatch_action(
        &state,
        Action::SetColumnDescription {
            scope: scope(),
            column: "body".to_string(),
            description: Some("The issue's text.".to_string()),
        },
        "agent-1-0-call-b",
        Actor::Agent {
            run_id: run.id,
            auto_apply: true,
        },
    )
    .await
    .unwrap();
    assert_eq!(applied.status, ActionStatus::Applied);
    assert_eq!(
        resolved("body").await.description.as_deref(),
        Some("The issue's text.")
    );

    // A person's later edit outranks the agent's row.
    dispatch_action(
        &state,
        Action::SetColumnDescription {
            scope: scope(),
            column: "body".to_string(),
            description: Some("Markdown body as written.".to_string()),
        },
        "req-user-body",
        Actor::Human {
            user_id: "user-1".to_string(),
        },
    )
    .await
    .unwrap();
    let body = resolved("body").await;
    assert_eq!(
        body.description.as_deref(),
        Some("Markdown body as written.")
    );
    assert_eq!(
        body.resolved_by.as_ref().map(|p| p.layer),
        Some(Layer::User)
    );
}
