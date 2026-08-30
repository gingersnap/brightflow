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

use brightflow_store::rusqlite::{params, Connection};
use brightflow_store::SqlitePool;
use dashmap::DashMap;

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
            brightflow_store::open_pool(&url, brightflow_store::SqlitePoolProfile::BUFFER).await?;

        // Create buffer table if it doesn't exist. execute_batch, not execute:
        // the constant is one CREATE TABLE plus three CREATE INDEX statements.
        pool.call(|conn| conn.execute_batch(BUFFER_TABLE_SQL))
            .await?;

        self.pools.insert(source_id.to_string(), pool.clone());
        Ok(pool)
    }

    /// Insert an event into the per-source buffer, deriving the session ID.
    pub async fn insert(&self, event: &mut Event) -> IngestResult<()> {
        let pool = self.get_pool(&event.source_id).await?;

        // One closure for the session lookup + insert: both statements run
        // back-to-back on the same connection, halving the request path's
        // pool round-trips versus separate calls.
        //
        // The clone buys that single round-trip: the closure must be 'static,
        // so it cannot borrow `event`. Measured before keeping it — cloning
        // this struct costs ~1.4 us against a ~440 us insert (0.3%), so the
        // durable write dominates and threading ownership through to avoid it
        // would trade real complexity for noise. Re-measure before changing
        // this, not after.
        let row = event.clone();
        let session_id = pool
            .call(move |conn| {
                let session_id =
                    derive_session_id(conn, &row.visitor_id, &row.user_id, &row.timestamp)?;
                conn.execute(
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
                    params![
                        row.id,
                        row.timestamp,
                        row.source_id,
                        row.event_name,
                        row.visitor_id,
                        session_id,
                        row.user_id,
                        row.hostname,
                        row.pathname,
                        row.page_url,
                        row.referrer,
                        row.referrer_source,
                        row.utm_source,
                        row.utm_medium,
                        row.utm_campaign,
                        row.utm_content,
                        row.utm_term,
                        row.browser,
                        row.browser_version,
                        row.os,
                        row.os_version,
                        row.device_type,
                        row.screen_size,
                        row.country,
                        row.region,
                        row.city,
                        row.properties,
                    ],
                )?;
                Ok(session_id)
            })
            .await?;

        event.session_id = session_id;
        Ok(())
    }

    /// Get all source IDs that have buffer databases.
    pub fn source_ids(&self) -> Vec<String> {
        self.pools.iter().map(|e| e.key().clone()).collect()
    }

    /// Delete the buffer database for a source. Sync: closing the pool does
    /// not block, and the file removals are small enough not to warrant a
    /// blocking-thread hop.
    pub fn delete_source(&self, source_id: &str) -> IngestResult<()> {
        if let Some((_, pool)) = self.pools.remove(source_id) {
            // close() does not wait for in-flight closures (sqlx's close did).
            // Safe on Unix: unlinking below leaves any straggler writing to the
            // detached inode, whose contents deletion discards anyway. On
            // Windows an in-flight writer could make the remove_file fail —
            // acceptable for a dev-machine platform, the next delete retries.
            pool.close();
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
fn derive_session_id(
    conn: &Connection,
    visitor_id: &str,
    user_id: &str,
    current_timestamp: &str,
) -> brightflow_store::rusqlite::Result<String> {
    // When user_id is known, look up by user_id for cross-day session continuity
    let last = if user_id.is_empty() {
        brightflow_store::fetch_optional::<(String, String), _>(
            conn,
            "SELECT session_id, timestamp FROM events
             WHERE visitor_id = ? ORDER BY timestamp DESC LIMIT 1",
            params![visitor_id],
        )?
    } else {
        brightflow_store::fetch_optional::<(String, String), _>(
            conn,
            "SELECT session_id, timestamp FROM events
             WHERE user_id = ? ORDER BY timestamp DESC LIMIT 1",
            params![user_id],
        )?
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

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded_conn() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory db");
        conn.execute_batch(BUFFER_TABLE_SQL).expect("schema");
        conn
    }

    fn insert_event(conn: &Connection, session: &str, visitor: &str, user: &str, ts: &str) {
        conn.execute(
            "INSERT INTO events (id, timestamp, event_name, visitor_id, session_id, user_id)
             VALUES (?, ?, 'pageview', ?, ?, ?)",
            params![uuid::Uuid::now_v7().to_string(), ts, visitor, session, user],
        )
        .expect("seed event");
    }

    #[test]
    fn reuses_session_within_timeout_and_rotates_after() {
        let conn = seeded_conn();
        insert_event(&conn, "s-1", "v-1", "", "2026-08-07T10:00:00+00:00");

        let same =
            derive_session_id(&conn, "v-1", "", "2026-08-07T10:29:00+00:00").expect("derive");
        assert_eq!(same, "s-1", "within 30 minutes keeps the session");

        let rotated =
            derive_session_id(&conn, "v-1", "", "2026-08-07T10:31:00+00:00").expect("derive");
        assert_ne!(rotated, "s-1", "after the timeout a new session starts");
    }

    #[test]
    fn user_id_lookup_survives_visitor_rotation() {
        let conn = seeded_conn();
        // Same user, but the daily visitor hash has rotated.
        insert_event(&conn, "s-1", "v-old", "u-1", "2026-08-07T10:00:00+00:00");

        let session =
            derive_session_id(&conn, "v-new", "u-1", "2026-08-07T10:10:00+00:00").expect("derive");
        assert_eq!(session, "s-1", "user_id lookup bridges the rotation");

        let anon =
            derive_session_id(&conn, "v-new", "", "2026-08-07T10:10:00+00:00").expect("derive");
        assert_ne!(anon, "s-1", "anonymous lookup only sees the new visitor id");
    }

    #[test]
    fn unparseable_timestamps_start_a_new_session() {
        let conn = seeded_conn();
        insert_event(&conn, "s-1", "v-1", "", "not-a-timestamp");
        let session =
            derive_session_id(&conn, "v-1", "", "2026-08-07T10:00:00+00:00").expect("derive");
        assert_ne!(session, "s-1");
    }
}
