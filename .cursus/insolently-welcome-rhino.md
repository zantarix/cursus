+++
"@zantarix/cursus" = "patch"
cursus = "patch"
+++

Fixes token leakage where GitHub access tokens, registry credentials, and other URL-embedded secrets could appear in error messages produced by failed git operations or package publishes.
