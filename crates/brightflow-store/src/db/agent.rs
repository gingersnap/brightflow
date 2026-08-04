//! Agent run records: one row per autonomous curation run, with status
//! and summary for the activity feed.

use super::StoreDb;
use crate::error::StoreResult;
use crate::models::AgentRunRow;

impl StoreDb {
    // Agent runs
    // =====================================================

    pub async fn insert_agent_run(
        &self,
        kind: &str,
        mode: &str,
        scope: &str,
        now_epoch: i64,
    ) -> StoreResult<AgentRunRow> {
        let row = sqlx::query_as::<_, AgentRunRow>(
            r"INSERT INTO agent_runs (kind, mode, scope, status, created_at)
              VALUES (?, ?, ?, 'running', ?)
              RETURNING *",
        )
        .bind(kind)
        .bind(mode)
        .bind(scope)
        .bind(now_epoch)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn get_agent_run(&self, id: i64) -> StoreResult<Option<AgentRunRow>> {
        let row = sqlx::query_as::<_, AgentRunRow>("SELECT * FROM agent_runs WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row)
    }

    pub async fn list_agent_runs(&self, limit: i64) -> StoreResult<Vec<AgentRunRow>> {
        let rows = sqlx::query_as::<_, AgentRunRow>(
            "SELECT * FROM agent_runs ORDER BY created_at DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn active_agent_run_for_scope(
        &self,
        scope: &str,
    ) -> StoreResult<Option<AgentRunRow>> {
        let row = sqlx::query_as::<_, AgentRunRow>(
            "SELECT * FROM agent_runs WHERE scope = ? AND status = 'running' LIMIT 1",
        )
        .bind(scope)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn finish_agent_run(
        &self,
        id: i64,
        status: &str,
        detail: Option<&str>,
        now_epoch: i64,
    ) -> StoreResult<()> {
        sqlx::query("UPDATE agent_runs SET status = ?, detail = ?, finished_at = ? WHERE id = ?")
            .bind(status)
            .bind(detail)
            .bind(now_epoch)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Boot cleanup: any run still 'running' from a previous process crashed.
    pub async fn fail_stuck_agent_runs(&self, now_epoch: i64) -> StoreResult<u64> {
        let result = sqlx::query(
            r"UPDATE agent_runs SET status = 'failed', detail = 'process restarted mid-run',
              finished_at = ? WHERE status = 'running'",
        )
        .bind(now_epoch)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    // =====================================================
}
