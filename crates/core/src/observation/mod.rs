//! Observation types for coding session capture.

mod builder;
mod content_filter;
mod input;
mod low_value_filter;
mod observation_type;

mod dedup;
#[cfg(test)]
mod dedup_tests;
mod merge;

pub use builder::*;
pub use content_filter::*;
pub use dedup::*;
pub use input::*;
pub use low_value_filter::{is_low_value_observation, is_trivial_tool_call};
pub use merge::*;
pub use observation_type::*;

use std::fmt;

use serde::{Deserialize, Serialize};

/// Ordinal position of a prompt within a session.
///
/// Semantically distinct from token counts or other numeric identifiers —
/// wrapping in a newtype prevents accidental swaps at construction sites.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PromptNumber(pub u32);

/// Token count for ROI (return on investment) tracking.
///
/// Semantically distinct from prompt ordinals or other numeric fields —
/// wrapping in a newtype prevents accidental swaps at construction sites.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DiscoveryTokens(pub u32);

impl From<u32> for PromptNumber {
    fn from(v: u32) -> Self {
        Self(v)
    }
}

impl PromptNumber {
    /// Convert to PostgreSQL `i32` safely, rejecting overflows.
    pub fn as_pg_i32(&self) -> Result<i32, &'static str> {
        i32::try_from(self.0).map_err(|_| "PromptNumber exceeds PostgreSQL i32 capacity")
    }
}

impl From<PromptNumber> for u32 {
    fn from(v: PromptNumber) -> Self {
        v.0
    }
}

impl fmt::Display for PromptNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl From<u32> for DiscoveryTokens {
    fn from(v: u32) -> Self {
        Self(v)
    }
}

impl DiscoveryTokens {
    /// Convert to PostgreSQL `i32` safely, rejecting overflows.
    pub fn as_pg_i32(&self) -> Result<i32, &'static str> {
        i32::try_from(self.0).map_err(|_| "DiscoveryTokens exceeds PostgreSQL i32 capacity")
    }
}

impl From<DiscoveryTokens> for u32 {
    fn from(v: DiscoveryTokens) -> Self {
        v.0
    }
}

impl fmt::Display for DiscoveryTokens {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
