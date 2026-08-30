-- Vocabulary hierarchy: `taxonomy_categories` becomes the one table for every
-- closed list the LLM resolves against — induced kinds (category,
-- subcategory, feedback_category) and imported kinds (product, competitor) —
-- with a parent pointer for the two-level hierarchies.
--
-- Why a rebuild: the uniqueness key changes from (table_id, name) to
-- (table_id, kind, parent_id, name) so the reserved `other` value can exist
-- under every parent, and SQLite cannot alter a UNIQUE constraint in place.
--
-- Why `parent_id` is NOT NULL with 0 as the root sentinel: SQLite treats NULLs
-- as distinct in a unique index, so a nullable parent would let the same root
-- name be defined twice.
--
-- Why the child table is rebuilt too: migrations run with foreign_keys=ON, and
-- DROP TABLE on a parent performs an implicit DELETE that fires the child's
-- ON DELETE CASCADE. Backing the labels up, dropping the child first, and
-- re-creating it after the rename is what keeps every row label alive.

CREATE TABLE taxonomy_categories_new (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    table_id     TEXT NOT NULL,
    kind         TEXT NOT NULL DEFAULT 'category'
                 CHECK (kind IN ('category', 'subcategory', 'feedback_category', 'product', 'competitor')),
    -- 0 = root. Non-zero rows point at a row of the parent kind
    -- (subcategory -> category, product component -> product area).
    parent_id    INTEGER NOT NULL DEFAULT 0,
    name         TEXT NOT NULL,
    description  TEXT,
    -- A frozen row refuses rename/redefine through the action executors; it
    -- is how `category` holds the long-horizon trend line while the levels
    -- below it are recalibrated.
    frozen       INTEGER NOT NULL DEFAULT 0,
    -- JSON array of accepted surface forms; imported kinds only.
    aliases_json TEXT,
    created_at   INTEGER NOT NULL,
    UNIQUE (table_id, kind, parent_id, name)
);

INSERT INTO taxonomy_categories_new (id, table_id, kind, parent_id, name, description, frozen, aliases_json, created_at)
SELECT id, table_id, 'category', 0, name, description, 0, NULL, created_at
FROM taxonomy_categories;

CREATE TEMP TABLE document_labels_backup AS
SELECT id, table_id, row_id, category_id, source, created_at FROM document_labels;

DROP TABLE document_labels;
DROP TABLE taxonomy_categories;

ALTER TABLE taxonomy_categories_new RENAME TO taxonomy_categories;

CREATE INDEX IF NOT EXISTS idx_taxonomy_categories_table_kind
    ON taxonomy_categories (table_id, kind, parent_id);

CREATE TABLE document_labels (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    table_id    TEXT NOT NULL,
    row_id      TEXT NOT NULL,
    category_id INTEGER NOT NULL REFERENCES taxonomy_categories(id) ON DELETE CASCADE,
    source      TEXT NOT NULL CHECK (source IN ('agent', 'human')),
    created_at  INTEGER NOT NULL,
    UNIQUE (table_id, row_id, category_id)
);

INSERT INTO document_labels (id, table_id, row_id, category_id, source, created_at)
SELECT id, table_id, row_id, category_id, source, created_at FROM document_labels_backup;

DROP TABLE document_labels_backup;

CREATE INDEX IF NOT EXISTS idx_document_labels_table ON document_labels (table_id);
CREATE INDEX IF NOT EXISTS idx_document_labels_row ON document_labels (table_id, row_id);
CREATE INDEX IF NOT EXISTS idx_document_labels_category ON document_labels (category_id);
