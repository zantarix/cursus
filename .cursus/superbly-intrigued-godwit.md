+++
cursus = "minor"
cursus-bin = "minor"
+++

Adds verified release commits when running on GitHub Actions with a GitHub App token. The prepare commit is now routed through the GitHub Git Data API, which causes GitHub to sign it with the web-flow GPG key and display the green Verified badge. Enabled automatically via \`signed_commits = "auto"\` (the default); can be disabled with \`signed_commits = "off"\`.
