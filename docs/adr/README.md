# Architecture Decision Records

This directory contains the Architecture Decision Records (ADRs) for the Cursus project.

ADRs are short documents that capture significant architectural decisions made during the development of this project. Each record describes the context behind a decision, the decision itself, the alternatives that were considered, and the consequences — both positive and negative. They serve as a historical log for current and future contributors to understand why the system is shaped the way it is.

Once an ADR is accepted and committed, it is treated as immutable. If a decision is later reversed or revised, a new ADR is created and the original's status is updated to reflect the change.

## Index

| ADR | Title | Status |
|-----|-------|--------|
| [ADR-000](000-founding-constraints.md) | Founding Constraints and Initial Choices | Accepted |
| [ADR-001](001-project-initialisation.md) | Project Initialisation | Accepted |
| [ADR-002](002-changeset-recording.md) | Changeset Recording | Accepted |
| [ADR-003](003-release-command.md) | Release Command | Accepted |
| [ADR-004](004-publish-command.md) | Publish Command | Accepted |
| [ADR-005](005-github-releases.md) | GitHub Releases Integration | Accepted |
| [ADR-006](006-git-lifecycle-hooks.md) | Git Lifecycle Hooks | Accepted |
| [ADR-007](007-honor-private-packages-during-publish.md) | Honor Private Package Markers During Publish | Accepted |
| [ADR-008](008-dry-run-local-only-guarantee.md) | Dry-Run Must Be Strictly Local-Only | Accepted |
| [ADR-009](009-javascript-package-manager-strategy.md) | JavaScript Package Manager Strategy | Accepted |
| [ADR-010](010-scoped-release-changeset-consumption.md) | Scoped Release Changeset Consumption | Accepted |
| [ADR-011](011-command-execution-strategy.md) | Command Execution Strategy | Accepted |
| [ADR-012](012-workspace-protocol-dependency-updates.md) | Skip workspace: Protocol Dependencies | Accepted |
| [ADR-013](013-logging-infrastructure.md) | Logging Infrastructure | Accepted (backend superseded by [ADR-018](018-replace-fern-with-cli-logger.md)) |
| [ADR-014](014-verbose-mode.md) | Verbose and Silent Modes | Accepted |
| [ADR-015](015-ci-managed-release-workflow.md) | CI-Managed Release Workflow | Accepted |
| [ADR-016](016-rename-release-to-prepare.md) | Rename `release` to `prepare` | Accepted |
| [ADR-017](017-late-guard-dry-run-pattern.md) | Late Guard Pattern for Dry-Run | Accepted |
| [ADR-018](018-replace-fern-with-cli-logger.md) | Replace fern with CliLogger | Accepted |
| [ADR-019](019-improved-init-workflow.md) | Improved Init Workflow | Accepted |
| [ADR-020](020-tui-screen-submodule-structure.md) | TUI Screen Submodule Structure | Accepted |
| [ADR-021](021-commit-references-in-changelog-entries.md) | Add Commit References to Changelog Entries | Accepted |
| [ADR-022](022-distribution-strategy.md) | Distribution Strategy for Cursus Binaries | Accepted |
| [ADR-023](023-dependency-propagation-bumps.md) | Dependency Propagation Bumps During Prepare | Accepted |
| [ADR-024](024-linked-package-versions.md) | Linked Package Versions in Monorepos | Accepted |
| [ADR-025](025-auto-changeset-from-conventional-commit.md) | Add `--auto` Flag to Derive Changesets from Conventional Commits | Accepted |
| [ADR-026](026-per-package-change-level-in-tui.md) | Per-Package Change Level Selection in TUI Wizard | Accepted |
| [ADR-027](027-mutation-testing-approach.md) | Adopt Mutation Testing as a Test Quality Verification Strategy | Accepted |
| [ADR-028](028-npm-oidc-trusted-publishing.md) | Support npm OIDC Trusted Publishing | Accepted |
| [ADR-029](029-cargo-publish-authentication-warning.md) | Warn on Missing Cargo Registry Token Before Publish | Accepted (warning behaviour superseded by [ADR-045](045-crates-io-trusted-publishing.md)) |
| [ADR-030](030-bin-lib-crate-separation.md) | Separate Binary and Library Crates with Environment Injection | Accepted |
| [ADR-031](031-changelog-guard-for-unprepared-packages.md) | Guard Publish and CI Against Never-Prepared Packages Using CHANGELOG.md | Accepted |
| [ADR-032](032-verify-changeset-on-branch.md) | Verify Changeset Presence on Feature Branches | Accepted |
| [ADR-033](033-windows-shell-execution.md) | Extend Command Execution to Support Windows | Accepted |
| [ADR-034](034-compile-time-embedded-localisation.md) | Use fluent-templates for Compile-Time Embedded Localisation | Accepted |
| [ADR-035](035-git-trait-abstraction.md) | Introduce a Git Trait for Abstracting All Git Operations | Accepted |
| [ADR-036](036-filesystem-trait-abstraction.md) | Introduce Filesystem Trait for File I/O Abstraction | Accepted |
| [ADR-037](037-async-library-with-tokio-runtime.md) | Make the Library Crate Async with Tokio Runtime | Accepted |
| [ADR-038](038-octocrab-github-client.md) | Replace ureq-Based RestGitHubClient with Shared Octocrab Implementation | Accepted |
| [ADR-039](039-split-dependency-versioning-strategy.md) | Split Dependency Versioning Strategy Between Library and Binary Crates | Accepted |
| [ADR-040](040-strip-git-trailers-from-automatic-changesets.md) | Strip Git Trailers from Conventional Commit Body During Parsing | Accepted |
| [ADR-041](041-rename-github-client-trait-to-code-forge-client.md) | Rename GitHubClient Trait to CodeForgeClient | Accepted |
| [ADR-042](042-repo-identity-in-constructor.md) | Move Repo Identity into CodeForgeClient Constructor | Accepted |
| [ADR-043](043-publish-private-packages-to-github-releases.md) | Allow Private Packages to Publish via Git Tags and GitHub Releases | Accepted |
| [ADR-044](044-per-package-github-release-artifacts.md) | Restructure GitHub Artifact Configuration to Per-Package Sections | Accepted |
| [ADR-045](045-crates-io-trusted-publishing.md) | Support crates.io OIDC Trusted Publishing | Accepted |
| [ADR-046](046-streaming-command-execution.md) | Stream Output of User-Configurable Shell Commands | Accepted |
| [ADR-047](047-configurable-release-target-branch.md) | Configurable Release Target Branch | Deprecated (2026-04-28) |
| [ADR-048](048-native-windows-build-runner.md) | Build Windows Artifacts Natively with MSVC and Static CRT | Accepted (2026-04-26) |
| [ADR-049](049-signed-release-artifacts.md) | Sign GitHub Release Artifacts and Verify in npm Postinstall | Accepted (2026-04-27) |
| [ADR-050](050-verified-release-commits-via-git-data-api.md) | Produce Verified Release Commits via the GitHub Git Data API | Accepted (2026-04-29) |
| [ADR-051](051-bundle-sigstore-deps-via-workspace-removal.md) | Bundle Sigstore Transitive Dependencies into the npm Tarball by Removing the Workspace Declaration | Accepted (2026-05-01) |
| [ADR-052](052-credential-redaction-in-error-messages.md) | Redact Credentials from Subprocess and API Error Messages | Accepted (2026-05-01) |
| [ADR-053](053-npm-package-node-spawner.md) | Use a Node.js Spawner Script for the npm Package Binary Entry Point | Accepted (2026-05-02) |
| [ADR-054](054-cargo-binstall-support.md) | Add cargo-binstall Support for Prebuilt Binary Installation | Accepted (2026-05-02) |
| [ADR-055](055-end-to-end-idempotent-publish-recovery.md) | End-to-End Idempotent Publish Recovery | Accepted (2026-05-03) |
| [ADR-056](056-gitlab-support-client-config-and-ci.md) | GitLab Support — Client, Config, and CI Integration | Accepted (2026-05-13) |
| [ADR-057](057-cursus-init-gitlab-support.md) | `cursus init` GitLab Support | Proposed (2026-05-12) |
| [ADR-058](058-verified-release-commits-on-gitlab-via-web-commits-api.md) | Produce Verified Release Commits on GitLab via the Web Commits API | Proposed (2026-05-12) |
| [ADR-059](059-forge-selection-runtime-rules.md) | Forge Selection Runtime Rules | Proposed (2026-05-12) |
