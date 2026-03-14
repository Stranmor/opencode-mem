-- Normalize project names: lowercase, hyphens→underscores, trim whitespace/trailing slashes.
-- Matches ProjectId::normalize() in crates/core/src/identifiers.rs.

UPDATE observations
SET project = LOWER(REPLACE(RTRIM(TRIM(project), '/'), '-', '_'))
WHERE project IS NOT NULL;

UPDATE sessions
SET project = LOWER(REPLACE(RTRIM(TRIM(project), '/'), '-', '_'))
WHERE project IS NOT NULL;

UPDATE session_summaries
SET project = LOWER(REPLACE(RTRIM(TRIM(project), '/'), '-', '_'))
WHERE project IS NOT NULL;

UPDATE user_prompts
SET project = LOWER(REPLACE(RTRIM(TRIM(project), '/'), '-', '_'))
WHERE project IS NOT NULL;

UPDATE pending_messages
SET project = LOWER(REPLACE(RTRIM(TRIM(project), '/'), '-', '_'))
WHERE project IS NOT NULL;

-- Normalize source_projects JSONB array in global_knowledge.
-- Each element is a string project name that needs the same normalization.
UPDATE global_knowledge
SET source_projects = (
    SELECT COALESCE(jsonb_agg(DISTINCT LOWER(REPLACE(RTRIM(TRIM(elem::text, '"'), '/'), '-', '_'))), '[]'::jsonb)
    FROM jsonb_array_elements(source_projects) AS elem
)
WHERE source_projects != '[]'::jsonb;
