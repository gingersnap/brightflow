CREATE TABLE IF NOT EXISTS salts (
    date TEXT PRIMARY KEY NOT NULL,
    salt TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
