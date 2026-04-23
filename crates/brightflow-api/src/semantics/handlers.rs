use axum::{
    extract::{Path, State},
    Json,
};

use crate::semantics::types::{
    BulkColumnSemanticsRequest, ColumnSemantic, ColumnSemanticsResponse, TableSettings,
    TableSettingsResponse,
};
use crate::shared::{AppError, AppResult};
use crate::state::{cache_key, AppState};

use brightflow_engine::data::config::{ColumnRole, TimeGranularity};
use brightflow_engine::data::merge::{ColumnOverride, TableSettingsOverride};

const VALID_ROLES: &[&str] = &["measure", "dimension", "time", "entity", "ignored"];

/// GET /api/sources/{source_id}/tables/{name}/semantics — list all overrides for a table
pub async fn list_semantics(
    State(state): State<AppState>,
    Path((source_id, name)): Path<(String, String)>,
) -> AppResult<Json<ColumnSemanticsResponse>> {
    let store = state
        .store()
        .ok_or_else(|| AppError::BadRequest("No store configured".into()))?;

    let rows = store.get_column_semantics(&source_id, &name).await?;

    let columns: Vec<ColumnSemantic> = rows
        .into_iter()
        .map(|r| ColumnSemantic {
            column_name: r.column_name,
            role: r.role,
            is_kpi: r.is_kpi,
            label: r.label,
            description: r.description,
        })
        .collect();

    Ok(Json(ColumnSemanticsResponse {
        table_name: name,
        columns,
    }))
}

/// PUT /api/sources/{source_id}/tables/{name}/semantics — bulk upsert overrides
pub async fn bulk_upsert_semantics(
    State(state): State<AppState>,
    Path((source_id, name)): Path<(String, String)>,
    Json(req): Json<BulkColumnSemanticsRequest>,
) -> AppResult<Json<ColumnSemanticsResponse>> {
    let store = state
        .store()
        .ok_or_else(|| AppError::BadRequest("No store configured".into()))?;

    // Validate roles
    for col in &req.columns {
        if !VALID_ROLES.contains(&col.role.as_str()) {
            return Err(AppError::BadRequest(format!(
                "Invalid role '{}' for column '{}'. Must be one of: {}",
                col.role,
                col.column_name,
                VALID_ROLES.join(", ")
            )));
        }
    }

    // Resolve table_id
    let table = store
        .db()
        .get_table(&source_id, &name)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Table '{name}' not found")))?;

    // Batch upsert
    let rows: Vec<brightflow_store::ColumnSemanticRow> = req
        .columns
        .iter()
        .map(|c| brightflow_store::ColumnSemanticRow {
            table_id: table.id.clone(),
            column_name: c.column_name.clone(),
            role: c.role.clone(),
            is_kpi: c.is_kpi,
            label: c.label.clone(),
            description: c.description.clone(),
            updated_at: String::new(), // ignored by upsert
        })
        .collect();

    store
        .db()
        .upsert_column_semantics_batch(&table.id, &rows)
        .await?;

    // Invalidate cache and update in-memory overrides
    let key = cache_key(&source_id, &name);
    state.invalidate_schema_cache(&key);
    let overrides: Vec<ColumnOverride> = req
        .columns
        .iter()
        .filter_map(|c| {
            let role = parse_role(&c.role)?;
            Some(ColumnOverride {
                column_name: c.column_name.clone(),
                role,
                is_kpi: c.is_kpi,
                label: c.label.clone(),
                description: c.description.clone(),
            })
        })
        .collect();
    state.schema_overrides.insert(key, overrides);

    Ok(Json(ColumnSemanticsResponse {
        table_name: name,
        columns: req.columns,
    }))
}

/// PUT /api/sources/{source_id}/tables/{name}/semantics/{col} — upsert single column
pub async fn upsert_column_semantic(
    State(state): State<AppState>,
    Path((source_id, name, col)): Path<(String, String, String)>,
    Json(req): Json<ColumnSemantic>,
) -> AppResult<Json<ColumnSemantic>> {
    let store = state
        .store()
        .ok_or_else(|| AppError::BadRequest("No store configured".into()))?;

    if !VALID_ROLES.contains(&req.role.as_str()) {
        return Err(AppError::BadRequest(format!(
            "Invalid role '{}'. Must be one of: {}",
            req.role,
            VALID_ROLES.join(", ")
        )));
    }

    let row = store
        .upsert_column_semantic(
            &source_id,
            &name,
            &col,
            &req.role,
            req.is_kpi,
            req.label.as_deref(),
            req.description.as_deref(),
        )
        .await?;

    // Invalidate cache and update in-memory overrides
    let key = cache_key(&source_id, &name);
    state.invalidate_schema_cache(&key);
    if let Some(role) = parse_role(&req.role) {
        let mut overrides = state
            .schema_overrides
            .get(&key)
            .map(|v| v.value().clone())
            .unwrap_or_default();
        overrides.retain(|o| o.column_name != col);
        overrides.push(ColumnOverride {
            column_name: col,
            role,
            is_kpi: req.is_kpi,
            label: req.label.clone(),
            description: req.description.clone(),
        });
        state.schema_overrides.insert(key, overrides);
    }

    Ok(Json(ColumnSemantic {
        column_name: row.column_name,
        role: row.role,
        is_kpi: row.is_kpi,
        label: row.label,
        description: row.description,
    }))
}

/// DELETE /api/sources/{source_id}/tables/{name}/semantics/{col} — delete override (revert to auto)
pub async fn delete_column_semantic(
    State(state): State<AppState>,
    Path((source_id, name, col)): Path<(String, String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let store = state
        .store()
        .ok_or_else(|| AppError::BadRequest("No store configured".into()))?;

    let deleted = store
        .delete_column_semantic(&source_id, &name, &col)
        .await?;

    if !deleted {
        return Err(AppError::NotFound(format!(
            "No semantic override for column '{col}' in table '{name}'"
        )));
    }

    // Invalidate cache and update in-memory overrides
    let key = cache_key(&source_id, &name);
    state.invalidate_schema_cache(&key);
    if let Some(mut overrides) = state.schema_overrides.get_mut(&key) {
        overrides.retain(|o| o.column_name != col);
    }

    Ok(Json(serde_json::json!({ "deleted": true })))
}

/// GET /api/sources/{source_id}/tables/{name}/settings — get table analysis settings
pub async fn get_table_settings(
    State(state): State<AppState>,
    Path((source_id, name)): Path<(String, String)>,
) -> AppResult<Json<TableSettingsResponse>> {
    let store = state
        .store()
        .ok_or_else(|| AppError::BadRequest("No store configured".into()))?;

    let row = store.get_table_settings(&source_id, &name).await?;

    let settings = row.map_or_else(
        || TableSettings {
            display_name: None,
            description: None,
            time_granularity: None,
            comparison_periods: None,
        },
        |r| TableSettings {
            display_name: r.display_name,
            description: r.description,
            time_granularity: r.time_granularity,
            comparison_periods: r.comparison_periods,
        },
    );

    Ok(Json(TableSettingsResponse {
        table_name: name,
        settings,
    }))
}

/// PUT /api/sources/{source_id}/tables/{name}/settings — upsert settings
pub async fn upsert_table_settings(
    State(state): State<AppState>,
    Path((source_id, name)): Path<(String, String)>,
    Json(req): Json<TableSettings>,
) -> AppResult<Json<TableSettingsResponse>> {
    let store = state
        .store()
        .ok_or_else(|| AppError::BadRequest("No store configured".into()))?;

    // Validate time_granularity if provided
    if let Some(ref g) = req.time_granularity {
        if !["day", "week", "month", "quarter", "year"].contains(&g.as_str()) {
            return Err(AppError::BadRequest(format!(
                "Invalid time_granularity '{g}'. Must be one of: day, week, month, quarter, year"
            )));
        }
    }

    let row = store
        .upsert_table_settings(
            &source_id,
            &name,
            req.display_name.as_deref(),
            req.description.as_deref(),
            req.time_granularity.as_deref(),
            req.comparison_periods,
        )
        .await?;

    // Invalidate schema cache and update in-memory settings
    let key = cache_key(&source_id, &name);
    state.invalidate_schema_cache(&key);
    let time_granularity = req.time_granularity.as_deref().and_then(parse_granularity);
    let comparison_periods = req.comparison_periods.and_then(|p| usize::try_from(p).ok());
    if time_granularity.is_some() || comparison_periods.is_some() {
        state.settings_overrides.insert(
            key,
            TableSettingsOverride {
                time_granularity,
                comparison_periods,
            },
        );
    }

    Ok(Json(TableSettingsResponse {
        table_name: name,
        settings: TableSettings {
            display_name: row.display_name,
            description: row.description,
            time_granularity: row.time_granularity,
            comparison_periods: row.comparison_periods,
        },
    }))
}

fn parse_role(s: &str) -> Option<ColumnRole> {
    match s {
        "measure" => Some(ColumnRole::Measure),
        "dimension" => Some(ColumnRole::Dimension),
        "time" => Some(ColumnRole::Time),
        "entity" => Some(ColumnRole::Entity),
        "ignored" => Some(ColumnRole::Ignored),
        _ => None,
    }
}

fn parse_granularity(s: &str) -> Option<TimeGranularity> {
    match s {
        "day" => Some(TimeGranularity::Day),
        "week" => Some(TimeGranularity::Week),
        "month" => Some(TimeGranularity::Month),
        "quarter" => Some(TimeGranularity::Quarter),
        "year" => Some(TimeGranularity::Year),
        _ => None,
    }
}
