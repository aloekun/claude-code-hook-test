#!/usr/bin/env bash
set -euo pipefail

input="$(cat)"
file="$(jq -r '.tool_input.file_path // .tool_input.path // empty' <<< "$input")"

case "$file" in
  *.py) ;;
  *) exit 0 ;;
esac

ruff format "$file" >/dev/null 2>&1 || true
ruff check --fix "$file" >/dev/null 2>&1 || true
diag="$(ruff check "$file" 2>&1 | head -20 || true)"

if [ -n "$diag" ]; then
  jq -Rn --arg msg "$diag" '{
    hookSpecificOutput: {
      hookEventName: "PostToolUse",
      additionalContext: $msg
    }
  }'
fi
