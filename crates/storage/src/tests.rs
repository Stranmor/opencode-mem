#[cfg(test)]
mod storage_tests {
    use crate::Storage;
    use chrono::Utc;
    use opencode_mem_core::{Observation, ObservationType, Session, SessionStatus};
    use tempfile::TempDir;

    fn create_test_storage() -> (Storage, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let storage = Storage::new(&db_path).unwrap();
        (storage, temp_dir)
    }

    fn create_test_observation(id: &str, project: &str) -> Observation {
        Observation {
            id: id.to_string(),
            session_id: "test-session".to_string(),
            project: Some(project.to_string()),
            observation_type: ObservationType::Discovery,
            title: format!("Test observation {}", id),
            subtitle: Some("Test subtitle".to_string()),
            narrative: Some("Test narrative".to_string()),
            facts: vec!["fact1".to_string(), "fact2".to_string()],
            concepts: vec![],
            files_read: vec!["file1.rs".to_string()],
            files_modified: vec!["file2.rs".to_string()],
            keywords: vec!["test".to_string(), "keyword".to_string()],
            prompt_number: Some(1),
            discovery_tokens: Some(100),
            created_at: Utc::now(),
        }
    }

    fn create_test_session(id: &str) -> Session {
        Session {
            id: id.to_string(),
            content_session_id: format!("content-{}", id),
            memory_session_id: Some(format!("memory-{}", id)),
            project: "test-project".to_string(),
            user_prompt: Some("Test prompt".to_string()),
            started_at: Utc::now(),
            ended_at: None,
            status: SessionStatus::Active,
            prompt_counter: 0,
        }
    }

    #[test]
    fn test_storage_new() {
        let (storage, _temp_dir) = create_test_storage();
        let stats = storage.get_stats().unwrap();
        assert_eq!(stats.observation_count, 0);
        assert_eq!(stats.session_count, 0);
    }

    #[test]
    fn test_save_and_get_observation() {
        let (storage, _temp_dir) = create_test_storage();
        let obs = create_test_observation("obs-1", "test-project");

        storage.save_observation(&obs).unwrap();

        let retrieved = storage.get_by_id("obs-1").unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, "obs-1");
        assert_eq!(retrieved.title, "Test observation obs-1");
    }

    #[test]
    fn test_save_and_get_session() {
        let (storage, _temp_dir) = create_test_storage();
        let session = create_test_session("session-1");

        storage.save_session(&session).unwrap();

        let retrieved = storage.get_session("session-1").unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, "session-1");
        assert_eq!(retrieved.status, SessionStatus::Active);
    }

    #[test]
    fn test_update_session_status() {
        let (storage, _temp_dir) = create_test_storage();
        let session = create_test_session("session-2");
        storage.save_session(&session).unwrap();

        storage
            .update_session_status("session-2", SessionStatus::Completed)
            .unwrap();

        let retrieved = storage.get_session("session-2").unwrap().unwrap();
        assert_eq!(retrieved.status, SessionStatus::Completed);
        assert!(retrieved.ended_at.is_some());
    }

    #[test]
    fn test_get_recent() {
        let (storage, _temp_dir) = create_test_storage();

        for i in 1..=5 {
            let obs = create_test_observation(&format!("obs-{}", i), "test-project");
            storage.save_observation(&obs).unwrap();
        }

        let recent = storage.get_recent(3).unwrap();
        assert_eq!(recent.len(), 3);
    }

    #[test]
    fn test_get_all_projects() {
        let (storage, _temp_dir) = create_test_storage();

        storage
            .save_observation(&create_test_observation("obs-1", "project-a"))
            .unwrap();
        storage
            .save_observation(&create_test_observation("obs-2", "project-b"))
            .unwrap();
        storage
            .save_observation(&create_test_observation("obs-3", "project-a"))
            .unwrap();

        let projects = storage.get_all_projects().unwrap();
        assert_eq!(projects.len(), 2);
        assert!(projects.contains(&"project-a".to_string()));
        assert!(projects.contains(&"project-b".to_string()));
    }

    #[test]
    fn test_get_stats() {
        let (storage, _temp_dir) = create_test_storage();

        storage
            .save_observation(&create_test_observation("obs-1", "project-a"))
            .unwrap();
        storage
            .save_session(&create_test_session("session-1"))
            .unwrap();

        let stats = storage.get_stats().unwrap();
        assert_eq!(stats.observation_count, 1);
        assert_eq!(stats.session_count, 1);
        assert_eq!(stats.project_count, 1);
    }

    #[test]
    fn test_get_observations_paginated() {
        let (storage, _temp_dir) = create_test_storage();

        for i in 1..=10 {
            let obs = create_test_observation(&format!("obs-{}", i), "test-project");
            storage.save_observation(&obs).unwrap();
        }

        let page1 = storage.get_observations_paginated(0, 5, None).unwrap();
        assert_eq!(page1.items.len(), 5);
        assert_eq!(page1.total, 10);
        assert_eq!(page1.offset, 0);
        assert_eq!(page1.limit, 5);

        let page2 = storage.get_observations_paginated(5, 5, None).unwrap();
        assert_eq!(page2.items.len(), 5);
        assert_eq!(page2.offset, 5);
    }

    #[test]
    fn test_get_observations_paginated_with_project_filter() {
        let (storage, _temp_dir) = create_test_storage();

        for i in 1..=5 {
            storage
                .save_observation(&create_test_observation(
                    &format!("obs-a-{}", i),
                    "project-a",
                ))
                .unwrap();
        }
        for i in 1..=3 {
            storage
                .save_observation(&create_test_observation(
                    &format!("obs-b-{}", i),
                    "project-b",
                ))
                .unwrap();
        }

        let result = storage
            .get_observations_paginated(0, 10, Some("project-a"))
            .unwrap();
        assert_eq!(result.total, 5);
        assert_eq!(result.items.len(), 5);
    }

    #[test]
    fn test_search_with_filters() {
        let (storage, _temp_dir) = create_test_storage();

        storage
            .save_observation(&create_test_observation("obs-1", "project-a"))
            .unwrap();
        storage
            .save_observation(&create_test_observation("obs-2", "project-b"))
            .unwrap();

        let results = storage
            .search_with_filters(None, Some("project-a"), None, 10)
            .unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_get_context_for_project() {
        let (storage, _temp_dir) = create_test_storage();

        for i in 1..=5 {
            storage
                .save_observation(&create_test_observation(
                    &format!("obs-{}", i),
                    "my-project",
                ))
                .unwrap();
        }

        let context = storage.get_context_for_project("my-project", 3).unwrap();
        assert_eq!(context.len(), 3);
    }

    #[test]
    fn test_search_by_file() {
        let (storage, _temp_dir) = create_test_storage();

        storage
            .save_observation(&create_test_observation("obs-1", "project-a"))
            .unwrap();

        let results = storage.search_by_file("file1.rs", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "obs-1");

        let no_results = storage.search_by_file("nonexistent.rs", 10).unwrap();
        assert!(no_results.is_empty());
    }

    #[test]
    fn test_search_sessions_empty_query() {
        let (storage, _temp_dir) = create_test_storage();

        let results = storage.search_sessions("", 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_prompts_empty_query() {
        let (storage, _temp_dir) = create_test_storage();

        let results = storage.search_prompts("", 10).unwrap();
        assert!(results.is_empty());
    }
}
