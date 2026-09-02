# `jails-support`

Write, run, encode and name. Nothing here knows what a Java project is.

- `apply` -- **the only module that writes.** `create`, `replace`, `put` and
  `put_bytes` say what the caller believes is already there; the `_outside_project`,
  `_in_scratch` and `_derived` verbs are the named exemptions, and the last two
  refuse a path outside `target/` or `build/`. `tests/architecture/` fails on
  an `fs::write` anywhere else.
- `process` -- `CommandSpec` and one synchronous executor; the one place a tool
  is resolved on `PATH`. Debug output prints the command before running it and
  never renders a secret.
- `hermetic` -- the runner for a subprocess that must not see the caller's
  environment.
- `scratch` -- `ScratchDir`, the only thing that reserves a temporary
  directory, exclusively, through the OS.
- `git` -- probes whether this machine's `git merge-file` accepts
  `--diff-algorithm`; `JAILS_GIT_DIFF_ALGORITHM` pins the answer.
- `unified` -- the bounded unified diff, capped on the product of line counts.
- `lock`, `identity`, `identifier`, `json`, `codec`, `Result`, `Failure`.
