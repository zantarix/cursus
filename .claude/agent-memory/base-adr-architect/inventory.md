# ADR Inventory

| ADR # | File name | Title | Status |
|-------|-----------|-------|--------|
| 000 | 000-founding-constraints.md | Founding Constraints and Initial Choices | Accepted |
| 001 | 001-project-initialisation.md | Project Initialisation | Accepted |
| 002 | 002-changeset-recording.md | Changeset Recording | Accepted |
| 003 | 003-release-command.md | Release Command | Accepted |
| 004 | 004-publish-command.md | Publish Command | Accepted |
| 005 | 005-github-releases.md | GitHub Releases | Accepted |
| 006 | 006-git-lifecycle-hooks.md | Git Lifecycle Hooks | Accepted |
| 007 | 007-honor-private-packages-during-publish.md | Honor Private Package Markers During Publish | Accepted |
| 008 | 008-dry-run-local-only-guarantee.md | Dry-Run Must Be Strictly Local-Only | Accepted |
| 009 | 009-javascript-package-manager-strategy.md | JavaScript Package Manager Strategy for Lockfiles and Publishing | Accepted |
| 010 | 010-scoped-release-changeset-consumption.md | Scoped Release Changeset Consumption | Accepted |
| 011 | 011-command-execution-strategy.md | Command Execution Strategy | Accepted |
| 012 | 012-workspace-protocol-dependency-updates.md | Skip `workspace:` Protocol Dependencies During Intra-Workspace Version Propagation | Accepted |
| 013 | 013-logging-infrastructure.md | Adopt the `log` Crate with `fern` for Application Logging | Accepted (backend superseded by ADR-018) |
| 014 | 014-verbose-mode.md | Add Verbose and Silent Modes via Global CLI Flags | Accepted |
| 015 | 015-ci-managed-release-workflow.md | CI-Managed Release Workflow | Accepted |
| 016 | 016-rename-release-to-prepare.md | Rename `release` Subcommand to `prepare` | Accepted |
| 017 | 017-late-guard-dry-run-pattern.md | Adopt Late Guard Pattern for Dry-Run Implementation | Accepted |
| 018 | 018-replace-fern-with-cli-logger.md | Replace `fern` with Hand-Rolled `log::Log` Implementation | Accepted |
| 019 | 019-improved-init-workflow.md | Improve Init Workflow | Accepted |
| 020 | 020-tui-screen-submodule-structure.md | Structure TUI Wizards as Submodule Directories with One File per Screen | Accepted |
| 021 | 021-commit-references-in-changelog-entries.md | Add Commit References to Changelog Entries | Accepted |
| 022 | 022-distribution-strategy.md | Distribution Strategy for Cursus Binaries | Accepted |
| 023 | 023-dependency-propagation-bumps.md | Dependency Propagation Bumps During Prepare | Accepted |
| 024 | 024-linked-package-versions.md | Linked Package Versions in Monorepos | Accepted |
| 025 | 025-auto-changeset-from-conventional-commit.md | Add `--auto` Flag to Derive Changesets from Conventional Commits | Accepted |
| 026 | 026-per-package-change-level-in-tui.md | Per-Package Change Level Selection in TUI Wizard | Accepted |
| 027 | 027-mutation-testing-approach.md | Adopt Mutation Testing as a Test Quality Verification Strategy | Accepted |
| 028 | 028-npm-oidc-trusted-publishing.md | Support npm OIDC Trusted Publishing | Accepted |
| 029 | 029-cargo-publish-authentication-warning.md | Warn on Missing Cargo Registry Token Before Publish | Accepted (warning behaviour superseded by ADR-045) |
| 030 | 030-bin-lib-crate-separation.md | Separate Binary and Library Crates with Environment Injection | Accepted |
| 031 | 031-changelog-guard-for-unprepared-packages.md | Guard Publish and CI Against Never-Prepared Packages Using `CHANGELOG.md` | Accepted |
| 032 | 032-verify-changeset-on-branch.md | Verify Changeset Presence on Feature Branches | Accepted |
| 033 | 033-windows-shell-execution.md | Extend Command Execution to Support Windows | Accepted |
| 034 | 034-compile-time-embedded-localisation.md | Use `fluent-templates` for Compile-Time Embedded Localisation | Accepted |
| 035 | 035-git-trait-abstraction.md | Introduce a Git Trait for Abstracting All Git Operations | Accepted |
| 036 | 036-filesystem-trait-abstraction.md | Introduce Filesystem Trait for File I/O Abstraction | Accepted |
| 037 | 037-async-library-with-tokio-runtime.md | Make the Library Crate Async with Tokio Runtime | Accepted |
| 038 | 038-octocrab-github-client.md | Replace `ureq`-Based `RestGitHubClient` with Shared Octocrab Implementation | Accepted |
| 039 | 039-split-dependency-versioning-strategy.md | Split Dependency Versioning Strategy Between Library and Binary Crates | Accepted |
| 040 | 040-strip-git-trailers-from-automatic-changesets.md | Strip Git Trailers from Conventional Commit Body During Parsing | Accepted |
| 041 | 041-rename-github-client-trait-to-code-forge-client.md | Rename `GitHubClient` Trait to `CodeForgeClient` | Accepted |
| 042 | 042-repo-identity-in-constructor.md | Move Repo Identity into `CodeForgeClient` Constructor | Accepted |
| 043 | 043-publish-private-packages-to-github-releases.md | Allow Private Packages to Publish via Git Tags and GitHub Releases | Accepted |
| 044 | 044-per-package-github-release-artifacts.md | Restructure GitHub Artifact Configuration to Per-Package Sections | Accepted |
| 045 | 045-crates-io-trusted-publishing.md | Support crates.io OIDC Trusted Publishing | Accepted |
| 046 | 046-streaming-command-execution.md | Stream Output of User-Configurable Shell Commands | Accepted |
| 047 | 047-configurable-release-target-branch.md | Configurable Release Target Branch | Deprecated (2026-04-28) |
| 048 | 048-native-windows-build-runner.md | Build Windows Artifacts Natively with MSVC and Static CRT | Accepted (2026-04-26) |
| 049 | 049-signed-release-artifacts.md | Sign GitHub Release Artifacts and Verify in npm Postinstall | Accepted (2026-04-27) |
| 050 | 050-verified-release-commits-via-git-data-api.md | Produce Verified Release Commits via the GitHub Git Data API | Accepted (2026-04-29) |
| 051 | 051-bundle-sigstore-deps-via-workspace-removal.md | Bundle Sigstore Transitive Dependencies into the npm Tarball by Removing the Workspace Declaration | Accepted (2026-05-01) |
| 052 | 052-credential-redaction-in-error-messages.md | Redact Credentials from Subprocess and API Error Messages | Accepted (2026-05-01) |
| 053 | 053-npm-package-node-spawner.md | Use a Node.js Spawner Script for the npm Package Binary Entry Point | Accepted (2026-05-02) |
| 054 | 054-cargo-binstall-support.md | Add cargo-binstall Support for Prebuilt Binary Installation | Accepted (2026-05-02) |
| 055 | 055-end-to-end-idempotent-publish-recovery.md | End-to-End Idempotent Publish Recovery | Accepted (2026-05-03) |
| 056 | 056-gitlab-support-client-config-and-ci.md | GitLab Support — Client, Config, and CI Integration | Proposed (2026-05-12) |
| 057 | 057-cursus-init-gitlab-support.md | `cursus init` GitLab Support | Proposed (2026-05-12) |
| 058 | 058-verified-release-commits-on-gitlab-via-web-commits-api.md | Produce Verified Release Commits on GitLab via the Web Commits API | Proposed (2026-05-12) |
| 059 | 059-forge-selection-runtime-rules.md | Forge Selection Runtime Rules | Proposed (2026-05-12) |
