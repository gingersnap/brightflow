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
//!
//! Applied rows also carry a content hash. Version, order and name drift are
//! all detectable from the list alone, but an *edit to an already-applied
//! file* is not: the database is silently no longer what the code describes,
//! and every later database diverges from every earlier one. The hash is the
//! only thing that catches it, so it is checked before anything is applied.

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
    ensure_checksum_column(conn)?;

    let applied: Vec<(i64, String, Option<String>)> = {
        let mut stmt =
            conn.prepare("SELECT version, name, checksum FROM schema_migrations ORDER BY version")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };

    let mut backfill: Vec<(i64, String)> = Vec::new();
    for (version, name, checksum) in &applied {
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
        let expected = checksum_of(m.sql);
        match checksum {
            // Rows written before the column existed are grandfathered: their
            // content was never recorded, so there is nothing to compare
            // against. Backfill from the current file and start checking from
            // the next run — refusing them instead would brick every database
            // that predates this check, for no evidence of an actual edit.
            None => backfill.push((*version, expected)),
            Some(found) if *found != expected => {
                return Err(MigrateError::Invalid(format!(
                    "applied version {version} ({name}) no longer matches the file it was applied \
                     from (recorded {found}, code has {expected}) — an already-applied migration \
                     was edited; revert it and add a new migration instead"
                )));
            },
            Some(_) => {},
        }
    }
    for (version, checksum) in backfill {
        conn.execute(
            "UPDATE schema_migrations SET checksum = ?1 WHERE version = ?2",
            (checksum, version),
        )?;
    }

    for m in migrations {
        if applied.iter().any(|(v, _, _)| *v == m.version) {
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
            "INSERT INTO schema_migrations (version, name, checksum) VALUES (?1, ?2, ?3)",
            (m.version, m.name, checksum_of(m.sql)),
        )?;
        tx.commit()?;
    }

    Ok(())
}

/// Hex content hash of a migration's SQL, stored alongside the applied row.
///
/// blake3 rather than a hand-rolled hash because it is already a workspace
/// dependency; this guards against accidental edits, not tampering, so any
/// stable digest would do.
fn checksum_of(sql: &str) -> String {
    blake3::hash(sql.as_bytes()).to_hex().to_string()
}

/// Add the `checksum` column when an existing `schema_migrations` predates it.
///
/// This table is owned by this module, not by any migration file, so it
/// evolves here — a migration cannot alter the table that records migrations.
fn ensure_checksum_column(conn: &Connection) -> rusqlite::Result<()> {
    let present: i64 = conn.query_row(
        "SELECT count(*) FROM pragma_table_info('schema_migrations') WHERE name = 'checksum'",
        [],
        |r| r.get(0),
    )?;
    if present == 0 {
        conn.execute_batch("ALTER TABLE schema_migrations ADD COLUMN checksum TEXT")?;
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
    fn editing_an_applied_migration_is_rejected() {
        let mut c = conn();
        run_migrations(&mut c, &[M1, M2]).expect("apply");

        // Same version, same name, different SQL — the one drift the version
        // list cannot show.
        let edited = Migration {
            sql: "CREATE TABLE users (id TEXT PRIMARY KEY, extra TEXT);",
            ..M1
        };
        let err = run_migrations(&mut c, &[edited, M2]).expect_err("edited file");
        let msg = err.to_string();
        assert!(matches!(err, MigrateError::Invalid(_)));
        assert!(msg.contains("no longer matches"), "got: {msg}");
        assert!(msg.contains("add a new migration"), "got: {msg}");

        // The untouched pair still applies cleanly afterwards.
        run_migrations(&mut c, &[M1, M2]).expect("unedited list still fine");
    }

    #[test]
    fn checksums_are_recorded_on_apply() {
        let mut c = conn();
        run_migrations(&mut c, &[M1, M2]).expect("apply");
        let recorded: Vec<String> = {
            let mut stmt = c
                .prepare("SELECT checksum FROM schema_migrations ORDER BY version")
                .expect("prepare");
            let rows = stmt.query_map([], |r| r.get(0)).expect("query");
            rows.collect::<rusqlite::Result<_>>().expect("collect")
        };
        assert_eq!(recorded, vec![checksum_of(M1.sql), checksum_of(M2.sql)]);
    }

    #[test]
    fn pre_checksum_databases_are_grandfathered_and_backfilled() {
        // A database written before the checksum column existed: the column is
        // added, the NULL row is accepted, and the hash is filled in so the
        // next run has something to check against.
        let mut c = conn();
        c.execute_batch(
            "CREATE TABLE schema_migrations (
                 version INTEGER PRIMARY KEY,
                 name TEXT NOT NULL,
                 applied_at TEXT NOT NULL DEFAULT (datetime('now'))
             );
             INSERT INTO schema_migrations (version, name) VALUES (1, '001_users');
             CREATE TABLE users (id TEXT PRIMARY KEY);
             CREATE INDEX idx_users ON users (id);",
        )
        .expect("pre-checksum state");

        run_migrations(&mut c, &[M1, M2]).expect("grandfathered, not refused");
        assert_eq!(applied_versions(&c), vec![1, 2]);

        let backfilled: String = c
            .query_row(
                "SELECT checksum FROM schema_migrations WHERE version = 1",
                [],
                |r| r.get(0),
            )
            .expect("checksum backfilled");
        assert_eq!(backfilled, checksum_of(M1.sql));
    }

    #[test]
    fn non_dense_versions_are_rejected() {
        let mut c = conn();
        let gap = Migration { version: 3, ..M2 };
        let err = run_migrations(&mut c, &[M1, gap]).expect_err("gap");
        assert!(matches!(err, MigrateError::Invalid(_)));
    }
}
