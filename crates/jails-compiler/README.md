# `jails-compiler`

Pure semantic lowering of a linked `AppModel` and a `WorkspaceSnapshot` to a
desired artifact tree. No filesystem, environment or subprocess access, held
by `canonical_compiler_is_pure_after_capture` and by the crate depending on
nothing that can read a disk.

`Compiler::compile` runs the passes: normalize facets, derive the dependency
graph, lower entities, operations, components and capabilities to typed
artifacts, derive schema and evolution, emit a `PlanDraft`. Every artifact
carries a stable ID, so merge and ejection pair BASE and THEIRS by identity
rather than by path.

Java bodies come from `templates/` as real `.java` files with `{{name}}`
substitution; anything structural stays in Rust and is passed in rendered.
Capabilities are `Pack`s: files, dependencies, properties and one ejection
boundary as data.

`docs/55-compiler.md` is the current work: orphaned templates, one Java shell,
one proof renderer.
