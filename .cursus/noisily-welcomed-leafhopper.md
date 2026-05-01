+++
cursus = "patch"
+++

Rejects package names and git ref names that start with '-' or contain ASCII control characters, preventing argv-smuggling attacks where a malicious workspace member name could be interpreted as a flag by the git binary.
