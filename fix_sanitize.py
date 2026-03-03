with open('crates/service/src/observation_service/side_effects.rs', 'r') as f:
    text = f.read()

old_block = """            let filtered_input = {
                let input_str = serde_json::to_string(&tool_call.input).unwrap_or_default();
                let filtered = sanitize_input(&input_str);
                serde_json::from_str(&filtered).unwrap_or_else(|e| {
                    tracing::warn!(
                        error = %e,
                        "Privacy/injection filter corrupted JSON input in infinite memory — using Null instead of unfiltered fallback"
                    );
                    serde_json::Value::Null
                })
            };"""

new_block = """            let filtered_input = opencode_mem_core::sanitize_json_values(&tool_call.input);"""

text = text.replace(old_block, new_block)

with open('crates/service/src/observation_service/side_effects.rs', 'w') as f:
    f.write(text)

with open('crates/service/src/observation_service/mod.rs', 'r') as f:
    text = f.read()

old_mod = """        let filtered_input = {
            let input_str = serde_json::to_string(&tool_call.input).unwrap_or_default();
            let filtered = sanitize_input(&input_str);
            serde_json::from_str(&filtered).unwrap_or_else(|e| {
                tracing::warn!(
                    error = %e,
                    "Privacy/injection filter corrupted JSON input — using Null instead of unfiltered fallback"
                );
                serde_json::Value::Null
    )
;"""

new_mod = """        let filtered_input = opencode_mem_core::sanitize_json_values(&tool_call.input);"""

text = text.replace(old_mod, new_mod)

with open('crates/service/src/observation_service/mod.rs', 'w') as f:
    f.write(text)
