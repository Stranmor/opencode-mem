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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_parse_valid_value() {
        let var_name = "TEST_ENV_PARSE_VALID_98271";
        std::env::set_var(var_name, "42");
        let result: u32 = env_parse_with_default(var_name, 10);
        assert_eq!(result, 42);
        std::env::remove_var(var_name);
    }

    #[test]
    fn test_env_parse_invalid_value() {
        let var_name = "TEST_ENV_PARSE_INVALID_98272";
        std::env::set_var(var_name, "banana");
        let result: u32 = env_parse_with_default(var_name, 10);
        assert_eq!(result, 10);
        std::env::remove_var(var_name);
    }

    #[test]
    fn test_env_parse_missing_var() {
        let var_name = "TEST_ENV_PARSE_MISSING_98273";
        std::env::remove_var(var_name);
        let result: u32 = env_parse_with_default(var_name, 10);
        assert_eq!(result, 10);
    }

    #[test]
    fn test_env_parse_empty_value() {
        let var_name = "TEST_ENV_PARSE_EMPTY_98274";
        std::env::set_var(var_name, "");
        let result: u32 = env_parse_with_default(var_name, 10);
        assert_eq!(result, 10);
        std::env::remove_var(var_name);
    }
}
