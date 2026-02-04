-- Migration: Add entities JSONB column to summary tables
-- Apply: psql -h 192.168.0.124 -p 5434 -U postgres -d infinite_memory -f migrations/001_add_entities.sql

ALTER TABLE summaries_5min ADD COLUMN IF NOT EXISTS entities JSONB;
ALTER TABLE summaries_hour ADD COLUMN IF NOT EXISTS entities JSONB;
ALTER TABLE summaries_day ADD COLUMN IF NOT EXISTS entities JSONB;

-- Optional: Create GIN indexes for entity search
CREATE INDEX IF NOT EXISTS idx_summaries_5min_entities ON summaries_5min USING GIN(entities) WHERE entities IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_summaries_hour_entities ON summaries_hour USING GIN(entities) WHERE entities IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_summaries_day_entities ON summaries_day USING GIN(entities) WHERE entities IS NOT NULL;
