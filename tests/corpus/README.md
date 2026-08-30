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

`flavour` is `spring` or `plain` and decides nothing except which assertions
apply — jails reads the pom itself.
