-- Admit the two intent-taxonomy agent kinds.
--
-- `agent_runs.kind` carries a CHECK constraint and SQLite cannot ALTER one, so
-- the table has to be rebuilt: create the replacement, copy, drop, rename. Row
-- ids are preserved because `action_log.agent_run_id` points at them (an
-- unenforced reference — there is no FK — so a renumbering would silently
-- detach every historical action from its run rather than fail loudly).
--
-- `action_log.action_kind` is deliberately unconstrained, so the four new
-- taxonomy ACTION kinds need no migration.

CREATE TABLE agent_runs_new (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    kind        TEXT NOT NULL CHECK (kind IN (
                    'auto_label',
                    'propose_merges',
                    'narrate_insights',
                    'triage_insights',
                    -- Proposes the intent vocabulary; a human ratifies it.
                    'propose_taxonomy',
                    -- Labels a stratified seed sample into that vocabulary.
                    'label_documents'
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
