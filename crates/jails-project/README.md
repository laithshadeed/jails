# `jails-project`

One resolved `Project`, and every reader-owned file jails reads or edits:
`jails.toml` (`config`), `compose.yaml` (`compose`), `pom.xml` (`pom`),
`build.gradle` (`gradle`), plus `inspect` (`routes`, `beans`, `stats` read from
source, never from a running context) and the offline SQL query workspace
(`query_compiler`, `query_workspace`, `named_query`, `schema`).

Every edit to a reader-owned file is surgical and leaves every other byte
alone. `jails.toml`'s `[layout]` keys are a closed set matching the eleven
layers, and `Config::layers()` is the one place a renamed layer is applied.
