//! HTTP handlers for per-table column semantics and analysis settings.
//!
//! Auth posture: session-authenticated. Writes here change how every subsequent
//! analysis interprets a table, so they update both SQLite and the in-memory
//! override caches on `AppState` — a write that only hit the database would take
//! effect at the next restart and look like it had been ignored.

use axum::{
    extract::{Path, State},
    Json,
};

use crate::semantics::types::{
    ColumnSemantic, ColumnSemanticsResponse, TableSettings, TableSettingsResponse,
};
use crate::shared::{AppError, AppResult};
use crate::state::{cache_key, AppState};

use brightflow_engine::data::config::TimeGranularity;
use brightflow_engine::data::merge::TableSettingsOverride;

/// GET /api/sources/{source_id}/tables/{name}/semantics — list all overrides for a table
pub async fn list_semantics(
    State(state): State<AppState>,
    Path((source_id, name)): Path<(String, String)>,
) -> AppResult<Json<ColumnSemanticsResponse>> {
    let store = state.require_store()?;

    let rows = store.get_column_semantics(&source_id, &name).await?;

    let columns: Vec<ColumnSemantic> = rows
        .into_iter()
        .map(|r| ColumnSemantic {
            column_name: r.column_name,
            role: r.role,
            is_kpi: r.is_kpi,
            polarity: r.polarity,
            label: r.label,
            description: r.description,
        })
        .collect();

    Ok(Json(ColumnSemanticsResponse {
        table_name: name,
        columns,
    }))
}
/// GET /api/sources/{source_id}/tables/{name}/settings — get table analysis settings
pub async fn get_table_settings(
    State(state): State<AppState>,
    Path((source_id, name)): Path<(String, String)>,
) -> AppResult<Json<TableSettingsResponse>> {
    let store = state.require_store()?;

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
    let store = state.require_store()?;

    // Validate time_granularity if provided
    if let Some(ref g) = req.time_granularity {
        if TimeGranularity::parse(g).is_none() {
            let allowed: Vec<&str> = TimeGranularity::ALL
                .iter()
                .map(|allowed| allowed.as_str())
                .collect();
            return Err(AppError::BadRequest(format!(
                "Invalid time_granularity '{g}'. Must be one of: {}",
                allowed.join(", ")
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

    // Update the in-memory settings so the next analysis run sees them.
    let key = cache_key(&source_id, &name);
    let time_granularity = req
        .time_granularity
        .as_deref()
        .and_then(TimeGranularity::parse);
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
