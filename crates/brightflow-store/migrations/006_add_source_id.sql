ALTER TABLE tables ADD COLUMN source_id TEXT;
CREATE INDEX idx_tables_source_id ON tables(source_id);

-- Backfill: web analytics tables follow the events_{source_id} naming convention
UPDATE tables SET source_id = 'web:' || substr(name, 8) WHERE name LIKE 'events_%' AND source_id IS NULL;
