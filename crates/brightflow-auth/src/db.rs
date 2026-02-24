use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

use crate::error::AuthResult;
use crate::models::{User, UserSettings};

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

    pub async fn get_data_mode(&self, user_id: &str) -> AuthResult<String> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT data_mode FROM user_settings WHERE user_id = ?")
                .bind(user_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map_or_else(|| "memory".to_string(), |r| r.0))
    }

    pub async fn upsert_user_settings(
        &self,
        user_id: &str,
        data_mode: &str,
    ) -> AuthResult<UserSettings> {
        let settings = sqlx::query_as::<_, UserSettings>(
            r"INSERT INTO user_settings (user_id, data_mode, updated_at)
              VALUES (?, ?, datetime('now'))
              ON CONFLICT(user_id) DO UPDATE SET
                data_mode = excluded.data_mode,
                updated_at = excluded.updated_at
              RETURNING *",
        )
        .bind(user_id)
        .bind(data_mode)
        .fetch_one(&self.pool)
        .await?;

        Ok(settings)
    }
}
