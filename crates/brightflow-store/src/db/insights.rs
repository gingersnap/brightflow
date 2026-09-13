//! Insights persistence: history rows feeding novelty decay, run records
//! behind the badge and run list, and per-insight state (dismiss / pin)
//! plus suppressions.

use rusqlite::params;

use super::StoreDb;
use crate::error::StoreResult;
use crate::models::{InsightHistoryRow, InsightRunRow, InsightStateRow, InsightSuppressionRow};
use crate::row::{execute, fetch_all, fetch_one, fetch_optional};

impl StoreDb {
    // Insight History CRUD (novelty decay input)
    // =====================================================

    pub async fn get_insight_history(&self, table_id: &str) -> StoreResult<Vec<InsightHistoryRow>> {
        let table_id = table_id.to_owned();
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<InsightHistoryRow, _>(
                    conn,
                    "SELECT * FROM insight_history WHERE table_id = ?",
                    params![table_id],
                )
            })
            .await?;
        Ok(rows)
    }

    /// Record that a batch of insights was shown: bumps shown_count and
    /// last_shown_at; a changed value signature resets the count (the story
    /// developed, so it is novel again).
    pub async fn record_shown_insights(
        &self,
        table_id: &str,
        shown: &[(String, String, String, String)], // (fingerprint, identity, insight_type, value_sig)
        now_epoch: i64,
    ) -> StoreResult<()> {
        let table_id = table_id.to_owned();
        let shown = shown.to_vec();
        self.pool
            .transaction(move |tx| {
                // One prepared statement, executed per shown insight.
                let mut stmt = tx.prepare(
                    r"INSERT INTO insight_history
                        (table_id, fingerprint, identity, insight_type, last_value_sig,
                         shown_count, first_shown_at, last_shown_at)
                      VALUES (?, ?, ?, ?, ?, 1, ?, ?)
                      ON CONFLICT (table_id, fingerprint) DO UPDATE SET
                        identity = excluded.identity,
                        insight_type = excluded.insight_type,
                        shown_count = CASE
                            WHEN insight_history.last_value_sig != excluded.last_value_sig THEN 1
                            ELSE insight_history.shown_count + 1
                        END,
                        last_value_sig = excluded.last_value_sig,
                        last_shown_at = excluded.last_shown_at",
                )?;
                for (fingerprint, identity, insight_type, value_sig) in &shown {
                    stmt.execute(params![
                        table_id,
                        fingerprint,
                        identity,
                        insight_type,
                        value_sig,
                        now_epoch,
                        now_epoch
                    ])?;
                }
                Ok(())
            })
            .await?;
        Ok(())
    }

    // =====================================================
    // Insight Runs (badge + run history)
    // =====================================================

    /// Cap on retained runs per table (pruned on insert).
    const INSIGHT_RUNS_KEEP: i64 = 100;

    #[allow(clippy::too_many_arguments)]
    pub async fn insert_insight_run(
        &self,
        table_id: &str,
        source_id: &str,
        table_name: &str,
        report_type: &str,
        triggered_by: &str,
        finding_count: i64,
        new_finding_count: i64,
        top_summary: Option<&str>,
        execution_time_ms: f64,
        computed_at: i64,
    ) -> StoreResult<InsightRunRow> {
        let table_id = table_id.to_owned();
        let source_id = source_id.to_owned();
        let table_name = table_name.to_owned();
        let report_type = report_type.to_owned();
        let triggered_by = triggered_by.to_owned();
        let top_summary = top_summary.map(str::to_owned);
        let row = self
            .pool
            .call(move |conn| {
                let row = fetch_one::<InsightRunRow, _>(
                    conn,
                    r"INSERT INTO insight_runs
                        (table_id, source_id, table_name, report_type, triggered_by,
                         finding_count, new_finding_count, top_summary, execution_time_ms, computed_at)
                      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                      RETURNING *",
                    params![
                        table_id,
                        source_id,
                        table_name,
                        report_type,
                        triggered_by,
                        finding_count,
                        new_finding_count,
                        top_summary,
                        execution_time_ms,
                        computed_at
                    ],
                )?;
                execute(
                    conn,
                    r"DELETE FROM insight_runs WHERE table_id = ? AND id NOT IN (
                        SELECT id FROM insight_runs WHERE table_id = ?
                        ORDER BY computed_at DESC, id DESC LIMIT ?)",
                    params![table_id, table_id, Self::INSIGHT_RUNS_KEEP],
                )?;
                Ok(row)
            })
            .await?;
        Ok(row)
    }

    pub async fn list_insight_runs(
        &self,
        table_id: &str,
        limit: i64,
    ) -> StoreResult<Vec<InsightRunRow>> {
        let table_id = table_id.to_owned();
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<InsightRunRow, _>(
                    conn,
                    r"SELECT * FROM insight_runs WHERE table_id = ?
                      ORDER BY computed_at DESC, id DESC LIMIT ?",
                    params![table_id, limit],
                )
            })
            .await?;
        Ok(rows)
    }

    /// Latest run for a table, optionally restricted to one trigger kind.
    pub async fn latest_insight_run(
        &self,
        table_id: &str,
        triggered_by: Option<&str>,
    ) -> StoreResult<Option<InsightRunRow>> {
        let table_id = table_id.to_owned();
        let row = match triggered_by {
            Some(t) => {
                let t = t.to_owned();
                self.pool
                    .call(move |conn| {
                        fetch_optional::<InsightRunRow, _>(
                            conn,
                            r"SELECT * FROM insight_runs WHERE table_id = ? AND triggered_by = ?
                              ORDER BY computed_at DESC, id DESC LIMIT 1",
                            params![table_id, t],
                        )
                    })
                    .await?
            },
            None => {
                self.pool
                    .call(move |conn| {
                        fetch_optional::<InsightRunRow, _>(
                            conn,
                            r"SELECT * FROM insight_runs WHERE table_id = ?
                              ORDER BY computed_at DESC, id DESC LIMIT 1",
                            params![table_id],
                        )
                    })
                    .await?
            },
        };
        Ok(row)
    }

    /// The most recent runs across every table, newest first.
    pub async fn list_recent_insight_runs(&self, limit: i64) -> StoreResult<Vec<InsightRunRow>> {
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<InsightRunRow, _>(
                    conn,
                    "SELECT * FROM insight_runs ORDER BY computed_at DESC, id DESC LIMIT ?",
                    params![limit],
                )
            })
            .await?;
        Ok(rows)
    }

    /// Latest run per table of a source (badge hydration).
    pub async fn latest_insight_runs_for_source(
        &self,
        source_id: &str,
    ) -> StoreResult<Vec<InsightRunRow>> {
        let source_id = source_id.to_owned();
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<InsightRunRow, _>(
                    conn,
                    r"SELECT r.* FROM insight_runs r
                      JOIN (SELECT table_id, MAX(computed_at) AS latest, MAX(id) AS latest_id
                            FROM insight_runs WHERE source_id = ? GROUP BY table_id) m
                        ON r.table_id = m.table_id AND r.id = m.latest_id
                      ORDER BY r.computed_at DESC",
                    params![source_id],
                )
            })
            .await?;
        Ok(rows)
    }

    pub async fn reset_insight_history(&self, table_id: &str) -> StoreResult<u64> {
        let table_id = table_id.to_owned();
        let affected = self
            .pool
            .call(move |conn| {
                execute(
                    conn,
                    "DELETE FROM insight_history WHERE table_id = ?",
                    params![table_id],
                )
            })
            .await?;
        Ok(affected)
    }

    // =====================================================
    // Insight State (dismiss / pin) + Suppressions
    // =====================================================

    pub async fn get_insight_states(&self, table_id: &str) -> StoreResult<Vec<InsightStateRow>> {
        let table_id = table_id.to_owned();
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<InsightStateRow, _>(
                    conn,
                    "SELECT * FROM insight_state WHERE table_id = ?",
                    params![table_id],
                )
            })
            .await?;
        Ok(rows)
    }

    pub async fn upsert_insight_state(
        &self,
        table_id: &str,
        fingerprint: &str,
        state: &str,
        reason: Option<&str>,
        annotation: Option<&str>,
        now_epoch: i64,
    ) -> StoreResult<InsightStateRow> {
        let table_id = table_id.to_owned();
        let fingerprint = fingerprint.to_owned();
        let state = state.to_owned();
        let reason = reason.map(str::to_owned);
        let annotation = annotation.map(str::to_owned);
        let row = self
            .pool
            .call(move |conn| {
                fetch_one::<InsightStateRow, _>(
                    conn,
                    r"INSERT INTO insight_state (table_id, fingerprint, state, reason, annotation, created_at)
                      VALUES (?, ?, ?, ?, ?, ?)
                      ON CONFLICT (table_id, fingerprint) DO UPDATE SET
                        state = excluded.state,
                        reason = excluded.reason,
                        annotation = excluded.annotation,
                        created_at = excluded.created_at
                      RETURNING *",
                    params![table_id, fingerprint, state, reason, annotation, now_epoch],
                )
            })
            .await?;
        Ok(row)
    }

    pub async fn delete_insight_state(
        &self,
        table_id: &str,
        fingerprint: &str,
    ) -> StoreResult<bool> {
        let table_id = table_id.to_owned();
        let fingerprint = fingerprint.to_owned();
        let affected = self
            .pool
            .call(move |conn| {
                execute(
                    conn,
                    "DELETE FROM insight_state WHERE table_id = ? AND fingerprint = ?",
                    params![table_id, fingerprint],
                )
            })
            .await?;
        Ok(affected > 0)
    }

    pub async fn get_insight_suppressions(
        &self,
        table_id: &str,
    ) -> StoreResult<Vec<InsightSuppressionRow>> {
        let table_id = table_id.to_owned();
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<InsightSuppressionRow, _>(
                    conn,
                    "SELECT * FROM insight_suppressions WHERE table_id = ?",
                    params![table_id],
                )
            })
            .await?;
        Ok(rows)
    }

    pub async fn add_insight_suppression(
        &self,
        table_id: &str,
        kind: &str,
        target: &str,
        now_epoch: i64,
    ) -> StoreResult<InsightSuppressionRow> {
        let table_id = table_id.to_owned();
        let kind = kind.to_owned();
        let target = target.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_one::<InsightSuppressionRow, _>(
                    conn,
                    r"INSERT INTO insight_suppressions (table_id, kind, target, created_at)
                      VALUES (?, ?, ?, ?)
                      ON CONFLICT (table_id, kind, target) DO UPDATE SET created_at = excluded.created_at
                      RETURNING *",
                    params![table_id, kind, target, now_epoch],
                )
            })
            .await?;
        Ok(row)
    }

    pub async fn delete_insight_suppression(&self, id: i64) -> StoreResult<bool> {
        let affected = self
            .pool
            .call(move |conn| {
                execute(
                    conn,
                    "DELETE FROM insight_suppressions WHERE id = ?",
                    params![id],
                )
            })
            .await?;
        Ok(affected > 0)
    }

    // =====================================================
}
