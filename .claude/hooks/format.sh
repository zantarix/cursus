#!/bin/bash

# Read hook input from stdin
INPUT=$(cat)

# Extract file path from the tool input
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty')

# Exit if no file path (shouldn't happen with Write/Edit)
if [ -z "$FILE_PATH" ]; then
  exit 0
fi

# Format based on file extension
if [[ "$FILE_PATH" == *.rs ]]; then
  cargo fmt -- "$FILE_PATH" 2>&1
elif [[ "$FILE_PATH" == *.md ]]; then
  markdownlint --fix "$FILE_PATH" 2>&1
fi

exit 0
