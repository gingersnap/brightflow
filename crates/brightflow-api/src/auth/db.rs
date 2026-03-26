use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

use crate::auth::error::AuthResult;
use crate::auth::models::User;

#[derive(Clone)]
pub struct AuthDb {
    pool: SqlitePool,
}

impl AuthDb {
    pub async fn new(database_url: &str) -> AuthResult<Self> {
        let options = SqliteConnectOptions::from_str(database_url)?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

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
        is_admin: bool,
    ) -> AuthResult<User> {
        let id = uuid::Uuid::new_v4().to_string();
        let user = sqlx::query_as::<_, User>(
            r"INSERT INTO users (id, email, display_name, password_hash, is_admin)
              VALUES (?, ?, ?, ?, ?)
              RETURNING *",
        )
        .bind(&id)
        .bind(email)
        .bind(display_name)
        .bind(password_hash)
        .bind(is_admin)
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
