<!--
One of six -- the brief for the simplification pass. `docs/50-simplify.md`
is the file every one of the five agents reads first; `51` to `55` are the
five plans, one per agent, and each names the paths it owns and the paths it
must not touch.

**A closed item is deleted from the file that holds it**, in the commit that
closes it -- never marked done. `git log -p -- docs/` is the record.

**Item numbers are stable and never reused.** `S<plan>.<n>` here; the older
`P*`, `A*`, `B*` identifiers keep resolving where `docs/00-contracts.md` says.

Every number in these six files is a measurement with its method beside it.
Re-measure before quoting one; the tree moves under every commit.
-->

# 50 — The simplification pass: five agents, one goal

The goal is **one system, described once, in fewer lines**. The canonical
compiler is finished -- 39 of 39 generators, 25 of 25 capabilities, ordinary
`jails new` seeds a model -- and what remains beside it is a legacy engine
nothing can reach, a binary that repeats one pipeline sixteen times, two
compatibility parsers for inputs this binary no longer writes, fifty-five
template files nothing renders, and prose describing all of the above as if
it were live.

`docs/00-contracts.md` still carries the five contracts, the deletion map and
the non-goals, and nothing here overrides them. What this file adds is the
measured state on 2026-09-02, five disjoint plans that reduce it, and the rules
that let five agents land deletions in the same tree without waiting on each
other.

## What was measured

Production lines are non-blank, non-comment lines with every `#[cfg(test)]`
module removed. Raw lines are `wc -l`. Method at the end of this section.

| crate | production | raw | `#[test]` |
|---|---:|---:|---:|
| `jails-model` | 15,996 | 20,631 | 79 |
| root `src/` (the binary) | 14,852 | 20,317 | 55 |
| `jails-compiler` | 14,376 | 21,526 | 80 |
| `jails-protocol` | 8,556 | 17,013 | 238 |
| `jails-project` | 8,443 | 14,909 | 190 |
| `jails-drive` | 8,265 | 11,463 | 94 |
| `jails-prepare` | 6,647 | 10,892 | 115 |
| `jails-report` | 4,352 | 6,627 | 55 |
| `jails-workspace` | 4,226 | 6,467 | 37 |
| `jails-support` | 3,302 | 6,839 | 115 |
| `jails-commit` | 2,558 | 4,955 | 62 |
| `jails-spec` | 879 | 1,702 | 9 |
| `jails-java` | 774 | 1,546 | 24 |
| `jails-generate` | 641 | 1,324 | 12 |
| `jails-contracts` | 588 | 1,021 | 5 |
| `jails-codemod` | 427 | 1,017 | 17 |
| `jails-codec-derive` | 243 | 323 | 1 |
| `jails-state` | 92 | 271 | 6 |
| `jails-testkit` | 0 | 45 | 0 |
| **total** | **95,217** | **148,888** | **1,194** |

Beside the crates: `tests/` is 50,080 raw lines and 596 tests (`tests/cli/model.rs`
alone is 14,392 lines, 159 tests); `templates/` is 197 files, 12,172 lines;
`CLAUDE.md` is 2,895 lines, `README.md` 1,927, `ARCHITECTURE.md` 402.

## What the measurement found

Six facts, each the seed of one or more plans. Every one was read off the
tree, and each plan carries the command that re-checks it.

1. **The legacy transaction kernel is unreachable from the binary.** Nothing in
   `src/` depends on `jails-commit`; `jails-prepare` is reached from exactly one
   function, `dispatch::finish_invocation`, for the JSON *error* envelope; and
   `jails-commit` is reached from two read-only sites, `migrate::apply_effect`
   (a frozen migration out of the object store) and `managed_drift`'s
   `unfinished_transactions`. Nothing creates `.jails/ledger.toml` any more.
   That kernel is `jails-prepare` + `jails-commit` + `jails-state` + most of
   `jails-protocol` + the codec: about **18,600 production lines, 35,000 raw,
   and 415 unit tests that test only it.** Plan `51`.

2. **Ordinary `jails new` is canonical and every document says it is not.**
   `new/spring.rs` seeds a model through `seed_canonical_model` on both the
   online and offline paths, and the comment beside it says so. `CLAUDE.md`,
   `README.md`, `ARCHITECTURE.md` and `docs/30-cutover.md` all still describe
   it as the cutover's first blocked step. The legacy-side pin this was waiting
   on (`reports: 21, tests: 57`) no longer exists in that form. Plans `51`
   and `52`.

3. **The binary repeats one pipeline sixteen times.** Sixteen `src/model_*.rs`
   frontends each read the model source, parse it, edit the text, parse it
   again and hand the result to `finish_generation` --
   `crate::model_generate_jdl::parse(` appears at 30 call sites in 11 files.
   Ten `owns()` switches in seven files still guard legacy branches whose other
   side no longer exists. Plan `52`.

4. **Two compatibility parsers exist to feed one one-shot.** `source.rs`
   (`.jails/model.toml`), `jdl.rs` (the pre-v1 draft), `jdl/upgrade.rs` and
   the `jdl/emit` renderer -- about 3,500 raw lines in `jails-model` -- are
   reachable only through `jails model upgrade --to 1`, and jails is not
   released, so the projects they would carry across are this repository's
   own. Plan `54`.

5. **Fifty-five of 197 template files are rendered by nothing** -- 2,310 lines
   under `templates/spring/` and `templates/generate/`, orphaned when the
   legacy generator was deleted. And the compiler assembles Java in 838
   `format!(` sites, each carrying its own copy of the package line, the import
   block and the class shell. Plan `55`.

6. **The tool crates still carry both halves of the strangler.** Five files
   parse Maven XML (P13.2); `jails-generate`'s SQL projection duplicates the
   compiler's; `jails-spec`'s field spec duplicates `jails-model`'s builtin
   registry; `jails-project` has modules with no caller outside
   `jails-prepare`. Plan `53`.

Reproduce the table with this, from the repository root. It is the
approximation the baseline was taken with; `tests/architecture/measure.rs` is
the authority where the two disagree, and the board's "largest module" row is
measured by it, not by this.

```
python3 - <<'PY'
import pathlib
def prod(src):
    out, i = [], 0
    while True:
        j = src.find('#[cfg(test)]', i)
        if j < 0: out.append(src[i:]); break
        out.append(src[i:j]); k = src.find('{', j); d, m = 0, k
        while m < len(src):
            if src[m] == '{': d += 1
            elif src[m] == '}':
                d -= 1
                if d == 0: break
            m += 1
        i = m + 1
    s = ''.join(out)
    return sum(1 for l in s.splitlines() if l.strip() and not l.strip().startswith('//'))
for c in sorted(pathlib.Path('crates').iterdir()) + [pathlib.Path('.')]:
    files = list((c / 'src').rglob('*.rs'))
    print(sum(prod(f.read_text()) for f in files), sum(len(f.read_text().splitlines()) for f in files), c.name or 'root')
PY
```

## The five plans

| plan | agent | owns | what it deletes | expected |
|---|---|---|---|---:|
| `docs/51-kernel.md` | **1 -- kernel** | `crates/jails-{prepare,commit,state,protocol,codec-derive}/**`, `crates/jails-support/src/codec*`, `crates/jails-spec/**`, `src/dispatch.rs`, the ledger readers in `jails-report`, `tests/protocol-golden/**`, the G1 canary, `docs/30-cutover.md` | the legacy transaction kernel and everything that exists to read its ledger | −18,000 production |
| `docs/52-binary.md` | **2 -- binary** | `src/**` (except `src/dispatch.rs` and `src/model_upgrade.rs`), `tests/cli/**` (except `generate.rs`, `capabilities.rs`, `tooling.rs`, `examples.rs`, `reports.rs`), `docs/feature-inventory.tsv`, `README.md`'s command sections | sixteen copies of one pipeline, every `owns()` branch, `jails app`'s legacy backend, `new`'s write path | −4,000 production |
| `docs/53-tool-crates.md` | **3 -- tool crates** | `crates/jails-{project,generate,java,drive,report,workspace,support,codemod,contracts}/**` (minus plan 1's files), `tests/cli/{tooling,capabilities,reports,examples}.rs`, `tests/corpus/**`, `tests/baseline.rs`, `tests/architecture_allowances.rs` | the second Maven parser, the second SQL projection, the second field spec, dead `jails-project` modules, `jails-generate` | −6,000 production |
| `docs/54-language.md` | **4 -- language** | `crates/jails-model/**`, `src/model_upgrade.rs`, `docs/10-language.md`, `docs/01-jdl-v1.md` §22 | the two compatibility parsers, the upgrade, the renderer that serves only it, a second patch path if there is one | −3,500 production |
| `docs/55-compiler.md` | **5 -- compiler** | `crates/jails-compiler/**`, `templates/**`, `tests/golden/**`, `tests/golden.rs`, `tests/agreement.rs`, `tests/cli/generate.rs`, `docs/20-generated-java.md` | 55 orphaned templates, the per-emitter copies of the Java shell, the second proof renderer | −2,500 templates, −2,500 production |

The expected column is an estimate from the reading above, written down so
the result can be compared with it. **A plan that lands fewer lines than
expected reports the number and why; it does not pad.**

## Rules that let five agents share one tree

**R1 -- delete, never mark.** An item is closed by deleting it from its plan
in the commit that closes it, and a subject is closed by deleting the code,
its tests, its board row, its `LAYERS` rows, its prose and its `docs/` items
in the same change. A gate whose subject is gone is a gate that measures
nothing; its row goes with the subject.

**R2 -- the call-site rule.** Path ownership is by the table above, with one
exception each way: an agent deleting a symbol may make the *minimal* edit at
its call sites in another agent's paths -- remove the call, remove the import,
delete the test whose only subject was that symbol -- in the same commit,
and nothing more. An agent may not add to, restructure or "improve" another
agent's path while there. Whoever lands second resolves the trivial conflict.

**R3 -- one landing order, three dependencies.** Everything else is parallel.

- Plan `51` step S51.2 lands **first**, within the first day: the surviving
  vocabulary of `jails-protocol` moves to `jails-spec` with `pub use`
  re-exports left behind, so no other agent's imports change. Until it lands,
  nobody else touches a `jails_protocol::` import.
- Plan `53`'s deletion of `jails-project` modules that only `jails-prepare`
  reaches waits for plan `51` step S51.4. Everything else in `53` starts now.
- Plan `54`'s parser deletion removes the `.jails/model.toml` and upgrade tests
  from `tests/cli/model.rs`, which plan `52` owns. R2 covers it.

**R4 -- the shared files keep their resolution rules** from
`docs/00-contracts.md`: `tests/golden/**` is regenerated and the diff is read;
`tests/architecture/board.rs` keeps both notes and re-measures;
`LAYERS` keeps both sides and dedupes; `tests/common/scenarios.rs` is append
only. Two are added: the workspace `Cargo.toml` `members` list is edited only
by the agent deleting or adding a crate; and `CLAUDE.md` is edited *by
section* -- each agent owns the sections about its subject, and plan `51` owns
the "Legacy workspace during cutover" section whole.

**R5 -- green is `mise run verify-rewrite`**, before every push, as before.
A step is one commit and, where the repository uses pull requests, one pull
request: a 30,000-line change that fails review is a week lost, and a
1,000-line one reverts alone. Rebase onto `main` at least daily.

**R6 -- a test dies only with its subject.** The suite is 50,000 lines and
some of it will go, but only the tests whose subject a plan deletes, and the
duplicates a plan can *name* -- two tests proving one property through one
path. Nothing is deleted to make a number.

**R7 -- no behaviour change on an advertised command.** A command in
`README.md` keeps doing what it does. A refusal that names a deleted path is
rewritten, not removed. Commands with no `README.md` section and no journey
are *listed for the user* by the plan that finds them, not deleted -- that is
a product decision.

**R8 -- measure and record.** Each step's commit message body carries the
production and raw line delta for the paths it touched, by the method above.
The board and this file are the only places a number is *kept*; every other
document quotes the method.

**R9 -- the non-goals stand.** `docs/00-contracts.md` §6.1: no template
engine, no three-crate rewrite for its own sake, no dynamic schema, no Java
parser, no deleting the merge base, and LOC is not the limiting variable --
which is why every item in the five plans names the *concept* it removes and
counts lines second.

## What happens to the four workstream documents

They stay, and they shrink. `docs/10-language.md`, `docs/20-generated-java.md`,
`docs/30-cutover.md` and `docs/40-gates-and-ci.md` hold open items with stable
identifiers, and each plan below lists the ones it closes. An agent closing one
deletes it there, per R1. Items the pass does not reach stay where they are.
When the pass ends, `docs/30-cutover.md` should hold nothing the cutover still
blocks on and may be retired whole by plan `51`, and the ownership table in
`docs/00-contracts.md` reverts to four workstreams over whatever is left.

## The end state, so it can be recognised

Ten crates, not eighteen: `jails-model`, `jails-contracts`, `jails-compiler`,
`jails-workspace`, `jails-codemod` (dependency-free, for both ladders),
`jails-support`, `jails-spec` (the shared CLI vocabulary and nothing
derived), `jails-project` (reader-owned files, with the Java reader folded in),
`jails-drive`, `jails-report`, plus the binary and `jails-testkit`. One Maven
document backend. One SQL projection. One field syntax parser, in the crate
that owns builtins. One mutation pipeline in the binary. No `owns()`. No
ledger, no store, no journal, no codec for them, and no prose that says
otherwise. A `CLAUDE.md` under 1,500 lines that describes what the code *is*.
