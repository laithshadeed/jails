<!--
Workstream C. `docs/00-contracts.md` carries the contracts and the ownership
rules; nothing here repeats them.

**A closed item is deleted from this file**, in the commit that closes it.
Item numbers are stable and never reused.
-->

# 30 — The workspace, the project crates, adoption

**Read `docs/00-contracts.md` first.** During the simplification pass,
`docs/51-kernel.md` and `docs/53-tool-crates.md` are the active plans for
these paths.

## What you own

`crates/jails-workspace/**` (capture, exact materialization, verification and
the single executor), `crates/jails-project/**`, `crates/jails-{drive,report,java}/**`,
and the binary surfaces `src/new.rs`, `src/app.rs`, `src/dispatch.rs`.

## What you do not touch

`crates/jails-model/**` is A's and `crates/jails-compiler/**` is B's. You
capture facts and execute plans; you do not decide what a model means or what
Java it renders.

## The specification sections this work answers to

§1.6 managed output and ejection (in `docs/00-contracts.md`), §15 caps and
project declarations, §16 lifecycle, change evidence and ownership, §23
deliberate non-goals.

## How you know you are green

```
cargo test -p jails-workspace -p jails-project
cargo test --test product_loop --test agreement --test crash
mise run verify-rewrite
```

---

# Open items

None open. Retiring this file is its own item (S51.7).
