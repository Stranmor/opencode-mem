-- Normalize project names in infinite memory tables
-- Matches ProjectId::normalize() in crates/core/src/identifiers.rs.

UPDATE events
SET project = LOWER(REPLACE(RTRIM(TRIM(project), '/'), '-', '_'))
WHERE project IS NOT NULL AND project != LOWER(REPLACE(RTRIM(TRIM(project), '/'), '-', '_'));

UPDATE summaries_5min
SET project = LOWER(REPLACE(RTRIM(TRIM(project), '/'), '-', '_'))
WHERE project IS NOT NULL AND project != LOWER(REPLACE(RTRIM(TRIM(project), '/'), '-', '_'));

UPDATE summaries_hour
SET project = LOWER(REPLACE(RTRIM(TRIM(project), '/'), '-', '_'))
WHERE project IS NOT NULL AND project != LOWER(REPLACE(RTRIM(TRIM(project), '/'), '-', '_'));

UPDATE summaries_day
SET project = LOWER(REPLACE(RTRIM(TRIM(project), '/'), '-', '_'))
WHERE project IS NOT NULL AND project != LOWER(REPLACE(RTRIM(TRIM(project), '/'), '-', '_'));
