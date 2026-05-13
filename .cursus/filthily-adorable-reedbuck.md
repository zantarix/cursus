+++
cursus = "minor"
cursus-bin = "minor"
+++

Cursus now rejects configurations with more than one forge section enabled at load time. Setting both `[github].enabled = true` and `[gitlab].enabled = true` in `.cursus/config.toml` produces a hard error that names the offending flags and explains the fix. Configs with a single enabled forge — or no enabled forge — continue to work as before.
