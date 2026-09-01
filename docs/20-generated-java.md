<!--
One of six. `docs/00-contracts.md` is the one every reader starts from; it
carries the contracts, the identifier map and the ownership table that keep
these six from contradicting each other.

**A closed item is deleted from the file that holds it**, in the commit that
closes it -- never marked done. `git log -p -- docs/` is the record.

**Item and section numbers are stable and never reused.** A section with no
open items disappears rather than being renumbered.

Status prose is dated where it is a measurement. Everything else is written in
the present tense as a rule: a note narrating what a module used to be gives a
reader nothing to act on and goes stale on its own.
-->

# 20 — The generated Java, and the IR that does not exist yet

**Read `docs/00-contracts.md` first.** It carries the five contracts, the
deletion map, the identifier map and the ownership table; nothing here repeats
them, and work that contradicts them is wrong however well it reads.

## What you own

`crates/jails-compiler/**` -- pure semantic lowering to a desired artifact
tree -- and `templates/**`, the Java bodies it renders.

**Templates are real `.java` files with `{{name}}` substitution**, never Rust
`format!` strings: Java is made of braces and `format!` owns that syntax.
Anything structural stays in Rust and is passed in rendered. A missing or
unused key is a panic, not silent text in a generated class.

## What you do not touch

`crates/jails-model/**` is A's. If an emitter needs a fact the model does
not carry, that is A's change and it lands first -- an emitter reading a field
that does not exist does not compile, which is the right order to discover it
in.

`crates/jails-workspace/**` is C's. The compiler may not observe the
filesystem, the environment or a subprocess; if you need an external fact,
capture supplies it and the change is C's.

Three things are shared with the other three workstreams and have resolution
rules in `docs/00-contracts.md`: `tests/golden/**`, `tests/architecture/board.rs`
and `LAYERS`. Append to `tests/common/scenarios.rs`; move nothing in it.

## The specification sections this work answers to

§9 app/types/fields and §9.7 canonical projection conventions, §12
operations and HTTP bindings, §13 relations, §14 generic components,
§15 caps and project declarations, §20.1 pass 4 (typed artifact IR),
§20.2 shared registries.

## How you know you are green

```
cargo test -p jails-compiler
UPDATE_GOLDEN=1 cargo test --test golden   # then READ the diff
mise run verify-rewrite
```

**The goldens compare bytes and never run the code**, so they are necessary and
not sufficient. The oracle for this workstream is the real-toolchain tier: a
generated project that compiles and passes its own tests. Reproduce every item
from a clean `jails new` before believing it, and state the command.

---

## A3.14 — no typed artifact IR

§20.1 pass 4 specifies `JavaFile` / `JavaDecl` / `JavaExpr` / `SqlExpr`. None
exist. The canonical emitters assemble Java and SQL with `format!` exactly as
the legacy generators do -- 67 sites in `emit_unit.rs`, 53 each in
`emit_sql.rs` and `emit_java.rs`.

**This is the largest unbuilt piece of the design**, and it is a phase rather
than a fix. It is also the reason for A4.2 below.


**Exit:** §20.1 pass 4's `JavaFile` / `JavaDecl` / `JavaExpr` / `SqlExpr`
exist and the emitters build them instead of strings. This is a phase rather
than a fix, and it is the reason A4.2 below is flat -- moving string assembly
into a new crate does not make it cheaper.

## A4.2 — the generation simplification has not happened

646 production lines per generator-or-capability on the legacy side; 619 on the
canonical side. Flat. The cause is A3.14 above.

**The one place a real IR exists is `Pack`**, and it is also the one place
legacy and canonical share templates and cannot drift. That is the shape the
rest of the emitters are missing, and it is the evidence that the shape works.

## A3.15 — §16.4's readable boundary path does not resolve

§16.4 says the *preferred* ejection reference is a readable, linked boundary
path -- `Entity.record`, `Entity.repo.fake`, `Entity.http.api` -- and that
"the boundary registry, not string concatenation in the parser, defines valid
paths". **There is no boundary registry.** `known_targets` in the linker is the
set of stable IDs already in the model, so an ejection resolves only against an
`art_*` id or a node id, and `jails model eject` takes "a stable entity,
operation, or capability id".

So the §4 complete example does not link: `eject Task.repo.fake` refuses with
`model-ejection-target`, and it is the only thing wrong with it.
`the_specification_complete_example_links_except_its_one_recorded_gap` pins
both halves -- the rest of the example links, and that line still refuses --
so this entry cannot go stale in either direction.

This is the same missing piece as §20.2's `OutputConvention` registry, seen
from the other side: both want one data table that maps a readable name to a
generated artifact, and neither exists. Emitters concatenate role suffixes at
the point of use instead (`format!("{}Repository", ..)` in two files), which is
why §20.2's "emitters MUST NOT concatenate a package, prefix, suffix, filename
or test marker" is not held today.

**Exit:** one registry, read by the linker for ejection targets and by the
emitters for output names, with the exhaustiveness test §20.2 asks for -- an
emitter asking for an unregistered role, or a registered role with no emitter,
fails the build.


## A1 — what coverage does not say

**`migration` is the one whose gap is genuinely in the plan.** A migration is
an irreproducible operation by construction (§1.6), so "covered" means the
plan carries it, not that a renderer produces it from the model.

**The §9.7 divergence is recorded rather than hidden.** Six of the
twenty-three emitted packages sit under a head §9.7 does not close, so a
`jails.toml` layer rename does not reach them. Their rule reads
`convention.facet.*` where a layer's reads `convention.layer.*`. Reconciling
them would move files in every project generated so far, which is why it is a
recorded divergence and not a bug.


## A6.2 — `Compiler::compile` carries what §20.1 splits into passes

```
508  crates/jails-compiler/src/lib.rs:68  Compiler::compile
```

The layers are conceptually present and are not separately addressable. A3.14
is the change that would naturally split it; splitting it *without* the IR
moves the problem rather than solving it, and the module-size ratchet cannot
tell the two apart.

---

## P9 — the two research items that are this workstream's

**P9.1 §4.6 — the repository contract test.** One contract interface executed
once against the fake and once against `JdbcOrderRepository`, so semantic drift
becomes a failing test. Today the two adapters can diverge silently.


**P9.5 §4.7 — policy and contract matrices, closed form only.** No expression
string, no SpEL passthrough -- the same rule that keeps `@check(...)` out of
the field spec. `@scope` and `require_scope_authorizer` already cover the
tenancy half.

## The one item you share with A

`A3.15`'s registry above is read by the linker as well as by you: ejection
targets resolve against it, which is why §16.4's readable boundary path does
not link today. Agree its shape with A before either of you writes it.
