//! Table operations for Delta Lake

use std::path::Path;

use chrono::{DateTime, Utc};
use deltalake::arrow::array::RecordBatch;
use deltalake::parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use deltalake::{open_table as delta_open_table, DeltaTable};
use polars::prelude::*;
use serde::{Deserialize, Serialize};
use tracing::debug;
use url::Url;

use crate::error::{StoreError, StoreResult};

/// Reference to a table in the store
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableRef {
    /// Table name
    pub name: String,
    /// Path to the table
    pub path: String,
}

/// Information about a table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableInfo {
    /// Table name
    pub name: String,
    /// Path to the table
    pub path: String,
    /// Current version
    pub version: i64,
    /// Number of rows (approximate, from metadata)
    pub num_rows: Option<i64>,
    /// Number of files
    pub num_files: usize,
    /// Schema as JSON
    pub schema: Option<serde_json::Value>,
    /// Created timestamp
    pub created_at: Option<DateTime<Utc>>,
    /// Last modified timestamp
    pub updated_at: Option<DateTime<Utc>>,
}

/// Convert a path to a URL for Delta Lake
pub fn path_to_url(path: &Path) -> StoreResult<Url> {
    let abs_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };

    Url::from_file_path(&abs_path)
        .map_err(|()| StoreError::Other(format!("Invalid path: {}", abs_path.display())))
}

/// Check if a directory is a Delta table
pub async fn table_exists(path: &Path) -> StoreResult<bool> {
    let delta_log = path.join("_delta_log");
    Ok(delta_log.exists() && delta_log.is_dir())
}

/// List all tables in a directory
pub async fn list_tables(root: &Path) -> StoreResult<Vec<TableRef>> {
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut tables = Vec::new();
    let entries = std::fs::read_dir(root)?;

    for entry_result in entries {
        let entry = entry_result?;
        let path = entry.path();

        if path.is_dir() {
            let delta_log = path.join("_delta_log");
            if delta_log.exists() && delta_log.is_dir() {
                let name = entry.file_name().to_str().unwrap_or_default().to_string();
                tables.push(TableRef {
                    name,
                    path: path.to_string_lossy().to_string(),
                });
            }
        }
    }

    Ok(tables)
}

/// Open an existing Delta table
pub async fn open_table(path: &Path) -> StoreResult<DeltaTable> {
    let url = path_to_url(path)?;
    debug!("Opening Delta table at {}", url);

    let table = delta_open_table(url).await.map_err(|e| {
        let err_str = e.to_string();
        if err_str.contains("not found") || err_str.contains("does not exist") {
            StoreError::TableNotFound(path.to_string_lossy().to_string())
        } else {
            StoreError::DeltaLake(e)
        }
    })?;

    Ok(table)
}

/// Get information about a table
pub async fn get_table_info(name: &str, path: &Path) -> StoreResult<TableInfo> {
    let table = open_table(path).await?;
    let snapshot = table
        .snapshot()
        .map_err(|e| StoreError::Other(e.to_string()))?;
    let metadata = snapshot.metadata();

    // Get schema as JSON - schema() returns Arc<StructType>
    let schema = snapshot.schema();
    let schema_json = serde_json::to_value(&*schema).ok();

    // Count parquet files in the table directory
    let num_files = count_parquet_files(path)?;

    // Get created timestamp
    let created_at = metadata
        .created_time()
        .and_then(DateTime::from_timestamp_millis);

    // Get version
    let version = table.version().unwrap_or(0);

    Ok(TableInfo {
        name: name.to_string(),
        path: path.to_string_lossy().to_string(),
        version,
        num_rows: None,
        num_files,
        schema: schema_json,
        created_at,
        updated_at: None,
    })
}

/// Count parquet files in a directory
fn count_parquet_files(path: &Path) -> StoreResult<usize> {
    let mut count = 0;
    if path.exists() && path.is_dir() {
        for dir_entry in std::fs::read_dir(path)? {
            let file_path = dir_entry?.path();
            if file_path.extension().is_some_and(|ext| ext == "parquet") {
                count += 1;
            }
        }
    }
    Ok(count)
}

/// Read a Delta table as a Polars DataFrame
pub async fn read_table(path: &Path) -> StoreResult<DataFrame> {
    let table = open_table(path).await?;
    read_delta_table_to_df(&table).await
}

/// Read a specific version of a Delta table as a Polars DataFrame
pub async fn read_table_version(path: &Path, version: i64) -> StoreResult<DataFrame> {
    let url = path_to_url(path)?;
    let table = deltalake::open_table_with_version(url, version).await?;
    read_delta_table_to_df(&table).await
}

/// Convert a Delta table to a Polars DataFrame by reading all parquet files
async fn read_delta_table_to_df(table: &DeltaTable) -> StoreResult<DataFrame> {
    let base_url = table.table_url();
    let base_path = base_url
        .to_file_path()
        .map_err(|()| StoreError::Other(format!("Cannot convert URL to file path: {base_url}")))?;

    // Find all parquet files in the table directory
    let mut parquet_files = Vec::new();
    if base_path.exists() && base_path.is_dir() {
        for dir_entry in std::fs::read_dir(&base_path)? {
            let file_path = dir_entry?.path();
            if file_path.extension().is_some_and(|ext| ext == "parquet") {
                parquet_files.push(file_path);
            }
        }
    }

    if parquet_files.is_empty() {
        return Ok(DataFrame::empty());
    }

    let mut all_batches: Vec<RecordBatch> = Vec::new();

    for file_path in parquet_files {
        debug!("Reading parquet file: {}", file_path.display());

        let file = std::fs::File::open(&file_path)?;
        let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
        let reader = builder.build()?;

        for batch_result in reader {
            let batch = batch_result?;
            all_batches.push(batch);
        }
    }

    if all_batches.is_empty() {
        return Ok(DataFrame::empty());
    }

    arrow_batches_to_polars(&all_batches)
}

/// Convert Arrow record batches to Polars DataFrame
fn arrow_batches_to_polars(batches: &[RecordBatch]) -> StoreResult<DataFrame> {
    if batches.is_empty() {
        return Ok(DataFrame::empty());
    }

    // Concatenate all batches
    let schema = batches[0].schema();
    let batch = deltalake::arrow::compute::concat_batches(&schema, batches)?;

    // Convert each column to a Polars Series
    let mut columns: Vec<Column> = Vec::with_capacity(schema.fields().len());

    for (idx, field) in schema.fields().iter().enumerate() {
        let array = batch.column(idx);
        let series = arrow_array_to_series(field.name(), array)?;
        columns.push(series.into());
    }

    Ok(DataFrame::new(columns)?)
}

/// Convert an Arrow array to a Polars Series
fn arrow_array_to_series(
    name: &str,
    array: &dyn deltalake::arrow::array::Array,
) -> StoreResult<Series> {
    use deltalake::arrow::array::*;
    use deltalake::arrow::datatypes::DataType;

    let series =
        match array.data_type() {
            DataType::Boolean => {
                let arr = array
                    .as_any()
                    .downcast_ref::<BooleanArray>()
                    .ok_or_else(|| {
                        StoreError::Other("Failed to downcast to BooleanArray".to_string())
                    })?;
                let values: Vec<Option<bool>> = arr.iter().collect();
                Series::new(name.into(), values)
            },
            DataType::Int8 => {
                let arr = array.as_any().downcast_ref::<Int8Array>().ok_or_else(|| {
                    StoreError::Other("Failed to downcast to Int8Array".to_string())
                })?;
                let values: Vec<Option<i8>> = arr.iter().collect();
                Series::new(name.into(), values)
            },
            DataType::Int16 => {
                let arr = array.as_any().downcast_ref::<Int16Array>().ok_or_else(|| {
                    StoreError::Other("Failed to downcast to Int16Array".to_string())
                })?;
                let values: Vec<Option<i16>> = arr.iter().collect();
                Series::new(name.into(), values)
            },
            DataType::Int32 => {
                let arr = array.as_any().downcast_ref::<Int32Array>().ok_or_else(|| {
                    StoreError::Other("Failed to downcast to Int32Array".to_string())
                })?;
                let values: Vec<Option<i32>> = arr.iter().collect();
                Series::new(name.into(), values)
            },
            DataType::Int64 => {
                let arr = array.as_any().downcast_ref::<Int64Array>().ok_or_else(|| {
                    StoreError::Other("Failed to downcast to Int64Array".to_string())
                })?;
                let values: Vec<Option<i64>> = arr.iter().collect();
                Series::new(name.into(), values)
            },
            DataType::UInt8 => {
                let arr = array.as_any().downcast_ref::<UInt8Array>().ok_or_else(|| {
                    StoreError::Other("Failed to downcast to UInt8Array".to_string())
                })?;
                // Cast u8 to i16 since Polars doesn't support u8 directly
                let values: Vec<Option<i16>> = arr.iter().map(|v| v.map(i16::from)).collect();
                Series::new(name.into(), values)
            },
            DataType::UInt16 => {
                let arr = array
                    .as_any()
                    .downcast_ref::<UInt16Array>()
                    .ok_or_else(|| {
                        StoreError::Other("Failed to downcast to UInt16Array".to_string())
                    })?;
                // Cast u16 to i32 since Polars doesn't support u16 directly
                let values: Vec<Option<i32>> = arr.iter().map(|v| v.map(i32::from)).collect();
                Series::new(name.into(), values)
            },
            DataType::UInt32 => {
                let arr = array
                    .as_any()
                    .downcast_ref::<UInt32Array>()
                    .ok_or_else(|| {
                        StoreError::Other("Failed to downcast to UInt32Array".to_string())
                    })?;
                let values: Vec<Option<u32>> = arr.iter().collect();
                Series::new(name.into(), values)
            },
            DataType::UInt64 => {
                let arr = array
                    .as_any()
                    .downcast_ref::<UInt64Array>()
                    .ok_or_else(|| {
                        StoreError::Other("Failed to downcast to UInt64Array".to_string())
                    })?;
                let values: Vec<Option<u64>> = arr.iter().collect();
                Series::new(name.into(), values)
            },
            DataType::Float32 => {
                let arr = array
                    .as_any()
                    .downcast_ref::<Float32Array>()
                    .ok_or_else(|| {
                        StoreError::Other("Failed to downcast to Float32Array".to_string())
                    })?;
                let values: Vec<Option<f32>> = arr.iter().collect();
                Series::new(name.into(), values)
            },
            DataType::Float64 => {
                let arr = array
                    .as_any()
                    .downcast_ref::<Float64Array>()
                    .ok_or_else(|| {
                        StoreError::Other("Failed to downcast to Float64Array".to_string())
                    })?;
                let values: Vec<Option<f64>> = arr.iter().collect();
                Series::new(name.into(), values)
            },
            DataType::Utf8 => {
                let arr = array
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .ok_or_else(|| {
                        StoreError::Other("Failed to downcast to StringArray".to_string())
                    })?;
                let values: Vec<Option<&str>> = arr.iter().collect();
                Series::new(name.into(), values)
            },
            DataType::LargeUtf8 => {
                let arr = array
                    .as_any()
                    .downcast_ref::<LargeStringArray>()
                    .ok_or_else(|| {
                        StoreError::Other("Failed to downcast to LargeStringArray".to_string())
                    })?;
                let values: Vec<Option<&str>> = arr.iter().collect();
                Series::new(name.into(), values)
            },
            DataType::Date32 => {
                let arr = array
                    .as_any()
                    .downcast_ref::<Date32Array>()
                    .ok_or_else(|| {
                        StoreError::Other("Failed to downcast to Date32Array".to_string())
                    })?;
                let values: Vec<Option<i32>> = arr.iter().collect();
                Series::new(name.into(), values)
            },
            DataType::Date64 => {
                let arr = array
                    .as_any()
                    .downcast_ref::<Date64Array>()
                    .ok_or_else(|| {
                        StoreError::Other("Failed to downcast to Date64Array".to_string())
                    })?;
                let values: Vec<Option<i64>> = arr.iter().collect();
                Series::new(name.into(), values)
            },
            DataType::Timestamp(_, _) => {
                // Try various timestamp types
                if let Some(arr) = array.as_any().downcast_ref::<TimestampMicrosecondArray>() {
                    let values: Vec<Option<i64>> = arr.iter().collect();
                    Series::new(name.into(), values)
                } else if let Some(arr) = array.as_any().downcast_ref::<TimestampMillisecondArray>()
                {
                    let values: Vec<Option<i64>> = arr.iter().collect();
                    Series::new(name.into(), values)
                } else if let Some(arr) = array.as_any().downcast_ref::<TimestampSecondArray>() {
                    let values: Vec<Option<i64>> = arr.iter().collect();
                    Series::new(name.into(), values)
                } else if let Some(arr) = array.as_any().downcast_ref::<TimestampNanosecondArray>()
                {
                    let values: Vec<Option<i64>> = arr.iter().collect();
                    Series::new(name.into(), values)
                } else {
                    // Fallback
                    let values: Vec<String> = (0..array.len()).map(|_| String::new()).collect();
                    Series::new(name.into(), values)
                }
            },
            _ => {
                // Fallback: create null strings
                let values: Vec<Option<&str>> = (0..array.len()).map(|_| None).collect();
                Series::new(name.into(), values)
            },
        };

    Ok(series)
}

/// Delete a Delta table
pub async fn delete_table(path: &Path) -> StoreResult<()> {
    if !table_exists(path).await? {
        return Err(StoreError::TableNotFound(
            path.to_string_lossy().to_string(),
        ));
    }

    std::fs::remove_dir_all(path)?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_table_not_exists() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let result = table_exists(&tmp.path().join("nonexistent")).await;
        assert!(result.is_ok());
        assert!(!result.expect("table_exists failed"));
    }
}
