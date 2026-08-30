# Workout validation scripts

Ten scripts, one per stacks workout. Each runs a sequence of `jails` commands
against a throwaway project and asserts on the Java that comes out.

**These are a spec, not a test suite.** A failing script means jails does not
have a feature yet — *or* that the script asks for something jails has since
decided is wrong. Both happen now; the 2026-08-30 run below has four of the
second kind. Read the refusal before changing jails.

```bash
./validation/01-normalise.sh          # one workout
./validation/01-normalise.sh --keep   # leave the project in /tmp to poke at
for f in validation/[0-9]*.sh; do "$f"; done   # all ten
```

Every script is self-contained: fresh `jails new-cli`, fresh temp dir, cleaned
up on exit. `lib.sh` holds the shared harness (`run`, `has`, `lacks`,
`exists`, `rejects`, `fixtures`, `build`, `verdict`).

## Current state

Run on 2026-08-30. **Failures are split by cause**, because the previous table
counted them together and a reader could not tell a missing feature from a
missing JDK:

| Workout | Real | Environmental | Blocked on |
| --- | ---: | ---: | --- |
| 01 normalise | **0** | 2 | — |
| 02 reconcile | **0** | 2 | — |
| 03 confidence | **0** | 2 | — |
| 04 cash application | **0** | 2 | — |
| 05 sqlite | 8 | 2 | `g repo` names its adapter `Jdbc<Name>Repository`; these expect `Sqlite<Name>Repository` |
| 06 rest api | **0** | 2 | — |
| 07 vat | **0** | 2 | — |
| 08 intercompany | **0** | 2 | — |
| 09 variance agent | **0** | 2 | — |
| 10 orchestrator | 1 | 2 | the same `g repo` naming |

The two environmental failures in every workout are the same pair on any
machine without the full toolchain: `mvn test` needs a JDK matching
`pom::TARGET_RELEASE` (26; this run had 21, so every build stopped at
`release version 26 not supported`), and `fixtures` reads `stacks/fixtures/`,
which is an untracked sibling checkout like `deps/`. Neither says anything
about jails. **Re-measure on a box with both**, or the "real" column is the
only column that means anything.

### What changed since 2026-08-14

The old table read 9, 6, 13, 18, 23, 10, 11, 25, 25 failures against
`list<T>`, `instant`, `map<K,V>`, `g sealed`, `g handler` and `Json.readJsonl`.
**All of those shipped**, which is what the next section is about, and the
count went to nine.

Four of the remaining failures were **this directory being stale, not jails**,
which inverts the assumption at the top of this file. Each was jails having
grown a refusal these scripts predate, and each fix is the one jails' own
`fix:` line names:

- workout 04 declared `from` and `to`, and 06 declared `offset` and `limit`.
  All four are PostgreSQL reserved words and would make the generated SQL
  invalid; they are `movedFrom`/`movedTo` and `skip`/`take` now.
- workout 10 declared a record called `Override`. Inside its own package that
  shadows `java.lang.Override`, which every Java file imports implicitly, so
  an `Override` component would be typed as the record being declared. It
  compiles, which is why nothing downstream would report it. It is
  `DecisionOverride` now.
- workout 05 ran `g repo Transaction` against a record it had declared as
  `CanonicalTransaction`, three lines under a comment saying the argument *is*
  the entity it stores. jails refused a name that was never declared.

**So "a failing script means jails does not have a feature yet" is no longer
safe to assume** -- it can equally mean the script asks for something jails
has since decided is wrong. Read the refusal before changing jails.

The nine that remain are one gap, stated once: `g repo` emits a single JDBC
adapter named `Jdbc<Name>Repository` whatever the dialect, and these workouts
were written expecting the dialect in the name. That is a real product
question -- one adapter or one per dialect -- not a stale expectation, so it
is left failing rather than renamed away.

## The features these scripts assumed — all seven have shipped

This section used to rank seven missing features by how many workouts they
unblocked. **Every one of them now exists**, which is what the scripts were
for; the list is kept because the *reasons* are still the design, and because
"a spec, not a test suite" only stays honest if the spec says what is done:

| Assumed feature | Where it lives now |
|---|---|
| `list<T>` field types | `generate/field.rs`, resolved recursively; the component is `List.copyOf`'d and defaults to empty |
| `map<K,V>` field types | same, both parameters resolved through the normal rules |
| `instant` field type | the builtin table in `generate/field.rs`; `datetime` still means `LocalDateTime` |
| `jails g repo <Name>` | `generate/repository.rs` — port, derived JDBC adapter, real-database IT |
| `jails g sealed <Name> <Variant>...` | `generate/domain.rs`, beside its open counterpart `g strategy` |
| `jails g handler <Name>` | `generate/web.rs`, over the JDK server `add http` sets up |
| `Json.readJsonl` | `add json` (`add/data.rs`), pinned by a unit test |

The syntax question below was decided in favour of `list<T>`: quoting at a
prompt is the cost, and it reads best in a manifest, which is where field
specs mostly live now (`.jails/app.toml`). `g repo` reads the record on disk
when it is given no fields, which is the rule §9.4 of `plan.md` generalises.

**What these scripts do *not* cover** is everything added since: `usecase`,
`query`, `transition`, `association`, `durable-job`, `http-workflow`,
`http-sink`, `fetcher`, `cases`, and the capabilities beyond `sqlite`/`json`/
`http`. The four proof applications under `examples/` are where those are
exercised end to end.

## Design notes worth deciding before building

**`<` and `>` need shell quoting.** Writing `matched:list<Match>` unquoted is a
shell redirect — every script here had to quote those arguments, and every
user will hit it. Alternatives that need no quoting: `matched:list:Match`,
`matched:Match...`, or `matched:Match+`. The current scripts use `list<T>`
because it reads best in a file; if you would rather optimise for typing at a
prompt, change the syntax and re-run these — that is what they are for.

**`g repo` needs to know the entity's fields.** It takes only a name, on the
assumption it reads the already-generated record. If that turns out awkward,
the alternative is passing fields again, which duplicates them.

**Assertions are regex over generated source.** Cheap and readable, but they
check shape, not behaviour — `build` (a real `mvn test`) is what proves the
output actually works. When a check fails, read the generated file before
trusting the message.
