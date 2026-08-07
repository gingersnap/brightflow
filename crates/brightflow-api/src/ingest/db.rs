//! Central ingest metadata: registered sources and the rotating daily salts.
//!
//! Separate from the per-source event buffers because this is small, shared, and
//! read on every single event — while the buffers are large, per-source, and
//! write-heavy.

use brightflow_store::rusqlite::params;
use brightflow_store::{
    execute, fetch_all, fetch_one, fetch_optional, migration, Migration, SqlitePool,
};

use super::error::{IngestError, IngestResult};
use super::models::{CreateSourceRequest, Source, UpdateSourceRequest, UserProfile};

/// The ingest metadata migration list, in apply order. Append-only.
static MIGRATIONS: &[Migration] = &[
    migration!(1, "migrations/ingest", "001_create_sources"),
    migration!(2, "migrations/ingest", "002_create_salts"),
    migration!(3, "migrations/ingest", "003_create_user_profiles"),
];

/// Central ingest metadata database (sources + salts).
#[derive(Debug, Clone)]
pub struct IngestDb {
    pool: SqlitePool,
}

impl IngestDb {
    /// Open or create the ingest metadata database.
    pub async fn new(database_url: &str) -> IngestResult<Self> {
        let pool = brightflow_store::open_pool(
            database_url,
            brightflow_store::SqlitePoolProfile::METADATA,
        )
        .await?;

        brightflow_store::migrate(&pool, MIGRATIONS).await?;

        Ok(Self { pool })
    }

    // ── Sources CRUD ──────────────────────────────────────────────

    pub async fn create_source(&self, req: &CreateSourceRequest) -> IngestResult<Source> {
        let id = uuid::Uuid::now_v7().to_string();
        let domain = req.domain.clone();
        let name = req.name.clone();
        let tz = req.timezone.as_deref().unwrap_or("UTC").to_owned();

        let insert_id = id.clone();
        self.pool
            .call(move |conn| {
                execute(
                    conn,
                    "INSERT INTO sources (id, domain, name, timezone) VALUES (?, ?, ?, ?)",
                    params![insert_id, domain, name, tz],
                )
                .map(|_| ())
            })
            .await?;

        self.get_source(&id).await
    }

    pub async fn get_source(&self, id: &str) -> IngestResult<Source> {
        let id = id.to_owned();
        let source = self
            .pool
            .call(move |conn| {
                fetch_one::<Source, _>(conn, "SELECT * FROM sources WHERE id = ?", params![id])
            })
            .await?;
        Ok(source)
    }

    pub async fn get_source_by_domain(&self, domain: &str) -> IngestResult<Option<Source>> {
        let domain = domain.to_owned();
        let source = self
            .pool
            .call(move |conn| {
                fetch_optional::<Source, _>(
                    conn,
                    "SELECT * FROM sources WHERE domain = ?",
                    params![domain],
                )
            })
            .await?;
        Ok(source)
    }

    pub async fn list_sources(&self) -> IngestResult<Vec<Source>> {
        let sources = self
            .pool
            .call(|conn| {
                fetch_all::<Source, _>(conn, "SELECT * FROM sources ORDER BY created_at DESC", [])
            })
            .await?;
        Ok(sources)
    }

    pub async fn update_source(&self, id: &str, req: &UpdateSourceRequest) -> IngestResult<Source> {
        let update_id = id.to_owned();
        let name = req.name.clone();
        let tz = req.timezone.clone();
        self.pool
            .call(move |conn| {
                if let Some(ref name) = name {
                    execute(
                        conn,
                        "UPDATE sources SET name = ?, updated_at = datetime('now') WHERE id = ?",
                        params![name, update_id],
                    )?;
                }
                if let Some(ref tz) = tz {
                    execute(
                        conn,
                        "UPDATE sources SET timezone = ?, updated_at = datetime('now') WHERE id = ?",
                        params![tz, update_id],
                    )?;
                }
                Ok(())
            })
            .await?;
        self.get_source(id).await
    }

    pub async fn delete_source(&self, id: &str) -> IngestResult<()> {
        let id = id.to_owned();
        self.pool
            .call(move |conn| {
                execute(conn, "DELETE FROM sources WHERE id = ?", params![id]).map(|_| ())
            })
            .await?;
        Ok(())
    }

    // ── Salt Management ───────────────────────────────────────────

    /// Get or create the daily salt for visitor ID hashing.
    pub async fn get_or_create_salt(&self, date: &str) -> IngestResult<String> {
        let date = date.to_owned();
        let salt = self
            .pool
            .call(move |conn| {
                // Try existing salt first
                let existing = fetch_optional::<(String,), _>(
                    conn,
                    "SELECT salt FROM salts WHERE date = ?",
                    params![date],
                )?;
                if let Some((salt,)) = existing {
                    return Ok(salt);
                }

                // Generate new salt and insert (INSERT OR IGNORE handles races)
                let new_salt = uuid::Uuid::new_v4().to_string();
                execute(
                    conn,
                    "INSERT OR IGNORE INTO salts (date, salt) VALUES (?, ?)",
                    params![date, new_salt],
                )?;

                // Re-fetch to handle race conditions (another process may have
                // inserted first)
                let (confirmed_salt,) = fetch_one::<(String,), _>(
                    conn,
                    "SELECT salt FROM salts WHERE date = ?",
                    params![date],
                )?;
                Ok(confirmed_salt)
            })
            .await?;
        Ok(salt)
    }

    // ── User Profiles ────────────────────────────────────────────

    /// Upsert a user profile, merging new traits with existing ones.
    pub async fn upsert_user_profile(
        &self,
        user_id: &str,
        source_id: &str,
        new_traits: &std::collections::HashMap<String, serde_json::Value>,
    ) -> IngestResult<UserProfile> {
        // Fetch existing traits
        let existing = self.get_user_profile(user_id, source_id).await?;
        let mut merged: std::collections::HashMap<String, serde_json::Value> =
            if let Some(ref profile) = existing {
                serde_json::from_str(&profile.traits).unwrap_or_default()
            } else {
                std::collections::HashMap::new()
            };

        // Merge new traits over existing
        for (k, v) in new_traits {
            merged.insert(k.clone(), v.clone());
        }

        let traits_json = serde_json::to_string(&merged)
            .map_err(|e| IngestError::Other(format!("Failed to serialize traits: {e}")))?;

        let upsert_user = user_id.to_owned();
        let upsert_source = source_id.to_owned();
        self.pool
            .call(move |conn| {
                execute(
                    conn,
                    "INSERT INTO user_profiles (user_id, source_id, traits)
                     VALUES (?, ?, ?)
                     ON CONFLICT(user_id, source_id) DO UPDATE SET
                        traits = excluded.traits,
                        updated_at = datetime('now')",
                    params![upsert_user, upsert_source, traits_json],
                )
                .map(|_| ())
            })
            .await?;

        // Return the updated profile
        self.get_user_profile(user_id, source_id)
            .await?
            .ok_or_else(|| IngestError::Other("Profile not found after upsert".to_string()))
    }

    /// Get a user profile by user_id and source_id.
    pub async fn get_user_profile(
        &self,
        user_id: &str,
        source_id: &str,
    ) -> IngestResult<Option<UserProfile>> {
        let user_id = user_id.to_owned();
        let source_id = source_id.to_owned();
        let profile = self
            .pool
            .call(move |conn| {
                fetch_optional::<UserProfile, _>(
                    conn,
                    "SELECT * FROM user_profiles WHERE user_id = ? AND source_id = ?",
                    params![user_id, source_id],
                )
            })
            .await?;
        Ok(profile)
    }

    /// Search user profiles by user_id prefix.
    pub async fn search_user_profiles(
        &self,
        source_id: &str,
        query: &str,
        limit: u32,
    ) -> IngestResult<Vec<UserProfile>> {
        let source_id = source_id.to_owned();
        let pattern = format!("{query}%");
        let profiles = self
            .pool
            .call(move |conn| {
                fetch_all::<UserProfile, _>(
                    conn,
                    "SELECT * FROM user_profiles
                     WHERE source_id = ? AND user_id LIKE ?
                     ORDER BY updated_at DESC
                     LIMIT ?",
                    params![source_id, pattern, limit],
                )
            })
            .await?;
        Ok(profiles)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Guard against adding a migration file and forgetting the list entry
    /// (or vice versa): the embedded list must equal the sorted directory.
    #[test]
    fn migrations_list_matches_directory() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/migrations/ingest");
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
