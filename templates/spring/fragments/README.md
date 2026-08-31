# Structural fragments

Not whole Java files: each is a *hole filler* for a template beside them, so
the extension is `.java.txt` rather than `.java` — a gate that scans
`templates/**.java` expecting a compilable unit would be reading a method body
here.

They live as files rather than as Rust constants because **both engines render
the same holes**, and CLAUDE.md records what two copies of one generated block
cost: `templates/add/` is shared for exactly this reason, and the two copies
that were not drifted on pinned action SHAs where nobody looks.

Structural variation still stays in Rust — the decision *whether* a hole is
filled is a `bool` in the emitter, per `template.rs`'s rule that this is
substitution and not a template engine. Only the text moved.
