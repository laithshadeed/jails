# Sanitized real-project corpus

Projects **jails did not create**, checked in as bytes, run through both
implementations by `tests/differential.rs`.

`simplify-sol.md`'s G5 asks for "sanitized adopted and reader-edited
Spring/plain projects", each running legacy and new plan/apply plus semantic
comparison and rerun. `tests/common/mod.rs::write_adopted_fixture` already
covers one adopted shape as a Rust table. This directory is for the ones that
are easier to check in than to escape into a string literal, and — the point —
**it grows without touching Rust**: drop a directory in, add its row to
`policy.tsv`, and the differential test picks it up.

## What a good entry is

A generator can be perfectly correct about its own layout and wrong about
somebody else's. So an entry earns its place by being foreign in a way the
others are not — a nesting, a naming, an absence — rather than by being one
more well-formed project.

- **Foreign in every respect a generator might assume**: its own groupId,
  artifactId and package root; directories jails would not have chosen;
  classes with bodies rather than stubs.
- **Declaring nothing jails is supposed to declare.** `spring-nested-adapters`
  deliberately omits `spring-boot-starter-webmvc-test`: a fixture that supplies
  what the tool must supply hides exactly the defect these exist to find.
  `CLAUDE.md` records the months that one cost.
- **`{TARGET_RELEASE}`** in a pom is substituted at copy time, so an entry does
  not go stale when the default release moves.

## Sanitizing

These are meant to come from real codebases. Before checking one in: no
credentials, hostnames, customer names or internal URLs; rename packages and
artifacts to `example`/`acme` coordinates; keep only enough files to carry the
shape being tested. The point is the *shape*, never the content.

## Adding one

1. `mkdir tests/corpus/<name>` and put the tree in it.
2. Add a row to `policy.tsv` saying what it exercises. A directory with no row,
   or a row with no directory, fails
   `the_corpus_policy_covers_every_checked_in_project`.
3. Run `cargo test --test differential corpus`.

`flavour` is `spring` or `plain` and decides nothing except whether `beans` is
run — jails reads the build file itself.

The `adopt` column is `;`-separated, because one real tree means several things
at once: it renames two layers, holds a third directory jails cannot classify,
and has two candidates for a fourth. An earlier version took one expectation
per entry, so an entry could state a quarter of what it was checked in to prove
and the rest sat in the prose column where nothing reads it.

- `records:<key>=<value>` — a `[layout]` row adoption must write.
- `reports:<dir>` — adoption must name `<dir>` **and must not record it**.
  Both halves matter: naming a directory and then writing it into `[layout]`
  anyway reads as diligence and behaves as a coin toss.
- `nothing` — adoption has nothing to learn, refuses, and writes no file.
  Stands alone.

## What is here

| entry | the shape it is for |
|---|---|
| `spring-nested-adapters` | a layer nested two deep, which adopt cannot discover and must report |
| `plain-flat-package` | no layer directories at all — nothing to learn, and it has to say so |
| `spring-renamed-layers` | the ordinary case: two known synonyms recorded, one unknown name reported |
| `spring-two-web-directories` | two candidates for one layer — neither written, both named, and the *other* layer still recorded |
| `gradle-kotlin-dsl` | no `pom.xml` at all; the door is any recognised build marker |
