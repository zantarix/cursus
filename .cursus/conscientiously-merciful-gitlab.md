+++
cursus = "minor"
cursus-bin = "minor"
+++

Harden GitLab forge support. Self-managed instances served over plain HTTP are now reachable end-to-end: the API client and the asset URLs surfaced in release notes both honour the scheme from `CI_API_V4_URL` or `[gitlab].host`. The release-asset host is also pinned to the same endpoint the API client used, so a stale or mirrored git remote can no longer cause asset links to point at the wrong instance. GitLab API errors run through credential redaction before being logged, matching the protection already in place for the signed-commit decorator.
