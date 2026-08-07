//! Deadpool-managed rusqlite pools with the shared pragma profile.
//!
//! SQLite has no async API, so every connection lives on a blocking thread and
//! work is sent to it as closures (`call`/`transaction`); the pool exists to
//! bound how many such threads a database uses, not to make SQLite concurrent.
//! Pragmas are applied in a post-create hook because everything except the
//! journal mode is per-connection state — applying them anywhere else would
//! leave connections 2..N without foreign keys or a busy timeout.

use std::path::PathBuf;
use std::time::Duration;

use deadpool_sqlite::{Config, Hook, HookError, Runtime};
use rusqlite::Connection;

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

/// Errors from opening a pool or running work on one of its connections.
#[derive(Debug, thiserror::Error)]
pub enum SqliteError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("connection pool error: {0}")]
    Pool(#[from] deadpool_sqlite::PoolError),
    /// The closure passed to `call` panicked or the pool was aborted. Nothing
    /// matches on the cause, so it is flattened to a message.
    #[error("connection task failed: {0}")]
    Interact(String),
    #[error("pool configuration error: {0}")]
    Config(String),
}

impl SqliteError {
    /// True for `SQLITE_CONSTRAINT_UNIQUE` and `SQLITE_CONSTRAINT_PRIMARYKEY`
    /// failures — the same pair sqlx's `is_unique_violation` reported, so
    /// callers mapping duplicates to conflict responses keep their behavior.
    #[must_use]
    pub fn is_unique_violation(&self) -> bool {
        match self {
            Self::Sqlite(rusqlite::Error::SqliteFailure(e, _)) => {
                e.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
                    || e.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY
            },
            _ => false,
        }
    }
}

/// Cloneable handle over a deadpool of rusqlite connections.
#[derive(Clone, Debug)]
pub struct SqlitePool {
    inner: deadpool_sqlite::Pool,
}

impl SqlitePool {
    /// Run a blocking closure on a pooled connection's dedicated thread.
    ///
    /// This is the only place the interact double-`Result` (transport error
    /// around the closure's own error) is flattened; call sites must never
    /// see it.
    pub async fn call<T, F>(&self, f: F) -> Result<T, SqliteError>
    where
        F: FnOnce(&mut Connection) -> rusqlite::Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let conn = self.inner.get().await?;
        let res = conn
            .interact(f)
            .await
            .map_err(|e| SqliteError::Interact(e.to_string()))?;
        Ok(res?)
    }

    /// Like `call` for closures with a caller-chosen error type (the migrator
    /// threads its own error through this).
    pub async fn call_result<T, E, F>(&self, f: F) -> Result<T, E>
    where
        F: FnOnce(&mut Connection) -> Result<T, E> + Send + 'static,
        T: Send + 'static,
        E: From<SqliteError> + Send + 'static,
    {
        let conn = self
            .inner
            .get()
            .await
            .map_err(SqliteError::from)
            .map_err(E::from)?;
        conn.interact(f)
            .await
            .map_err(|e| E::from(SqliteError::Interact(e.to_string())))?
    }

    /// Run a closure inside a transaction: commit on `Ok`, rollback on `Err`
    /// (via drop). Deferred `BEGIN`, matching the semantics call sites had
    /// with sqlx's `pool.begin()`.
    pub async fn transaction<T, F>(&self, f: F) -> Result<T, SqliteError>
    where
        F: FnOnce(&rusqlite::Transaction<'_>) -> rusqlite::Result<T> + Send + 'static,
        T: Send + 'static,
    {
        self.call(move |conn| {
            let tx = conn.transaction()?;
            let out = f(&tx)?;
            tx.commit()?;
            Ok(out)
        })
        .await
    }

    /// Mark the pool closed and drop idle connections. In-flight closures
    /// finish; their connections are dropped on return instead of recycled.
    /// Unlike sqlx's `close().await` this does not wait for stragglers —
    /// callers deleting the database file behind the pool rely on unlink
    /// semantics (see `EventBuffer::delete_source`).
    pub fn close(&self) {
        self.inner.close();
    }
}

/// Open (creating if missing) a pool with the shared pragmas on every
/// connection.
///
/// Accepts both the workspace's `sqlite:{path}?mode=rwc` URLs and plain
/// filesystem paths, so hand-set env overrides in either form work.
pub async fn open_pool(
    database_url: &str,
    profile: SqlitePoolProfile,
) -> Result<SqlitePool, SqliteError> {
    let path = brightflow_core::sqlite_db_path(database_url)
        .unwrap_or_else(|| PathBuf::from(database_url));
    // usize::try_from is infallible for u32 on every supported target.
    let max = usize::try_from(profile.max_connections).unwrap_or(usize::MAX);
    let pool = Config::new(path)
        .builder(Runtime::Tokio1)
        .map_err(|e| SqliteError::Config(e.to_string()))?
        .max_size(max)
        .post_create(Hook::async_fn(move |conn, _metrics| {
            Box::pin(async move {
                conn.interact(move |conn| apply_pragmas(conn, profile))
                    .await
                    .map_err(|e| HookError::message(e.to_string()))?
                    .map_err(HookError::Backend)
            })
        }))
        .build()
        .map_err(|e| SqliteError::Config(e.to_string()))?;

    // Check out one connection eagerly so a bad path or unwritable directory
    // fails here, not on first query — constructors (`StoreDb::new` etc.)
    // rely on surfacing setup errors immediately, as sqlx's connect did.
    drop(pool.get().await?);

    Ok(SqlitePool { inner: pool })
}

/// The shared pragma set, applied per connection (see module doc).
fn apply_pragmas(conn: &Connection, profile: SqlitePoolProfile) -> rusqlite::Result<()> {
    // busy_timeout goes through the C API: the pragma form returns a row, and
    // the API call cannot be mistyped in a format string.
    conn.busy_timeout(Duration::from_secs(5))?;
    // Some of these pragmas (journal_mode, mmap_size) echo a result row;
    // execute_batch steps each statement once and ignores rows, which is
    // exactly what we want. Values are interpolated raw, as sqlx did — the
    // profile fields are compile-time constants, not user input.
    conn.execute_batch(&format!(
        "PRAGMA journal_mode = WAL;\n\
         PRAGMA foreign_keys = ON;\n\
         PRAGMA synchronous = NORMAL;\n\
         PRAGMA cache_size = {};\n\
         PRAGMA mmap_size = {};\n\
         PRAGMA temp_store = MEMORY;",
        profile.cache_size, profile.mmap_size,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn temp_pool(dir: &tempfile::TempDir) -> SqlitePool {
        let url = format!("sqlite:{}?mode=rwc", dir.path().join("t.db").display());
        open_pool(&url, SqlitePoolProfile::METADATA)
            .await
            .expect("pool opens")
    }

    #[tokio::test]
    async fn pooled_connections_have_wal_and_foreign_keys() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pool = temp_pool(&dir).await;

        // Assert on a *pooled* connection: this catches the hook not running,
        // which a direct Connection::open would not.
        let (fk, mode): (i64, String) = pool
            .call(|conn| {
                let fk = conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0))?;
                let mode = conn.query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
                Ok((fk, mode))
            })
            .await
            .expect("pragmas readable");
        assert_eq!(fk, 1);
        assert_eq!(mode, "wal");
    }

    #[tokio::test]
    async fn plain_path_without_url_prefix_is_accepted() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("plain.db").display().to_string();
        let pool = open_pool(&path, SqlitePoolProfile::BUFFER)
            .await
            .expect("plain path opens");
        pool.call(|conn| conn.execute_batch("CREATE TABLE t (x)"))
            .await
            .expect("usable");
    }

    #[tokio::test]
    async fn unique_violation_is_detected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pool = temp_pool(&dir).await;
        pool.call(|conn| conn.execute_batch("CREATE TABLE u (id TEXT PRIMARY KEY, v TEXT UNIQUE)"))
            .await
            .expect("create");
        pool.call(|conn| {
            conn.execute("INSERT INTO u (id, v) VALUES ('a', 'x')", [])
                .map(|_| ())
        })
        .await
        .expect("first insert");

        let dup_unique = pool
            .call(|conn| {
                conn.execute("INSERT INTO u (id, v) VALUES ('b', 'x')", [])
                    .map(|_| ())
            })
            .await
            .expect_err("duplicate unique value");
        assert!(dup_unique.is_unique_violation());

        let dup_pk = pool
            .call(|conn| {
                conn.execute("INSERT INTO u (id, v) VALUES ('a', 'y')", [])
                    .map(|_| ())
            })
            .await
            .expect_err("duplicate primary key");
        assert!(dup_pk.is_unique_violation());

        let not_unique = pool
            .call(|conn| conn.execute("INSERT INTO nope VALUES (1)", []).map(|_| ()))
            .await
            .expect_err("missing table");
        assert!(!not_unique.is_unique_violation());
    }

    #[tokio::test]
    async fn transaction_commits_on_ok_and_rolls_back_on_err() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pool = temp_pool(&dir).await;
        pool.call(|conn| conn.execute_batch("CREATE TABLE t (x INTEGER)"))
            .await
            .expect("create");

        pool.transaction(|tx| {
            tx.execute("INSERT INTO t (x) VALUES (1)", [])?;
            Ok(())
        })
        .await
        .expect("commit");

        let failed = pool
            .transaction(|tx| {
                tx.execute("INSERT INTO t (x) VALUES (2)", [])?;
                tx.execute("INSERT INTO nope VALUES (1)", [])?;
                Ok(())
            })
            .await;
        assert!(failed.is_err());

        let rows: i64 = pool
            .call(|conn| conn.query_row("SELECT count(*), max(x) FROM t", [], |r| r.get(0)))
            .await
            .expect("count");
        assert_eq!(rows, 1, "failed transaction must roll back its insert");
    }
}
