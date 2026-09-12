//! `AppState`: the shared handle every handler receives.
//!
//! Cheap to clone by design — every field is an `Arc`, a `DashMap`, or a
//! broadcast sender, because axum clones the state per request. Caches keyed by
//! source *and* table use `cache_key()` so a table name alone can never alias two
//! sources (the same mistake `DatasetSource::StoreTable` once made).

use crate::analytics::session::{DatasetData, DatasetManager, DatasetSource};
use crate::shared::AppResult;
use crate::system::log_layer::LogEntry;
use crate::system::sampler::SystemSnapshot;
use brightflow_engine::data::merge::{ColumnOverride, TableSettingsOverride};
use brightflow_scheduler::Scheduler;
use brightflow_store::{ParquetStore, TableInfo};
use brightflow_types::{ResolvedColumn, ResolvedTable};
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
    /// Column semantic overrides keyed by table name
    pub schema_overrides: Arc<DashMap<String, Vec<ColumnOverride>>>,
    /// Table analysis settings overrides keyed by table name
    pub settings_overrides: Arc<DashMap<String, TableSettingsOverride>>,
    /// Text Explorer index cache, keyed by `cache_key(source_id, table)`.
    /// The index module owns the caching contract (versioning + eviction);
    /// this is only the shared map it lives in.
    pub text_indexes: Arc<DashMap<String, Arc<crate::textexplore::index::TextIndex>>>,
    /// Abort handles for in-flight agent runs, keyed by run id.
    pub agent_runs: Arc<DashMap<i64, tokio::task::AbortHandle>>,
    /// Abort handles for in-flight enrichment runs, keyed by run id.
    pub enrichment_jobs: Arc<DashMap<String, tokio::task::AbortHandle>>,
    /// Per-table materialisation locks, keyed by `cache_key(source_id, table)`.
    /// Held by `enrichment::runner::materialize` for its read-rebuild-write so
    /// two functions on one table serialise instead of racing the version
    /// check. Entries are never removed: one small Arc per table.
    pub materialize_locks: Arc<DashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    /// Single-flight markers for post-sync insight auto-runs, keyed by
    /// `cache_key(source_id, table)` (see `insights::auto`).
    pub insight_auto_inflight: Arc<DashMap<String, ()>>,
    /// Optional scheduler for background jobs
    pub scheduler: Option<Arc<Scheduler>>,
    /// Authentication database
    pub auth_db: Option<Arc<crate::auth::AuthDb>>,
    /// Per-IP login token buckets (see `auth::rate_limit`). Always present —
    /// an empty limiter simply never throttles, so the login handler doesn't
    /// need an `Option` branch on a security control.
    pub login_limiter: Arc<crate::auth::LoginLimiter>,
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
            schema_overrides: Arc::new(DashMap::new()),
            settings_overrides: Arc::new(DashMap::new()),
            text_indexes: Arc::new(DashMap::new()),
            agent_runs: Arc::new(DashMap::new()),
            enrichment_jobs: Arc::new(DashMap::new()),
            materialize_locks: Arc::new(DashMap::new()),
            insight_auto_inflight: Arc::new(DashMap::new()),
            scheduler: None,

            auth_db: None,
            login_limiter: Arc::new(crate::auth::LoginLimiter::new()),
            scheduler_db: None,
            system_metrics: Arc::new(RwLock::new(SystemSnapshot::default())),
            log_sender,
            curation_events: broadcast::channel(1024).0,
            start_time: Instant::now(),
            ingest: None,
            paths: None,
        }
    }

    /// The materialisation lock for one table (created on first use).
    pub fn materialize_lock(&self, key: &str) -> Arc<tokio::sync::Mutex<()>> {
        Arc::clone(
            &self
                .materialize_locks
                .entry(key.to_string())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))),
        )
    }

    /// Load every table's resolved semantics from the store into memory.
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
            self.refresh_overrides_from_store(&table_ref.source_id, &table_ref.name)
                .await;
        }
    }

    /// Re-read one table's resolved semantics from the store and replace its
    /// in-memory overrides — columns and table settings both. Every writer of
    /// semantic rows calls this after its rows land; the database stays the
    /// source of truth.
    pub async fn refresh_overrides_from_store(&self, source_id: &str, table_name: &str) {
        let Some(store) = &self.store else {
            return;
        };
        let key = cache_key(source_id, table_name);
        match store.resolved_columns(source_id, table_name).await {
            Ok(columns) => {
                let overrides: Vec<ColumnOverride> =
                    columns.iter().map(override_from_resolved).collect();
                self.schema_overrides.insert(key.clone(), overrides);
            },
            Err(e) => tracing::warn!(
                "Failed to refresh column semantics for '{source_id}/{table_name}': {e}"
            ),
        }
        match store.resolved_table(source_id, table_name).await {
            Ok(resolved) => match resolved.as_ref().and_then(settings_from_resolved) {
                Some(settings) => {
                    self.settings_overrides.insert(key, settings);
                },
                None => {
                    self.settings_overrides.remove(&key);
                },
            },
            Err(e) => tracing::warn!(
                "Failed to refresh table settings for '{source_id}/{table_name}': {e}"
            ),
        }
    }

    /// Replace one column's in-memory semantic override for a table (keyed by
    /// `cache_key`), so the next analysis run and the next `load_table` see a
    /// write without a restart. Every writer of `column_semantics` calls this
    /// after its row lands; the database stays the source of truth.
    pub fn set_column_override(&self, key: &str, ovr: ColumnOverride) {
        let mut overrides = self
            .schema_overrides
            .get(key)
            .map(|v| v.value().clone())
            .unwrap_or_default();
        overrides.retain(|o| o.column_name != ovr.column_name);
        overrides.push(ovr);
        self.schema_overrides.insert(key.to_string(), overrides);
    }

    /// Forget one column's in-memory override — the inverse of
    /// `set_column_override`, for columns that were dropped from the table.
    pub fn remove_column_override(&self, key: &str, column: &str) {
        if let Some(mut overrides) = self.schema_overrides.get_mut(key) {
            overrides.retain(|o| o.column_name != column);
        }
    }

    /// Get a reference to the Parquet store
    pub fn store(&self) -> Option<&Arc<ParquetStore>> {
        self.store.as_ref()
    }

    /// The store, or the canonical 503 for handlers that cannot work without
    /// one. Use `store()` instead where degrading gracefully is intended.
    pub fn require_store(&self) -> AppResult<&Arc<ParquetStore>> {
        self.store
            .as_ref()
            .ok_or(crate::shared::AppError::StoreUnavailable)
    }

    /// The scheduler database, or the canonical error for handlers that
    /// cannot work without one.
    pub fn require_scheduler_db(&self) -> AppResult<&Arc<brightflow_scheduler::SchedulerDb>> {
        self.scheduler_db.as_ref().ok_or_else(|| {
            crate::shared::AppError::Internal("No scheduler database configured".to_string())
        })
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

        Self {
            table_index: Arc::new(RwLock::new(index)),
            store: Some(Arc::new(store)),
            ..Self::new()
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
    /// Previously loaded store tables are left alone. A store dataset holds only
    /// `Vec<PathBuf>` — parquet paths scanned lazily per query, not materialized frames
    /// — so keeping several resident costs a path vector each, and evicting them broke
    /// any other tab still holding the evicted id.
    pub async fn load_table(&self, source_id: &str, table_name: &str) -> AppResult<String> {
        let store = self.require_store()?;

        let source = DatasetSource::StoreTable {
            source_id: source_id.to_string(),
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
}

/// Build a composite DashMap key for source/table-keyed caches.
pub(crate) fn cache_key(source_id: &str, table_name: &str) -> String {
    format!("{source_id}|{table_name}")
}

/// The engine's view of one resolved column.
pub(crate) fn override_from_resolved(column: &ResolvedColumn) -> ColumnOverride {
    ColumnOverride {
        column_name: column.name.clone(),
        role: column.role,
        is_kpi: column.is_kpi.unwrap_or(false),
        polarity: column.polarity.unwrap_or_default(),
        label: column.label.clone(),
        description: column.description.clone(),
    }
}

/// The engine's view of a resolved table's analysis settings; `None` when
/// nothing analysis-relevant is set.
pub(crate) fn settings_from_resolved(table: &ResolvedTable) -> Option<TableSettingsOverride> {
    let comparison_periods = table.comparison_periods.map(|p| p as usize);
    if table.time_granularity.is_none() && comparison_periods.is_none() {
        return None;
    }
    Some(TableSettingsOverride {
        time_granularity: table.time_granularity,
        comparison_periods,
    })
}
