-- Normalize project names: lowercase, hyphens→underscores, trim whitespace/trailing slashes.
-- Matches ProjectId::normalize() in crates/core/src/identifiers.rs.

UPDATE observations
SET project = LOWER(REPLACE(RTRIM(TRIM(project), '/'), '-', '_'))
WHERE project IS NOT NULL AND project != LOWER(REPLACE(RTRIM(TRIM(project), '/'), '-', '_'));

UPDATE sessions
SET project = LOWER(REPLACE(RTRIM(TRIM(project), '/'), '-', '_'))
WHERE project IS NOT NULL AND project != LOWER(REPLACE(RTRIM(TRIM(project), '/'), '-', '_'));

UPDATE session_summaries
SET project = LOWER(REPLACE(RTRIM(TRIM(project), '/'), '-', '_'))
WHERE project IS NOT NULL AND project != LOWER(REPLACE(RTRIM(TRIM(project), '/'), '-', '_'));

UPDATE user_prompts
SET project = LOWER(REPLACE(RTRIM(TRIM(project), '/'), '-', '_'))
WHERE project IS NOT NULL AND project != LOWER(REPLACE(RTRIM(TRIM(project), '/'), '-', '_'));

UPDATE pending_messages
SET project = LOWER(REPLACE(RTRIM(TRIM(project), '/'), '-', '_'))
WHERE project IS NOT NULL AND project != LOWER(REPLACE(RTRIM(TRIM(project), '/'), '-', '_'));

-- Normalize source_projects JSONB array in global_knowledge.
UPDATE global_knowledge
SET source_projects = (
    SELECT COALESCE(
        jsonb_agg(normalized_elem ORDER BY normalized_elem), 
        '[]'::jsonb
    )
    FROM (
        SELECT DISTINCT LOWER(REPLACE(RTRIM(TRIM(elem), '/'), '-', '_')) AS normalized_elem
        FROM jsonb_array_elements_text(source_projects) AS elem
    ) sub
)
WHERE source_projects != '[]'::jsonb
  AND source_projects != (
      SELECT COALESCE(
          jsonb_agg(normalized_elem ORDER BY normalized_elem), 
          '[]'::jsonb
      )
      FROM (
          SELECT DISTINCT LOWER(REPLACE(RTRIM(TRIM(elem), '/'), '-', '_')) AS normalized_elem
          FROM jsonb_array_elements_text(source_projects) AS elem
      ) sub
  );
