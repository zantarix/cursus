+++
cursus = "patch"
+++

`prepare` now fails immediately with a clear error when run on a detached HEAD under the branch strategy, instead of creating a `cursus-release/detached` branch and failing later. Check out a branch or use the push strategy.
