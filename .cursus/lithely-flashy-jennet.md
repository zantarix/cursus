+++
"@zantarix/cursus" = "patch"
cursus = "patch"
+++

Rejects changeset files larger than 64 KiB and config.toml larger than 256 KiB to prevent out-of-memory conditions when processing maliciously oversized inputs.
