# Refactor plan, in plain English

This file answers three questions for each suggestion:

1. What is confusing or risky today?
2. What should change?
3. Why is that change useful?

The main recommendation is simple: **finish the V2 migration and delete V1
before attempting a large redesign**. Most of the repository's complexity
comes from supporting both systems at once.

## What to do first

Do the work in this order:

1. Make every build and test target pass.
2. Finish the remaining V2 work.
3. Delete the old V1 implementation.
4. Make internal Rust modules private.
5. Simplify the path from CLI input to generated files.
6. Fix and split the architecture test.
7. Split the largest source and test files.
8. Clean up dependencies, documentation, and folders.

---

## 1. Make the whole project green

### What is wrong?

The normal production build can pass while test code is broken.

At the time of this review, this command failed:

```bash
cargo clippy --workspace --all-targets
```

One test in `crates/jails-project/src/properties.rs` still calls `remove` with
two arguments even though the function now needs three. There are also unused
imports in `src/main.rs` and `src/invoke.rs`.

### What should change?

- Fix the outdated test call.
- Remove the unused imports.
- Add CI that always runs:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets
cargo test --workspace
```

### Why?

Refactoring is safer when every target starts green. Otherwise a new failure
can be mistaken for an old one.

---

## 2. Finish V2, then delete V1

### What is wrong?

The repository currently contains two systems for changing a project:

- **V1** edits files directly.
- **V2** plans the complete change and commits it as a transaction.

The CLI in `src/main.rs` now uses V2, but much of V1 still exists in the
library crates. That means there are two implementations of planning, writing,
recording, and recovering changes.

### What should change?

First, finish the V2 command failures listed in `pending.md`.

Then remove old V1 entry points such as:

- `jails_generate::add::add`
- `jails_generate::add::add_in`
- the old `add::shrink::sync`
- `generate_with_timestamps`
- `generate_in_project`
- the old `generate::remove::destroy`
- `jails_tooling::rename::rename`

After removing those entry points, remove files and helpers that only V1 uses.
Likely examples are:

- `crates/jails-tooling/src/rename.rs`
- `crates/jails-project/src/generated_files.rs`
- V1-only direct-write helpers
- V1-only tests

Keep support for reading old schema-1 projects until the project explicitly
drops that compatibility. Reading an old project format does **not** require
keeping the old V1 write engine.

### Why?

This is the largest available cleanup. Deleting V1 will remove more
complexity than reorganising V1 and V2 together.

---

## 3. Make internal Rust code private

### What is wrong?

Many crate roots expose every module with `pub mod`, and `jails-generate`
re-exports many items with `pub use ...::*`.

Rust assumes public code may be used by another project. It therefore cannot
reliably warn that this code is unused. That helped old V1 functions survive
after the CLI stopped calling them.

### What should change?

For each crate:

- Make modules private by default.
- Export only the small API that other crates need.
- Prefer named exports over `pub use ...::*`.
- Use `pub(crate)` for implementation shared inside one crate.
- Use `pub(super)` for helpers shared only with a parent module.

### Why?

The compiler will find more dead code, and callers will no longer depend on
the library's current folder layout.

---

## 4. Represent a generation request only once

### What is wrong?

One `jails generate` request is currently represented several ways:

- `src/app.rs::ResolvedIntent`
- `jails_engine::route::Intent`
- `jails_generate::generate::Recipe`
- `IntentSpec` and `IntentId` in `jails-protocol`
- long function argument lists

The same names, fields, indexes, package, `on`, and `yields` values are parsed
or checked in several layers.

### What should change?

Parse and validate the input once, producing one request object. For example:

```rust
struct CanonicalIntent {
    id: IntentId,
    spec: IntentSpec,
    syntax: RequestSyntax,
}
```

The exact name is unimportant. The flow should be:

```text
CLI or manifest text
    -> one validated request
    -> engine plan
    -> generator
    -> desired files
```

Pass this object through the engine and generator instead of repeatedly
passing long lists of strings and booleans.

### Why?

Every layer will agree on the request's canonical name, package, arguments,
indexes, and lifecycle. Functions will also become shorter and harder to call
incorrectly.

---

## 5. Break up the giant generator function

### What is wrong?

`artifacts_for` in `crates/jails-generate/src/generate/recipes.rs` is roughly
600 lines. It validates input, detects the project type, chooses packages,
selects renderers, builds artifacts, and writes feature-specific errors.

### What should change?

Keep one complete `match` on `ArtifactKind`, but make every branch call a
small feature renderer:

```rust
match request.kind() {
    ArtifactKind::Controller => controller::render(request, project),
    ArtifactKind::Record => domain::render_record(request, project),
    ArtifactKind::Search => spring::search::render(request, project),
    // ...
}
```

Validate the request before it reaches these renderers.

Also put stable facts about artifact kinds and field types in one place. These
facts include labels, aliases, lifecycle, suffixes, accepted arguments, Java
types, SQL types, and example values. Keep actual template rendering in
`jails-generate`.

### Why?

Someone changing search generation should not need to understand every other
generator. A new artifact kind or field type should not require edits to
several matching tables that can drift apart.

---

## 6. Fix the architecture test

### What is wrong?

`tests/architecture.rs` treats a module name as globally unique. This prevents
two unrelated crates from both having a sensible name such as `dispatch`.

That limitation is already affecting production names: `src/invoke.rs`
explains that it is not named `dispatch` because another crate already uses
that module name.

The test file is also about 1,700 lines and mixes architecture rules, a small
Rust parser, dependency layers, historical notes, ratchets, and parser tests.

### What should change?

Identify a module by both crate and module path, for example:

```text
(jails, dispatch)
(jails-java, dispatch)
```

Then allow duplicate local module names in different crates.

Split the test internally while keeping one integration-test binary:

```text
tests/architecture.rs
tests/architecture/source.rs
tests/architecture/model.rs
tests/architecture/rules.rs
tests/architecture/ratchets.rs
```

Use Cargo metadata for crate dependencies where possible instead of rebuilding
that information from source text.

### Why?

Architecture tests should enforce the intended design. Production code should
not need awkward names to work around the test scanner.

---

## 7. Improve the lower-level crate boundaries

### Machine state

`jails-commit` currently depends on `jails-project` for machine-state and
filesystem observations. This points in the wrong direction: committing a
transaction is lower-level than understanding Maven and Java projects.

Move ledger reading/writing, schema translation, legacy machine-file
discovery, and plan rechecks into a small lower-level boundary. This could be
a `jails-state` crate or modules owned by `jails-commit`/`jails-protocol`.

### `jails-support`

`jails-support` has become a home for many unrelated helpers: filesystem IO,
JSON, codemods, locks, scratch directories, normal processes, sandboxed
processes, error aliases, command printing, and a test cwd lock.

Clean it up gradually:

- Move `CWD_LOCK` into test support.
- Route raw `Command` users through `process::CommandSpec`.
- Rename `runner` to `sandbox_runner` or `hermetic_runner`.
- Move state encoding beside machine state.
- Move project-specific codemods beside project editing.

Keep interactive and hermetic process execution visibly separate; they have
different safety rules.

### Why?

Lower-level crates should provide a few coherent capabilities. They should not
become general dumping grounds or depend on higher-level project knowledge.

---

## 8. Split large files without changing behaviour

### Production code

Suggested root layout:

```text
src/main.rs          # parse, run, choose exit status
src/cli/mod.rs       # Clap definitions
src/cli/dispatch.rs  # exhaustive Command match
src/invoke.rs        # preview, commit, report
src/new/             # new-project creation
src/app/             # app-manifest syntax
```

Split `src/new.rs` into Spring download/offline creation, plain project
creation, default files, git setup, and atomic publication.

Suggested engine layout:

```text
jails-engine/src/route/
  mod.rs
  request.rs
  planning.rs
  transition.rs
  session.rs
  commands/
```

### Tests

The largest test files are approximately:

- `tests/cli.rs`: 7,600 lines
- `tests/engine.rs`: 4,000 lines
- `tests/architecture.rs`: 1,700 lines
- `tests/common/mod.rs`: 1,000 lines

Split them into submodules, but keep a small number of integration-test
binaries. For example:

```text
tests/cli.rs
tests/cli/new.rs
tests/cli/generate.rs
tests/cli/capabilities.rs
tests/cli/app.rs
tests/cli/tooling.rs
```

### Why?

This improves navigation without changing behaviour, exposing private code,
or creating a separate slow integration-test binary for every small file.

---

## 9. Clean up Cargo dependencies

### What should change?

- Remove `jails-protocol` from `jails-tooling` if it remains unused.
- Move the root package's `jails-commit`, `jails-protocol`, and `jails-spec`
  dependencies to `[dev-dependencies]` if only integration tests use them.
- Add `[workspace.dependencies]` to the root manifest.
- Declare shared versions and internal crate paths once.
- Add package metadata such as `rust-version`, license, repository, and
  `publish = false` where appropriate.
- Remove temporary facade dependencies after the migration finishes.

### Why?

Cargo files should show the real runtime architecture. Test-only or unused
dependencies make crates look more coupled and increase build work.

---

## 10. Repair documentation and folder names

### Documentation

`plan.md`, `abstract.md`, and `playground.md` were deleted, but the repository
still contains many references to them. Some comments also still say V2 is not
called by `main.rs`, which is no longer true.

Replace important old references with stable decision documents:

```text
docs/decisions/001-one-writer.md
docs/decisions/002-transaction-protocol.md
docs/decisions/003-machine-state-compatibility.md
docs/decisions/004-hermetic-processes.md
```

Use `pending.md` only for unfinished work. Use decision documents to explain
rules that production code must keep following.

### Folders

The roles of these folders are not immediately clear:

- `examples/`: manifest inputs
- `playground/`: complete generated applications
- `tests/golden/`: expected generated trees
- `validation/`: shell scenarios

Either document those roles clearly or rename them to something like:

```text
examples/manifests/
examples/worktrees/
tests/fixtures/golden/
scripts/validation/
```

Keep `deps/`, `ideas/`, and `patterns/` as ignored research material rather
than including them in builds or normal indexing.

### Why?

A new contributor should be able to tell which documents are current and
which example folders are inputs, outputs, fixtures, or validation scripts.

---

## Suggested pull requests

Keep each pull request small enough to review and leave the whole workspace
green after every one.

### PR 1: Restore the baseline

- Fix the outdated test.
- Remove unused imports.
- Add format, clippy, and test CI.

### PR 2: Finish V2

- Fix the remaining V2 command failures.
- Verify the example applications.
- Update user-facing property-format documentation.

### PR 3: Delete V1

- Remove old add/generate/remove/rename entry points.
- Remove V1-only helpers, bookkeeping, and tests.
- Reduce the public file-writing API.

### PR 4: Close crate APIs

- Make internal modules private.
- Remove wildcard exports.
- Remove unused and test-only production dependencies.
- Centralise workspace dependencies.

### PR 5: Fix architecture rules and documentation

- Use crate-qualified module identities.
- Split the architecture test.
- Replace references to deleted documents.
- Correct stale V2 comments.

### PR 6: Simplify generation

- Introduce one validated generation request.
- Pass it through the engine and generator.
- Split `artifacts_for` into feature renderers.
- Centralise artifact-kind and field-type facts.

### PR 7: Improve lower-level boundaries

- Move machine-state IO below `jails-project`.
- Simplify `jails-support`.
- Split the largest production and test files.

## If only three things get done

1. Finish V2 and delete V1.
2. Make crate internals private so Rust can find dead code.
3. Use one validated request object from CLI parsing through generation.

Those three changes will remove the most duplication and make later cleanup
much easier.
