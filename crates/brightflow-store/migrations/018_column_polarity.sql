-- Measure polarity: which direction of movement is good news. Display-only in
-- v1 — the engine tags findings with a sentiment, the UI colors them; scoring
-- is untouched.
ALTER TABLE column_semantics ADD COLUMN polarity TEXT NOT NULL DEFAULT 'neutral'
  CHECK (polarity IN ('higher_is_better', 'lower_is_better', 'neutral'));
