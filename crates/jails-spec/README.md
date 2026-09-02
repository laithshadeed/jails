# `jails-spec`

The closed CLI vocabularies and where a project is.

- `spec::kind` -- `ArtifactKind`, `Capability` and the other `clap::ValueEnum`s,
  so `clap_complete` can emit static completion lists and the CLI spelling is
  the recorded spelling.
- `spec::field` -- the compact `name:type[!?]` field syntax and its markers
  (`@pk`, `@unique`, `@index`, `@positive`, `@nonnegative`, `@scope`). An
  unknown marker is an error, never a no-op.
- `spec::paths` and `spec::layout` -- `find_project_root` and the eleven
  package layers.
- `build` -- which build tool a directory uses, and nothing more. The door is
  any recognised marker, nearest wins; jails never parses a foreign build file.

This is where a symbol shared by several crates lives when it belongs to none
of them.
