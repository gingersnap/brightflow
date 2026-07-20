-- The pending-proposals count now runs on every curation WS event, not just
-- on badge polls; give the status filter an index (only created_at was
-- indexed before).
CREATE INDEX IF NOT EXISTS idx_action_log_status ON action_log (status);
