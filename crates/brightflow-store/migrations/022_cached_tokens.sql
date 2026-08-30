-- Prompt tokens served from the provider's prefix cache, per cell and per
-- run. Without this the "cached tokens are cheaper" claim cannot be measured.
-- NULL = the provider did not report the figure (distinct from 0).

ALTER TABLE enrichment_cache ADD COLUMN cached_tokens INTEGER;
ALTER TABLE enrichment_runs ADD COLUMN cached_tokens INTEGER NOT NULL DEFAULT 0;
