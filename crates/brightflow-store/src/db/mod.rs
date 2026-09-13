//! The store's SQLite pool and its metadata queries, grouped by domain.
//!
//! `StoreDb` is one struct over one pool; the query methods live in
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
mod semantics;

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
}
