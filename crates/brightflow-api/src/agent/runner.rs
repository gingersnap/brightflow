//! The agent loop: context → tools → propose actions → feed results back.

use axum::extract::{Path as AxumPath, State};
use axum::Json;
use brightflow_llm::{ChatClient, ChatMessage, ToolDef};
use serde_json::json;

use crate::actions::handlers::{dispatch_action, Actor};
use crate::actions::types::{Action, ActionStatus};
use crate::agent::sampling::SampleDoc;
use crate::shared::{AppError, AppResult};
use crate::state::AppState;

/// Hard iteration cap — a runaway loop stops here.
const MAX_ITERATIONS: usize = 12;
/// Token budget guard (provider-reported totals).
const MAX_TOTAL_TOKENS: u64 = 120_000;
/// Top insight candidates fed to triage/narrate runs.
const TRIAGE_CANDIDATES: usize = 12;

/// Iteration cap for a run kind.
///
/// `label_documents` works through a ~1–2k row seed a batch at a time, so the
/// cluster-sized default of 12 would stop it a few percent in. This is still
/// the bounded cold path: cost is O(seed sample), never O(rows).
fn max_iterations(kind: &str) -> usize {
    match kind {
        "label_documents" => 80,
        _ => MAX_ITERATIONS,
    }
}

/// Token budget for a run kind.
fn max_total_tokens(kind: &str) -> u64 {
    match kind {
        "label_documents" => 900_000,
        _ => MAX_TOTAL_TOKENS,
    }
}

/// Execute one agent run to completion. Called from a spawned task; the
/// caller owns run-row lifecycle on abort.
pub async fn execute_run(
    state: AppState,
    run_id: i64,
    kind: String,
    source_id: String,
    table: String,
    auto_apply: bool,
    parent_id: Option<i64>,
) {
    let outcome = run_inner(
        &state, run_id, &kind, &source_id, &table, auto_apply, parent_id,
    )
    .await;
    let Some(store) = state.store() else { return };
    let (status, detail) = match outcome {
        Ok(summary) => ("completed", summary),
        Err(e) => ("failed", format!("{e}")),
    };
    if let Err(e) = store
        .db()
        .finish_agent_run(
            run_id,
            status,
            Some(&detail),
            chrono::Utc::now().timestamp(),
        )
        .await
    {
        tracing::warn!("failed to finish agent run {run_id}: {e}");
    }
    state.agent_runs.remove(&run_id);
    crate::actions::events::emit_run(&state, run_id).await;
}

async fn run_inner(
    state: &AppState,
    run_id: i64,
    kind: &str,
    source_id: &str,
    table: &str,
    auto_apply: bool,
    parent_id: Option<i64>,
) -> AppResult<String> {
    let client = crate::llm::default_client(state).await?;
    let (system, context) = build_context(state, kind, source_id, table, parent_id).await?;
    let tools = tools_for(kind);
    let defaults = vocab_defaults(kind, parent_id);

    let mut messages = vec![ChatMessage::system(system), ChatMessage::user(context)];
    let mut applied = 0usize;
    let mut proposed = 0usize;
    let mut failed = 0usize;
    let mut total_tokens = 0u64;
    let mut narration = String::new();
    let (iteration_cap, token_cap) = (max_iterations(kind), max_total_tokens(kind));

    for iteration in 0..iteration_cap {
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
            )
            .map(|action| apply_vocab_defaults(action, defaults))
            {
                Ok(action) => {
                    let request_id = format!("agent-{run_id}-{iteration}-{}", call.id);
                    let actor = Actor::Agent { run_id, auto_apply };
                    match dispatch_action(state, action, &request_id, actor).await {
                        // Real execution feedback matters: a label_documents
                        // call naming a bad category now fails immediately and
                        // the model can correct course instead of queueing
                        // doomed proposals.
                        Ok(response) => match response.status {
                            ActionStatus::Applied => {
                                applied += 1;
                                json!({ "status": "applied", "logId": response.log_id }).to_string()
                            },
                            ActionStatus::Failed => {
                                failed += 1;
                                let error = response
                                    .result
                                    .get("error")
                                    .and_then(|e| e.as_str())
                                    .unwrap_or("action failed");
                                json!({
                                    "status": "failed",
                                    "logId": response.log_id,
                                    "error": error
                                })
                                .to_string()
                            },
                            _ => {
                                proposed += 1;
                                json!({
                                    "status": "proposal_recorded",
                                    "logId": response.log_id,
                                    "note": "awaiting human approval"
                                })
                                .to_string()
                            },
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
        if total_tokens > token_cap {
            tracing::warn!("agent run {run_id} hit token budget ({token_cap})");
            break;
        }
    }

    let mut parts = Vec::new();
    if applied > 0 {
        parts.push(format!("{applied} applied"));
    }
    if proposed > 0 {
        parts.push(format!("{proposed} proposals"));
    }
    if failed > 0 {
        parts.push(format!("{failed} failed"));
    }
    let mut summary = if parts.is_empty() {
        // Keep the old zero-case wording for propose-mode runs.
        if auto_apply {
            "0 actions".to_string()
        } else {
            "0 proposals".to_string()
        }
    } else {
        parts.join(", ")
    };
    if !narration.is_empty() {
        summary = format!("{summary}\n{narration}");
    }
    Ok(summary)
}

/// One retry on transient errors, ~5s apart (plus jitter), then give up.
async fn chat_with_retry(
    client: &ChatClient,
    messages: &[ChatMessage],
    tools: &[ToolDef],
) -> AppResult<brightflow_llm::ChatOutcome> {
    brightflow_llm::chat_with_backoff(
        client,
        messages,
        tools,
        &brightflow_llm::ChatOptions::default(),
        &brightflow_llm::RetryPolicy {
            max_attempts: 2,
            base_delay_ms: 5000,
        },
    )
    .await
    .map_err(|e| AppError::Internal(format!("llm: {e}")))
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

/// The vocabulary level an induction run defines into: (kind, parent). The
/// model never chooses these — they are pinned server-side so a
/// `propose_subcategories` run cannot wander into another parent or level.
fn vocab_defaults(kind: &str, parent_id: Option<i64>) -> Option<(&'static str, i64)> {
    match kind {
        "propose_categories" | "propose_taxonomy" => Some(("category", 0)),
        "propose_subcategories" => Some(("subcategory", parent_id.unwrap_or(0))),
        "propose_feedback_categories" => Some(("feedback_category", 0)),
        _ => None,
    }
}

/// Pin `vocab_kind`/`parent_id` on a define action per `vocab_defaults`.
fn apply_vocab_defaults(action: Action, defaults: Option<(&'static str, i64)>) -> Action {
    match (action, defaults) {
        (
            Action::DefineTaxonomyCategory {
                scope,
                name,
                description,
                ..
            },
            Some((kind, parent)),
        ) => Action::DefineTaxonomyCategory {
            scope,
            name,
            description,
            vocab_kind: Some(kind.to_string()),
            parent_id: Some(parent),
            aliases: None,
        },
        (action, _) => action,
    }
}

/// Manifest slice + `done` tool per run kind.
fn tools_for(kind: &str) -> Vec<ToolDef> {
    let action_kinds: &[&str] = match kind {
        "auto_label" => &["assign_cluster_label", "rename_cluster"],
        "propose_merges" => &["merge_clusters", "mark_cluster_noise"],
        "triage_insights" => &["dismiss_insight", "pin_insight", "annotate_insight"],
        "propose_taxonomy"
        | "propose_categories"
        | "propose_subcategories"
        | "propose_feedback_categories" => &["define_taxonomy_category"],
        "label_documents" => &["label_document"],
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
    // Positional destructure of (kind, label, description, undoable): the
    // LLM gets the long description, never the short UI label.
    crate::actions::types::ACTION_KINDS
        .iter()
        .filter_map(|(kind, _label, description, _)| {
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

/// The table's approved intent taxonomy, as prompt JSON.
async fn existing_taxonomy(
    state: &AppState,
    source_id: &str,
    table: &str,
) -> AppResult<Vec<serde_json::Value>> {
    existing_vocabulary(state, source_id, table, None).await
}

/// The current entries the model is shown, optionally one level only, so
/// an induction run proposes a diff against the list rather than a fresh
/// list.
async fn existing_vocabulary(
    state: &AppState,
    source_id: &str,
    table: &str,
    level: Option<(&str, i64)>,
) -> AppResult<Vec<serde_json::Value>> {
    let store = state.require_store()?;
    let Some(table_row) = store.db().get_table(source_id, table).await? else {
        return Ok(Vec::new());
    };
    let categories = store.db().get_taxonomy_categories(&table_row.id).await?;
    Ok(categories
        .into_iter()
        .filter(|c| level.is_none_or(|(kind, parent)| c.kind == kind && c.parent_id == parent))
        .map(|c| {
            json!({
                "id": c.id,
                "kind": c.kind,
                "parentId": c.parent_id,
                "name": c.name,
                "description": c.description,
            })
        })
        .collect())
}

/// Fewest classified summaries an induction run will accept. Inducing a
/// vocabulary from less produces a guessed list, which the design forbids.
const MIN_SUMMARIES_FOR_INDUCTION: usize = 50;

/// Shared instruction for the three summary-driven induction runs.
fn induction_system(level: &str, corpus: &str, cap: usize) -> String {
    format!(
        "You are defining {level} for a customer-insight vocabulary. Read the sample of \
         {corpus} and propose entries via define_taxonomy_category, then call done.\n\n\
         Propose AT MOST {cap} entries in total, existing ones included — if the corpus \
         needs more, group: a broader entry that covers several themes beats a longer \
         list. Do not propose 'other'; it exists implicitly. Do not re-propose an existing \
         entry unless its definition should change.\n\n\
         Categorize by WHAT IS WRONG or WHAT IS WANTED for the user, never by tooling, \
         file format, mechanism, or how the text is written. If an entry would still make \
         sense after someone rewrote the text in different words, it is real; if it would \
         evaporate, it is a format bucket — discard it.\n\n\
         Entries should be mutually distinguishable, cover the corpus, and be specific \
         enough to act on. Give each a short name and a one-sentence definition — the \
         definition is what a classifier will judge against, so make it precise."
    )
}

/// Kind-specific context: what the model sees.
async fn build_context(
    state: &AppState,
    kind: &str,
    source_id: &str,
    table: &str,
    parent_id: Option<i64>,
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
        "propose_taxonomy" => {
            let docs = crate::agent::sampling::stratified_sample(
                state,
                source_id,
                table,
                crate::agent::sampling::TAXONOMY_SAMPLE,
            )
            .await?;
            if docs.is_empty() {
                return Err(AppError::BadRequest(
                    "no documents to sample — sync the table first".to_string(),
                ));
            }
            let existing = existing_taxonomy(state, source_id, table).await?;
            let samples: Vec<serde_json::Value> = docs.iter().map(SampleDoc::to_json).collect();

            // The negative instruction is the entire point of this run. Left to
            // itself the model reliably proposes format buckets ("Automated
            // Backport Commits") — which are *accurate* descriptions of what it
            // was shown and *useless* as intents.
            let system = "You are defining a problem-intent taxonomy for a support/issue \
                 corpus. Read the sample tickets and propose 8-15 categories via \
                 define_taxonomy_category, then call done.\n\n\
                 Categorize by WHAT IS WRONG for the user — the symptom or failure. \
                 Good: 'authentication failure', 'data loss on sync', 'slow query \
                 performance', 'incorrect billing amount'.\n\n\
                 NEVER categorize by tooling, file format, mechanism, or how the ticket \
                 is written. Bad: 'backport commits', 'stack traces', 'issues with code \
                 snippets', 'templated bug reports', 'force push'. Those describe the \
                 SHAPE of the text, not the problem. If a proposed category would still \
                 make sense after someone rewrote the ticket in different words, it is a \
                 real intent; if it would evaporate, it is a format bucket — discard it.\n\n\
                 Categories should be mutually distinguishable, cover the corpus, and be \
                 specific enough to act on. Give each a short name and a one-sentence \
                 description of the symptom.";

            Ok((
                system.to_string(),
                serde_json::to_string_pretty(&json!({
                    "existingCategories": existing,
                    "sampleTickets": samples,
                }))
                .unwrap_or_default(),
            ))
        },
        "label_documents" => {
            let existing = existing_taxonomy(state, source_id, table).await?;
            if existing.is_empty() {
                return Err(AppError::BadRequest(
                    "no taxonomy defined — run propose_taxonomy and approve categories first"
                        .to_string(),
                ));
            }
            let docs = crate::agent::sampling::stratified_sample(
                state,
                source_id,
                table,
                crate::agent::sampling::LABEL_SAMPLE,
            )
            .await?;
            if docs.is_empty() {
                return Err(AppError::BadRequest(
                    "no documents to sample — sync the table first".to_string(),
                ));
            }

            // Batch: one label_document call per row, but many rows per model
            // turn. One row per turn would burn the iteration budget on protocol
            // overhead long before the seed sample was covered.
            let batches: Vec<serde_json::Value> = docs
                .chunks(crate::agent::sampling::LABEL_BATCH)
                .map(|chunk| json!(chunk.iter().map(SampleDoc::to_json).collect::<Vec<_>>()))
                .collect();

            let system = "You are labelling support tickets with problem intents. For EVERY \
                 ticket shown, call label_document with its rowId and the categories that \
                 apply, drawn ONLY from the approved taxonomy below. Use the exact category \
                 names given.\n\n\
                 Judge by WHAT PROBLEM the ticket describes, not by how it is written. \
                 Ignore whether it contains a stack trace, a backport script, a template, \
                 or code — formatting is not intent. Two tickets in totally different \
                 formats can share an intent; two tickets in identical formats often do \
                 not.\n\n\
                 A ticket may have several intents, or none — pass an empty list rather \
                 than forcing a bad fit. Work through the batches in order, calling \
                 label_document once per ticket. Call done only after every ticket has \
                 been labelled.";

            Ok((
                system.to_string(),
                serde_json::to_string_pretty(&json!({
                    "approvedTaxonomy": existing,
                    "batches": batches,
                }))
                .unwrap_or_default(),
            ))
        },
        "propose_categories" => {
            let docs = crate::agent::sampling::summary_sample(
                state,
                source_id,
                table,
                crate::agent::sampling::INDUCTION_SAMPLE,
                None,
            )
            .await?;
            require_summaries(docs.len(), "classified tickets")?;
            let existing =
                existing_vocabulary(state, source_id, table, Some(("category", 0))).await?;
            Ok((
                induction_system(
                    "the root CATEGORIES (why people contact support)",
                    "one-line ticket summaries",
                    brightflow_engine::enrichment::INDUCED_CAP,
                ),
                serde_json::to_string_pretty(&json!({
                    "existingEntries": existing,
                    "summaries": docs,
                }))
                .unwrap_or_default(),
            ))
        },
        "propose_subcategories" => {
            let parent_id = parent_id.ok_or_else(|| {
                AppError::BadRequest("propose_subcategories needs parent_id".to_string())
            })?;
            let store = state.require_store()?;
            let parent = store
                .db()
                .get_taxonomy_category(parent_id)
                .await?
                .filter(|p| p.kind == "category")
                .ok_or_else(|| AppError::NotFound(format!("category {parent_id} not found")))?;
            let docs = crate::agent::sampling::summary_sample(
                state,
                source_id,
                table,
                crate::agent::sampling::INDUCTION_SAMPLE,
                Some(&parent.name),
            )
            .await?;
            require_summaries(
                docs.len(),
                &format!("tickets classified as '{}'", parent.name),
            )?;
            let existing =
                existing_vocabulary(state, source_id, table, Some(("subcategory", parent_id)))
                    .await?;
            Ok((
                induction_system(
                    &format!(
                        "the SUBCATEGORIES of the category '{}' ({})",
                        parent.name,
                        parent.description.as_deref().unwrap_or("no definition")
                    ),
                    "one-line ticket summaries, all already classified under that category",
                    brightflow_engine::enrichment::INDUCED_CAP,
                ),
                serde_json::to_string_pretty(&json!({
                    "parent": { "id": parent.id, "name": parent.name, "description": parent.description },
                    "existingEntries": existing,
                    "summaries": docs,
                }))
                .unwrap_or_default(),
            ))
        },
        "propose_feedback_categories" => {
            let docs = crate::agent::sampling::feedback_summary_sample(
                state,
                source_id,
                table,
                crate::agent::sampling::INDUCTION_SAMPLE,
            )
            .await?;
            require_summaries(docs.len(), "extracted feedback mentions")?;
            let existing =
                existing_vocabulary(state, source_id, table, Some(("feedback_category", 0)))
                    .await?;
            Ok((
                induction_system(
                    "the FEEDBACK CATEGORIES (what people think of the product: friction, \
                     wants, praise)",
                    "short feedback summaries extracted from tickets",
                    brightflow_engine::enrichment::INDUCED_CAP,
                ),
                serde_json::to_string_pretty(&json!({
                    "existingEntries": existing,
                    "summaries": docs,
                }))
                .unwrap_or_default(),
            ))
        },
        other => Err(AppError::BadRequest(format!(
            "unknown agent kind '{other}'"
        ))),
    }
}

fn require_summaries(found: usize, what: &str) -> AppResult<()> {
    if found < MIN_SUMMARIES_FOR_INDUCTION {
        return Err(AppError::BadRequest(format!(
            "only {found} {what} have a summary — at least {MIN_SUMMARIES_FOR_INDUCTION} are \
             needed to induce a vocabulary rather than guess one"
        )));
    }
    Ok(())
}

#[cfg(test)]
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

    /// The tier invariant behind auto-apply-by-default: every tool the runner
    /// hands out (minus `done`) must be an undoable action kind. If a future
    /// run kind gets an irreversible tool, auto-apply silently degrades that
    /// tool to propose — which is safe, but should be a deliberate choice,
    /// not an accident this test lets slip through.
    #[test]
    fn every_agent_tool_maps_to_an_undoable_kind() {
        let run_kinds = [
            "auto_label",
            "propose_merges",
            "narrate_insights",
            "triage_insights",
            "propose_taxonomy",
            "label_documents",
        ];
        for run_kind in run_kinds {
            for tool in tools_for(run_kind) {
                if tool.name == "done" {
                    continue;
                }
                assert!(
                    crate::actions::types::kind_is_undoable(&tool.name),
                    "agent run kind '{run_kind}' hands out irreversible tool '{}'",
                    tool.name
                );
            }
        }
    }

    /// An induction run's defines land on the pinned level regardless of
    /// what the model wrote; other kinds pass through untouched.
    #[test]
    fn vocab_defaults_pin_kind_and_parent_on_defines() {
        let action = parse_action(
            "define_taxonomy_category",
            r#"{"name": "vat", "vocab_kind": "category", "parent_id": 99}"#,
            "src-1",
            "issues",
        )
        .unwrap();
        let pinned = apply_vocab_defaults(action, vocab_defaults("propose_subcategories", Some(7)));
        match pinned {
            Action::DefineTaxonomyCategory {
                vocab_kind,
                parent_id,
                ..
            } => {
                assert_eq!(vocab_kind.as_deref(), Some("subcategory"));
                assert_eq!(parent_id, Some(7));
            },
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(
            vocab_defaults("propose_feedback_categories", None),
            Some(("feedback_category", 0))
        );
        assert_eq!(vocab_defaults("auto_label", None), None);
        for kind in [
            "propose_categories",
            "propose_subcategories",
            "propose_feedback_categories",
        ] {
            let names: Vec<String> = tools_for(kind).into_iter().map(|t| t.name).collect();
            assert_eq!(names, vec!["define_taxonomy_category", "done"]);
        }
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
