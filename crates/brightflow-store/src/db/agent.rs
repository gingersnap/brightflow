//! Agent run records: one row per autonomous curation run, with status
//! and summary for the activity feed.

use rusqlite::params;

use super::StoreDb;
use crate::error::StoreResult;
use crate::models::AgentRunRow;
use crate::row::{execute, fetch_all, fetch_one, fetch_optional};

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
        let kind = kind.to_owned();
        let mode = mode.to_owned();
        let scope = scope.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_one::<AgentRunRow, _>(
                    conn,
                    r"INSERT INTO agent_runs (kind, mode, scope, status, created_at)
                      VALUES (?, ?, ?, 'running', ?)
                      RETURNING *",
                    params![kind, mode, scope, now_epoch],
                )
            })
            .await?;
        Ok(row)
    }

    pub async fn get_agent_run(&self, id: i64) -> StoreResult<Option<AgentRunRow>> {
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<AgentRunRow, _>(
                    conn,
                    "SELECT * FROM agent_runs WHERE id = ?",
                    params![id],
                )
            })
            .await?;
        Ok(row)
    }

    pub async fn list_agent_runs(&self, limit: i64) -> StoreResult<Vec<AgentRunRow>> {
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<AgentRunRow, _>(
                    conn,
                    "SELECT * FROM agent_runs ORDER BY created_at DESC LIMIT ?",
                    params![limit],
                )
            })
            .await?;
        Ok(rows)
    }

    pub async fn active_agent_run_for_scope(
        &self,
        scope: &str,
    ) -> StoreResult<Option<AgentRunRow>> {
        let scope = scope.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<AgentRunRow, _>(
                    conn,
                    "SELECT * FROM agent_runs WHERE scope = ? AND status = 'running' LIMIT 1",
                    params![scope],
                )
            })
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
        let status = status.to_owned();
        let detail = detail.map(str::to_owned);
        self.pool
            .call(move |conn| {
                execute(
                    conn,
                    "UPDATE agent_runs SET status = ?, detail = ?, finished_at = ? WHERE id = ?",
                    params![status, detail, now_epoch, id],
                )
                .map(|_| ())
            })
            .await?;
        Ok(())
    }

    /// Boot cleanup: any run still 'running' from a previous process crashed.
    pub async fn fail_stuck_agent_runs(&self, now_epoch: i64) -> StoreResult<u64> {
        let affected = self
            .pool
            .call(move |conn| {
                execute(
                    conn,
                    r"UPDATE agent_runs SET status = 'failed', detail = 'process restarted mid-run',
                      finished_at = ? WHERE status = 'running'",
                    params![now_epoch],
                )
            })
            .await?;
        Ok(affected)
    }

    // =====================================================
}
