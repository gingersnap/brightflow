//! Insights persistence: history rows feeding novelty decay, run records
//! behind the badge and run list, and per-insight state (dismiss / pin)
//! plus suppressions.

use super::StoreDb;
use crate::error::StoreResult;
use crate::models::{InsightHistoryRow, InsightRunRow, InsightStateRow, InsightSuppressionRow};

impl StoreDb {
    // Insight History CRUD (novelty decay input)
    // =====================================================

    pub async fn get_insight_history(&self, table_id: &str) -> StoreResult<Vec<InsightHistoryRow>> {
        let rows = sqlx::query_as::<_, InsightHistoryRow>(
            "SELECT * FROM insight_history WHERE table_id = ?",
        )
        .bind(table_id)
        .fetch_all(&self.pool)
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
        let mut tx = self.pool.begin().await?;
        for (fingerprint, identity, insight_type, value_sig) in shown {
            sqlx::query(
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
            )
            .bind(table_id)
            .bind(fingerprint)
            .bind(identity)
            .bind(insight_type)
            .bind(value_sig)
            .bind(now_epoch)
            .bind(now_epoch)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
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
        let row = sqlx::query_as::<_, InsightRunRow>(
            r"INSERT INTO insight_runs
                (table_id, source_id, table_name, report_type, triggered_by,
                 finding_count, new_finding_count, top_summary, execution_time_ms, computed_at)
              VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
              RETURNING *",
        )
        .bind(table_id)
        .bind(source_id)
        .bind(table_name)
        .bind(report_type)
        .bind(triggered_by)
        .bind(finding_count)
        .bind(new_finding_count)
        .bind(top_summary)
        .bind(execution_time_ms)
        .bind(computed_at)
        .fetch_one(&self.pool)
        .await?;
        sqlx::query(
            r"DELETE FROM insight_runs WHERE table_id = ? AND id NOT IN (
                SELECT id FROM insight_runs WHERE table_id = ?
                ORDER BY computed_at DESC, id DESC LIMIT ?)",
        )
        .bind(table_id)
        .bind(table_id)
        .bind(Self::INSIGHT_RUNS_KEEP)
        .execute(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn list_insight_runs(
        &self,
        table_id: &str,
        limit: i64,
    ) -> StoreResult<Vec<InsightRunRow>> {
        let rows = sqlx::query_as::<_, InsightRunRow>(
            r"SELECT * FROM insight_runs WHERE table_id = ?
              ORDER BY computed_at DESC, id DESC LIMIT ?",
        )
        .bind(table_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Latest run for a table, optionally restricted to one trigger kind.
    pub async fn latest_insight_run(
        &self,
        table_id: &str,
        triggered_by: Option<&str>,
    ) -> StoreResult<Option<InsightRunRow>> {
        let row = match triggered_by {
            Some(t) => {
                sqlx::query_as::<_, InsightRunRow>(
                    r"SELECT * FROM insight_runs WHERE table_id = ? AND triggered_by = ?
                      ORDER BY computed_at DESC, id DESC LIMIT 1",
                )
                .bind(table_id)
                .bind(t)
                .fetch_optional(&self.pool)
                .await?
            },
            None => {
                sqlx::query_as::<_, InsightRunRow>(
                    r"SELECT * FROM insight_runs WHERE table_id = ?
                      ORDER BY computed_at DESC, id DESC LIMIT 1",
                )
                .bind(table_id)
                .fetch_optional(&self.pool)
                .await?
            },
        };
        Ok(row)
    }

    /// Latest run per table of a source (badge hydration).
    pub async fn latest_insight_runs_for_source(
        &self,
        source_id: &str,
    ) -> StoreResult<Vec<InsightRunRow>> {
        let rows = sqlx::query_as::<_, InsightRunRow>(
            r"SELECT r.* FROM insight_runs r
              JOIN (SELECT table_id, MAX(computed_at) AS latest, MAX(id) AS latest_id
                    FROM insight_runs WHERE source_id = ? GROUP BY table_id) m
                ON r.table_id = m.table_id AND r.id = m.latest_id
              ORDER BY r.computed_at DESC",
        )
        .bind(source_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn reset_insight_history(&self, table_id: &str) -> StoreResult<u64> {
        let result = sqlx::query("DELETE FROM insight_history WHERE table_id = ?")
            .bind(table_id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    // =====================================================
    // Insight State (dismiss / pin) + Suppressions
    // =====================================================

    pub async fn get_insight_states(&self, table_id: &str) -> StoreResult<Vec<InsightStateRow>> {
        let rows =
            sqlx::query_as::<_, InsightStateRow>("SELECT * FROM insight_state WHERE table_id = ?")
                .bind(table_id)
                .fetch_all(&self.pool)
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
        let row = sqlx::query_as::<_, InsightStateRow>(
            r"INSERT INTO insight_state (table_id, fingerprint, state, reason, annotation, created_at)
              VALUES (?, ?, ?, ?, ?, ?)
              ON CONFLICT (table_id, fingerprint) DO UPDATE SET
                state = excluded.state,
                reason = excluded.reason,
                annotation = excluded.annotation,
                created_at = excluded.created_at
              RETURNING *",
        )
        .bind(table_id)
        .bind(fingerprint)
        .bind(state)
        .bind(reason)
        .bind(annotation)
        .bind(now_epoch)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn delete_insight_state(
        &self,
        table_id: &str,
        fingerprint: &str,
    ) -> StoreResult<bool> {
        let result =
            sqlx::query("DELETE FROM insight_state WHERE table_id = ? AND fingerprint = ?")
                .bind(table_id)
                .bind(fingerprint)
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn get_insight_suppressions(
        &self,
        table_id: &str,
    ) -> StoreResult<Vec<InsightSuppressionRow>> {
        let rows = sqlx::query_as::<_, InsightSuppressionRow>(
            "SELECT * FROM insight_suppressions WHERE table_id = ?",
        )
        .bind(table_id)
        .fetch_all(&self.pool)
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
        let row = sqlx::query_as::<_, InsightSuppressionRow>(
            r"INSERT INTO insight_suppressions (table_id, kind, target, created_at)
              VALUES (?, ?, ?, ?)
              ON CONFLICT (table_id, kind, target) DO UPDATE SET created_at = excluded.created_at
              RETURNING *",
        )
        .bind(table_id)
        .bind(kind)
        .bind(target)
        .bind(now_epoch)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn delete_insight_suppression(&self, id: i64) -> StoreResult<bool> {
        let result = sqlx::query("DELETE FROM insight_suppressions WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    // =====================================================
}
