---
name: analyse-mutations
description: Use to analyse mutation test results to increase test coverage.
---

If this skill is invoked directly by the user, assume that `cargo mutants` has already been run by them. If you decide
to proactively use this skill then you should ask the user to run `cargo mutants` first and confirm with them that they
have done so. You should not run this command directly yourself as it is slow and generates a lot of useless context for
you.

Once the report has been generated, you can access it in `mutants.out/missed.txt`. This file lists every mutation test
that failed and indicates a code path that is currently insufficiently tested. Each line of this file is of the
following format:

```
<filename>:<line number>:<column number>: <description of how the file was changed>
```

If you wish to see the precise way that the file was changed, you can access a diff in the
`mutants.out/diff/<filename>_line_<line number>_col_<column number>.diff` file.

You should analyze these outputs and devise ways to increase the test coverage.
