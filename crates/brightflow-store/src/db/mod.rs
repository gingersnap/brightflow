//! The store's SQLite pool and its metadata queries, grouped by domain.
//!
//! `StoreDb` is one struct over one pool; the query methods live in
//! per-domain files (catalog, insights, actions, curation, agent,
//! enrichment) purely for navigability. Every method is still a
//! self-contained named query, so there is exactly one *directory* to look
//! at for "what does the store persist", and the migrations directory stays
//! the only schema authority. The pool itself comes from the shared
//! bootstrap in `crate::sqlite`.

mod actions;
mod agent;
mod catalog;
mod curation;
mod enrichment;
mod insights;

use sqlx::SqlitePool;

use crate::error::StoreResult;

#[derive(Clone)]
pub struct StoreDb {
    pool: SqlitePool,
}

impl StoreDb {
    pub async fn new(database_url: &str) -> StoreResult<Self> {
        let pool = crate::sqlite::open_sqlite_pool(
            database_url,
            crate::sqlite::SqlitePoolProfile::METADATA,
        )
        .await?;

        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Self { pool })
    }
}
