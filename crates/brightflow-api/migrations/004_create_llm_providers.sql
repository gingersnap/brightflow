-- LLM provider configurations (OpenAI-compatible chat-completions endpoints).
-- api_key is stored plaintext, matching the existing precedent for connector
-- tokens; at-rest encryption is shared future work for both.
CREATE TABLE IF NOT EXISTS llm_providers (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT NOT NULL UNIQUE,
    base_url   TEXT NOT NULL,
    api_key    TEXT,
    model      TEXT NOT NULL,
    is_default INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
