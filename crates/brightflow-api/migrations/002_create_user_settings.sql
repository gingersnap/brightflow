CREATE TABLE IF NOT EXISTS user_settings (
    user_id TEXT PRIMARY KEY NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    data_mode TEXT NOT NULL DEFAULT 'memory',
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
