-- Supervised intent taxonomy: the vocabulary of what tickets are ABOUT, plus
-- per-ROW label assignments used to train the classifier head.
--
-- Why rows and not clusters: the pre-existing label loop is circular. The API's
-- `refresh_label_artifact` builds each label's centroid from the CLUSTER
-- centroid, so `predicted_label` is just the cluster assignment wearing a nicer
-- name — and clusters are format-shaped, so cluster labels re-teach the format
-- bias by construction. Labels must attach to rows to break that loop.
--
-- Mirrors 011_topic_curation.sql: epoch-second INTEGER timestamps supplied by
-- the caller, UNIQUE on the upsert key, loose TEXT `table_id` references.

-- The intent vocabulary itself. Proposed by the `propose_taxonomy` agent, then
-- renamed/pruned by a human — the LLM bootstraps the vocabulary, a human
-- ratifies it.
CREATE TABLE IF NOT EXISTS taxonomy_categories (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    table_id    TEXT NOT NULL,
    name        TEXT NOT NULL,
    description TEXT,
    created_at  INTEGER NOT NULL,
    UNIQUE (table_id, name)
);

-- ROW-level labels. Multi-label: a row may carry several intents, or none.
--
-- `source` records who asserted the label. A human assertion outranks an agent
-- assertion for the same (row, category) — the agent only ever proposes.
CREATE TABLE IF NOT EXISTS document_labels (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    table_id    TEXT NOT NULL,
    -- Row identity per the existing per-table convention (DocRef.id).
    row_id      TEXT NOT NULL,
    category_id INTEGER NOT NULL REFERENCES taxonomy_categories(id) ON DELETE CASCADE,
    source      TEXT NOT NULL CHECK (source IN ('agent', 'human')),
    created_at  INTEGER NOT NULL,
    UNIQUE (table_id, row_id, category_id)
);

-- Training reads every labelled row for a table; curation reads one row at a
-- time. Both are hot enough to index.
CREATE INDEX IF NOT EXISTS idx_document_labels_table ON document_labels (table_id);
CREATE INDEX IF NOT EXISTS idx_document_labels_row ON document_labels (table_id, row_id);
CREATE INDEX IF NOT EXISTS idx_document_labels_category ON document_labels (category_id);
