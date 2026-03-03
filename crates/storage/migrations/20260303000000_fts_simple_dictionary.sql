-- Change FTS dictionary to 'simple' for multilingual support

-- 1. Update the trigger function for `observations`
CREATE OR REPLACE FUNCTION observations_search_vec_update() RETURNS trigger AS $$
        DECLARE
            facts_text TEXT;
            keywords_text TEXT;
        BEGIN
            SELECT COALESCE(string_agg(elem, ' '), '')
              INTO facts_text
              FROM jsonb_array_elements_text(COALESCE(NEW.facts, '[]'::jsonb)) AS elem;

            SELECT COALESCE(string_agg(elem, ' '), '')
              INTO keywords_text
              FROM jsonb_array_elements_text(COALESCE(NEW.keywords, '[]'::jsonb)) AS elem;

            NEW.search_vec :=
                setweight(to_tsvector('simple', COALESCE(NEW.title, '')), 'A') ||
                setweight(to_tsvector('simple', COALESCE(NEW.subtitle, '')), 'B') ||
                setweight(to_tsvector('simple', COALESCE(NEW.narrative, '')), 'C') ||
                setweight(to_tsvector('simple', facts_text), 'C') ||
                setweight(to_tsvector('simple', keywords_text), 'D');
            RETURN NEW;
        END;
$$ LANGUAGE plpgsql;

-- Trigger an update on all observations to rebuild search_vec
UPDATE observations SET id = id;

-- 2. Update generated column for `global_knowledge`
ALTER TABLE global_knowledge DROP COLUMN IF EXISTS search_vec;
ALTER TABLE global_knowledge ADD COLUMN search_vec tsvector GENERATED ALWAYS AS (
    setweight(to_tsvector('simple', COALESCE(title, '')), 'A') ||
    setweight(to_tsvector('simple', COALESCE(description, '')), 'B') ||
    setweight(to_tsvector('simple', COALESCE(instructions, '')), 'C')
) STORED;
CREATE INDEX idx_gk_search_vec ON global_knowledge USING GIN (search_vec);

-- 3. Update generated column for `session_summaries`
ALTER TABLE session_summaries DROP COLUMN IF EXISTS search_vec;
ALTER TABLE session_summaries ADD COLUMN search_vec tsvector GENERATED ALWAYS AS (
    setweight(to_tsvector('simple', COALESCE(request, '')), 'A') ||
    setweight(to_tsvector('simple', COALESCE(learned, '')), 'B') ||
    setweight(to_tsvector('simple', COALESCE(completed, '')), 'C') ||
    to_tsvector('simple', COALESCE(investigated, '')) ||
    to_tsvector('simple', COALESCE(next_steps, '')) ||
    to_tsvector('simple', COALESCE(notes, ''))
) STORED;
CREATE INDEX idx_ss_search_vec ON session_summaries USING GIN (search_vec);
