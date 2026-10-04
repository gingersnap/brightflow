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
    /// (via drop).
    ///
    /// `BEGIN IMMEDIATE`, a deliberate divergence from the deferred `BEGIN`
    /// sqlx's `pool.begin()` gave these call sites. A deferred transaction that
    /// reads before it writes takes its read snapshot at the `SELECT`; if
    /// another connection commits in between, the later write fails with
    /// `SQLITE_BUSY_SNAPSHOT`, which SQLite does *not* route through the busy
    /// handler — so the `busy_timeout` set in `apply_pragmas` never applies and
    /// the caller just gets an error. Taking the write lock up front makes such
    /// a transaction wait on the busy handler instead, at the cost of
    /// serializing writers that would otherwise have overlapped.
    pub async fn transaction<T, F>(&self, f: F) -> Result<T, SqliteError>
    where
        F: FnOnce(&rusqlite::Transaction<'_>) -> rusqlite::Result<T> + Send + 'static,
        T: Send + 'static,
    {
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
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
/// filesystem paths, so hand-set env overrides in either form work. The path
/// `:memory:` opens an in-memory database on exactly one connection: every
/// SQLite connection to `:memory:` is its own empty database, so a larger
/// pool would hand out connections that cannot see each other's writes.
pub async fn open_pool(
    database_url: &str,
    profile: SqlitePoolProfile,
) -> Result<SqlitePool, SqliteError> {
    let path = brightflow_core::sqlite_db_path(database_url)
        .unwrap_or_else(|| PathBuf::from(database_url));
    let max = pool_size(&path, profile);
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

/// Connections to allow: the profile's bound, or one for `:memory:` (see
/// `open_pool`).
fn pool_size(path: &std::path::Path, profile: SqlitePoolProfile) -> usize {
    if path.as_os_str() == IN_MEMORY {
        return 1;
    }
    // usize::try_from is infallible for u32 on every supported target.
    usize::try_from(profile.max_connections).unwrap_or(usize::MAX)
}

/// SQLite's name for a private in-memory database.
const IN_MEMORY: &str = ":memory:";

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

    async fn memory_pool() -> SqlitePool {
        open_pool(IN_MEMORY, SqlitePoolProfile::METADATA)
            .await
            .expect("pool opens")
    }

    #[test]
    fn memory_pools_get_one_connection_and_files_get_the_profile() {
        let memory = std::path::Path::new(IN_MEMORY);
        assert_eq!(pool_size(memory, SqlitePoolProfile::METADATA), 1);
        let file = std::path::Path::new("/w/litehouse.db");
        assert_eq!(pool_size(file, SqlitePoolProfile::METADATA), 5);
        assert_eq!(pool_size(file, SqlitePoolProfile::BUFFER), 3);
    }

    #[tokio::test]
    async fn a_memory_pool_sees_its_own_writes_across_calls() {
        let pool = memory_pool().await;
        pool.call(|conn| conn.execute_batch("CREATE TABLE t (x); INSERT INTO t VALUES (1);"))
            .await
            .expect("write");
        let n: i64 = pool
            .call(|conn| conn.query_row("SELECT count(*) FROM t", [], |r| r.get(0)))
            .await
            .expect("read on a later checkout");
        assert_eq!(n, 1);
    }

    #[tokio::test]
    async fn pooled_connections_have_foreign_keys() {
        let pool = memory_pool().await;

        // Assert on a *pooled* connection: this catches the hook not running,
        // which a direct Connection::open would not.
        let fk: i64 = pool
            .call(|conn| conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0)))
            .await
            .expect("pragma readable");
        assert_eq!(fk, 1);
    }

    #[tokio::test]
    async fn unique_violation_is_detected() {
        let pool = memory_pool().await;
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
        let pool = memory_pool().await;
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
