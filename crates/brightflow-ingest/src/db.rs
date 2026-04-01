use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;

use crate::error::{IngestError, IngestResult};
use crate::models::{CreateSourceRequest, Source, UpdateSourceRequest};

/// Central ingest metadata database (sources + salts).
#[derive(Debug, Clone)]
pub struct IngestDb {
    pool: SqlitePool,
}

impl IngestDb {
    /// Open or create the ingest metadata database.
    pub async fn new(database_url: &str) -> IngestResult<Self> {
        let options: SqliteConnectOptions = database_url
            .parse::<SqliteConnectOptions>()
            .map_err(|e| IngestError::Other(format!("Invalid database URL: {e}")))?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true)
            .pragma("synchronous", "NORMAL")
            .pragma("cache_size", "-64000")
            .pragma("mmap_size", "268435456")
            .pragma("temp_store", "MEMORY")
            .busy_timeout(Duration::from_secs(5));

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Self { pool })
    }

    // ── Sources CRUD ──────────────────────────────────────────────

    pub async fn create_source(&self, req: &CreateSourceRequest) -> IngestResult<Source> {
        let id = uuid::Uuid::now_v7().to_string();
        let tz = req.timezone.as_deref().unwrap_or("UTC");

        sqlx::query("INSERT INTO sources (id, domain, name, timezone) VALUES (?, ?, ?, ?)")
            .bind(&id)
            .bind(&req.domain)
            .bind(&req.name)
            .bind(tz)
            .execute(&self.pool)
            .await?;

        self.get_source(&id).await
    }

    pub async fn get_source(&self, id: &str) -> IngestResult<Source> {
        let source = sqlx::query_as::<_, Source>("SELECT * FROM sources WHERE id = ?")
            .bind(id)
            .fetch_one(&self.pool)
            .await?;
        Ok(source)
    }

    pub async fn get_source_by_domain(&self, domain: &str) -> IngestResult<Option<Source>> {
        let source = sqlx::query_as::<_, Source>("SELECT * FROM sources WHERE domain = ?")
            .bind(domain)
            .fetch_optional(&self.pool)
            .await?;
        Ok(source)
    }

    pub async fn list_sources(&self) -> IngestResult<Vec<Source>> {
        let sources = sqlx::query_as::<_, Source>("SELECT * FROM sources ORDER BY created_at DESC")
            .fetch_all(&self.pool)
            .await?;
        Ok(sources)
    }

    pub async fn update_source(&self, id: &str, req: &UpdateSourceRequest) -> IngestResult<Source> {
        if let Some(ref name) = req.name {
            sqlx::query("UPDATE sources SET name = ?, updated_at = datetime('now') WHERE id = ?")
                .bind(name)
                .bind(id)
                .execute(&self.pool)
                .await?;
        }
        if let Some(ref tz) = req.timezone {
            sqlx::query(
                "UPDATE sources SET timezone = ?, updated_at = datetime('now') WHERE id = ?",
            )
            .bind(tz)
            .bind(id)
            .execute(&self.pool)
            .await?;
        }
        self.get_source(id).await
    }

    pub async fn delete_source(&self, id: &str) -> IngestResult<()> {
        sqlx::query("DELETE FROM sources WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ── Salt Management ───────────────────────────────────────────

    /// Get or create the daily salt for visitor ID hashing.
    pub async fn get_or_create_salt(&self, date: &str) -> IngestResult<String> {
        // Try existing salt first
        let existing = sqlx::query_scalar::<_, String>("SELECT salt FROM salts WHERE date = ?")
            .bind(date)
            .fetch_optional(&self.pool)
            .await?;

        if let Some(salt) = existing {
            return Ok(salt);
        }

        // Generate new salt and insert (INSERT OR IGNORE handles races)
        let new_salt = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT OR IGNORE INTO salts (date, salt) VALUES (?, ?)")
            .bind(date)
            .bind(&new_salt)
            .execute(&self.pool)
            .await?;

        // Re-fetch to handle race conditions (another process may have inserted first)
        let confirmed_salt =
            sqlx::query_scalar::<_, String>("SELECT salt FROM salts WHERE date = ?")
                .bind(date)
                .fetch_one(&self.pool)
                .await?;

        Ok(confirmed_salt)
    }

    /// Clean up old salts (keep last N days).
    pub async fn cleanup_old_salts(&self, keep_days: i64) -> IngestResult<u64> {
        let result = sqlx::query("DELETE FROM salts WHERE date < date('now', ?)")
            .bind(format!("-{keep_days} days"))
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected())
    }
}
