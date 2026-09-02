# `jails-testkit`

Test infrastructure a dependent crate's tests need: `hold_cwd()`, the one lock
around the process-global working directory. Not `#[cfg(test)]`, because a
dependent crate's tests cannot see one.
