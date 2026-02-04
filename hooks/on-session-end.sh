#!/usr/bin/env bash
# Hook: on-session-end
# Called when an IDE/editor session ends
#
# Environment variables (set by IDE):
#   SESSION_ID    - Session identifier being closed
#   PROJECT_PATH  - Path to the project root
#   DURATION      - Session duration in seconds (optional)
#
# Usage: ./on-session-end.sh [session_id] [project_path]

set -euo pipefail

SESSION_ID="${1:-${SESSION_ID:-unknown}}"
PROJECT_PATH="${2:-${PROJECT_PATH:-$(pwd)}}"
DURATION="${DURATION:-}"

OPENCODE_MEM="${OPENCODE_MEM_BIN:-opencode-mem}"

echo "[opencode-mem] Session ended: $SESSION_ID"
echo "[opencode-mem] Project: $PROJECT_PATH"

if [[ -n "$DURATION" ]]; then
    echo "[opencode-mem] Duration: ${DURATION}s"
fi

if command -v "$OPENCODE_MEM" &>/dev/null; then
    echo "[opencode-mem] Session summary available via: $OPENCODE_MEM stats"
else
    echo "[opencode-mem] Warning: opencode-mem binary not found"
fi
