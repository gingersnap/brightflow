//! SQLite-backed user store for authentication.
//!
//! Owns its own pool and runs the `migrations/` directory at construction, so a
//! fresh deployment gets a usable users table without a separate migrate step.
//! Deliberately thin: it does lookups and inserts and holds no password
//! policy — rows carry whatever hash the caller supplies, and nothing here
//! verifies one.

use sqlx::SqlitePool;

use crate::auth::error::AuthResult;
use crate::auth::models::User;
use brightflow_store::{open_sqlite_pool, SqlitePoolProfile};

#[derive(Clone)]
pub struct AuthDb {
    pool: SqlitePool,
}

impl AuthDb {
    pub async fn new(database_url: &str) -> AuthResult<Self> {
        let pool = open_sqlite_pool(database_url, SqlitePoolProfile::METADATA).await?;

        // Run migrations
        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Self { pool })
    }

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
        let user = sqlx::query_as::<_, User>(
            r"INSERT INTO users (id, email, display_name, password_hash)
              VALUES (?, ?, ?, ?)
              RETURNING *",
        )
        .bind(&id)
        .bind(email)
        .bind(display_name)
        .bind(password_hash)
        .fetch_one(&self.pool)
        .await?;

        Ok(user)
    }

    pub async fn get_user_by_id(&self, id: &str) -> AuthResult<Option<User>> {
        let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(user)
    }

    pub async fn get_user_by_email(&self, email: &str) -> AuthResult<Option<User>> {
        let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE email = ?")
            .bind(email)
            .fetch_optional(&self.pool)
            .await?;
        Ok(user)
    }

    pub async fn user_count(&self) -> AuthResult<i64> {
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(&self.pool)
            .await?;
        Ok(count.0)
    }
}
