#!/usr/bin/env bash
# Hook: on-session-start
# Called when a new IDE/editor session starts
# 
# Environment variables (set by IDE):
#   SESSION_ID    - Unique session identifier
#   PROJECT_PATH  - Path to the project root
#   EDITOR        - Editor name (cursor, vscode, etc.)
#
# Usage: ./on-session-start.sh [session_id] [project_path]

set -euo pipefail

SESSION_ID="${1:-${SESSION_ID:-$(uuidgen 2>/dev/null || date +%s)}}"
PROJECT_PATH="${2:-${PROJECT_PATH:-$(pwd)}}"
EDITOR="${EDITOR:-unknown}"

# Get the opencode-mem binary path
OPENCODE_MEM="${OPENCODE_MEM_BIN:-opencode-mem}"

# Log session start (search for recent context)
echo "[opencode-mem] Session started: $SESSION_ID"
echo "[opencode-mem] Project: $PROJECT_PATH"
echo "[opencode-mem] Editor: $EDITOR"

# Fetch recent observations for this project to prime context
if command -v "$OPENCODE_MEM" &>/dev/null; then
    echo "[opencode-mem] Fetching recent memories..."
    "$OPENCODE_MEM" recent --limit 5 2>/dev/null || true
else
    echo "[opencode-mem] Warning: opencode-mem binary not found"
    echo "[opencode-mem] Install with: cargo install --path crates/cli"
fi
