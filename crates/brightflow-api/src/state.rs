use crate::analytics::session::{DatasetData, DatasetManager, DatasetSource};
use crate::shared::AppResult;
use crate::system::log_layer::LogEntry;
use crate::system::sampler::SystemSnapshot;
use brightflow_engine::data::config::{ColumnRole, TimeGranularity};
use brightflow_engine::data::merge::{build_schema, ColumnOverride, TableSettingsOverride};
use brightflow_engine::data::schema::DataSchema;
use brightflow_scheduler::Scheduler;
use brightflow_store::{ColumnSemanticRow, ParquetStore, TableAnalysisSettingsRow, TableInfo};
use dashmap::DashMap;
use polars::prelude::*;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, RwLock};

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    /// Thread-safe storage for loaded datasets
    pub datasets: DatasetManager,
    /// Index of available Parquet tables (metadata only, no data loaded)
    pub table_index: Arc<RwLock<Vec<TableInfo>>>,
    /// Reference to the Parquet store for lazy loading
    store: Option<Arc<ParquetStore>>,
    /// Schema cache keyed by table name (invalidated on override changes)
    pub schemas: Arc<DashMap<String, DataSchema>>,
    /// Column semantic overrides keyed by table name
    pub schema_overrides: Arc<DashMap<String, Vec<ColumnOverride>>>,
    /// Table analysis settings overrides keyed by table name
    pub settings_overrides: Arc<DashMap<String, TableSettingsOverride>>,
    /// Per-table enrichment overrides, keyed by `cache_key(source_id, table)`.
    /// Hydrated from `table_enrichment_settings` at startup, updated by the
    /// enrichment settings endpoints.
    pub enrichment_overrides:
        Arc<DashMap<String, brightflow_engine::enrichment::EnrichmentOverrides>>,
    /// Text Explorer indexes keyed by `cache_key(source_id, table)`.
    /// Version-checked against `tables.version` on every request; bounded
    /// eviction lives in `textexplore::index`.
    pub text_indexes: Arc<DashMap<String, Arc<crate::textexplore::index::TextIndex>>>,
    /// Abort handles for in-flight agent runs, keyed by run id.
    pub agent_runs: Arc<DashMap<i64, tokio::task::AbortHandle>>,
    /// Abort handles for in-flight enrichment runs, keyed by run id.
    pub enrichment_jobs: Arc<DashMap<String, tokio::task::AbortHandle>>,
    /// Optional scheduler for background jobs
    pub scheduler: Option<Arc<Scheduler>>,
    /// Authentication database
    pub auth_db: Option<Arc<crate::auth::AuthDb>>,
    /// Scheduler database
    pub scheduler_db: Option<Arc<brightflow_scheduler::SchedulerDb>>,
    /// Live system metrics snapshot (updated by background sampler)
    pub system_metrics: Arc<RwLock<SystemSnapshot>>,
    /// Broadcast sender for log entries (from custom tracing Layer)
    pub log_sender: broadcast::Sender<LogEntry>,
    /// Broadcast sender for curation events (action log + agent runs),
    /// fanned out to every `/api/ws` client. Capacity is deliberately modest:
    /// a lagged subscriber gets an `ActionResync` and refetches.
    pub curation_events: broadcast::Sender<crate::actions::events::CurationEvent>,
    /// Server start time (for uptime calculation)
    pub start_time: Instant,
    /// Event ingestion engine (sources, buffer, geo, UA parser)
    pub ingest: Option<Arc<crate::ingest::IngestState>>,
    /// Workspace paths for connector discovery and output
    pub paths: Option<brightflow_core::WorkspacePaths>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    /// Create new AppState with empty dataset manager
    pub fn new() -> Self {
        let (log_sender, _) = broadcast::channel(1000);
        Self {
            datasets: DatasetManager::new(),
            table_index: Arc::new(RwLock::new(Vec::new())),
            store: None,
            schemas: Arc::new(DashMap::new()),
            schema_overrides: Arc::new(DashMap::new()),
            settings_overrides: Arc::new(DashMap::new()),
            enrichment_overrides: Arc::new(DashMap::new()),
            text_indexes: Arc::new(DashMap::new()),
            agent_runs: Arc::new(DashMap::new()),
            enrichment_jobs: Arc::new(DashMap::new()),
            scheduler: None,

            auth_db: None,
            scheduler_db: None,
            system_metrics: Arc::new(RwLock::new(SystemSnapshot::default())),
            log_sender,
            curation_events: broadcast::channel(1024).0,
            start_time: Instant::now(),
            ingest: None,
            paths: None,
        }
    }

    /// Create new AppState with an existing log broadcast sender (from init_tracing).
    pub fn with_log_sender(log_sender: broadcast::Sender<LogEntry>) -> Self {
        Self {
            datasets: DatasetManager::new(),
            table_index: Arc::new(RwLock::new(Vec::new())),
            store: None,
            schemas: Arc::new(DashMap::new()),
            schema_overrides: Arc::new(DashMap::new()),
            settings_overrides: Arc::new(DashMap::new()),
            enrichment_overrides: Arc::new(DashMap::new()),
            text_indexes: Arc::new(DashMap::new()),
            agent_runs: Arc::new(DashMap::new()),
            enrichment_jobs: Arc::new(DashMap::new()),
            scheduler: None,

            auth_db: None,
            scheduler_db: None,
            system_metrics: Arc::new(RwLock::new(SystemSnapshot::default())),
            log_sender,
            curation_events: broadcast::channel(1024).0,
            start_time: Instant::now(),
            ingest: None,
            paths: None,
        }
    }

    /// Load column semantic overrides from SQLite into memory (DashMaps).
    pub async fn load_overrides_from_store(&self) {
        let Some(store) = &self.store else {
            return;
        };

        let tables = match store.list_tables().await {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!("Failed to list tables for override loading: {e}");
                return;
            },
        };

        for table_ref in tables {
            let key = cache_key(&table_ref.source_id, &table_ref.name);

            match store
                .get_column_semantics(&table_ref.source_id, &table_ref.name)
                .await
            {
                Ok(rows) if !rows.is_empty() => {
                    let overrides: Vec<ColumnOverride> =
                        rows.iter().filter_map(convert_semantic_row).collect();
                    let kpi_count = overrides.iter().filter(|o| o.is_kpi).count();
                    tracing::info!(
                        "Loaded {} column overrides for '{}/{}' ({} KPIs)",
                        overrides.len(),
                        table_ref.source_id,
                        table_ref.name,
                        kpi_count,
                    );
                    self.schema_overrides.insert(key.clone(), overrides);
                },
                Ok(_) => {},
                Err(e) => {
                    tracing::warn!(
                        "Failed to load column semantics for '{}/{}': {e}",
                        table_ref.source_id,
                        table_ref.name
                    );
                },
            }

            match store
                .get_table_settings(&table_ref.source_id, &table_ref.name)
                .await
            {
                Ok(Some(row)) => {
                    if let Some(settings) = convert_settings_row(&row) {
                        self.settings_overrides.insert(key, settings);
                    }
                },
                Ok(None) => {},
                Err(e) => {
                    tracing::warn!(
                        "Failed to load table settings for '{}/{}': {e}",
                        table_ref.source_id,
                        table_ref.name
                    );
                },
            }
        }
    }

    /// Get a cached schema, or build one on-demand from a DataFrame + overrides.
    pub fn get_or_build_schema(
        &self,
        source_id: &str,
        table_name: &str,
        df: &DataFrame,
    ) -> Result<DataSchema, anyhow::Error> {
        let key = cache_key(source_id, table_name);
        if let Some(cached) = self.schemas.get(&key) {
            return Ok(cached.clone());
        }

        let overrides = self
            .schema_overrides
            .get(&key)
            .map(|v| v.value().clone())
            .unwrap_or_default();
        let settings = self.settings_overrides.get(&key).map(|v| v.value().clone());

        let schema = build_schema(df, &overrides, settings.as_ref())?;
        self.schemas.insert(key, schema.clone());
        Ok(schema)
    }

    /// Invalidate the cached schema for a composite key (call after override mutations).
    pub fn invalidate_schema_cache(&self, key: &str) {
        self.schemas.remove(key);
    }

    /// Get a reference to the Parquet store
    pub fn store(&self) -> Option<&Arc<ParquetStore>> {
        self.store.as_ref()
    }

    /// Re-read table metadata from the store and update the index
    pub async fn refresh_table_index(&self) {
        if let Some(store) = &self.store {
            let tables = store.list_tables().await.unwrap_or_default();
            let mut index = Vec::with_capacity(tables.len());
            for table_ref in tables {
                if let Ok(info) = store
                    .table_info(&table_ref.source_id, &table_ref.name)
                    .await
                {
                    index.push(info);
                }
            }
            tracing::info!("Refreshed table index: {} tables", index.len());
            *self.table_index.write().await = index;
        }
    }

    /// Create AppState with a Parquet store - loads metadata only, no data
    pub async fn with_store(store: ParquetStore) -> Self {
        let tables = store.list_tables().await.unwrap_or_default();

        // Load metadata for each table (does NOT load actual data)
        let mut index = Vec::with_capacity(tables.len());
        for table_ref in tables {
            if let Ok(info) = store
                .table_info(&table_ref.source_id, &table_ref.name)
                .await
            {
                index.push(info);
            }
        }

        tracing::info!(
            "Indexed {} tables (metadata only, no data loaded)",
            index.len()
        );

        let (log_sender, _) = broadcast::channel(1000);
        Self {
            datasets: DatasetManager::new(),
            table_index: Arc::new(RwLock::new(index)),
            store: Some(Arc::new(store)),
            schemas: Arc::new(DashMap::new()),
            schema_overrides: Arc::new(DashMap::new()),
            settings_overrides: Arc::new(DashMap::new()),
            enrichment_overrides: Arc::new(DashMap::new()),
            text_indexes: Arc::new(DashMap::new()),
            agent_runs: Arc::new(DashMap::new()),
            enrichment_jobs: Arc::new(DashMap::new()),
            scheduler: None,

            auth_db: None,
            scheduler_db: None,
            system_metrics: Arc::new(RwLock::new(SystemSnapshot::default())),
            log_sender,
            curation_events: broadcast::channel(1024).0,
            start_time: Instant::now(),
            ingest: None,
            paths: None,
        }
    }

    /// Create AppState with a Parquet store and an existing log broadcast sender.
    pub async fn with_store_and_log_sender(
        store: ParquetStore,
        log_sender: broadcast::Sender<LogEntry>,
    ) -> Self {
        let tables = store.list_tables().await.unwrap_or_default();

        let mut index = Vec::with_capacity(tables.len());
        for table_ref in tables {
            if let Ok(info) = store
                .table_info(&table_ref.source_id, &table_ref.name)
                .await
            {
                index.push(info);
            }
        }

        tracing::info!(
            "Indexed {} tables (metadata only, no data loaded)",
            index.len()
        );

        Self {
            datasets: DatasetManager::new(),
            table_index: Arc::new(RwLock::new(index)),
            store: Some(Arc::new(store)),
            schemas: Arc::new(DashMap::new()),
            schema_overrides: Arc::new(DashMap::new()),
            settings_overrides: Arc::new(DashMap::new()),
            enrichment_overrides: Arc::new(DashMap::new()),
            text_indexes: Arc::new(DashMap::new()),
            agent_runs: Arc::new(DashMap::new()),
            enrichment_jobs: Arc::new(DashMap::new()),
            scheduler: None,

            auth_db: None,
            scheduler_db: None,
            system_metrics: Arc::new(RwLock::new(SystemSnapshot::default())),
            log_sender,
            curation_events: broadcast::channel(1024).0,
            start_time: Instant::now(),
            ingest: None,
            paths: None,
        }
    }

    /// Create AppState and load a default dataset from CSV
    pub async fn with_default_dataset(csv_path: &str) -> AppResult<Self> {
        let state = Self::new();

        let path = csv_path.to_string();
        let df = tokio::task::spawn_blocking(move || -> Result<DataFrame, PolarsError> {
            CsvReadOptions::default()
                .with_infer_schema_length(Some(10000))
                .try_into_reader_with_file_path(Some(Path::new(&path).into()))?
                .finish()
        })
        .await??;

        let name = Path::new(csv_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("default")
            .to_string();

        state
            .datasets
            .add_dataset(name, DatasetData::Uploaded(df), DatasetSource::Default);

        Ok(state)
    }

    /// Create AppState with both a default CSV dataset and store metadata
    pub async fn with_default_and_store(csv_path: &str, store: ParquetStore) -> AppResult<Self> {
        // First create with store (metadata only)
        let state = Self::with_store(store).await;

        // Then load the default CSV dataset
        let path = csv_path.to_string();
        let df = tokio::task::spawn_blocking(move || -> Result<DataFrame, PolarsError> {
            CsvReadOptions::default()
                .with_infer_schema_length(Some(10000))
                .try_into_reader_with_file_path(Some(Path::new(&path).into()))?
                .finish()
        })
        .await??;

        let name = Path::new(csv_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("default")
            .to_string();

        state
            .datasets
            .add_dataset(name, DatasetData::Uploaded(df), DatasetSource::Default);

        Ok(state)
    }

    /// Get the list of available tables (metadata only). Queries the store live
    /// so freshly-synced connector tables show up without a server restart.
    pub async fn get_available_tables(&self) -> Vec<TableInfo> {
        let Some(store) = &self.store else {
            return self.table_index.read().await.clone();
        };
        let refs = store.list_tables().await.unwrap_or_default();
        let mut out = Vec::with_capacity(refs.len());
        for r in refs {
            if let Ok(info) = store.table_info(&r.source_id, &r.name).await {
                out.push(info);
            }
        }
        (*self.table_index.write().await).clone_from(&out);
        out
    }

    /// Load a specific table on-demand (lazy parquet scan).
    ///
    /// This unloads any previously loaded store tables first.
    pub async fn load_table(&self, source_id: &str, table_name: &str) -> AppResult<String> {
        let store = self.store.as_ref().ok_or_else(|| {
            crate::shared::AppError::BadRequest("No store configured".to_string())
        })?;

        // Unload any existing store tables
        self.unload_store_tables();

        let source = DatasetSource::StoreTable {
            table_name: table_name.to_string(),
            version: -1,
        };

        tracing::info!(
            "Loading table '{}/{}' (lazy parquet scan)",
            source_id,
            table_name
        );
        let files = store.get_table_parquet_paths(source_id, table_name).await?;
        tracing::info!(
            "Registered {} parquet file(s) for '{}/{}'",
            files.len(),
            source_id,
            table_name
        );
        let id = self.datasets.add_dataset(
            table_name.to_string(),
            DatasetData::Parquet { files },
            source,
        );

        Ok(id)
    }

    /// Unload all store tables from memory (keeps "default" and uploaded datasets)
    pub fn unload_store_tables(&self) {
        let to_remove: Vec<_> = self
            .datasets
            .list_datasets()
            .iter()
            .filter(|d| d.id.starts_with("store:"))
            .map(|d| d.id.clone())
            .collect();

        for id in &to_remove {
            self.datasets.delete_dataset(id);
        }

        if !to_remove.is_empty() {
            tracing::info!("Unloaded {} table(s) from memory", to_remove.len());
        }
    }

    /// Check if a table exists in the index for the given source
    pub async fn table_exists(&self, source_id: &str, table_name: &str) -> bool {
        let index = self.table_index.read().await;
        index
            .iter()
            .any(|t| t.source_id == source_id && t.name == table_name)
    }

    /// Load a table directly from a store (lazy parquet scan)
    pub async fn load_store_table(
        &self,
        store: &ParquetStore,
        source_id: &str,
        table_name: &str,
    ) -> AppResult<String> {
        let files = store.get_table_parquet_paths(source_id, table_name).await?;

        let source = DatasetSource::StoreTable {
            table_name: table_name.to_string(),
            version: -1,
        };

        let id = self.datasets.add_dataset(
            table_name.to_string(),
            DatasetData::Parquet { files },
            source,
        );
        Ok(id)
    }

    /// Load all tables from a store
    pub async fn load_all_store_tables(
        &self,
        store: &ParquetStore,
    ) -> Vec<(String, Result<String, String>)> {
        let tables = match store.list_tables().await {
            Ok(t) => t,
            Err(e) => return vec![("*".to_string(), Err(e.to_string()))],
        };

        let mut results = Vec::with_capacity(tables.len());
        for table_ref in tables {
            let result = match self
                .load_store_table(store, &table_ref.source_id, &table_ref.name)
                .await
            {
                Ok(id) => Ok(id),
                Err(e) => Err(e.to_string()),
            };
            results.push((table_ref.name, result));
        }
        results
    }
}

/// Build a composite DashMap key for source/table-keyed caches.
pub(crate) fn cache_key(source_id: &str, table_name: &str) -> String {
    format!("{source_id}|{table_name}")
}

/// Convert a `ColumnSemanticRow` to a `ColumnOverride`.
fn convert_semantic_row(row: &ColumnSemanticRow) -> Option<ColumnOverride> {
    let role = match row.role.as_str() {
        "measure" => ColumnRole::Measure,
        "dimension" => ColumnRole::Dimension,
        "time" => ColumnRole::Time,
        "entity" => ColumnRole::Entity,
        "ignored" => ColumnRole::Ignored,
        other => {
            tracing::warn!("Unknown column role '{}' for '{}'", other, row.column_name);
            return None;
        },
    };
    Some(ColumnOverride {
        column_name: row.column_name.clone(),
        role,
        is_kpi: row.is_kpi,
        label: row.label.clone(),
        description: row.description.clone(),
    })
}

/// Convert a `TableAnalysisSettingsRow` to a `TableSettingsOverride`.
/// Load stored enrichment overrides into the state map at startup.
///
/// Source of truth is each table's `topic_model` enrichment function
/// (migration 016 converted every legacy `table_enrichment_settings` row
/// into one; the deprecated table is now write-only dual-write).
pub async fn hydrate_enrichment_overrides(state: &AppState, store: &ParquetStore) {
    let tables = store.list_tables().await.unwrap_or_default();
    let mut hydrated = 0;
    for t in &tables {
        let Ok(Some(row)) = store.db().get_table(&t.source_id, &t.name).await else {
            continue;
        };
        let Ok(functions) = store.db().list_enrichment_functions(&row.id).await else {
            continue;
        };
        let Some(function) = functions.into_iter().find(|f| f.kind == "topic_model") else {
            continue;
        };
        let Ok(Some(version)) = store
            .db()
            .get_enrichment_function_version(&function.id, function.current_version)
            .await
        else {
            continue;
        };
        let Ok(brightflow_engine::enrichment::FunctionSpec::TopicModel(tm)) =
            serde_json::from_str::<brightflow_engine::enrichment::FunctionSpec>(
                &version.config_json,
            )
        else {
            tracing::warn!(
                "topic_model function {} has unreadable config — not hydrated",
                function.id
            );
            continue;
        };
        state
            .enrichment_overrides
            .insert(cache_key(&t.source_id, &t.name), tm.overrides);
        hydrated += 1;
    }
    if hydrated > 0 {
        tracing::info!("Hydrated {hydrated} table enrichment overrides from functions");
    }
}

/// Seed known TOML schema data into SQLite if `column_semantics` is empty.
pub async fn seed_column_semantics(store: &ParquetStore) {
    // Only seed if we have tables but no semantics yet
    let has_semantics = store.has_any_column_semantics().await.unwrap_or(true);
    if has_semantics {
        return;
    }

    let tables = store.list_tables().await.unwrap_or_default();
    if tables.is_empty() {
        return;
    }

    let table_names: Vec<String> = tables.iter().map(|t| t.name.clone()).collect();
    tracing::info!(
        "Seeding column semantics for {} tables: {:?}",
        table_names.len(),
        table_names
    );

    // Define seed data for known GitHub tables
    #[allow(clippy::type_complexity)]
    let seed_data: &[(&str, &[(&str, &str, bool)], &str, i32)] = &[
        // (table_name, [(column, role, is_kpi)], time_granularity, comparison_periods)
        (
            "issues",
            &[
                ("created_at", "time", false),
                ("comments", "measure", true),
                ("reactions_total", "measure", true),
                ("state", "dimension", false),
                ("author_association", "dimension", false),
                ("user_login", "dimension", false),
                ("label_names", "dimension", false),
                ("is_pull_request", "dimension", false),
                ("locked", "dimension", false),
                ("state_reason", "dimension", false),
                ("id", "ignored", false),
                ("number", "ignored", false),
                ("title", "ignored", false),
                ("body", "ignored", false),
                ("html_url", "ignored", false),
                ("updated_at", "ignored", false),
                ("closed_at", "ignored", false),
                ("user_id", "ignored", false),
                ("assignee_logins", "ignored", false),
                ("milestone_number", "ignored", false),
                ("milestone_title", "ignored", false),
                ("active_lock_reason", "ignored", false),
            ],
            "week",
            4,
        ),
        (
            "pull_requests",
            &[
                ("created_at", "time", false),
                ("additions", "measure", true),
                ("deletions", "measure", true),
                ("changed_files", "measure", true),
                ("commits", "measure", false),
                ("comments", "measure", false),
                ("review_comments", "measure", false),
                ("state", "dimension", false),
                ("draft", "dimension", false),
                ("author_association", "dimension", false),
                ("user_login", "dimension", false),
                ("base_ref", "dimension", false),
                ("merged_by_login", "dimension", false),
                ("label_names", "dimension", false),
                ("id", "ignored", false),
                ("number", "ignored", false),
                ("title", "ignored", false),
                ("body", "ignored", false),
                ("html_url", "ignored", false),
                ("updated_at", "ignored", false),
                ("closed_at", "ignored", false),
                ("merged_at", "ignored", false),
                ("head_ref", "ignored", false),
                ("head_sha", "ignored", false),
                ("base_sha", "ignored", false),
                ("merge_commit_sha", "ignored", false),
                ("milestone_title", "ignored", false),
                ("reviewer_logins", "ignored", false),
                ("user_id", "ignored", false),
            ],
            "week",
            4,
        ),
        (
            "issue_comments",
            &[
                ("created_at", "time", false),
                ("reactions_total", "measure", true),
                ("author_association", "dimension", false),
                ("user_login", "dimension", false),
                ("id", "ignored", false),
                ("issue_number", "ignored", false),
                ("body", "ignored", false),
                ("html_url", "ignored", false),
                ("updated_at", "ignored", false),
                ("user_id", "ignored", false),
            ],
            "week",
            4,
        ),
    ];

    let db = store.db();

    // Seed data is keyed by table name only; apply each preset to every matching
    // (source_id, table_name) pair so two GitHub presets both get sensible defaults.
    let seed_map: std::collections::HashMap<&str, (_, _, _)> = seed_data
        .iter()
        .map(|(name, columns, granularity, periods)| (*name, (*columns, *granularity, *periods)))
        .collect();

    for table_ref in &tables {
        let Some((columns, granularity, periods)) = seed_map.get(table_ref.name.as_str()) else {
            continue;
        };
        let Ok(Some(table)) = db.get_table(&table_ref.source_id, &table_ref.name).await else {
            continue;
        };

        let rows: Vec<ColumnSemanticRow> = columns
            .iter()
            .map(|(col, role, is_kpi)| ColumnSemanticRow {
                table_id: table.id.clone(),
                column_name: (*col).to_string(),
                role: (*role).to_string(),
                is_kpi: *is_kpi,
                label: None,
                description: None,
                updated_at: String::new(),
            })
            .collect();

        if let Err(e) = db.upsert_column_semantics_batch(&table.id, &rows).await {
            tracing::warn!(
                "Failed to seed column semantics for '{}/{}': {e}",
                table_ref.source_id,
                table_ref.name
            );
            continue;
        }

        if let Err(e) = db
            .upsert_table_settings(&table.id, None, None, Some(*granularity), Some(*periods))
            .await
        {
            tracing::warn!(
                "Failed to seed table settings for '{}/{}': {e}",
                table_ref.source_id,
                table_ref.name
            );
            continue;
        }

        tracing::info!(
            "Seeded {} column overrides for '{}/{}' (granularity={}, periods={})",
            columns.len(),
            table_ref.source_id,
            table_ref.name,
            granularity,
            periods
        );
    }
}

fn convert_settings_row(row: &TableAnalysisSettingsRow) -> Option<TableSettingsOverride> {
    let time_granularity = row.time_granularity.as_deref().and_then(|g| match g {
        "day" => Some(TimeGranularity::Day),
        "week" => Some(TimeGranularity::Week),
        "month" => Some(TimeGranularity::Month),
        "quarter" => Some(TimeGranularity::Quarter),
        "year" => Some(TimeGranularity::Year),
        _ => None,
    });
    let comparison_periods = row.comparison_periods.and_then(|p| usize::try_from(p).ok());

    if time_granularity.is_none() && comparison_periods.is_none() {
        return None;
    }

    Some(TableSettingsOverride {
        time_granularity,
        comparison_periods,
    })
}
