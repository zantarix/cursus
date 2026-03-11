---
name: commit
description: Use to verify and commit staged or unstaged changes with a well-crafted commit message.
---

Commit the current changes following this project's standards. Perform the steps below in order, stopping and reporting any failures.

1. **Verify** the code quality using the `verify-code` skill, if any files in the `src/` or `tests/` folder have changed. Fix any failures before proceeding.

2. **Stage** changes: prefer adding specific files by name rather than `git add -A`. Never stage secrets, credentials, or large binaries.

3. **Draft** a commit message:
   - Follow Conventional Commit style for the message formatting. You can find a copy of this style guide in `commits.md`.
   - Run `git log --oneline -10` to match the existing commit message style.
   - Summarise the *why*, not the *what*.
   - Use the imperative mood in the subject line (e.g. "Add", "Fix", "Refactor").
   - Keep the subject line under 72 characters.
   - If the change is non-trivial, add a blank line followed by a short body.

4. **Commit** using a HEREDOC to preserve formatting:

   ```bash
   git commit -m "$(cat <<'EOF'
   <subject line>

   <optional body>

   Co-Authored-By: <description of current model>
   EOF
   )"
   ```

5. **Confirm** success with `git status` and report the commit hash and subject to the user.

**Safety rules (never violate):**

- Never amend an existing commit unless the user explicitly asks.
- Never force-push.
- Never skip hooks (`--no-verify`).
- Never push to the remote unless the user explicitly asks.
