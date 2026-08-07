//! The action log: every curation action with its undo payload and
//! lifecycle status — the audit trail the activity feed renders.

use rusqlite::params;

use super::StoreDb;
use crate::error::StoreResult;
use crate::models::ActionLogRow;
use crate::row::{execute, fetch_all, fetch_one, fetch_optional};

impl StoreDb {
    // Action log
    // =====================================================

    /// Insert a new action-log entry. Returns None when the request_id was
    /// already recorded (idempotent replay — fetch the existing row instead).
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_action(
        &self,
        request_id: &str,
        actor_type: &str,
        agent_run_id: Option<i64>,
        action_kind: &str,
        params_json: &str,
        status: &str,
        now_epoch: i64,
    ) -> StoreResult<Option<ActionLogRow>> {
        let request_id = request_id.to_owned();
        let actor_type = actor_type.to_owned();
        let action_kind = action_kind.to_owned();
        let params_json = params_json.to_owned();
        let status = status.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<ActionLogRow, _>(
                    conn,
                    r"INSERT INTO action_log
                        (request_id, actor_type, agent_run_id, action_kind, params_json, status, created_at)
                      VALUES (?, ?, ?, ?, ?, ?, ?)
                      ON CONFLICT (request_id) DO NOTHING
                      RETURNING *",
                    params![
                        request_id,
                        actor_type,
                        agent_run_id,
                        action_kind,
                        params_json,
                        status,
                        now_epoch
                    ],
                )
            })
            .await?;
        Ok(row)
    }

    pub async fn get_action_by_request_id(
        &self,
        request_id: &str,
    ) -> StoreResult<Option<ActionLogRow>> {
        let request_id = request_id.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<ActionLogRow, _>(
                    conn,
                    "SELECT * FROM action_log WHERE request_id = ?",
                    params![request_id],
                )
            })
            .await?;
        Ok(row)
    }

    pub async fn get_action(&self, id: i64) -> StoreResult<Option<ActionLogRow>> {
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<ActionLogRow, _>(
                    conn,
                    "SELECT * FROM action_log WHERE id = ?",
                    params![id],
                )
            })
            .await?;
        Ok(row)
    }

    pub async fn list_actions(&self, limit: i64) -> StoreResult<Vec<ActionLogRow>> {
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<ActionLogRow, _>(
                    conn,
                    "SELECT * FROM action_log ORDER BY created_at DESC, id DESC LIMIT ?",
                    params![limit],
                )
            })
            .await?;
        Ok(rows)
    }

    /// Every action awaiting approval, **oldest first**.
    ///
    /// Ascending id is load-bearing, not cosmetic: proposals have ordering
    /// dependencies. `propose_taxonomy` defines a category and `label_documents`
    /// then references it by name, so approving the label before the category
    /// exists fails with "not a category in this table's taxonomy". Creation
    /// order is the order they were meant to apply in.
    pub async fn list_proposed_actions(&self) -> StoreResult<Vec<ActionLogRow>> {
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<ActionLogRow, _>(
                    conn,
                    "SELECT * FROM action_log WHERE status = 'proposed' ORDER BY id ASC",
                    params![],
                )
            })
            .await?;
        Ok(rows)
    }

    /// Count of actions awaiting approval. Cheap enough to poll for a badge.
    pub async fn count_proposed_actions(&self) -> StoreResult<i64> {
        let (n,) = self
            .pool
            .call(move |conn| {
                fetch_one::<(i64,), _>(
                    conn,
                    "SELECT COUNT(*) FROM action_log WHERE status = 'proposed'",
                    params![],
                )
            })
            .await?;
        Ok(n)
    }

    pub async fn list_actions_for_agent_run(&self, run_id: i64) -> StoreResult<Vec<ActionLogRow>> {
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<ActionLogRow, _>(
                    conn,
                    "SELECT * FROM action_log WHERE agent_run_id = ? ORDER BY id",
                    params![run_id],
                )
            })
            .await?;
        Ok(rows)
    }

    pub async fn update_action_result(
        &self,
        id: i64,
        status: &str,
        result_json: Option<&str>,
        undo_json: Option<&str>,
        resolved_at: i64,
    ) -> StoreResult<()> {
        let status = status.to_owned();
        let result_json = result_json.map(str::to_owned);
        let undo_json = undo_json.map(str::to_owned);
        self.pool
            .call(move |conn| {
                execute(
                    conn,
                    r"UPDATE action_log
                      SET status = ?, result_json = ?, undo_json = ?, resolved_at = ?
                      WHERE id = ?",
                    params![status, result_json, undo_json, resolved_at, id],
                )
                .map(|_| ())
            })
            .await?;
        Ok(())
    }

    pub async fn set_action_status(
        &self,
        id: i64,
        status: &str,
        resolved_at: i64,
    ) -> StoreResult<()> {
        let status = status.to_owned();
        self.pool
            .call(move |conn| {
                execute(
                    conn,
                    "UPDATE action_log SET status = ?, resolved_at = ? WHERE id = ?",
                    params![status, resolved_at, id],
                )
                .map(|_| ())
            })
            .await?;
        Ok(())
    }

    // =====================================================
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Fresh, fully migrated `StoreDb` backed by a SQLite file in `tmp`.
    async fn temp_db(tmp: &TempDir) -> StoreDb {
        let db_url = format!(
            "sqlite:{}?mode=rwc",
            tmp.path().join("litehouse.db").display()
        );
        StoreDb::new(&db_url).await.expect("failed to open db")
    }

    /// Bulk approve applies proposals **oldest first**, and this is the accessor
    /// that guarantees it.
    ///
    /// The ordering is a correctness requirement, not a preference: an agent
    /// proposes `define_taxonomy_category` before the `label_document` that
    /// names it, so applying newest-first would fail every label with "not a
    /// category in this table's taxonomy". The neighbouring `list_actions`
    /// (the audit feed) is deliberately DESC, so this is an easy thing to get
    /// backwards by copy-paste.
    #[tokio::test]
    async fn list_proposed_actions_is_oldest_first_and_only_proposed() {
        let tmp = TempDir::new().expect("temp dir");
        let db = temp_db(&tmp).await;

        // Insert in creation order: two proposals, one already applied, one more.
        for (request_id, kind, status) in [
            ("r1", "define_taxonomy_category", "proposed"),
            ("r2", "label_document", "proposed"),
            ("r3", "rename_cluster", "applied"),
            ("r4", "label_document", "proposed"),
        ] {
            let row = db
                .insert_action(request_id, "agent", Some(1), kind, "{}", status, 0)
                .await
                .expect("insert")
                .expect("no request_id conflict");
            if status == "applied" {
                db.update_action_result(row.id, "applied", None, None, 0)
                    .await
                    .expect("mark applied");
            }
        }

        let proposed = db.list_proposed_actions().await.expect("list");
        assert_eq!(proposed.len(), 3, "the applied action must not be returned");

        let ids: Vec<i64> = proposed.iter().map(|r| r.id).collect();
        let mut ascending = ids.clone();
        ascending.sort_unstable();
        assert_eq!(
            ids, ascending,
            "must be oldest-first, or dependent proposals fail"
        );
        assert_eq!(
            proposed.first().map(|r| r.action_kind.as_str()),
            Some("define_taxonomy_category"),
            "the category must be applied before the label that names it"
        );

        assert_eq!(db.count_proposed_actions().await.expect("count"), 3);
    }

    #[tokio::test]
    async fn count_proposed_actions_is_zero_on_a_fresh_store() {
        let tmp = TempDir::new().expect("temp dir");
        let db = temp_db(&tmp).await;
        assert_eq!(db.count_proposed_actions().await.expect("count"), 0);
        assert!(db.list_proposed_actions().await.expect("list").is_empty());
    }
}
