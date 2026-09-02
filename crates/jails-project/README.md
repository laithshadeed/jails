# `jails-project`

The reader. `capture` is the one place jails looks at a project: it fills a
`WorkspaceSnapshot` for the compiler, over the *intended* model so a command
that declares a thing sees the trees it needs, and its `observe` half produces
the `ProjectFacts` every command reads. `project::Project` is a root plus
those facts -- the one answer to "what is this project" -- and `documents`
holds the bounded, lossless adapters over the reader's build file, properties
and compose file, with `pom`, the one reader of `pom.xml`, beside them;
`merge` is the three-way merge they and the workspace share.

Every reader-owned file jails edits lives here too: `jails.toml` (`config`),
`compose.yaml` (`compose`), `build.gradle` (`gradle`), plus `inspect`
(`routes`, `beans`, `stats` read from source, never from a running context).

Reading Java lives here too, folded in from its own crate once nothing below
this one needed it:

- `java` -- a deliberately small reader: annotations and what they attach to, a
  type's supertypes, a constructor's parameters. Not a parser and must not grow
  into one.
- `classfile` -- the smallest reader that answers "which types does this class
  name": constant pool only. `CONSTANT_Long` and `CONSTANT_Double` take two
  pool slots.
- `template` -- `{{name}}` substitution into real `.java` files. A missing or
  unused key is a panic. Substitution only, never a template engine.

Every edit to a reader-owned file is surgical and leaves every other byte
alone. `jails.toml`'s `[layout]` keys are a closed set matching the eleven
layers, and `Config::layers()` is the one place a renamed layer is applied.
