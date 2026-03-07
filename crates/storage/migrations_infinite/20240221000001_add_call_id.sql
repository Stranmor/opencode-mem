-- Add call_id for idempotent tracking
DO $$ BEGIN
    ALTER TABLE raw_events ADD COLUMN IF NOT EXISTS call_id TEXT;
EXCEPTION WHEN duplicate_column THEN NULL;
END $$;

CREATE UNIQUE INDEX IF NOT EXISTS idx_raw_events_call_id ON raw_events(call_id) WHERE call_id IS NOT NULL;
