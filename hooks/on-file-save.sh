#!/usr/bin/env bash
# Hook: on-file-save
# Called when a file is saved in the IDE/editor
#
# Environment variables (set by IDE):
#   FILE_PATH     - Absolute path to the saved file
#   PROJECT_PATH  - Path to the project root
#   SESSION_ID    - Current session identifier
#
# Usage: ./on-file-save.sh <file_path> [project_path]

set -euo pipefail

FILE_PATH="${1:-${FILE_PATH:-}}"
PROJECT_PATH="${2:-${PROJECT_PATH:-$(pwd)}}"
SESSION_ID="${SESSION_ID:-}"

if [[ -z "$FILE_PATH" ]]; then
    echo "[opencode-mem] Error: FILE_PATH required" >&2
    exit 1
fi

OPENCODE_MEM="${OPENCODE_MEM_BIN:-opencode-mem}"

FILENAME=$(basename "$FILE_PATH")
EXTENSION="${FILENAME##*.}"

echo "[opencode-mem] File saved: $FILE_PATH"

case "$EXTENSION" in
    rs|py|ts|js|go|c|cpp|h|hpp)
        echo "[opencode-mem] Code file detected: $EXTENSION"
        ;;
    md|txt|rst)
        echo "[opencode-mem] Documentation file detected"
        ;;
    *)
        echo "[opencode-mem] Other file type: $EXTENSION"
        ;;
esac
