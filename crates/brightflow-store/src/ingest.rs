//! Data ingestion into Delta Lake tables

use std::path::Path;
use std::sync::Arc;

use deltalake::arrow::array::RecordBatch;
use deltalake::arrow::datatypes::Schema as ArrowSchema;
use deltalake::kernel::{DataType, PrimitiveType, StructField, StructType};
use deltalake::operations::create::CreateBuilder;
use deltalake::parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use deltalake::protocol::SaveMode;
use deltalake::DeltaTable;
use polars::prelude::*;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::error::{StoreError, StoreResult};
use crate::table::{path_to_url, table_exists};

/// Options for data ingestion
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IngestOptions {
    /// Save mode: Append, Overwrite, ErrorIfExists, Ignore
    #[serde(default)]
    pub mode: IngestMode,

    /// Partition columns
    #[serde(default)]
    pub partition_by: Vec<String>,

    /// Table description
    pub description: Option<String>,
}

/// Mode for ingestion
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum IngestMode {
    /// Append to existing data
    #[default]
    Append,
    /// Overwrite existing data
    Overwrite,
    /// Error if table exists
    ErrorIfExists,
    /// Ignore if table exists
    Ignore,
}

impl From<IngestMode> for SaveMode {
    fn from(mode: IngestMode) -> Self {
        match mode {
            IngestMode::Append => Self::Append,
            IngestMode::Overwrite => Self::Overwrite,
            IngestMode::ErrorIfExists => Self::ErrorIfExists,
            IngestMode::Ignore => Self::Ignore,
        }
    }
}

/// Ingest a Parquet file into a Delta table
pub async fn ingest_parquet(
    table_path: &Path,
    parquet_path: &Path,
    options: &IngestOptions,
) -> StoreResult<()> {
    if !parquet_path.exists() {
        return Err(StoreError::FileNotFound(parquet_path.to_path_buf()));
    }

    // Read parquet file to get schema and data
    let file = std::fs::File::open(parquet_path)?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
    let arrow_schema = builder.schema().clone();
    let reader = builder.build()?;

    let batches: Vec<RecordBatch> = reader.collect::<Result<Vec<_>, _>>()?;

    if batches.is_empty() {
        debug!("Parquet file is empty, skipping ingestion");
        return Ok(());
    }

    write_batches_to_delta(table_path, &arrow_schema, batches, options).await
}

/// Ingest a Polars DataFrame into a Delta table
pub async fn ingest_dataframe(
    table_path: &Path,
    df: DataFrame,
    options: &IngestOptions,
) -> StoreResult<()> {
    if df.is_empty() {
        debug!("DataFrame is empty, skipping ingestion");
        return Ok(());
    }

    // Convert Polars DataFrame to Arrow RecordBatch via IPC
    let batches = df_to_arrow_batches(&df)?;

    if batches.is_empty() {
        return Ok(());
    }

    let arrow_schema = batches[0].schema();
    write_batches_to_delta(table_path, &arrow_schema, batches, options).await
}

/// Convert Polars DataFrame to Arrow RecordBatches
fn df_to_arrow_batches(df: &DataFrame) -> StoreResult<Vec<RecordBatch>> {
    use polars::io::ipc::IpcStreamWriter;

    let mut ipc_buffer = Vec::new();
    let mut writer = IpcStreamWriter::new(&mut ipc_buffer);

    // Clone the dataframe to write
    let mut df_clone = df.clone();
    writer.finish(&mut df_clone)?;

    // Read back as Arrow using deltalake's arrow version
    let cursor = std::io::Cursor::new(ipc_buffer);
    let reader = deltalake::arrow::ipc::reader::StreamReader::try_new(cursor, None)?;

    let batches: Vec<RecordBatch> = reader.collect::<Result<Vec<_>, _>>()?;
    Ok(batches)
}

/// Write Arrow batches to a Delta table
async fn write_batches_to_delta(
    table_path: &Path,
    arrow_schema: &Arc<ArrowSchema>,
    batches: Vec<RecordBatch>,
    options: &IngestOptions,
) -> StoreResult<()> {
    let url = path_to_url(table_path)?;
    let exists = table_exists(table_path).await?;

    if exists {
        // Handle based on mode
        if options.mode == IngestMode::ErrorIfExists {
            return Err(StoreError::TableAlreadyExists(
                table_path.to_string_lossy().to_string(),
            ));
        }
        if options.mode == IngestMode::Ignore {
            debug!("Table exists and mode is Ignore, skipping");
            return Ok(());
        }

        // Append or overwrite to existing table
        let table = deltalake::open_table(url).await?;
        write_to_existing_table(table, batches, options).await
    } else {
        // Create new table
        create_and_write_table(&url, arrow_schema, batches, options).await
    }
}

/// Create a new Delta table and write data
async fn create_and_write_table(
    url: &url::Url,
    arrow_schema: &Arc<ArrowSchema>,
    batches: Vec<RecordBatch>,
    options: &IngestOptions,
) -> StoreResult<()> {
    info!("Creating new Delta table at {}", url);

    // Convert Arrow schema to Delta schema
    let delta_schema = arrow_to_delta_schema(arrow_schema)?;
    let fields: Vec<StructField> = delta_schema.fields().cloned().collect();

    // Create the table
    let mut builder = CreateBuilder::new()
        .with_location(url.as_str())
        .with_columns(fields);

    if !options.partition_by.is_empty() {
        builder = builder.with_partition_columns(&options.partition_by);
    }

    if let Some(desc) = &options.description {
        builder = builder.with_comment(desc);
    }

    let table = builder.await?;
    info!("Created Delta table version {:?}", table.version());

    // Write the data
    write_to_existing_table(table, batches, options).await
}

/// Write data to an existing Delta table
async fn write_to_existing_table(
    table: DeltaTable,
    batches: Vec<RecordBatch>,
    options: &IngestOptions,
) -> StoreResult<()> {
    let version_before = table.version().unwrap_or(0);

    // Use the table's write operation which handles snapshot correctly
    let mut builder = table
        .write(batches)
        .with_save_mode(options.mode.clone().into());

    if !options.partition_by.is_empty() {
        builder = builder.with_partition_columns(&options.partition_by);
    }

    let table = builder.await?;
    info!(
        "Wrote to Delta table, version {} -> {:?}",
        version_before,
        table.version()
    );

    Ok(())
}

/// Convert Arrow schema to Delta schema
fn arrow_to_delta_schema(arrow_schema: &ArrowSchema) -> StoreResult<StructType> {
    let fields: Vec<StructField> = arrow_schema
        .fields()
        .iter()
        .map(|f| {
            StructField::new(
                f.name(),
                arrow_to_delta_type(f.data_type()),
                f.is_nullable(),
            )
        })
        .collect();

    StructType::try_new(fields).map_err(|e| StoreError::Other(e.to_string()))
}

/// Convert Arrow data type to Delta data type
fn arrow_to_delta_type(arrow_type: &deltalake::arrow::datatypes::DataType) -> DataType {
    use deltalake::arrow::datatypes::DataType as AT;

    match arrow_type {
        AT::Boolean => DataType::Primitive(PrimitiveType::Boolean),
        AT::Int8 => DataType::Primitive(PrimitiveType::Byte),
        AT::Int16 => DataType::Primitive(PrimitiveType::Short),
        AT::Int32 => DataType::Primitive(PrimitiveType::Integer),
        AT::Int64 => DataType::Primitive(PrimitiveType::Long),
        AT::UInt8 => DataType::Primitive(PrimitiveType::Short),
        AT::UInt16 => DataType::Primitive(PrimitiveType::Integer),
        AT::UInt32 => DataType::Primitive(PrimitiveType::Long),
        AT::UInt64 => DataType::Primitive(PrimitiveType::Long),
        AT::Float32 => DataType::Primitive(PrimitiveType::Float),
        AT::Float64 => DataType::Primitive(PrimitiveType::Double),
        AT::Utf8 | AT::LargeUtf8 => DataType::Primitive(PrimitiveType::String),
        AT::Binary | AT::LargeBinary => DataType::Primitive(PrimitiveType::Binary),
        AT::Date32 | AT::Date64 => DataType::Primitive(PrimitiveType::Date),
        AT::Timestamp(_, _) => DataType::Primitive(PrimitiveType::Timestamp),
        AT::Decimal128(p, s) => {
            // Try to create decimal type with given precision/scale, fall back to (38, 0)
            let decimal_type = deltalake::kernel::DecimalType::try_new(*p, *s as u8)
                .or_else(|_| deltalake::kernel::DecimalType::try_new(38, 0))
                .unwrap_or_else(|e| {
                    // This should never happen as (38, 0) is always valid
                    unreachable!("Failed to create default decimal type: {e}")
                });
            DataType::Primitive(PrimitiveType::Decimal(decimal_type))
        },
        AT::List(field) => {
            let inner = arrow_to_delta_type(field.data_type());
            DataType::Array(Box::new(deltalake::kernel::ArrayType::new(
                inner,
                field.is_nullable(),
            )))
        },
        AT::Struct(fields) => {
            let struct_fields: Vec<StructField> = fields
                .iter()
                .map(|f| {
                    StructField::new(
                        f.name(),
                        arrow_to_delta_type(f.data_type()),
                        f.is_nullable(),
                    )
                })
                .collect();
            let struct_type = StructType::try_new(struct_fields)
                .unwrap_or_else(|_| StructType::new_unchecked(vec![]));
            DataType::Struct(Box::new(struct_type))
        },
        // Default to string for unsupported types
        _ => DataType::Primitive(PrimitiveType::String),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ingest_mode_default() {
        let mode = IngestMode::default();
        assert_eq!(mode, IngestMode::Append);
    }

    #[test]
    fn test_ingest_options_default() {
        let opts = IngestOptions::default();
        assert_eq!(opts.mode, IngestMode::Append);
        assert!(opts.partition_by.is_empty());
        assert!(opts.description.is_none());
    }
}
