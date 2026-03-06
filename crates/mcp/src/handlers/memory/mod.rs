mod read_handlers;
mod write_handlers;

pub(super) use read_handlers::*;
pub(super) use write_handlers::*;

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code")]
#[expect(clippy::indexing_slicing, reason = "test code — asserts guard length")]
#[path = "../memory_tests.rs"]
mod tests;
