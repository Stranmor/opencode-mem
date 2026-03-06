use super::helpers::{create_pg_storage, unique_id};
use opencode_mem_core::{KnowledgeInput, KnowledgeType};
use opencode_mem_storage::traits::KnowledgeStore;

#[tokio::test]
#[ignore]
async fn pg_save_and_search_knowledge() {
    let storage = create_pg_storage().await;
    let tag = unique_id();
    let title = format!("Knowledge integration {tag}");

    let input = KnowledgeInput::new(
        KnowledgeType::Pattern,
        title.clone(),
        format!("Description for pattern {tag}"),
        Some("Step-by-step instructions".to_owned()),
        vec!["trigger-a".to_owned(), "trigger-b".to_owned()],
        Some("pg-test-project".to_owned()),
        Some("obs-ref".to_owned()),
    );

    let saved = storage.save_knowledge(input).await.unwrap();
    assert_eq!(saved.title, title);
    assert_eq!(saved.knowledge_type, KnowledgeType::Pattern);

    let results = storage.search_knowledge("integration", 10).await.unwrap();
    let found = results.iter().any(|r| r.knowledge.id == saved.id);
    assert!(found, "Saved knowledge should be found via search");

    storage.delete_knowledge(&saved.id).await.unwrap();
}

#[tokio::test]
#[ignore]
async fn pg_knowledge_dedup() {
    let storage = create_pg_storage().await;
    let tag = unique_id();
    let title = format!("Dedup knowledge {tag}");

    let input1 = KnowledgeInput::new(
        KnowledgeType::Pattern,
        title.clone(),
        "First description".to_owned(),
        None,
        vec!["trigger1".to_owned()],
        Some("project-a".to_owned()),
        None,
    );
    let saved1 = storage.save_knowledge(input1).await.unwrap();

    let input2 = KnowledgeInput::new(
        KnowledgeType::Pattern,
        title.clone(),
        "Second description".to_owned(),
        None,
        vec!["trigger2".to_owned()],
        Some("project-b".to_owned()),
        None,
    );
    let saved2 = storage.save_knowledge(input2).await.unwrap();

    assert_eq!(saved1.id, saved2.id, "Same title should reuse the same ID");

    let fetched = storage.get_knowledge(&saved2.id).await.unwrap().unwrap();
    assert!(
        fetched.triggers.contains(&"trigger1".to_owned()),
        "First trigger should be preserved"
    );
    assert!(
        fetched.triggers.contains(&"trigger2".to_owned()),
        "Second trigger should be merged"
    );

    assert_eq!(fetched.description, "Second description");

    storage.delete_knowledge(&saved1.id).await.unwrap();
}
