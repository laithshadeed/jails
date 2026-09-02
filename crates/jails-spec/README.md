# `jails-spec`

Where a jails project is, and what builds it. No closed vocabulary and no
clap: `Layer`, `CapabilityKind`, `ArtifactKind`, `EndpointMethod`,
`RequestFormat`, `Precondition`, `BuildSystem` and the compact field syntax
all live in `jails-model`, which owns every closed set.

- `spec::paths` -- `find_project_root`, and where inside a project a class
  goes.
- `build` -- which build tool a directory uses, and nothing more. The door is
  any recognised marker, nearest wins; jails never parses a foreign build
  file. Deliberately not `jails_model::BuildSystem`: this answers what a
  directory looks like from outside, `Foreign(name)` included.
- `release` -- the three version pins a generated project carries.
- `spec::{coordinate, constant, suffix, policy}` -- the small tables the
  generators are given: a Maven coordinate, a generated constant, the suffix a
  kind's principal type carries, and the typed evolution policy a rename asks
  for.

`docs/53-tool-crates.md` S53.8 asks whether what is left still needs to be a
crate.
