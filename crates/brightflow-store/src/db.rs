//! SQLite-backed metadata store (Litehouse)

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

use crate::error::StoreResult;
use crate::models::{ColumnStatRow, TableFileRow, TableRow};

#[derive(Clone)]
pub struct StoreDb {
    pool: SqlitePool,
}

impl StoreDb {
    pub async fn new(database_url: &str) -> StoreResult<Self> {
        let options = SqliteConnectOptions::from_str(database_url)?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Self { pool })
    }

    // =====================================================
    // Tables CRUD
    // =====================================================

    pub async fn create_table(&self, name: &str) -> StoreResult<TableRow> {
        let id = uuid::Uuid::now_v7().to_string();
        let row = sqlx::query_as::<_, TableRow>(
            r"INSERT INTO tables (id, name)
              VALUES (?, ?)
              RETURNING *",
        )
        .bind(&id)
        .bind(name)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn get_table_by_name(&self, name: &str) -> StoreResult<Option<TableRow>> {
        let row = sqlx::query_as::<_, TableRow>("SELECT * FROM tables WHERE name = ?")
            .bind(name)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row)
    }

    pub async fn list_tables(&self) -> StoreResult<Vec<TableRow>> {
        let rows = sqlx::query_as::<_, TableRow>("SELECT * FROM tables ORDER BY name")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows)
    }

    pub async fn update_table_meta(
        &self,
        id: &str,
        schema_json: Option<&str>,
        primary_keys: Option<&str>,
        total_rows: i64,
    ) -> StoreResult<Option<TableRow>> {
        let row = sqlx::query_as::<_, TableRow>(
            r"UPDATE tables
              SET schema_json = COALESCE(?, schema_json),
                  primary_keys = COALESCE(?, primary_keys),
                  total_rows = ?,
                  version = version + 1,
                  updated_at = datetime('now')
              WHERE id = ?
              RETURNING *",
        )
        .bind(schema_json)
        .bind(primary_keys)
        .bind(total_rows)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn delete_table(&self, name: &str) -> StoreResult<bool> {
        let result = sqlx::query("DELETE FROM tables WHERE name = ?")
            .bind(name)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    // =====================================================
    // Table Files CRUD
    // =====================================================

    pub async fn add_table_file(
        &self,
        table_id: &str,
        path: &str,
        num_rows: i64,
        size_bytes: i64,
    ) -> StoreResult<TableFileRow> {
        let id = uuid::Uuid::now_v7().to_string();
        let row = sqlx::query_as::<_, TableFileRow>(
            r"INSERT INTO table_files (id, table_id, path, num_rows, size_bytes)
              VALUES (?, ?, ?, ?, ?)
              RETURNING *",
        )
        .bind(&id)
        .bind(table_id)
        .bind(path)
        .bind(num_rows)
        .bind(size_bytes)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn list_table_files(&self, table_id: &str) -> StoreResult<Vec<TableFileRow>> {
        let rows = sqlx::query_as::<_, TableFileRow>(
            "SELECT * FROM table_files WHERE table_id = ? ORDER BY added_at",
        )
        .bind(table_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Replace all files for a table in a single transaction.
    /// Used by merge operations that rewrite to a single consolidated file.
    pub async fn replace_table_files(
        &self,
        table_id: &str,
        new_files: &[(String, i64, i64)], // (path, num_rows, size_bytes)
    ) -> StoreResult<Vec<TableFileRow>> {
        let mut tx = self.pool.begin().await?;

        sqlx::query("DELETE FROM table_files WHERE table_id = ?")
            .bind(table_id)
            .execute(&mut *tx)
            .await?;

        for (path, num_rows, size_bytes) in new_files {
            let id = uuid::Uuid::now_v7().to_string();
            sqlx::query(
                r"INSERT INTO table_files (id, table_id, path, num_rows, size_bytes)
                  VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(table_id)
            .bind(path)
            .bind(num_rows)
            .bind(size_bytes)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        self.list_table_files(table_id).await
    }

    pub async fn delete_table_files(&self, table_id: &str) -> StoreResult<u64> {
        let result = sqlx::query("DELETE FROM table_files WHERE table_id = ?")
            .bind(table_id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    // =====================================================
    // Column Stats
    // =====================================================

    pub async fn upsert_column_stats(
        &self,
        table_id: &str,
        stats: &[ColumnStatRow],
    ) -> StoreResult<()> {
        let mut tx = self.pool.begin().await?;

        // Clear existing stats for this table before inserting new ones
        sqlx::query("DELETE FROM table_column_stats WHERE table_id = ?")
            .bind(table_id)
            .execute(&mut *tx)
            .await?;

        for stat in stats {
            sqlx::query(
                r"INSERT INTO table_column_stats (table_id, column_name, min_value, max_value, null_count)
                  VALUES (?, ?, ?, ?, ?)",
            )
            .bind(table_id)
            .bind(&stat.column_name)
            .bind(&stat.min_value)
            .bind(&stat.max_value)
            .bind(stat.null_count)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    pub async fn get_column_stats(&self, table_id: &str) -> StoreResult<Vec<ColumnStatRow>> {
        let rows = sqlx::query_as::<_, ColumnStatRow>(
            "SELECT * FROM table_column_stats WHERE table_id = ? ORDER BY column_name",
        )
        .bind(table_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}
