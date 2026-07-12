-- First-class action log: every curation operation — human click or LLM tool
-- call — flows through the same dispatch and lands here (Linear AIG model).
CREATE TABLE IF NOT EXISTS action_log (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    -- Client-generated idempotency key: replaying a request returns the
    -- stored response instead of re-executing.
    request_id   TEXT NOT NULL UNIQUE,
    actor_type   TEXT NOT NULL CHECK (actor_type IN ('human','agent')),
    agent_run_id INTEGER,
    action_kind  TEXT NOT NULL,
    params_json  TEXT NOT NULL,
    result_json  TEXT,
    -- Inverse action (serialized) for undo; NULL = not undoable
    undo_json    TEXT,
    status       TEXT NOT NULL CHECK (status IN ('applied','proposed','rejected','undone','failed')),
    created_at   INTEGER NOT NULL,
    resolved_at  INTEGER
);

CREATE INDEX IF NOT EXISTS idx_action_log_created ON action_log (created_at DESC);

-- Agent runs (3C): background LLM curation sessions that emit actions.
CREATE TABLE IF NOT EXISTS agent_runs (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    kind        TEXT NOT NULL CHECK (kind IN ('auto_label','propose_merges','narrate_insights','triage_insights')),
    mode        TEXT NOT NULL CHECK (mode IN ('propose','auto_apply')),
    scope       TEXT NOT NULL DEFAULT '',
    status      TEXT NOT NULL CHECK (status IN ('running','completed','failed','cancelled')),
    detail      TEXT,
    created_at  INTEGER NOT NULL,
    finished_at INTEGER
);
