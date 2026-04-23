use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use polars::prelude::*;

use super::buffer::EventBuffer;
use super::error::IngestResult;
use super::models::Event;

use brightflow_store::ParquetStore;

const DEFAULT_FLUSH_INTERVAL_SECS: u64 = 60;
const DEFAULT_BATCH_SIZE: i64 = 10_000;

/// Background task that flushes buffered events from SQLite to Parquet files.
pub struct FlushTask {
    buffer: Arc<EventBuffer>,
    events_store_path: PathBuf,
    flush_interval: Duration,
    batch_size: i64,
    store: Option<Arc<ParquetStore>>,
}

impl FlushTask {
    #[must_use]
    pub fn new(
        buffer: Arc<EventBuffer>,
        events_store_path: PathBuf,
        store: Option<Arc<ParquetStore>>,
    ) -> Self {
        Self {
            buffer,
            events_store_path,
            flush_interval: Duration::from_secs(DEFAULT_FLUSH_INTERVAL_SECS),
            batch_size: DEFAULT_BATCH_SIZE,
            store,
        }
    }

    /// Run the flush loop forever (spawn this with `tokio::spawn`).
    pub async fn start(&self) {
        tracing::info!(
            "Event flush task started (interval: {}s)",
            self.flush_interval.as_secs()
        );
        loop {
            tokio::time::sleep(self.flush_interval).await;
            if let Err(e) = self.tick().await {
                tracing::error!("Flush tick error: {e}");
            }
        }
    }

    async fn tick(&self) -> IngestResult<()> {
        let sources = self.buffer.source_ids();
        if sources.is_empty() {
            tracing::debug!("Flush tick: no sources with buffered events");
        }
        for source_id in sources {
            tracing::info!("Flushing source {source_id}...");
            if let Err(e) = self.flush_source(&source_id).await {
                tracing::error!("Failed to flush source {source_id}: {e}");
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    async fn flush_source(&self, source_id: &str) -> IngestResult<()> {
        let pool = self.buffer.get_pool(source_id).await?;

        // Fetch buffered events
        let rows: Vec<Event> =
            sqlx::query_as::<_, Event>("SELECT * FROM events ORDER BY timestamp LIMIT ?")
                .bind(self.batch_size)
                .fetch_all(&pool)
                .await?;

        if rows.is_empty() {
            return Ok(());
        }

        let count = rows.len();
        let ids: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();

        // Determine date partition from first row
        let date = rows
            .first()
            .map_or("unknown", |r| r.timestamp.get(..10).unwrap_or("unknown"))
            .to_string();

        // Build DataFrame
        let df = rows_to_dataframe(&rows)?;

        // Clone df for stats extraction (Polars clone is cheap — Arc<Vec<Series>>)
        let df_for_stats = df.clone();

        // Write Parquet
        let dir = self.events_store_path.join(source_id).join(&date);
        std::fs::create_dir_all(&dir)?;
        let filename = format!("{}.parquet", uuid::Uuid::now_v7());
        let path = dir.join(&filename);

        let path_clone = path.clone();
        tokio::task::spawn_blocking(move || -> IngestResult<()> {
            let file = std::fs::File::create(&path_clone)?;
            let mut df_owned = df;
            ParquetWriter::new(file)
                .with_compression(ParquetCompression::Zstd(None))
                .finish(&mut df_owned)?;
            Ok(())
        })
        .await??;

        // Register file in the catalog store
        if let Some(store) = &self.store {
            let table_name = format!("events_{source_id}");
            let file_stats = brightflow_store::extract_file_column_stats(&df_for_stats, "");
            if let Err(e) = store
                .register_file(
                    &format!("web:{source_id}"),
                    &table_name,
                    &path,
                    &[("date", date.as_str())],
                    Some(&["date"]),
                    Some(file_stats),
                )
                .await
            {
                tracing::error!("Failed to register flushed file in catalog: {e}");
            }
        }

        // Delete flushed rows in batches (SQLite variable limit is 999)
        for chunk in ids.chunks(999) {
            let placeholders: String = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let query = format!("DELETE FROM events WHERE id IN ({placeholders})");
            let mut q = sqlx::query(&query);
            for id in chunk {
                q = q.bind(id);
            }
            q.execute(&pool).await?;
        }

        tracing::info!(
            "Flushed {count} events for source {source_id} to {}",
            path.display()
        );
        Ok(())
    }
}

/// Convert a Vec<Event> into a Polars DataFrame.
fn rows_to_dataframe(rows: &[Event]) -> IngestResult<DataFrame> {
    macro_rules! str_col {
        ($field:ident, $rows:expr) => {
            Column::new(
                stringify!($field).into(),
                $rows.iter().map(|r| r.$field.as_str()).collect::<Vec<_>>(),
            )
        };
    }

    let df = DataFrame::new(vec![
        str_col!(id, rows),
        str_col!(timestamp, rows),
        str_col!(source_id, rows),
        str_col!(event_name, rows),
        str_col!(visitor_id, rows),
        str_col!(session_id, rows),
        str_col!(user_id, rows),
        str_col!(hostname, rows),
        str_col!(pathname, rows),
        str_col!(page_url, rows),
        str_col!(referrer, rows),
        str_col!(referrer_source, rows),
        str_col!(utm_source, rows),
        str_col!(utm_medium, rows),
        str_col!(utm_campaign, rows),
        str_col!(utm_content, rows),
        str_col!(utm_term, rows),
        str_col!(browser, rows),
        str_col!(browser_version, rows),
        str_col!(os, rows),
        str_col!(os_version, rows),
        str_col!(device_type, rows),
        str_col!(screen_size, rows),
        str_col!(country, rows),
        str_col!(region, rows),
        str_col!(city, rows),
        str_col!(properties, rows),
    ])?;

    Ok(df)
}
