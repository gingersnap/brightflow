//! Topic-curation overlay: cluster edits, excluded terms, the intent
//! taxonomy, and row-level document labels.

use rusqlite::params;

use super::StoreDb;
use crate::error::{StoreError, StoreResult};
use crate::models::{
    ClusterEditRow, DocumentLabelRow, DocumentLabelWithName, ExcludedTermRow, TaxonomyCategoryRow,
    UnresolvedSubjectRow,
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
    // Vocabulary (taxonomy_categories: every closed list the LLM resolves
    // against — induced kinds and imported kinds, two levels deep)
    // =====================================================

    /// Every vocabulary row of a table, all kinds, parents before children.
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
                    "SELECT * FROM taxonomy_categories WHERE table_id = ?
                     ORDER BY kind, parent_id, name",
                    params![table_id],
                )
            })
            .await?;
        Ok(rows)
    }

    /// One level: the rows of `kind` under `parent_id` (0 = roots).
    pub async fn list_vocabulary(
        &self,
        table_id: &str,
        kind: &str,
        parent_id: i64,
    ) -> StoreResult<Vec<TaxonomyCategoryRow>> {
        let table_id = table_id.to_owned();
        let kind = kind.to_owned();
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<TaxonomyCategoryRow, _>(
                    conn,
                    "SELECT * FROM taxonomy_categories
                     WHERE table_id = ? AND kind = ? AND parent_id = ?
                     ORDER BY name",
                    params![table_id, kind, parent_id],
                )
            })
            .await?;
        Ok(rows)
    }

    /// Rows whose parent is `parent_id`, any kind.
    pub async fn count_vocabulary_children(&self, parent_id: i64) -> StoreResult<i64> {
        let n = self
            .pool
            .call(move |conn| {
                conn.query_row(
                    "SELECT count(*) FROM taxonomy_categories WHERE parent_id = ?",
                    params![parent_id],
                    |r| r.get::<_, i64>(0),
                )
            })
            .await?;
        Ok(n)
    }

    /// Define (or touch) a vocabulary entry. Idempotent on
    /// (table, kind, parent, name): re-defining keeps the id and only fills
    /// a missing description.
    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_taxonomy_category(
        &self,
        table_id: &str,
        kind: &str,
        parent_id: i64,
        name: &str,
        description: Option<&str>,
        aliases_json: Option<&str>,
        now_epoch: i64,
    ) -> StoreResult<TaxonomyCategoryRow> {
        let table_id = table_id.to_owned();
        let kind = kind.to_owned();
        let name = name.to_owned();
        let description = description.map(ToOwned::to_owned);
        let aliases_json = aliases_json.map(ToOwned::to_owned);
        let row = self
            .pool
            .call(move |conn| {
                fetch_one::<TaxonomyCategoryRow, _>(
                    conn,
                    r"INSERT INTO taxonomy_categories
                        (table_id, kind, parent_id, name, description, aliases_json, created_at)
                      VALUES (?, ?, ?, ?, ?, ?, ?)
                      ON CONFLICT (table_id, kind, parent_id, name) DO UPDATE SET
                        description = COALESCE(excluded.description, taxonomy_categories.description),
                        aliases_json = COALESCE(excluded.aliases_json, taxonomy_categories.aliases_json)
                      RETURNING *",
                    params![
                        table_id,
                        kind,
                        parent_id,
                        name,
                        description,
                        aliases_json,
                        now_epoch
                    ],
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

    /// Exact-name lookup within one level.
    pub async fn get_taxonomy_category_by_name(
        &self,
        table_id: &str,
        kind: &str,
        parent_id: i64,
        name: &str,
    ) -> StoreResult<Option<TaxonomyCategoryRow>> {
        let table_id = table_id.to_owned();
        let kind = kind.to_owned();
        let name = name.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<TaxonomyCategoryRow, _>(
                    conn,
                    "SELECT * FROM taxonomy_categories
                     WHERE table_id = ? AND kind = ? AND parent_id = ? AND name = ?",
                    params![table_id, kind, parent_id, name],
                )
            })
            .await?;
        Ok(row)
    }

    /// Rename an entry. The human's right to fix the LLM's wording; the
    /// frozen check belongs to the action executor, not here.
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

    /// Replace an entry's description — the definition the model classifies
    /// against, as distinct from its display name.
    pub async fn set_taxonomy_description(
        &self,
        category_id: i64,
        description: Option<&str>,
    ) -> StoreResult<Option<TaxonomyCategoryRow>> {
        let description = description.map(ToOwned::to_owned);
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<TaxonomyCategoryRow, _>(
                    conn,
                    "UPDATE taxonomy_categories SET description = ? WHERE id = ? RETURNING *",
                    params![description, category_id],
                )
            })
            .await?;
        Ok(row)
    }

    pub async fn set_taxonomy_frozen(
        &self,
        category_id: i64,
        frozen: bool,
    ) -> StoreResult<Option<TaxonomyCategoryRow>> {
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<TaxonomyCategoryRow, _>(
                    conn,
                    "UPDATE taxonomy_categories SET frozen = ? WHERE id = ? RETURNING *",
                    params![frozen, category_id],
                )
            })
            .await?;
        Ok(row)
    }

    /// Restore an entry's name and description (undo path).
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

    /// Delete an entry. `document_labels` cascade via the FK
    /// (`foreign_keys(true)` is set on the pool, so the cascade really fires).
    /// Children are the caller's problem: the executor refuses to delete a
    /// parent that still has any.
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

    /// Recreate a deleted entry under its ORIGINAL id, along with the row
    /// labels that cascaded away (undo path).
    ///
    /// The explicit id is load-bearing: labels and materialised Parquet
    /// values reference entries by id, so reinserting under a fresh
    /// autoincrement id would restore the entry but silently orphan
    /// everything that pointed at it.
    pub async fn recreate_taxonomy_category(
        &self,
        row: &TaxonomyCategoryRow,
        labels: &[(String, String, i64)],
    ) -> StoreResult<()> {
        // The table carries TWO uniqueness constraints — the primary key AND
        // UNIQUE(table_id, kind, parent_id, name) — so `ON CONFLICT (id)`
        // alone does not make this insert safe. If the name was re-defined
        // under a NEW id after the delete, reinserting the old row violates
        // the name constraint, the transaction rolls back, and the
        // snapshotted labels are unrecoverable. Detect that case and say so,
        // instead of surfacing a raw 500.
        if let Some(existing) = self
            .get_taxonomy_category_by_name(&row.table_id, &row.kind, row.parent_id, &row.name)
            .await?
        {
            if existing.id != row.id {
                return Err(StoreError::Other(format!(
                    "cannot restore '{}': a different entry (id {}) now uses that name at \
                     this level. Rename or remove it first, then retry the undo.",
                    row.name, existing.id
                )));
            }
        }

        let row = row.clone();
        let labels = labels.to_vec();
        self.pool
            .transaction(move |tx| {
                execute(
                    tx,
                    r"INSERT INTO taxonomy_categories
                        (id, table_id, kind, parent_id, name, description, frozen, aliases_json, created_at)
                      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                      ON CONFLICT (id) DO UPDATE SET
                        name = excluded.name,
                        description = excluded.description,
                        frozen = excluded.frozen,
                        aliases_json = excluded.aliases_json",
                    params![
                        row.id,
                        row.table_id,
                        row.kind,
                        row.parent_id,
                        row.name,
                        row.description,
                        row.frozen,
                        row.aliases_json,
                        row.created_at
                    ],
                )?;

                let mut stmt = tx.prepare(
                    r"INSERT INTO document_labels (table_id, row_id, category_id, source, created_at)
                      VALUES (?, ?, ?, ?, ?)
                      ON CONFLICT (table_id, row_id, category_id) DO NOTHING",
                )?;
                for (row_id, source, label_created_at) in labels {
                    stmt.execute(params![
                        row.table_id,
                        row_id,
                        row.id,
                        source,
                        label_created_at
                    ])?;
                }
                Ok(())
            })
            .await?;
        Ok(())
    }

    /// Append an accepted surface form to an entry's alias list. Idempotent
    /// (case-insensitive) so mapping the same surface twice stores it once.
    pub async fn append_taxonomy_alias(
        &self,
        category_id: i64,
        alias: &str,
    ) -> StoreResult<Option<TaxonomyCategoryRow>> {
        let alias = alias.trim().to_owned();
        let Some(row) = self.get_taxonomy_category(category_id).await? else {
            return Ok(None);
        };
        let mut aliases: Vec<String> = row
            .aliases_json
            .as_deref()
            .and_then(|j| serde_json::from_str(j).ok())
            .unwrap_or_default();
        if !aliases.iter().any(|a| a.eq_ignore_ascii_case(&alias)) {
            aliases.push(alias);
        }
        let json = serde_json::to_string(&aliases).map_err(|e| StoreError::Other(e.to_string()))?;
        let updated = self
            .pool
            .call(move |conn| {
                fetch_optional::<TaxonomyCategoryRow, _>(
                    conn,
                    "UPDATE taxonomy_categories SET aliases_json = ? WHERE id = ? RETURNING *",
                    params![json, category_id],
                )
            })
            .await?;
        Ok(updated)
    }

    // =====================================================
    // Unresolved subjects (mention-extraction review queue)
    // =====================================================

    /// Replace the counts for one table with the latest materialisation's
    /// `(kind, surface) → count`. Rows no longer seen drop to zero but keep
    /// their status; rows seen again keep theirs too.
    pub async fn sync_unresolved_subjects(
        &self,
        table_id: &str,
        counts: &[((String, String), usize)],
        now_epoch: i64,
    ) -> StoreResult<()> {
        let table_id = table_id.to_owned();
        let counts: Vec<(String, String, i64)> = counts
            .iter()
            .map(|((k, s), n)| (k.clone(), s.clone(), i64::try_from(*n).unwrap_or(i64::MAX)))
            .collect();
        self.pool
            .transaction(move |tx| {
                execute(
                    tx,
                    "UPDATE unresolved_subjects SET mention_count = 0 WHERE table_id = ?",
                    params![table_id],
                )?;
                let mut stmt = tx.prepare(
                    r"INSERT INTO unresolved_subjects
                        (table_id, kind, surface, mention_count, first_seen, last_seen)
                      VALUES (?, ?, ?, ?, ?, ?)
                      ON CONFLICT (table_id, kind, surface) DO UPDATE SET
                        mention_count = excluded.mention_count,
                        last_seen = excluded.last_seen",
                )?;
                for (kind, surface, n) in counts {
                    stmt.execute(params![table_id, kind, surface, n, now_epoch, now_epoch])?;
                }
                Ok(())
            })
            .await?;
        Ok(())
    }

    /// Queue entries, busiest first. `status = None` lists every status.
    pub async fn list_unresolved_subjects(
        &self,
        table_id: &str,
        status: Option<&str>,
    ) -> StoreResult<Vec<UnresolvedSubjectRow>> {
        let table_id = table_id.to_owned();
        let status = status.map(ToOwned::to_owned);
        let rows = self
            .pool
            .call(move |conn| match status {
                Some(st) => fetch_all::<UnresolvedSubjectRow, _>(
                    conn,
                    "SELECT * FROM unresolved_subjects WHERE table_id = ? AND status = ?
                     ORDER BY mention_count DESC, surface",
                    params![table_id, st],
                ),
                None => fetch_all::<UnresolvedSubjectRow, _>(
                    conn,
                    "SELECT * FROM unresolved_subjects WHERE table_id = ?
                     ORDER BY mention_count DESC, surface",
                    params![table_id],
                ),
            })
            .await?;
        Ok(rows)
    }

    pub async fn get_unresolved_subject(
        &self,
        id: i64,
    ) -> StoreResult<Option<UnresolvedSubjectRow>> {
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<UnresolvedSubjectRow, _>(
                    conn,
                    "SELECT * FROM unresolved_subjects WHERE id = ?",
                    params![id],
                )
            })
            .await?;
        Ok(row)
    }

    pub async fn set_unresolved_subject_status(
        &self,
        id: i64,
        status: &str,
        mapped_to: Option<i64>,
    ) -> StoreResult<Option<UnresolvedSubjectRow>> {
        let status = status.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<UnresolvedSubjectRow, _>(
                    conn,
                    "UPDATE unresolved_subjects SET status = ?, mapped_to = ? WHERE id = ?
                     RETURNING *",
                    params![status, mapped_to, id],
                )
            })
            .await?;
        Ok(row)
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
            .upsert_taxonomy_category(
                "t1",
                "category",
                0,
                "auth failure",
                Some("cannot log in"),
                None,
                0,
            )
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
            .upsert_taxonomy_category("t1", "category", 0, "data loss", None, None, 0)
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

        db.recreate_taxonomy_category(&cat, &snapshot)
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
            .upsert_taxonomy_category("t1", "category", 0, "billing", None, None, 0)
            .await
            .expect("define");
        db.delete_taxonomy_category(original.id)
            .await
            .expect("delete");
        // The same name is re-defined and gets a NEW id.
        let replacement = db
            .upsert_taxonomy_category("t1", "category", 0, "billing", None, None, 0)
            .await
            .expect("redefine");
        assert_ne!(replacement.id, original.id);

        let err = db
            .recreate_taxonomy_category(&original, &[])
            .await
            .expect_err("must refuse rather than violate the level-scoped UNIQUE");
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
            .upsert_taxonomy_category("t1", "category", 0, "a", None, None, 0)
            .await
            .expect("a");
        let b = db
            .upsert_taxonomy_category("t1", "category", 0, "b", None, None, 0)
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

    /// The level-scoped uniqueness key: the same name may exist under two
    /// different parents (every parent gets its own "shipping"), and a
    /// subcategory never collides with a root category of the same name.
    #[tokio::test]
    async fn names_are_unique_per_level_not_per_table() {
        let tmp = TempDir::new().expect("temp dir");
        let db = temp_db(&tmp).await;

        let billing = db
            .upsert_taxonomy_category("t1", "category", 0, "billing", None, None, 0)
            .await
            .expect("root");
        let orders = db
            .upsert_taxonomy_category("t1", "category", 0, "orders", None, None, 0)
            .await
            .expect("root");
        let a = db
            .upsert_taxonomy_category("t1", "subcategory", billing.id, "shipping", None, None, 0)
            .await
            .expect("child a");
        let b = db
            .upsert_taxonomy_category("t1", "subcategory", orders.id, "shipping", None, None, 0)
            .await
            .expect("child b");
        assert_ne!(a.id, b.id);
        assert_eq!(a.parent_id, billing.id);
        assert_eq!(
            db.count_vocabulary_children(billing.id).await.expect("n"),
            1
        );
        assert_eq!(
            db.list_vocabulary("t1", "subcategory", orders.id)
                .await
                .expect("list")
                .len(),
            1
        );

        let frozen = db
            .set_taxonomy_frozen(billing.id, true)
            .await
            .expect("freeze")
            .expect("row");
        assert!(frozen.frozen);
    }

    #[tokio::test]
    async fn unresolved_subjects_sync_keeps_status_and_zeroes_the_unseen() {
        let tmp = TempDir::new().expect("temp dir");
        let db = temp_db(&tmp).await;
        let first = vec![
            (("competitor".to_string(), "Unknown Corp".to_string()), 9),
            (("product".to_string(), "invoices page".to_string()), 2),
        ];
        db.sync_unresolved_subjects("t1", &first, 100)
            .await
            .expect("sync");
        let open = db
            .list_unresolved_subjects("t1", Some("open"))
            .await
            .expect("list");
        assert_eq!(open.len(), 2);
        assert_eq!(open[0].surface, "Unknown Corp");
        let mapped = db
            .set_unresolved_subject_status(open[1].id, "mapped", Some(42))
            .await
            .expect("map")
            .expect("row");
        assert_eq!(mapped.mapped_to, Some(42));

        let second = vec![(("competitor".to_string(), "Unknown Corp".to_string()), 11)];
        db.sync_unresolved_subjects("t1", &second, 200)
            .await
            .expect("sync");
        let all = db.list_unresolved_subjects("t1", None).await.expect("list");
        let corp = all.iter().find(|r| r.surface == "Unknown Corp").unwrap();
        assert_eq!(
            (corp.mention_count, corp.first_seen, corp.last_seen),
            (11, 100, 200)
        );
        let page = all.iter().find(|r| r.surface == "invoices page").unwrap();
        assert_eq!((page.mention_count, page.status.as_str()), (0, "mapped"));

        let cat = db
            .upsert_taxonomy_category("t1", "product", 0, "Invoice screen", None, None, 0)
            .await
            .expect("define");
        let with_alias = db
            .append_taxonomy_alias(cat.id, "invoices page")
            .await
            .expect("alias")
            .expect("row");
        db.append_taxonomy_alias(cat.id, "Invoices Page")
            .await
            .expect("alias again");
        assert_eq!(
            with_alias.aliases_json.as_deref(),
            Some("[\"invoices page\"]")
        );
        let again = db
            .get_taxonomy_category(cat.id)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(again.aliases_json.as_deref(), Some("[\"invoices page\"]"));
    }
}
