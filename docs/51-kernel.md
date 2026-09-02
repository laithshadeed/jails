<!--
One of six. `docs/50-simplify.md` is the brief every agent reads first; it
carries the baseline, the ownership table and rules R1-R9. Nothing here
repeats them.

**A closed item is deleted from this file**, in the commit that closes it.
Item numbers `S51.n` are stable and never reused.
-->

# 51 — Kernel: delete the legacy transaction kernel

**Read `docs/50-simplify.md` first.** You are agent 1. Your subject is the
strangler's other half: the transaction kernel the canonical executor
replaced, the vocabulary crate that existed to feed it, the codec that
serialised it, and every reader of the ledger it kept. **None of it is
reachable from a project this binary can create**, and the plan is to prove
that, move the six things that survive, and delete the rest.

## What you own

`crates/jails-prepare/**`, `crates/jails-commit/**`, `crates/jails-state/**`,
`crates/jails-protocol/**`, `crates/jails-codec-derive/**`,
`crates/jails-support/src/codec.rs` and `codec/`, `crates/jails-spec/**` (the
landing zone for what survives), `src/dispatch.rs`,
`crates/jails-report/src/lifecycle_status.rs`,
`crates/jails-report/src/managed_drift.rs`, the ledger half of
`crates/jails-report/src/schema_lineage.rs`, `tests/protocol-golden/**`,
`scripts/verify-rewrite-g1-canary.sh` and
`.github/workflows/verify-rewrite-g1-canary.yml`, the `JAILS_LEGACY_BIN`
machinery in `tests/product_loop.rs`, `docs/30-cutover.md`, the "Legacy
workspace during cutover" section of `CLAUDE.md`, and the workspace
`Cargo.toml` `members` list for the crates you remove.

## What you do not touch

`src/**` beyond `src/dispatch.rs` is agent 2's; the surviving tool crates are
agent 3's; `jails-model` is agent 4's; the compiler is agent 5's. R2 lets you
edit a call site of a symbol you delete and nothing else.

## Baseline

| | production | raw | tests |
|---|---:|---:|---:|
| `jails-prepare` | 6,647 | 10,892 | 115 |
| `jails-protocol` | 8,556 | 17,013 | 238 |
| `jails-commit` | 2,558 | 4,955 | 62 |
| `jails-codec-derive` | 243 | 323 | 1 |
| `jails-state` | 92 | 271 | 6 |
| `jails-support::codec` | 555 | -- | -- |
| ledger readers in `jails-report` | ~1,000 | -- | -- |

The reachability facts, each a command you re-run before you start:

```
grep -rn 'jails_prepare\|jails_commit' src crates/*/src --include=*.rs \
  | grep -v '^crates/jails-prepare/\|^crates/jails-commit/'
grep -rln 'ledger.toml' src crates/*/src
grep -rhoE 'jails_protocol::[a-z_]+' src crates/jails-{drive,report,project,generate,spec,state,support}/src | sort | uniq -c | sort -rn
```

On 2026-09-02 the first prints six lines in `src/dispatch.rs` (the JSON error
envelope), two in `crates/jails-drive/src/migrate.rs` (a frozen migration read
out of the object store) and one in `managed_drift.rs`. The second finds no
writer outside `jails-commit`. The third is the list in S51.2.

## Steps

**S51.1 -- Prove nothing writes a ledger, and pin it.** `Store::at` is the
only constructor and `create_subdirectories` its only writer; find every
caller and confirm each is on a path the binary cannot reach from a modelled
project. Then add one test in `tests/cli` that runs `jails new`, `jails g
record`, `jails add db` and `jails sync` in a scratch project and asserts no
`.jails/ledger.toml`, `.jails/objects` or `.jails/receipts` exists afterwards.
That test outlives everything below; it is the claim the deletion rests on.

**S51.2 -- Move what survives of `jails-protocol` into `jails-spec`, first.**
The third grep above says what the surviving crates still take from it.
Measured 2026-09-02, by symbol:

| symbol | used by | goes to |
|---|---|---|
| `identity::*` (33 uses) | java, report, drive | already `jails-support::identity`; point callers there and delete the re-export |
| `database::*` (26; `MigrationInputV1`, `SchemaObjectKind`, `SchemaProvenance`, the catalog) | drive `live_sql`/`migrate`/`datasource`, project `query_*`/`schema`, root `sql_command`/`schema_command` | `jails-spec::database` |
| `entity`, `snapshot`, `lifecycle`, `record`, `resource_status` | `lifecycle_status`, `managed_drift`, `schema_lineage` | **die with S51.3** -- do not move |
| `coordinate::MavenCoordinate` (9) | root `add dependency` | `jails-spec` |
| `request::{RenameStrategy, TypeChangeStrategy, ColumnRenamePolicy, ExternalRenamePolicy}` | root `model_rename`/`model_field_evolution` | `jails-spec::kind` beside the other closed vocabularies |
| `request::CanonicalRequestSyntaxV1` and its fingerprint | `src/dispatch.rs` only | dies with S51.3 |
| `recipe::{strip_redundant_suffix, recorded_name}` and the suffix table | root, via `facade.rs` | `jails-spec::kind` |
| `declaration::{parse_fields, FieldSpec, ConstantSpec}` | root field syntax, `jails-generate::sql` | `jails-spec::spec::field`, **for now** -- agent 3 and agent 4 decide its final home (S53.4) |
| `compatibility::APP_MANIFEST_SCHEMA` and the `docs/compatibility.tsv` reader | root `app`, commit | `jails-spec`; drop the rows of that table that name ledger formats |
| `render`, `edit`, `conflict`, `change`, `feature`, `application`, `envelope` | prepare, commit, project `projection` | die |

Land it as one commit that **adds** the new homes and leaves `pub use`
re-exports in `jails-protocol`, so nobody else's tree changes; a second commit
moves the call sites (R2) and drops the re-exports. Two traps. A survivor
must not need `#[derive(Codec)]`; if one does, it is a wire type and the thing
reading the wire is legacy -- delete that reader instead (S51.3b is the
instance). And `jails-spec` may not grow a dependency on anything above it;
`no_module_depends_on_a_layer_above_its_own` will say so.

**S51.3 -- Replace the three seams, then the ledger readers.**

- **(a) `dispatch::finish_invocation`** renders a refused command as a
  `jails.command-result` envelope through `jails-prepare::command` and
  `serialize`. The shape is pinned by a test in `tests/cli` (search for
  `command-result` and `schema_version`) and by
  `tests/protocol-golden/command-envelope.txt`. Replace it with one
  `serde_json` value built in the binary -- under thirty lines -- that renders
  the same bytes for the refused case, which is the only case this function
  has. Keep the pinning test; retarget or delete the golden.
- **(b) `migrate::apply_effect`** replays a migration effect by reading the
  frozen bytes back out of `.jails/objects`. A canonical project has no
  object store: its migrations are managed files whose digest the compiler
  lock holds. Find its caller (a `jails migrate` or `jails sql` flag), decide
  whether the command is advertised in `README.md`, and either delete the
  function with its flag or re-implement it over the lock. **Recommendation:
  delete** -- `jails migrate --check` is the surviving command and nothing in
  the canonical path produces an "effect" to replay.
- **(c) `managed_drift`** asks the store for `unfinished_transactions` and
  `jails-state::compat` for a ledger. The canonical `doctor` checks live in
  `src/model_doctor.rs` and already cover managed-output drift from the lock.
  Delete the module and its two calls in `jails-report::doctor`.
- **(d) `lifecycle_status`** is `jails resource status` for a project with a
  ledger and no model, reached only through the `!owns()` branch of
  `schema_command` (agent 2's; R2). Delete it.
- **(e) `schema_lineage`** has one canonical caller, `columns_from` in
  `src/model_doctor.rs`. Keep exactly that function and the SQL reading under
  it; delete the ledger half. Its `jails_generate::sql::columns` call is agent
  3's subject (S53.3) -- leave the call, tell them.
- **(f) `jails-state::compat::read`** is the ledger classifier. After (c) to
  (e) its callers are `jails-report::doctor` (one "unreadable ledger" check)
  and the binary's `model_init`, `model_status`, `model_doctor` and
  `model_command`, which read it to *refuse* a legacy project by name. One
  refusal survives -- a project holding `.jails/ledger.toml` and no model is
  told so -- and it is a ten-line `is_file` check in `model_command::project_root`,
  not a classifier. Hand that to agent 2; delete the crate.

**S51.4 -- Delete the kernel.** `jails-prepare`, `jails-commit`, `jails-state`,
`jails-protocol`, and the `fault-injection` feature the root `Cargo.toml`
names for a `tests/engine.rs` that no longer exists. Remove them from the
workspace `members`. Then decide the codec by measurement:

```
grep -rl 'derive(.*Codec\|impl Codec' crates/*/src src
```

On 2026-09-02 that is 32 files in `jails-protocol`, 3 in `jails-prepare`,
1 in `jails-commit`, 8 in `jails-support` (the `identity` newtypes) and 2 in
`jails-drive`. If the two in `jails-drive` are `testd`'s length-framed wire
(`testing/testd.rs`) then `codec` and `jails-codec-derive` are the daemon's
and stay, trimmed to what it uses; if they only encode identity values for
the ledger, both go. Record which in the commit.

**S51.5 -- The gates and the goldens.** Each of these measures a subject you
deleted, and R1 says the row goes with it:

- board rows: *types whose wire format is hand-written*, *codec halves outside
  `impl Codec`*, *`KIND_FILES`/`NO_FILE_TABLE` references*, *`dry_run ||
  pretend` sites*, *aliases hiding the one `Change`/`Artifact` type*, *ad-hoc
  `(path, body, label)` file tuples*, *structs in `src/` with a
  `contents`/`body` field*, and the *`doctor` module lines* row whose target
  was withdrawn. Delete each with its measuring function in `measure.rs`
  when nothing else calls it. The `--diff-algorithm` row keeps its one
  surviving site.
- `LAYERS` rows for every deleted module; `layers_lists_each_module_once`
  and `every_path_a_gate_names_is_a_file_the_scanner_found` will list them.
- `tests/protocol-golden/`: `ledger-v11.toml`, `prepared-bundle.txt`,
  `command-envelope.txt` go; `plan-bundle-v1.json` and `compiler-lock-v2.json`
  are the canonical contracts and stay, with whatever reads them.
- `docs/compatibility.tsv`: delete the rows naming ledger, journal, receipt or
  intent formats; `docs/feature-inventory.tsv`'s owner column for the crates
  you delete is agent 2's file -- R2.

**S51.6 -- Withdraw the differential claim honestly (P13.11).** The G1 canary
compares the binary against a frozen legacy revision that predates JDL v1,
and `docs/40-gates-and-ci.md` already records that no scheduled run has
happened. With the legacy engine deleted there is no second implementation to
be differential *against*. Delete `scripts/verify-rewrite-g1-canary.sh`, its
workflow, the `verify-rewrite-g1-canary` task in `mise.toml`, and the
`JAILS_LEGACY_BIN` subject in `tests/product_loop.rs`; keep the 38 scenarios
as the canonical regression suite they already are, and rewrite *What "both
implementations" currently means* in `docs/40-gates-and-ci.md` to one
paragraph saying so. `every_test_target_a_script_names_exists` and
`every_script_and_task_the_automation_names_exists` will hold you to it.

**S51.7 -- The prose.** In order, because each depends on the last:

- `docs/30-cutover.md`: P13.2 closes with agent 3's S53.3; what is left
  (P8.11a, P9.2, P9.6-P9.10) is the workspace workstream's and stays.
- `CLAUDE.md` and `ARCHITECTURE.md`: delete the "scheduled for deletion"
  paragraphs and every remaining mention of the crates you removed.
- Then widen `every_cross_reference_in_the_documents_resolves` to
  `CLAUDE.md`, `ARCHITECTURE.md` and `README.md` (P13.12) and fix what it
  reports, which is the check that the prose pass was complete.

## Traps

- **Deleting a crate does not delete a facade line.** Each crate's `lib.rs`
  re-exports the crates below it; `jails-drive` and `jails-report` name
  `jails_commit::store` in theirs. The compiler finds these; a `pub use` of
  a symbol nothing calls does not.
- **`jails-support::identity` is not the kernel's.** The validating newtypes
  are used by `jails-drive` and `jails-report`, which survive. Agent 3
  decides what of the 1,022 lines is still needed once your users are gone
  (S53.7); you only remove the `jails_protocol::identity` path to it.
- **A refusal that names a deleted path is a defect** (`docs/00-contracts.md`
  D3). Every `fix:` line in the tree that says `jails model import`,
  `jails continue` or a `.jails/objects` path is yours to find before you
  finish: `every_command_a_message_tells_the_reader_to_run_is_one_that_exists`
  catches the command half, not the path half.

## Items you close elsewhere

`docs/40-gates-and-ci.md` P13.11 and P13.12; `docs/00-contracts.md` §1.7's
kernel row and the "scheduled for deletion" lines in `ARCHITECTURE.md`.

## Green

```
cargo test --workspace
mise run verify-rewrite
```

plus the S51.1 test, which is the one that says the deletion was safe.
