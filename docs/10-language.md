<!--
Workstream A. `docs/00-contracts.md` carries the contracts and the ownership
rules; nothing here repeats them.

**A closed item is deleted from this file**, in the commit that closes it.
Item numbers are stable and never reused.
-->

# 10 — The language — model, grammar, linker, diagnostics

**Read `docs/00-contracts.md` first.** During the simplification pass,
`docs/54-language.md` is the active plan for these paths.

## What you own

`crates/jails-model/**` -- the closed source schema, stable IDs, linking,
semantic diagnostics, `AppModel` and `Evolution`. Above it, the JDL front ends
in the binary: `src/model_generate_jdl*`, `src/model_jdl_edit.rs`,
`src/model_explain.rs`. You are the only workstream that may change what a
`.jails/model.jdl` means.

## What you do not touch

`crates/jails-compiler/**` and `templates/**` are B's: an emitter reading a
new model field is B's change. Add the field, link it and diagnose it here
first -- a field nothing reads is inert, an emitter reading a field that does
not exist does not compile.

## The specification sections this work answers to

§5 lexical, §6 grammar, §7 source and linked model, §8 stable identity,
§9 app/types/fields, §10 enums, §11 projections, §18 static semantics and
diagnostics, §19 formatting and CLI source edits, §20.3 normalization,
§20.4 language versioning, §21 conformance.

## The surface you own, as closed registries

Every one of these is a closed set: an unknown member is a refusal, never a
silent no-op. Each is exhaustive somewhere the compiler checks, so the count is
a fact about the tree, re-measured by the command beside it.

| registry | where | spec | count by |
|---|---|---|---|
| capability kinds (`cap`) | `CapabilityKind`, `capability.rs`; the parser asks `declared_in_source` | §15.1 | `awk '/^pub enum CapabilityKind/,/^}/' crates/jails-model/src/capability.rs \| grep -cE '^    [A-Z]'` (26 total; §15.1 closes the 24 with `declarable_in_source`) |
| layers | `Layer::ALL`, `layout.rs` | §9.7 | `awk '/^pub enum Layer/,/^}/' crates/jails-model/src/layout.rs \| grep -cE '^    [A-Z]'` |
| component kinds | `ComponentKind::parse`, `component.rs`; rules in `linker/component/registry.rs` | §14.2 | `grep -c '=> Ok(Self::' crates/jails-model/src/component.rs` |
| builtin scalars | `ALL`, `builtin.rs` -- one `BuiltinSemantics` row each | §9.2 | `grep -c 'token: "' crates/jails-model/src/builtin.rs` |
| field attributes | `parse_field`'s unknown-attribute refusal, `jdl/v1/parser/declaration.rs` | §9.4 | read the match |
| projection spellings (`use`) | `projection_list`, `jdl/v1/parser/projection.rs` | §11.1 | `grep -o '"[a-z-]*"' crates/jails-model/src/jdl/v1/parser/projection.rs \| sort -u` |
| operation statements | `parse_operation_member`, `jdl/v1/parser/operation.rs` | §12 | read the match |
| `EvolutionStep` variants | `evolution.rs` | §16 | `awk '/^pub enum EvolutionStep/,/^}/' crates/jails-model/src/evolution.rs \| grep -cE '^    [A-Z]'` |
| syntax diagnostics (`JDL*`) | `jdl/v1/` | §18 | `grep -rho '"JDL[0-9]*"' crates/jails-model/src/jdl/v1/ \| sort -u \| wc -l` |
| semantic diagnostics (`model-*`) | the linker | §18.2 | `grep -rho '"model-[a-z0-9-]*"' crates/jails-model/src/ \| sort -u \| wc -l` |

Three properties of that table are the ones to defend:

- **The registry is the authority, not a second list.** `ComponentKind`'s
  member rules live in one exhaustive `match`, so a kind added without a row
  is a compile error. `Presence` is three-valued so `Optional` is a deliberate
  answer.
- **One question, one answer.** `BuiltinSemantics` is one row per builtin and
  the board holds a ratchet on the *largest table of per-builtin knowledge
  outside its row*.
- **The aliases in `BuiltinSemantics` are the CLI's, and `jdl 1` refuses them
  by name.** `text`, `String`, `bool` and the other Java spellings are
  canonicalised on the way in by the compact CLI syntax
  (`jails_model::field_syntax::normalize_type`), while `jdl 1` matches
  `BuiltinType::from_token` on the canonical token alone and
  `TypeRef::parse` refuses a bare alias naming the canonical token. Only the
  bare one: `com.example.Path` is a project type whose final segment collides.
  `currency` has no aliases, so `Currency` stays a project type.

## How you know you are green

```
cargo test -p jails-model
cargo test --test cli model::
mise run verify-rewrite
```

`the_specification_complete_example_links_except_its_one_recorded_gap` is the
canary: it extracts §4 out of `docs/01-jdl-v1.md` and links it, pinning both
that the rest of the example links and that the one recorded gap still
refuses.

---

## A3.13 — one diagnostic contract

`JDL####` codes are `jails-model`'s parser's, `model-*` codes are its
linker's, `compile-*` is `jails-compiler`'s, `workspace-*` is
`jails-workspace`'s, and `every_diagnostic_code_belongs_to_the_crate_that_owns_its_phase`
fails the build when a code escapes its crate. `Diagnostic::new`,
`Diagnostics::from_vec`, `Diagnostic::warning` and `Severity` are public, so a
crate above can produce one. Below the parser a diagnostic points at a model
path, not a source span: the compiler never sees the reader's bytes.

**Exit:** the `Result<_, String>` returns in `jails-compiler` and
`jails-workspace` become `Diagnostic`s with a `compile-*` or `workspace-*`
code. Those crates are B's and C's; the contract they adopt is landed.

```
grep -rho 'Result<[^>]*, *String>' crates/jails-{compiler,workspace}/src | wc -l
```

## Open items

**P9.3 §4.2 -- slices.** `rename resource` accepts a `<slice>.<name>`
selector and nothing declares a slice. A slice is a declaration, not a CLI
selector: §4.2 says package layout, ports, migrations and route prefixes
derive from it, and a thing that derives names has to be in the model so
`AppModel.derived` stays a function of the model. The implicit slice is
derived, never written, so a project with no slices keeps working unchanged
and `model fmt --check` passes on every existing file. It is a v2 construct
and pays §6.2's full price: grammar, typed payload, validation, stable-ID rule
(what happens to `ent_order` when `Order` moves into a slice), ownership
boundary, formatter, CLI mapping, upgrade rule (every existing model lands on
the implicit slice without a byte changing), conformance tests.

## The one item you share with B

**A3.15 -- the boundary registry** lives in `docs/20-generated-java.md`
because most of it is about output names. The linker half is yours:
`known_targets` is the set of stable IDs already in the model, so `eject
Task.repo.fake` refuses with `model-ejection-target` and §16.4's readable
boundary path does not resolve. One registry serves both halves; agree its
shape with B before either of you writes it, because an ejection has to
resolve to the id the compiler emits and those are built by `format!` at the
point of use (`Entity.repo.fake` against art_ent_note_repository_memory). A
registry written only here would be a second answer to "what is this artifact
called".
