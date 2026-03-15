#[cfg(test)]
mod tests {
    use opencode_mem_core::{GlobalKnowledge, KnowledgeType};
    use crate::handlers::context::select_relevant_knowledge;

    fn sample_knowledge_full(
        id: &str,
        title: &str,
        source_projects: Vec<&str>,
        usage_count: i64,
        confidence: f64,
        created_at: &str,
    ) -> GlobalKnowledge {
        GlobalKnowledge::new(
            id.to_owned(),
            KnowledgeType::Pattern,
            title.to_owned(),
            "description".to_owned(),
            None,
            vec![],
            source_projects.into_iter().map(str::to_owned).collect(),
            vec![],
            confidence,
            usage_count,
            None,
            created_at.to_owned(),
            created_at.to_owned(),
            None,
        )
    }

    #[test]
    fn breaker_test_limit_exceeded() {
        let mut entries = Vec::new();
        for i in 0..5 {
            entries.push(sample_knowledge_full(&format!("proj-{}", i), "title", vec!["demo"], 0, 0.5, "2026-01-01T00:00:00Z"));
            entries.push(sample_knowledge_full(&format!("recent-{}", i), "title", vec![], 0, 0.5, "2026-01-02T00:00:00Z"));
            entries.push(sample_knowledge_full(&format!("proven-{}", i), "title", vec![], 10, 0.5, "2026-01-01T00:00:00Z"));
        }

        // limit = 1
        let selected = select_relevant_knowledge(entries, "demo", 1);
        assert!(selected.len() <= 1, "Expected <= 1, got {}", selected.len());
    }
}
