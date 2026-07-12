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
}

impl ChatOutcome {
    pub fn tool_calls(&self) -> &[ToolCall] {
        self.message.tool_calls.as_deref().unwrap_or(&[])
    }

    pub fn text(&self) -> &str {
        self.message.content.as_deref().unwrap_or("")
    }
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
        let mut body = serde_json::json!({
            "model": self.model,
            "messages": messages,
        });
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
            if let Some(obj) = body.as_object_mut() {
                obj.insert("tools".to_string(), serde_json::Value::Array(wire_tools));
            }
        }

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

fn truncate(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.len() > 300 {
        format!("{}…", &trimmed[..300])
    } else {
        trimmed.to_string()
    }
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
struct WireUsage {
    #[serde(default)]
    total_tokens: Option<u64>,
}

fn parse_completion(text: &str) -> Result<ChatOutcome, LlmError> {
    let wire: WireResponse =
        serde_json::from_str(text).map_err(|e| LlmError::BadResponse(e.to_string()))?;
    let choice = wire
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| LlmError::BadResponse("no choices in response".to_string()))?;
    Ok(ChatOutcome {
        message: choice.message,
        finish_reason: choice.finish_reason,
        total_tokens: wire.usage.and_then(|u| u.total_tokens),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
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
}
