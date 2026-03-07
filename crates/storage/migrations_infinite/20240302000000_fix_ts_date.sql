CREATE INDEX IF NOT EXISTS idx_raw_events_unsummarized
ON raw_events(ts ASC)
WHERE summary_5min_id IS NULL;

CREATE INDEX IF NOT EXISTS idx_summaries_5min_unaggregated
ON summaries_5min(ts_start ASC)
WHERE summary_hour_id IS NULL;

CREATE INDEX IF NOT EXISTS idx_summaries_hour_unaggregated
ON summaries_hour(ts_start ASC)
WHERE summary_day_id IS NULL;
