<!--
One of six. `docs/50-simplify.md` is the brief every agent reads first; it
carries the baseline, the ownership table and rules R1-R9. Nothing here
repeats them.

**A closed item is deleted from this file**, in the commit that closes it.
Item numbers `S51.n` are stable and never reused.
-->

# 51 — Kernel: what is left after the deletion

**Read `docs/50-simplify.md` first.** You are agent 1. The transaction kernel
is gone: the five crates that held it, the SQL workspace built on its
vocabulary, the ledger readers in `jails-report`, the differential canary
and the wire-format goldens. What survives of its vocabulary lives in
`jails-spec` (`coordinate`, `policy`, `constant`, `suffix`, `manifest`).

## What you own

`crates/jails-spec/**`, `src/dispatch.rs`, `tests/protocol-golden/**`.

## Steps

Every step is closed.

## Green

```
cargo test --workspace
mise run verify-rewrite
```
