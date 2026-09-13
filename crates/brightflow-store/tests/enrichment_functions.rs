//! Phase-1 store tests: migration 016 data-migration, function/version CRUD,
//! cache round-trips, and `replace_table_data`.
//!
//! The store-backed tests boot from a copy of the committed test workspace
//! (via `brightflow_test_support`), so they run against a genuinely migrated
//! Litehouse with the committed catalog/Parquet tree present, not a bare empty
//! DB. Each test seeds its own tables/functions on top of that copy. The
//! migration-016 test is pure SQL (no workspace) and stays on a scratch
//! connection.

#![expect(
    clippy::expect_used,
    clippy::shadow_unrelated,
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    reason = "integration tests panic on failure by design"
)]

use brightflow_store::rusqlite::Connection;
use brightflow_store::{IngestMode, IngestOptions, ParquetStore, StoreError};
use brightflow_test_support::{copy_template, TestWorkspace};
use polars::prelude::*;
use std::path::Path;
use tempfile::TempDir;

fn migration_files() -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    let mut files: Vec<(String, String)> = std::fs::read_dir(&dir)
        .expect("migrations dir")
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "sql"))
        .map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let sql = std::fs::read_to_string(e.path()).expect("read migration");
            (name, sql)
        })
        .collect();
    files.sort();
    files
}

fn file_backed_conn(tmp: &TempDir) -> Connection {
    let conn = Connection::open(tmp.path().join("mig.db")).expect("open db");
    conn.pragma_update(None, "foreign_keys", "ON")
        .expect("fk pragma");
    conn
}

/// Apply migrations up to (and including) 015, insert a customized
/// table_enrichment_settings fixture, then apply 016 and assert the settings
/// row became a promoted topic_model function with a matching config_json.
#[test]
fn migration_016_converts_settings_to_promoted_functions() {
    let tmp = TempDir::new().expect("tmp");
    let conn = file_backed_conn(&tmp);

    let files = migration_files();
    let migration_016 = files
        .iter()
        .find(|(name, _)| name.starts_with("016"))
        .expect("016 exists")
        .clone();

    for (name, sql) in &files {
        if name.starts_with("016") {
            break;
        }
        conn.execute_batch(sql).expect(name);
    }

    // Fixture: one table with customized enrichment settings, one without any.
    conn.execute_batch(
        r#"
        INSERT INTO tables (id, name, source_id) VALUES ('t-issues', 'issues', 'connector:x');
        INSERT INTO tables (id, name, source_id) VALUES ('t-users', 'users', 'connector:x');
        INSERT INTO table_enrichment_settings
            (table_id, text_columns, cleaning_profile, language_column, embedder, min_cluster_size, algorithm)
        VALUES
            ('t-issues', '["title","body"]', 'plain', NULL, 'potion-base-32M', 25, 'hdbscan');
        "#,
    )
    .expect("fixture");

    conn.execute_batch(&migration_016.1).expect("016 applies");

    let (id, name, kind, status, current_version): (String, String, String, String, i64) = conn
        .query_row(
            "SELECT id, name, kind, status, current_version FROM enrichment_functions",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .expect("one migrated function");
    assert_eq!(id, "topic-t-issues");
    assert_eq!(name, "topics");
    assert_eq!(kind, "topic_model");
    assert_eq!(status, "promoted");
    assert_eq!(current_version, 1);

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM enrichment_functions", [], |r| {
            r.get(0)
        })
        .expect("count");
    assert_eq!(count, 1, "tables without settings must not get a function");

    let config_json: String = conn
        .query_row(
            "SELECT config_json FROM enrichment_function_versions WHERE function_id = 'topic-t-issues' AND version = 1",
            [],
            |r| r.get(0),
        )
        .expect("version 1 snapshot");
    let config: serde_json::Value = serde_json::from_str(&config_json).expect("valid json");
    assert_eq!(config["kind"], "topic_model");
    assert_eq!(config["text_columns"], serde_json::json!(["title", "body"]));
    assert_eq!(config["cleaning_profile"], "plain");
    assert_eq!(config["language_column"], serde_json::Value::Null);
    assert_eq!(config["embedder"], "potion-base-32M");
    assert_eq!(config["min_cluster_size"], 25);
    assert_eq!(config["algorithm"], "hdbscan");
}

/// Open the store against a copy's genuinely migrated Litehouse + committed
/// Parquet tree (root = the copy's `store/` dir, which the copied catalog is
/// relative to).
async fn temp_store(ws: &TestWorkspace) -> ParquetStore {
    ParquetStore::new(ws.paths.store(), &ws.paths.litehouse_url())
        .await
        .expect("store")
}

async fn seed_table(store: &ParquetStore, source_id: &str, name: &str) -> String {
    store
        .db()
        .create_table(name, source_id)
        .await
        .expect("table")
        .id
}

#[tokio::test]
async fn function_crud_and_version_bump() {
    let ws = copy_template().unwrap();
    let store = temp_store(&ws).await;
    let db = store.db();
    let table_id = seed_table(&store, "upload:a", "leads").await;

    let created = db
        .create_enrichment_function(
            &table_id,
            "sentiment",
            "ticket_classify",
            "draft",
            r#"{"kind":"ticket_classify"}"#,
        )
        .await
        .expect("create");
    assert_eq!(created.current_version, 1);
    assert_eq!(created.status, "draft");

    let next = db
        .update_enrichment_function_config(&created.id, r#"{"kind":"ticket_classify","v":2}"#)
        .await
        .expect("bump");
    assert_eq!(next, 2);

    let reread = db
        .get_enrichment_function(&created.id)
        .await
        .expect("get")
        .expect("exists");
    assert_eq!(reread.current_version, 2);

    let versions = db
        .list_enrichment_function_versions(&created.id)
        .await
        .expect("versions");
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0].version, 2, "newest first");
    assert_eq!(versions[1].config_json, r#"{"kind":"ticket_classify"}"#);

    // Promote / list gating
    assert!(db
        .list_promoted_functions(&table_id, "ticket_classify")
        .await
        .expect("list")
        .is_empty());
    db.set_enrichment_function_status(&created.id, "promoted")
        .await
        .expect("promote");
    assert_eq!(
        db.list_promoted_functions(&table_id, "ticket_classify")
            .await
            .expect("list")
            .len(),
        1
    );

    // Unique (table_id, name)
    let dup = db
        .create_enrichment_function(&table_id, "sentiment", "ticket_classify", "draft", "{}")
        .await;
    assert!(dup.is_err(), "duplicate name must be rejected");

    // Delete cascades versions
    assert!(db
        .delete_enrichment_function(&created.id)
        .await
        .expect("delete"));
    assert!(db
        .list_enrichment_function_versions(&created.id)
        .await
        .expect("versions")
        .is_empty());
}

#[tokio::test]
async fn cache_round_trip_and_housekeeping() {
    let ws = copy_template().unwrap();
    let store = temp_store(&ws).await;
    let db = store.db();
    let table_id = seed_table(&store, "upload:a", "leads").await;
    let f = db
        .create_enrichment_function(&table_id, "fn1", "ticket_classify", "draft", "{}")
        .await
        .expect("create");

    db.upsert_cached_cell(
        &f.id,
        "spec-a",
        "in-1",
        "ok",
        Some(r#"{"v":1}"#),
        None,
        Some(100),
        Some(20),
        None,
        1,
    )
    .await
    .expect("upsert ok");
    db.upsert_cached_cell(
        &f.id,
        "spec-a",
        "in-2",
        "error",
        None,
        Some("boom"),
        Some(50),
        Some(0),
        None,
        1,
    )
    .await
    .expect("upsert err");
    db.upsert_cached_cell(
        &f.id,
        "spec-b",
        "in-1",
        "ok",
        Some(r#"{"v":2}"#),
        None,
        Some(300),
        Some(60),
        None,
        2,
    )
    .await
    .expect("upsert other spec");

    let hits = db
        .get_cached_cells(
            &f.id,
            "spec-a",
            &["in-1".into(), "in-2".into(), "in-miss".into()],
        )
        .await
        .expect("lookup");
    assert_eq!(hits.len(), 2);

    let (total, errors) = db.count_cached(&f.id, "spec-a").await.expect("count");
    assert_eq!((total, errors), (2, 1));

    // p75 over three rows (120, 50, 360 total tokens) → offset 2 → 360
    let p75 = db.cache_stats_p75_tokens(&f.id).await.expect("p75");
    assert_eq!(p75, Some(360));

    // Overwrite updates in place
    db.upsert_cached_cell(
        &f.id,
        "spec-a",
        "in-2",
        "ok",
        Some(r#"{"v":9}"#),
        None,
        Some(10),
        Some(5),
        None,
        1,
    )
    .await
    .expect("overwrite");
    let (_, errors) = db.count_cached(&f.id, "spec-a").await.expect("count");
    assert_eq!(errors, 0);

    // scope=failed clearing
    db.upsert_cached_cell(
        &f.id,
        "spec-a",
        "in-3",
        "error",
        None,
        Some("x"),
        None,
        None,
        None,
        1,
    )
    .await
    .expect("err row");
    assert_eq!(
        db.delete_error_cache_for_spec(&f.id, "spec-a")
            .await
            .expect("clear errors"),
        1
    );

    // Housekeeping: keep only spec-b
    let pruned = db
        .prune_cache_except(&f.id, &["spec-b".to_string()])
        .await
        .expect("prune");
    assert_eq!(pruned, 2);
    let (total_b, _) = db.count_cached(&f.id, "spec-b").await.expect("count b");
    assert_eq!(total_b, 1);
}

#[tokio::test]
async fn run_lifecycle() {
    let ws = copy_template().unwrap();
    let store = temp_store(&ws).await;
    let db = store.db();
    let table_id = seed_table(&store, "upload:a", "leads").await;
    let f = db
        .create_enrichment_function(&table_id, "fn1", "ticket_classify", "draft", "{}")
        .await
        .expect("create");

    let run = db
        .insert_enrichment_run(&f.id, 1, "full", 100)
        .await
        .expect("insert run");
    assert_eq!(run.status, "running");
    assert!(db
        .active_enrichment_run(&f.id)
        .await
        .expect("active")
        .is_some());

    db.update_enrichment_run_progress(&run.id, 40, 2, 10, 4000, 800, 1200)
        .await
        .expect("progress");
    let reread = db
        .get_enrichment_run(&run.id)
        .await
        .expect("get")
        .expect("exists");
    assert_eq!(reread.rows_done, 40);
    assert_eq!(reread.total_tokens, 4800);

    db.finish_enrichment_run(&run.id, "completed", None)
        .await
        .expect("finish");
    assert!(db
        .active_enrichment_run(&f.id)
        .await
        .expect("active")
        .is_none());
}

fn write_test_parquet(dir: &Path, name: &str, ids: &[i64]) -> std::path::PathBuf {
    let mut df = df!("id" => ids.to_vec(), "title" => ids.iter().map(|i| format!("row {i}")).collect::<Vec<_>>())
        .expect("df");
    let path = dir.join(name);
    let file = std::fs::File::create(&path).expect("create");
    ParquetWriter::new(file).finish(&mut df).expect("write");
    path
}

#[tokio::test]
async fn replace_table_data_consolidates_multi_file_tables() {
    let ws = copy_template().unwrap();
    let store = temp_store(&ws).await;

    // Two appends → two files. Fixture parquet is written beside the workspace
    // root (not into the store) and ingested through the genuine path.
    let p1 = write_test_parquet(ws.root(), "a.parquet", &[1, 2, 3]);
    let p2 = write_test_parquet(ws.root(), "b.parquet", &[4, 5]);
    let opts = IngestOptions {
        mode: IngestMode::Append,
        ..Default::default()
    };
    store
        .ingest_parquet("upload:x", "leads", &p1, Some(opts.clone()))
        .await
        .expect("ingest 1");
    let info = store
        .ingest_parquet("upload:x", "leads", &p2, Some(opts))
        .await
        .expect("ingest 2");
    assert_eq!(info.num_files, 2);

    // Replace with an enriched frame (extra column).
    let enriched = df!(
        "id" => [1i64, 2, 3, 4, 5],
        "title" => ["a", "b", "c", "d", "e"],
        "sentiment" => ["pos", "neg", "pos", "neg", "pos"]
    )
    .expect("df");
    store
        .replace_table_data("upload:x", "leads", enriched, Some(info.version))
        .await
        .expect("replace");

    let after = store.table_info("upload:x", "leads").await.expect("info");
    assert_eq!(after.num_files, 1);
    assert_eq!(after.num_rows, Some(5));
    let read = store.read_table("upload:x", "leads").await.expect("read");
    assert!(read.column("sentiment").is_ok());
    assert_eq!(read.height(), 5);

    // Stale expected_version must be refused.
    let err = store
        .replace_table_data(
            "upload:x",
            "leads",
            df!("id" => [1i64]).expect("df"),
            Some(info.version),
        )
        .await
        .expect_err("version moved");
    assert!(matches!(err, StoreError::VersionConflict { .. }));
}

#[tokio::test]
async fn source_registry_round_trip() {
    let ws = copy_template().unwrap();
    let store = temp_store(&ws).await;
    let db = store.db();

    db.register_source(
        "upload:abc",
        "upload",
        "Leads Q3",
        Some(r#"{"file":"leads.csv"}"#),
    )
    .await
    .expect("register");
    let listed = db.list_registered_sources("upload").await.expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "Leads Q3");

    assert!(db
        .get_registered_source("upload:abc")
        .await
        .expect("get")
        .is_some());
    assert!(db
        .delete_registered_source("upload:abc")
        .await
        .expect("delete"));
    assert!(db
        .list_registered_sources("upload")
        .await
        .expect("list")
        .is_empty());
}

/// `get_promoted_function_config` returns the promoted function's *current*
/// version config and None when nothing is promoted.
#[tokio::test]
async fn promoted_function_config_returns_current_version_only() {
    let ws = copy_template().unwrap();
    let store = temp_store(&ws).await;
    let table_id = seed_table(&store, "src", "issues").await;
    let db = store.db();

    // Draft function: not promoted, so no config resolves.
    let created = db
        .create_enrichment_function(&table_id, "topics", "topic_model", "draft", r#"{"v":1}"#)
        .await
        .expect("create");
    assert_eq!(
        db.get_promoted_function_config(&table_id, "topic_model")
            .await
            .expect("query"),
        None,
        "draft functions must not resolve"
    );

    // Promote and bump: the *current* version's config comes back.
    db.set_enrichment_function_status(&created.id, "promoted")
        .await
        .expect("promote");
    db.update_enrichment_function_config(&created.id, r#"{"v":2}"#)
        .await
        .expect("bump");
    assert_eq!(
        db.get_promoted_function_config(&table_id, "topic_model")
            .await
            .expect("query")
            .as_deref(),
        Some(r#"{"v":2}"#)
    );

    // Kind is part of the key.
    assert_eq!(
        db.get_promoted_function_config(&table_id, "ticket_classify")
            .await
            .expect("query"),
        None
    );
}

/// Migration 029 rewrites the engine-shaped `schema_json` of older rows into
/// the contract's `TableSchema`, mapping the Polars type word to a logical
/// type and keeping the word as `physical`; a row already in the new shape
/// is left alone.
#[test]
fn migration_029_rewrites_old_schema_json_into_table_schema() {
    use brightflow_types::LogicalType as L;
    let tmp = TempDir::new().expect("tmp");
    let conn = file_backed_conn(&tmp);
    let files = migration_files();
    let migration_029 = files
        .iter()
        .find(|(name, _)| name.starts_with("029"))
        .expect("029 exists")
        .clone();
    for (name, sql) in &files {
        if name.starts_with("029") {
            break;
        }
        conn.execute_batch(sql).expect(name);
    }
    conn.execute_batch(
        r#"
        INSERT INTO tables (id, name, source_id, schema_json) VALUES
          ('t-old', 'issues', 'c:x',
           '{"fields":[{"name":"id","type":"i64","nullable":true},{"name":"title","type":"str","nullable":true},{"name":"ts","type":"datetime[μs, UTC]","nullable":true},{"name":"tags","type":"list[str]","nullable":true}]}'),
          ('t-new', 'orders', 'c:x',
           '{"columns":[{"name":"id","datatype":"Integer","nullable":true}]}'),
          ('t-none', 'empty', 'c:x', NULL);
        "#,
    )
    .expect("fixture");

    conn.execute_batch(&migration_029.1).expect("029 applies");

    let read = |id: &str| -> Option<String> {
        conn.query_row("SELECT schema_json FROM tables WHERE id = ?", [id], |r| {
            r.get(0)
        })
        .expect("row")
    };
    let old: brightflow_types::TableSchema =
        serde_json::from_str(&read("t-old").expect("json")).expect("parses as TableSchema");
    let types: Vec<(String, L, Option<String>)> = old
        .columns
        .iter()
        .map(|c| (c.name.clone(), c.datatype, c.physical.clone()))
        .collect();
    assert_eq!(
        types,
        vec![
            ("id".to_string(), L::Integer, Some("i64".to_string())),
            ("title".to_string(), L::String, Some("str".to_string())),
            (
                "ts".to_string(),
                L::DateTimeTz,
                Some("datetime[μs, UTC]".to_string())
            ),
            ("tags".to_string(), L::Opaque, Some("list[str]".to_string())),
        ]
    );
    assert_eq!(
        read("t-new").as_deref(),
        Some(r#"{"columns":[{"name":"id","datatype":"Integer","nullable":true}]}"#)
    );
    assert_eq!(read("t-none"), None);
}
