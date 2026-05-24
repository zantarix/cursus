+++
cursus = "patch"
+++

Fixes GitLab releases failing in CI when the runner token cannot push tags. Release tags are now created through the forge API (GitLab Tags API / GitHub Git Data API) when verified commits are enabled, so the git remote no longer needs code-push permission. Tags remain annotated but unsigned.
