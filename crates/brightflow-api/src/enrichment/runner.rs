//! Async LLM batch runner + column materialization for `llm_prompt` functions.
//!
//! The engine owns the pure pieces (render/hash/schema/validate); this module
//! owns the async loop, the cache, and the parquet rewrite.

use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use futures::StreamExt;
use polars::prelude::*;

use brightflow_engine::enrichment::{
    extract_column_refs, input_hash, output_tool_schema, render_prompt, spec_hash, validate_output,
    LlmPromptSpec, OutputField, OutputType,
};
use brightflow_llm::{
    chat_with_backoff, ChatClient, ChatMessage, ChatOptions, ChatOutcome, RetryPolicy, ToolDef,
};
use brightflow_store::ParquetStore;

use crate::shared::{AppError, AppResult};
use crate::state::AppState;

/// Concurrent in-flight LLM calls per run.
const CONCURRENCY: usize = 4;
/// Tool name for the forced structured-output call.
const TOOL_NAME: &str = "set_values";
/// Progress-row update cadence.
const PROGRESS_EVERY: Duration = Duration::from_secs(2);

const SYSTEM_PROMPT: &str = "You are a data-enrichment function. Apply the instruction to the \
given row and record the result by calling the `set_values` tool. Always call the tool — never \
reply with prose.";

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
}

/// Batch outcome keyed by input hash.
pub struct ExecOutcome {
    pub cells: HashMap<String, CellResult>,
    pub cache_hits: usize,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
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

/// Render + hash every row of `df` for this spec. Template references must
/// already be validated against the schema; a missing column errors here.
pub fn prepare_inputs(df: &DataFrame, spec: &LlmPromptSpec) -> AppResult<Vec<RowInput>> {
    let refs = extract_column_refs(&spec.prompt_template);
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

fn set_values_tool(outputs: &[OutputField]) -> ToolDef {
    ToolDef {
        name: TOOL_NAME.to_string(),
        description: "Record the enrichment output values for this row.".to_string(),
        parameters: output_tool_schema(outputs),
    }
}

/// Pull the arguments object out of a completion: the forced tool call when
/// the provider honored `tool_choice`, else the message text parsed as JSON
/// (some providers ignore `tool_choice`).
fn extract_args(outcome: &ChatOutcome) -> Result<serde_json::Value, String> {
    if let Some(call) = outcome
        .tool_calls()
        .iter()
        .find(|c| c.function.name == TOOL_NAME)
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

/// One cell: render → chat (forced tool call) → validate → one re-ask.
/// Never panics, never propagates: every path lands in a CellResult.
async fn compute_cell(client: &ChatClient, spec: &LlmPromptSpec, row: &RowInput) -> CellResult {
    let mut prompt_tokens = 0_i64;
    let mut completion_tokens = 0_i64;
    let tool = set_values_tool(&spec.outputs);
    let tools = [tool];
    let options = ChatOptions {
        tool_choice: Some(TOOL_NAME.to_string()),
        temperature: Some(0.0),
        max_tokens: None,
    };
    let policy = RetryPolicy::default();
    let user_prompt = render_prompt(&spec.prompt_template, &row.rendered);
    let mut messages = vec![
        ChatMessage::system(SYSTEM_PROMPT),
        ChatMessage::user(user_prompt),
    ];

    let error_cell = |error: String, prompt_used: i64, completion_used: i64| CellResult {
        status: "error".to_string(),
        value: None,
        error: Some(error),
        cached: false,
        prompt_tokens: prompt_used,
        completion_tokens: completion_used,
    };

    let first = match chat_with_backoff(client, &messages, &tools, &options, &policy).await {
        Ok(outcome) => outcome,
        Err(e) => return error_cell(format!("llm: {e}"), prompt_tokens, completion_tokens),
    };
    add_tokens(&first, &mut prompt_tokens, &mut completion_tokens);

    let first_error = match extract_args(&first)
        .and_then(|v| validate_output(&spec.outputs, &v).map(serde_json::Value::Object))
    {
        Ok(value) => {
            return CellResult {
                status: "ok".to_string(),
                value: Some(value),
                error: None,
                cached: false,
                prompt_tokens,
                completion_tokens,
            }
        },
        Err(why) => why,
    };

    // One re-ask with the validation error, then give up.
    messages.push(first.message.clone());
    messages.push(ChatMessage::user(format!(
        "Your previous output was invalid: {first_error}. Call `{TOOL_NAME}` again with corrected \
         values that satisfy the schema."
    )));
    let second = match chat_with_backoff(client, &messages, &tools, &options, &policy).await {
        Ok(outcome) => outcome,
        Err(e) => {
            return error_cell(
                format!("{first_error}; re-ask failed: {e}"),
                prompt_tokens,
                completion_tokens,
            )
        },
    };
    add_tokens(&second, &mut prompt_tokens, &mut completion_tokens);

    match extract_args(&second)
        .and_then(|v| validate_output(&spec.outputs, &v).map(serde_json::Value::Object))
    {
        Ok(value) => CellResult {
            status: "ok".to_string(),
            value: Some(value),
            error: None,
            cached: false,
            prompt_tokens,
            completion_tokens,
        },
        Err(why) => error_cell(
            format!("invalid after re-ask: {why}"),
            prompt_tokens,
            completion_tokens,
        ),
    }
}

/// Execute all cells for `rows`.
///
/// Bulk cache lookup first, then misses through the LLM at bounded
/// concurrency. Every computed cell (ok AND error) is cached. Per-cell
/// failures never abort the batch. When `run_id` is set, the run row gets
/// progress updates every ~2s (row counts include duplicate-input rows).
pub async fn execute_cells(
    store: &ParquetStore,
    client: &ChatClient,
    function_id: &str,
    version: i64,
    spec: &LlmPromptSpec,
    rows: &[RowInput],
    run_id: Option<&str>,
) -> AppResult<ExecOutcome> {
    let shash = spec_hash(spec);

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
    for cached in store
        .db()
        .get_cached_cells(function_id, &shash, &hashes)
        .await?
    {
        cache_hits += 1;
        cells.insert(
            cached.input_hash.clone(),
            CellResult {
                status: cached.status.clone(),
                value: cached
                    .value_json
                    .as_deref()
                    .and_then(|s| serde_json::from_str(s).ok()),
                error: cached.error.clone(),
                cached: true,
                prompt_tokens: 0,
                completion_tokens: 0,
            },
        );
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
    let mut last_progress = Instant::now();

    let mut stream = futures::stream::iter(misses.into_iter().map(|row: RowInput| {
        let client = client.clone();
        let spec = spec.clone();
        async move {
            let cell = compute_cell(&client, &spec, &row).await;
            (row.hash, cell)
        }
    }))
    .buffer_unordered(CONCURRENCY);

    while let Some((hash, cell)) = stream.next().await {
        prompt_total += cell.prompt_tokens;
        completion_total += cell.completion_tokens;

        // Cache ok AND error results — a full re-run must not hammer the
        // provider with known-bad rows; scope=failed clears errors first.
        let value_json = cell
            .value
            .as_ref()
            .and_then(|v| serde_json::to_string(v).ok());
        if let Err(e) = store
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
            )
            .await?;
    }

    Ok(ExecOutcome {
        cells,
        cache_hits,
        prompt_tokens: prompt_total,
        completion_tokens: completion_total,
    })
}

/// Rewrite the table with one materialized column per output field, plus a
/// `{fn}__status` column.
///
/// Values come from the cache under the current spec; rows without a cache
/// entry get nulls (they fill in on the next run — unchanged content is a
/// free cache hit).
///
/// A concurrent sync moving `tables.version` fails the optimistic check; one
/// retry re-reads and rebuilds. The residual window is the same one
/// `merge_parquet` already lives with.
pub async fn materialize(
    state: &AppState,
    store: &ParquetStore,
    source_id: &str,
    table_name: &str,
    function_id: &str,
    function_name: &str,
    spec: &LlmPromptSpec,
) -> AppResult<()> {
    let shash = spec_hash(spec);
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
        let inputs = prepare_inputs(&df, spec)?;
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

        let mut out_df = df;
        // Drop any pre-existing columns with our names (re-materialization).
        let mut names: Vec<String> = spec.outputs.iter().map(|f| f.name.clone()).collect();
        names.push(format!("{function_name}__status"));
        for name in &names {
            if out_df.column(name).is_ok() {
                out_df = out_df.drop(name).map_err(AppError::Polars)?;
            }
        }

        for field in &spec.outputs {
            let column = build_output_column(field, &values_per_row);
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
            Ok(()) => break,
            Err(brightflow_store::StoreError::VersionConflict { .. }) if attempts < 2 => {
                tracing::info!("table '{table_name}' moved during materialization — retrying once");
            },
            Err(e) => return Err(AppError::Store(e)),
        }
    }

    state.invalidate_schema_cache(&crate::state::cache_key(source_id, table_name));
    Ok(())
}

fn build_output_column(field: &OutputField, values: &[Option<serde_json::Value>]) -> Column {
    let name = field.name.as_str().into();
    let field_values = values
        .iter()
        .map(|v| v.as_ref().and_then(|obj| obj.get(&field.name)));
    match &field.dtype {
        OutputType::Number => {
            let data: Vec<Option<f64>> = field_values
                .map(|v| v.and_then(serde_json::Value::as_f64))
                .collect();
            Column::new(name, data)
        },
        OutputType::Bool => {
            let data: Vec<Option<bool>> = field_values
                .map(|v| v.and_then(serde_json::Value::as_bool))
                .collect();
            Column::new(name, data)
        },
        OutputType::Json => {
            let data: Vec<Option<String>> = field_values
                .map(|v| v.and_then(|x| serde_json::to_string(x).ok()))
                .collect();
            Column::new(name, data)
        },
        OutputType::String | OutputType::Enum { .. } => {
            let data: Vec<Option<String>> = field_values
                .map(|v| v.and_then(|x| x.as_str().map(ToString::to_string)))
                .collect();
            Column::new(name, data)
        },
    }
}

/// Full/incremental run driver: execute all cells, materialize, finish the
/// run row. Spawned as an aborted-on-cancel task; the abort handle lives in
/// `state.enrichment_jobs`.
pub async fn execute_full_run(
    state: AppState,
    run_id: String,
    function_id: String,
    function_name: String,
    version: i64,
    source_id: String,
    table_name: String,
    spec: LlmPromptSpec,
) {
    let result = drive_run(
        &state,
        &run_id,
        &function_id,
        &function_name,
        version,
        &source_id,
        &table_name,
        &spec,
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
    spec: &LlmPromptSpec,
) -> AppResult<()> {
    let store = state
        .store()
        .ok_or_else(|| AppError::Internal("store unavailable".to_string()))?;
    let client = crate::llm::client_for(state, &spec.provider_id, spec.model.as_deref()).await?;
    let df = store.read_table(source_id, table_name).await?;
    let inputs = prepare_inputs(&df, spec)?;
    execute_cells(
        store,
        &client,
        function_id,
        version,
        spec,
        &inputs,
        Some(run_id),
    )
    .await?;
    materialize(
        state,
        store,
        source_id,
        table_name,
        function_id,
        function_name,
        spec,
    )
    .await
}
