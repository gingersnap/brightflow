//! Async LLM batch runner + column materialization for the two ticket
//! functions.
//!
//! The engine owns the pure pieces (render/hash/schema/validate); this module
//! owns the async loop, the cache, and the parquet rewrite. Both kinds share
//! one cell shape — `(input_hash → value_json)` under a spec hash — and differ
//! only in how a row becomes messages and how a value becomes columns, which
//! is what [`RunSpec`] dispatches on.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::StreamExt;
use polars::prelude::*;

use brightflow_engine::enrichment::mentions::{self, ExtractCell, SubjectResolver, FLAG_COLUMNS};
use brightflow_engine::enrichment::ticket_classify::{
    self, ClassifyCell, VocabNames, LANGUAGE_INPUT, OUTPUT_COLUMNS, RETIRED_COLUMNS,
};
use brightflow_engine::enrichment::{
    input_hash, ticket_classify_hash, ticket_extract_hash, FunctionSpec, TicketClassifySpec,
    TicketExtractSpec,
};
use brightflow_engine::nlp::detect_language;
use brightflow_llm::{
    chat_with_backoff, ChatClient, ChatMessage, ChatOptions, ChatOutcome, RetryPolicy, ToolDef,
};
use brightflow_store::ParquetStore;

use crate::shared::{AppError, AppResult};
use crate::state::AppState;

/// Concurrent in-flight LLM calls per run.
const CONCURRENCY: usize = 4;
/// Progress-row update cadence.
const PROGRESS_EVERY: Duration = Duration::from_secs(2);

/// Sampling temperature for every enrichment call. Part of the fingerprint.
const TEMPERATURE: f64 = 0.0;

/// Fingerprint for `ticket_classify`: the engine's system prompt rendered
/// with no names (every entry shows as `#id`), so the hash tracks the prompt
/// wording and the definitions but never a display name.
fn classify_fingerprint(spec: &TicketClassifySpec) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(ticket_classify::system_prompt(spec, &VocabNames::new()).as_bytes());
    hasher.update(&TEMPERATURE.to_le_bytes());
    hasher.finalize().to_hex().to_string()
}

/// Cache key for a `ticket_classify` spec.
pub fn classify_spec_hash(spec: &TicketClassifySpec) -> String {
    ticket_classify_hash(spec, &classify_fingerprint(spec))
}

/// Fingerprint for `ticket_extract`, same construction as the classifier's.
fn extract_fingerprint(spec: &TicketExtractSpec) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(mentions::system_prompt(spec, &VocabNames::new()).as_bytes());
    hasher.update(&TEMPERATURE.to_le_bytes());
    hasher.finalize().to_hex().to_string()
}

/// Cache key for a `ticket_extract` spec.
pub fn extract_spec_hash(spec: &TicketExtractSpec) -> String {
    ticket_extract_hash(spec, &extract_fingerprint(spec))
}

/// The cache key of any spec.
pub fn spec_hash_for(spec: &FunctionSpec) -> Option<String> {
    match spec {
        FunctionSpec::TicketClassify(tc) => Some(classify_spec_hash(tc)),
        FunctionSpec::TicketExtract(te) => Some(extract_spec_hash(te)),
    }
}

/// Materialised output columns on the *parent* table of any runnable spec.
///
/// Excludes the `{fn}__status` column; empty for kinds this runner does not
/// drive. The extractor's child table is not listed here.
pub fn output_columns_for(spec: &FunctionSpec) -> Vec<String> {
    match spec {
        FunctionSpec::TicketClassify(_) => {
            OUTPUT_COLUMNS.iter().map(|c| (*c).to_string()).collect()
        },
        FunctionSpec::TicketExtract(_) => FLAG_COLUMNS.iter().map(|c| (*c).to_string()).collect(),
    }
}

/// A runnable per-row function: what the runner needs beyond the stored spec.
#[derive(Debug, Clone)]
pub enum RunSpec {
    TicketClassify {
        spec: TicketClassifySpec,
        /// Live id → name lookup, loaded when the run starts. Names reach the
        /// prompt and the materialised columns, never the cache key.
        names: Arc<VocabNames>,
    },
    TicketExtract {
        spec: TicketExtractSpec,
        names: Arc<VocabNames>,
        /// Name/alias → id for products and competitors.
        resolver: Arc<SubjectResolver>,
    },
}

impl RunSpec {
    pub fn spec_hash(&self) -> String {
        match self {
            Self::TicketClassify { spec, .. } => classify_spec_hash(spec),
            Self::TicketExtract { spec, .. } => extract_spec_hash(spec),
        }
    }

    pub fn provider(&self) -> (&str, Option<&str>) {
        match self {
            Self::TicketClassify { spec, .. } => (&spec.provider_id, spec.model.as_deref()),
            Self::TicketExtract { spec, .. } => (&spec.provider_id, spec.model.as_deref()),
        }
    }

    /// (text columns, language column).
    fn text_inputs(&self) -> (&[String], Option<&str>) {
        match self {
            Self::TicketClassify { spec, .. } => {
                (&spec.text_columns, spec.language_column.as_deref())
            },
            Self::TicketExtract { spec, .. } => {
                (&spec.text_columns, spec.language_column.as_deref())
            },
        }
    }

    /// Table columns rendered into each row's inputs.
    fn input_columns(&self) -> Vec<String> {
        let (text_columns, language_column) = self.text_inputs();
        let mut cols = text_columns.to_vec();
        if let Some(lang) = language_column {
            if !cols.iter().any(|c| c == lang) {
                cols.push(lang.to_string());
            }
        }
        cols
    }

    /// Derived inputs added after rendering. Deterministic functions of the
    /// rendered values only, so they are safe inside the input hash.
    fn augment(&self, rendered: &mut BTreeMap<String, String>) {
        let (text_columns, language_column) = self.text_inputs();
        let language = if let Some(col) = language_column {
            rendered
                .get(col)
                .and_then(|v| normalize_lang(v))
                .unwrap_or_default()
        } else {
            {
                let text: Vec<&str> = text_columns
                    .iter()
                    .filter_map(|c| rendered.get(c).map(String::as_str))
                    .collect();
                detect_language(&text.join("\n"))
                    .unwrap_or_default()
                    .to_string()
            }
        };
        rendered.insert(LANGUAGE_INPUT.to_string(), language);
    }

    /// The per-row user turn for the two ticket kinds.
    fn ticket_turn(&self, row: &RowInput) -> ChatMessage {
        let (text_columns, _) = self.text_inputs();
        let language = row
            .rendered
            .get(LANGUAGE_INPUT)
            .map(String::as_str)
            .filter(|l| !l.is_empty());
        ChatMessage::user(ticket_classify::user_prompt(
            text_columns,
            &row.rendered,
            language,
        ))
    }

    fn messages(&self, row: &RowInput) -> Vec<ChatMessage> {
        match self {
            Self::TicketClassify { spec, names } => vec![
                ChatMessage::system(ticket_classify::system_prompt(spec, names)),
                self.ticket_turn(row),
            ],
            Self::TicketExtract { spec, names, .. } => vec![
                ChatMessage::system(mentions::system_prompt(spec, names)),
                self.ticket_turn(row),
            ],
        }
    }

    fn tool(&self) -> ToolDef {
        match self {
            Self::TicketClassify { spec, names } => ToolDef {
                name: ticket_classify::TOOL_NAME.to_string(),
                description: "Record the classification of this ticket.".to_string(),
                parameters: ticket_classify::tool_schema(spec, names),
            },
            Self::TicketExtract { spec, names, .. } => ToolDef {
                name: mentions::TOOL_NAME.to_string(),
                description: "Record every mention in this ticket (an empty list is a valid \
                              answer)."
                    .to_string(),
                parameters: mentions::tool_schema(spec, names),
            },
        }
    }

    fn tool_name(&self) -> &'static str {
        match self {
            Self::TicketClassify { .. } => ticket_classify::TOOL_NAME,
            Self::TicketExtract { .. } => mentions::TOOL_NAME,
        }
    }

    /// Validate a model answer into the cell's `value_json`.
    fn validate(&self, args: &serde_json::Value) -> Result<serde_json::Value, String> {
        match self {
            Self::TicketClassify { spec, names } => ticket_classify::validate(spec, names, args)
                .and_then(|cell| serde_json::to_value(cell).map_err(|e| e.to_string())),
            Self::TicketExtract {
                spec,
                names,
                resolver,
            } => mentions::validate(spec, names, resolver, args)
                .and_then(|cell| serde_json::to_value(cell).map_err(|e| e.to_string())),
        }
    }

    /// A cell's `value_json` as a human reads it: cells store vocabulary ids,
    /// so sample-run previews resolve them to current names (and list
    /// mentions as one row per mention).
    pub fn display_value(&self, value: &serde_json::Value) -> serde_json::Value {
        match self {
            Self::TicketClassify { names, .. } => {
                let Ok(cell) = serde_json::from_value::<ClassifyCell>(value.clone()) else {
                    return value.clone();
                };
                serde_json::json!({
                    "summary": cell.summary,
                    "category": ticket_classify::resolve_name(names, cell.category_id),
                    "subcategory": ticket_classify::resolve_name(names, cell.subcategory_id),
                    "sentiment": cell.sentiment,
                })
            },
            Self::TicketExtract { names, .. } => {
                let Ok(cell) = serde_json::from_value::<ExtractCell>(value.clone()) else {
                    return value.clone();
                };
                let mentions: Vec<serde_json::Value> = cell
                    .mentions
                    .iter()
                    .map(|m| {
                        let subject = m
                            .subject_id
                            .map(|id| ticket_classify::resolve_name(names, id))
                            .or_else(|| {
                                m.subject_surface
                                    .as_ref()
                                    .map(|s| format!("{s} (unresolved)"))
                            });
                        serde_json::json!({
                            "type": m.mention_type,
                            "subject": subject,
                            "feedback_summary": m.feedback_summary,
                            "feedback_category": m
                                .feedback_category_id
                                .map(|id| ticket_classify::resolve_name(names, id)),
                            "incidental": m.incidental,
                            "sentiment": m.sentiment,
                            "confidence": m.confidence,
                        })
                    })
                    .collect();
                serde_json::json!({ "mentions": mentions })
            },
        }
    }

    /// Materialised output column names on the parent table (excluding
    /// `{fn}__status`).
    pub fn output_columns(&self) -> Vec<String> {
        match self {
            Self::TicketClassify { .. } => {
                OUTPUT_COLUMNS.iter().map(|c| (*c).to_string()).collect()
            },
            Self::TicketExtract { .. } => FLAG_COLUMNS.iter().map(|c| (*c).to_string()).collect(),
        }
    }

    /// Columns an earlier shape of this function wrote and the current one
    /// does not. Materialisation drops them so a table enriched under the old
    /// shape does not carry stale columns next to the live ones.
    pub fn retired_columns(&self) -> Vec<String> {
        match self {
            Self::TicketClassify { .. } => {
                RETIRED_COLUMNS.iter().map(|c| (*c).to_string()).collect()
            },
            Self::TicketExtract { .. } => Vec::new(),
        }
    }

    /// Prompt-side character count for the cold-start token heuristic.
    pub fn prompt_chars(&self) -> usize {
        match self {
            Self::TicketClassify { spec, names } => {
                ticket_classify::system_prompt(spec, names).len()
            },
            Self::TicketExtract { spec, names, .. } => mentions::system_prompt(spec, names).len(),
        }
    }

    /// Output count for the cold-start token heuristic.
    pub fn output_count(&self) -> usize {
        self.output_columns().len()
    }
}

/// Lowercase primary subtag of a source-provided language tag ("sv-SE" →
/// "sv"); None for blank.
fn normalize_lang(raw: &str) -> Option<String> {
    let primary = raw.trim().split(['-', '_']).next()?.to_ascii_lowercase();
    (!primary.is_empty()).then_some(primary)
}

/// One row's rendered inputs, hashed for cache identity.
#[derive(Debug, Clone)]
pub struct RowInput {
    pub rendered: BTreeMap<String, String>,
    pub hash: String,
}

/// One computed (or cache-hit) cell.
#[derive(Debug, Clone)]
pub struct CellResult {
    pub status: String,
    pub value: Option<serde_json::Value>,
    pub error: Option<String>,
    pub cached: bool,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    /// Prompt tokens served from the provider's prefix cache; None when
    /// the provider never reported the figure.
    pub cached_tokens: Option<i64>,
}

/// Batch outcome keyed by input hash.
pub struct ExecOutcome {
    pub cells: HashMap<String, CellResult>,
    pub cache_hits: usize,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub cached_tokens: i64,
}

/// Row-level (done, failed, cached) counts, weighting each cell by how many
/// table rows share its input hash.
fn progress_counts(
    cells: &HashMap<String, CellResult>,
    multiplicity: &HashMap<String, i64>,
) -> (i64, i64, i64) {
    let mut done = 0_i64;
    let mut failed = 0_i64;
    let mut cached = 0_i64;
    for (hash, cell) in cells {
        let weight = multiplicity.get(hash).copied().unwrap_or(1);
        done += weight;
        if cell.status == "error" {
            failed += weight;
        }
        if cell.cached {
            cached += weight;
        }
    }
    (done, failed, cached)
}

/// Render + hash every row of `df` for any runnable spec.
pub fn prepare_run_inputs(df: &DataFrame, run: &RunSpec) -> AppResult<Vec<RowInput>> {
    let refs = run.input_columns();
    let mut columns: Vec<(String, Vec<String>)> = Vec::with_capacity(refs.len());
    for name in &refs {
        columns.push((name.clone(), column_as_strings(df, name)?));
    }

    let height = df.height();
    let mut out = Vec::with_capacity(height);
    for i in 0..height {
        let mut rendered = BTreeMap::new();
        for (name, values) in &columns {
            rendered.insert(name.clone(), values.get(i).cloned().unwrap_or_default());
        }
        run.augment(&mut rendered);
        let pairs: Vec<(String, String)> = rendered
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        out.push(RowInput {
            hash: input_hash(&pairs),
            rendered,
        });
    }
    Ok(out)
}

/// Read any column as strings; nulls become "".
fn column_as_strings(df: &DataFrame, name: &str) -> AppResult<Vec<String>> {
    let col = df
        .column(name)
        .map_err(|_| AppError::BadRequest(format!("column '{name}' not found in table")))?;
    let casted = col
        .as_materialized_series()
        .cast(&DataType::String)
        .map_err(|e| AppError::Internal(format!("cannot render column '{name}' as text: {e}")))?;
    let ca = casted
        .str()
        .map_err(|e| AppError::Internal(format!("column '{name}' cast: {e}")))?;
    Ok(ca
        .into_iter()
        .map(|v| v.unwrap_or_default().to_string())
        .collect())
}

/// Pull the arguments object out of a completion: the forced tool call when
/// the provider honored `tool_choice`, else the message text parsed as JSON
/// (some providers ignore `tool_choice`).
fn extract_args_named(outcome: &ChatOutcome, tool_name: &str) -> Result<serde_json::Value, String> {
    if let Some(call) = outcome
        .tool_calls()
        .iter()
        .find(|c| c.function.name == tool_name)
        .or_else(|| outcome.tool_calls().first())
    {
        return serde_json::from_str(&call.function.arguments)
            .map_err(|e| format!("tool arguments not valid JSON: {e}"));
    }
    let text = outcome.text().trim();
    let stripped = text
        .strip_prefix("```json")
        .or_else(|| text.strip_prefix("```"))
        .map_or(text, |s| s.strip_suffix("```").unwrap_or(s))
        .trim();
    if stripped.is_empty() {
        return Err("model returned neither a tool call nor JSON text".to_string());
    }
    serde_json::from_str(stripped).map_err(|e| format!("response is not valid JSON: {e}"))
}

fn add_tokens(outcome: &ChatOutcome, prompt: &mut i64, completion: &mut i64) {
    let p = outcome.prompt_tokens.map(|v| i64::try_from(v).unwrap_or(0));
    let c = outcome
        .completion_tokens
        .map(|v| i64::try_from(v).unwrap_or(0));
    match (p, c) {
        (Some(p), c) => {
            *prompt += p;
            *completion += c.unwrap_or(0);
        },
        // No split reported: attribute the total to the prompt side so the
        // estimate history still has signal.
        (None, _) => {
            *prompt += outcome
                .total_tokens
                .map_or(0, |v| i64::try_from(v).unwrap_or(0));
        },
    }
}

/// Fold a reported cached-token count into a running Option: stays None
/// until any call reports one, so "never reported" is distinguishable from 0.
fn add_cached_tokens(outcome: &ChatOutcome, cached: &mut Option<i64>) {
    if let Some(c) = outcome.cached_tokens {
        let c = i64::try_from(c).unwrap_or(0);
        *cached = Some(cached.unwrap_or(0) + c);
    }
}

/// One cell: render → chat (forced tool call) → validate → one re-ask.
///
/// Never panics, never propagates: every path lands in a CellResult.
/// Public for the eval CLI, which scores cells without touching the cache.
/// The message list for the single re-ask after a validation failure.
///
/// Deliberately a *fresh single turn* — the original system and user turns with
/// the failure appended — rather than a continuation of the tool-call exchange.
/// Echoing the assistant's tool call back and answering it with a `role:"tool"`
/// message is what the OpenAI protocol describes, but providers disagree on
/// what may legally follow a tool result. Mistral rejects the whole request
/// (`invalid_request_message_order`, "Not the same number of function calls and
/// responses"), so every re-ask died there and no row could ever recover from a
/// validation slip.
///
/// This call carries no state worth preserving — one stateless extraction with
/// a forced tool choice — so re-asking in exactly the shape that already
/// succeeded is both simpler and portable. The rejection travels as text in the
/// user turn instead of as a tool-call echo.
fn reask_messages(original: &[ChatMessage], tool_name: &str, error: &str) -> Vec<ChatMessage> {
    let mut out = original.to_vec();
    let note = format!(
        "Your previous answer was rejected: {error}. Call `{tool_name}` again with corrected \
         values that satisfy every rule above."
    );
    match out.last_mut() {
        // Append to the final user turn, keeping the exact system/user
        // alternation the first call used.
        Some(last) if last.role == "user" => {
            let mut content = last.content.clone().unwrap_or_default();
            content.push_str("\n\n");
            content.push_str(&note);
            last.content = Some(content);
        },
        _ => out.push(ChatMessage::user(note)),
    }
    out
}

pub async fn compute_cell(client: &ChatClient, run: &RunSpec, row: &RowInput) -> CellResult {
    let mut prompt_tokens = 0_i64;
    let mut completion_tokens = 0_i64;
    let mut cached_tokens: Option<i64> = None;
    let tools = [run.tool()];
    let tool_name = run.tool_name();
    let options = ChatOptions {
        tool_choice: Some(tool_name.to_string()),
        temperature: Some(TEMPERATURE),
        max_tokens: None,
        // Off: this is a forced tool call over a fixed schema, so thinking
        // traces would be billed completion tokens for no gain.
        reasoning_effort: None,
    };
    let policy = RetryPolicy::default();
    let messages = run.messages(row);

    let error_cell =
        |error: String, prompt_used: i64, completion_used: i64, cached: Option<i64>| CellResult {
            status: "error".to_string(),
            value: None,
            error: Some(error),
            cached: false,
            prompt_tokens: prompt_used,
            completion_tokens: completion_used,
            cached_tokens: cached,
        };

    let first = match chat_with_backoff(client, &messages, &tools, &options, &policy).await {
        Ok(outcome) => outcome,
        Err(e) => {
            return error_cell(
                format!("llm: {e}"),
                prompt_tokens,
                completion_tokens,
                cached_tokens,
            )
        },
    };
    add_tokens(&first, &mut prompt_tokens, &mut completion_tokens);
    add_cached_tokens(&first, &mut cached_tokens);

    let first_error = match extract_args_named(&first, tool_name).and_then(|v| run.validate(&v)) {
        Ok(value) => {
            return CellResult {
                status: "ok".to_string(),
                value: Some(value),
                error: None,
                cached: false,
                prompt_tokens,
                completion_tokens,
                cached_tokens,
            }
        },
        Err(why) => why,
    };

    // One re-ask with the validation error, then give up.
    let reask = reask_messages(&messages, tool_name, &first_error);
    let second = match chat_with_backoff(client, &reask, &tools, &options, &policy).await {
        Ok(outcome) => outcome,
        Err(e) => {
            return error_cell(
                format!("{first_error}; re-ask failed: {e}"),
                prompt_tokens,
                completion_tokens,
                cached_tokens,
            )
        },
    };
    add_tokens(&second, &mut prompt_tokens, &mut completion_tokens);
    add_cached_tokens(&second, &mut cached_tokens);

    match extract_args_named(&second, tool_name).and_then(|v| run.validate(&v)) {
        Ok(value) => CellResult {
            status: "ok".to_string(),
            value: Some(value),
            error: None,
            cached: false,
            prompt_tokens,
            completion_tokens,
            cached_tokens,
        },
        Err(why) => error_cell(
            format!("invalid after re-ask: {why}"),
            prompt_tokens,
            completion_tokens,
            cached_tokens,
        ),
    }
}

/// Execute all cells for `rows`.
///
/// Whether a batch may reuse previously computed cells.
///
/// `Bypass` exists for sample runs: a "test" that replays cached answers is not
/// a test, since the whole reason to run one is to see what the current prompt
/// and config produce. It also keeps results computed from a *draft* config out
/// of the cache, where they would later be served as if the draft had shipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheMode {
    /// Read hits and write results — the normal, cost-saving path.
    Use,
    /// Neither read nor write.
    Bypass,
}

/// A cached row as a reusable result, or `None` when it must be recomputed.
///
/// Only a success is a hit. A stored error is kept for reporting and for
/// `scope=failed` to clear, but never replayed: replaying one pins the row to
/// a failure it may have long grown out of — a fixed prompt, a fixed bug, a
/// provider outage since passed — and `sample_run` takes no scope, so a test
/// run could never escape it at all. The cost is one call per still-failing
/// row per run, which is the deliberate trade.
fn cache_hit(cached: &brightflow_store::EnrichmentCacheRow) -> Option<CellResult> {
    if cached.status != "ok" {
        return None;
    }
    Some(CellResult {
        status: cached.status.clone(),
        value: cached
            .value_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok()),
        error: cached.error.clone(),
        cached: true,
        prompt_tokens: 0,
        completion_tokens: 0,
        cached_tokens: None,
    })
}

/// Bulk cache lookup first, then misses through the LLM at bounded concurrency.
///
/// Every computed cell is written to the cache, successes and errors alike, but
/// only successes are read back as hits (see `cache_hit`) — so a failed row is
/// retried on the next run instead of being pinned to its error. Per-cell
/// failures never abort the batch. When `run_id` is set, the run row gets
/// progress updates every ~2s (row counts include duplicate-input rows).
pub async fn execute_cells(
    store: &ParquetStore,
    client: &ChatClient,
    function_id: &str,
    version: i64,
    run: &RunSpec,
    rows: &[RowInput],
    run_id: Option<&str>,
    cache: CacheMode,
) -> AppResult<ExecOutcome> {
    let shash = run.spec_hash();

    // Deduplicate by content: identical inputs share one cell.
    let mut unique: Vec<&RowInput> = Vec::new();
    let mut multiplicity: HashMap<String, i64> = HashMap::new();
    for row in rows {
        let count = multiplicity.entry(row.hash.clone()).or_insert(0);
        if *count == 0 {
            unique.push(row);
        }
        *count += 1;
    }
    let hashes: Vec<String> = unique.iter().map(|r| r.hash.clone()).collect();

    let mut cells: HashMap<String, CellResult> = HashMap::new();
    let mut cache_hits = 0_usize;
    if cache == CacheMode::Use {
        for cached in store
            .db()
            .get_cached_cells(function_id, &shash, &hashes)
            .await?
        {
            if let Some(cell) = cache_hit(&cached) {
                cache_hits += 1;
                cells.insert(cached.input_hash.clone(), cell);
            }
        }
    }

    // Collected eagerly on purpose: a lazy iterator would hold a borrow of
    // `cells` inside the stream while the loop below inserts into it.
    #[allow(clippy::needless_collect)]
    let misses: Vec<RowInput> = unique
        .iter()
        .filter(|r| !cells.contains_key(&r.hash))
        .map(|r| (*r).clone())
        .collect();

    let mut prompt_total = 0_i64;
    let mut completion_total = 0_i64;
    let mut cached_total = 0_i64;
    let mut last_progress = Instant::now();

    let mut stream = futures::stream::iter(misses.into_iter().map(|row: RowInput| {
        let client = client.clone();
        let run = run.clone();
        async move {
            let cell = compute_cell(&client, &run, &row).await;
            (row.hash, cell)
        }
    }))
    .buffer_unordered(CONCURRENCY);

    while let Some((hash, cell)) = stream.next().await {
        prompt_total += cell.prompt_tokens;
        completion_total += cell.completion_tokens;
        cached_total += cell.cached_tokens.unwrap_or(0);

        // Errors are written as well as successes, but only so the failure is
        // reportable and `scope=failed` has rows to clear. They are never
        // served back as hits (see the read above), so storing one cannot pin
        // a row to a failure it has since grown out of.
        let value_json = cell
            .value
            .as_ref()
            .and_then(|v| serde_json::to_string(v).ok());
        if cache == CacheMode::Bypass {
            // Nothing is written either: a sample may have been computed from a
            // draft config, and storing that would serve it back later as
            // though the draft had shipped.
        } else if let Err(e) = store
            .db()
            .upsert_cached_cell(
                function_id,
                &shash,
                &hash,
                &cell.status,
                value_json.as_deref(),
                cell.error.as_deref(),
                Some(cell.prompt_tokens),
                Some(cell.completion_tokens),
                cell.cached_tokens,
                version,
            )
            .await
        {
            tracing::warn!("enrichment cache write failed: {e}");
        }
        cells.insert(hash, cell);

        if let Some(rid) = run_id {
            if last_progress.elapsed() >= PROGRESS_EVERY {
                let (done, failed, cached_rows) = progress_counts(&cells, &multiplicity);
                if let Err(e) = store
                    .db()
                    .update_enrichment_run_progress(
                        rid,
                        done,
                        failed,
                        cached_rows,
                        prompt_total,
                        completion_total,
                        cached_total,
                    )
                    .await
                {
                    tracing::warn!("enrichment run progress write failed: {e}");
                }
                last_progress = Instant::now();
            }
        }
    }
    drop(stream);

    if let Some(rid) = run_id {
        let (done, failed, cached_rows) = progress_counts(&cells, &multiplicity);
        store
            .db()
            .update_enrichment_run_progress(
                rid,
                done,
                failed,
                cached_rows,
                prompt_total,
                completion_total,
                cached_total,
            )
            .await?;
    }

    Ok(ExecOutcome {
        cells,
        cache_hits,
        prompt_tokens: prompt_total,
        completion_tokens: completion_total,
        cached_tokens: cached_total,
    })
}

/// Rewrite the table with one materialized column per output, plus a
/// `{fn}__status` column.
///
/// Values come from the cache under the current spec; rows without a cache
/// entry get nulls (they fill in on the next run — unchanged content is a
/// free cache hit). For `ticket_classify` the `language` column is written
/// from the pre-call detector for every row, cached or not.
///
/// One materialisation per table at a time (`AppState::materialize_lock`):
/// two functions rebuilding the same table would otherwise race on
/// `replace_table_data`. The optimistic version check stays for the
/// scheduler's merge, which is a different process boundary and does not
/// take this lock; one retry re-reads and rebuilds.
pub async fn materialize(
    state: &AppState,
    store: &ParquetStore,
    source_id: &str,
    table_name: &str,
    function_id: &str,
    function_name: &str,
    run: &RunSpec,
) -> AppResult<()> {
    let shash = run.spec_hash();
    let lock = state.materialize_lock(&crate::state::cache_key(source_id, table_name));
    let _guard = lock.lock().await;
    let mut attempts = 0_u8;
    loop {
        attempts += 1;
        let table_row = store
            .db()
            .get_table(source_id, table_name)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("table '{table_name}' not found")))?;
        let df = store.read_table(source_id, table_name).await?;
        if df.height() == 0 {
            return Ok(());
        }
        let inputs = prepare_run_inputs(&df, run)?;
        let mut hashes: Vec<String> = inputs.iter().map(|r| r.hash.clone()).collect();
        hashes.sort_unstable();
        hashes.dedup();
        let cached = store
            .db()
            .get_cached_cells(function_id, &shash, &hashes)
            .await?;
        let by_hash: HashMap<&str, &brightflow_store::EnrichmentCacheRow> =
            cached.iter().map(|c| (c.input_hash.as_str(), c)).collect();

        let values_per_row: Vec<Option<serde_json::Value>> = inputs
            .iter()
            .map(|r| {
                by_hash.get(r.hash.as_str()).and_then(|c| {
                    c.value_json
                        .as_deref()
                        .and_then(|s| serde_json::from_str(s).ok())
                })
            })
            .collect();
        let status_per_row: Vec<Option<String>> = inputs
            .iter()
            .map(|r| by_hash.get(r.hash.as_str()).map(|c| c.status.clone()))
            .collect();

        let mut column_names: Vec<String> = run.output_columns();
        column_names.extend(run.retired_columns());
        column_names.push(format!("{function_name}__status"));
        let mut out_df = drop_stale_columns(df, &column_names).map_err(AppError::Polars)?;

        let mut child: Option<ChildTable> = None;
        let columns = match run {
            RunSpec::TicketClassify { names, .. } => {
                build_classify_columns(&inputs, &values_per_row, names)
            },
            RunSpec::TicketExtract {
                names, resolver, ..
            } => {
                let cells = extract_cells(&values_per_row, resolver);
                let id_column = parent_id_column(&out_df, table_name);
                let rows = mentions::build_mention_rows(&id_column, &cells, names)
                    .map_err(AppError::Polars)?;
                child = Some((rows, mentions::unresolved_surfaces(&cells)));
                mentions::derive_ticket_flags(&cells)
            },
        };
        for column in columns {
            out_df.with_column(column).map_err(AppError::Polars)?;
        }
        let status_name = format!("{function_name}__status");
        out_df
            .with_column(Column::new(status_name.as_str().into(), status_per_row))
            .map_err(AppError::Polars)?;

        match store
            .replace_table_data(source_id, table_name, out_df, Some(table_row.version))
            .await
        {
            Ok(()) => {
                if let Some((rows, unresolved)) = child {
                    let child_name = mentions::mentions_table_name(table_name);
                    write_child_table(store, source_id, &child_name, rows).await?;
                    state.invalidate_schema_cache(&crate::state::cache_key(source_id, &child_name));
                    if let Err(e) = store
                        .db()
                        .sync_unresolved_subjects(
                            &table_row.id,
                            &unresolved,
                            chrono::Utc::now().timestamp(),
                        )
                        .await
                    {
                        tracing::warn!("unresolved-subject sync failed: {e}");
                    }
                }
                break;
            },
            Err(brightflow_store::StoreError::VersionConflict { .. }) if attempts < 2 => {
                tracing::info!("table '{table_name}' moved during materialization — retrying once");
            },
            Err(e) => return Err(AppError::Store(e)),
        }
    }

    state.invalidate_schema_cache(&crate::state::cache_key(source_id, table_name));
    Ok(())
}

/// The extractor's child frame plus the unresolved `(kind, surface) → count`
/// it produced, carried from the rebuild to the write.
type ChildTable = (DataFrame, Vec<((String, String), usize)>);

/// Parse cached extraction cells and re-resolve any surface the live
/// resolver now knows (a mapped queue entry back-fills without an LLM call).
fn extract_cells(
    values: &[Option<serde_json::Value>],
    resolver: &SubjectResolver,
) -> Vec<Option<ExtractCell>> {
    values
        .iter()
        .map(|v| {
            let mut cell: ExtractCell = serde_json::from_value(v.clone()?).ok()?;
            for m in &mut cell.mentions {
                if m.subject_id.is_none() {
                    if let Some(id) = m
                        .subject_surface
                        .as_deref()
                        .and_then(|s| resolver.resolve(s))
                    {
                        m.subject_id = Some(id);
                        m.subject_surface = None;
                    }
                }
            }
            Some(cell)
        })
        .collect()
}

/// The parent's id column, per the table's display convention; a row index
/// when the table has none, so the child table still keys to something.
fn parent_id_column(df: &DataFrame, table_name: &str) -> Column {
    let id_column = crate::enrichment::display::DocDisplay::for_table(table_name).id_column;
    df.column(id_column).map_or_else(
        |_| {
            let idx: Vec<i64> = (0..df.height())
                .map(|i| i64::try_from(i).unwrap_or(i64::MAX))
                .collect();
            Column::new("row_index".into(), idx)
        },
        Column::clone,
    )
}

/// Write the whole child table: replace when it exists, create otherwise.
/// Creation goes through a scratch Parquet under the store root because the
/// store's create path takes a file, not a frame.
async fn write_child_table(
    store: &ParquetStore,
    source_id: &str,
    table_name: &str,
    df: DataFrame,
) -> AppResult<()> {
    if store.db().get_table(source_id, table_name).await?.is_some() {
        store
            .replace_table_data(source_id, table_name, df, None)
            .await?;
        return Ok(());
    }
    // The store's create path registers nothing for an empty file; a table
    // with no mentions yet simply does not exist until the first one lands.
    if df.height() == 0 {
        return Ok(());
    }
    let scratch = store
        .root_path()
        .join(format!(".mentions-{}.parquet", uuid::Uuid::now_v7()));
    let mut frame = df;
    let path = scratch.clone();
    let written = tokio::task::spawn_blocking(move || -> PolarsResult<()> {
        let file = std::fs::File::create(&path)?;
        ParquetWriter::new(file).finish(&mut frame)?;
        Ok(())
    })
    .await
    .map_err(|e| AppError::Internal(format!("scratch write join: {e}")))?;
    let ingested = match written {
        Ok(()) => store
            .ingest_parquet(source_id, table_name, &scratch, None)
            .await
            .map(|_| ())
            .map_err(AppError::Store),
        Err(e) => Err(AppError::Polars(e)),
    };
    drop(std::fs::remove_file(&scratch));
    ingested
}

/// The six `ticket_classify` columns, in `OUTPUT_COLUMNS` order. Ids resolve
/// to current names here, which is what makes a rename a re-materialise
/// rather than a re-call.
fn build_classify_columns(
    inputs: &[RowInput],
    values: &[Option<serde_json::Value>],
    names: &VocabNames,
) -> Vec<Column> {
    let cells: Vec<Option<ClassifyCell>> = values
        .iter()
        .map(|v| {
            v.as_ref()
                .and_then(|x| serde_json::from_value(x.clone()).ok())
        })
        .collect();
    let summary: Vec<Option<String>> = cells
        .iter()
        .map(|c| c.as_ref().map(|c| c.summary.clone()))
        .collect();
    let language: Vec<Option<String>> = inputs
        .iter()
        .map(|r| {
            r.rendered
                .get(LANGUAGE_INPUT)
                .filter(|l| !l.is_empty())
                .cloned()
        })
        .collect();
    let category: Vec<Option<String>> = cells
        .iter()
        .map(|c| {
            c.as_ref()
                .map(|c| ticket_classify::resolve_name(names, c.category_id))
        })
        .collect();
    let subcategory: Vec<Option<String>> = cells
        .iter()
        .map(|c| {
            c.as_ref()
                .map(|c| ticket_classify::resolve_name(names, c.subcategory_id))
        })
        .collect();
    let sentiment: Vec<Option<String>> = cells
        .iter()
        .map(|c| c.as_ref().map(|c| c.sentiment.clone()))
        .collect();
    vec![
        Column::new(OUTPUT_COLUMNS[0].into(), summary),
        Column::new(OUTPUT_COLUMNS[1].into(), language),
        Column::new(OUTPUT_COLUMNS[2].into(), category),
        Column::new(OUTPUT_COLUMNS[3].into(), subcategory),
        Column::new(OUTPUT_COLUMNS[4].into(), sentiment),
    ]
}

/// Remove every column in `names` that the frame has, so re-materialisation
/// replaces rather than duplicates: the function's current outputs, its
/// retired ones, and its status column. Names the frame lacks are skipped.
fn drop_stale_columns(df: DataFrame, names: &[String]) -> PolarsResult<DataFrame> {
    let mut out = df;
    for name in names {
        if out.column(name).is_ok() {
            out = out.drop(name)?;
        }
    }
    Ok(out)
}

/// Full/incremental run driver: execute all cells, materialize, finish the
/// run row. Spawned as an aborted-on-cancel task; the abort handle lives in
/// `state.enrichment_jobs`.
#[allow(clippy::too_many_arguments)]
pub async fn execute_full_run(
    state: AppState,
    run_id: String,
    function_id: String,
    function_name: String,
    version: i64,
    source_id: String,
    table_name: String,
    run: RunSpec,
) {
    let result = drive_run(
        &state,
        &run_id,
        &function_id,
        &function_name,
        version,
        &source_id,
        &table_name,
        &run,
    )
    .await;
    if let Some(store) = state.store() {
        let (status, error) = match &result {
            Ok(()) => ("completed", None),
            Err(e) => ("failed", Some(e.to_string())),
        };
        if let Err(db_err) = store
            .db()
            .finish_enrichment_run(&run_id, status, error.as_deref())
            .await
        {
            tracing::error!("failed to finish enrichment run {run_id}: {db_err}");
        }
    }
    if let Err(e) = result {
        tracing::warn!("enrichment run {run_id} failed: {e}");
    }
    state.enrichment_jobs.remove(&run_id);
}

#[allow(clippy::too_many_arguments)]
async fn drive_run(
    state: &AppState,
    run_id: &str,
    function_id: &str,
    function_name: &str,
    version: i64,
    source_id: &str,
    table_name: &str,
    run: &RunSpec,
) -> AppResult<()> {
    let store = state.require_store()?;
    let (provider_id, model) = run.provider();
    let client = crate::llm::client_for(state, provider_id, model).await?;
    let df = store.read_table(source_id, table_name).await?;
    let inputs = prepare_run_inputs(&df, run)?;
    execute_cells(
        store,
        &client,
        function_id,
        version,
        run,
        &inputs,
        Some(run_id),
        CacheMode::Use,
    )
    .await?;
    materialize(
        state,
        store,
        source_id,
        table_name,
        function_id,
        function_name,
        run,
    )
    .await
}

#[cfg(test)]
mod tests {

    use brightflow_store::EnrichmentCacheRow;

    fn cache_row(status: &str, value: Option<&str>, error: Option<&str>) -> EnrichmentCacheRow {
        EnrichmentCacheRow {
            function_id: "fn-1".to_string(),
            spec_hash: "spec-1".to_string(),
            input_hash: "row-1".to_string(),
            status: status.to_string(),
            value_json: value.map(str::to_string),
            error: error.map(str::to_string),
            prompt_tokens: Some(1),
            completion_tokens: Some(2),
            cached_tokens: None,
            version: 1,
            created_at: "2026-08-30 19:20:35".to_string(),
        }
    }

    #[test]
    fn a_cached_success_is_reused() {
        let hit = cache_hit(&cache_row("ok", Some(r#"{"a":1}"#), None)).expect("ok row is a hit");
        assert_eq!(hit.status, "ok");
        assert!(hit.cached);
        assert_eq!(hit.value, Some(json!({"a": 1})));
        // A hit bills nothing: the tokens were paid on the run that stored it.
        assert_eq!(hit.prompt_tokens, 0);
        assert_eq!(hit.completion_tokens, 0);
    }

    #[test]
    fn a_cached_error_is_never_reused() {
        // The policy this pins: errors are stored for reporting but must not
        // be replayed, or a row stays broken across the very fix that would
        // have repaired it — and sample_run, which takes no scope, could
        // never get past one.
        assert!(cache_hit(&cache_row("error", None, Some("llm: boom"))).is_none());
    }

    #[test]
    fn reask_repeats_the_first_calls_shape_with_the_failure_appended() {
        // No assistant or tool turns: providers disagree on what may legally
        // follow a tool result, and Mistral rejects such a request outright,
        // which is what silently killed every re-ask. Reusing the shape that
        // already succeeded cannot hit that class of error at all.
        let original = vec![
            ChatMessage::system("rules"),
            ChatMessage::user("ticket text"),
        ];
        let out = reask_messages(&original, "classify", "bad sentiment pairing");

        let roles: Vec<&str> = out.iter().map(|m| m.role.as_str()).collect();
        assert_eq!(roles, vec!["system", "user"]);

        let user = out[1].content.clone().expect("user turn has content");
        assert!(
            user.starts_with("ticket text"),
            "original turn preserved: {user}"
        );
        assert!(user.contains("bad sentiment pairing"), "states why: {user}");
        assert!(user.contains("classify"), "names the tool: {user}");
    }

    #[test]
    fn reask_appends_a_user_turn_when_the_last_message_is_not_one() {
        let original = vec![ChatMessage::system("rules")];
        let out = reask_messages(&original, "classify", "nope");
        let roles: Vec<&str> = out.iter().map(|m| m.role.as_str()).collect();
        assert_eq!(roles, vec!["system", "user"]);
    }

    use super::*;
    use brightflow_llm::{ToolCall, ToolCallFunction};
    use polars::df;
    use serde_json::json;

    fn cell(status: &str, cached: bool) -> CellResult {
        CellResult {
            status: status.to_string(),
            value: None,
            error: None,
            cached,
            prompt_tokens: 0,
            completion_tokens: 0,
            cached_tokens: None,
        }
    }

    /// A ChatOutcome with optional text content and/or tool calls.
    fn outcome(content: Option<&str>, calls: &[(&str, &str)]) -> ChatOutcome {
        let tool_calls = if calls.is_empty() {
            None
        } else {
            Some(
                calls
                    .iter()
                    .enumerate()
                    .map(|(i, (name, args))| ToolCall {
                        id: format!("t{i}"),
                        call_type: "function".to_string(),
                        function: ToolCallFunction {
                            name: (*name).to_string(),
                            arguments: (*args).to_string(),
                        },
                    })
                    .collect(),
            )
        };
        ChatOutcome {
            message: ChatMessage {
                role: "assistant".to_string(),
                content: content.map(str::to_string),
                tool_calls,
                tool_call_id: None,
            },
            finish_reason: None,
            total_tokens: None,
            prompt_tokens: None,
            completion_tokens: None,
            cached_tokens: None,
        }
    }

    fn token_outcome(
        prompt: Option<u64>,
        completion: Option<u64>,
        total: Option<u64>,
    ) -> ChatOutcome {
        let mut out = outcome(None, &[]);
        out.prompt_tokens = prompt;
        out.completion_tokens = completion;
        out.total_tokens = total;
        out
    }

    #[test]
    fn progress_counts_empty_cells() {
        assert_eq!(progress_counts(&HashMap::new(), &HashMap::new()), (0, 0, 0));
    }

    #[test]
    fn progress_counts_weights_rows_by_multiplicity() {
        let cells = HashMap::from([
            ("h1".to_string(), cell("ok", false)),
            ("h2".to_string(), cell("error", true)),
            ("h3".to_string(), cell("ok", true)),
        ]);
        // h3 is absent from the multiplicity map: it falls back to weight 1.
        let multiplicity = HashMap::from([("h1".to_string(), 3), ("h2".to_string(), 2)]);
        let (done, failed, cached) = progress_counts(&cells, &multiplicity);
        assert_eq!(done, 6);
        assert_eq!(failed, 2);
        assert_eq!(cached, 3);
    }

    #[test]
    fn column_as_strings_casts_and_fills_nulls() {
        let df = df!("n" => &[Some(7_i64), None]).unwrap();
        assert_eq!(column_as_strings(&df, "n").unwrap(), vec!["7", ""]);
        let err = column_as_strings(&df, "missing").unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[test]
    fn add_tokens_accumulates_a_reported_split() {
        let (mut prompt, mut completion) = (1_i64, 2_i64);
        add_tokens(
            &token_outcome(Some(10), Some(5), Some(15)),
            &mut prompt,
            &mut completion,
        );
        assert_eq!((prompt, completion), (11, 7));
        // Prompt reported without completion: completion side adds 0.
        add_tokens(
            &token_outcome(Some(10), None, None),
            &mut prompt,
            &mut completion,
        );
        assert_eq!((prompt, completion), (21, 7));
    }

    #[test]
    fn add_tokens_without_split_attributes_total_to_prompt() {
        let (mut prompt, mut completion) = (0_i64, 0_i64);
        add_tokens(
            &token_outcome(None, None, Some(30)),
            &mut prompt,
            &mut completion,
        );
        assert_eq!((prompt, completion), (30, 0));
        // Nothing reported at all: both stay put.
        add_tokens(
            &token_outcome(None, None, None),
            &mut prompt,
            &mut completion,
        );
        assert_eq!((prompt, completion), (30, 0));
    }

    #[test]
    fn add_tokens_drops_completion_when_prompt_is_missing() {
        // Pins current behavior: a completion count without a prompt count is
        // discarded — only the total lands, on the prompt side.
        let (mut prompt, mut completion) = (0_i64, 0_i64);
        add_tokens(
            &token_outcome(None, Some(5), Some(30)),
            &mut prompt,
            &mut completion,
        );
        assert_eq!((prompt, completion), (30, 0));
    }

    fn classify_run() -> RunSpec {
        use brightflow_engine::enrichment::VocabEntry;
        let spec = TicketClassifySpec {
            text_columns: vec!["title".to_string(), "body".to_string()],
            language_column: None,
            provider_id: "p1".to_string(),
            model: None,
            categories: vec![VocabEntry {
                id: 1,
                parent_id: 0,
                description: Some("charges".to_string()),
            }],
            subcategories: vec![],
        };
        let names: VocabNames = VocabNames::from([(1, "Billing".to_string())]);
        RunSpec::TicketClassify {
            spec,
            names: Arc::new(names),
        }
    }

    fn classify_run_with_description(description: &str) -> RunSpec {
        let RunSpec::TicketClassify { mut spec, names } = classify_run() else {
            unreachable!()
        };
        spec.categories[0].description = Some(description.to_string());
        RunSpec::TicketClassify { spec, names }
    }

    /// Language is detected pre-call, stored in the rendered inputs (so it is
    /// part of the cache identity) and carried into the user turn; the system
    /// turn is byte-identical across rows.
    #[test]
    fn classify_inputs_carry_a_detected_language_and_a_stable_prefix() {
        let df = df!(
            "title" => &["Faktura saknar moms", "Invoice missing VAT line"],
            "body" => &[
                "Fakturan visar inte momsen separat för kunder inom EU och beloppet blir därför fel.",
                "The generated invoice does not show the VAT breakdown for customers in the EU.",
            ],
        )
        .unwrap();
        let run = classify_run();
        let inputs = prepare_run_inputs(&df, &run).unwrap();
        assert_eq!(inputs[0].rendered[LANGUAGE_INPUT], "sv");
        assert_eq!(inputs[1].rendered[LANGUAGE_INPUT], "en");
        let m0 = run.messages(&inputs[0]);
        let m1 = run.messages(&inputs[1]);
        assert_eq!(
            m0[0].content, m1[0].content,
            "system turn must be prefix-cacheable"
        );
        assert!(m0[1]
            .content
            .as_deref()
            .unwrap()
            .starts_with("Ticket language: sv."));
        assert_eq!(run.tool_name(), ticket_classify::TOOL_NAME);
    }

    /// A source-provided language column wins over detection and is
    /// normalised to its primary subtag.
    #[test]
    fn classify_uses_the_language_column_when_configured() {
        let RunSpec::TicketClassify { mut spec, names } = classify_run() else {
            unreachable!()
        };
        spec.language_column = Some("lang".to_string());
        let run = RunSpec::TicketClassify { spec, names };
        let df = df!(
            "title" => &["x"],
            "body" => &["The generated invoice does not show the VAT breakdown."],
            "lang" => &["fi-FI"],
        )
        .unwrap();
        let inputs = prepare_run_inputs(&df, &run).unwrap();
        assert_eq!(inputs[0].rendered[LANGUAGE_INPUT], "fi");
    }

    /// Names never reach the hash: renaming an entry keeps every cell.
    #[test]
    fn classify_hash_ignores_names() {
        let a = classify_run();
        let RunSpec::TicketClassify { spec, .. } = classify_run() else {
            unreachable!()
        };
        let b = RunSpec::TicketClassify {
            spec,
            names: Arc::new(VocabNames::from([(1, "Payments".to_string())])),
        };
        assert_eq!(a.spec_hash(), b.spec_hash());
        assert_ne!(
            a.spec_hash(),
            classify_run_with_description("charges and refunds").spec_hash()
        );
    }

    /// Materialised columns resolve ids to current names and write the
    /// language for rows with no cell.
    #[test]
    fn classify_columns_resolve_ids_and_always_write_language() {
        let names: VocabNames = VocabNames::from([(1, "Billing".to_string())]);
        let inputs = vec![
            RowInput {
                rendered: BTreeMap::from([(LANGUAGE_INPUT.to_string(), "sv".to_string())]),
                hash: "h1".to_string(),
            },
            RowInput {
                rendered: BTreeMap::from([(LANGUAGE_INPUT.to_string(), String::new())]),
                hash: "h2".to_string(),
            },
        ];
        let values = vec![
            Some(json!({
                "summary": "Invoice omits VAT",
                "category_id": 1,
                "subcategory_id": 0,
                "sentiment": "negative",
            })),
            None,
        ];
        let cols = build_classify_columns(&inputs, &values, &names);
        let get = |i: usize, row: usize| -> Option<String> {
            cols[i]
                .as_materialized_series()
                .str()
                .unwrap()
                .get(row)
                .map(str::to_string)
        };
        assert_eq!(get(0, 0).as_deref(), Some("Invoice omits VAT"));
        assert_eq!(get(1, 0).as_deref(), Some("sv"));
        assert_eq!(get(1, 1), None);
        assert_eq!(get(2, 0).as_deref(), Some("Billing"));
        assert_eq!(get(3, 0).as_deref(), Some("other"));
        assert_eq!(get(4, 0).as_deref(), Some("negative"));
        assert_eq!(get(2, 1), None);
        assert_eq!(cols.len(), OUTPUT_COLUMNS.len());
    }

    #[test]
    fn retired_columns_are_dropped_with_current_ones() {
        // A table enriched under the two-field sentiment shape keeps
        // `sentiment_polarity` until something removes it; the retired list
        // is that something. Unrelated columns and absent names are untouched.
        let df = df!(
            "title" => &["a"],
            "sentiment" => &["neutral"],
            "sentiment_polarity" => &["neutral"],
            "sentiment_strength" => &["low"],
        )
        .unwrap();
        let mut names: Vec<String> = OUTPUT_COLUMNS.iter().map(|c| (*c).to_string()).collect();
        names.extend(RETIRED_COLUMNS.iter().map(|c| (*c).to_string()));
        names.push("classify__status".to_string());
        let out = drop_stale_columns(df, &names).unwrap();
        assert_eq!(out.get_column_names_str(), vec!["title"]);
    }

    #[test]
    fn normalize_lang_takes_the_primary_subtag() {
        assert_eq!(normalize_lang("sv-SE").as_deref(), Some("sv"));
        assert_eq!(normalize_lang(" EN_us ").as_deref(), Some("en"));
        assert_eq!(normalize_lang(""), None);
    }
}
