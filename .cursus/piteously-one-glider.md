+++
"@zantarix/cursus" = "minor"
cursus = "minor"
cursus-bin = "minor"
+++

Add GitLab support to `cursus init`. The wizard now prompts you to pick GitHub, GitLab, or Neither as your forge, with a dedicated GitLab editor screen that auto-detects `group/project` from your git origin and surfaces a self-managed host field for non-gitlab.com instances. The generated `.cursus/config.toml` writes the chosen forge as `enabled = true` and emits the other forge as a commented-out template, so switching forges later is a hand-edit away. The config also reorders active sections to the top of the file so your live configuration is visible without scrolling as the schema grows.
