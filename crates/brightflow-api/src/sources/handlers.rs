use axum::extract::State;
use axum::Json;

use crate::shared::AppResult;
use crate::state::AppState;

use super::profiles::{connector_tools, web_analytics_tools};
use super::types::{SourceKind, SourceTable, UnifiedSource};

/// GET /api/sources/unified — merged list of event sources + connector sources
pub async fn list_unified_sources(
    State(state): State<AppState>,
) -> AppResult<Json<Vec<UnifiedSource>>> {
    let table_index = state.get_available_tables().await;
    let mut sources = Vec::new();

    // 1. Event sources (web analytics)
    if let Some(ingest) = &state.ingest {
        if let Ok(event_sources) = ingest.db.list_sources().await {
            for src in event_sources {
                sources.push(UnifiedSource {
                    id: format!("web:{}", src.id),
                    name: src.name.clone(),
                    kind: SourceKind::WebAnalytics,
                    connector_name: None,
                    domain: Some(src.domain.clone()),
                    tables: Vec::new(),
                    tools: web_analytics_tools(),
                    created_at: src.created_at.clone(),
                    ready: true,
                });
            }
        }
    }

    // 2. Connector sources
    if let Some(scheduler_db) = &state.scheduler_db {
        if let Ok(configs) = scheduler_db.list_connector_configs().await {
            for config in configs {
                // Fetch sync states to find table names (each endpoint IS the table name)
                let sync_states = scheduler_db
                    .list_sync_states(&config.id)
                    .await
                    .unwrap_or_default();

                let tables: Vec<SourceTable> = sync_states
                    .iter()
                    .map(|ss| {
                        let num_rows = table_index
                            .iter()
                            .find(|t| t.name == ss.endpoint)
                            .and_then(|t| t.num_rows);

                        SourceTable {
                            name: ss.endpoint.clone(),
                            num_rows,
                        }
                    })
                    .collect();

                let ready = sync_states
                    .iter()
                    .any(|ss| ss.last_sync_status.as_deref() == Some("completed"));

                // Extract connector name from path (last segment)
                let connector_name = config
                    .connector_path
                    .rsplit('/')
                    .next()
                    .unwrap_or(&config.connector_path)
                    .to_string();

                let tools = connector_tools(&connector_name);

                sources.push(UnifiedSource {
                    id: format!("connector:{}", config.id),
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

    Ok(Json(sources))
}
