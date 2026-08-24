# What is pending

This replaces `plan.md`, `abstract.md` and `playground.md`, which were deleted.
They are in git history if the reasoning behind a decision is needed —
`git log --diff-filter=D -- plan.md` finds the commit that removed them, and
roughly 237 comments across the code still cite them by section number.

This file is only what is **not done**. What the code already is belongs in
`CLAUDE.md`; what it does belongs in `README.md`.

---

## 1. The V2 cutover — done

### What this is, plainly

jails had two engines for writing files into a project.

- **V1** was the direct one: each command opened files and wrote them.
- **V2** is transactional: a command computes the whole change, takes a lock,
  writes it as one unit, and records enough to finish or undo an interrupted
  run.

V2 had been built and tested for a long time but was never switched on. That
switch is made, and V1 is deleted.

**It had to happen all at once**, which is why it is worth recording how. Both
engines kept their bookkeeping in `.jails/ledger.toml` in formats neither could
read, so the moment `jails generate` used V2, `jails destroy` on V1 could not
read what it wrote. Every command flipped in one commit.

### Where it stands

**The switch is made and the whole suite is green** — 169 command-level tests
and the full workspace. `main.rs` routes `generate`, `destroy`, `add`,
`remove`, `sync`, `rename`, `adopt`, `fmt` and the whole `app` aggregate
through the V2 engine, with `--pretend`, `--debug`, `--no-start` and a new
`--output human|json` honoured in one place. Because the workspace denies dead
code, the switch also forced V1's deletion from the binary: `src/adopt.rs`,
`src/app/reconcile.rs`, `src/app/shadow.rs` and V1's app-state reader are gone.

Getting from 50 failures to zero fixed real defects rather than restating
assertions. The ones worth knowing about, because each names a rule the next
recipe will meet too:

- **A `@SpringBootTest` gets its container import at birth.** V1 spliced
  `@Import(TestcontainersConfig.class)` into every such test on disk and caught
  later ones on a second reconciliation pass. Under the protocol the row that
  owns a file decides its bytes, so the *capability writing the test* puts the
  import in. That deleted the second pass — and with it a second `spotless`
  run and an empty transaction.
- **Two migrations in one transition both came out `V001`.** Serial numbers
  were computed from the directory. `migration_file` computes them from the
  projection, keyed on the description, so a re-apply finds the file it already
  wrote instead of renumbering it.
- **Every JDBC round-trip test was emitted `@Disabled`** in exactly the
  projects that had asked for a database: the recipe checked disk for
  `TestcontainersConfig.java`, and in an `app apply` the whole manifest is one
  transition. Same fix, same rule — read the projection, not the directory.
  Thirteen skipped integration tests across the three proof applications.
- **`generate cli` moved the packaged entry point with a `std::fs` write after
  the plan.** It is `SemanticEdit::MavenMainClass` now, carrying the entry point
  it displaces so `destroy` can put it back. Without it a manifest that
  generated a CLI and registered its commands produced a jar answering only
  `help`.
- **`destroy strategy` sweeps implementations it never named.** A strategy is
  an interface plus a bean per implementation, and `destroy` is given no
  variant list; an implementation written by hand afterwards is still one of
  the strategy's classes, and leaving it behind implementing a deleted
  interface stops the project compiling. Those are `absences` with `force`,
  which is exactly the flag for "the bytes are not jails'", and the deletion
  prompt is the human ask it requires.

Three tests were restated rather than fixed, each because V2's answer is the
better one:

- **A second identical `generate` is a no-op, not `already exists`.** The file
  is owned by the intent that wrote it, so "nothing changed" is the honest
  answer — and an edited file is three-way merged rather than refused.
- **`docker compose --file` is handed the committed object**, not the live
  `compose.yaml`. The effect runs after the commit publishes; running against
  what somebody edited in between would start services this transition never
  described, and a retry would not repeat the first attempt.
- **The `spring.factories` migration is gone.** It deleted a registration *an
  earlier jails* had written. There is no earlier jails.

### Legacy support — ripped out

**jails is not released, so there were no old projects to be compatible with.**
Everything that existed only to carry a schema-1 project forward is gone, and
so is the direct write path it was welded to:

- `compat::translate`, `MachineState::Legacy` and `crates/jails-project/src/ledger.rs`
  (the schema-1 parser). A `.jails/ledger.toml` this binary cannot decode is an
  **error** now, naming the file and saying it was written by a different
  jails.
- `LegacyEntry`, `LegacyKey`, `LegacySourceKind`, `legacy_after`, and the
  `legacy` table in the schema-2 ledger. `SpecPresence` went with them —
  "unknown origin" was a schema-1 answer.
- `LegacySourcePath` / `LegacyFileName` / `LegacyDirectoryKind`, the
  `LegacyMachine` operation target, `LegacyMigrationIdentity` and the whole
  `jails-prepare::migration` module. Nothing under `.jails/` is a plan's target
  any more, so `OperationTarget` collapsed into `ProjectPath`.
- `route::adopt_legacy` and `jails adopt`'s `--legacy-key` / `--intent` /
  `--replace` / `--force`. `adopt` is layout adoption only. The `claimed` path
  set went with it — it existed so that one command could write over a file
  jails had not written, and nothing else ever set it.
- `doctor`'s adoptable-row listing: **77 of 77 warnings** on the example
  applications, every one of them because the binary still wrote schema 1.
- `generated_files`' registry half and its fold of `.jails/app-state-v1`,
  `.jails/intents/*` and `.jails/models/*`. What is left is one function:
  which fields a recorded intent declared.

V1 itself went in the same commit, because the two could not be separated —
`generated_files` *was* the schema-1 registry, and V1's write path *was* its
only writer. `add::add`/`add_in`, `generate::generate_in_project`,
`generate/remove.rs`, `add/shrink.rs` and `add/test_wiring.rs` are deleted.
`add::preflight_in` survives, re-expressed over the pure planner: it is what
makes `jails add db security` refuse before either is installed.

### Still to do

**Hosted CI**, which has never been set up. That is the only item left from the
cutover.

The four example applications are proved by the suite rather than by hand:
`SPRING_APP_MANIFESTS` and `ledger_cli_manifest_builds_without_spring`
`include_str!` the four `examples/*/.jails/app.toml` files directly, generate
from them, and run the full Maven gate — so a manifest that stopped building
fails `cargo test`. Verified once by hand as well: a fresh `jails new-cli
--app examples/ledger-cli/.jails/app.toml` gives `doctor` "15 checks, all
clear" (it was 15 plus 77 adoptable-row warnings) and `jails check` BUILD
SUCCESS. The six skipped tests there are `g cases` stubs, `@Disabled` on
purpose because they name what the reader has to write.

---

## 2. Gradle and Maven parity

**Maven stays the default.** `jails new` creates a Maven project and should go
on doing so; parity is about jails *working on* a Gradle project somebody else
created, which is the case `minicom-public/spring` is.

Landed: `gradle.rs` reads and splices a Groovy `build.gradle`; `Build::Gradle`
means "jails can read this"; `add`, `generate`, `doctor`, `about`, `build`,
`clean`, `check`, `test` and `run` all work. `build.gradle.kts` and a root
holding only `settings.gradle` stay `Foreign` on purpose.

Still Maven-only, roughly in the order they hurt:

| what | why it is not portable yet |
|---|---|
| ~~`*IT` tests never run~~ | **Done.** A Failsafe claim renders a marked `integrationTest` task wired into `check`, with `test` excluding `*IT` so they do not run twice. Verified against real Gradle: `> Task :integrationTest FAILED` on a deliberately failing `*IT` |
| ~~`add coverage`~~ | **Done.** JaCoCo ships with Gradle, so there is no version to pin and no `plugins {}` block to reach into |
| ~~`jails watch`~~ | **Done.** Gradle's own `--continuous bootRun` rather than the devtools loop: continuous mode re-runs a task when an input changes and needs nothing added to the build. The two compose where the project has devtools |
| ~~`jails mvn`~~ | **Done.** `jails gradle` is the sibling, and each refuses the other's project by name |
| ~~`test --failed`, `--json`, `--slowest`~~ | **Done.** They read the same document: Gradle's `Test` task writes Surefire's JUnit XML schema, under `build/test-results/<task>/`. `surefire.rs` is `reports.rs` and reads both. One difference, and it is the plausible-wrong-answer kind: Gradle writes `name="passes()"` with the parentheses, so an untrimmed selector matches nothing and `--failed` would run zero tests and report success. Verified against Gradle 9.6.1 |
| `add format` / `jails fmt` | Spotless is spliced as a Maven plugin, and Gradle's `com.diffplug.spotless` needs a *version* and an entry in the `plugins {}` block -- the one feature that cannot be a self-contained appended block. `add format` **refuses** on Gradle rather than recording itself installed having written nothing |
| `testd`, `test --fast`, `test --affected`, `jails console` | All need a resolved classpath, which jails gets from `dependency:build-classpath`. Gradle has no equivalent without adding a task to the build -- and adding one to a file the reader owns, for a convenience, is a different bargain from splicing a dependency they asked for |

`Change.plugins` is still `(artifact_id, xml_block)` -- a Maven plugin with
Maven's syntax baked in -- and `ResourceKey::MavenPlugin` still keys the claim
by a coordinate Gradle does not resolve. **That is a naming debt, deliberately
taken.** `gradle::feature_of` maps the coordinate onto what the plugin *does*,
which is total for the closed set jails emits and `None` for anything else, so
the behaviour is right and only the name is wrong. Renaming the key to the
feature is a protocol change across five files; it buys no behaviour and can
be done whenever the churn is convenient.

The rule that makes the debt safe: a plugin with no known Gradle equivalent
**refuses the whole capability**, so nothing is ever half-installed on the
strength of a name jails half-recognised.

---

## 3. V1 against V2, as the cutover actually found them

Every row is a difference a failing test named, so this is the migration's
evidence rather than a design summary. V1 no longer exists; the table is kept
because each row is a decision somebody may want the reason for.

| | V1 — the direct write path | V2 — the transaction protocol |
|---|---|---|
| **Where a write happens** | Wherever the command is. `add.rs` spliced the pom, `shrink.rs` deleted files, `generate` wrote and then ran side effects | One executor, from one prepared operation list. `tests/architecture.rs` holds the write-layer count at zero everywhere else |
| **Atomicity** | Per file. `rename` rewrote contents, then moved files, and its own comment admitted the half-applied state | One transition. A move is `Create`+`Delete` in one list, and an interrupted run is recoverable from the journal |
| **Bookkeeping** | `.jails/ledger.toml`, schema 1: recipe, name, package, files | Same path, schema 2: entities with owners and specs, one-shot receipts, keyed resources, guarded before-images. Unreadable to each other, which is why the flip is one commit |
| **What `--pretend` is** | A second walk that printed what it thought would happen | The same computation, stopped one step before the lock. There is no second function |
| **Reporting** | Each command printed as it went, in its own words | One value per command, rendered once: `--output human` or `--output json`, the same facts either way |
| **Re-running the same generate** | Refused: `already exists` | A no-op. The file is owned by the intent that wrote it, so nothing changed is nothing to do |
| **`generate cli` and the packaged jar** | Rewrote `<mainClass>` with a `std::fs` call after the plan, so nothing recorded it | `SemanticEdit::MavenMainClass`, carrying the entry point it displaces so `destroy` restores it |
| **`destroy strategy`'s unnamed implementations** | Walked the domain directory and deleted them | Same sweep, as `absences` with `force` — the bytes are not jails', so the deletion prompt is the ask that authorises it |
| **The container import in a generated test** | Spliced into every `@SpringBootTest` on disk, plus a second pass for ones written later | Written by the row that owns the file, at birth. No second pass, so no second `spotless` run and no empty transition |
| **Starting compose** | `docker compose --file compose.yaml`, whatever it says by then | `--file <committed object>`. The effect runs after the commit publishes, and a retry has to repeat what the first attempt did |
| **Re-running over an edited file** | Refused, or clobbered | Three-way merged against the recorded base. Only a genuine overlap refuses |
| **`destroy` with no record** | Recomputed the paths by offering each generator argument shapes | Refuses, and names the command that would have recorded it. Guessing at paths is how files nobody wrote get deleted |
| **`destroy migration`/`association`/`field`** | Refused: forward-only | Same, decided before any lookup so the reason is forward-only rather than "not recorded" |
| **`destroy cases`** | Rebuilt the test path from the markdown path | Refuses. A one-shot is a receipt over the source's bytes and the schema has no list for taking one back; regenerating from the same brief is already a no-op |
| **Confirmation before deleting** | Asked from inside `destroy`, and again from inside `remove`, over two hand-built path lists | Asked once, of the plan, at the dispatch point. What you are shown is exactly what saying yes does |
| **A capability's properties** | One `# jails:<capability>` block, spliced and deleted whole | One claim per key. No markers; `remove` retires the keys it owns and leaves the reader's alone |
| **A property's comment** | Left behind when the key went | Removed with its key — but only when byte-identical to what jails wrote |
| **The last claim leaving a file** | Special-cased per capability | One rule: an empty file is not one anybody keeps, so it goes with the last claim |
| **`add a b c`** | Preflighted all three, then applied all three | Preflights all three, then one transition each. A refusal still lands before any is installed |
| **`add format`** | Shelled out to `spotless:apply` after its own write path | A second transition — the same one `jails fmt` is — so the formatter runs in a scratch tree and commits only what it changed |
| **A deleted source's `.class`** | Swept by `shrink.rs`, per capability | Swept from the receipt's own delete list, so every route gets it and no route knows about `target/` |
| **`app plan`** | A separate walk printing `pending`/`update`/`applied` per row | `app apply --pretend`. It names files, not rows, and an entity that changes nothing is not listed |
| **A store this binary cannot decode** | Read by whichever parser matched | An error naming the file. There is one format, so a ledger jails cannot read was written by a different jails — guessing at an older schema and translating what it thought it found is how a wrong answer looks right |

### Three V1 behaviours deliberately not carried over

Each is an answer, not an oversight, but each is a loss and is recorded as one.

- **`destroy cases`** — above.
- **`remove`'s `changed since jails wrote` note** — V1 named which generated
  files had been edited before deleting them. The confirmation prompt now
  lists every deletion and takes no for an answer, which covers the risk;
  saying *which* of them was edited needs a warning the report does not carry.

---

## 4. Open defects in what jails generates

Found by generating four production applications and running them. Everything
else that exercise found is fixed.

- **A `@unique` violation answers 500, not 409.** Create a resource, then
  create another with the same value in its unique column. jails put that
  constraint in the schema and `add api` generates an `ApiException.Conflict`
  documented "Becomes a 409" — nothing connects the two, so a duplicate reads
  as the server breaking. 5xx is what alerting pages on and what clients
  retry, so a duplicate becomes an incident and then a retry storm.

  It is not a one-line handler. `DuplicateKeyException` arrives with the JDBC
  stack; `ApiExceptionHandler` is written by `add api`, which does not require
  a database. An unconditional arm hands an `api`-without-`db` project a
  compile error for a file it did not write. The fix needs a conditional arm
  plus a pass that revisits `api` after `db` lands — `app apply` already
  reconciles twice for this reason, `jails add api` then `jails add db` does
  not. **Decide that ordering contract before writing the handler.** The
  generated controller test is where the assertion goes.

- **Generated business behaviour is still unwritten, by design.** The ledger
  match rules and the Kafka listeners in every generated application contain
  the application-specific reaction nobody has written, so the ledger does not
  reconcile and a received event drives nothing. That is the honest boundary
  of a scaffolding tool. The open question is whether the declarative manifest
  can be extended far enough to generate those decisions, or whether they are
  properly the reader's code.

- **Deferred maintenance:** the JSON sample table and the field-type
  vocabulary are two lists of the same types. They were five apart, which is
  how a `uri` component came to document a request its own record refuses.
  One table would close it.

---

## 5. Not started, and open by design

- **Conflicted merges cannot be resumed.** When a regeneration and a reader's
  edit genuinely overlap, the three-way merge produces conflict markers. The
  specification commits those with a frozen record the next invocation
  continues or aborts. The bytes are produced and validated; the frozen
  record, the refusal while it stands, and the continue/abort commands do not
  exist. jails refuses instead, naming the hunk count. **It lands as one piece
  or not at all** — a project that can enter a conflicted state and not leave
  it is worse than one that refuses the merge. Building the enter side alone
  was tried and backed out.

- **`generate cli` retargets the POM's `<mainClass>`** with a direct write
  after the plan — the last instance of the shape the cutover exists to
  remove. It needs a keyed claim in the protocol, which is a schema addition
  and therefore a specification change first.

- **Unmeasured:** the k6 load profile `add loadtest` writes has never been
  run, so the p99 claim is unmeasured and says so. Spring context-cache misses
  across the example applications have never been counted.

- **Anti-goals**, unchanged: domain-specific generators, executable plugin
  hooks, a conditional template language, an ORM or a runtime support jar,
  silent Gradle support, an embedded model server, incremental `check`, or
  treating a skipped test as coverage.
