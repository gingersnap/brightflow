-- Models: a table that knows its recipe. A model's output is an ordinary
-- row in `tables` (Explore, the engine and enrichment read it like any
-- other), and this row records where it came from: the input table and a
-- versioned chain of operations. Created, updated and deleted through the
-- action bus; rebuilt after every write to its input.
--
-- Deleting the output table takes the model with it. Deleting the input
-- does not: the model and its last output stay, the input link goes null,
-- and the next build fails with a named error rather than the model
-- silently disappearing.

CREATE TABLE models (
    id               TEXT PRIMARY KEY NOT NULL,
    output_table_id  TEXT NOT NULL UNIQUE REFERENCES tables(id) ON DELETE CASCADE,
    input_table_id   TEXT REFERENCES tables(id) ON DELETE SET NULL,
    current_version  INTEGER NOT NULL DEFAULT 1,
    created_by       TEXT,
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL
);

CREATE INDEX models_input_idx ON models(input_table_id);

-- Immutable recipe snapshots, one per version. `recipe_json` is the contract
-- crate's ModelRecipe; `client_spec` is the Explore snapshot that produced
-- it, opaque to the store, kept so the builder can reopen the model.
CREATE TABLE model_versions (
    model_id     TEXT NOT NULL REFERENCES models(id) ON DELETE CASCADE,
    version      INTEGER NOT NULL,
    recipe_json  TEXT NOT NULL,
    client_spec  TEXT,
    created_by   TEXT,
    created_at   INTEGER NOT NULL,
    PRIMARY KEY (model_id, version)
);

-- One row per build attempt, so Activity can list them like enrichment runs.
CREATE TABLE model_builds (
    id           TEXT PRIMARY KEY NOT NULL,
    model_id     TEXT NOT NULL REFERENCES models(id) ON DELETE CASCADE,
    version      INTEGER NOT NULL,
    status       TEXT NOT NULL CHECK (status IN ('running', 'completed', 'failed')),
    triggered_by TEXT NOT NULL,
    rows         INTEGER,
    error        TEXT,
    started_at   INTEGER NOT NULL,
    finished_at  INTEGER
);

CREATE INDEX model_builds_model_idx ON model_builds(model_id, started_at);
