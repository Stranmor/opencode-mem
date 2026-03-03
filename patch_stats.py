import re

with open('crates/storage/src/pg_storage/mod.rs', 'r') as f:
    text = f.read()

# Add OBSERVATION_COLUMNS to mod.rs
obs_cols = """pub const SESSION_COLUMNS: &str =
    "id, active_project, started_at, last_updated_at, metadata, observation_count";

pub const SUMMARY_COLUMNS: &str = "id, session_id, start_time, end_time, message_count, prompt_number, discovery_tokens, text_summary, metadata, is_active, created_at, updated_at";

pub const OBSERVATION_COLUMNS: &str = "id, project, ts, observation_type, observation_noise_level, title, content_narrative, content_facts, content_decisions, content_todos, content_risks, content_related_files, content_unresolved_questions, message_id, session_id, metadata, search_vec";
"""

text = text.replace(
    'pub const SESSION_COLUMNS: &str =\n    "id, active_project, started_at, last_updated_at, metadata, observation_count";\n\npub const SUMMARY_COLUMNS: &str = "id, session_id, start_time, end_time, message_count, prompt_number, discovery_tokens, text_summary, metadata, is_active, created_at, updated_at";',
    obs_cols
)

with open('crates/storage/src/pg_storage/mod.rs', 'w') as f:
    f.write(text)


with open('crates/storage/src/pg_storage/observations.rs', 'r') as f:
    text = f.read()

text = text.replace('pub const OBSERVATION_COLUMNS: &str = "id, project, ts, observation_type, observation_noise_level, title, content_narrative, content_facts, content_decisions, content_todos, content_risks, content_related_files, content_unresolved_questions, message_id, session_id, metadata, search_vec";\n\n', '')
text = text.replace('OBSERVATION_COLUMNS', 'super::OBSERVATION_COLUMNS')

with open('crates/storage/src/pg_storage/observations.rs', 'w') as f:
    f.write(text)


with open('crates/storage/src/pg_storage/embeddings.rs', 'r') as f:
    text = f.read()

old_query = """        let query = r#"
            SELECT id, project, ts, observation_type, observation_noise_level, title, content_narrative, content_facts, content_decisions, content_todos, content_risks, content_related_files, content_unresolved_questions, message_id, session_id, metadata, search_vec
            FROM observations o
            WHERE NOT EXISTS (
                SELECT 1 FROM embeddings e WHERE e.id = o.id
            )
            ORDER BY ts ASC
            LIMIT $1
        "#;
        
        let rows = sqlx::query(query)"""

new_query = """        let query = format!(r#"
            SELECT {}
            FROM observations o
            WHERE NOT EXISTS (
                SELECT 1 FROM embeddings e WHERE e.id = o.id
            )
            ORDER BY ts ASC
            LIMIT $1
        "#, super::OBSERVATION_COLUMNS);
        
        let rows = sqlx::query(&query)"""

text = text.replace(old_query, new_query)

with open('crates/storage/src/pg_storage/embeddings.rs', 'w') as f:
    f.write(text)


with open('crates/storage/src/pg_storage/stats.rs', 'r') as f:
    text = f.read()

old_query = """        let query = r#"
            SELECT id, project, ts, observation_type, observation_noise_level, title, content_narrative, content_facts, content_decisions, content_todos, content_risks, content_related_files, content_unresolved_questions, message_id, session_id, metadata, search_vec
            FROM observations
            ORDER BY ts DESC
            LIMIT $1
        "#;
        let rows = sqlx::query(query)"""

new_query = """        let query = format!(r#"
            SELECT {}
            FROM observations
            ORDER BY ts DESC
            LIMIT $1
        "#, super::OBSERVATION_COLUMNS);
        let rows = sqlx::query(&query)"""

text = text.replace(old_query, new_query)

with open('crates/storage/src/pg_storage/stats.rs', 'w') as f:
    f.write(text)

print("Patch applied successfully.")
