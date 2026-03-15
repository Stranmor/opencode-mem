-- Knowledge cleanup migration: strip UUIDs from titles, merge duplicates,
-- fix null/invalid project names.

-- 1. Strip UUID suffixes from knowledge titles.
-- Pattern: optional whitespace + hex UUID (8-4-4+ with optional trailing segments).
UPDATE global_knowledge
SET title = TRIM(REGEXP_REPLACE(title, '\s*[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4,}(-[0-9a-f]*)*\s*', '', 'gi')),
    updated_at = NOW()
WHERE title ~* '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4,}';

-- 2. Merge duplicate knowledge entries (same lowercased title after UUID stripping).
-- Keep the entry with highest usage_count (ties broken by earliest created_at).
-- Sum usage_counts, merge source_observations arrays, take highest confidence.
DO $$
DECLARE
    dup RECORD;
    keeper_id TEXT;
    merged_usage BIGINT;
    merged_obs JSONB;
    max_confidence FLOAT8;
BEGIN
    -- Find groups of duplicates (2+ entries with same normalized title)
    FOR dup IN
        SELECT LOWER(TRIM(title)) AS norm_title,
               knowledge_type,
               COUNT(*) AS cnt
        FROM global_knowledge
        WHERE archived_at IS NULL
          AND LENGTH(TRIM(title)) > 10
        GROUP BY LOWER(TRIM(title)), knowledge_type
        HAVING COUNT(*) > 1
    LOOP
        -- Determine the keeper: highest usage_count, then earliest created_at
        SELECT id INTO keeper_id
        FROM global_knowledge
        WHERE LOWER(TRIM(title)) = dup.norm_title
          AND knowledge_type = dup.knowledge_type
          AND archived_at IS NULL
        ORDER BY usage_count DESC, created_at ASC
        LIMIT 1;

        -- Aggregate usage_count, source_observations, and max confidence from all duplicates
        SELECT COALESCE(SUM(usage_count), 0),
               COALESCE(
                   (SELECT jsonb_agg(DISTINCT elem)
                    FROM global_knowledge g2,
                         jsonb_array_elements(g2.source_observations) AS elem
                    WHERE LOWER(TRIM(g2.title)) = dup.norm_title
                      AND g2.knowledge_type = dup.knowledge_type
                      AND g2.archived_at IS NULL
                      AND elem != 'null'::jsonb),
                   '[]'::jsonb
               ),
               COALESCE(MAX(confidence), 0.5)
        INTO merged_usage, merged_obs, max_confidence
        FROM global_knowledge
        WHERE LOWER(TRIM(title)) = dup.norm_title
          AND knowledge_type = dup.knowledge_type
          AND archived_at IS NULL;

        UPDATE global_knowledge
        SET usage_count = merged_usage,
            source_observations = merged_obs,
            confidence = max_confidence,
            updated_at = NOW()
        WHERE id = keeper_id;

        DELETE FROM global_knowledge
        WHERE LOWER(TRIM(title)) = dup.norm_title
          AND knowledge_type = dup.knowledge_type
          AND archived_at IS NULL
          AND id != keeper_id;
    END LOOP;
END $$;

-- 3. Fix NULL project in observations — set to 'unknown'.
UPDATE observations
SET project = 'unknown'
WHERE project IS NULL;

-- 4. Fix 'ivan plankin' project name (personal name, not a project).
UPDATE observations
SET project = 'unknown'
WHERE LOWER(project) = 'ivan plankin';
