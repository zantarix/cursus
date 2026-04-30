+++
"@zantarix/cursus" = "patch"
cursus = "patch"
+++

Fixes security vulnerabilities in the npm postinstall download script: redirect targets are now validated against an allowlist of known GitHub domains, response sizes are bounded to prevent memory exhaustion, and GitHub API rate-limit errors include actionable retry guidance.
