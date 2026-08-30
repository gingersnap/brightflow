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
use brightflow_engine::data::config::{ColumnRole, Polarity, TimeGranularity};
use brightflow_engine::data::merge::{ColumnOverride, TableSettingsOverride};
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
            schemas: Arc::new(DashMap::new()),
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

    /// Invalidate the cached schema for a composite key (call after override mutations).
    pub fn invalidate_schema_cache(&self, key: &str) {
        self.schemas.remove(key);
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

/// Convert a `ColumnSemanticRow` to a `ColumnOverride`.
fn convert_semantic_row(row: &ColumnSemanticRow) -> Option<ColumnOverride> {
    let Some(role) = ColumnRole::parse(&row.role) else {
        tracing::warn!(
            "Unknown column role '{}' for '{}'",
            row.role,
            row.column_name
        );
        return None;
    };
    let polarity = Polarity::parse(&row.polarity).unwrap_or_else(|| {
        tracing::warn!(
            "Unknown polarity '{}' for '{}' — treating as neutral",
            row.polarity,
            row.column_name
        );
        Polarity::Neutral
    });
    Some(ColumnOverride {
        column_name: row.column_name.clone(),
        role,
        is_kpi: row.is_kpi,
        polarity,
        label: row.label.clone(),
        description: row.description.clone(),
    })
}

/// Seed known TOML schema data into SQLite if `column_semantics` is empty.
pub async fn seed_column_semantics(store: &ParquetStore) {
    // Only seed if we have tables but no semantics yet
    let has_semantics = store.db().has_any_column_semantics().await.unwrap_or(true);
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
                polarity: "neutral".to_string(),
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
