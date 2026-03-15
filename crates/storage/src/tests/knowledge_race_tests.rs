use crate::pg_storage::PgStorage;
use crate::traits::KnowledgeStore;
use opencode_mem_core::{KnowledgeInput, KnowledgeType};
use std::sync::Arc;

#[tokio::test]
#[ignore]
async fn test_knowledge_trigram_race_condition() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(20)
        .connect(&url)
        .await
        .unwrap();
    let storage = Arc::new(PgStorage::from_pool(pool));

    let unique_suffix = uuid::Uuid::new_v4().to_string();
    let title = format!(
        "Concurrent Trigram Dedup Race Condition [{}]",
        unique_suffix
    );

    let mut handles = vec![];
    for i in 0..10 {
        let st = Arc::clone(&storage);
        let t = title.clone();
        handles.push(tokio::spawn(async move {
            let input = KnowledgeInput::new(
                KnowledgeType::Gotcha,
                t,
                format!("Description {}", i),
                None,
                vec!["trigger1".to_owned()],
                None,
                None,
            );
            st.save_knowledge(input).await.unwrap()
        }));
    }

    let mut ids = std::collections::HashSet::new();
    for h in handles {
        let res = h.await.unwrap();
        ids.insert(res.id);
    }

    assert_eq!(
        ids.len(),
        1,
        "Race condition: Multiple duplicate knowledge entries created!"
    );
}
