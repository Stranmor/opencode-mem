-- Fix for crash caused by manual ts_date column missing from INSERT
-- Follows SPOT axiom: ts_date is strictly derived from ts_start

ALTER TABLE summaries_day
DROP COLUMN IF EXISTS ts_date CASCADE;

ALTER TABLE summaries_day
ADD COLUMN IF NOT EXISTS ts_date DATE GENERATED ALWAYS AS ((ts_start AT TIME ZONE 'UTC')::date) STORED;
