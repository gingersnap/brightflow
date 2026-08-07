//! SQLite-backed user store for authentication.
//!
//! Owns its own pool and runs the `migrations/` directory at construction, so a
//! fresh deployment gets a usable users table without a separate migrate step.
//! Deliberately thin: it does lookups and inserts and holds no password
//! policy — rows carry whatever hash the caller supplies, and nothing here
//! verifies one.

use brightflow_store::rusqlite::params;
use brightflow_store::{
    fetch_one, fetch_optional, migration, open_pool, Migration, SqlitePool, SqlitePoolProfile,
};

use crate::auth::error::AuthResult;
use crate::auth::models::User;

/// The auth-database migration list, in apply order. Append-only. This
/// directory also owns the `tower_sessions` table (006): the session store is
/// a schema consumer like everything else, not a second migration authority.
static MIGRATIONS: &[Migration] = &[
    migration!(1, "001_create_users"),
    migration!(2, "002_create_user_settings"),
    migration!(3, "003_drop_user_settings"),
    migration!(4, "004_create_llm_providers"),
    migration!(5, "005_drop_is_admin"),
    migration!(6, "006_create_sessions"),
];

#[derive(Clone, Debug)]
pub struct AuthDb {
    pool: SqlitePool,
}

impl AuthDb {
    pub async fn new(database_url: &str) -> AuthResult<Self> {
        let pool = open_pool(database_url, SqlitePoolProfile::METADATA).await?;

        brightflow_store::migrate(&pool, MIGRATIONS).await?;

        Ok(Self { pool })
    }

    /// Raw pool access for the two consumers that run their own SQL against
    /// auth.db: the session store and the llm_providers handlers.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn create_user(
        &self,
        email: &str,
        display_name: &str,
        password_hash: &str,
    ) -> AuthResult<User> {
        let id = uuid::Uuid::new_v4().to_string();
        let email = email.to_owned();
        let display_name = display_name.to_owned();
        let password_hash = password_hash.to_owned();
        let user = self
            .pool
            .call(move |conn| {
                fetch_one::<User, _>(
                    conn,
                    r"INSERT INTO users (id, email, display_name, password_hash)
                      VALUES (?, ?, ?, ?)
                      RETURNING *",
                    params![id, email, display_name, password_hash],
                )
            })
            .await?;

        Ok(user)
    }

    pub async fn get_user_by_id(&self, id: &str) -> AuthResult<Option<User>> {
        let id = id.to_owned();
        let user = self
            .pool
            .call(move |conn| {
                fetch_optional::<User, _>(conn, "SELECT * FROM users WHERE id = ?", params![id])
            })
            .await?;
        Ok(user)
    }

    pub async fn get_user_by_email(&self, email: &str) -> AuthResult<Option<User>> {
        let email = email.to_owned();
        let user = self
            .pool
            .call(move |conn| {
                fetch_optional::<User, _>(
                    conn,
                    "SELECT * FROM users WHERE email = ?",
                    params![email],
                )
            })
            .await?;
        Ok(user)
    }

    pub async fn user_count(&self) -> AuthResult<i64> {
        let count = self
            .pool
            .call(|conn| fetch_one::<(i64,), _>(conn, "SELECT COUNT(*) FROM users", []))
            .await?;
        Ok(count.0)
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
            .map(|e| e.expect("dir entry"))
            .filter(|e| e.path().is_file())
            .filter_map(|e| {
                e.file_name()
                    .to_string_lossy()
                    .strip_suffix(".sql")
                    .map(ToOwned::to_owned)
            })
            .collect();
        on_disk.sort();
        let embedded: Vec<String> = MIGRATIONS.iter().map(|m| m.name.to_owned()).collect();
        assert_eq!(embedded, on_disk);
    }
}
