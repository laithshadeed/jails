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
`jails new` seeds a model -- and the legacy engine, its two compatibility
parsers, the transaction kernel and the fifty-five templates nothing rendered
are deleted. What remains beside the compiler is the shape of what survives:
several spellings of each contract and a translation layer between every
pair.

`docs/00-contracts.md` still carries the five contracts, the deletion map and
the non-goals, and nothing here overrides them. What this file adds is the
measured state, five disjoint plans that reduce it, and the rules that let
five agents land deletions in the same tree without waiting on each other.

## What was measured

Production lines are non-blank, non-comment lines with every `#[cfg(test)]`
module removed. Raw lines are `wc -l`. Measured 2026-09-02, after the one
mutation pipeline landed; method at the end of this section.

| crate | production | raw |
|---|---:|---:|
| `jails-compiler` | 14,376 | 21,356 |
| root `src/` (the binary) | 13,071 | 17,988 |
| `jails-model` | 11,942 | 15,931 |
| `jails-drive` | 7,186 | 10,165 |
| `jails-workspace` | 4,208 | 6,405 |
| `jails-project` | 3,932 | 7,285 |
| `jails-report` | 3,403 | 5,376 |
| `jails-support` | 2,569 | 5,388 |
| `jails-java` | 752 | 1,505 |
| `jails-spec` | 634 | 1,220 |
| `jails-contracts` | 588 | 1,012 |
| `jails-codemod` | 361 | 795 |
| `jails-codec-derive` | 243 | 323 |
| `jails-testkit` | 0 | 36 |
| **total** | **63,265** | **94,785** |

Beside the crates: `tests/` is 46,455 raw lines (`tests/cli/model.rs` alone is
about 13,800); `templates/` is 142 files, every one named by a Rust source.

## What the measurement found

The facts below seed the plans. Every one was read off the tree, and each
plan carries the command that re-checks it.

1. **Every mutating frontend starts and ends in one place, and decides its
   change once.** `model_command::Current::load` is the one read of the model
   and `model_generate::finish_generation` the one pipeline behind it: a
   frontend edits the JDL text and names an `Evolution`, the model is what
   the edited source links to, and the plan's input bytes are the evolution
   serialised. `ModelPatch` and `model_apply.rs` are gone (S60.1, closed).

2. **The compiler assembles Java in 834 `format!(` sites.** The package
   line, import block and class shell are rendered by `emit_java::render` at
   20 call sites and by `emit_capability::render` at one; 19 `format!` sites
   still write an `import` line by hand. Plan `55`.

3. **The tool crates still carry a second project model** beside the
   snapshot (the second Maven parser is gone: one element walk in
   `jails-workspace/src/documents/pom.rs`). Plan `53`.

4. **`#[derive(Codec)]` has one user left.** The test-execution wire became
   one vocabulary over a `serde` daemon protocol, so the codec and its derive
   crate are the deletion that remains. Plan `51`.

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

| plan | agent | owns | what it deletes |
|---|---|---|---|
| `docs/51-kernel.md` | **1 -- kernel** | `crates/jails-codec-derive/**`, `crates/jails-support/src/codec*`, `crates/jails-spec/**`, `src/dispatch.rs`, `tests/protocol-golden/**`, `docs/30-cutover.md` | the codec, once the test-execution wire is one protocol |
| `docs/52-binary.md` | **2 -- binary** | `src/**` except `src/dispatch.rs`, `tests/cli/**` except `generate.rs`, `capabilities.rs`, `tooling.rs`, `examples.rs`, `reports.rs`, `docs/feature-inventory.tsv`, `README.md`'s command sections | the second decision of each mutation, `new`'s three seeds, the unread flags |
| `docs/53-tool-crates.md` | **3 -- tool crates** | `crates/jails-{project,java,drive,report,workspace,support,codemod,contracts}/**` (minus plan 1's files), `tests/cli/{tooling,capabilities,reports,examples}.rs`, `tests/corpus/**`, `tests/baseline.rs`, `tests/architecture_allowances.rs` | the second project model, the two test-execution vocabularies |
| `docs/54-language.md` | **4 -- language** | `crates/jails-model/**`, `docs/10-language.md` | the parser's repeated attribute handling |
| `docs/55-compiler.md` | **5 -- compiler** | `crates/jails-compiler/**`, `templates/**`, `tests/golden/**`, `tests/golden.rs`, `tests/agreement.rs`, `tests/cli/generate.rs`, `docs/20-generated-java.md` | the remaining copies of the Java shell, the second proof renderer |

**A plan that lands fewer lines than it expected reports the number and why;
it does not pad.**

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
  test-execution wire is the codec's last user.

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

**R6 -- a test dies only with its subject.** The suite is 46,000 lines and
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

Twelve crates: `jails-model`, `jails-contracts`, `jails-compiler`,
`jails-workspace`, `jails-codemod` (dependency-free, for both ladders),
`jails-support`, `jails-spec` (the shared CLI vocabulary and nothing
derived), `jails-project` (reader-owned files, with the Java reader folded in),
`jails-drive`, `jails-report`, plus the binary and `jails-testkit`; the codec
derive crate goes with its last user. One Maven document backend. One SQL
projection. One field syntax parser, in the crate that owns builtins. One
decision per mutation in the binary. One test-execution vocabulary. A
`CLAUDE.md` that describes what the code *is*.
