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
    processing_started_at TIMESTAMPTZ,
    processing_instance_id TEXT,
    retry_count INT NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS summaries_5min (
    id BIGSERIAL PRIMARY KEY,
    ts_start TIMESTAMPTZ NOT NULL,
    ts_end TIMESTAMPTZ NOT NULL,
    session_id TEXT,
    project TEXT,
    content TEXT NOT NULL,
    event_count INT NOT NULL DEFAULT 0,
    entities JSONB,
    summary_hour_id BIGINT,
    processing_started_at TIMESTAMPTZ,
    processing_instance_id TEXT,
    retry_count INT NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS summaries_hour (
    id BIGSERIAL PRIMARY KEY,
    ts_start TIMESTAMPTZ NOT NULL,
    ts_end TIMESTAMPTZ NOT NULL,
    session_id TEXT,
    project TEXT,
    content TEXT NOT NULL,
    event_count INT NOT NULL DEFAULT 0,
    entities JSONB,
    summary_day_id BIGINT,
    processing_started_at TIMESTAMPTZ,
    processing_instance_id TEXT,
    retry_count INT NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS summaries_day (
    id BIGSERIAL PRIMARY KEY,
    ts_start TIMESTAMPTZ NOT NULL,
    ts_end TIMESTAMPTZ NOT NULL,
    session_id TEXT,
    project TEXT,
    content TEXT NOT NULL,
    event_count INT NOT NULL DEFAULT 0,
    entities JSONB
);

CREATE INDEX IF NOT EXISTS idx_raw_events_ts ON raw_events(ts);
CREATE INDEX IF NOT EXISTS idx_raw_events_session ON raw_events(session_id);
CREATE INDEX IF NOT EXISTS idx_raw_events_summary ON raw_events(summary_5min_id);
CREATE INDEX IF NOT EXISTS idx_summaries_5min_hour ON summaries_5min(summary_hour_id);
CREATE INDEX IF NOT EXISTS idx_summaries_hour_day ON summaries_hour(summary_day_id);
