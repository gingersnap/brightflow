//! One-time bootstrap migration from manifest.json files to SQLite.
//!
//! Scans the store root for table directories containing `manifest.json`,
//! imports their metadata into the SQLite database, moves parquet files
//! from the `data/` subdirectory to the table root, and cleans up.

use std::path::Path;

use chrono::{DateTime, Utc};
use polars::prelude::*;
use serde::Deserialize;
use tracing::info;

use crate::db::StoreDb;
use crate::error::StoreResult;
use crate::stats;

/// Legacy manifest format (for deserialization only)
#[derive(Deserialize)]
struct LegacyManifest {
    name: String,
    #[serde(default)]
    version: u64,
    schema: Option<serde_json::Value>,
    #[serde(default)]
    primary_keys: Vec<String>,
    #[serde(default)]
    files: Vec<LegacyFileEntry>,
    #[serde(default)]
    total_rows: i64,
    #[allow(dead_code)]
    created_at: DateTime<Utc>,
    #[allow(dead_code)]
    updated_at: DateTime<Utc>,
}

#[derive(Deserialize)]
struct LegacyFileEntry {
    path: String,
    num_rows: i64,
    #[serde(default)]
    size_bytes: u64,
    #[allow(dead_code)]
    added_at: DateTime<Utc>,
}

/// Migrate any existing manifest.json-based tables to SQLite.
///
/// For each table directory with a manifest.json:
/// 1. Parse the manifest and insert table + file rows into SQLite
/// 2. Move parquet files from `{table}/data/*.parquet` to `{table}/*.parquet`
/// 3. Delete the manifest.json and empty data/ directory
/// 4. Compute and store column stats
///
/// This is a no-op if no manifest.json files exist.
pub async fn migrate_manifests(db: &StoreDb, root: &Path) -> StoreResult<()> {
    if !root.exists() {
        return Ok(());
    }

    let entries = std::fs::read_dir(root)?;
    let mut migrated = 0u32;

    for entry_result in entries {
        let entry = entry_result?;
        let table_dir = entry.path();

        if !table_dir.is_dir() {
            continue;
        }

        let manifest_path = table_dir.join("manifest.json");
        if !manifest_path.exists() {
            continue;
        }

        let content = std::fs::read_to_string(&manifest_path)?;
        let manifest: LegacyManifest = match serde_json::from_str(&content) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(
                    "Skipping malformed manifest at {}: {e}",
                    manifest_path.display()
                );
                continue;
            },
        };

        // Skip if already migrated (table exists in db)
        if db.get_table_by_name(&manifest.name).await?.is_some() {
            // Clean up the manifest file since the table is already in the db
            std::fs::remove_file(&manifest_path).ok();
            continue;
        }

        info!(
            "Migrating table '{}' from manifest.json to SQLite",
            manifest.name
        );

        // Create table in db
        let table_row = db.create_table(&manifest.name).await?;

        let schema_str = manifest
            .schema
            .as_ref()
            .and_then(|s| serde_json::to_string(s).ok());
        let pks_str = if manifest.primary_keys.is_empty() {
            None
        } else {
            serde_json::to_string(&manifest.primary_keys).ok()
        };

        // Update version to match the old manifest
        for _ in 0..manifest.version {
            db.update_table_meta(
                &table_row.id,
                schema_str.as_deref(),
                pks_str.as_deref(),
                manifest.total_rows,
            )
            .await?;
        }
        if manifest.version == 0 {
            db.update_table_meta(
                &table_row.id,
                schema_str.as_deref(),
                pks_str.as_deref(),
                manifest.total_rows,
            )
            .await?;
        }

        // Move files from data/ subdirectory to table root and register in db
        for legacy_file in &manifest.files {
            let old_path = table_dir.join(&legacy_file.path);
            if !old_path.exists() {
                continue;
            }

            // Generate new UUIDv7 filename in the table directory (no data/ subdir)
            let new_filename = format!("{}.parquet", uuid::Uuid::now_v7());
            let new_path = table_dir.join(&new_filename);
            let relative_path = format!("{}/{new_filename}", manifest.name);

            std::fs::rename(&old_path, &new_path)?;

            let size_bytes = i64::try_from(legacy_file.size_bytes).unwrap_or(0);
            db.add_table_file(
                &table_row.id,
                &relative_path,
                legacy_file.num_rows,
                size_bytes,
            )
            .await?;
        }

        // Compute column stats from the migrated files
        let file_rows = db.list_table_files(&table_row.id).await?;
        if let Some(first_file) = file_rows.first() {
            let abs_path = root.join(&first_file.path);
            if abs_path.exists() {
                if let Ok(file) = std::fs::File::open(&abs_path) {
                    if let Ok(df) = ParquetReader::new(file).finish() {
                        let column_stats = stats::extract_column_stats(&df, &table_row.id);
                        db.upsert_column_stats(&table_row.id, &column_stats).await?;
                    }
                }
            }
        }

        // Cleanup: remove manifest.json and empty data/ directory
        std::fs::remove_file(&manifest_path).ok();
        let data_dir = table_dir.join("data");
        if data_dir.exists() && data_dir.is_dir() {
            // Only remove if empty
            std::fs::remove_dir(&data_dir).ok();
        }

        migrated += 1;
    }

    if migrated > 0 {
        info!("Migrated {migrated} table(s) from manifest.json to SQLite");
    }

    Ok(())
}
