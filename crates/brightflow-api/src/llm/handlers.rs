//! LLM provider CRUD + connection test.

use axum::extract::{Path, State};
use axum::Json;
use brightflow_llm::ChatClient;
use sqlx::Row;

use crate::auth::AuthDb;
use crate::llm::types::{LlmProviderResponse, LlmTestResponse, UpsertLlmProviderRequest};
use crate::shared::{AppError, AppResult};
use crate::state::AppState;

/// Raw provider row (internal — carries the key).
#[derive(Debug, Clone)]
pub struct ProviderRow {
    pub id: i64,
    pub name: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
    pub is_default: bool,
}

pub async fn list_provider_rows(db: &AuthDb) -> AppResult<Vec<ProviderRow>> {
    let rows = sqlx::query(
        "SELECT id, name, base_url, api_key, model, is_default FROM llm_providers ORDER BY id",
    )
    .fetch_all(db.pool())
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(rows
        .into_iter()
        .map(|r| ProviderRow {
            id: r.get(0),
            name: r.get(1),
            base_url: r.get(2),
            api_key: r.get(3),
            model: r.get(4),
            is_default: r.get::<i64, _>(5) != 0,
        })
        .collect())
}

pub async fn insert_provider_row(
    db: &AuthDb,
    name: &str,
    base_url: &str,
    api_key: Option<&str>,
    model: &str,
    is_default: bool,
) -> AppResult<i64> {
    if is_default {
        sqlx::query("UPDATE llm_providers SET is_default = 0")
            .execute(db.pool())
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
    }
    let result = sqlx::query(
        r"INSERT INTO llm_providers (name, base_url, api_key, model, is_default)
          VALUES (?, ?, ?, ?, ?)
          ON CONFLICT (name) DO UPDATE SET
            base_url = excluded.base_url,
            api_key = COALESCE(excluded.api_key, llm_providers.api_key),
            model = excluded.model,
            is_default = excluded.is_default",
    )
    .bind(name)
    .bind(base_url)
    .bind(api_key)
    .bind(model)
    .bind(is_default)
    .execute(db.pool())
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(result.last_insert_rowid())
}

fn auth_db(state: &AppState) -> AppResult<&AuthDb> {
    state
        .auth_db
        .as_deref()
        .ok_or_else(|| AppError::BadRequest("no database configured".to_string()))
}

fn redact(row: ProviderRow) -> LlmProviderResponse {
    LlmProviderResponse {
        id: row.id,
        name: row.name,
        base_url: row.base_url,
        has_api_key: row.api_key.as_deref().is_some_and(|k| !k.is_empty()),
        model: row.model,
        is_default: row.is_default,
    }
}

/// `GET /api/llm/providers`
pub async fn list_providers(
    State(state): State<AppState>,
) -> AppResult<Json<Vec<LlmProviderResponse>>> {
    let rows = list_provider_rows(auth_db(&state)?).await?;
    Ok(Json(rows.into_iter().map(redact).collect()))
}

/// `POST /api/llm/providers` — create or update by name.
pub async fn upsert_provider(
    State(state): State<AppState>,
    Json(req): Json<UpsertLlmProviderRequest>,
) -> AppResult<Json<Vec<LlmProviderResponse>>> {
    if req.name.trim().is_empty() || req.base_url.trim().is_empty() || req.model.trim().is_empty() {
        return Err(AppError::BadRequest(
            "name, baseUrl, and model are required".to_string(),
        ));
    }
    let db = auth_db(&state)?;
    insert_provider_row(
        db,
        req.name.trim(),
        req.base_url.trim(),
        req.api_key.as_deref(),
        req.model.trim(),
        req.is_default,
    )
    .await?;
    let rows = list_provider_rows(db).await?;
    Ok(Json(rows.into_iter().map(redact).collect()))
}

/// `DELETE /api/llm/providers/{id}`
pub async fn delete_provider(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<Vec<LlmProviderResponse>>> {
    let db = auth_db(&state)?;
    sqlx::query("DELETE FROM llm_providers WHERE id = ?")
        .bind(id)
        .execute(db.pool())
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let rows = list_provider_rows(db).await?;
    Ok(Json(rows.into_iter().map(redact).collect()))
}

/// `POST /api/llm/providers/{id}/test` — one-token ping.
pub async fn test_provider(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<LlmTestResponse>> {
    let rows = list_provider_rows(auth_db(&state)?).await?;
    let provider = rows
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| AppError::NotFound(format!("provider {id} not found")))?;
    let client = ChatClient::new(provider.base_url, provider.api_key, provider.model);
    match client.ping().await {
        Ok(()) => Ok(Json(LlmTestResponse {
            ok: true,
            error: None,
        })),
        Err(e) => Ok(Json(LlmTestResponse {
            ok: false,
            error: Some(e.to_string()),
        })),
    }
}
