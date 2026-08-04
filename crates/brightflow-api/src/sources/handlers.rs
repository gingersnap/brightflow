//! HTTP handlers for the unified source list.
//!
//! Auth posture: session-authenticated, read-only.

use crate::shared::AppResult;
use crate::state::AppState;
use axum::extract::State;
use axum::Json;

use super::profiles::{connector_tools, upload_tools, web_analytics_tools};
use super::types::{SourceKind, SourceTable, UnifiedSource};

/// GET /api/sources/unified — merged list of event sources + connector sources
pub async fn list_unified_sources(
    State(state): State<AppState>,
) -> AppResult<Json<Vec<UnifiedSource>>> {
    let mut sources = Vec::new();

    // 1. Event sources (web analytics)
    if let Some(ingest) = &state.ingest {
        if let Ok(event_sources) = ingest.db.list_sources().await {
            for src in event_sources {
                let source_id_key = brightflow_core::web_source_id(&src.id);
                let tables: Vec<SourceTable> = match state.store() {
                    Some(store) => store
                        .list_tables_by_source(&source_id_key)
                        .await
                        .unwrap_or_default()
                        .iter()
                        .map(|t| SourceTable {
                            name: t.name.clone(),
                            num_rows: Some(t.total_rows),
                            enrichable: crate::shared::schema_has_text_column(
                                t.schema_json.as_deref(),
                            ),
                        })
                        .collect(),
                    None => Vec::new(),
                };
                let ready = !tables.is_empty();
                sources.push(UnifiedSource {
                    id: source_id_key,
                    name: src.name.clone(),
                    kind: SourceKind::WebAnalytics,
                    connector_name: None,
                    domain: Some(src.domain.clone()),
                    tables,
                    tools: web_analytics_tools(),
                    created_at: src.created_at.clone(),
                    ready,
                });
            }
        }
    }

    // 2. Connector sources
    if let Some(scheduler_db) = &state.scheduler_db {
        if let Ok(configs) = scheduler_db.list_connector_configs().await {
            for config in configs {
                let source_id_key = brightflow_core::connector_source_id(&config.id);

                let tables: Vec<SourceTable> = match state.store() {
                    Some(store) => store
                        .list_tables_by_source(&source_id_key)
                        .await
                        .unwrap_or_default()
                        .iter()
                        .map(|t| SourceTable {
                            name: t.name.clone(),
                            num_rows: Some(t.total_rows),
                            enrichable: crate::shared::schema_has_text_column(
                                t.schema_json.as_deref(),
                            ),
                        })
                        .collect(),
                    None => Vec::new(),
                };
                let ready = !tables.is_empty();

                // Extract connector name from path (last segment)
                let connector_name = config
                    .connector_path
                    .rsplit('/')
                    .next()
                    .unwrap_or(&config.connector_path)
                    .to_string();

                let tools = connector_tools(&connector_name);

                sources.push(UnifiedSource {
                    id: source_id_key,
                    name: config.name.clone(),
                    kind: SourceKind::Connector,
                    connector_name: Some(connector_name),
                    domain: None,
                    tables,
                    tools,
                    created_at: config.created_at.clone(),
                    ready,
                });
            }
        }
    }

    // 3. Persistent CSV uploads
    if let Some(store) = state.store() {
        if let Ok(uploads) = store.db().list_registered_sources("upload").await {
            for src in uploads {
                let tables: Vec<SourceTable> = store
                    .list_tables_by_source(&src.source_id)
                    .await
                    .unwrap_or_default()
                    .iter()
                    .map(|t| SourceTable {
                        name: t.name.clone(),
                        num_rows: Some(t.total_rows),
                        enrichable: crate::shared::schema_has_text_column(t.schema_json.as_deref()),
                    })
                    .collect();
                let ready = !tables.is_empty();
                sources.push(UnifiedSource {
                    id: src.source_id.clone(),
                    name: src.name.clone(),
                    kind: SourceKind::Upload,
                    connector_name: None,
                    domain: None,
                    tables,
                    tools: upload_tools(),
                    created_at: src.created_at.clone(),
                    ready,
                });
            }
        }
    }

    Ok(Json(sources))
}
