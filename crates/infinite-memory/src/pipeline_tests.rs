use crate::event_types::Summary;
use chrono::{TimeZone, Utc};

#[test]
fn test_bucket_logic_unsorted() {
    let t1 = Utc.timestamp_opt(1700000000, 0).unwrap();
    let t2 = Utc.timestamp_opt(1700004000, 0).unwrap(); // > 1 hour later
    let t3 = Utc.timestamp_opt(1700000500, 0).unwrap();

    let s1 = Summary {
        id: 1,
        ts_start: t1,
        ts_end: t1,
        session_id: None,
        project: None,
        content: "s1".into(),
        event_count: 1,
        entities: None,
    };
    let s2 = Summary {
        id: 2,
        ts_start: t2,
        ts_end: t2,
        session_id: None,
        project: None,
        content: "s2".into(),
        event_count: 1,
        entities: None,
    };
    let s3 = Summary {
        id: 3,
        ts_start: t3,
        ts_end: t3,
        session_id: None,
        project: None,
        content: "s3".into(),
        event_count: 1,
        entities: None,
    };

    let mut session_summaries = vec![s2.clone(), s1.clone(), s3.clone()];
    session_summaries.sort_by_key(|s| s.ts_start);

    let mut buckets = Vec::new();
    let mut current_bucket = Vec::new();
    let mut bucket_start = session_summaries[0].ts_start;

    for s in session_summaries {
        if s.ts_start.timestamp() / 3600 != bucket_start.timestamp() / 3600 {
            buckets.push(current_bucket.clone());
            current_bucket.clear();
            bucket_start = s.ts_start;
        }
        current_bucket.push(s);
    }
    if !current_bucket.is_empty() {
        buckets.push(current_bucket);
    }

    assert_eq!(buckets.len(), 2);
    assert_eq!(buckets[0].len(), 2); // s1, s3
    assert_eq!(buckets[1].len(), 1); // s2
    assert_eq!(buckets[0][0].id, 1);
    assert_eq!(buckets[0][1].id, 3);
    assert_eq!(buckets[1][0].id, 2);
}
