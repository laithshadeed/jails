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

## The features these scripts assume

Ranked by how many workouts they unblock. None of them knows what an
"accounting transaction" is — every one is ordinary Java plumbing.

### 1. `list<T>` field types — needed by 2, 3, 4, 7, 8, 9, 10

```bash
jails g value ReconcileResult 'matched:list<Match>' 'unmatchedBank:list<string>'
```

The single biggest blocker. Every workout from 2 onward returns a record of
grouped buckets, and there is currently no way to declare one.

Element types resolve through the existing rules: lowercase through the field
table, capitalized as a type you own. The generated component must be
`List.copyOf`'d (a record holding a caller's mutable list is not immutable)
and default to empty rather than null (no consumer should have to null-check a
bucket). A bare `list` or an unknown element type is an error, not
`List<Object>`.

### 2. `map<K,V>` field types — needed by 6, 8, 9, 10

```bash
jails g value ApiError code:string! message:string! 'details:map<string,string>'
```

Same treatment: `Map.copyOf`, empty default, both type parameters resolved
through the normal rules.

### 3. `instant` field type — needed by 4, 7, 8, 10

Audit timestamps are `Instant`, not `LocalDateTime`. A moment on a global
timeline is not a wall-clock reading, and `datetime` already takes the latter,
so this needs its own token. Roughly a one-line addition to the field table.

### 4. `jails g repo <Name>` — needed by 5, 10

Emits the port/adapter pair, which is otherwise three hand-written files every
time:

- `app/<Name>Repository.java` — interface, no `java.sql` in any signature, so
  the application layer stays persistence-ignorant
- `adapters/Sqlite<Name>Repository.java` — implements it over plain JDBC,
  `PreparedStatement` throughout, try-with-resources
- a companion test round-tripping against the in-memory database `add sqlite`
  already provides

No ORM — the gym bans them, and `add sqlite` is already framework-free.

### 5. `jails g sealed <Name> <Variant>...` — needed by 9, 10

```bash
jails g sealed ToolOutcome Succeeded Failed TimedOut
```

A sealed interface plus one record per variant. This is the case an enum
cannot cover: a closed set where each case carries different data. The payoff
is exhaustiveness — a `switch` over it fails to compile when a variant is
added later. The gym's own `Result<T,E>` has exactly this shape.

### 6. `jails g handler <Name>` — needed by 6, 10

An `HttpHandler` on the JDK server `add http` already sets up, wired to a
service passed in the constructor (so CLI and HTTP share one code path),
returning a shared error envelope, plus an integration test that drives it
over a real loopback socket on an ephemeral port.

### 7. `Json.readJsonl` — needed by 4, 5

One JSON object per line, blank lines skipped, returning `List<JsonNode>`. An
event log is the canonical JSONL case and both those workouts take `.jsonl`
input. Small addition to the existing `add json` capability.

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
