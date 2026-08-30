-- Subjects Call B named that resolved to no vocabulary entry: the review
-- queue. Nine mentions of an unknown competitor is a better signal than
-- silent absorption into the catalog, so unresolved surfaces are counted
-- here per materialisation instead of being dropped. `surface` is the
-- model's normalised NAME for the entity, never a quote from a ticket.
--
-- Mapping one appends the surface as an alias on the chosen entry and marks
-- the row `mapped`; the next materialisation re-resolves it without an LLM
-- call. `ignored` rows stay counted but are hidden from the queue.

CREATE TABLE IF NOT EXISTS unresolved_subjects (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    table_id      TEXT NOT NULL,
    kind          TEXT NOT NULL,
    surface       TEXT NOT NULL,
    mention_count INTEGER NOT NULL DEFAULT 0,
    first_seen    INTEGER NOT NULL,
    last_seen     INTEGER NOT NULL,
    status        TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'mapped', 'ignored')),
    mapped_to     INTEGER,
    UNIQUE (table_id, kind, surface)
);

CREATE INDEX IF NOT EXISTS idx_unresolved_subjects_table
    ON unresolved_subjects (table_id, status, mention_count DESC);
