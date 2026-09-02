# `jails-report`

Commands that **answer a question**: `doctor`, `why`, `explain`, `src`,
`commands`. Read-only by construction: the crate sits below `jails-drive`, so a
report that started something would not compile.

`why` is a table of (signature, explanation, fix) rules matched against a log;
add rules only from failures that happened. `explain` is a hand-written table
with one entry per artifact kind, held complete by a test. `commands` walks the
live `clap::Command`, so there is no second list of what jails can do.
