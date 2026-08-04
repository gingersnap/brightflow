//! Integration tests against a scripted mock OpenAI-compatible server.

#![expect(
    clippy::unwrap_used,
    reason = "integration tests panic on failure by design"
)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use axum::{Json, Router};

use brightflow_llm::{ChatClient, ChatMessage, LlmError, ToolDef};

/// Scripted responses served in order; repeats the last one when exhausted.
#[derive(Clone)]
struct Script {
    responses: Arc<Vec<(StatusCode, serde_json::Value)>>,
    hits: Arc<AtomicUsize>,
    require_bearer: Option<String>,
}

async fn completions(
    State(script): State<Script>,
    headers: HeaderMap,
    Json(_body): Json<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Some(expected) = &script.require_bearer {
        let ok = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v == format!("Bearer {expected}"));
        if !ok {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "bad key"})),
            );
        }
    }
    let idx = script.hits.fetch_add(1, Ordering::SeqCst);
    let (status, body) = script
        .responses
        .get(idx.min(script.responses.len() - 1))
        .cloned()
        .unwrap();
    (status, Json(body))
}

async fn serve(script: Script) -> String {
    let app = Router::new()
        .route("/v1/chat/completions", post(completions))
        .with_state(script);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}/v1")
}

fn text_response(content: &str) -> serde_json::Value {
    serde_json::json!({
        "choices": [{
            "message": {"role": "assistant", "content": content},
            "finish_reason": "stop"
        }],
        "usage": {"total_tokens": 7}
    })
}

#[tokio::test]
async fn plain_chat_round_trip() {
    let base = serve(Script {
        responses: Arc::new(vec![(StatusCode::OK, text_response("hi there"))]),
        hits: Arc::new(AtomicUsize::new(0)),
        require_bearer: None,
    })
    .await;
    let client = ChatClient::new(base, None, "test-model");
    let outcome = client
        .chat(&[ChatMessage::user("hello")], &[])
        .await
        .unwrap();
    assert_eq!(outcome.text(), "hi there");
    assert_eq!(outcome.total_tokens, Some(7));
}

#[tokio::test]
async fn bearer_auth_is_sent_and_enforced() {
    let base = serve(Script {
        responses: Arc::new(vec![(StatusCode::OK, text_response("ok"))]),
        hits: Arc::new(AtomicUsize::new(0)),
        require_bearer: Some("sekrit".to_string()),
    })
    .await;

    // Wrong key → Auth error
    let bad = ChatClient::new(base.clone(), Some("wrong".to_string()), "m");
    assert!(matches!(
        bad.chat(&[ChatMessage::user("x")], &[]).await,
        Err(LlmError::Auth { status: 401, .. })
    ));

    // Right key → success
    let good = ChatClient::new(base, Some("sekrit".to_string()), "m");
    assert!(good.ping().await.is_ok());
}

#[tokio::test]
async fn tool_calls_are_parsed() {
    let tool_response = serde_json::json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call_42",
                    "type": "function",
                    "function": {
                        "name": "rename_cluster",
                        "arguments": "{\"cluster_id\": 3, \"name\": \"Payments\"}"
                    }
                }]
            },
            "finish_reason": "tool_calls"
        }]
    });
    let base = serve(Script {
        responses: Arc::new(vec![(StatusCode::OK, tool_response)]),
        hits: Arc::new(AtomicUsize::new(0)),
        require_bearer: None,
    })
    .await;
    let client = ChatClient::new(base, None, "m");
    let tools = [ToolDef {
        name: "rename_cluster".to_string(),
        description: "Rename a topic cluster".to_string(),
        parameters: serde_json::json!({"type": "object"}),
    }];
    let outcome = client
        .chat(&[ChatMessage::user("label these")], &tools)
        .await
        .unwrap();
    let calls = outcome.tool_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].id, "call_42");
    assert_eq!(calls[0].function.name, "rename_cluster");
    let args: serde_json::Value = serde_json::from_str(&calls[0].function.arguments).unwrap();
    assert_eq!(args["name"], "Payments");
}

#[tokio::test]
async fn rate_limit_and_server_errors_map() {
    let base = serve(Script {
        responses: Arc::new(vec![
            (
                StatusCode::TOO_MANY_REQUESTS,
                serde_json::json!({"error": "slow down"}),
            ),
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({"error": "boom"}),
            ),
        ]),
        hits: Arc::new(AtomicUsize::new(0)),
        require_bearer: None,
    })
    .await;
    let client = ChatClient::new(base, None, "m");
    assert!(matches!(
        client.chat(&[ChatMessage::user("x")], &[]).await,
        Err(LlmError::RateLimited(_))
    ));
    assert!(matches!(
        client.chat(&[ChatMessage::user("x")], &[]).await,
        Err(LlmError::Server { status: 500, .. })
    ));
}
