-- Enrichment functions: stored, versioned, re-runnable derived-column owners.
-- Kinds: llm_prompt | topic_model | classifier. Draft functions are edited
-- freely; promoted functions run automatically on sync.

CREATE TABLE IF NOT EXISTS enrichment_functions (
    id              TEXT PRIMARY KEY NOT NULL,
    table_id        TEXT NOT NULL REFERENCES tables(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    kind            TEXT NOT NULL CHECK (kind IN ('llm_prompt', 'topic_model', 'classifier')),
    status          TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'promoted')),
    current_version INTEGER NOT NULL DEFAULT 1,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (table_id, name)
);

-- The topics artifact directory is table-keyed (topics_artifact_dir), so at
-- most one topic_model function may exist per table.
CREATE UNIQUE INDEX IF NOT EXISTS enrichment_functions_one_topic_model
    ON enrichment_functions(table_id) WHERE kind = 'topic_model';

CREATE INDEX IF NOT EXISTS enrichment_functions_table_idx
    ON enrichment_functions(table_id);

-- Immutable config snapshots. config_json is the serde form of the engine's
-- FunctionSpec (internally tagged on "kind").
CREATE TABLE IF NOT EXISTS enrichment_function_versions (
    function_id TEXT NOT NULL REFERENCES enrichment_functions(id) ON DELETE CASCADE,
    version     INTEGER NOT NULL,
    config_json TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (function_id, version)
);

CREATE TABLE IF NOT EXISTS enrichment_runs (
    id                TEXT PRIMARY KEY NOT NULL,
    function_id       TEXT NOT NULL REFERENCES enrichment_functions(id) ON DELETE CASCADE,
    version           INTEGER NOT NULL,
    mode              TEXT NOT NULL CHECK (mode IN ('sample', 'full', 'incremental')),
    status            TEXT NOT NULL DEFAULT 'running'
                      CHECK (status IN ('running', 'completed', 'failed', 'cancelled')),
    rows_total        INTEGER NOT NULL DEFAULT 0,
    rows_done         INTEGER NOT NULL DEFAULT 0,
    rows_failed       INTEGER NOT NULL DEFAULT 0,
    rows_cached       INTEGER NOT NULL DEFAULT 0,
    prompt_tokens     INTEGER NOT NULL DEFAULT 0,
    completion_tokens INTEGER NOT NULL DEFAULT 0,
    total_tokens      INTEGER NOT NULL DEFAULT 0,
    error             TEXT,
    created_at        TEXT NOT NULL DEFAULT (datetime('now')),
    finished_at       TEXT
);

CREATE INDEX IF NOT EXISTS enrichment_runs_function_idx
    ON enrichment_runs(function_id, created_at);

-- Per-cell result cache. Keyed on (function, spec content, input content) —
-- deliberately NOT on version, so draft prompt edits recompute without a
-- version bump and reverting a prompt re-hits old cache for free. `version`
-- is bookkeeping only.
CREATE TABLE IF NOT EXISTS enrichment_cache (
    function_id       TEXT NOT NULL REFERENCES enrichment_functions(id) ON DELETE CASCADE,
    spec_hash         TEXT NOT NULL,
    input_hash        TEXT NOT NULL,
    status            TEXT NOT NULL CHECK (status IN ('ok', 'error')),
    value_json        TEXT,
    error             TEXT,
    prompt_tokens     INTEGER,
    completion_tokens INTEGER,
    version           INTEGER NOT NULL DEFAULT 1,
    created_at        TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (function_id, spec_hash, input_hash)
);

-- Data migration: every table_enrichment_settings row becomes a promoted
-- topic_model function at version 1. The deterministic id ('topic-' ||
-- table_id) is safe because at most one topic_model exists per table.
-- table_enrichment_settings stays (deprecated) for one release; old
-- endpoints dual-write it.
INSERT INTO enrichment_functions (id, table_id, name, kind, status, current_version, created_at, updated_at)
SELECT 'topic-' || table_id, table_id, 'topics', 'topic_model', 'promoted', 1,
       datetime('now'), datetime('now')
FROM table_enrichment_settings;

INSERT INTO enrichment_function_versions (function_id, version, config_json, created_at)
SELECT 'topic-' || table_id, 1,
       json_object(
           'kind', 'topic_model',
           'text_columns', CASE
               WHEN text_columns IS NOT NULL AND json_valid(text_columns) THEN json(text_columns)
               ELSE NULL
           END,
           'cleaning_profile', cleaning_profile,
           'language_column', language_column,
           'embedder', embedder,
           'min_cluster_size', min_cluster_size,
           'algorithm', algorithm
       ),
       datetime('now')
FROM table_enrichment_settings;
