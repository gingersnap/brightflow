//! LLM provider configuration (optional, provider-agnostic).
//!
//! Providers are OpenAI-compatible chat-completions endpoints; a `ChatClient`
//! is constructed from (base_url, api_key, model) — Claude-compat proxies,
//! Mistral, Scaleway, ollama, llama.cpp all work. Env fallback
//! `BRIGHTFLOW_LLM_BASE_URL` / `_API_KEY` / `_MODEL` seeds a provider row.

pub mod handlers;
pub mod types;

use brightflow_llm::ChatClient;

use crate::shared::{AppError, AppResult};
use crate::state::AppState;

/// Build a chat client from the default (or only) configured provider.
pub async fn default_client(state: &AppState) -> AppResult<ChatClient> {
    let db = state
        .auth_db
        .as_ref()
        .ok_or_else(|| AppError::BadRequest("no database configured".to_string()))?;
    let rows = handlers::list_provider_rows(db).await?;
    let provider = rows
        .iter()
        .find(|p| p.is_default)
        .or_else(|| rows.first())
        .ok_or_else(|| {
            AppError::BadRequest(
                "no LLM provider configured — add one under Settings → LLM".to_string(),
            )
        })?;
    Ok(ChatClient::new(
        provider.base_url.clone(),
        provider.api_key.clone(),
        provider.model.clone(),
    ))
}

/// Build a chat client for a specific provider (by id) with an optional
/// model override. `""` or `"default"` resolve to the default provider.
pub async fn client_for(
    state: &AppState,
    provider_id: &str,
    model_override: Option<&str>,
) -> AppResult<ChatClient> {
    let db = state
        .auth_db
        .as_ref()
        .ok_or_else(|| AppError::BadRequest("no database configured".to_string()))?;
    let rows = handlers::list_provider_rows(db).await?;
    let provider = if provider_id.is_empty() || provider_id == "default" {
        rows.iter().find(|p| p.is_default).or_else(|| rows.first())
    } else {
        let id: i64 = provider_id
            .parse()
            .map_err(|_| AppError::BadRequest(format!("invalid provider id '{provider_id}'")))?;
        rows.iter().find(|p| p.id == id)
    }
    .ok_or_else(|| {
        AppError::BadRequest(format!(
            "LLM provider '{provider_id}' not found — configure one under Settings → LLM"
        ))
    })?;
    let model = model_override
        .filter(|m| !m.trim().is_empty())
        .unwrap_or(&provider.model);
    Ok(ChatClient::new(
        provider.base_url.clone(),
        provider.api_key.clone(),
        model.to_string(),
    ))
}

/// Seed a provider row from `BRIGHTFLOW_LLM_*` env vars when none exist.
pub async fn seed_from_env(state: &AppState) {
    let Some(db) = state.auth_db.as_ref() else {
        return;
    };
    let Ok(base_url) = std::env::var("BRIGHTFLOW_LLM_BASE_URL") else {
        return;
    };
    let model = std::env::var("BRIGHTFLOW_LLM_MODEL").unwrap_or_else(|_| "default".to_string());
    let api_key = std::env::var("BRIGHTFLOW_LLM_API_KEY").ok();
    match handlers::list_provider_rows(db).await {
        Ok(rows) if rows.is_empty() => {
            if let Err(e) = handlers::insert_provider_row(
                db,
                "env",
                &base_url,
                api_key.as_deref(),
                &model,
                true,
            )
            .await
            {
                tracing::warn!("failed to seed LLM provider from env: {e}");
            } else {
                tracing::info!("Seeded LLM provider from BRIGHTFLOW_LLM_* env");
            }
        },
        _ => {},
    }
}
