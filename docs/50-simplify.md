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
compatibility parsers for inputs this binary no longer writes, and fifty-five
template files nothing renders.

`docs/00-contracts.md` still carries the five contracts, the deletion map and
the non-goals, and nothing here overrides them. What this file adds is the
measured state on 2026-09-02, five disjoint plans that reduce it, and the rules
that let five agents land deletions in the same tree without waiting on each
other.

## What was measured

Production lines are non-blank, non-comment lines with every `#[cfg(test)]`
module removed. Raw lines are `wc -l`. Method at the end of this section.

| crate | production | raw |
|---|---:|---:|
| `jails-compiler` | 14,376 | 21,356 |
| root `src/` (the binary) | 13,457 | 18,385 |
| `jails-model` | 11,935 | 15,924 |
| `jails-drive` | 7,186 | 10,165 |
| `jails-workspace` | 4,226 | 6,409 |
| `jails-project` | 3,945 | 7,327 |
| `jails-report` | 3,403 | 5,442 |
| `jails-support` | 2,633 | 5,459 |
| `jails-java` | 752 | 1,505 |
| `jails-spec` | 641 | 1,239 |
| `jails-contracts` | 588 | 1,012 |
| `jails-codemod` | 361 | 795 |
| `jails-codec-derive` | 243 | 323 |
| `jails-testkit` | 0 | 36 |
| **total** | **63,746** | **95,377** |

Beside the crates: `tests/` is 46,486 raw lines (`tests/cli/model.rs` alone is
about 14,000); `templates/` is 142 files.

## What the measurement found

The facts below seed the plans. Every one was read off the tree, and each
plan carries the command that re-checks it. The transaction kernel, the SQL
workspace built on its vocabulary, the two compatibility parsers, the
one-shot upgrade and the orphaned templates are deleted already; what
remains is the shape of what survives.

2. **Ordinary `jails new` is canonical.** `new/spring.rs` seeds a model
   through `seed_canonical_model` on both the online and offline paths, so
   there is no project shape left that reaches the legacy crates. Plans `51`
   and `52`.

3. **The binary repeats one pipeline sixteen times.** Sixteen `src/model_*.rs`
   frontends each read the model source, parse it, edit the text, parse it
   again and hand the result to `finish_generation` --
   `crate::model_generate_jdl::parse(` appears at 30 call sites in 11 files.
   Ten `owns()` switches in seven files still guard legacy branches whose other
   side no longer exists. Plan `52`.

5. **The compiler assembles Java in 838 `format!(` sites**, each carrying its
   own copy of the package line, the import block and the class shell. Plan
   `55`.

6. **The tool crates still carry a second Maven parser** (P13.2) and a
   second project model beside the snapshot. Plan `53`.

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

## The shape the plans converge on

`docs/60-abstraction.md` names it: five nouns (source, model, snapshot,
desired, plan), four verbs (edit, compile, plan, execute), one owner per
closed vocabulary, generators as data the way capabilities already are. Read
it before starting a step, and read its last paragraph twice: a deletion that
adds a new shape is not progress.

## The five plans

| plan | agent | owns | what it deletes | expected |
|---|---|---|---|---:|
| `docs/51-kernel.md` | **1 -- kernel** | `crates/jails-codec-derive/**`, `crates/jails-support/src/codec*`, `crates/jails-spec/**`, `src/dispatch.rs`, `tests/protocol-golden/**`, `docs/30-cutover.md` | the codec the daemon's wire still uses, once the wire is one protocol | −1,000 production |
| `docs/52-binary.md` | **2 -- binary** | `src/**` (except `src/dispatch.rs` and `src/model_upgrade.rs`), `tests/cli/**` (except `generate.rs`, `capabilities.rs`, `tooling.rs`, `examples.rs`, `reports.rs`), `docs/feature-inventory.tsv`, `README.md`'s command sections | sixteen copies of one pipeline, every `owns()` branch, `jails app`'s legacy backend, `new`'s write path | −4,000 production |
| `docs/53-tool-crates.md` | **3 -- tool crates** | `crates/jails-{project,java,drive,report,workspace,support,codemod,contracts}/**` (minus plan 1's files), `tests/cli/{tooling,capabilities,reports,examples}.rs`, `tests/corpus/**`, `tests/baseline.rs`, `tests/architecture_allowances.rs` | the second Maven parser, the second project model, the two test-execution vocabularies | −4,000 production |
| `docs/54-language.md` | **4 -- language** | `crates/jails-model/**`, `docs/10-language.md` | a second patch path if there is one, the parser's repeated attribute handling | −1,500 production |
| `docs/55-compiler.md` | **5 -- compiler** | `crates/jails-compiler/**`, `templates/**`, `tests/golden/**`, `tests/golden.rs`, `tests/agreement.rs`, `tests/cli/generate.rs`, `docs/20-generated-java.md` | the per-emitter copies of the Java shell, the second proof renderer | −2,500 production |

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

- Plan `51`'s codec decision (S51.4) waits on plan `53`'s S53.5, because the
  daemon's wire is the codec's last user.
- Plan `52`'s one pipeline (S52.1) and plan `54`'s `Edit` (S60.1) are the same
  change seen from two crates; agree the `Edit` shape before either starts.
- Plan `53`'s S53.3 and plan `55`'s S55.6 both touch what `doctor` compares
  migrations against; land S55.6 first.

**R4 -- the shared files keep their resolution rules** from
`docs/00-contracts.md`: `tests/golden/**` is regenerated and the diff is read;
`tests/architecture/board.rs` keeps both notes and re-measures;
`LAYERS` keeps both sides and dedupes; `tests/common/scenarios.rs` is append
only. Two are added: the workspace `Cargo.toml` `members` list is edited only
by the agent deleting or adding a crate; and `CLAUDE.md` is edited *by
section* -- each agent owns the sections about its subject.

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

Ten crates, not thirteen: `jails-model`, `jails-contracts`, `jails-compiler`,
`jails-workspace`, `jails-codemod` (dependency-free, for both ladders),
`jails-support`, `jails-spec` (the shared CLI vocabulary and nothing
derived), `jails-project` (reader-owned files, with the Java reader folded in),
`jails-drive`, `jails-report`, plus the binary and `jails-testkit`. One Maven
document backend. One SQL projection. One field syntax parser, in the crate
that owns builtins. One mutation pipeline in the binary. No `owns()`. No
ledger, no store, no journal, no codec for them, and no prose that says
otherwise. A `CLAUDE.md` that describes what the code *is*.
