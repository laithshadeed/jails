# `jails-workspace`

Exact materialization, verification and the single executor, over the
snapshot `jails-project`'s `capture` produced.

- `materialize` freezes a `PlanDraft` into a content-addressed `PlanBundle`,
  through the document adapters in `jails-project`.
- `reconcile` and `reader_facet` are the three-way merge over BASE (the
  accepted projection), OURS (the live file) and THEIRS (the next render).
- `execute` locks, rechecks every precondition, stages under `.jails-staged-`,
  publishes exact after-images and converges on retry. `tests/crash.rs` trips
  every failpoint twice, once unwinding and once aborting in a child process.
