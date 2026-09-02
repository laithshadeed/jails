# `jails-drive`

Commands that **start something**: `run`, `test` and its engines (`testd`, the
console launcher, the build tool), `migrate`, `kafka`, `console`, `bench`,
`lint`, `affected`, `live_sql`. The one edge back down is `run` to
`report::why`, because `mvn spring-boot:run` exits 0 over a failed startup.

`testd` is a resident JVM behind a unix socket. Its classpath is split on
purpose: the daemon holds the dependencies and hands only `target/classes` and
`target/test-classes` to JUnit as `--class-path`, so a child loader per run
sees fresh classes. It does not compile; the editor's language server writes
`target/classes` on save.
