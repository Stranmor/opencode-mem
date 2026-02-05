use std::fmt::Write as _;

use opencode_mem_core::Observation;

/// Builder for constructing context strings from observations.
#[derive(Debug)]
pub struct ContextBuilder {
    /// Observations to include in the context.
    observations: Vec<Observation>,
    /// Number of observations to show in full detail.
    full_count: usize,
    /// Number of observations to show as index references.
    index_count: usize,
}

impl Default for ContextBuilder {
    fn default() -> Self {
        return Self::new();
    }
}

impl ContextBuilder {
    /// Creates a new context builder with default settings.
    #[must_use]
    pub const fn new() -> Self {
        return Self { observations: Vec::new(), full_count: 5, index_count: 50 };
    }

    /// Sets the observations to include in the context.
    #[must_use]
    pub fn with_observations(mut self, observations: Vec<Observation>) -> Self {
        self.observations = observations;
        return self;
    }

    /// Sets the number of observations to show in full detail.
    #[must_use]
    pub const fn with_full_count(mut self, count: usize) -> Self {
        self.full_count = count;
        return self;
    }

    /// Sets the number of observations to show as index references.
    #[must_use]
    pub const fn with_index_count(mut self, count: usize) -> Self {
        self.index_count = count;
        return self;
    }

    /// Builds the context string.
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
        let index_titles: Vec<_> = index_obs.map(|obs| format!("#{}", obs.id)).collect();

        if !index_titles.is_empty() {
            #[expect(clippy::let_underscore_must_use, reason = "String::write is infallible")]
            let _ = writeln!(output, "\nAdditional observations: {}", index_titles.join(", "));
        }

        output.push_str(
            "\nUse these memories for context about this project's patterns and past decisions.\n",
        );
        return output;
    }
}

/// Formats observations for injection into agent context.
#[must_use]
pub fn format_context_for_injection(observations: Vec<Observation>) -> String {
    return ContextBuilder::new().with_observations(observations).build();
}
