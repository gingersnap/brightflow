//! Both ticket calls end to end against a scripted OpenAI-compatible mock:
//! classify → six columns with ids resolved to names; extract → the
//! `{table}_mentions` child table, the parent flags, the unresolved queue,
//! and — the property a PK merge would have broken — a ticket re-enriched
//! with fewer mentions leaves no stale rows. The mock answers from the
//! ticket text it is sent, so the assertions do not depend on the runner's
//! concurrency order, and it records every system turn so the
//! prefix-caching contract (byte-identical across rows) is checked too.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    clippy::shadow_unrelated,
    reason = "integration tests panic on failure by design, read top to bottom, and \
              reuse binding names across sequential steps"
)]

use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use polars::prelude::*;

use brightflow_api::enrichment::runner::{
    execute_cells, materialize, prepare_run_inputs, CacheMode,
};
use brightflow_api::enrichment::vocab;
use brightflow_api::state::AppState;
use brightflow_engine::enrichment::{FunctionSpec, TicketClassifySpec, TicketExtractSpec};
use brightflow_llm::ChatClient;
use brightflow_store::ParquetStore;
use brightflow_test_support::{copy_template, TestWorkspace};

const SOURCE: &str = "test-source";
const TABLE: &str = "issues";

/// What the mock returns for the extract call: full mentions, or nothing.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ExtractMode {
    Mentions,
    Empty,
}

#[derive(Clone)]
struct Mock {
    system_turns: Arc<Mutex<Vec<String>>>,
    extract_mode: Arc<Mutex<ExtractMode>>,
}

fn tool_response(name: &str, args: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call-1",
                    "type": "function",
                    "function": { "name": name, "arguments": args.to_string() }
                }]
            },
            "finish_reason": "tool_calls"
        }],
        "usage": {
            "prompt_tokens": 300, "completion_tokens": 20, "total_tokens": 320,
            "prompt_tokens_details": { "cached_tokens": 256 }
        }
    })
}

async fn completions(
    State(mock): State<Mock>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let messages = body["messages"].as_array().unwrap();
    let system = messages[0]["content"].as_str().unwrap().to_string();
    let user = messages[1]["content"].as_str().unwrap().to_string();
    mock.system_turns.lock().unwrap().push(system);
    let tool = body["tools"][0]["function"]["name"].as_str().unwrap();
    let about_vat = user.contains("VAT");
    if tool == "classify_ticket" {
        return Json(tool_response(
            tool,
            &if about_vat {
                serde_json::json!({
                    "summary": "Invoice omits the VAT line for EU customers",
                    "category": "Billing",
                    "subcategory": "VAT",
                    "sentiment_polarity": "negative",
                    "sentiment_strength": "low"
                })
            } else {
                serde_json::json!({
                    "summary": "Password reset link expired",
                    "category": "other",
                    "subcategory": "other",
                    // `none` was folded into `neutral`: text with no evaluative
                    // content is neutral, and strength is always meaningful.
                    "sentiment_polarity": "neutral",
                    "sentiment_strength": "low"
                })
            },
        ));
    }
    let mode = *mock.extract_mode.lock().unwrap();
    let mentions = if about_vat && mode == ExtractMode::Mentions {
        serde_json::json!([
            { "type": "product", "subject": "invoices page", "feedback_summary": null,
              "feedback_category": null, "incidental": false, "polarity": "negative", "confidence": 0.9 },
            { "type": "feedback", "subject": null, "feedback_summary": "invoice screen confusing",
              "feedback_category": "Usability", "incidental": true, "polarity": "negative", "confidence": 0.8 },
            { "type": "competitor", "subject": "Unknown Corp", "feedback_summary": null,
              "feedback_category": null, "incidental": true, "polarity": "positive", "confidence": 0.7 }
        ])
    } else {
        serde_json::json!([])
    };
    Json(tool_response(
        tool,
        &serde_json::json!({ "mentions": mentions }),
    ))
}

async fn serve(mock: Mock) -> String {
    let app = Router::new()
        .route("/v1/chat/completions", post(completions))
        .with_state(mock);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}/v1")
}

async fn state_with_issues(ws: &TestWorkspace) -> AppState {
    let store = ParquetStore::new(ws.paths.store(), &ws.paths.litehouse_url())
        .await
        .unwrap();
    let parquet_path = ws.root().join("issues.parquet");
    let mut df = df!(
        "id" => &[1_i64, 2, 3],
        "title" => &["Invoice missing VAT line", "Password reset link expired", "Invoice missing VAT line"],
        "body" => &[
            "The generated invoice does not show the VAT breakdown for customers in the EU.",
            "The reset link expired before I could use it, please send another one.",
            "The generated invoice does not show the VAT breakdown for customers in the EU.",
        ],
    )
    .unwrap();
    let file = std::fs::File::create(&parquet_path).unwrap();
    ParquetWriter::new(file).finish(&mut df).unwrap();
    store
        .ingest_parquet(SOURCE, TABLE, &parquet_path, None)
        .await
        .unwrap();
    AppState::with_store(store).await
}

/// Define the vocabulary directly in the store: a category with one
/// subcategory, a product with an alias, and one feedback category.
async fn seed_vocabulary(store: &ParquetStore, table_id: &str) {
    let db = store.db();
    let billing = db
        .upsert_taxonomy_category(
            table_id,
            "category",
            0,
            "Billing",
            Some("charges and invoices"),
            None,
            0,
        )
        .await
        .unwrap();
    db.upsert_taxonomy_category(
        table_id,
        "subcategory",
        billing.id,
        "VAT",
        Some("missing VAT line"),
        None,
        0,
    )
    .await
    .unwrap();
    let area = db
        .upsert_taxonomy_category(
            table_id,
            "product",
            0,
            "Billing area",
            Some("billing"),
            None,
            0,
        )
        .await
        .unwrap();
    db.upsert_taxonomy_category(
        table_id,
        "product",
        area.id,
        "Invoice screen",
        Some("the invoices page"),
        Some(r#"["invoices page"]"#),
        0,
    )
    .await
    .unwrap();
    db.upsert_taxonomy_category(
        table_id,
        "feedback_category",
        0,
        "Usability",
        Some("friction"),
        None,
        0,
    )
    .await
    .unwrap();
}

async fn create_function(
    store: &ParquetStore,
    table_id: &str,
    name: &str,
    mut spec: FunctionSpec,
) -> String {
    vocab::inject(store, table_id, &mut spec).await.unwrap();
    let row = store
        .db()
        .create_enrichment_function(
            table_id,
            name,
            spec.kind_str(),
            "promoted",
            &serde_json::to_string(&spec).unwrap(),
        )
        .await
        .unwrap();
    row.id
}

fn text_columns() -> Vec<String> {
    vec!["title".to_string(), "body".to_string()]
}

#[tokio::test]
async fn classify_and_extract_materialize_columns_child_table_and_queue() {
    let ws = copy_template().unwrap();
    let state = state_with_issues(&ws).await;
    let store = Arc::clone(state.store().unwrap());
    let table_id = store
        .db()
        .get_table(SOURCE, TABLE)
        .await
        .unwrap()
        .unwrap()
        .id;
    seed_vocabulary(&store, &table_id).await;

    let mock = Mock {
        system_turns: Arc::new(Mutex::new(Vec::new())),
        extract_mode: Arc::new(Mutex::new(ExtractMode::Mentions)),
    };
    let base = serve(mock.clone()).await;
    let client = ChatClient::new(base, None, "mock".to_string());

    // ── Call A ────────────────────────────────────────────────────────
    let classify_id = create_function(
        &store,
        &table_id,
        "classify",
        FunctionSpec::TicketClassify(TicketClassifySpec {
            text_columns: text_columns(),
            language_column: None,
            provider_id: "p".to_string(),
            model: None,
            categories: vec![],
            subcategories: vec![],
        }),
    )
    .await;
    let version = store
        .db()
        .get_enrichment_function_version(&classify_id, 1)
        .await
        .unwrap()
        .unwrap();
    let spec: FunctionSpec = serde_json::from_str(&version.config_json).unwrap();
    let run = vocab::run_spec_for(&store, &table_id, spec).await.unwrap();
    let df = store.read_table(SOURCE, TABLE).await.unwrap();
    let inputs = prepare_run_inputs(&df, &run).unwrap();
    let outcome = execute_cells(
        &store,
        &client,
        &classify_id,
        1,
        &run,
        &inputs,
        None,
        CacheMode::Use,
    )
    .await
    .unwrap();
    // Rows 1 and 3 are identical: one cell, two rows.
    assert_eq!(outcome.cells.len(), 2);
    assert_eq!(outcome.cached_tokens, 2 * 256);
    materialize(
        &state,
        &store,
        SOURCE,
        TABLE,
        &classify_id,
        "classify",
        &run,
    )
    .await
    .unwrap();

    let df = store.read_table(SOURCE, TABLE).await.unwrap();
    let col = |name: &str| -> Vec<Option<String>> {
        df.column(name)
            .unwrap()
            .str()
            .unwrap()
            .into_iter()
            .map(|o| o.map(str::to_string))
            .collect()
    };
    assert_eq!(col("category")[0].as_deref(), Some("Billing"));
    assert_eq!(col("subcategory")[0].as_deref(), Some("VAT"));
    assert_eq!(col("category")[1].as_deref(), Some("other"));
    assert_eq!(col("language")[0].as_deref(), Some("en"));
    assert_eq!(col("sentiment_polarity")[1].as_deref(), Some("neutral"));
    assert_eq!(col("classify__status")[2].as_deref(), Some("ok"));
    assert!(col("summary")[0]
        .as_deref()
        .unwrap()
        .starts_with("Invoice omits"));

    // Prefix caching: every system turn of the classify call was identical.
    let turns = mock.system_turns.lock().unwrap().clone();
    assert!(turns.len() >= 2);
    assert!(turns.iter().all(|t| t == &turns[0]));
    mock.system_turns.lock().unwrap().clear();

    // ── Call B ────────────────────────────────────────────────────────
    let extract_id = create_function(
        &store,
        &table_id,
        "extract",
        FunctionSpec::TicketExtract(TicketExtractSpec {
            text_columns: text_columns(),
            language_column: None,
            provider_id: "p".to_string(),
            model: None,
            products: vec![],
            competitors: vec![],
            feedback_categories: vec![],
        }),
    )
    .await;
    let version = store
        .db()
        .get_enrichment_function_version(&extract_id, 1)
        .await
        .unwrap()
        .unwrap();
    let spec: FunctionSpec = serde_json::from_str(&version.config_json).unwrap();
    let run = vocab::run_spec_for(&store, &table_id, spec).await.unwrap();
    let df = store.read_table(SOURCE, TABLE).await.unwrap();
    let inputs = prepare_run_inputs(&df, &run).unwrap();
    execute_cells(
        &store,
        &client,
        &extract_id,
        1,
        &run,
        &inputs,
        None,
        CacheMode::Use,
    )
    .await
    .unwrap();
    materialize(&state, &store, SOURCE, TABLE, &extract_id, "extract", &run)
        .await
        .unwrap();

    let parent = store.read_table(SOURCE, TABLE).await.unwrap();
    let flag = |name: &str| -> Vec<Option<bool>> {
        parent
            .column(name)
            .unwrap()
            .bool()
            .unwrap()
            .into_iter()
            .collect()
    };
    assert_eq!(
        flag("has_feedback"),
        vec![Some(true), Some(false), Some(true)]
    );
    assert_eq!(flag("has_incidental_feedback")[0], Some(true));
    assert_eq!(flag("has_competitor_mention")[1], Some(false));
    let counts: Vec<Option<i32>> = parent
        .column("mention_count")
        .unwrap()
        .i32()
        .unwrap()
        .into_iter()
        .collect();
    assert_eq!(counts, vec![Some(3), Some(0), Some(3)]);

    let mentions = store.read_table(SOURCE, "issues_mentions").await.unwrap();
    assert_eq!(mentions.height(), 6, "two VAT tickets × three mentions");
    assert_eq!(
        mentions.column("ticket_id").unwrap().dtype(),
        &DataType::Int64
    );
    // The alias "invoices page" resolved to the catalog entry's name.
    assert!(mentions
        .column("subject")
        .unwrap()
        .str()
        .unwrap()
        .into_iter()
        .any(|v| v == Some("Invoice screen")));
    assert!(mentions
        .column("subject_surface")
        .unwrap()
        .str()
        .unwrap()
        .into_iter()
        .any(|v| v == Some("Unknown Corp")));
    assert!(mentions
        .column("feedback_category")
        .unwrap()
        .str()
        .unwrap()
        .into_iter()
        .any(|v| v == Some("Usability")));

    // The unknown competitor landed in the review queue, counted once per mention.
    let queue = store
        .db()
        .list_unresolved_subjects(&table_id, Some("open"))
        .await
        .unwrap();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].surface, "Unknown Corp");
    assert_eq!(queue[0].mention_count, 2);

    // ── Re-enrichment with fewer mentions leaves no stale child rows ──
    *mock.extract_mode.lock().unwrap() = ExtractMode::Empty;
    store
        .db()
        .delete_cache_for_spec(&extract_id, &run.spec_hash())
        .await
        .unwrap();
    let df = store.read_table(SOURCE, TABLE).await.unwrap();
    let inputs = prepare_run_inputs(&df, &run).unwrap();
    execute_cells(
        &store,
        &client,
        &extract_id,
        1,
        &run,
        &inputs,
        None,
        CacheMode::Use,
    )
    .await
    .unwrap();
    materialize(&state, &store, SOURCE, TABLE, &extract_id, "extract", &run)
        .await
        .unwrap();
    let mentions = store.read_table(SOURCE, "issues_mentions").await.unwrap();
    assert_eq!(mentions.height(), 0);
    let parent = store.read_table(SOURCE, TABLE).await.unwrap();
    let counts: Vec<Option<i32>> = parent
        .column("mention_count")
        .unwrap()
        .i32()
        .unwrap()
        .into_iter()
        .collect();
    assert_eq!(counts, vec![Some(0), Some(0), Some(0)]);
    let queue = store
        .db()
        .list_unresolved_subjects(&table_id, Some("open"))
        .await
        .unwrap();
    assert_eq!(
        queue[0].mention_count, 0,
        "unseen surfaces drop to zero, keep status"
    );
}
