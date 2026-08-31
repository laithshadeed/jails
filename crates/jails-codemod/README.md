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

It lived in `jails-project` until 2026-08-29, and neither `jails-compiler` nor `jails-workspace` depends on that crate — so three *more* implementations of the marked block had appeared there. They were not careless: reuse was structurally unavailable, so the fourth, fifth and sixth `format!` were forced.

The gate that was supposed to stop exactly this had never been able to. It counted blanked source, where a `# jails:` literal has already been replaced by spaces, so it read zero whatever the code said — a vacuous gate and a held line print the same word.

`tests/architecture/` now fails on a `# jails:` literal outside this crate, counted against `file.literals`.

---

## Two details that are load-bearing

- **`Marked::indented` exists** because a marker at column zero inside a YAML mapping is a parse error rather than a misplaced comment.
- **There is no `replace`.** Nothing needs one, and `remove` then `add` is the path `sync` takes.
- **`annotate` reads through blanked source**, so the `@SpringBootTest` inside `TestcontainersConfig`'s own Javadoc example is not mistaken for one on a class.
