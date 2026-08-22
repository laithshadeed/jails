# Workout validation scripts

Ten scripts, one per stacks workout. Each runs a sequence of `jails` commands
against a throwaway project and asserts on the Java that comes out.

**These are a spec, not a test suite.** A failing script means jails does not
have a feature yet — not that you did something wrong. As features land, more
checks go green.

```bash
./validation/01-normalise.sh          # one workout
./validation/01-normalise.sh --keep   # leave the project in /tmp to poke at
for f in validation/[0-9]*.sh; do "$f"; done   # all ten
```

Every script is self-contained: fresh `jails new-cli`, fresh temp dir, cleaned
up on exit. `lib.sh` holds the shared harness (`run`, `has`, `lacks`,
`exists`, `rejects`, `fixtures`, `build`, `verdict`).

## Current state

Run on 2026-08-14, after `g enum` / capitalized types / `!`/`?` landed:

| Workout | Failures | Blocked on |
| --- | ---: | --- |
| 01 normalise | **0** | — |
| 02 reconcile | 9 | `list<T>` |
| 03 confidence | 6 | `list<T>` |
| 04 cash application | 13 | `list<T>`, `instant`, `Json.readJsonl` |
| 05 sqlite | 18 | `g repo` |
| 06 rest api | 23 | `g handler`, `map<K,V>` |
| 07 vat | 10 | `list<T>`, `instant` |
| 08 intercompany | 11 | `list<T>`, `map<K,V>`, `instant` |
| 09 variance agent | 25 | `g sealed`, `list<T>`, `map<K,V>` |
| 10 orchestrator | 25 | all of the above |

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
