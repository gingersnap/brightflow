-- Per-file partition values (e.g., date = '2026-04-01')
CREATE TABLE IF NOT EXISTS file_partitions (
    file_id TEXT NOT NULL REFERENCES table_files(id) ON DELETE CASCADE,
    partition_key TEXT NOT NULL,
    partition_value TEXT NOT NULL,
    PRIMARY KEY (file_id, partition_key)
);
CREATE INDEX IF NOT EXISTS idx_file_partitions_lookup
    ON file_partitions(partition_key, partition_value);

-- Per-file column statistics (min/max for file-level pruning)
CREATE TABLE IF NOT EXISTS file_column_stats (
    file_id TEXT NOT NULL REFERENCES table_files(id) ON DELETE CASCADE,
    column_name TEXT NOT NULL,
    min_value TEXT,
    max_value TEXT,
    null_count INTEGER,
    PRIMARY KEY (file_id, column_name)
);

-- Partition column definitions on the table itself
ALTER TABLE tables ADD COLUMN partition_columns TEXT;
