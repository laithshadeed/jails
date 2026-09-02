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

**P13.2 Five production files parse Maven XML; the design asks for one.**
`jails-project/src/pom.rs` is the reader being replaced;
`jails-workspace/src/{capture,documents}.rs` and `documents/build_feature.rs`
are the document backend; `jails-protocol/src/vocabulary/coordinate.rs` reads
a plugin block as a protocol value. The ratchet exists so a sixth answer
cannot appear. `jails-workspace/src/capture/observe.rs`'s `junit_version` is
below the bar: it matches one element to read one artifact's version.

**Exit:** the board's *production files parsing Maven XML with their own
scanner* row reads one. `docs/53-tool-crates.md` S53.3 is the plan.

## P8.11a — the adoption half of P8.11

- **`jails adopt resource <Name>`** registers an existing hand-written type
  into the model so `resource field`, `destroy` and `rename resource` work on
  it. Today they refuse: *"no `Message` is recorded in this project"*.
- **`modernize` does not re-plan jails' own output.** It moves the Boot
  version, and the Boot version decides what jails' generated files should say
  -- `javax.validation` against `jakarta.validation`, the `@AutoConfigureMockMvc`
  package, the MockMvc form. It should recompile the model afterwards.

Both write reader-owned files on projects with no model, `apply::` is banned
outside the write layer, and `execute` takes a `PlanBundle` whose `Plan`
carries a `CanonicalModelPatch`. So either the layout and release become model
nodes, or `jails-workspace` gains an explicit reader-file operation no compiler
produced. Forging a `Plan` outside the compiler is not on the list.

## Product direction that is yours

**P9.2 §3.3 — frozen conflicts, `continue` and `abort`.** Three-way
reconciliation, conflict detection and per-path reporting all work; the marker
bytes are produced and dropped. `PendingIdentity`, `ResolutionIdentity` and
`RestoreIdentity` exist and nothing reaches them. A refusal must never name
`jails continue`, which does not exist.

**P9.6 §5.1 — Gradle behavioural parity** for the warm test engine, `jails
fmt` and `jails console`. **Exit:** `jails test`, `--engine build` and
`--engine warm` discover the same tests and report the same counts on both
build systems.

**P9.7 §2.3, §2.4a, §2.4b, §2.4c — the latency work, each behind a dated
measurement.** The incremental source/AST index, additive test-dependency
hints, service identity labels and semantic readiness. No item in this group
starts without a number.

**P9.8 §2.7 — the Ecto-style SQL sandbox stays deliberately deferred.** Not a
roadmap dependency and not a default. If the experiment is run, record the
negative result rather than deleting the section.

**P9.10 `jails schema diff` requires `.jails/app.toml`**, so it does not run on
the shape `jails new` produces.
