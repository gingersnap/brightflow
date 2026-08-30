-- Admit the three vocabulary-induction agent kinds. Same rebuild as 014:
-- `agent_runs.kind` carries a CHECK constraint SQLite cannot alter, and row
-- ids are preserved because `action_log.agent_run_id` points at them.
--
-- The three kinds induce from ticket SUMMARIES (Call A output), one level at
-- a time: root categories; the subcategories of one parent (the parent id
-- travels in `scope`); feedback categories from mention summaries.

CREATE TABLE agent_runs_new (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    kind        TEXT NOT NULL CHECK (kind IN (
                    'auto_label',
                    'propose_merges',
                    'narrate_insights',
                    'triage_insights',
                    'propose_taxonomy',
                    'label_documents',
                    'propose_categories',
                    'propose_subcategories',
                    'propose_feedback_categories'
                )),
    mode        TEXT NOT NULL CHECK (mode IN ('propose', 'auto_apply')),
    scope       TEXT NOT NULL DEFAULT '',
    status      TEXT NOT NULL CHECK (status IN ('running', 'completed', 'failed', 'cancelled')),
    detail      TEXT,
    created_at  INTEGER NOT NULL,
    finished_at INTEGER
);

INSERT INTO agent_runs_new (id, kind, mode, scope, status, detail, created_at, finished_at)
SELECT id, kind, mode, scope, status, detail, created_at, finished_at FROM agent_runs;

DROP TABLE agent_runs;

ALTER TABLE agent_runs_new RENAME TO agent_runs;
