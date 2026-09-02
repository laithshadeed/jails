# `jails-model`

The closed source schema, stable IDs, linking, semantic diagnostics,
`AppModel` and `ModelPatch`. No dependencies.

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

`docs/54-language.md` removes the two compatibility parsers and the renderer
that serves only the upgrade.
