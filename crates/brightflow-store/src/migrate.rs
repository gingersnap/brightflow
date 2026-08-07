//! In-tree forward-only migrator, replacing `sqlx::migrate!`.
//!
//! Each database crate embeds its `migrations/*.sql` files at compile time
//! via the `migration!` macro and passes the list to `migrate` at startup.
//! Applied versions are tracked in a `schema_migrations` table owned by this
//! module. There is deliberately no compatibility with sqlx's
//! `_sqlx_migrations` table: databases predating the sqlx removal are
//! disposable and are recreated from scratch, not adopted.
//!
//! Each pending file runs inside its own transaction (batch + tracking
//! insert), so a failing migration leaves the database at the previous
//! version instead of half-applied — the same per-file semantics sqlx had.

use rusqlite::Connection;

use crate::pool::{SqliteError, SqlitePool};

/// One embedded migration file. Construct with the `migration!` macro so the
/// SQL is included at compile time from the invoking crate's `migrations/`.
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    pub version: i64,
    pub name: &'static str,
    pub sql: &'static str,
}

/// Errors from validating or applying a migration list.
#[derive(Debug, thiserror::Error)]
pub enum MigrateError {
    #[error("migration {version} ({name}) failed: {source}")]
    Apply {
        version: i64,
        name: String,
        #[source]
        source: rusqlite::Error,
    },
    /// The embedded list and the database disagree (gap, rename, or a
    /// database ahead of the binary). Nothing is applied in this state:
    /// guessing would risk re-running destructive statements.
    #[error("invalid migration state: {0}")]
    Invalid(String),
    #[error(transparent)]
    Db(#[from] rusqlite::Error),
    #[error(transparent)]
    Pool(#[from] SqliteError),
}

/// Embed a migration file from the invoking crate's `migrations/` directory.
///
/// A custom directory can be given with the three-argument form. `env!` and
/// `include_str!` expand at the invocation site, so each crate embeds its own
/// files even though the macro lives here.
#[macro_export]
macro_rules! migration {
    ($version:expr, $name:literal) => {
        $crate::Migration {
            version: $version,
            name: $name,
            sql: include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/migrations/",
                $name,
                ".sql"
            )),
        }
    };
    ($version:expr, $dir:literal, $name:literal) => {
        $crate::Migration {
            version: $version,
            name: $name,
            sql: include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/",
                $dir,
                "/",
                $name,
                ".sql"
            )),
        }
    };
}

/// Apply all pending migrations on a pooled connection.
pub async fn migrate(
    pool: &SqlitePool,
    migrations: &'static [Migration],
) -> Result<(), MigrateError> {
    pool.call_result(move |conn| run_migrations(conn, migrations))
        .await
}

/// Sync core of `migrate`, usable directly in tests and CLI tools.
pub fn run_migrations(conn: &mut Connection, migrations: &[Migration]) -> Result<(), MigrateError> {
    validate_list(migrations)?;

    // A database migrated by the pre-rusqlite sqlx stack has schema but no
    // schema_migrations rows, so re-applying from version 1 would fail on the
    // first non-idempotent statement with something baffling ("duplicate
    // column name"). Refuse with the actual explanation instead: these
    // databases are disposable by decision, not adoptable.
    let sqlx_era: i64 = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE name = '_sqlx_migrations'",
        [],
        |r| r.get(0),
    )?;
    if sqlx_era > 0 {
        return Err(MigrateError::Invalid(
            "this database was created before the move off sqlx (it has a _sqlx_migrations \
             table) and cannot be migrated in place — delete the .db/.db-wal/.db-shm files \
             and restart to recreate it"
                .to_owned(),
        ));
    }

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )?;

    let applied: Vec<(i64, String)> = {
        let mut stmt =
            conn.prepare("SELECT version, name FROM schema_migrations ORDER BY version")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };

    for (version, name) in &applied {
        let Some(m) = migrations.iter().find(|m| m.version == *version) else {
            return Err(MigrateError::Invalid(format!(
                "database has applied version {version} ({name}) which this binary does not know \
                 — database is newer than the code"
            )));
        };
        if m.name != name {
            return Err(MigrateError::Invalid(format!(
                "applied version {version} is named {name:?} in the database but {:?} in the code",
                m.name
            )));
        }
    }

    for m in migrations {
        if applied.iter().any(|(v, _)| *v == m.version) {
            continue;
        }
        let tx = conn.transaction()?;
        tx.execute_batch(m.sql)
            .map_err(|source| MigrateError::Apply {
                version: m.version,
                name: m.name.to_owned(),
                source,
            })?;
        tx.execute(
            "INSERT INTO schema_migrations (version, name) VALUES (?1, ?2)",
            (m.version, m.name),
        )?;
        tx.commit()?;
    }

    Ok(())
}

/// Versions must be dense from 1 in list order: a gap or reorder means a file
/// was lost or misnumbered, and applying around it would diverge from every
/// database migrated before the mistake.
fn validate_list(migrations: &[Migration]) -> Result<(), MigrateError> {
    for (i, m) in migrations.iter().enumerate() {
        let expected = i64::try_from(i).unwrap_or(i64::MAX) + 1;
        if m.version != expected {
            return Err(MigrateError::Invalid(format!(
                "expected version {expected} at position {i}, found {} ({})",
                m.version, m.name
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const M1: Migration = Migration {
        version: 1,
        name: "001_users",
        sql: "CREATE TABLE users (id TEXT PRIMARY KEY);
              CREATE INDEX idx_users ON users (id);",
    };
    const M2: Migration = Migration {
        version: 2,
        name: "002_posts",
        sql: "CREATE TABLE posts (id TEXT PRIMARY KEY, user_id TEXT REFERENCES users(id));",
    };

    fn conn() -> Connection {
        Connection::open_in_memory().expect("in-memory db")
    }

    fn applied_versions(conn: &Connection) -> Vec<i64> {
        let mut stmt = conn
            .prepare("SELECT version FROM schema_migrations ORDER BY version")
            .expect("prepare");
        let rows = stmt.query_map([], |r| r.get(0)).expect("query");
        rows.collect::<rusqlite::Result<_>>().expect("collect")
    }

    #[test]
    fn fresh_apply_runs_all_and_tracks_them() {
        let mut c = conn();
        run_migrations(&mut c, &[M1, M2]).expect("apply");
        assert_eq!(applied_versions(&c), vec![1, 2]);
        // Multi-statement batches ran fully: the index from M1 exists.
        let n: i64 = c
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name = 'idx_users'",
                [],
                |r| r.get(0),
            )
            .expect("index lookup");
        assert_eq!(n, 1);
    }

    #[test]
    fn rerun_is_idempotent_and_partial_state_resumes() {
        let mut c = conn();
        run_migrations(&mut c, &[M1]).expect("first");
        run_migrations(&mut c, &[M1, M2]).expect("resume");
        run_migrations(&mut c, &[M1, M2]).expect("idempotent");
        assert_eq!(applied_versions(&c), vec![1, 2]);
    }

    #[test]
    fn failing_migration_rolls_back_and_leaves_prior_versions() {
        let mut c = conn();
        let bad = Migration {
            version: 2,
            name: "002_bad",
            sql: "CREATE TABLE ok_table (id TEXT); INSERT INTO missing VALUES (1);",
        };
        let err = run_migrations(&mut c, &[M1, bad]).expect_err("must fail");
        assert!(matches!(err, MigrateError::Apply { version: 2, .. }));
        assert_eq!(applied_versions(&c), vec![1]);
        // The failed file's earlier statements were rolled back too.
        let n: i64 = c
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name = 'ok_table'",
                [],
                |r| r.get(0),
            )
            .expect("table lookup");
        assert_eq!(n, 0);
    }

    #[test]
    fn renamed_and_unknown_applied_versions_are_rejected() {
        let mut c = conn();
        run_migrations(&mut c, &[M1, M2]).expect("apply");

        let renamed = Migration {
            name: "001_other",
            ..M1
        };
        let err = run_migrations(&mut c, &[renamed, M2]).expect_err("rename");
        assert!(matches!(err, MigrateError::Invalid(_)));

        let err = run_migrations(&mut c, &[M1]).expect_err("db newer than code");
        assert!(matches!(err, MigrateError::Invalid(_)));
    }

    #[test]
    fn sqlx_era_database_is_refused_with_explanation() {
        let mut c = conn();
        c.execute_batch(
            "CREATE TABLE _sqlx_migrations (version BIGINT PRIMARY KEY);
             CREATE TABLE users (id TEXT PRIMARY KEY);",
        )
        .expect("old-stack schema");
        let err = run_migrations(&mut c, &[M1]).expect_err("must refuse");
        let msg = err.to_string();
        assert!(msg.contains("_sqlx_migrations"), "got: {msg}");
        assert!(msg.contains("delete"), "got: {msg}");
    }

    #[test]
    fn non_dense_versions_are_rejected() {
        let mut c = conn();
        let gap = Migration { version: 3, ..M2 };
        let err = run_migrations(&mut c, &[M1, gap]).expect_err("gap");
        assert!(matches!(err, MigrateError::Invalid(_)));
    }
}
