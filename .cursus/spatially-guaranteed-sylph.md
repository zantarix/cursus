+++
cursus = "minor"
+++

Fixes `cursus change` incorrectly attributing file changes inside an ignored sub-project to its releasable parent. Adds `match_files_to_projects_in_scope`, `Config::load_all_projects`, and `Config::load_projects_partitioned` to the public API.
