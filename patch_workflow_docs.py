import re

with open('crates/mcp/src/tools.rs', 'r') as f:
    text = f.read()

# Replace \u{2192} with actual character '→' to avoid any raw string issues
text = text.replace(r'\u{2192}', '→')

with open('crates/mcp/src/tools.rs', 'w') as f:
    f.write(text)
