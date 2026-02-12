//! Environment variable parsing with warn-level logging for invalid values.

/// Parse an environment variable with a default fallback.
///
/// - If the variable is not set: returns `default` silently (expected case).
/// - If the variable is set but cannot be parsed: logs a warning and returns `default`.
///
/// This replaces the pattern `env::var("X").ok().and_then(|v| v.parse().ok()).unwrap_or(default)`
/// which silently swallows parse failures.
pub fn env_parse_with_default<T: std::str::FromStr + std::fmt::Display>(
    var: &str,
    default: T,
) -> T {
    match std::env::var(var) {
        Ok(v) => match v.parse() {
            Ok(n) => n,
            Err(_) => {
                tracing::warn!(
                    var,
                    value = %v,
                    default = %default,
                    "invalid env var value, using default"
                );
                default
            },
        },
        Err(_) => default,
    }
}
