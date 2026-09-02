# `jails-workspace`

Capture, exact materialization, verification and the single executor.

- `capture` reads every external fact once into a `WorkspaceSnapshot`, over
  the *intended* model (its `intended` argument) so a command that declares
  a thing sees the trees it needs.
- `materialize` freezes a `PlanDraft` into a content-addressed `PlanBundle`.
- `merge` and `reconcile` are the three-way merge over BASE (the accepted
  projection), OURS (the live file) and THEIRS (the next render).
- `documents` are the bounded, lossless adapters for `pom.xml`, `build.gradle`
  and `application.properties`: marked blocks and per-key edits with captured
  before-images.
- `execute` locks, rechecks every precondition, stages under `.jails-staged-`,
  publishes exact after-images and converges on retry. `tests/crash.rs` trips
  every failpoint twice, once unwinding and once aborting in a child process.
