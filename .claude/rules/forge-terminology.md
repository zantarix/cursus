# Forge-specific terminology

User-facing surfaces use each forge's native vocabulary. Internal code stays forge-neutral.

## User-facing (use forge vocabulary)

- **Config keys (TOML):** GitLab uses `group`/`project`, not `owner`/`repo`.
- **Locale strings (`.ftl`):** Phrase prompts in forge terms — "GitLab project (group/project, e.g. acme/my-app)".
- **Error messages:** Say "merge request" when the active forge is GitLab, "pull request" for GitHub.
- **Docs:** Each forge's page uses its own glossary, not a translation of the GitHub page.

## Internal code (forge-neutral)

`CodeForgeClient` trait method names and shared types (`create_pull_request`, `head`/`base`) must not be renamed per-forge. GitLab implementations translate only at the API boundary (`head`→`source_branch`, `base`→`target_branch`).
