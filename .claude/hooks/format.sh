#!/usr/bin/env bash

# Read hook input from stdin
INPUT=$(cat)

# Extract file path and project directory from the tool input
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty')
PROJECT_DIR=$(echo "$INPUT" | jq -r '.cwd // empty')

# Exit if no file path (shouldn't happen with Write/Edit)
if [ -z "$FILE_PATH" ]; then
  exit 0
fi

# Exit if no project directory
if [ -z "$PROJECT_DIR" ]; then
  exit 0
fi

# Only format files within the project directory
if [[ "$FILE_PATH" != "$PROJECT_DIR"* ]]; then
  exit 0
fi

# Format based on file extension
if [[ "$FILE_PATH" == *.rs ]]; then
  cargo fmt -- "$FILE_PATH" 2>&1 || true
elif [[ "$FILE_PATH" == *.md ]]; then
  markdownlint --fix "$FILE_PATH" 2>&1 || true
fi

exit 0
