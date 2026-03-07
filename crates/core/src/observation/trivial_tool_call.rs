pub fn is_trivial_tool_call(tool_name: &str, input: &serde_json::Value) -> bool {
    let t = tool_name.to_lowercase();

    // Pure read/discovery operations (file system)
    if t == "read" || t == "grep" || t == "glob" || t == "ast_grep_search" || t == "look_at" {
        return true;
    }

    // Pure metadata/status (explicitly negligible)
    if t == "todowrite" || t.starts_with("session_") || t.starts_with("memory_") {
        return true;
    }

    // LSP analysis tools
    if t.starts_with("lsp_") && t != "lsp_rename" {
        return true;
    }

    // Read-only web/browser ops
    if t == "webfetch"
        || t == "playwright_browser_take_screenshot"
        || t == "playwright_browser_snapshot"
    {
        return true;
    }

    // Bash read-only or negligible commands
    if t == "bash" {
        if let Some(cmd) = input.get("command").and_then(|c| c.as_str()) {
            let cmd_lower = cmd.to_lowercase();
            let trimmed = cmd_lower.trim();

            // Shell metacharacters that allow command chaining or redirection
            let has_metachars = cmd.contains([';', '&', '|', '<', '>', '\n', '$', '`']);

            if !has_metachars
                && (trimmed == "ls"
                    || trimmed.starts_with("ls ")
                    || trimmed == "pwd"
                    || trimmed.starts_with("cat ")
                    || trimmed.starts_with("echo ")
                    || trimmed.starts_with("grep ")
                    || trimmed == "git status"
                    || trimmed.starts_with("git status ")
                    || trimmed == "git log"
                    || trimmed.starts_with("git log ")
                    || trimmed == "git diff"
                    || trimmed.starts_with("git diff ")
                    || trimmed == "cargo check"
                    || trimmed.starts_with("cargo check ")
                    || trimmed == "npm test"
                    || trimmed.starts_with("npm test ")
                    || trimmed == "npm run test"
                    || trimmed.starts_with("npm run test ")
                    || trimmed == "pytest"
                    || trimmed.starts_with("pytest "))
            {
                return true;
            }
        }
    }

    false
}
