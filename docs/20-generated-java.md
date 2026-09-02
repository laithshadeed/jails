<!--
Workstream B. `docs/00-contracts.md` carries the contracts and the ownership
rules; nothing here repeats them.

**A closed item is deleted from this file**, in the commit that closes it.
Item numbers are stable and never reused.
-->

# 20 — The generated Java, and the IR

**Read `docs/00-contracts.md` first.** During the simplification pass,
`docs/55-compiler.md` is the active plan for these paths.

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
phase, not a fix. `docs/55-compiler.md` S55.2 takes its first rung, one
`JavaUnit` for the package line, the import block and the class shell; `Pack`
is the shape the rest is missing.

## A3.15 — §16.4's readable boundary path does not resolve

§16.4 says the preferred ejection reference is a readable, linked boundary
path -- `Entity.record`, `Entity.repo.fake`, `Entity.http.api` -- defined by a
boundary registry rather than string concatenation. There is no such registry:
`known_targets` in the linker is the set of stable IDs already in the model,
so an ejection resolves only against an `art_*` id or a node id, and emitters
concatenate role suffixes at the point of use (`format!("{}Repository", ..)`).
§20.2's "emitters MUST NOT concatenate a package, prefix, suffix, filename or
test marker" is therefore not held.

`the_specification_complete_example_links_except_its_one_recorded_gap` pins
the gap: `eject Task.repo.fake` in the §4 example refuses with
`model-ejection-target`, and it is the only thing wrong with the example.

**Exit:** one registry, read by the linker for ejection targets and by the
emitters for output names, with the exhaustiveness test §20.2 asks for -- an
emitter asking for an unregistered role, or a registered role with no emitter,
fails the build.

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
