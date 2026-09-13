//! Models: derived tables with a versioned recipe, and their builds.
//!
//! A model row points at its output table (cascades with it) and at its
//! input table (nulled when that is deleted). Versions are immutable
//! snapshots appended under `BEGIN IMMEDIATE`, as enrichment does, so two
//! concurrent updates cannot compute the same next version. The recipe is
//! JSON the store never reads inside; the API's executor is its only
//! reader. Writes come through the action bus, which decides who may write.

use rusqlite::params;

use super::StoreDb;
use crate::error::StoreResult;
use crate::models::{ModelBuildRow, ModelListRow, ModelRow, ModelVersionRow};
use crate::row::{execute, fetch_all, fetch_one, fetch_optional};

impl StoreDb {
    /// Insert a model and its first version in one transaction.
    pub async fn insert_model(
        &self,
        model: &ModelRow,
        first_version: &ModelVersionRow,
    ) -> StoreResult<()> {
        let model = model.clone();
        let version = first_version.clone();
        self.pool
            .transaction(move |tx| {
                execute(
                    tx,
                    r"INSERT INTO models
                        (id, output_table_id, input_table_id, current_version,
                         created_by, created_at, updated_at)
                      VALUES (?, ?, ?, ?, ?, ?, ?)",
                    params![
                        model.id,
                        model.output_table_id,
                        model.input_table_id,
                        model.current_version,
                        model.created_by,
                        model.created_at,
                        model.updated_at,
                    ],
                )?;
                insert_version(tx, &version)?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    /// Put a deleted model back exactly as it was: the row and every version
    /// under their original ids. The output table must exist again first.
    pub async fn restore_model(
        &self,
        model: &ModelRow,
        versions: &[ModelVersionRow],
    ) -> StoreResult<()> {
        let model = model.clone();
        let versions = versions.to_vec();
        self.pool
            .transaction(move |tx| {
                execute(
                    tx,
                    r"INSERT INTO models
                        (id, output_table_id, input_table_id, current_version,
                         created_by, created_at, updated_at)
                      VALUES (?, ?, ?, ?, ?, ?, ?)",
                    params![
                        model.id,
                        model.output_table_id,
                        model.input_table_id,
                        model.current_version,
                        model.created_by,
                        model.created_at,
                        model.updated_at,
                    ],
                )?;
                for version in &versions {
                    insert_version(tx, version)?;
                }
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub async fn get_model(&self, id: &str) -> StoreResult<Option<ModelRow>> {
        let id = id.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<ModelRow, _>(
                    conn,
                    "SELECT * FROM models WHERE id = ?",
                    params![id],
                )
            })
            .await?;
        Ok(row)
    }

    /// The model whose output is this table, if the table is one.
    pub async fn get_model_by_output(&self, table_id: &str) -> StoreResult<Option<ModelRow>> {
        let table_id = table_id.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<ModelRow, _>(
                    conn,
                    "SELECT * FROM models WHERE output_table_id = ?",
                    params![table_id],
                )
            })
            .await?;
        Ok(row)
    }

    /// Every model whose output lives in a source, with both tables named,
    /// by output name.
    pub async fn list_models_for_source(&self, source_id: &str) -> StoreResult<Vec<ModelListRow>> {
        let source_id = source_id.to_owned();
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<ModelListRow, _>(
                    conn,
                    r"SELECT m.id, o.source_id, m.output_table_id, o.name AS output_table,
                             m.input_table_id, i.name AS input_table,
                             m.current_version, m.created_by, m.created_at, m.updated_at
                      FROM models m
                      JOIN tables o ON o.id = m.output_table_id
                      LEFT JOIN tables i ON i.id = m.input_table_id
                      WHERE o.source_id = ?
                      ORDER BY o.name",
                    params![source_id],
                )
            })
            .await?;
        Ok(rows)
    }

    /// Models built from this table, by output name.
    pub async fn models_with_input(&self, table_id: &str) -> StoreResult<Vec<ModelRow>> {
        let table_id = table_id.to_owned();
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<ModelRow, _>(
                    conn,
                    r"SELECT m.* FROM models m JOIN tables o ON o.id = m.output_table_id
                      WHERE m.input_table_id = ? ORDER BY o.name",
                    params![table_id],
                )
            })
            .await?;
        Ok(rows)
    }

    pub async fn get_model_version(
        &self,
        model_id: &str,
        version: i64,
    ) -> StoreResult<Option<ModelVersionRow>> {
        let model_id = model_id.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<ModelVersionRow, _>(
                    conn,
                    "SELECT * FROM model_versions WHERE model_id = ? AND version = ?",
                    params![model_id, version],
                )
            })
            .await?;
        Ok(row)
    }

    /// Every version of a model, oldest first — what a delete captures for
    /// its undo.
    pub async fn list_model_versions(&self, model_id: &str) -> StoreResult<Vec<ModelVersionRow>> {
        let model_id = model_id.to_owned();
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<ModelVersionRow, _>(
                    conn,
                    "SELECT * FROM model_versions WHERE model_id = ? ORDER BY version",
                    params![model_id],
                )
            })
            .await?;
        Ok(rows)
    }

    /// Append a version and make it current; returns the new version number.
    pub async fn append_model_version(
        &self,
        model_id: &str,
        recipe_json: &str,
        client_spec: Option<&str>,
        created_by: Option<&str>,
        now: i64,
    ) -> StoreResult<i64> {
        let model_id = model_id.to_owned();
        let recipe_json = recipe_json.to_owned();
        let client_spec = client_spec.map(str::to_owned);
        let created_by = created_by.map(str::to_owned);
        let next = self
            .pool
            .transaction(move |tx| {
                // The read stays inside the IMMEDIATE transaction so two
                // concurrent updates cannot both compute the same `next`.
                let (next,): (i64,) = fetch_one(
                    tx,
                    "SELECT current_version + 1 FROM models WHERE id = ?",
                    params![model_id],
                )?;
                insert_version(
                    tx,
                    &ModelVersionRow {
                        model_id: model_id.clone(),
                        version: next,
                        recipe_json,
                        client_spec,
                        created_by,
                        created_at: now,
                    },
                )?;
                execute(
                    tx,
                    "UPDATE models SET current_version = ?, updated_at = ? WHERE id = ?",
                    params![next, now, model_id],
                )?;
                Ok(next)
            })
            .await?;
        Ok(next)
    }

    /// Point a model at an existing earlier version (an update's undo).
    pub async fn set_model_current_version(
        &self,
        model_id: &str,
        version: i64,
        now: i64,
    ) -> StoreResult<bool> {
        let model_id = model_id.to_owned();
        let n = self
            .pool
            .call(move |conn| {
                execute(
                    conn,
                    "UPDATE models SET current_version = ?, updated_at = ? WHERE id = ?",
                    params![version, now, model_id],
                )
            })
            .await?;
        Ok(n > 0)
    }

    /// Delete a model's rows; the output table is the caller's to drop.
    pub async fn delete_model(&self, id: &str) -> StoreResult<bool> {
        let id = id.to_owned();
        let n = self
            .pool
            .call(move |conn| execute(conn, "DELETE FROM models WHERE id = ?", params![id]))
            .await?;
        Ok(n > 0)
    }

    pub async fn insert_model_build(&self, build: &ModelBuildRow) -> StoreResult<()> {
        let build = build.clone();
        self.pool
            .call(move |conn| {
                execute(
                    conn,
                    r"INSERT INTO model_builds
                        (id, model_id, version, status, triggered_by, rows, error,
                         started_at, finished_at)
                      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    params![
                        build.id,
                        build.model_id,
                        build.version,
                        build.status,
                        build.triggered_by,
                        build.rows,
                        build.error,
                        build.started_at,
                        build.finished_at,
                    ],
                )
            })
            .await?;
        Ok(())
    }

    pub async fn finish_model_build(
        &self,
        id: &str,
        status: &str,
        rows: Option<i64>,
        error: Option<&str>,
        finished_at: i64,
    ) -> StoreResult<bool> {
        let id = id.to_owned();
        let status = status.to_owned();
        let error = error.map(str::to_owned);
        let n = self
            .pool
            .call(move |conn| {
                execute(
                    conn,
                    r"UPDATE model_builds
                      SET status = ?, rows = ?, error = ?, finished_at = ? WHERE id = ?",
                    params![status, rows, error, finished_at, id],
                )
            })
            .await?;
        Ok(n > 0)
    }

    pub async fn get_model_build(&self, id: &str) -> StoreResult<Option<ModelBuildRow>> {
        let id = id.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<ModelBuildRow, _>(
                    conn,
                    "SELECT * FROM model_builds WHERE id = ?",
                    params![id],
                )
            })
            .await?;
        Ok(row)
    }

    /// The newest builds across every model, for the jobs list.
    pub async fn list_recent_model_builds(&self, limit: i64) -> StoreResult<Vec<ModelBuildRow>> {
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<ModelBuildRow, _>(
                    conn,
                    "SELECT * FROM model_builds ORDER BY started_at DESC, id DESC LIMIT ?",
                    params![limit],
                )
            })
            .await?;
        Ok(rows)
    }

    /// A model's newest build, for its summary.
    pub async fn latest_model_build(&self, model_id: &str) -> StoreResult<Option<ModelBuildRow>> {
        let model_id = model_id.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<ModelBuildRow, _>(
                    conn,
                    r"SELECT * FROM model_builds WHERE model_id = ?
                      ORDER BY started_at DESC, id DESC LIMIT 1",
                    params![model_id],
                )
            })
            .await?;
        Ok(row)
    }

    /// Builds still marked running from a previous process are failed at
    /// startup; nothing can finish them.
    pub async fn fail_stuck_model_builds(&self, now: i64) -> StoreResult<u64> {
        let n = self
            .pool
            .call(move |conn| {
                execute(
                    conn,
                    r"UPDATE model_builds
                      SET status = 'failed', error = 'interrupted by restart', finished_at = ?
                      WHERE status = 'running'",
                    params![now],
                )
            })
            .await?;
        Ok(n)
    }
}

fn insert_version(tx: &rusqlite::Connection, version: &ModelVersionRow) -> rusqlite::Result<()> {
    execute(
        tx,
        r"INSERT INTO model_versions
            (model_id, version, recipe_json, client_spec, created_by, created_at)
          VALUES (?, ?, ?, ?, ?, ?)",
        params![
            version.model_id,
            version.version,
            version.recipe_json,
            version.client_spec,
            version.created_by,
            version.created_at,
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn temp_db(tmp: &TempDir) -> StoreDb {
        let db_url = format!(
            "sqlite:{}?mode=rwc",
            tmp.path().join("litehouse.db").display()
        );
        StoreDb::new(&db_url).await.expect("failed to open db")
    }

    fn model(id: &str, output: &str, input: &str) -> ModelRow {
        ModelRow {
            id: id.to_string(),
            output_table_id: output.to_string(),
            input_table_id: Some(input.to_string()),
            current_version: 1,
            created_by: Some("user:u1".to_string()),
            created_at: 100,
            updated_at: 100,
        }
    }

    fn version(model_id: &str, version: i64) -> ModelVersionRow {
        ModelVersionRow {
            model_id: model_id.to_string(),
            version,
            recipe_json: format!("{{\"version\":1,\"operations\":[],\"v\":{version}}}"),
            client_spec: None,
            created_by: Some("user:u1".to_string()),
            created_at: 100,
        }
    }

    #[tokio::test]
    async fn insert_get_list_version_and_delete() {
        let tmp = TempDir::new().expect("temp dir");
        let db = temp_db(&tmp).await;
        let input = db.create_table("orders", "c:s1").await.expect("input");
        let output = db
            .create_table("orders_by_month", "c:s1")
            .await
            .expect("output");

        db.insert_model(&model("m1", &output.id, &input.id), &version("m1", 1))
            .await
            .expect("insert");
        let by_output = db
            .get_model_by_output(&output.id)
            .await
            .expect("query")
            .expect("model row");
        assert_eq!(by_output.id, "m1");
        assert_eq!(by_output.current_version, 1);

        let next = db
            .append_model_version(
                "m1",
                "{\"version\":1,\"operations\":[]}",
                Some("{}"),
                None,
                200,
            )
            .await
            .expect("append");
        assert_eq!(next, 2);
        let current = db.get_model("m1").await.expect("query").expect("row");
        assert_eq!(current.current_version, 2);
        assert_eq!(current.updated_at, 200);
        assert_eq!(
            db.list_model_versions("m1").await.expect("versions").len(),
            2
        );

        let listed = db.list_models_for_source("c:s1").await.expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].output_table, "orders_by_month");
        assert_eq!(listed[0].input_table.as_deref(), Some("orders"));

        let dependents = db.models_with_input(&input.id).await.expect("dependents");
        assert_eq!(dependents.len(), 1);

        assert!(db
            .set_model_current_version("m1", 1, 300)
            .await
            .expect("set"));
        assert_eq!(
            db.get_model("m1")
                .await
                .expect("q")
                .expect("row")
                .current_version,
            1
        );

        assert!(db.delete_model("m1").await.expect("delete"));
        assert!(db
            .list_model_versions("m1")
            .await
            .expect("versions")
            .is_empty());
    }

    /// Deleting the output table takes the model; deleting the input only
    /// nulls the link.
    #[tokio::test]
    async fn cascades_follow_the_output_and_set_null_follows_the_input() {
        let tmp = TempDir::new().expect("temp dir");
        let db = temp_db(&tmp).await;
        let input = db.create_table("orders", "c:s1").await.expect("input");
        let output = db.create_table("derived", "c:s1").await.expect("output");
        db.insert_model(&model("m1", &output.id, &input.id), &version("m1", 1))
            .await
            .expect("insert");

        db.delete_table("c:s1", "orders").await.expect("drop input");
        let row = db.get_model("m1").await.expect("q").expect("row");
        assert_eq!(row.input_table_id, None);
        assert!(db.models_with_input(&input.id).await.expect("q").is_empty());

        db.delete_table("c:s1", "derived")
            .await
            .expect("drop output");
        assert!(db.get_model("m1").await.expect("q").is_none());
        assert!(db.list_model_versions("m1").await.expect("q").is_empty());
    }

    #[tokio::test]
    async fn builds_are_recorded_finished_and_failed_on_restart() {
        let tmp = TempDir::new().expect("temp dir");
        let db = temp_db(&tmp).await;
        let input = db.create_table("orders", "c:s1").await.expect("input");
        let output = db.create_table("derived", "c:s1").await.expect("output");
        db.insert_model(&model("m1", &output.id, &input.id), &version("m1", 1))
            .await
            .expect("insert");

        let build = ModelBuildRow {
            id: "b1".to_string(),
            model_id: "m1".to_string(),
            version: 1,
            status: "running".to_string(),
            triggered_by: "create".to_string(),
            rows: None,
            error: None,
            started_at: 100,
            finished_at: None,
        };
        db.insert_model_build(&build).await.expect("insert build");
        assert!(db
            .finish_model_build("b1", "completed", Some(3), None, 101)
            .await
            .expect("finish"));
        let latest = db
            .latest_model_build("m1")
            .await
            .expect("q")
            .expect("build");
        assert_eq!(latest.status, "completed");
        assert_eq!(latest.rows, Some(3));

        db.insert_model_build(&ModelBuildRow {
            id: "b2".to_string(),
            started_at: 102,
            ..build.clone()
        })
        .await
        .expect("insert build");
        assert_eq!(db.fail_stuck_model_builds(103).await.expect("fail"), 1);
        let recent = db.list_recent_model_builds(10).await.expect("list");
        assert_eq!(recent[0].id, "b2");
        assert_eq!(recent[0].status, "failed");
        assert_eq!(recent[0].error.as_deref(), Some("interrupted by restart"));

        // A restore puts the model back with every version under its ids.
        let versions = db.list_model_versions("m1").await.expect("versions");
        let row = db.get_model("m1").await.expect("q").expect("row");
        assert!(db.delete_model("m1").await.expect("delete"));
        db.restore_model(&row, &versions).await.expect("restore");
        assert_eq!(
            db.list_model_versions("m1").await.expect("versions").len(),
            1
        );
    }
}
