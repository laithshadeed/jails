# `jails-model`

The closed source schema, stable IDs, linking, semantic diagnostics,
`AppModel` and `Evolution` -- and **every closed vocabulary in jails**. No
dependencies, unless the `cli` feature is on, which adds clap so the CLI's
`ValueEnum`s can be these enums rather than copies of them.

- `jdl/v1` -- the `jdl 1` front end: lossless tokens and CST spans, the
  formatter, and byte-preserving syntax edits.
- `linker` -- resolution and the `model-*` diagnostics, one exhaustive rule
  table per closed registry (component kinds, capabilities, builtins,
  attributes, operation statements).
- `derived` -- every name the compiler derives rather than the author writing,
  keyed by owner and role with the rule that produced it; recomputed from the
  model after every patch, never accumulated.
- `builtin` -- `BuiltinSemantics`, one row per scalar type and the only place
  a type's Java, SQL and sample knowledge lives.
- `field_syntax` -- the compact `name:type[!?]` parser and its markers
  (`@pk`, `@unique`, `@index`, `@positive`, `@nonnegative`, `@scope`). An
  unknown marker is an error, never a no-op. It sits beside `builtin` because
  `normalize_type` canonicalizes onto that table's names.
- `layout`, `capability`, `artifact`, `build`, `unit`, `operation` -- the
  closed sets: layers, capability kinds, generator kinds, build languages,
  HTTP methods, wire formats and If-Match policies. One list each, and the
  member the CLI must not offer is `value(skip)` with its reason beside it.

`docs/54-language.md` removes the two compatibility parsers and the renderer
that serves only the upgrade.
