//! The read side of models: one list per source. Writes go through the
//! action bus (`actions::exec::models`), so the UI learns about a model
//! from here and changes it from there.

use axum::{
    extract::{Path, State},
    Json,
};
use serde::Serialize;
use ts_rs::TS;

use brightflow_types::ModelRecipe;

use crate::shared::{AppError, AppResult};
use crate::state::AppState;

/// A model as a listing shows it: which table it is, what it is built from,
/// and how its last build went.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ModelSummary {
    pub id: String,
    /// The output table's name.
    pub table: String,
    /// The input table's name; absent once the input has been deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub input_table: Option<String>,
    #[ts(type = "number")]
    pub version: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub created_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_build: Option<ModelBuildSummary>,
    /// The current recipe, so the UI can describe the model.
    pub recipe: ModelRecipe,
    /// The Explore snapshot the current recipe was captured from, when the
    /// builder made it; absent for a recipe an agent wrote.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "unknown")]
    pub client_spec: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ModelBuildSummary {
    /// running | completed | failed
    pub status: String,
    #[ts(type = "number")]
    pub started_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub finished_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub rows: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
}

/// GET /api/sources/{source_id}/models
pub async fn list_models(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
) -> AppResult<Json<Vec<ModelSummary>>> {
    let store = state.require_store()?;
    let db = store.db();
    let mut out = Vec::new();
    for m in db.list_models_for_source(&source_id).await? {
        let version = db
            .get_model_version(&m.id, m.current_version)
            .await?
            .ok_or_else(|| {
                AppError::Internal(format!("model '{}' has no current version", m.id))
            })?;
        let recipe: ModelRecipe = serde_json::from_str(&version.recipe_json)
            .map_err(|e| AppError::Internal(format!("stored recipe is not readable: {e}")))?;
        let client_spec = version
            .client_spec
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());
        let last_build = db
            .latest_model_build(&m.id)
            .await?
            .map(|b| ModelBuildSummary {
                status: b.status,
                started_at: b.started_at,
                finished_at: b.finished_at,
                rows: b.rows,
                error: b.error,
            });
        out.push(ModelSummary {
            id: m.id,
            table: m.output_table,
            input_table: m.input_table,
            version: m.current_version,
            created_by: m.created_by,
            last_build,
            recipe,
            client_spec,
        });
    }
    Ok(Json(out))
}
