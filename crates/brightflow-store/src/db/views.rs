//! Saved views: named Explore configurations over a table, one row each.
//!
//! The spec is the client's JSON and the store never reads inside it. Writes
//! take the whole row so an undo can put back exactly what was there; the
//! action bus decides who may write.

use rusqlite::params;

use super::StoreDb;
use crate::error::StoreResult;
use crate::models::{SavedViewListRow, SavedViewRow};
use crate::row::{execute, fetch_all, fetch_optional};

impl StoreDb {
    /// Insert a view as given, or replace the row with the same id (an
    /// undo putting a deleted view back, a rename or re-save restoring
    /// the previous row).
    pub async fn upsert_saved_view(&self, row: &SavedViewRow) -> StoreResult<()> {
        let row = row.clone();
        self.pool
            .call(move |conn| {
                execute(
                    conn,
                    r"INSERT INTO saved_views
                        (id, table_id, name, kind, spec_json, created_by, created_at, updated_at)
                      VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                      ON CONFLICT(id) DO UPDATE SET
                        name = excluded.name, kind = excluded.kind,
                        spec_json = excluded.spec_json, created_by = excluded.created_by,
                        created_at = excluded.created_at, updated_at = excluded.updated_at",
                    params![
                        row.id,
                        row.table_id,
                        row.name,
                        row.kind,
                        row.spec_json,
                        row.created_by,
                        row.created_at,
                        row.updated_at,
                    ],
                )
            })
            .await?;
        Ok(())
    }

    pub async fn get_saved_view(&self, id: &str) -> StoreResult<Option<SavedViewRow>> {
        let id = id.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<SavedViewRow, _>(
                    conn,
                    "SELECT * FROM saved_views WHERE id = ?",
                    params![id],
                )
            })
            .await?;
        Ok(row)
    }

    /// A table's view by name, for the "does this name exist" check.
    pub async fn get_saved_view_by_name(
        &self,
        table_id: &str,
        name: &str,
    ) -> StoreResult<Option<SavedViewRow>> {
        let table_id = table_id.to_owned();
        let name = name.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<SavedViewRow, _>(
                    conn,
                    "SELECT * FROM saved_views WHERE table_id = ? AND name = ?",
                    params![table_id, name],
                )
            })
            .await?;
        Ok(row)
    }

    /// Every view of every table of a source, by table then name.
    pub async fn list_saved_views_for_source(
        &self,
        source_id: &str,
    ) -> StoreResult<Vec<SavedViewListRow>> {
        let source_id = source_id.to_owned();
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<SavedViewListRow, _>(
                    conn,
                    r"SELECT v.id, v.table_id, t.source_id, t.name AS table_name,
                             v.name, v.kind, v.spec_json, v.created_by,
                             v.created_at, v.updated_at
                      FROM saved_views v JOIN tables t ON t.id = v.table_id
                      WHERE t.source_id = ?
                      ORDER BY t.name, v.name",
                    params![source_id],
                )
            })
            .await?;
        Ok(rows)
    }

    /// Delete a view; `Ok(false)` when there was none.
    pub async fn delete_saved_view(&self, id: &str) -> StoreResult<bool> {
        let id = id.to_owned();
        let n = self
            .pool
            .call(move |conn| execute(conn, "DELETE FROM saved_views WHERE id = ?", params![id]))
            .await?;
        Ok(n > 0)
    }
}
