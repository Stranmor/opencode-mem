with open('crates/infinite-memory/src/event_queries.rs', 'r') as f:
    text = f.read()

text = text.replace(
    'UPDATE raw_events SET processing_instance_id = NULL, retry_count = retry_count + 1 WHERE id = ANY($1)',
    'UPDATE raw_events SET processing_instance_id = NULL, processing_started_at = NULL, retry_count = retry_count + 1 WHERE id = ANY($1)'
)

with open('crates/infinite-memory/src/event_queries.rs', 'w') as f:
    f.write(text)

