import re

with open('crates/mcp/src/handlers/infinite.rs', 'r') as f:
    text = f.read()

old_block = """            let from = args.get("from").and_then(|f| f.as_str()).unwrap_or("");
            let to = args.get("to").and_then(|t| t.as_str()).unwrap_or("");"""

new_block = """            let from = args.get("start").and_then(|f| f.as_str()).unwrap_or("");
            let to = args.get("end").and_then(|t| t.as_str()).unwrap_or("");"""

text = text.replace(old_block, new_block)

with open('crates/mcp/src/handlers/infinite.rs', 'w') as f:
    f.write(text)
