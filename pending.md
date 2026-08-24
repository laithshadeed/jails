# What is pending

This replaces `plan.md`, `abstract.md` and `playground.md`, which were deleted.
They are in git history if the reasoning behind a decision is needed —
`git log --diff-filter=D -- plan.md` finds the commit that removed them, and
roughly 237 comments across the code still cite them by section number.

This file is only what is **not done**. What the code already is belongs in
`CLAUDE.md`; what it does belongs in `README.md`.

---

## 1. The V2 cutover — in flight, on branch `v2-dispatch-flip`

### What this is, plainly

jails has two engines for writing files into a project.

- **V1** is the direct one: each command opens files and writes them.
- **V2** is transactional: a command computes the whole change, takes a lock,
  writes it as one unit, and records enough to finish or undo an interrupted
  run.

V2 has been built and tested for a long time but was never switched on. The
remaining work is the switch.

**It has to happen all at once.** Both engines keep their bookkeeping in
`.jails/ledger.toml`, in formats neither can read. The moment `jails generate`
uses V2, `jails destroy` on V1 cannot read what it wrote. So every command
flips together, in one commit, or none do.

### Where it stands

The switch is made and the code builds. `main.rs` routes `generate`,
`destroy`, `add`, `remove`, `sync`, `rename`, `adopt`, `fmt` and the whole
`app` aggregate through the V2 engine, with `--pretend`, `--debug`,
`--no-start` and a new `--output human|json` honoured in one place. Because
the workspace denies dead code, the switch also forced V1's deletion:
`src/adopt.rs`, `src/app/reconcile.rs`, `src/app/shadow.rs` and V1's app-state
reader are gone.

**19 of 169 command-level tests still fail**, down from 50 when it first
compiled. That number is the honest measure of how much is left. Each failure
is one of two things:

- a test checking for V1's exact wording, which needs restating against what
  V2 says; or
- something V2 genuinely does not do yet, which needs fixing.

`main` is untouched and green. The branch cannot be merged until all 19 pass,
because a half-flipped tool is worse than an unflipped one.

### The 19, as of the last run

```
a_gradle_project_gets_the_commands_that_do_not_need_maven
add_db_no_start_skips_docker_compose_up
add_db_installs_postgres_flyway_and_testcontainers_without_an_orm
add_db_on_spring_migrates_the_global_initializer_to_an_import
app_apply_keys_a_suffixed_name_to_the_row_generate_writes
app_manifest_builds_the_crawler_skeleton_and_is_resumable
app_manifest_builds_the_support_inbox_from_the_same_generic_intents
app_manifest_merges_an_edited_intent_over_user_changes
app_manifest_refuses_an_intent_update_without_git_before_writing
app_manifests_compile_without_manual_source_edits
app_manifests_pass_the_full_generated_verification_gate
a_generated_command_is_reachable_by_name_through_jails_run
destroy_strategy_removes_the_implementations_it_did_not_name
generate_errors_on_duplicate_file
generate_field_updates_unchanged_derivatives_preserves_edits_and_adds_a_migration
generated_http_sink_delivers_typed_json_with_a_stable_idempotency_key
ledger_cli_manifest_builds_without_spring
remove_names_generated_files_that_were_edited_before_deleting_them
scaffold_refuses_to_silently_flatten_a_project_record_component
```

Two are diagnosed and not yet fixed:

- **`scaffold_refuses_to_silently_flatten_a_project_record_component`** — a
  scaffold referencing a project record looks up that record's declared field
  spec. V2 reads it from the schema-2 store now, but the refusal message says
  the record has no `@pk` when it plainly does, so the constraint is being
  lost somewhere between the stored spec and the check.
- **`generate_errors_on_duplicate_file`** — V1 refused a second identical
  `generate`; V2 treats it as a no-op, which is *better*. The test pins the
  old refusal and should be restated: a second identical run changes nothing,
  and a second run over an edited file preserves the edit.

### Rip out legacy support — decided, not yet done

**jails is not released, so there are no old projects to be compatible with.**
Everything below exists only to carry a schema-1 project forward and should be
deleted outright:

- `compat::translate` and the whole `MachineState::Legacy` path — a
  `.jails/ledger.toml` that is not schema 2 becomes an error telling the
  reader to delete `.jails` and regenerate.
- `LegacyEntry`, `legacy_after`, and every legacy row in the schema-2 ledger.
- `route::adopt_legacy`, and `jails adopt`'s `--legacy-key` / `--intent` /
  `--replace` / `--force` options. `adopt` goes back to being layout adoption
  only.
- `doctor`'s adoptable-row listing. On the four example applications this is
  **77 of 77 warnings** — every entity of a freshly generated project is
  reported as adoptable, purely because the binary still writes schema 1.
- `crates/jails-project/src/ledger.rs` (the schema-1 parser) and
  `generated_files`' fold of `.jails/app-state-v1`, `.jails/intents/*` and
  `.jails/models/*`.
- The schema-1 half of `generated_files::model_fields`.

This deletes far more than it adds and removes the single largest source of
noise in `doctor`. It is a separate commit from the cutover, and it comes
after it: none of the 18 failing tests is a legacy test, so doing it first
would be churn against a moving target.

### Still to do after the tests pass

1. **Two documentation changes.** A capability's properties no longer sit in a
   `# jails:<capability>` block — V2 owns each setting by key. That is a
   user-visible change to a file people edit, and it belongs in `README.md`
   and `CLAUDE.md` before it ships.
2. **Delete what is left of V1** in the library crates. The binary's copy is
   already gone; `jails-generate`'s `add::add`, `generate::destroy` and
   friends are still compiled because a library's unused public function is
   not dead code.
3. **Prove it on the four example applications** — regenerate
   `examples/{payments-gateway,support-inbox,web-crawler,ledger-cli}` through
   the flipped binary and confirm `jails check` is still green in all four.
4. **Hosted CI**, which has never been set up.

---

## 2. V1 against V2, as the cutover actually found them

Every row is a difference a failing test named, so this is the migration's
evidence rather than a design summary. It is also the checklist for finishing:
anything here that is not yet true of V2 is work.

| | V1 — the direct write path | V2 — the transaction protocol |
|---|---|---|
| **Where a write happens** | Wherever the command is. `add.rs` spliced the pom, `shrink.rs` deleted files, `generate` wrote and then ran side effects | One executor, from one prepared operation list. `tests/architecture.rs` holds the write-layer count at zero everywhere else |
| **Atomicity** | Per file. `rename` rewrote contents, then moved files, and its own comment admitted the half-applied state | One transition. A move is `Create`+`Delete` in one list, and an interrupted run is recoverable from the journal |
| **Bookkeeping** | `.jails/ledger.toml`, schema 1: recipe, name, package, files | Same path, schema 2: entities with owners and specs, one-shot receipts, keyed resources, guarded before-images. Unreadable to each other, which is why the flip is one commit |
| **What `--pretend` is** | A second walk that printed what it thought would happen | The same computation, stopped one step before the lock. There is no second function |
| **Reporting** | Each command printed as it went, in its own words | One value per command, rendered once: `--output human` or `--output json`, the same facts either way |
| **Re-running the same generate** | Refused: `already exists` | A no-op. The file is owned by the intent that wrote it, so nothing changed is nothing to do |
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
| **A pre-schema-2 project** | n/a | Translated in memory. Every schema-1 row becomes a *legacy* row with files and no owner, because the old format never recorded who asked for it |
| **Claiming a legacy row** | n/a | `jails adopt --legacy-key <key> --intent <kind>:<Name>`, with `--replace --force` for a row whose files have drifted. `doctor` prints the exact command per row |

### Three V1 behaviours deliberately not carried over

Each is an answer, not an oversight, but each is a loss and is recorded as one.

- **`destroy cases`** — above.
- **`adopt --manifest <path>`** — the specification has it, for claiming a row
  as owned by an application manifest rather than by the command line. The
  route has no manifest-owner path, so the flag is not wired at all rather
  than wired to nothing.
- **`remove`'s `changed since jails wrote` note** — V1 named which generated
  files had been edited before deleting them. The confirmation prompt now
  lists every deletion and takes no for an answer, which covers the risk;
  saying *which* of them was edited needs a warning the report does not carry.

---

## 3. Open defects in what jails generates

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

## 4. Not started, and open by design

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
