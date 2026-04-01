CREATE TABLE IF NOT EXISTS user_profiles (
    user_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    traits TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (user_id, source_id)
);

CREATE INDEX IF NOT EXISTS idx_user_profiles_source ON user_profiles(source_id, user_id);
