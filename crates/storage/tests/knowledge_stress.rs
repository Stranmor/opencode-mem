#![expect(clippy::unwrap_used, reason = "test code")]

use opencode_mem_core::{KnowledgeInput, KnowledgeType};
use opencode_mem_storage::Storage;
use std::sync::Arc;
use tempfile::tempdir;

#[tokio::test]
async fn test_knowledge_race_condition() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let storage = Arc::new(Storage::new(&db_path).unwrap());

    let mut handles = vec![];
    for i in 0..10 {
        let storage = Arc::clone(&storage);
        handles.push(tokio::spawn(async move {
            let input = KnowledgeInput::new(
                KnowledgeType::Pattern,
                "Race Condition Test".to_owned(),
                format!("Description from thread {i}"),
                Some("Instructions".to_owned()),
                vec!["trigger".to_owned()],
                Some("project".to_owned()),
                Some("obs".to_owned()),
            );
            storage.save_knowledge(input)
        }));
    }

    for handle in handles {
        let _ = handle.await.unwrap();
    }

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM global_knowledge WHERE title = 'Race Condition Test'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(count, 1, "Should only have 1 entry for the same title, but found {count}");
}

#[tokio::test]
async fn test_knowledge_data_loss() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let storage = Storage::new(&db_path).unwrap();

    let input1 = KnowledgeInput::new(
        KnowledgeType::Pattern,
        "Merge Test".to_owned(),
        "Original Description".to_owned(),
        Some("Original Instructions".to_owned()),
        vec!["t1".to_owned()],
        Some("p1".to_owned()),
        Some("o1".to_owned()),
    );
    storage.save_knowledge(input1).unwrap();

    let input2 = KnowledgeInput::new(
        KnowledgeType::Pattern,
        "Merge Test".to_owned(),
        "New Description".to_owned(),
        Some("New Instructions".to_owned()),
        vec!["t2".to_owned()],
        Some("p2".to_owned()),
        Some("o2".to_owned()),
    );
    storage.save_knowledge(input2).unwrap();

    let knowledge = storage.list_knowledge(None, 1).unwrap().pop().unwrap();

    assert_eq!(knowledge.description, "New Description");
    assert!(knowledge.triggers.contains(&"t1".to_owned()));
    assert!(knowledge.triggers.contains(&"t2".to_owned()));
}
