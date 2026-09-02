# `jails-codemod`

Surgical edits to text somebody else owns, with **no dependencies at all** so
every crate on either ladder can reach it.

- `marked` -- the `# jails:<marker>` ... `# /jails:<marker>` block, which is how
  jails edits a file the reader owns and what makes `remove` the exact inverse
  of `add`. `Marked::indented` exists because a marker at column zero inside a
  YAML mapping is a parse error.
- `annotate` -- the `@Import(...)` splice into a `@SpringBootTest` class, read
  through blanked source so an annotation inside a Javadoc example is not taken
  for one on a class.
- `dispatch` -- registering a generated command in a project's own CLI, found by
  shape (`is_dispatcher`) rather than by filename.
- `text` -- `blanked`, which replaces comments and string literals with spaces
  of the same length so a scan cannot be fooled and byte offsets still index the
  original.

`tests/architecture/` fails on a `# jails:` literal outside this crate.
