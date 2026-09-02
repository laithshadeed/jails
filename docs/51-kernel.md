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

`crates/jails-codec-derive/**`, `crates/jails-support/src/codec.rs` and
`codec/`, `crates/jails-spec/**`, `src/dispatch.rs`, `tests/protocol-golden/**`,
`docs/30-cutover.md`.

## Steps

**S51.4 -- The codec.** `#[derive(Codec)]` and `jails_support::codec` have
one user left: `jails-drive::testing`'s v1 test-plan wire (`TestSelector`,
`TestExecutionPlanV1`, `TestReportV1`) and the daemon protocol in
`testing/testd.rs`. `docs/60-abstraction.md` S60.6 makes that one vocabulary
with `testd::v2`, which frames `serde` values; when it lands, delete the
codec, the derive crate, the board's *types whose wire format is
hand-written* and *codec halves outside `impl Codec`* rows, and the `extern
crate self` line in `jails-support`. `hex` and `sha256` are the two things
outside the wire that reach into `codec`; they move to `jails-support`'s
root.

**S51.7 -- The prose.** `docs/30-cutover.md` holds the workspace workstream's
open items; when nothing is left in it, delete the file and its row in
`docs/00-contracts.md`.

## Green

```
cargo test --workspace
mise run verify-rewrite
```
