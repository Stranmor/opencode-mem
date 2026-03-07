mod mutations;
mod queries;

pub use mutations::*;
pub use queries::*;

use chrono::{DateTime, Utc};
use opencode_mem_core::InfiniteSummary;

pub(crate) type SummaryRow = (
    i64,
    DateTime<Utc>,
    DateTime<Utc>,
    Option<String>,
    Option<String>,
    String,
    i32,
    Option<serde_json::Value>,
);

pub(crate) fn row_to_summary(row: SummaryRow) -> InfiniteSummary {
    let (id, ts_start, ts_end, session_id, project, content, event_count, entities_json) = row;
    let entities = entities_json.and_then(|e| {
        serde_json::from_value(e.clone())
            .map_err(|err| {
                tracing::warn!(
                    "Failed to parse SummaryEntities for summary {}: {}",
                    id,
                    err
                );
                err
            })
            .ok()
    });
    InfiniteSummary {
        id,
        ts_start,
        ts_end,
        session_id,
        project,
        content,
        event_count,
        entities,
    }
}
