-- Record WHICH human performed an action. `actor_type` only distinguishes
-- human from agent; a curated vocabulary needs the audit line to name the
-- person who approved, renamed or froze each entry. NULL for agent rows and
-- for rows written before this column existed.

ALTER TABLE action_log ADD COLUMN user_id TEXT;
