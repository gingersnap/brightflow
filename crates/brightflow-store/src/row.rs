//! Hand-written row mapping: the `FromRow` trait plus query helpers that keep
//! call sites one-liners inside `SqlitePool::call` closures.
//!
//! rusqlite has no derive for struct mapping, so each model implements
//! `FromRow` explicitly with named-column access — many queries are
//! `SELECT *`, and naming the columns keeps them order-independent. The
//! helpers take `&Connection` so the same code works on a plain connection
//! and inside a `Transaction` (which derefs to one).

use rusqlite::types::FromSql;
use rusqlite::{Connection, OptionalExtension, Params, Row};

/// A type constructible from one result row.
pub trait FromRow: Sized {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self>;
}

// Positional tuple impls for scalar and pair projections (`SELECT count(*)`,
// `SELECT a, b` …), mirroring the query_as tuple sites the sqlx code had.
impl<A: FromSql> FromRow for (A,) {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok((row.get(0)?,))
    }
}

impl<A: FromSql, B: FromSql> FromRow for (A, B) {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok((row.get(0)?, row.get(1)?))
    }
}

impl<A: FromSql, B: FromSql, C: FromSql> FromRow for (A, B, C) {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    }
}

/// Implement `FromRow` for a struct whose field names mirror its column names
/// 1:1 (the models-file convention).
///
/// Fields are listed once and read by name, so column order never matters and
/// a rename fails loudly at query time.
#[macro_export]
macro_rules! impl_from_row {
    ($ty:ty { $($field:ident),+ $(,)? }) => {
        impl $crate::FromRow for $ty {
            fn from_row(row: &$crate::rusqlite::Row<'_>) -> $crate::rusqlite::Result<Self> {
                Ok(Self { $($field: row.get(stringify!($field))?),+ })
            }
        }
    };
}

/// All matching rows, mapped through `FromRow`.
pub fn fetch_all<T: FromRow, P: Params>(
    conn: &Connection,
    sql: &str,
    params: P,
) -> rusqlite::Result<Vec<T>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params, |row| T::from_row(row))?;
    rows.collect()
}

/// Exactly one row; `QueryReturnedNoRows` if there is none. Also the shape
/// for `INSERT/UPDATE … RETURNING *`, which must run as a query — rusqlite's
/// `execute` rejects statements that return rows.
pub fn fetch_one<T: FromRow, P: Params>(
    conn: &Connection,
    sql: &str,
    params: P,
) -> rusqlite::Result<T> {
    conn.query_row(sql, params, |row| T::from_row(row))
}

/// Zero or one row.
pub fn fetch_optional<T: FromRow, P: Params>(
    conn: &Connection,
    sql: &str,
    params: P,
) -> rusqlite::Result<Option<T>> {
    conn.query_row(sql, params, |row| T::from_row(row))
        .optional()
}

/// `Connection::execute` returning the affected-row count as `u64`, so the
/// former `rows_affected()` call sites need no per-site cast.
pub fn execute<P: Params>(conn: &Connection, sql: &str, params: P) -> rusqlite::Result<u64> {
    let n = conn.execute(sql, params)?;
    // usize→u64 is lossless on every supported target.
    Ok(u64::try_from(n).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Pair {
        id: String,
        n: i64,
    }

    impl FromRow for Pair {
        fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
            Ok(Self {
                id: row.get("id")?,
                n: row.get("n")?,
            })
        }
    }

    fn seeded() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory db");
        conn.execute_batch(
            "CREATE TABLE t (id TEXT PRIMARY KEY, n INTEGER NOT NULL);
             INSERT INTO t (id, n) VALUES ('a', 1), ('b', 2);",
        )
        .expect("seed");
        conn
    }

    #[test]
    fn fetch_all_maps_structs_by_column_name() {
        let conn = seeded();
        let rows: Vec<Pair> =
            fetch_all(&conn, "SELECT n, id FROM t ORDER BY id", []).expect("rows");
        // Columns selected in swapped order still map correctly by name.
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "a");
        assert_eq!(rows[0].n, 1);
    }

    #[test]
    fn fetch_one_returns_row_or_no_rows_error() {
        let conn = seeded();
        let row: Pair = fetch_one(&conn, "SELECT * FROM t WHERE id = ?1", ["a"]).expect("row");
        assert_eq!(row.n, 1);
        let missing = fetch_one::<Pair, _>(&conn, "SELECT * FROM t WHERE id = ?1", ["z"]);
        assert!(matches!(missing, Err(rusqlite::Error::QueryReturnedNoRows)));
    }

    #[test]
    fn fetch_optional_and_tuples() {
        let conn = seeded();
        let none: Option<Pair> =
            fetch_optional(&conn, "SELECT * FROM t WHERE id = ?1", ["z"]).expect("query");
        assert!(none.is_none());
        let (count,): (i64,) = fetch_one(&conn, "SELECT count(*) FROM t", []).expect("count");
        assert_eq!(count, 2);
        let pairs: Vec<(String, i64)> =
            fetch_all(&conn, "SELECT id, n FROM t ORDER BY id", []).expect("tuples");
        assert_eq!(pairs[1], ("b".to_owned(), 2));
    }

    #[test]
    fn execute_returns_affected_rows() {
        let conn = seeded();
        let n = execute(&conn, "UPDATE t SET n = n + 1", []).expect("update");
        assert_eq!(n, 2);
        let n = execute(&conn, "DELETE FROM t WHERE id = ?1", ["a"]).expect("delete");
        assert_eq!(n, 1);
    }
}
