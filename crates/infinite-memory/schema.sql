-- Infinite Memory PostgreSQL Schema
-- Apply: psql -h 192.168.0.124 -p 5434 -U postgres -d infinite_memory -f schema.sql

-- Raw events table (all AGI interactions)
CREATE TABLE IF NOT EXISTS raw_events (
    id BIGSERIAL PRIMARY KEY,
    ts TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    session_id TEXT NOT NULL,
    project TEXT,
    event_type TEXT NOT NULL,
    content JSONB NOT NULL,
    files TEXT[] NOT NULL DEFAULT '{}',
    tools TEXT[] NOT NULL DEFAULT '{}',
    summary_5min_id BIGINT,
    -- For concurrent worker processing (visibility timeout pattern)
    processing_started_at TIMESTAMPTZ,
    processing_instance_id TEXT
);

-- Indexes for raw_events
CREATE INDEX IF NOT EXISTS idx_raw_events_ts ON raw_events(ts DESC);
CREATE INDEX IF NOT EXISTS idx_raw_events_session ON raw_events(session_id);
CREATE INDEX IF NOT EXISTS idx_raw_events_project ON raw_events(project) WHERE project IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_raw_events_unsummarized ON raw_events(ts ASC) 
    WHERE summary_5min_id IS NULL;
CREATE INDEX IF NOT EXISTS idx_raw_events_processing ON raw_events(processing_started_at) 
    WHERE processing_started_at IS NOT NULL;
-- GIN index for JSONB content search
CREATE INDEX IF NOT EXISTS idx_raw_events_content ON raw_events USING GIN(content);

-- 5-minute summaries
CREATE TABLE IF NOT EXISTS summaries_5min (
    id BIGSERIAL PRIMARY KEY,
    ts_start TIMESTAMPTZ NOT NULL,
    ts_end TIMESTAMPTZ NOT NULL,
    session_id TEXT,
    project TEXT,
    content TEXT NOT NULL,
    event_count INTEGER NOT NULL,
    entities JSONB,
    summary_hour_id BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_summaries_5min_ts ON summaries_5min(ts_start);
CREATE INDEX IF NOT EXISTS idx_summaries_5min_session ON summaries_5min(session_id) WHERE session_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_summaries_5min_unaggregated ON summaries_5min(ts_start ASC) 
    WHERE summary_hour_id IS NULL;

-- Hour summaries
CREATE TABLE IF NOT EXISTS summaries_hour (
    id BIGSERIAL PRIMARY KEY,
    ts_start TIMESTAMPTZ NOT NULL,
    ts_end TIMESTAMPTZ NOT NULL,
    session_id TEXT,
    project TEXT,
    content TEXT NOT NULL,
    event_count INTEGER NOT NULL,
    entities JSONB,
    summary_day_id BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_summaries_hour_ts ON summaries_hour(ts_start);
CREATE INDEX IF NOT EXISTS idx_summaries_hour_session ON summaries_hour(session_id) WHERE session_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_summaries_hour_unaggregated ON summaries_hour(ts_start ASC) 
    WHERE summary_day_id IS NULL;

-- Day summaries
CREATE TABLE IF NOT EXISTS summaries_day (
    id BIGSERIAL PRIMARY KEY,
    ts_start TIMESTAMPTZ NOT NULL,
    ts_end TIMESTAMPTZ NOT NULL,
    session_id TEXT,
    project TEXT,
    content TEXT NOT NULL,
    event_count INTEGER NOT NULL,
    entities JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_summaries_day_ts ON summaries_day(ts_start);
CREATE INDEX IF NOT EXISTS idx_summaries_day_session ON summaries_day(session_id) WHERE session_id IS NOT NULL;

-- Foreign keys (deferred to allow schema evolution)
ALTER TABLE raw_events 
    DROP CONSTRAINT IF EXISTS fk_raw_events_summary_5min;
ALTER TABLE raw_events 
    ADD CONSTRAINT fk_raw_events_summary_5min 
    FOREIGN KEY (summary_5min_id) REFERENCES summaries_5min(id)
    ON DELETE SET NULL;

ALTER TABLE summaries_5min 
    DROP CONSTRAINT IF EXISTS fk_summaries_5min_hour;
ALTER TABLE summaries_5min 
    ADD CONSTRAINT fk_summaries_5min_hour 
    FOREIGN KEY (summary_hour_id) REFERENCES summaries_hour(id)
    ON DELETE SET NULL;

ALTER TABLE summaries_hour 
    DROP CONSTRAINT IF EXISTS fk_summaries_hour_day;
ALTER TABLE summaries_hour 
    ADD CONSTRAINT fk_summaries_hour_day 
    FOREIGN KEY (summary_day_id) REFERENCES summaries_day(id)
    ON DELETE SET NULL;
