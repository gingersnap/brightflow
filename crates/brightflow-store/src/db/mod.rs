//! The store's SQLite pool and its metadata queries, grouped by domain.
//!
//! `StoreDb` is one struct over one pool, reached as `ParquetStore::db()`,
//! and it is the store's query API: callers use its methods directly
//! rather than through wrappers on the facade. The query methods live in
//! per-domain files (catalog, semantics, insights, actions, curation, agent,
//! enrichment) purely for navigability. Every method is still a
//! self-contained named query, so there is exactly one *directory* to look
//! at for "what does the store persist", and the migrations directory stays
//! the only schema authority — `MIGRATIONS` below only embeds it.

mod actions;
mod agent;
mod catalog;
mod curation;
mod enrichment;
mod insights;
mod models;
mod semantics;
mod views;

pub use semantics::{AppliedDeclaration, StoredMetric, StoredRelationship};

use crate::error::StoreResult;
use crate::migration;
use crate::pool::SqlitePool;
use crate::Migration;

/// The store's migration list, in apply order. Append-only: add a new file
/// and a new entry, never edit or reorder existing ones (the migrator
/// rejects gaps and renames).
static MIGRATIONS: &[Migration] = &[
    migration!(1, "001_create_tables"),
    migration!(2, "002_create_table_files"),
    migration!(3, "003_create_table_column_stats"),
    migration!(4, "004_file_partitions_and_stats"),
    migration!(5, "005_column_semantics"),
    migration!(6, "006_add_source_id"),
    migration!(7, "007_source_scoped_tables"),
    migration!(8, "008_table_enrichment_settings"),
    migration!(9, "009_insight_history"),
    migration!(10, "010_action_log"),
    migration!(11, "011_topic_curation"),
    migration!(12, "012_enrichment_algorithm"),
    migration!(13, "013_intent_taxonomy"),
    migration!(14, "014_taxonomy_agent_kinds"),
    migration!(15, "015_sources"),
    migration!(16, "016_enrichment_functions"),
    migration!(17, "017_action_log_status_index"),
    migration!(18, "018_column_polarity"),
    migration!(19, "019_insight_runs"),
    migration!(20, "020_vocabulary_hierarchy"),
    migration!(21, "021_action_log_user"),
    migration!(22, "022_cached_tokens"),
    migration!(23, "023_vocabulary_agent_kinds"),
    migration!(24, "024_unresolved_subjects"),
    migration!(25, "025_ticket_function_kinds"),
    migration!(26, "026_drop_embedding_tables"),
    migration!(27, "027_layered_semantics"),
    migration!(28, "028_declaration_changes"),
    migration!(29, "029_schema_json_columns"),
    migration!(30, "030_describe_table_agent_kind"),
    migration!(31, "031_saved_views"),
    migration!(32, "032_models"),
];

#[derive(Clone, Debug)]
pub struct StoreDb {
    pool: SqlitePool,
}

impl StoreDb {
    pub async fn new(database_url: &str) -> StoreResult<Self> {
        let pool =
            crate::pool::open_pool(database_url, crate::pool::SqlitePoolProfile::METADATA).await?;

        crate::migrate(&pool, MIGRATIONS).await?;

        Ok(Self { pool })
    }

    /// A fresh, fully migrated in-memory database for unit tests.
    #[cfg(test)]
    pub(crate) async fn in_memory() -> Self {
        Self::new(":memory:").await.expect("in-memory store opens")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Guard against adding a migration file and forgetting the list entry
    /// (or vice versa): the embedded list must equal the sorted directory.
    #[test]
    fn migrations_list_matches_directory() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/migrations");
        let mut on_disk: Vec<String> = std::fs::read_dir(dir)
            .expect("migrations dir")
            .map(|e| {
                e.expect("dir entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .filter_map(|n| n.strip_suffix(".sql").map(ToOwned::to_owned))
            .collect();
        on_disk.sort();
        let embedded: Vec<String> = MIGRATIONS.iter().map(|m| m.name.to_owned()).collect();
        assert_eq!(embedded, on_disk);
    }

    /// An in-memory database migrated up to, not including, `version`, so a
    /// data migration can be applied over rows the test plants first.
    fn migrated_before(version: i64) -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory db");
        conn.pragma_update(None, "foreign_keys", "ON")
            .expect("fk pragma");
        for m in MIGRATIONS.iter().take_while(|m| m.version < version) {
            conn.execute_batch(m.sql).expect(m.name);
        }
        conn
    }

    fn migration_sql(version: i64) -> &'static str {
        MIGRATIONS
            .iter()
            .find(|m| m.version == version)
            .expect("migration exists")
            .sql
    }

    #[test]
    fn migration_016_converts_settings_to_promoted_functions() {
        let conn = migrated_before(16);
        // One table with customized enrichment settings, one without any.
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

        conn.execute_batch(migration_sql(16)).expect("016 applies");

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

    #[test]
    fn migration_029_rewrites_old_schema_json_into_table_schema() {
        use brightflow_types::LogicalType as L;
        let conn = migrated_before(29);
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

        conn.execute_batch(migration_sql(29)).expect("029 applies");

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
}
