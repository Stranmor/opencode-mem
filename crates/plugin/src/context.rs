use std::fmt::Write as _;

use opencode_mem_core::Observation;

#[derive(Debug)]
pub struct ContextBuilder {
    observations: Vec<Observation>,
    full_count: usize,
    index_count: usize,
}

impl Default for ContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextBuilder {
    #[must_use]
    pub const fn new() -> Self {
        Self { observations: Vec::new(), full_count: 5, index_count: 50 }
    }

    #[must_use]
    pub fn with_observations(mut self, observations: Vec<Observation>) -> Self {
        self.observations = observations;
        self
    }

    #[must_use]
    pub const fn with_full_count(mut self, count: usize) -> Self {
        self.full_count = count;
        self
    }

    #[must_use]
    pub const fn with_index_count(mut self, count: usize) -> Self {
        self.index_count = count;
        self
    }

    #[must_use]
    pub fn build(&self) -> String {
        if self.observations.is_empty() {
            return String::new();
        }

        let mut output = String::new();
        output.push_str("## Relevant memories from past sessions\n\n");

        let full_obs = self.observations.iter().take(self.full_count);
        for obs in full_obs {
            #[expect(clippy::let_underscore_must_use, reason = "String::write is infallible")]
            let _ = writeln!(
                output,
                "- [{}] {}: {}",
                obs.observation_type.as_str(),
                obs.title,
                obs.narrative.as_deref().unwrap_or("")
            );
        }

        let index_obs = self.observations.iter().skip(self.full_count).take(self.index_count);
        let index_titles: Vec<_> = index_obs.map(|o| format!("#{}", o.id)).collect();

        if !index_titles.is_empty() {
            #[expect(clippy::let_underscore_must_use, reason = "String::write is infallible")]
            let _ = writeln!(output, "\nAdditional observations: {}", index_titles.join(", "));
        }

        output.push_str(
            "\nUse these memories for context about this project's patterns and past decisions.\n",
        );
        output
    }
}

#[must_use]
pub fn format_context_for_injection(observations: Vec<Observation>) -> String {
    ContextBuilder::new().with_observations(observations).build()
}
