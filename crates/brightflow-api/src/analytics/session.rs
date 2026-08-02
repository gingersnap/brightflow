use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use polars::prelude::*;
use serde::Serialize;
use ts_rs::TS;

/// Source of a dataset
#[derive(Clone, Debug)]
pub enum DatasetSource {
    /// Built-in default dataset
    Default,
    /// User-uploaded CSV file
    Upload { filename: String },
    /// Loaded from Parquet store table
    StoreTable {
        /// Source the table belongs to. Part of the dataset identity: table names are
        /// only unique *within* a source, so dropping this silently aliases two
        /// sources' same-named tables onto one another.
        source_id: String,
        /// Name of the table
        table_name: String,
        /// Version of the table (-1 for latest)
        version: i64,
    },
}

/// The underlying data for a dataset
#[derive(Clone, Debug)]
pub enum DatasetData {
    /// DataFrame from CSV upload or default dataset
    Uploaded(DataFrame),
    /// References to parquet files, scanned lazily per query
    Parquet { files: Vec<PathBuf> },
}

/// A loaded dataset with metadata
pub struct Dataset {
    pub name: String,
    pub data: DatasetData,
    pub loaded_at: DateTime<Utc>,
    pub source: DatasetSource,
}

impl Dataset {
    pub fn new(name: String, data: DatasetData, source: DatasetSource) -> Self {
        Self {
            name,
            data,
            loaded_at: Utc::now(),
            source,
        }
    }

    pub fn row_count(&self) -> Option<usize> {
        match &self.data {
            DatasetData::Uploaded(df) => Some(df.height()),
            // Row count requires .collect() which can't run inside the tokio runtime.
            // The executor returns total_rows with each query result instead.
            DatasetData::Parquet { .. } => None,
        }
    }

    pub fn column_count(&self) -> Option<usize> {
        Some(self.columns().len())
    }

    pub fn columns(&self) -> Vec<ColumnInfo> {
        match &self.data {
            DatasetData::Uploaded(df) => df
                .get_columns()
                .iter()
                .map(|col| ColumnInfo {
                    name: col.name().to_string(),
                    dtype: dtype_to_string(col.dtype()),
                    role: None,
                    is_kpi: None,
                    label: None,
                    polarity: None,
                })
                .collect(),
            DatasetData::Parquet { files } => {
                if let Some(first) = files.first() {
                    if let Ok(mut lf) = LazyFrame::scan_parquet(first, ScanArgsParquet::default()) {
                        if let Ok(schema) = lf.collect_schema() {
                            return schema
                                .iter()
                                .map(|(name, dtype)| ColumnInfo {
                                    name: name.to_string(),
                                    dtype: dtype_to_string(dtype),
                                    role: None,
                                    is_kpi: None,
                                    label: None,
                                    polarity: None,
                                })
                                .collect();
                        }
                    }
                }
                Vec::new()
            },
        }
    }
}

/// Column metadata
#[derive(Clone, Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ColumnInfo {
    pub name: String,
    pub dtype: String,
    /// Semantic role override (if configured)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Whether this column is a KPI (only meaningful for measures)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_kpi: Option<bool>,
    /// Display label override
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Measure polarity: higher_is_better | lower_is_better | neutral
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub polarity: Option<String>,
}

/// Summary info for a dataset
#[derive(Clone, Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DatasetInfo {
    pub id: String,
    pub name: String,
    pub row_count: Option<usize>,
    pub column_count: Option<usize>,
    pub loaded_at: DateTime<Utc>,
}

/// Thread-safe manager for datasets
#[derive(Clone)]
pub struct DatasetManager {
    datasets: Arc<DashMap<String, Dataset>>,
}

impl Default for DatasetManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DatasetManager {
    pub fn new() -> Self {
        Self {
            datasets: Arc::new(DashMap::new()),
        }
    }

    /// Add a new dataset, returns its ID
    pub fn add_dataset(&self, name: String, data: DatasetData, source: DatasetSource) -> String {
        let id = match &source {
            DatasetSource::Default => "default".to_string(),
            DatasetSource::Upload { .. } => uuid::Uuid::new_v4().to_string(),
            // Source-scoped: a bare table name is ambiguous across sources, and the
            // collision is silent — the second load evicts the first and every client
            // still holding the old id starts reading the other source's rows. The `|`
            // separator matches `state::cache_key` so composite keys read alike.
            DatasetSource::StoreTable {
                source_id,
                table_name,
                ..
            } => format!("store:{source_id}|{table_name}"),
        };

        self.datasets
            .insert(id.clone(), Dataset::new(name, data, source));
        id
    }

    /// Get a clone of the dataset data for use in blocking tasks
    pub fn get_data(&self, id: &str) -> Option<DatasetData> {
        self.datasets.get(id).map(|d| d.data.clone())
    }

    /// Get a reference to a dataset by ID
    pub fn get_dataset(&self, id: &str) -> Option<dashmap::mapref::one::Ref<'_, String, Dataset>> {
        self.datasets.get(id)
    }

    /// Delete a dataset by ID (cannot delete default)
    pub fn delete_dataset(&self, id: &str) -> bool {
        if id == "default" {
            return false;
        }
        self.datasets.remove(id).is_some()
    }

    /// List all datasets
    pub fn list_datasets(&self) -> Vec<DatasetInfo> {
        self.datasets
            .iter()
            .map(|entry| DatasetInfo {
                id: entry.key().clone(),
                name: entry.value().name.clone(),
                row_count: entry.value().row_count(),
                column_count: entry.value().column_count(),
                loaded_at: entry.value().loaded_at,
            })
            .collect()
    }

    /// Check if a dataset exists
    pub fn has_dataset(&self, id: &str) -> bool {
        self.datasets.contains_key(id)
    }
}

/// Convert Polars DataType to a display string
fn dtype_to_string(dtype: &DataType) -> String {
    match dtype {
        DataType::Boolean => "bool",
        DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64 => "int",
        DataType::UInt8 | DataType::UInt16 | DataType::UInt32 | DataType::UInt64 => "uint",
        DataType::Float32 | DataType::Float64 => "float",
        DataType::String => "string",
        DataType::Datetime(_, _) => "datetime",
        DataType::Date => "date",
        DataType::Time => "time",
        DataType::Duration(_) => "duration",
        DataType::Null => "null",
        DataType::List(_) => "list",
        DataType::Struct(_) => "struct",
        _ => "unknown",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parquet(path: &str) -> DatasetData {
        DatasetData::Parquet {
            files: vec![PathBuf::from(path)],
        }
    }

    fn store_source(source_id: &str, table_name: &str) -> DatasetSource {
        DatasetSource::StoreTable {
            source_id: source_id.to_string(),
            table_name: table_name.to_string(),
            version: -1,
        }
    }

    fn files_of(data: &DatasetData) -> Vec<PathBuf> {
        match data {
            DatasetData::Parquet { files } => files.clone(),
            DatasetData::Uploaded(_) => Vec::new(),
        }
    }

    #[test]
    fn default_source_keys_to_default() {
        let mgr = DatasetManager::new();
        let id = mgr.add_dataset(
            "sales.csv".to_string(),
            parquet("/tmp/x.parquet"),
            DatasetSource::Default,
        );
        assert_eq!(id, "default");
    }

    #[test]
    fn upload_source_keys_to_a_fresh_uuid() {
        let mgr = DatasetManager::new();
        let source = || DatasetSource::Upload {
            filename: "sales.csv".to_string(),
        };
        let a = mgr.add_dataset("sales.csv".to_string(), parquet("/tmp/a.parquet"), source());
        let b = mgr.add_dataset("sales.csv".to_string(), parquet("/tmp/b.parquet"), source());
        // Uploads are distinct artifacts even under the same filename.
        assert_ne!(a, b);
        assert_eq!(uuid::Uuid::parse_str(&a).map(|_| ()), Ok(()));
    }

    #[test]
    fn store_source_keys_on_source_and_table() {
        let mgr = DatasetManager::new();
        let id = mgr.add_dataset(
            "issues".to_string(),
            parquet("/store/src-a/issues/000.parquet"),
            store_source("src-a", "issues"),
        );
        assert_eq!(id, "store:src-a|issues");
    }

    /// Regression: before source scoping, both of these keyed to `store:issues`, so the
    /// second load evicted the first and any client still holding the old id silently
    /// received the other source's rows.
    #[test]
    fn same_table_name_under_two_sources_stays_separate() {
        let mgr = DatasetManager::new();
        let a = mgr.add_dataset(
            "issues".to_string(),
            parquet("/store/src-a/issues/000.parquet"),
            store_source("src-a", "issues"),
        );
        let b = mgr.add_dataset(
            "issues".to_string(),
            parquet("/store/src-b/issues/000.parquet"),
            store_source("src-b", "issues"),
        );

        assert_ne!(a, b);
        assert!(mgr.has_dataset(&a) && mgr.has_dataset(&b));

        // The point of the fix: each id resolves to *its own* files, not the other's.
        let files_a = files_of(&mgr.get_data(&a).expect("src-a dataset missing"));
        let files_b = files_of(&mgr.get_data(&b).expect("src-b dataset missing"));
        assert_eq!(
            files_a,
            vec![PathBuf::from("/store/src-a/issues/000.parquet")]
        );
        assert_eq!(
            files_b,
            vec![PathBuf::from("/store/src-b/issues/000.parquet")]
        );
        assert_ne!(files_a, files_b);
    }

    #[test]
    fn store_ids_keep_the_store_prefix_for_bulk_unload() {
        // `AppState::unload_store_tables` filters on this prefix; source scoping must
        // not break that.
        let mgr = DatasetManager::new();
        let id = mgr.add_dataset(
            "issues".to_string(),
            parquet("/store/src-a/issues/000.parquet"),
            store_source("src-a", "issues"),
        );
        assert!(id.starts_with("store:"));
    }

    #[test]
    fn reloading_the_same_source_table_replaces_in_place() {
        let mgr = DatasetManager::new();
        let first = mgr.add_dataset(
            "issues".to_string(),
            parquet("/store/src-a/issues/000.parquet"),
            store_source("src-a", "issues"),
        );
        let second = mgr.add_dataset(
            "issues".to_string(),
            parquet("/store/src-a/issues/001.parquet"),
            store_source("src-a", "issues"),
        );
        // A re-sync of the same table should refresh, not accumulate.
        assert_eq!(first, second);
        assert_eq!(mgr.list_datasets().len(), 1);
        assert_eq!(
            files_of(&mgr.get_data(&second).expect("dataset missing")),
            vec![PathBuf::from("/store/src-a/issues/001.parquet")]
        );
    }

    #[test]
    fn default_dataset_cannot_be_deleted() {
        let mgr = DatasetManager::new();
        let id = mgr.add_dataset(
            "sales.csv".to_string(),
            parquet("/tmp/x.parquet"),
            DatasetSource::Default,
        );
        assert!(!mgr.delete_dataset(&id));
        assert!(mgr.has_dataset("default"));
    }

    #[test]
    fn store_datasets_can_be_deleted() {
        let mgr = DatasetManager::new();
        let id = mgr.add_dataset(
            "issues".to_string(),
            parquet("/store/src-a/issues/000.parquet"),
            store_source("src-a", "issues"),
        );
        assert!(mgr.delete_dataset(&id));
        assert!(!mgr.has_dataset(&id));
    }
}
