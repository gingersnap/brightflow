//! Topic-curation overlay: cluster edits, excluded terms, the intent
//! taxonomy, and row-level document labels.

use rusqlite::params;

use super::StoreDb;
use crate::error::{StoreError, StoreResult};
use crate::models::{
    ClusterEditRow, DocumentLabelRow, DocumentLabelWithName, ExcludedTermRow, TaxonomyCategoryRow,
};
use crate::row::{execute, fetch_all, fetch_one, fetch_optional};

impl StoreDb {
    // Cluster edits (curation overlay)
    // =====================================================

    pub async fn get_cluster_edits(&self, table_id: &str) -> StoreResult<Vec<ClusterEditRow>> {
        let table_id = table_id.to_owned();
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<ClusterEditRow, _>(
                    conn,
                    "SELECT * FROM cluster_edits WHERE table_id = ?",
                    params![table_id],
                )
            })
            .await?;
        Ok(rows)
    }

    /// Upsert an edit keyed by (table, centroid fingerprint). `Some(inner)`
    /// fields overwrite; `None` fields keep the existing value.
    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_cluster_edit(
        &self,
        table_id: &str,
        centroid_fingerprint: &str,
        centroid_json: &str,
        cluster_id: Option<i64>,
        custom_name: Option<Option<&str>>,
        label: Option<Option<&str>>,
        is_noise: Option<bool>,
        merged_into: Option<Option<i64>>,
        now_epoch: i64,
    ) -> StoreResult<ClusterEditRow> {
        let table_id = table_id.to_owned();
        let centroid_fingerprint = centroid_fingerprint.to_owned();
        let centroid_json = centroid_json.to_owned();
        let custom_name = custom_name.map(|v| v.map(ToOwned::to_owned));
        let label = label.map(|v| v.map(ToOwned::to_owned));

        let row = self
            .pool
            .call(move |conn| {
                execute(
                    conn,
                    r"INSERT INTO cluster_edits (table_id, centroid_fingerprint, centroid_json, cluster_id, updated_at)
                      VALUES (?, ?, ?, ?, ?)
                      ON CONFLICT (table_id, centroid_fingerprint) DO UPDATE SET
                        centroid_json = excluded.centroid_json,
                        cluster_id = excluded.cluster_id,
                        orphaned = 0,
                        updated_at = excluded.updated_at",
                    params![
                        table_id,
                        centroid_fingerprint,
                        centroid_json,
                        cluster_id,
                        now_epoch
                    ],
                )?;

                if let Some(v) = custom_name {
                    execute(
                        conn,
                        "UPDATE cluster_edits SET custom_name = ? WHERE table_id = ? AND centroid_fingerprint = ?",
                        params![v, table_id, centroid_fingerprint],
                    )?;
                }
                if let Some(v) = label {
                    execute(
                        conn,
                        "UPDATE cluster_edits SET label = ? WHERE table_id = ? AND centroid_fingerprint = ?",
                        params![v, table_id, centroid_fingerprint],
                    )?;
                }
                if let Some(v) = is_noise {
                    execute(
                        conn,
                        "UPDATE cluster_edits SET is_noise = ? WHERE table_id = ? AND centroid_fingerprint = ?",
                        params![v, table_id, centroid_fingerprint],
                    )?;
                }
                if let Some(v) = merged_into {
                    execute(
                        conn,
                        "UPDATE cluster_edits SET merged_into = ? WHERE table_id = ? AND centroid_fingerprint = ?",
                        params![v, table_id, centroid_fingerprint],
                    )?;
                }

                fetch_one::<ClusterEditRow, _>(
                    conn,
                    "SELECT * FROM cluster_edits WHERE table_id = ? AND centroid_fingerprint = ?",
                    params![table_id, centroid_fingerprint],
                )
            })
            .await?;
        Ok(row)
    }

    /// Reconciliation write-back: point an edit at a new raw cluster id (or
    /// orphan it when no new centroid matches).
    pub async fn set_cluster_edit_target(
        &self,
        edit_id: i64,
        cluster_id: Option<i64>,
        centroid_json: Option<&str>,
        now_epoch: i64,
    ) -> StoreResult<()> {
        let centroid_json = centroid_json.map(ToOwned::to_owned);
        self.pool
            .call(move |conn| {
                if let Some(json) = centroid_json {
                    execute(
                        conn,
                        r"UPDATE cluster_edits
                          SET cluster_id = ?, centroid_json = ?, orphaned = 0, updated_at = ?
                          WHERE id = ?",
                        params![cluster_id, json, now_epoch, edit_id],
                    )
                    .map(|_| ())
                } else {
                    execute(
                        conn,
                        r"UPDATE cluster_edits
                          SET cluster_id = NULL, orphaned = 1, updated_at = ?
                          WHERE id = ?",
                        params![now_epoch, edit_id],
                    )
                    .map(|_| ())
                }
            })
            .await?;
        Ok(())
    }

    pub async fn delete_cluster_edit(&self, edit_id: i64) -> StoreResult<bool> {
        let affected = self
            .pool
            .call(move |conn| {
                execute(
                    conn,
                    "DELETE FROM cluster_edits WHERE id = ?",
                    params![edit_id],
                )
            })
            .await?;
        Ok(affected > 0)
    }

    // =====================================================
    // Excluded terms
    // =====================================================

    pub async fn get_excluded_terms(&self, table_id: &str) -> StoreResult<Vec<ExcludedTermRow>> {
        let table_id = table_id.to_owned();
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<ExcludedTermRow, _>(
                    conn,
                    "SELECT * FROM excluded_terms WHERE table_id = ? ORDER BY term",
                    params![table_id],
                )
            })
            .await?;
        Ok(rows)
    }

    pub async fn add_excluded_term(
        &self,
        table_id: &str,
        term: &str,
        now_epoch: i64,
    ) -> StoreResult<ExcludedTermRow> {
        let table_id = table_id.to_owned();
        let term = term.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_one::<ExcludedTermRow, _>(
                    conn,
                    r"INSERT INTO excluded_terms (table_id, term, created_at)
                      VALUES (?, ?, ?)
                      ON CONFLICT (table_id, term) DO UPDATE SET created_at = excluded.created_at
                      RETURNING *",
                    params![table_id, term, now_epoch],
                )
            })
            .await?;
        Ok(row)
    }

    pub async fn remove_excluded_term(&self, table_id: &str, term: &str) -> StoreResult<bool> {
        let table_id = table_id.to_owned();
        let term = term.to_owned();
        let affected = self
            .pool
            .call(move |conn| {
                execute(
                    conn,
                    "DELETE FROM excluded_terms WHERE table_id = ? AND term = ?",
                    params![table_id, term],
                )
            })
            .await?;
        Ok(affected > 0)
    }

    // =====================================================
    // Intent taxonomy
    // =====================================================

    pub async fn get_taxonomy_categories(
        &self,
        table_id: &str,
    ) -> StoreResult<Vec<TaxonomyCategoryRow>> {
        let table_id = table_id.to_owned();
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<TaxonomyCategoryRow, _>(
                    conn,
                    "SELECT * FROM taxonomy_categories WHERE table_id = ? ORDER BY name",
                    params![table_id],
                )
            })
            .await?;
        Ok(rows)
    }

    /// Define (or touch) an intent category. Idempotent on (table, name).
    pub async fn upsert_taxonomy_category(
        &self,
        table_id: &str,
        name: &str,
        description: Option<&str>,
        now_epoch: i64,
    ) -> StoreResult<TaxonomyCategoryRow> {
        let table_id = table_id.to_owned();
        let name = name.to_owned();
        let description = description.map(ToOwned::to_owned);
        let row = self
            .pool
            .call(move |conn| {
                fetch_one::<TaxonomyCategoryRow, _>(
                    conn,
                    r"INSERT INTO taxonomy_categories (table_id, name, description, created_at)
                      VALUES (?, ?, ?, ?)
                      ON CONFLICT (table_id, name) DO UPDATE SET
                        description = COALESCE(excluded.description, taxonomy_categories.description)
                      RETURNING *",
                    params![table_id, name, description, now_epoch],
                )
            })
            .await?;
        Ok(row)
    }

    pub async fn get_taxonomy_category(
        &self,
        category_id: i64,
    ) -> StoreResult<Option<TaxonomyCategoryRow>> {
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<TaxonomyCategoryRow, _>(
                    conn,
                    "SELECT * FROM taxonomy_categories WHERE id = ?",
                    params![category_id],
                )
            })
            .await?;
        Ok(row)
    }

    pub async fn get_taxonomy_category_by_name(
        &self,
        table_id: &str,
        name: &str,
    ) -> StoreResult<Option<TaxonomyCategoryRow>> {
        let table_id = table_id.to_owned();
        let name = name.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<TaxonomyCategoryRow, _>(
                    conn,
                    "SELECT * FROM taxonomy_categories WHERE table_id = ? AND name = ?",
                    params![table_id, name],
                )
            })
            .await?;
        Ok(row)
    }

    /// Rename a category. The human's right to fix the LLM's wording.
    pub async fn rename_taxonomy_category(
        &self,
        category_id: i64,
        name: &str,
    ) -> StoreResult<Option<TaxonomyCategoryRow>> {
        let name = name.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<TaxonomyCategoryRow, _>(
                    conn,
                    "UPDATE taxonomy_categories SET name = ? WHERE id = ? RETURNING *",
                    params![name, category_id],
                )
            })
            .await?;
        Ok(row)
    }

    /// Restore a category's name and description (undo path).
    pub async fn update_taxonomy_category(
        &self,
        category_id: i64,
        name: &str,
        description: Option<&str>,
    ) -> StoreResult<Option<TaxonomyCategoryRow>> {
        let name = name.to_owned();
        let description = description.map(ToOwned::to_owned);
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<TaxonomyCategoryRow, _>(
                    conn,
                    "UPDATE taxonomy_categories SET name = ?, description = ? WHERE id = ? RETURNING *",
                    params![name, description, category_id],
                )
            })
            .await?;
        Ok(row)
    }

    /// Delete a category. `document_labels` cascade via the FK
    /// (`foreign_keys(true)` is set on the pool, so the cascade really fires).
    pub async fn delete_taxonomy_category(&self, category_id: i64) -> StoreResult<bool> {
        let affected = self
            .pool
            .call(move |conn| {
                execute(
                    conn,
                    "DELETE FROM taxonomy_categories WHERE id = ?",
                    params![category_id],
                )
            })
            .await?;
        Ok(affected > 0)
    }

    /// Recreate a deleted category under its ORIGINAL id, along with the row
    /// labels that cascaded away (undo path).
    ///
    /// The explicit id is load-bearing: labels reference categories by id, so
    /// reinserting under a fresh autoincrement id would restore the category
    /// but silently orphan every label that pointed at it.
    pub async fn recreate_taxonomy_category(
        &self,
        category_id: i64,
        table_id: &str,
        name: &str,
        description: Option<&str>,
        created_at: i64,
        labels: &[(String, String, i64)],
    ) -> StoreResult<()> {
        // The table carries TWO uniqueness constraints — the primary key AND
        // UNIQUE(table_id, name) — so `ON CONFLICT (id)` alone does not make
        // this insert safe. If the name was re-defined under a NEW id after the
        // delete, reinserting the old row violates the name constraint, the
        // transaction rolls back, and the snapshotted labels are unrecoverable.
        // Detect that case and say so, instead of surfacing a raw 500.
        if let Some(existing) = self.get_taxonomy_category_by_name(table_id, name).await? {
            if existing.id != category_id {
                return Err(StoreError::Other(format!(
                    "cannot restore category '{name}': a different category (id {}) now uses \
                     that name. Rename or remove it first, then retry the undo.",
                    existing.id
                )));
            }
        }

        let table_id = table_id.to_owned();
        let name = name.to_owned();
        let description = description.map(ToOwned::to_owned);
        let labels = labels.to_vec();
        self.pool
            .transaction(move |tx| {
                execute(
                    tx,
                    r"INSERT INTO taxonomy_categories (id, table_id, name, description, created_at)
                      VALUES (?, ?, ?, ?, ?)
                      ON CONFLICT (id) DO UPDATE SET
                        name = excluded.name,
                        description = excluded.description",
                    params![category_id, table_id, name, description, created_at],
                )?;

                let mut stmt = tx.prepare(
                    r"INSERT INTO document_labels (table_id, row_id, category_id, source, created_at)
                      VALUES (?, ?, ?, ?, ?)
                      ON CONFLICT (table_id, row_id, category_id) DO NOTHING",
                )?;
                for (row_id, source, label_created_at) in labels {
                    stmt.execute(params![
                        table_id,
                        row_id,
                        category_id,
                        source,
                        label_created_at
                    ])?;
                }
                Ok(())
            })
            .await?;
        Ok(())
    }

    // =====================================================
    // Document labels (row-level supervision)
    // =====================================================

    /// Every labelled row for a table, with category names — the training feed.
    pub async fn get_document_labels(
        &self,
        table_id: &str,
    ) -> StoreResult<Vec<DocumentLabelWithName>> {
        let table_id = table_id.to_owned();
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<DocumentLabelWithName, _>(
                    conn,
                    r"SELECT dl.row_id, dl.category_id, tc.name, dl.source
                      FROM document_labels dl
                      JOIN taxonomy_categories tc ON tc.id = dl.category_id
                      WHERE dl.table_id = ?
                      ORDER BY dl.row_id, tc.name",
                    params![table_id],
                )
            })
            .await?;
        Ok(rows)
    }

    /// Labels for one row.
    pub async fn get_labels_for_row(
        &self,
        table_id: &str,
        row_id: &str,
    ) -> StoreResult<Vec<DocumentLabelWithName>> {
        let table_id = table_id.to_owned();
        let row_id = row_id.to_owned();
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<DocumentLabelWithName, _>(
                    conn,
                    r"SELECT dl.row_id, dl.category_id, tc.name, dl.source
                      FROM document_labels dl
                      JOIN taxonomy_categories tc ON tc.id = dl.category_id
                      WHERE dl.table_id = ? AND dl.row_id = ?
                      ORDER BY tc.name",
                    params![table_id, row_id],
                )
            })
            .await?;
        Ok(rows)
    }

    /// Replace a row's entire label set, transactionally.
    ///
    /// Replace rather than merge: the curation UI shows a row's labels as a set
    /// and the human edits that set, so a partial write would leave labels the
    /// curator thought they had removed. The delete+insert runs in one
    /// transaction so a failure can't leave the row unlabelled.
    pub async fn set_document_labels(
        &self,
        table_id: &str,
        row_id: &str,
        category_ids: &[i64],
        source: &str,
        now_epoch: i64,
    ) -> StoreResult<Vec<DocumentLabelRow>> {
        {
            let table_id = table_id.to_owned();
            let row_id = row_id.to_owned();
            let category_ids = category_ids.to_vec();
            let source = source.to_owned();
            self.pool
                .transaction(move |tx| {
                    execute(
                        tx,
                        "DELETE FROM document_labels WHERE table_id = ? AND row_id = ?",
                        params![table_id, row_id],
                    )?;

                    let mut stmt = tx.prepare(
                        r"INSERT INTO document_labels (table_id, row_id, category_id, source, created_at)
                          VALUES (?, ?, ?, ?, ?)
                          ON CONFLICT (table_id, row_id, category_id) DO UPDATE SET
                            source = excluded.source,
                            created_at = excluded.created_at",
                    )?;
                    for category_id in category_ids {
                        stmt.execute(params![table_id, row_id, category_id, source, now_epoch])?;
                    }
                    Ok(())
                })
                .await?;
        }

        // Re-read after commit so callers get the rows as stored.
        let table_id = table_id.to_owned();
        let row_id = row_id.to_owned();
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<DocumentLabelRow, _>(
                    conn,
                    "SELECT * FROM document_labels WHERE table_id = ? AND row_id = ? ORDER BY category_id",
                    params![table_id, row_id],
                )
            })
            .await?;
        Ok(rows)
    }

    /// Raw label rows for one document row (undo capture needs `source` and
    /// `created_at`, which the name-joined view drops).
    pub async fn get_label_rows_for_row(
        &self,
        table_id: &str,
        row_id: &str,
    ) -> StoreResult<Vec<DocumentLabelRow>> {
        let table_id = table_id.to_owned();
        let row_id = row_id.to_owned();
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<DocumentLabelRow, _>(
                    conn,
                    "SELECT * FROM document_labels WHERE table_id = ? AND row_id = ? ORDER BY category_id",
                    params![table_id, row_id],
                )
            })
            .await?;
        Ok(rows)
    }

    /// Every label attached to a category — snapshotted before a delete so the
    /// cascade can be undone.
    pub async fn get_document_labels_for_category(
        &self,
        category_id: i64,
    ) -> StoreResult<Vec<DocumentLabelRow>> {
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<DocumentLabelRow, _>(
                    conn,
                    "SELECT * FROM document_labels WHERE category_id = ? ORDER BY row_id",
                    params![category_id],
                )
            })
            .await?;
        Ok(rows)
    }

    /// Restore a row's exact prior label set, preserving each label's original
    /// `source` and `created_at` (undo path).
    ///
    /// An empty `labels` clears the row — that is a real prior state ("this row
    /// was unlabelled"), not a no-op.
    pub async fn restore_document_labels(
        &self,
        table_id: &str,
        row_id: &str,
        labels: &[(i64, String, i64)],
    ) -> StoreResult<()> {
        let table_id = table_id.to_owned();
        let row_id = row_id.to_owned();
        let labels = labels.to_vec();
        self.pool
            .transaction(move |tx| {
                execute(
                    tx,
                    "DELETE FROM document_labels WHERE table_id = ? AND row_id = ?",
                    params![table_id, row_id],
                )?;

                let mut stmt = tx.prepare(
                    r"INSERT INTO document_labels (table_id, row_id, category_id, source, created_at)
                      VALUES (?, ?, ?, ?, ?)
                      ON CONFLICT (table_id, row_id, category_id) DO UPDATE SET
                        source = excluded.source,
                        created_at = excluded.created_at",
                )?;
                for (category_id, source, created_at) in labels {
                    stmt.execute(params![table_id, row_id, category_id, source, created_at])?;
                }
                Ok(())
            })
            .await?;
        Ok(())
    }

    /// Per-category labelled-row counts — drives `MIN_LABEL_SUPPORT` feedback in
    /// the curation UI ("this category has too few examples to train on").
    pub async fn count_labels_per_category(&self, table_id: &str) -> StoreResult<Vec<(i64, i64)>> {
        let table_id = table_id.to_owned();
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<(i64, i64), _>(
                    conn,
                    r"SELECT category_id, COUNT(*) as n
                      FROM document_labels WHERE table_id = ?
                      GROUP BY category_id",
                    params![table_id],
                )
            })
            .await?;
        Ok(rows)
    }

    /// Distinct rows carrying at least one label.
    pub async fn count_labelled_rows(&self, table_id: &str) -> StoreResult<i64> {
        let table_id = table_id.to_owned();
        let (n,) = self
            .pool
            .call(move |conn| {
                fetch_one::<(i64,), _>(
                    conn,
                    "SELECT COUNT(DISTINCT row_id) FROM document_labels WHERE table_id = ?",
                    params![table_id],
                )
            })
            .await?;
        Ok(n)
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

    /// Deleting a category must take its row labels with it — the FK cascade
    /// only fires because the pool sets `foreign_keys(true)`, which is easy to
    /// lose in a refactor and silent when it breaks.
    #[tokio::test]
    async fn deleting_a_category_cascades_to_its_labels() {
        let tmp = TempDir::new().expect("temp dir");
        let db = temp_db(&tmp).await;

        let cat = db
            .upsert_taxonomy_category("t1", "auth failure", Some("cannot log in"), 0)
            .await
            .expect("define");
        db.set_document_labels("t1", "row-1", &[cat.id], "human", 0)
            .await
            .expect("label");
        assert_eq!(db.count_labelled_rows("t1").await.expect("count"), 1);

        assert!(db.delete_taxonomy_category(cat.id).await.expect("delete"));
        assert_eq!(
            db.count_labelled_rows("t1").await.expect("count"),
            0,
            "labels must cascade away with their category"
        );
    }

    /// The undo path reinserts a deleted category under its ORIGINAL id so the
    /// restored labels still point at it.
    #[tokio::test]
    async fn recreating_a_category_restores_its_labels() {
        let tmp = TempDir::new().expect("temp dir");
        let db = temp_db(&tmp).await;

        let cat = db
            .upsert_taxonomy_category("t1", "data loss", None, 0)
            .await
            .expect("define");
        db.set_document_labels("t1", "row-1", &[cat.id], "agent", 0)
            .await
            .expect("label");

        let snapshot: Vec<(String, String, i64)> = db
            .get_document_labels_for_category(cat.id)
            .await
            .expect("snapshot")
            .into_iter()
            .map(|l| (l.row_id, l.source, l.created_at))
            .collect();
        db.delete_taxonomy_category(cat.id).await.expect("delete");

        db.recreate_taxonomy_category(cat.id, "t1", "data loss", None, 0, &snapshot)
            .await
            .expect("recreate");

        let labels = db.get_document_labels("t1").await.expect("labels");
        assert_eq!(labels.len(), 1);
        assert_eq!(labels.first().map(|l| l.name.as_str()), Some("data loss"));
        assert_eq!(labels.first().map(|l| l.category_id), Some(cat.id));
    }

    /// Restoring under an id whose NAME has since been taken must fail loudly
    /// rather than roll back with a raw constraint error.
    #[tokio::test]
    async fn recreating_a_category_reports_a_name_clash() {
        let tmp = TempDir::new().expect("temp dir");
        let db = temp_db(&tmp).await;

        let original = db
            .upsert_taxonomy_category("t1", "billing", None, 0)
            .await
            .expect("define");
        db.delete_taxonomy_category(original.id)
            .await
            .expect("delete");
        // The same name is re-defined and gets a NEW id.
        let replacement = db
            .upsert_taxonomy_category("t1", "billing", None, 0)
            .await
            .expect("redefine");
        assert_ne!(replacement.id, original.id);

        let err = db
            .recreate_taxonomy_category(original.id, "t1", "billing", None, 0, &[])
            .await
            .expect_err("must refuse rather than violate UNIQUE(table_id, name)");
        assert!(
            err.to_string().contains("billing"),
            "the error must name the clash: {err}"
        );
    }

    #[tokio::test]
    async fn set_document_labels_replaces_the_whole_set() {
        let tmp = TempDir::new().expect("temp dir");
        let db = temp_db(&tmp).await;

        let a = db
            .upsert_taxonomy_category("t1", "a", None, 0)
            .await
            .expect("a");
        let b = db
            .upsert_taxonomy_category("t1", "b", None, 0)
            .await
            .expect("b");

        db.set_document_labels("t1", "row-1", &[a.id, b.id], "agent", 0)
            .await
            .expect("set both");
        assert_eq!(
            db.get_labels_for_row("t1", "row-1")
                .await
                .expect("get")
                .len(),
            2
        );

        // Replace, not merge.
        db.set_document_labels("t1", "row-1", &[a.id], "human", 0)
            .await
            .expect("replace");
        let rows = db.get_labels_for_row("t1", "row-1").await.expect("get");
        assert_eq!(rows.len(), 1, "the removed label must be gone, not merged");
        assert_eq!(rows.first().map(|r| r.source.as_str()), Some("human"));

        // An empty set clears the row — a real prior state, not a no-op.
        db.set_document_labels("t1", "row-1", &[], "human", 0)
            .await
            .expect("clear");
        assert!(db
            .get_labels_for_row("t1", "row-1")
            .await
            .expect("get")
            .is_empty());
    }
}
