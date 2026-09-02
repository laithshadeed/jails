<!--
Workstream B. `docs/00-contracts.md` carries the contracts and the ownership
rules; nothing here repeats them.

**A closed item is deleted from this file**, in the commit that closes it.
Item numbers are stable and never reused.
-->

# 20 — The generated Java, and the IR

**Read `docs/00-contracts.md` first.** The simplification pass's compiler
plan (plan 55) closed every item it held and its file is gone; the git log
of that path is its record. `docs/60-abstraction.md` S60.3 is the direction
these paths move in now.

## What you own

`crates/jails-compiler/**` -- pure semantic lowering to a desired artifact
tree -- and `templates/**`, the Java bodies it renders.

**Templates are real `.java` files with `{{name}}` substitution**, never Rust
`format!` strings. Anything structural stays in Rust and is passed in rendered.
A missing or unused key is a panic.

## What you do not touch

`crates/jails-model/**` is A's: a fact the model does not carry is A's change
and lands first. `crates/jails-workspace/**` is C's: the compiler may not
observe the filesystem, the environment or a subprocess; an external fact is
captured, and the change is C's.

## The specification sections this work answers to

§9 app/types/fields and §9.7 canonical projection conventions, §12 operations
and HTTP bindings, §13 relations, §14 generic components, §15 caps and project
declarations, §20.1 pass 4 (typed artifact IR), §20.2 shared registries.

## How you know you are green

```
cargo test -p jails-compiler
UPDATE_GOLDEN=1 cargo test --test golden   # then READ the diff
mise run verify-rewrite
```

**The goldens compare bytes and never run the code.** The oracle is the
real-toolchain tier: a generated project that compiles and passes its own
tests. Reproduce every item from a clean `jails new` and state the command.

---

## A3.14 — the typed artifact IR

§20.1 pass 4 specifies `JavaFile` / `JavaDecl` / `JavaExpr` / `SqlExpr`. The
emitters assemble Java and SQL with `format!`:

```
grep -rc 'format!(' crates/jails-compiler/src | awk -F: '{s+=$2} END {print s}'
```

**Exit:** the IR exists and the emitters build it instead of strings. It is a
phase, not a fix. Two rungs are landed: one `JavaUnit` for the package line,
the import block and the class shell, and one `emit_mockmvc` for the MockMvc
dialect; and `Recipe<N>` (`docs/60-abstraction.md` S60.3), the declarative
shape every capability pack, twelve component kinds, the event's Kafka slice
and the outbox are rows of. What the rows do not yet carry is the structural
Java -- a record's components, a repository's column list, a query's SQL --
which the five remaining function passes still build with `format!`; S60.3
names them and keeps the count.

## A1 — what coverage does not say

**`migration` is deliberately not a declaration** (§1.6, §12.6). "Covered"
means the plan carries it as an `AppendMigration`, not that a renderer
produces it from the model.

**The §9.7 divergence is recorded rather than hidden.** Six of the
twenty-three emitted packages sit under a head §9.7 does not close, so a
`jails.toml` layer rename does not reach them. Their rule reads
`convention.facet.*` where a layer's reads `convention.layer.*`. Reconciling
them would move files in every project generated so far.

## A6.2 — `Compiler::compile` carries what §20.1 splits into passes

The layers are conceptually present and are not separately addressable. A3.14
is the change that splits it; splitting it without the IR moves the problem
rather than solving it.

---

## P9.5 §4.7 — policy and contract matrices, closed form only

No expression string, no SpEL passthrough -- the same rule that keeps
`@check(...)` out of the field spec. `@scope` and `require_scope_authorizer`
cover the tenancy half.
