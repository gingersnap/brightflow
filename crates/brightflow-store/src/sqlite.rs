//! Shared SQLite pool bootstrap for every embedded database in the workspace.
//!
//! Store catalog, auth users, ingest metadata, scheduler state, and the
//! per-source event buffers all open SQLite the same way: WAL journaling with
//! `synchronous = NORMAL` (the latency/durability trade for local databases
//! that are not sources of truth), foreign keys on, in-memory temp store, and
//! a 5s busy timeout. Keeping the pragmas in one place is what stops the five
//! call sites from drifting apart; only the sizing differs, via a profile.

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;
use std::time::Duration;

/// Sizing knobs that differ between database roles.
#[derive(Debug, Clone, Copy)]
pub struct SqlitePoolProfile {
    pub max_connections: u32,
    /// `cache_size` pragma value (negative = KiB).
    pub cache_size: &'static str,
    /// `mmap_size` pragma value in bytes.
    pub mmap_size: &'static str,
}

impl SqlitePoolProfile {
    /// Long-lived metadata catalogs: store, auth, ingest sources, scheduler.
    pub const METADATA: Self = Self {
        max_connections: 5,
        cache_size: "-64000",
        mmap_size: "268435456",
    };

    /// Per-source event buffers: many pools can be alive at once, so each
    /// gets fewer connections and a smaller cache/mmap footprint.
    pub const BUFFER: Self = Self {
        max_connections: 3,
        cache_size: "-16000",
        mmap_size: "67108864",
    };
}

/// Open (creating if missing) a SQLite pool with the shared pragmas.
///
/// Migrations are deliberately left to the caller: each database owns its own
/// `migrations/` directory and schema authority.
pub async fn open_sqlite_pool(
    database_url: &str,
    profile: SqlitePoolProfile,
) -> Result<SqlitePool, sqlx::Error> {
    let options = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true)
        .pragma("synchronous", "NORMAL")
        .pragma("cache_size", profile.cache_size)
        .pragma("mmap_size", profile.mmap_size)
        .pragma("temp_store", "MEMORY")
        .busy_timeout(Duration::from_secs(5));

    SqlitePoolOptions::new()
        .max_connections(profile.max_connections)
        .connect_with(options)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn pool_opens_with_wal_and_foreign_keys() {
        let dir = tempfile::tempdir().expect("tempdir");
        let url = format!("sqlite:{}?mode=rwc", dir.path().join("t.db").display());
        let pool = open_sqlite_pool(&url, SqlitePoolProfile::METADATA)
            .await
            .expect("pool opens");

        let (fk,): (i64,) = sqlx::query_as("PRAGMA foreign_keys")
            .fetch_one(&pool)
            .await
            .expect("pragma foreign_keys");
        assert_eq!(fk, 1);

        let (mode,): (String,) = sqlx::query_as("PRAGMA journal_mode")
            .fetch_one(&pool)
            .await
            .expect("pragma journal_mode");
        assert_eq!(mode, "wal");
    }
}
