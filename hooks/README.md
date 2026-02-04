# IDE/Editor Hooks for opencode-mem

Example shell scripts for integrating opencode-mem with IDEs and editors.

## Available Hooks

| Hook | Trigger | Purpose |
|------|---------|---------|
| `on-session-start.sh` | IDE opens project | Fetch recent memories for context |
| `on-file-save.sh` | File saved | Track file changes |
| `on-session-end.sh` | IDE closes | Log session summary |

## Installation

```bash
chmod +x hooks/*.sh
```

## Environment Variables

All hooks accept these environment variables:

| Variable | Description |
|----------|-------------|
| `SESSION_ID` | Unique session identifier |
| `PROJECT_PATH` | Project root directory |
| `OPENCODE_MEM_BIN` | Path to opencode-mem binary (default: `opencode-mem`) |

### Hook-specific Variables

**on-file-save.sh:**
- `FILE_PATH` - Absolute path to saved file

**on-session-end.sh:**
- `DURATION` - Session duration in seconds

## IDE Integration Examples

### Cursor/VS Code

Add to `.vscode/settings.json`:

```json
{
  "files.saveDelay": 0,
  "runOnSave.commands": [
    {
      "match": ".*",
      "command": "./hooks/on-file-save.sh ${file}"
    }
  ]
}
```

### Neovim

```lua
vim.api.nvim_create_autocmd("VimEnter", {
  callback = function()
    vim.fn.system("./hooks/on-session-start.sh")
  end
})

vim.api.nvim_create_autocmd("BufWritePost", {
  callback = function()
    local file = vim.fn.expand("%:p")
    vim.fn.system("./hooks/on-file-save.sh " .. file)
  end
})

vim.api.nvim_create_autocmd("VimLeave", {
  callback = function()
    vim.fn.system("./hooks/on-session-end.sh")
  end
})
```

### Emacs

```elisp
(add-hook 'after-init-hook
  (lambda () (shell-command "./hooks/on-session-start.sh")))

(add-hook 'after-save-hook
  (lambda () (shell-command (concat "./hooks/on-file-save.sh " buffer-file-name))))

(add-hook 'kill-emacs-hook
  (lambda () (shell-command "./hooks/on-session-end.sh")))
```

## HTTP API Alternative

For programmatic integration, use the HTTP API directly:

```bash
# Start server
opencode-mem serve --port 37777

# Create observation
curl -X POST http://localhost:37777/api/observations \
  -H "Content-Type: application/json" \
  -d '{"title": "Session started", "observation_type": "session"}'

# Search memories
curl "http://localhost:37777/api/search?q=your+query"
```

## Customization

Copy and modify these scripts for your workflow. The scripts are intentionally minimal — extend them based on your needs.
