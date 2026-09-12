//! `store *`, `migrate-events`, `compact`: direct Litehouse operations.
//!
//! `ingest` accepts CSV as well as Parquet. The store itself only ingests
//! Parquet; a CSV input is converted here, in the CLI, so the storage path
//! stays single-format. This mirrors `export`, which has always written CSV —
//! a store you can export from but only feed Parquet into is a one-way door.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use polars::prelude::{ParquetWriter, SerReader, SerWriter};

use brightflow_store::{IngestMode, IngestOptions, ParquetStore};

use crate::StoreCommands;

/// Convert a CSV input to a temporary Parquet file, returning it plus the
/// `TempDir` that owns its lifetime.
///
/// The caller must hold the `TempDir` until ingest finishes: dropping it
/// deletes the file out from under the read.
fn csv_to_temp_parquet(input: &Path) -> Result<(tempfile::TempDir, PathBuf)> {
    let mut df = polars::io::csv::read::CsvReadOptions::default()
        .with_has_header(true)
        .try_into_reader_with_file_path(Some(input.to_path_buf()))
        .with_context(|| format!("open CSV {}", input.display()))?
        .finish()
        .with_context(|| format!("parse CSV {}", input.display()))?;

    let dir = tempfile::tempdir().context("temp dir for CSV conversion")?;
    let parquet = dir.path().join("converted.parquet");
    let mut file = std::fs::File::create(&parquet).context("create temp Parquet")?;
    ParquetWriter::new(&mut file)
        .finish(&mut df)
        .context("write temp Parquet")?;
    Ok((dir, parquet))
}

pub(crate) async fn handle_store_command(cmd: StoreCommands) -> Result<()> {
    let wp = brightflow_core::WorkspacePaths::from_env();
    let litehouse_url = wp.litehouse_url();

    match cmd {
        StoreCommands::List { path } => {
            let store_path = path.unwrap_or_else(|| wp.store());
            let store = ParquetStore::new(&store_path, &litehouse_url).await?;
            let tables = store.list_tables().await?;

            if tables.is_empty() {
                println!("No tables found in store at {}", store_path.display());
            } else {
                println!("Tables in {}:", store_path.display());
                for table in tables {
                    println!("  - {} ({})", table.name, table.source_id);
                }
            }
        },

        StoreCommands::Info { source, name, path } => {
            let store_path = path.unwrap_or_else(|| wp.store());
            let store = ParquetStore::new(&store_path, &litehouse_url).await?;
            let info = store.table_info(&source, &name).await?;

            println!("Source: {}", info.source_id);
            println!("Table: {}", info.name);
            println!("Path: {}", info.path);
            println!("Version: {}", info.version);
            println!("Files: {}", info.num_files);
            if let Some(rows) = info.num_rows {
                println!("Rows: {rows}");
            }
            if let Some(created) = info.created_at {
                println!("Created: {created}");
            }
            if let Some(schema) = &info.schema {
                println!("Schema: {}", serde_json::to_string_pretty(schema)?);
            }
        },

        StoreCommands::Ingest {
            source,
            table,
            input,
            path,
            overwrite,
        } => {
            if !input.exists() {
                anyhow::bail!("Input file not found: {}", input.display());
            }

            let store_path = path.unwrap_or_else(|| wp.store());
            let store = ParquetStore::new(&store_path, &litehouse_url).await?;
            std::fs::create_dir_all(&store_path)?;

            let options = IngestOptions {
                mode: if overwrite {
                    IngestMode::Overwrite
                } else {
                    IngestMode::Append
                },
                ..Default::default()
            };

            // `_tmp` is bound, not dropped: it owns the converted file for the
            // duration of the ingest below.
            let is_csv = input
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("csv"));
            let (_tmp, source_file) = if is_csv {
                let (dir, parquet) = csv_to_temp_parquet(&input)?;
                (Some(dir), parquet)
            } else {
                (None, input.clone())
            };

            tracing::info!("Ingesting {} into {}/{}", input.display(), source, table);
            let info = store
                .ingest_parquet(&source, &table, &source_file, Some(options))
                .await?;
            // The detector gives the table its base semantic layer.
            brightflow_api::semantics::detect::declare_detected(&store, &source, &table).await?;

            println!("Ingested into table '{}/{}'", info.source_id, info.name);
            println!("Version: {}", info.version);
            println!("Files: {}", info.num_files);
        },

        StoreCommands::Export {
            source,
            table,
            output,
            path,
        } => {
            let store_path = path.unwrap_or_else(|| wp.store());
            let store = ParquetStore::new(&store_path, &litehouse_url).await?;
            let df = store.read_table(&source, &table).await?;

            // Create output directory if needed
            if let Some(parent) = output.parent() {
                std::fs::create_dir_all(parent)?;
            }

            // Write to CSV
            let mut file = std::fs::File::create(&output)?;
            polars::io::csv::write::CsvWriter::new(&mut file).finish(&mut df.clone())?;

            println!(
                "Exported table '{}/{}' to {}",
                source,
                table,
                output.display()
            );
            println!("Rows: {}", df.height());
        },

        StoreCommands::Delete {
            source,
            table,
            path,
            force,
        } => {
            if !force {
                println!(
                    "Are you sure you want to delete table '{source}/{table}'? This cannot be undone."
                );
                println!("Run with --force to confirm.");
                return Ok(());
            }

            let store_path = path.unwrap_or_else(|| wp.store());
            let store = ParquetStore::new(&store_path, &litehouse_url).await?;
            store.delete_table(&source, &table).await?;
            println!("Deleted table '{source}/{table}'");
        },
    }

    Ok(())
}
