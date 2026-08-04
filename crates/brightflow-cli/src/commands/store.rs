//! `store *`, `migrate-events`, `compact`: direct Litehouse operations.

use anyhow::Result;
use polars::prelude::SerWriter;

use brightflow_store::{IngestMode, IngestOptions, ParquetStore};

use crate::StoreCommands;

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

            tracing::info!("Ingesting {} into {}/{}", input.display(), source, table);
            let info = store
                .ingest_parquet(&source, &table, &input, Some(options))
                .await?;

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
