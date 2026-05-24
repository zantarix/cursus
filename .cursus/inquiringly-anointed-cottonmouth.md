+++
cursus = "minor"
+++

Adds GitLab merge request references to generated changelog entries. When a changeset's commit came from a GitLab merge request, the changelog now links it using GitLab syntax (`!123+`, including cross-project `group/proj!123+` references) instead of leaving it unlinked. GitHub pull request references are detected and rendered as before.
