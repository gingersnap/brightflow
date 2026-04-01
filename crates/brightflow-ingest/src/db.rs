use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;

use crate::error::{IngestError, IngestResult};
use crate::models::{CreateSourceRequest, Source, UpdateSourceRequest, UserProfile};

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

    // ── User Profiles ────────────────────────────────────────────

    /// Upsert a user profile, merging new traits with existing ones.
    pub async fn upsert_user_profile(
        &self,
        user_id: &str,
        source_id: &str,
        new_traits: &std::collections::HashMap<String, serde_json::Value>,
    ) -> IngestResult<UserProfile> {
        // Fetch existing traits
        let existing = self.get_user_profile(user_id, source_id).await?;
        let mut merged: std::collections::HashMap<String, serde_json::Value> =
            if let Some(ref profile) = existing {
                serde_json::from_str(&profile.traits).unwrap_or_default()
            } else {
                std::collections::HashMap::new()
            };

        // Merge new traits over existing
        for (k, v) in new_traits {
            merged.insert(k.clone(), v.clone());
        }

        let traits_json = serde_json::to_string(&merged)
            .map_err(|e| IngestError::Other(format!("Failed to serialize traits: {e}")))?;

        sqlx::query(
            "INSERT INTO user_profiles (user_id, source_id, traits)
             VALUES (?, ?, ?)
             ON CONFLICT(user_id, source_id) DO UPDATE SET
                traits = excluded.traits,
                updated_at = datetime('now')",
        )
        .bind(user_id)
        .bind(source_id)
        .bind(&traits_json)
        .execute(&self.pool)
        .await?;

        // Return the updated profile
        self.get_user_profile(user_id, source_id)
            .await?
            .ok_or_else(|| IngestError::Other("Profile not found after upsert".to_string()))
    }

    /// Get a user profile by user_id and source_id.
    pub async fn get_user_profile(
        &self,
        user_id: &str,
        source_id: &str,
    ) -> IngestResult<Option<UserProfile>> {
        let profile = sqlx::query_as::<_, UserProfile>(
            "SELECT * FROM user_profiles WHERE user_id = ? AND source_id = ?",
        )
        .bind(user_id)
        .bind(source_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(profile)
    }

    /// Search user profiles by user_id prefix.
    pub async fn search_user_profiles(
        &self,
        source_id: &str,
        query: &str,
        limit: u32,
    ) -> IngestResult<Vec<UserProfile>> {
        let profiles = sqlx::query_as::<_, UserProfile>(
            "SELECT * FROM user_profiles
             WHERE source_id = ? AND user_id LIKE ?
             ORDER BY updated_at DESC
             LIMIT ?",
        )
        .bind(source_id)
        .bind(format!("{query}%"))
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(profiles)
    }
}
