//! LLM provider CRUD + connection test.

use axum::extract::{Path, State};
use axum::Json;
use brightflow_llm::ChatClient;
use brightflow_store::rusqlite::params;

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
    db.pool()
        .call(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, base_url, api_key, model, is_default FROM llm_providers ORDER BY id",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(ProviderRow {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    base_url: r.get(2)?,
                    api_key: r.get(3)?,
                    model: r.get(4)?,
                    is_default: r.get::<_, i64>(5)? != 0,
                })
            })?;
            rows.collect()
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))
}

pub async fn insert_provider_row(
    db: &AuthDb,
    name: &str,
    base_url: &str,
    api_key: Option<&str>,
    model: &str,
    is_default: bool,
) -> AppResult<i64> {
    let name = name.to_owned();
    let base_url = base_url.to_owned();
    let api_key = api_key.map(str::to_owned);
    let model = model.to_owned();
    db.pool()
        .call(move |conn| {
            if is_default {
                conn.execute("UPDATE llm_providers SET is_default = 0", [])?;
            }
            conn.execute(
                r"INSERT INTO llm_providers (name, base_url, api_key, model, is_default)
                  VALUES (?, ?, ?, ?, ?)
                  ON CONFLICT (name) DO UPDATE SET
                    base_url = excluded.base_url,
                    api_key = COALESCE(excluded.api_key, llm_providers.api_key),
                    model = excluded.model,
                    is_default = excluded.is_default",
                params![name, base_url, api_key, model, is_default],
            )?;
            // Same connection as the insert, so the rowid is the one above.
            Ok(conn.last_insert_rowid())
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))
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
    db.pool()
        .call(move |conn| {
            conn.execute("DELETE FROM llm_providers WHERE id = ?", params![id])
                .map(|_| ())
        })
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
