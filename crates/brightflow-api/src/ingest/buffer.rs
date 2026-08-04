//! Per-source SQLite write buffer for incoming events.
//!
//! Events land here first and are flushed to Parquet in batches. Writing each
//! event straight to Parquet would be pathological (one tiny file per hit), and
//! buffering in memory would lose events on restart — SQLite gives durability at
//! write-time with batching at read-time.
//!
//! One pool per source, created lazily: sources are independent, so a busy site
//! never blocks a quiet one behind the same write lock.

use std::path::PathBuf;

use dashmap::DashMap;
use sqlx::SqlitePool;

use super::error::IngestResult;
use super::models::Event;

const BUFFER_TABLE_SQL: &str = r"
CREATE TABLE IF NOT EXISTS events (
    id TEXT PRIMARY KEY NOT NULL,
    timestamp TEXT NOT NULL,
    source_id TEXT NOT NULL DEFAULT '',
    event_name TEXT NOT NULL,
    visitor_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    user_id TEXT NOT NULL DEFAULT '',
    hostname TEXT NOT NULL DEFAULT '',
    pathname TEXT NOT NULL DEFAULT '',
    page_url TEXT NOT NULL DEFAULT '',
    referrer TEXT NOT NULL DEFAULT '',
    referrer_source TEXT NOT NULL DEFAULT '',
    utm_source TEXT NOT NULL DEFAULT '',
    utm_medium TEXT NOT NULL DEFAULT '',
    utm_campaign TEXT NOT NULL DEFAULT '',
    utm_content TEXT NOT NULL DEFAULT '',
    utm_term TEXT NOT NULL DEFAULT '',
    browser TEXT NOT NULL DEFAULT '',
    browser_version TEXT NOT NULL DEFAULT '',
    os TEXT NOT NULL DEFAULT '',
    os_version TEXT NOT NULL DEFAULT '',
    device_type TEXT NOT NULL DEFAULT '',
    screen_size TEXT NOT NULL DEFAULT '',
    country TEXT NOT NULL DEFAULT '',
    region TEXT NOT NULL DEFAULT '',
    city TEXT NOT NULL DEFAULT '',
    properties TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp);
CREATE INDEX IF NOT EXISTS idx_events_visitor ON events(visitor_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_events_user ON events(user_id, timestamp DESC);
";

const SESSION_TIMEOUT_MINUTES: i64 = 30;

/// Manages per-source SQLite buffer databases.
///
/// Each source gets its own `.db` file, eliminating write contention
/// between sources.
pub struct EventBuffer {
    buffer_dir: PathBuf,
    pools: DashMap<String, SqlitePool>,
}

impl EventBuffer {
    #[must_use]
    pub fn new(buffer_dir: PathBuf) -> Self {
        Self {
            buffer_dir,
            pools: DashMap::new(),
        }
    }

    /// Get or create the SQLite pool for a source.
    pub async fn get_pool(&self, source_id: &str) -> IngestResult<SqlitePool> {
        if let Some(pool) = self.pools.get(source_id) {
            return Ok(pool.clone());
        }

        let db_path = self.buffer_dir.join(format!("{source_id}.db"));
        let url = format!("sqlite:{}?mode=rwc", db_path.display());

        let pool =
            brightflow_store::open_sqlite_pool(&url, brightflow_store::SqlitePoolProfile::BUFFER)
                .await?;

        // Create buffer table if it doesn't exist
        sqlx::query(BUFFER_TABLE_SQL).execute(&pool).await?;

        self.pools.insert(source_id.to_string(), pool.clone());
        Ok(pool)
    }

    /// Insert an event into the per-source buffer, deriving the session ID.
    pub async fn insert(&self, event: &mut Event) -> IngestResult<()> {
        let pool = self.get_pool(&event.source_id).await?;

        // Derive session ID (use user_id for session continuity when available)
        event.session_id =
            derive_session_id(&pool, &event.visitor_id, &event.user_id, &event.timestamp).await?;

        sqlx::query(
            "INSERT INTO events (
                id, timestamp, source_id, event_name, visitor_id, session_id, user_id,
                hostname, pathname, page_url,
                referrer, referrer_source,
                utm_source, utm_medium, utm_campaign, utm_content, utm_term,
                browser, browser_version, os, os_version, device_type, screen_size,
                country, region, city, properties
            ) VALUES (
                ?, ?, ?, ?, ?, ?, ?,
                ?, ?, ?,
                ?, ?,
                ?, ?, ?, ?, ?,
                ?, ?, ?, ?, ?, ?,
                ?, ?, ?, ?
            )",
        )
        .bind(&event.id)
        .bind(&event.timestamp)
        .bind(&event.source_id)
        .bind(&event.event_name)
        .bind(&event.visitor_id)
        .bind(&event.session_id)
        .bind(&event.user_id)
        .bind(&event.hostname)
        .bind(&event.pathname)
        .bind(&event.page_url)
        .bind(&event.referrer)
        .bind(&event.referrer_source)
        .bind(&event.utm_source)
        .bind(&event.utm_medium)
        .bind(&event.utm_campaign)
        .bind(&event.utm_content)
        .bind(&event.utm_term)
        .bind(&event.browser)
        .bind(&event.browser_version)
        .bind(&event.os)
        .bind(&event.os_version)
        .bind(&event.device_type)
        .bind(&event.screen_size)
        .bind(&event.country)
        .bind(&event.region)
        .bind(&event.city)
        .bind(&event.properties)
        .execute(&pool)
        .await?;

        Ok(())
    }

    /// Get all source IDs that have buffer databases.
    pub fn source_ids(&self) -> Vec<String> {
        self.pools.iter().map(|e| e.key().clone()).collect()
    }

    /// Delete the buffer database for a source.
    pub async fn delete_source(&self, source_id: &str) -> IngestResult<()> {
        if let Some((_, pool)) = self.pools.remove(source_id) {
            pool.close().await;
        }
        let db_path = self.buffer_dir.join(format!("{source_id}.db"));
        if db_path.exists() {
            std::fs::remove_file(&db_path)?;
        }
        // Also remove WAL/SHM files
        let wal_path = self.buffer_dir.join(format!("{source_id}.db-wal"));
        let shm_path = self.buffer_dir.join(format!("{source_id}.db-shm"));
        drop(std::fs::remove_file(wal_path));
        drop(std::fs::remove_file(shm_path));
        Ok(())
    }
}

/// Derive session ID based on gap-based session detection.
///
/// If a known `user_id` is provided, use it for session lookup (allows session
/// continuity across daily visitor hash rotation). Otherwise fall back to `visitor_id`.
///
/// If the last event from this identity was less than 30 minutes ago,
/// reuse the same session. Otherwise, start a new session.
async fn derive_session_id(
    pool: &SqlitePool,
    visitor_id: &str,
    user_id: &str,
    current_timestamp: &str,
) -> IngestResult<String> {
    // When user_id is known, look up by user_id for cross-day session continuity
    let last = if user_id.is_empty() {
        sqlx::query_as::<_, (String, String)>(
            "SELECT session_id, timestamp FROM events
             WHERE visitor_id = ? ORDER BY timestamp DESC LIMIT 1",
        )
        .bind(visitor_id)
        .fetch_optional(pool)
        .await?
    } else {
        sqlx::query_as::<_, (String, String)>(
            "SELECT session_id, timestamp FROM events
             WHERE user_id = ? ORDER BY timestamp DESC LIMIT 1",
        )
        .bind(user_id)
        .fetch_optional(pool)
        .await?
    };

    if let Some((last_session_id, last_ts)) = last {
        if let (Ok(last_time), Ok(current_time)) = (
            chrono::DateTime::parse_from_rfc3339(&last_ts),
            chrono::DateTime::parse_from_rfc3339(current_timestamp),
        ) {
            let gap = current_time.signed_duration_since(last_time).num_minutes();
            if gap < SESSION_TIMEOUT_MINUTES {
                return Ok(last_session_id);
            }
        }
    }

    // New session
    Ok(uuid::Uuid::now_v7().to_string())
}
