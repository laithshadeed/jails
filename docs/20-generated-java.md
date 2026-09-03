<!--
Workstream B. `docs/00-contracts.md` carries the contracts and the ownership
rules; nothing here repeats them.

**A closed item is deleted from this file**, in the commit that closes it.
Item numbers are stable and never reused.
-->

# 20 — The generated Java, and the IR

**Read `docs/00-contracts.md` first.** The simplification pass's compiler
plan (plan 55) closed every item it held and its file is gone; the git log
of that path is its record. `docs/60-abstraction.md` item 2 is the shape
these paths have converged on, and the rule that decides whether the next
emitter joins it.

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

## How the Java is assembled

§20.1 pass 4 specifies `JavaFile` / `JavaDecl` / `JavaExpr` / `SqlExpr`. What
stands in its place is three shapes rather than one IR, and each of the three
is landed: one `JavaUnit` for the package line, the import block and the class
shell; one `emit_mockmvc` for the MockMvc dialect; and `Recipe<N>`
(`docs/60-abstraction.md`, item 2), the declarative shape every capability
pack, twelve component kinds, the entity's one-file facets, the operation
ports, the three storage adapters, six of the eight source-unit kinds, the
event's Kafka slice and the outbox are rows of -- with the structural Java a
row cannot spell (a record's components and compact constructor, an enum's
constants, a port's `Input` record, a table's column and bind lists, a sealed
hierarchy's permits clause) as named fragment renderers in
`emit_java/{fragment,operation,storage,unit}.rs`.

**Which emitters are still functions is decided, not pending.** A
`Fragment::Rendered` is one named function of `(&AppModel, &Node)` and a
recipe's `files` is a static list of one file each, so an emitter is a row
when its structural blocks are independent per-node answers, and a function
when its blocks are several readings of one pass over the fields, when a block
needs a fact the signature does not carry, or when its file count is not one
per row. `emit.rs` holds the two tables and each remaining module's own doc
names its gate: `dto` (the hoist, and the validation package a Boot major
moved), `http` (the `.http` collection's own `{{...}}` syntax, and the MockMvc
dialect), `seed` (one sampling pass feeding both the data file and the
`@Disabled`), the record's companion test (the same), `strategy` (one file per
variant), `controller` (the dialect), the operation adapters' SQL ladder, the
proofs that reach across nodes, and `emit_architecture` (one file per model).

The number to read is the `format!` count, **less the refusals**, because a
refusal message is prose and will always be built with `format!`:

```
all=$(grep -rho 'format!(' crates/jails-compiler/src | wc -l)
refusals=$(grep -rhoE 'Diagnostic::new\(|refuse::' crates/jails-compiler/src | wc -l)
echo $((all - refusals))     # 643: the Java and SQL, not the refusal prose
```

**Read that number as a ratchet, not as progress.** A class body is one
`format!` site however long it is, so moving the three storage adapters --
about 120 lines of Java that lived in Rust strings with every brace doubled
-- into real `.java` templates moved it by two, and the six unit kinds moved
it by fourteen. It only ever falls, which is worth keeping; the thing it
stands for is Java assembled in Rust, and it stands for it loosely. What the
row conversions actually bought is one shell, one path rule, one provenance
rule and one import rule for each file that moved -- which is how the units'
three companion tests, the last generated Java writing its own import block by
hand, came to be ordered the way palantir-java-format orders one.

Output names are the boundary registry's (`jails_model::boundary`): a row's
role is a registry entry and the function emitters name their artifacts
through `Boundary::owned_by`, so §20.2's "emitters MUST NOT concatenate"
holds for every entity artifact and the exhaustiveness tests in
`ejectable.rs` keep it so.

## What coverage does not say

**`migration` is deliberately not a declaration** (§1.6, §12.6). "Covered"
means the plan carries it as an `AppendMigration`, not that a renderer
produces it from the model.

**The §9.7 divergence is recorded rather than hidden.** Six of the
twenty-three emitted packages sit under a head §9.7 does not close, so a
`jails.toml` layer rename does not reach them. Their rule reads
`convention.facet.*` where a layer's reads `convention.layer.*`. Reconciling
them would move files in every project generated so far.

---

## Policy and contract matrices stay closed form

No expression string, no SpEL passthrough -- the same rule that keeps
`@check(...)` out of the field spec. `@scope` and `require_scope_authorizer`
cover the tenancy half.
