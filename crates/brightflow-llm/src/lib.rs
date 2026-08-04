//! Provider-agnostic LLM client speaking the OpenAI chat-completions dialect.
//!
//! Works against any compatible endpoint — ollama, llama.cpp, Mistral,
//! Scaleway, OpenAI, Anthropic-compat proxies — configured with nothing but
//! `base_url` + optional `api_key` + `model`. Non-streaming, tool-calling
//! capable. No vendor SDK.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("llm request failed: {0}")]
    Transport(String),
    #[error("llm rate limited (429): {0}")]
    RateLimited(String),
    #[error("llm auth failed ({status}): {message}")]
    Auth { status: u16, message: String },
    #[error("llm server error ({status}): {message}")]
    Server { status: u16, message: String },
    #[error("llm response unparseable: {0}")]
    BadResponse(String),
    #[error("llm request timed out")]
    Timeout,
}

/// One chat message (system / user / assistant / tool).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self::text("system", content)
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::text("user", content)
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self::text("assistant", content)
    }

    /// A tool-result message answering one tool call.
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".to_string(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
        }
    }

    fn text(role: &str, content: impl Into<String>) -> Self {
        Self {
            role: role.to_string(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }
}

/// One tool the model may call (OpenAI "function" flavor).
#[derive(Debug, Clone, Serialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    /// JSON Schema of the arguments object.
    pub parameters: serde_json::Value,
}

/// A tool call emitted by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type", default = "default_tool_type")]
    pub call_type: String,
    pub function: ToolCallFunction,
}

fn default_tool_type() -> String {
    "function".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    /// JSON-encoded arguments string (per the OpenAI wire format).
    pub arguments: String,
}

/// The assistant's turn: text and/or tool calls.
#[derive(Debug, Clone)]
pub struct ChatOutcome {
    pub message: ChatMessage,
    pub finish_reason: Option<String>,
    /// Total tokens reported by the provider, when available.
    pub total_tokens: Option<u64>,
    /// Prompt-side tokens, when the provider reports the split.
    pub prompt_tokens: Option<u64>,
    /// Completion-side tokens, when the provider reports the split.
    pub completion_tokens: Option<u64>,
}

impl ChatOutcome {
    pub fn tool_calls(&self) -> &[ToolCall] {
        self.message.tool_calls.as_deref().unwrap_or(&[])
    }

    pub fn text(&self) -> &str {
        self.message.content.as_deref().unwrap_or("")
    }
}

/// Per-call knobs beyond messages and tools.
#[derive(Debug, Clone, Default)]
pub struct ChatOptions {
    /// Force the model to call this tool (by name) instead of answering freely.
    pub tool_choice: Option<String>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u64>,
}

/// Exponential-backoff policy for `chat_with_backoff`.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_delay_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay_ms: 2000,
        }
    }
}

/// One completion with retries on transient failures (rate limit, 5xx,
/// timeout). Delay doubles each attempt with jitter; other errors and
/// exhausted attempts surface as-is.
pub async fn chat_with_backoff(
    client: &ChatClient,
    messages: &[ChatMessage],
    tools: &[ToolDef],
    options: &ChatOptions,
    policy: &RetryPolicy,
) -> Result<ChatOutcome, LlmError> {
    let mut attempt: u32 = 0;
    loop {
        attempt += 1;
        let result = client.chat_with_options(messages, tools, options).await;
        match result {
            Ok(outcome) => return Ok(outcome),
            Err(err @ (LlmError::RateLimited(_) | LlmError::Server { .. } | LlmError::Timeout)) => {
                if attempt >= policy.max_attempts {
                    return Err(err);
                }
                let exp = attempt.saturating_sub(1).min(10);
                let backoff = policy.base_delay_ms.saturating_mul(1_u64 << exp);
                tokio::time::sleep(std::time::Duration::from_millis(
                    backoff.saturating_add(jitter_ms(backoff)),
                ))
                .await;
            },
            Err(err) => return Err(err),
        }
    }
}

/// Cheap decorrelated jitter in `[0, backoff/2]` without a rand dependency.
fn jitter_ms(backoff: u64) -> u64 {
    let half = (backoff / 2).max(1);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::from(d.subsec_nanos()));
    nanos % half
}

/// Minimal chat-completions client.
#[derive(Debug, Clone)]
pub struct ChatClient {
    http: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    model: String,
}

const DEFAULT_TIMEOUT_SECS: u64 = 120;

impl ChatClient {
    /// `base_url` up to but not including `/chat/completions`,
    /// e.g. `http://localhost:11434/v1` or `https://api.mistral.ai/v1`.
    pub fn new(
        base_url: impl Into<String>,
        api_key: Option<String>,
        model: impl Into<String>,
    ) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()
            .unwrap_or_default();
        Self {
            http,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key,
            model: model.into(),
        }
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    /// One completion turn. `tools` may be empty.
    pub async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDef],
    ) -> Result<ChatOutcome, LlmError> {
        self.chat_with_options(messages, tools, &ChatOptions::default())
            .await
    }

    /// One completion turn with per-call options (forced tool choice,
    /// temperature, max tokens). `tools` may be empty.
    pub async fn chat_with_options(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDef],
        options: &ChatOptions,
    ) -> Result<ChatOutcome, LlmError> {
        let body = build_request_body(&self.model, messages, tools, options);

        let url = format!("{}/chat/completions", self.base_url);
        let mut request = self.http.post(&url).json(&body);
        if let Some(key) = &self.api_key {
            if !key.is_empty() {
                request = request.bearer_auth(key);
            }
        }

        let response = request.send().await.map_err(|e| {
            if e.is_timeout() {
                LlmError::Timeout
            } else {
                LlmError::Transport(e.to_string())
            }
        })?;

        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|e| LlmError::Transport(e.to_string()))?;

        match status.as_u16() {
            200 => parse_completion(&text),
            401 | 403 => Err(LlmError::Auth {
                status: status.as_u16(),
                message: truncate(&text),
            }),
            429 => Err(LlmError::RateLimited(truncate(&text))),
            s if s >= 500 => Err(LlmError::Server {
                status: s,
                message: truncate(&text),
            }),
            s => Err(LlmError::Transport(format!(
                "unexpected status {s}: {}",
                truncate(&text)
            ))),
        }
    }

    /// Cheap connectivity test: one-token completion.
    pub async fn ping(&self) -> Result<(), LlmError> {
        let messages = [ChatMessage::user("ping")];
        self.chat(&messages, &[]).await.map(|_| ())
    }
}

/// Assemble the chat-completions request body (kept separate so tests can
/// assert the exact wire shape without a server).
fn build_request_body(
    model: &str,
    messages: &[ChatMessage],
    tools: &[ToolDef],
    options: &ChatOptions,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": model,
        "messages": messages,
    });
    let Some(obj) = body.as_object_mut() else {
        return body;
    };
    if !tools.is_empty() {
        let wire_tools: Vec<serde_json::Value> = tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    }
                })
            })
            .collect();
        obj.insert("tools".to_string(), serde_json::Value::Array(wire_tools));
    }
    if let Some(name) = &options.tool_choice {
        obj.insert(
            "tool_choice".to_string(),
            serde_json::json!({ "type": "function", "function": { "name": name } }),
        );
    }
    if let Some(temperature) = options.temperature {
        obj.insert("temperature".to_string(), serde_json::json!(temperature));
    }
    if let Some(max_tokens) = options.max_tokens {
        obj.insert("max_tokens".to_string(), serde_json::json!(max_tokens));
    }
    body
}

/// Byte budget for provider error bodies embedded in `LlmError` messages.
const ERROR_BODY_MAX_BYTES: usize = 300;

/// Trim and cap a provider error body at `ERROR_BODY_MAX_BYTES` bytes,
/// appending an ellipsis when cut. The cut always lands on a UTF-8 char
/// boundary (backing off past any multi-byte tail), so this never panics on
/// arbitrary provider bytes.
fn truncate(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.len() <= ERROR_BODY_MAX_BYTES {
        return trimmed.to_string();
    }
    // Walk back from the budget to the nearest char boundary; at most 3 steps
    // since a UTF-8 sequence is at most 4 bytes.
    let mut end = ERROR_BODY_MAX_BYTES;
    while !trimmed.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &trimmed[..end])
}

#[derive(Debug, Deserialize)]
struct WireResponse {
    choices: Vec<WireChoice>,
    #[serde(default)]
    usage: Option<WireUsage>,
}

#[derive(Debug, Deserialize)]
struct WireChoice {
    message: ChatMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(clippy::struct_field_names)]
struct WireUsage {
    #[serde(default)]
    total_tokens: Option<u64>,
    #[serde(default)]
    prompt_tokens: Option<u64>,
    #[serde(default)]
    completion_tokens: Option<u64>,
}

fn parse_completion(text: &str) -> Result<ChatOutcome, LlmError> {
    let wire: WireResponse =
        serde_json::from_str(text).map_err(|e| LlmError::BadResponse(e.to_string()))?;
    let choice = wire
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| LlmError::BadResponse("no choices in response".to_string()))?;
    let usage = wire.usage.unwrap_or(WireUsage {
        total_tokens: None,
        prompt_tokens: None,
        completion_tokens: None,
    });
    Ok(ChatOutcome {
        message: choice.message,
        finish_reason: choice.finish_reason,
        total_tokens: usage.total_tokens,
        prompt_tokens: usage.prompt_tokens,
        completion_tokens: usage.completion_tokens,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tool_call_response() {
        let raw = r#"{
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {"name": "rename_cluster", "arguments": "{\"name\":\"Payments\"}"}
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {"total_tokens": 42}
        }"#;
        let outcome = parse_completion(raw).unwrap();
        assert_eq!(outcome.tool_calls().len(), 1);
        assert_eq!(outcome.tool_calls()[0].function.name, "rename_cluster");
        assert_eq!(outcome.total_tokens, Some(42));
        assert_eq!(outcome.finish_reason.as_deref(), Some("tool_calls"));
    }

    #[test]
    fn parses_plain_text_response() {
        let raw = r#"{"choices":[{"message":{"role":"assistant","content":"hello"},"finish_reason":"stop"}]}"#;
        let outcome = parse_completion(raw).unwrap();
        assert_eq!(outcome.text(), "hello");
        assert!(outcome.tool_calls().is_empty());
    }

    #[test]
    fn bad_json_maps_to_bad_response() {
        assert!(matches!(
            parse_completion("not json"),
            Err(LlmError::BadResponse(_))
        ));
    }

    #[test]
    fn parses_usage_token_split() {
        let raw = r#"{
            "choices": [{"message": {"role": "assistant", "content": "hi"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 30, "completion_tokens": 12, "total_tokens": 42}
        }"#;
        let outcome = parse_completion(raw).unwrap();
        assert_eq!(outcome.prompt_tokens, Some(30));
        assert_eq!(outcome.completion_tokens, Some(12));
        assert_eq!(outcome.total_tokens, Some(42));
    }

    #[test]
    fn missing_usage_split_is_none() {
        let raw = r#"{
            "choices": [{"message": {"role": "assistant", "content": "hi"}}],
            "usage": {"total_tokens": 42}
        }"#;
        let outcome = parse_completion(raw).unwrap();
        assert_eq!(outcome.prompt_tokens, None);
        assert_eq!(outcome.completion_tokens, None);
        assert_eq!(outcome.total_tokens, Some(42));
    }

    #[test]
    fn tool_choice_serializes_as_forced_function() {
        let tools = [ToolDef {
            name: "set_values".to_string(),
            description: "Record output values".to_string(),
            parameters: serde_json::json!({"type": "object"}),
        }];
        let options = ChatOptions {
            tool_choice: Some("set_values".to_string()),
            temperature: Some(0.0),
            max_tokens: Some(512),
        };
        let body = build_request_body("m", &[ChatMessage::user("x")], &tools, &options);
        assert_eq!(
            body["tool_choice"],
            serde_json::json!({"type": "function", "function": {"name": "set_values"}})
        );
        assert_eq!(body["temperature"], serde_json::json!(0.0));
        assert_eq!(body["max_tokens"], serde_json::json!(512));
        assert_eq!(body["tools"][0]["function"]["name"], "set_values");
    }

    #[test]
    fn truncate_cuts_ascii_over_limit() {
        let long = "a".repeat(ERROR_BODY_MAX_BYTES + 50);
        let out = truncate(&long);
        assert_eq!(out, format!("{}…", "a".repeat(ERROR_BODY_MAX_BYTES)));
    }

    #[test]
    fn truncate_keeps_exact_limit_untouched() {
        let exact = "b".repeat(ERROR_BODY_MAX_BYTES);
        assert_eq!(truncate(&exact), exact);
    }

    #[test]
    fn truncate_backs_off_to_char_boundary_mid_multibyte() {
        // 'x' + 151×'ä' (2 bytes each) = 303 bytes; every 'ä' boundary is odd,
        // so byte 300 falls mid-character. Pre-fix this sliced at 300 and
        // panicked; now it must back off to byte 299.
        let s = format!("x{}", "ä".repeat(151));
        let out = truncate(&s);
        assert_eq!(out, format!("x{}…", "ä".repeat(149)));
        assert!(out.len() <= ERROR_BODY_MAX_BYTES + '…'.len_utf8());
    }

    #[test]
    fn truncate_backs_off_mid_emoji() {
        // 'y' + 76×4-byte emoji = 305 bytes; boundaries sit at 1 + 4k, so byte
        // 300 lands inside an emoji and the cut must retreat to byte 297.
        let s = format!("y{}", "😀".repeat(76));
        let out = truncate(&s);
        assert_eq!(out, format!("y{}…", "😀".repeat(74)));
    }

    #[test]
    fn jitter_stays_within_half_backoff() {
        // Jitter is time-seeded so the exact value is not assertable, but the
        // contract `[0, backoff/2]` (and 0 for backoff 0) is.
        assert_eq!(jitter_ms(0), 0);
        assert_eq!(jitter_ms(1), 0);
        for backoff in [2, 100, 4000] {
            assert!(jitter_ms(backoff) <= backoff / 2);
        }
    }

    #[test]
    fn default_options_add_no_extra_fields() {
        let body = build_request_body("m", &[ChatMessage::user("x")], &[], &ChatOptions::default());
        assert!(body.get("tool_choice").is_none());
        assert!(body.get("temperature").is_none());
        assert!(body.get("max_tokens").is_none());
        assert!(body.get("tools").is_none());
    }
}
