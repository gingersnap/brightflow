-- Admit the two built-in ticket kinds (`ticket_classify`, `ticket_extract`)
-- to `enrichment_functions.kind`. The CHECK cannot be altered in place, so
-- the table is rebuilt — and because migrations run with foreign_keys=ON,
-- DROP TABLE on the parent would cascade-delete every version, run and
-- cached cell. The three child tables are therefore backed up, dropped
-- first, and recreated (schema as of 016 + 022) after the rename. Row ids
-- are preserved throughout: `state.enrichment_jobs` and the frontend key on
-- function ids, and cache rows key on them.

CREATE TABLE enrichment_functions_new (
    id              TEXT PRIMARY KEY NOT NULL,
    table_id        TEXT NOT NULL REFERENCES tables(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    kind            TEXT NOT NULL CHECK (kind IN (
                        'llm_prompt', 'topic_model', 'classifier',
                        'ticket_classify', 'ticket_extract'
                    )),
    status          TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'promoted')),
    current_version INTEGER NOT NULL DEFAULT 1,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (table_id, name)
);

INSERT INTO enrichment_functions_new
    (id, table_id, name, kind, status, current_version, created_at, updated_at)
SELECT id, table_id, name, kind, status, current_version, created_at, updated_at
FROM enrichment_functions;

CREATE TEMP TABLE versions_backup AS SELECT * FROM enrichment_function_versions;
CREATE TEMP TABLE runs_backup AS SELECT * FROM enrichment_runs;
CREATE TEMP TABLE cache_backup AS SELECT * FROM enrichment_cache;

DROP TABLE enrichment_cache;
DROP TABLE enrichment_runs;
DROP TABLE enrichment_function_versions;
DROP TABLE enrichment_functions;

ALTER TABLE enrichment_functions_new RENAME TO enrichment_functions;

CREATE UNIQUE INDEX IF NOT EXISTS enrichment_functions_one_topic_model
    ON enrichment_functions(table_id) WHERE kind = 'topic_model';
CREATE INDEX IF NOT EXISTS enrichment_functions_table_idx
    ON enrichment_functions(table_id);

CREATE TABLE enrichment_function_versions (
    function_id TEXT NOT NULL REFERENCES enrichment_functions(id) ON DELETE CASCADE,
    version     INTEGER NOT NULL,
    config_json TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (function_id, version)
);
INSERT INTO enrichment_function_versions (function_id, version, config_json, created_at)
SELECT function_id, version, config_json, created_at FROM versions_backup;

CREATE TABLE enrichment_runs (
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
    finished_at       TEXT,
    cached_tokens     INTEGER NOT NULL DEFAULT 0
);
INSERT INTO enrichment_runs
    (id, function_id, version, mode, status, rows_total, rows_done, rows_failed, rows_cached,
     prompt_tokens, completion_tokens, total_tokens, error, created_at, finished_at, cached_tokens)
SELECT id, function_id, version, mode, status, rows_total, rows_done, rows_failed, rows_cached,
       prompt_tokens, completion_tokens, total_tokens, error, created_at, finished_at, cached_tokens
FROM runs_backup;
CREATE INDEX IF NOT EXISTS enrichment_runs_function_idx
    ON enrichment_runs(function_id, created_at);

CREATE TABLE enrichment_cache (
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
    cached_tokens     INTEGER,
    PRIMARY KEY (function_id, spec_hash, input_hash)
);
INSERT INTO enrichment_cache
    (function_id, spec_hash, input_hash, status, value_json, error, prompt_tokens,
     completion_tokens, version, created_at, cached_tokens)
SELECT function_id, spec_hash, input_hash, status, value_json, error, prompt_tokens,
       completion_tokens, version, created_at, cached_tokens
FROM cache_backup;

DROP TABLE cache_backup;
DROP TABLE runs_backup;
DROP TABLE versions_backup;
