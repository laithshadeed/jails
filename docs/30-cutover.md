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

## P8.11a — the adoption half of P8.11

**`jails adopt resource <Name>`** registers an existing hand-written type
into the model so `resource field`, `destroy` and `rename resource` work on
it. Today they refuse: *"no `Message` is recorded in this project"*. It
writes a declaration plus the `eject` lines that say the reader owns the
implementation, so it needs the readable boundary path (`Message.record`,
`Message.repo.fake`) to link -- A3.15's registry, which is the compiler's
and the linker's -- before it can be written. The `modernize` half closed:
on a modelled project it recompiles the model against the versions it moved,
the way `jails sync` does.

## Product direction that is yours

**P9.6 §5.1 — Gradle behavioural parity** for the warm test engine, `jails
fmt` and `jails console`. **Exit:** `jails test`, `--engine build` and
`--engine warm` discover the same tests and report the same counts on both
build systems.
