//! Vocabulary persistence: the hierarchical `taxonomy_categories` table every
//! enrichment call resolves against, and the unresolved-subject review queue.

use rusqlite::params;

use super::StoreDb;
use crate::error::{StoreError, StoreResult};
use crate::models::{TaxonomyCategoryRow, UnresolvedSubjectRow};
use crate::row::{execute, fetch_all, fetch_one, fetch_optional};

impl StoreDb {
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

    /// Delete an entry. Children are the caller's problem: the executor
    /// refuses to delete a parent that still has any.
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

    /// Recreate a deleted entry under its ORIGINAL id (undo path).
    ///
    /// The explicit id is load-bearing: cached cells and materialised Parquet
    /// values reference entries by id, so reinserting under a fresh
    /// autoincrement id would restore the entry but silently orphan
    /// everything that pointed at it.
    pub async fn recreate_taxonomy_category(&self, row: &TaxonomyCategoryRow) -> StoreResult<()> {
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
        self.pool
            .call(move |conn| {
                execute(
                    conn,
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
                )
                .map(|_| ())
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
            .recreate_taxonomy_category(&original)
            .await
            .expect_err("must refuse rather than violate the level-scoped UNIQUE");
        assert!(
            err.to_string().contains("billing"),
            "the error must name the clash: {err}"
        );
    }

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
