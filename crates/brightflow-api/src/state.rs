use crate::analytics::session::{DatasetData, DatasetManager, DatasetSource};
use crate::shared::AppResult;
use crate::system::log_layer::LogEntry;
use crate::system::sampler::SystemSnapshot;
use brightflow_insights::data::config::SchemaConfig;
use brightflow_insights::data::schema::DataSchema;
use brightflow_scheduler::Scheduler;
use brightflow_store::{ParquetStore, TableInfo};
use dashmap::DashMap;
use polars::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, RwLock};

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    /// Thread-safe storage for loaded datasets
    pub datasets: DatasetManager,
    /// Index of available Delta tables (metadata only, no data loaded)
    pub table_index: Arc<RwLock<Vec<TableInfo>>>,
    /// Reference to the Parquet store for lazy loading
    store: Option<Arc<ParquetStore>>,
    /// Global schema configs keyed by table name
    pub schemas: Arc<DashMap<String, DataSchema>>,
    /// Optional scheduler for background jobs
    pub scheduler: Option<Arc<Scheduler>>,
    /// Path to connector config directory
    pub connector_config_dir: Option<PathBuf>,
    /// Authentication database
    pub auth_db: Option<Arc<crate::auth::AuthDb>>,
    /// Scheduler database
    pub scheduler_db: Option<Arc<brightflow_scheduler::SchedulerDb>>,
    /// Live system metrics snapshot (updated by background sampler)
    pub system_metrics: Arc<RwLock<SystemSnapshot>>,
    /// Broadcast sender for log entries (from custom tracing Layer)
    pub log_sender: broadcast::Sender<LogEntry>,
    /// Server start time (for uptime calculation)
    pub start_time: Instant,
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
            scheduler: None,
            connector_config_dir: None,
            auth_db: None,
            scheduler_db: None,
            system_metrics: Arc::new(RwLock::new(SystemSnapshot::default())),
            log_sender,
            start_time: Instant::now(),
        }
    }

    /// Create new AppState with an existing log broadcast sender (from init_tracing).
    pub fn with_log_sender(log_sender: broadcast::Sender<LogEntry>) -> Self {
        Self {
            datasets: DatasetManager::new(),
            table_index: Arc::new(RwLock::new(Vec::new())),
            store: None,
            schemas: Arc::new(DashMap::new()),
            scheduler: None,
            connector_config_dir: None,
            auth_db: None,
            scheduler_db: None,
            system_metrics: Arc::new(RwLock::new(SystemSnapshot::default())),
            log_sender,
            start_time: Instant::now(),
        }
    }

    /// Load schema configs from YAML files in a directory
    pub fn load_schemas_from_dir(&self, dir: &Path) {
        if !dir.exists() || !dir.is_dir() {
            tracing::debug!("Schema directory not found: {}", dir.display());
            return;
        }

        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("Failed to read schema directory {}: {}", dir.display(), e);
                return;
            },
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "toml") {
                let table_name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .to_string();

                match SchemaConfig::load(&path) {
                    Ok(config) => {
                        let schema = DataSchema::from_config(&config);
                        tracing::info!(
                            "Loaded schema for '{}': {} KPIs, {} metrics, {} dimensions",
                            table_name,
                            schema.kpi_columns.len(),
                            schema.metric_columns.len(),
                            schema.dimension_columns.len(),
                        );
                        self.schemas.insert(table_name, schema);
                    },
                    Err(e) => {
                        tracing::warn!("Failed to load schema from {}: {}", path.display(), e);
                    },
                }
            }
        }
    }

    /// Get schema for a table by name
    pub fn get_schema(&self, table_name: &str) -> Option<DataSchema> {
        self.schemas.get(table_name).map(|s| s.clone())
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
                if let Ok(info) = store.table_info(&table_ref.name).await {
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
            if let Ok(info) = store.table_info(&table_ref.name).await {
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
            scheduler: None,
            connector_config_dir: None,
            auth_db: None,
            scheduler_db: None,
            system_metrics: Arc::new(RwLock::new(SystemSnapshot::default())),
            log_sender,
            start_time: Instant::now(),
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
            if let Ok(info) = store.table_info(&table_ref.name).await {
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
            scheduler: None,
            connector_config_dir: None,
            auth_db: None,
            scheduler_db: None,
            system_metrics: Arc::new(RwLock::new(SystemSnapshot::default())),
            log_sender,
            start_time: Instant::now(),
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

        state.datasets.add_dataset(
            name,
            DatasetData::Eager(df),
            brightflow_core::DataMode::Memory,
            DatasetSource::Default,
        );

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

        state.datasets.add_dataset(
            name,
            DatasetData::Eager(df),
            brightflow_core::DataMode::Memory,
            DatasetSource::Default,
        );

        Ok(state)
    }

    /// Get the list of available tables (metadata only)
    pub async fn get_available_tables(&self) -> Vec<TableInfo> {
        self.table_index.read().await.clone()
    }

    /// Load a specific table on-demand with the given data mode
    ///
    /// This unloads any previously loaded tables first to keep memory usage low.
    pub async fn load_table(
        &self,
        table_name: &str,
        data_mode: brightflow_core::DataMode,
    ) -> AppResult<String> {
        let store = self.store.as_ref().ok_or_else(|| {
            crate::shared::AppError::BadRequest("No store configured".to_string())
        })?;

        // Unload any existing store tables to free memory
        self.unload_store_tables();

        let source = DatasetSource::DeltaTable {
            table_name: table_name.to_string(),
            version: -1,
        };

        let id = match data_mode {
            brightflow_core::DataMode::Memory => {
                tracing::info!("Loading table '{}' into memory (eager)", table_name);
                let df = store.read_table(table_name).await?;
                tracing::info!("Loaded table '{}': {} rows", table_name, df.height());
                self.datasets.add_dataset(
                    table_name.to_string(),
                    DatasetData::Eager(df),
                    data_mode,
                    source,
                )
            },
            brightflow_core::DataMode::Lazy => {
                tracing::info!("Loading table '{}' in lazy mode (parquet scan)", table_name);
                let parquet_files = store.get_table_parquet_paths(table_name).await?;
                tracing::info!(
                    "Registered {} parquet file(s) for lazy scan of '{}'",
                    parquet_files.len(),
                    table_name
                );
                self.datasets.add_dataset(
                    table_name.to_string(),
                    DatasetData::Lazy { parquet_files },
                    data_mode,
                    source,
                )
            },
        };

        Ok(id)
    }

    /// Unload all store tables from memory (keeps "default" and uploaded datasets)
    pub fn unload_store_tables(&self) {
        let to_remove: Vec<_> = self
            .datasets
            .list_datasets()
            .iter()
            .filter(|d| d.id.starts_with("delta:"))
            .map(|d| d.id.clone())
            .collect();

        for id in &to_remove {
            self.datasets.delete_dataset(id);
        }

        if !to_remove.is_empty() {
            tracing::info!("Unloaded {} table(s) from memory", to_remove.len());
        }
    }

    /// Check if a table exists in the index
    pub async fn table_exists(&self, table_name: &str) -> bool {
        let index = self.table_index.read().await;
        index.iter().any(|t| t.name == table_name)
    }

    /// Load a table directly from a store
    pub async fn load_store_table(
        &self,
        store: &ParquetStore,
        table_name: &str,
    ) -> AppResult<String> {
        let df = store.read_table(table_name).await?;

        let source = DatasetSource::DeltaTable {
            table_name: table_name.to_string(),
            version: -1,
        };

        let id = self.datasets.add_dataset(
            table_name.to_string(),
            DatasetData::Eager(df),
            brightflow_core::DataMode::Memory,
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
            let result = match self.load_store_table(store, &table_ref.name).await {
                Ok(id) => Ok(id),
                Err(e) => Err(e.to_string()),
            };
            results.push((table_ref.name, result));
        }
        results
    }
}
