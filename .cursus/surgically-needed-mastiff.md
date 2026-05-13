+++
cursus = "minor"
cursus-bin = "minor"
+++

Add first-class GitLab support: [gitlab] config section, ReqwestGitLabClient implementing CodeForgeClient via the Kitware gitlab crate, GitLab CI token detection at the binary boundary, and a forge-neutral crate::forge module layout (relocates crate::github to crate::forge::github)
