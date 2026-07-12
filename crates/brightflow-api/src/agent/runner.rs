//! The agent loop: context → tools → propose actions → feed results back.

use axum::extract::{Path as AxumPath, State};
use axum::Json;
use brightflow_llm::{ChatClient, ChatMessage, ToolDef};
use serde_json::json;

use crate::actions::handlers::{dispatch_action, Actor};
use crate::actions::types::Action;
use crate::shared::{AppError, AppResult};
use crate::state::AppState;

/// Hard iteration cap — a runaway loop stops here.
const MAX_ITERATIONS: usize = 12;
/// Token budget guard (provider-reported totals).
const MAX_TOTAL_TOKENS: u64 = 120_000;
/// Top insight candidates fed to triage/narrate runs.
const TRIAGE_CANDIDATES: usize = 12;

fn now_epoch() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    )
    .unwrap_or(0)
}

/// Execute one agent run to completion. Called from a spawned task; the
/// caller owns run-row lifecycle on abort.
pub async fn execute_run(
    state: AppState,
    run_id: i64,
    kind: String,
    source_id: String,
    table: String,
) {
    let outcome = run_inner(&state, run_id, &kind, &source_id, &table).await;
    let Some(store) = state.store() else { return };
    let (status, detail) = match outcome {
        Ok(summary) => ("completed", summary),
        Err(e) => ("failed", format!("{e}")),
    };
    if let Err(e) = store
        .db()
        .finish_agent_run(run_id, status, Some(&detail), now_epoch())
        .await
    {
        tracing::warn!("failed to finish agent run {run_id}: {e}");
    }
    state.agent_runs.remove(&run_id);
}

async fn run_inner(
    state: &AppState,
    run_id: i64,
    kind: &str,
    source_id: &str,
    table: &str,
) -> AppResult<String> {
    let client = crate::llm::default_client(state).await?;
    let (system, context) = build_context(state, kind, source_id, table).await?;
    let tools = tools_for(kind);

    let mut messages = vec![ChatMessage::system(system), ChatMessage::user(context)];
    let mut proposed = 0usize;
    let mut total_tokens = 0u64;
    let mut narration = String::new();

    for iteration in 0..MAX_ITERATIONS {
        let outcome = chat_with_retry(&client, &messages, &tools).await?;
        total_tokens += outcome.total_tokens.unwrap_or(0);

        let calls = outcome.tool_calls().to_vec();
        if calls.is_empty() {
            // Plain text turn: narration (or the model being chatty). Done.
            narration = outcome.text().to_string();
            break;
        }
        messages.push(outcome.message.clone());

        let mut finished = false;
        for call in calls {
            if call.function.name == "done" {
                finished = true;
                messages.push(ChatMessage::tool_result(call.id.clone(), "acknowledged"));
                continue;
            }
            // Reconstruct a full Action from the tool call: the tool name is
            // the action kind; scope fields are injected server-side so the
            // model can't wander across tables.
            let reply = match parse_action(
                &call.function.name,
                &call.function.arguments,
                source_id,
                table,
            ) {
                Ok(action) => {
                    let request_id = format!("agent-{run_id}-{iteration}-{}", call.id);
                    match dispatch_action(state, action, &request_id, Actor::Agent { run_id }).await
                    {
                        Ok(response) => {
                            proposed += 1;
                            json!({
                                "status": "proposal_recorded",
                                "logId": response.log_id,
                                "note": "awaiting human approval"
                            })
                            .to_string()
                        },
                        Err(e) => json!({ "status": "error", "error": e.to_string() }).to_string(),
                    }
                },
                // Malformed arguments: report back, never partially apply.
                Err(e) => json!({ "status": "invalid_arguments", "error": e }).to_string(),
            };
            messages.push(ChatMessage::tool_result(call.id.clone(), reply));
        }
        if finished {
            break;
        }
        if total_tokens > MAX_TOTAL_TOKENS {
            tracing::warn!("agent run {run_id} hit token budget");
            break;
        }
    }

    let mut summary = format!("{proposed} proposals");
    if !narration.is_empty() {
        summary = format!("{summary}\n{narration}");
    }
    Ok(summary)
}

/// One retry on rate-limit/server errors, then give up.
async fn chat_with_retry(
    client: &ChatClient,
    messages: &[ChatMessage],
    tools: &[ToolDef],
) -> AppResult<brightflow_llm::ChatOutcome> {
    match client.chat(messages, tools).await {
        Ok(outcome) => Ok(outcome),
        Err(brightflow_llm::LlmError::RateLimited(_) | brightflow_llm::LlmError::Server { .. }) => {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            client
                .chat(messages, tools)
                .await
                .map_err(|e| AppError::Internal(format!("llm: {e}")))
        },
        Err(e) => Err(AppError::Internal(format!("llm: {e}"))),
    }
}

/// Parse tool-call arguments into a full Action, injecting the scope.
fn parse_action(
    tool_name: &str,
    arguments: &str,
    source_id: &str,
    table: &str,
) -> Result<Action, String> {
    let mut args: serde_json::Value =
        serde_json::from_str(arguments).map_err(|e| format!("arguments not valid JSON: {e}"))?;
    let obj = args
        .as_object_mut()
        .ok_or_else(|| "arguments must be a JSON object".to_string())?;
    obj.insert("kind".to_string(), json!(tool_name));
    obj.insert("source_id".to_string(), json!(source_id));
    obj.insert("table".to_string(), json!(table));
    serde_json::from_value(args).map_err(|e| format!("invalid action parameters: {e}"))
}

/// Manifest slice + `done` tool per run kind.
fn tools_for(kind: &str) -> Vec<ToolDef> {
    let action_kinds: &[&str] = match kind {
        "auto_label" => &["assign_cluster_label", "rename_cluster"],
        "propose_merges" => &["merge_clusters", "mark_cluster_noise"],
        "triage_insights" => &["dismiss_insight", "pin_insight", "annotate_insight"],
        _ => &[], // narrate_insights: text only
    };
    let manifest = manifest_schemas();
    let mut tools: Vec<ToolDef> = action_kinds
        .iter()
        .filter_map(|k| {
            manifest
                .iter()
                .find(|(mk, _, _)| mk == k)
                .map(|(mk, desc, schema)| ToolDef {
                    name: mk.clone(),
                    description: desc.clone(),
                    parameters: schema.clone(),
                })
        })
        .collect();
    tools.push(ToolDef {
        name: "done".to_string(),
        description: "Call when you have proposed everything worth proposing.".to_string(),
        parameters: json!({ "type": "object", "properties": {} }),
    });
    tools
}

/// (kind, description, parameter schema) triples from the action manifest,
/// with the server-injected scope fields stripped from the schema.
fn manifest_schemas() -> Vec<(String, String, serde_json::Value)> {
    let root = schemars::schema_for!(Action);
    let root_value = serde_json::to_value(&root).unwrap_or(serde_json::Value::Null);
    let variants = root_value
        .get("oneOf")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    crate::actions::types::ACTION_KINDS
        .iter()
        .filter_map(|(kind, description, _)| {
            let mut schema = variants
                .iter()
                .find(|v| {
                    v.pointer("/properties/kind/const")
                        .and_then(|c| c.as_str())
                        .is_some_and(|c| c == *kind)
                })?
                .clone();
            // The model never supplies scope/tag fields — the server injects them.
            if let Some(props) = schema
                .pointer_mut("/properties")
                .and_then(|p| p.as_object_mut())
            {
                props.remove("kind");
                props.remove("source_id");
                props.remove("table");
            }
            if let Some(required) = schema
                .pointer_mut("/required")
                .and_then(|r| r.as_array_mut())
            {
                required.retain(|v| !matches!(v.as_str(), Some("kind" | "source_id" | "table")));
            }
            Some(((*kind).to_string(), (*description).to_string(), schema))
        })
        .collect()
}

/// Kind-specific context: what the model sees.
async fn build_context(
    state: &AppState,
    kind: &str,
    source_id: &str,
    table: &str,
) -> AppResult<(String, String)> {
    match kind {
        "auto_label" | "propose_merges" => {
            let overview = crate::topics::handlers::get_overview(
                State(state.clone()),
                AxumPath((source_id.to_string(), table.to_string())),
            )
            .await?
            .0;
            if !overview.ready {
                return Err(AppError::BadRequest(
                    "no fitted clusters — run recluster first".to_string(),
                ));
            }
            let clusters: Vec<serde_json::Value> = overview
                .clusters
                .iter()
                .map(|c| {
                    json!({
                        "clusterId": c.id,
                        "size": c.size,
                        "topTerms": c.top_terms,
                        "sampleTitles": c.sample_titles,
                    })
                })
                .collect();
            let system = if kind == "auto_label" {
                "You are a topic curator. For each cluster, propose a short, specific \
                 label (2-4 words) via assign_cluster_label, and where the auto-generated \
                 name is unreadable, a display name via rename_cluster. Base proposals \
                 ONLY on the provided terms and sample titles. Call done when finished."
            } else {
                "You are a topic curator. Identify clusters that describe the SAME topic \
                 and propose merge_clusters (fold the smaller into the larger). Propose \
                 mark_cluster_noise for clusters that are clearly spam or formatting \
                 artifacts. Be conservative — only propose merges you are confident in. \
                 Call done when finished."
            };
            Ok((
                system.to_string(),
                serde_json::to_string_pretty(&json!({ "clusters": clusters })).unwrap_or_default(),
            ))
        },
        "triage_insights" | "narrate_insights" => {
            let response = crate::insights::handlers::run_trends(
                State(state.clone()),
                Json(crate::insights::types::TrendsRequest {
                    source_id: source_id.to_string(),
                    dataset_id: table.to_string(),
                    config: crate::insights::types::EngineConfig::default(),
                }),
            )
            .await?
            .0;
            let candidates: Vec<serde_json::Value> = response
                .tree
                .roots
                .iter()
                .take(TRIAGE_CANDIDATES)
                .filter_map(|id| response.tree.nodes.get(id.0))
                .map(|n| {
                    json!({
                        "fingerprint": n.fingerprint,
                        "summary": n.summary,
                        "why": n.why,
                        "score": n.significance,
                    })
                })
                .collect();
            let system = if kind == "triage_insights" {
                "You are an analytics triage assistant. Review the insights: propose \
                 dismiss_insight for findings that are boring, already known, or likely \
                 wrong (pick the reason), pin_insight for genuinely important ones, and \
                 annotate_insight where a short note adds context. Call done when finished."
            } else {
                "You are an analytics narrator. Write a concise plain-language summary \
                 (3-6 sentences) of what these insights say about the data. Respond with \
                 text only — no tool calls."
            };
            Ok((
                system.to_string(),
                serde_json::to_string_pretty(&json!({ "insights": candidates }))
                    .unwrap_or_default(),
            ))
        },
        other => Err(AppError::BadRequest(format!(
            "unknown agent kind '{other}'"
        ))),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_action_injects_scope() {
        let action = parse_action(
            "rename_cluster",
            r#"{"cluster_id": 3, "name": "Payments"}"#,
            "src-1",
            "issues",
        )
        .unwrap();
        assert_eq!(action.kind(), "rename_cluster");
        assert_eq!(action.scope(), ("src-1", "issues"));
    }

    #[test]
    fn parse_action_rejects_malformed_arguments() {
        // Not JSON
        assert!(parse_action("rename_cluster", "not json", "s", "t").is_err());
        // Wrong types
        assert!(parse_action(
            "rename_cluster",
            r#"{"cluster_id": "three", "name": 5}"#,
            "s",
            "t"
        )
        .is_err());
        // Unknown tool name
        assert!(parse_action("drop_all_tables", "{}", "s", "t").is_err());
    }

    #[test]
    fn parse_action_ignores_model_supplied_scope() {
        // Even if the model tries to smuggle a different table, the server
        // injection wins.
        let action = parse_action(
            "dismiss_insight",
            r#"{"fingerprint": "abc", "reason": "boring", "table": "other", "source_id": "evil"}"#,
            "src-1",
            "posts",
        )
        .unwrap();
        assert_eq!(action.scope(), ("src-1", "posts"));
    }

    #[test]
    fn tools_include_done_and_strip_scope_fields() {
        let tools = tools_for("auto_label");
        assert!(tools.iter().any(|t| t.name == "done"));
        let label_tool = tools
            .iter()
            .find(|t| t.name == "assign_cluster_label")
            .expect("assign_cluster_label tool");
        let props = label_tool.parameters.pointer("/properties").unwrap();
        assert!(props.get("source_id").is_none(), "scope is server-injected");
        assert!(props.get("kind").is_none());
        assert!(props.get("label").is_some());
    }
}
