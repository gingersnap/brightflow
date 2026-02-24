use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: String,
    pub email: String,
    pub display_name: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub is_admin: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct UserSettings {
    pub user_id: String,
    pub data_mode: String,
    pub updated_at: String,
}

// --- Connector Config ---

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorConfig {
    pub id: String,
    pub name: String,
    pub connector_path: String,
    pub config_json: String,
    pub created_at: String,
    pub updated_at: String,
}

// --- Scheduler Job ---

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct SchedulerJob {
    pub id: String,
    pub name: String,
    pub connector_id: String,
    pub interval_secs: i64,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

// --- Sync State ---

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct SyncState {
    pub connector_id: String,
    pub endpoint: String,
    pub cursor_field: Option<String>,
    pub cursor_value: Option<String>,
    pub last_sync_at: Option<String>,
    pub last_sync_status: Option<String>,
    pub rows_synced: i64,
}

// --- Sync Run ---

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct SyncRun {
    pub id: String,
    pub job_id: Option<String>,
    pub connector_id: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub status: String,
    pub endpoints_synced: Option<String>,
    pub rows_synced: i64,
    pub error: Option<String>,
}

impl axum_login::AuthUser for User {
    type Id = String;

    fn id(&self) -> Self::Id {
        self.id.clone()
    }

    fn session_auth_hash(&self) -> &[u8] {
        self.password_hash.as_bytes()
    }
}
