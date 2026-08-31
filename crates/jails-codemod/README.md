# `jails-codemod`

Surgical edits to text somebody else owns — and **no dependencies at all**, which is the entire point.

---

## Purpose & Overview

Three edits share this crate for one reason: there is nowhere in the workspace that cannot reach it. Every one of them has two engines performing it, on two ladders that cannot see each other, and a second copy of a surgical edit to a reader's file is a copy that drifts.

- **[`marked`](../../crates/jails-codemod/src/marked.rs)** — the `# jails:<marker>` … `# /jails:<marker>` block. This is how jails edits a file the reader owns, and what makes `remove` the exact inverse of `add`.
- **[`annotate`](../../crates/jails-codemod/src/annotate.rs)** — splicing and unsplicing an `@Import` on the `@SpringBootTest` classes already on disk, plus recognising one. Text in, text out.
- **[`tidy`](../../crates/jails-codemod/src/tidy.rs)** / **[`text`](../../crates/jails-codemod/src/text.rs)** — blank-line normalisation, and the blanking pass every scan here runs first.

---

## Why it is its own crate

Neither `jails-compiler` nor `jails-workspace` depends on `jails-project`, so a splice living there is unreachable from the canonical ladder and each crate that needs one writes its own `format!`. A crate with no dependencies is reachable from everywhere, which is the only arrangement in which one implementation can serve both ladders.

`tests/architecture/` fails on a `# jails:` literal outside this crate, counted against **`file.literals`** — not `file.production`. Blanked source has every string literal replaced by spaces, and a `# jails:` marker only ever appears inside one, so a gate reading blanked source reports zero whatever the code says.

---

## Two details that are load-bearing

- **`Marked::indented` exists** because a marker at column zero inside a YAML mapping is a parse error rather than a misplaced comment.
- **There is no `replace`.** Nothing needs one, and `remove` then `add` is the path `sync` takes.
- **`annotate` reads through blanked source**, so the `@SpringBootTest` inside `TestcontainersConfig`'s own Javadoc example is not mistaken for one on a class.
