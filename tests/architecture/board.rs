//! **The board**: what is counted, and what it may not exceed.
//!
//! One row per architecture property, each a ratchet that fails in both
//! directions -- see the crate docs. The *how* of every count lives in
//! [`crate::measure`]; this file is the numbers and the reasons beside them, so
//! a change to a ceiling is a change to this file and nothing else.

use crate::measure::*;

/// One measurable gate.
pub(crate) struct Ratchet {
    /// What is being counted, phrased as the thing that should shrink.
    pub(crate) name: &'static str,
    /// The refactoring or rule that closes it.
    pub(crate) rung: &'static str,
    /// Today's recorded number. May only ever be lowered.
    pub(crate) ceiling: usize,
    /// Where the row is finished. `ceiling == target` is a closed gate.
    pub(crate) target: usize,
    /// Why the number matters, printed when the row fails.
    pub(crate) why: &'static str,
}

pub(crate) fn gates() -> Vec<(Ratchet, usize)> {
    let src = sources();
    vec![
        (
            Ratchet {
                name: "`root: &Path` parameters (target withdrawn — §8.0)",
                rung: "Introduce Parameter Object (`Project`)",
                // Every `root: &Path` in production, including the containment
                // boundaries that are the cure rather than the disease: the
                // `_at` entry points that resolve a `Project` from a root and
                // stop the walk up from the process directory (`jails new`
                // stands in the parent of the project it is creating), the
                // executor's own constructor, capture's `canonical_root`, and
                // the readers whose subject *is* a directory -- `build::detect`,
                // `compat::read`, `testd::socket_path`, `affected::select`, the
                // reader-tree refusals before a rename, the staged-temporary
                // sweep. None of those threads a root further down. The disease
                // is a root threaded through a call graph so each level can
                // re-derive facts, and the row below counts that on its own.
                ceiling: 81,
                // Withdrawn, not reached: the count includes modules whose
                // subject *is* a path, so a target under the ceiling reads as
                // a demand to stop writing modules. The row below is the
                // condition; this one stays a ratchet against growth, which is
                // why the target is the number at withdrawal rather than one
                // under the ceiling.
                target: 142,
                why: "Every one is a fact re-derived from a primitive instead of read off \
                      the resolved `Project`, and the count rises silently unless it is held.",
            },
            root_path_parameters(src),
        ),
        (
            Ratchet {
                name: "undeclared root-taking readers of the pom",
                rung: "Introduce Parameter Object (`Project`)",
                // The row above counts every `root: &Path`, including modules
                // whose whole subject *is* a path. This is the disease itself:
                // a function handed a primitive that goes back to disk for a
                // fact the resolved `Project` already holds. It measures the
                // *undeclared* ones, because the few that survive are each a
                // case where reading again is correct and a `Project` would be
                // wrong -- see `A_FRESH_READ_IS_CORRECT`. Nought means every
                // one of them is a decision somebody wrote down.
                ceiling: 0,
                target: 0,
                why: "Feature Envy on `Project`: a second read of the pom for a fact the \
                      caller already resolved, which is how two answers to one question \
                      appear in one run.",
            },
            rederivers(src)
                .into_iter()
                .filter(|(_, name)| {
                    !A_FRESH_READ_IS_CORRECT
                        .iter()
                        .any(|(declared, _)| declared == name)
                })
                .count(),
        ),
        (
            Ratchet {
                name: "structs in `src/` with a `contents`/`body` field",
                rung: "Extract Class (one `Change`)",
                ceiling: 1,
                target: 1,
                why: "Exactly one struct may carry a file body, and it is `model::Artifact`: \
                      a second shape for `a file to write` is a second definition of what \
                      gets written.",
            },
            body_carrying_structs(src),
        ),
        (
            Ratchet {
                name: "ad-hoc `(path, body, label)` file tuples",
                rung: "Extract Class (one `Change`)",
                ceiling: 0,
                target: 0,
                why: "A positional `(path, body, label)` tuple says by position what \
                      `model::Artifact` says by name. Swap two fields and it compiles and \
                      emits wrong Java.",
            },
            file_tuple_types(src),
        ),
        (
            Ratchet {
                name: "aliases hiding the one `Change`/`Artifact` type",
                rung: "Extract Class (one `Change`)",
                ceiling: 0,
                target: 0,
                why: "Two names for one type is how several shapes for one thing come to \
                      exist.",
            },
            type_aliases(src),
        ),
        (
            Ratchet {
                name: "`dry_run || pretend` sites",
                rung: "Command with undo (one `describe`)",
                ceiling: 0,
                target: 0,
                why: "Two names for one boolean, OR'd at dispatch because the global flag \
                      and the per-command flag reach two different implementations. \
                      Connascence of meaning crossing a module boundary.",
            },
            count_matches(src, "dry_run || pretend") + count_matches(src, "pretend || dry_run"),
        ),
        (
            Ratchet {
                name: "`KIND_FILES`/`NO_FILE_TABLE` references",
                rung: "Separate Query from Modifier; derive `destroy`",
                ceiling: 0,
                target: 0,
                why: "A second transcription of the file list the generator right next door \
                      already computes. `tests/agreement.rs` polices it, and a test that \
                      polices duplication is a receipt for a decision not made.",
            },
            count_matches(src, "KIND_FILES") + count_matches(src, "NO_FILE_TABLE"),
        ),
        (
            Ratchet {
                name: "JSON payloads spelling their version anything but `schema_version`",
                rung: "one vocabulary across the machine-readable surface",
                // Every JSON emitter spells its version field `schema_version`,
                // so an editor integration reading several of them does not
                // have to know which is which. The *numbers* stay per-payload
                // on purpose: each payload has its own schema, so one global
                // number would bump `routes --json` because `doctor --json`
                // gained a field.
                ceiling: 0,
                target: 0,
                why: "A machine-readable surface with two names for one field makes every \
                      consumer carry a special case, and the special case is what breaks \
                      when a tenth emitter picks a third name.",
            },
            src.iter()
                .map(|file| file.literals.matches("\\\"version\\\": ").count())
                .sum(),
        ),
        (
            Ratchet {
                name: "`# jails:` block literals outside `jails-codemod`",
                rung: "collect the splice primitives",
                // The marked-block format has one owner, `jails-codemod`, a
                // crate with no dependencies at all so every crate can reach
                // it. A change to the markers, or to the rule about the
                // trailing newline, is then made once rather than in several
                // places and forgotten in one.
                //
                // **The row counts `file.literals`, not `file.production`.**
                // Blanked source has every string literal replaced by spaces,
                // and a `# jails:` marker only ever appears inside one, so a
                // gate reading blanked source reads zero whatever the code
                // says -- and a gate that cannot fail prints the same word as
                // one that is holding.
                ceiling: MARKED_BLOCK_LITERALS,
                target: 0,
                why: "The marked block is how jails edits a file the reader owns, and it is \
                      what makes `remove` the exact inverse of `add`. A second implementation \
                      of it is a second answer to what `remove db` deletes.",
            },
            src.iter()
                .filter(|file| !file.path.ends_with(CODEMOD_RS))
                .map(|file| {
                    file.literals.matches("# jails:").count()
                        + file.literals.matches("# /jails:").count()
                })
                .sum(),
        ),
        (
            Ratchet {
                name: "`--diff-algorithm` sites outside `jails-support::git`",
                rung: "one owner for what this machine's git can do",
                // A fence rather than a ratchet. The two merge implementations
                // live in ladders that cannot see each other, so neither can
                // learn what the other found out about the machine's git; on
                // git <= 2.43 `git merge-file` rejects the flag with exit 129,
                // a usage error, and every regeneration over an edited file
                // fails.
                ceiling: 0,
                target: 0,
                why: "A capability decision made in two places is one that eventually gets \
                      made two ways. `jails_support::git` probes once and builds the argv for \
                      both merges; a third site spelling the flag itself is how the first two \
                      came to disagree with the machine they ran on.",
            },
            src.iter()
                .filter(|file| !file.path.ends_with(GIT_RS))
                .map(|file| file.literals.matches("--diff-algorithm").count())
                .sum(),
        ),
        (
            Ratchet {
                name: "production files parsing Maven XML with their own scanner",
                rung: "one document backend",
                // The target of one is the ask rather than a number reachable
                // in one change: most of the count is `pom.rs` beside
                // `jails-workspace/src/documents.rs`, which replaces it, so the
                // duplication is deliberate until the cutover and the row is
                // what makes that cutover measurable rather than asserted.
                // Until then what it buys is that another scanner cannot
                // appear: a scanner matching a raw substring is how `doctor`
                // comes to name the wrong container config and then report
                // every other test as missing an import of it.
                ceiling: MAVEN_XML_PARSERS,
                target: 1,
                why: "A tool that half-understands a build file and reports a dependency the \
                      build does not have is the worst outcome available -- and two scanners \
                      are two half-understandings that disagree without saying so.",
            },
            maven_xml_parsers(src),
        ),
        (
            Ratchet {
                name: "largest table of per-builtin knowledge outside its row",
                rung: "one semantics row per builtin type",
                // `BuiltinType::semantics` is an exhaustive match, so a builtin
                // added to the enum does not compile until somebody writes what
                // it means. Several exhaustive matches over the enum are each
                // forced by the compiler and none of them checked against the
                // others, and a rule phrased as a negation -- a string default
                // for anything not in a list of numeric types -- silently
                // accepts a default a new builtin cannot parse. `LiteralKind`
                // states it positively, which turns that into a compile error.
                ceiling: LARGEST_BUILTIN_TABLE,
                target: LARGEST_BUILTIN_TABLE,
                why: "A second table of what a builtin means is a second answer, and the one \
                      that is wrong is whichever the reader did not edit -- which the compiler \
                      cannot report, because both are exhaustive over the same enum.",
            },
            largest_builtin_table(src),
        ),
        (
            Ratchet {
                name: "compiler passes reaching outside the captured snapshot",
                rung: "planning is a function of the capture",
                // `jails-compiler`, `jails-model` and `jails-contracts` depend
                // on nothing that can read a disk or start a process, and this
                // says so as a number rather than as a dependency list somebody
                // has to read: a `use std::fs` inside a pass compiles fine and
                // only shows up later as a plan that depended on a file nobody
                // recorded reading. Gated at zero because a purity property is
                // cheap to keep and expensive to recover: once three passes
                // read files, the fix is a redesign of what capture returns.
                ceiling: 0,
                target: 0,
                why: "The reason to capture first is that planning becomes a function -- same \
                      snapshot, same request, same plan -- which is what makes a plan safe to \
                      show before it is applied. One read inside a pass ends that quietly.",
            },
            compiler_reaches_outside_the_snapshot(src),
        ),
        (
            Ratchet {
                name: "types whose wire format is hand-written (target withdrawn)",
                rung: "one owner per persisted format",
                // `#[derive(Codec)]` is normative about the two things a hand
                // codec keeps getting a choice over: a struct encodes its
                // fields in declaration order, and an enum's tag is explicit
                // (`#[codec(tag = N)]`), never a Rust discriminant, so
                // reordering variants cannot renumber the wire.
                //
                // **The target is withdrawn, not reached.** What is left is
                // not more of the same: a codec that enforces key ordering
                // while it decodes, one that counts a recursion depth, one
                // that re-parses a value through its constructor so a decoded
                // value cannot carry what the CLI would reject. Those are
                // decisions about *this* format, and a derive that grew
                // attributes for each would be a worse restatement of the same
                // code. The row is for the number not *rising*: a new
                // persisted type derives its format, or says in the commit why
                // it is one of the bespoke ones.
                ceiling: HAND_WRITTEN_CODECS,
                target: HAND_WRITTEN_CODECS,
                why: "A hand-written codec states the field list three times -- in the type, \
                      in `encode`, in `decode` -- so a field added to the type and forgotten \
                      in the codec is a silent change of format rather than a compile error.",
            },
            hand_written_codecs(src),
        ),
        (
            Ratchet {
                name: "refusals with no `fix:` line (target withdrawn)",
                rung: "a refusal says what to do next",
                // The `fix:` convention is a substring convention over free
                // text, so this counts it everywhere: a `return Err(..)` or
                // `Err(..)` whose argument builds a message with no `fix:` in
                // it.
                //
                // **The target is withdrawn, not reached**: the count includes
                // refusals that genuinely have no next step to name. A decoder
                // rejecting a corrupt tag, a length over its cap, a duplicate
                // row -- these can only say what they found, and a `fix:` line
                // on one would be an invented instruction. Demanding zero would
                // read as "put a fix line on everything", which is worse than
                // the drift it is trying to stop. The row is for the number
                // not *rising*: a new refusal either carries a fix or lowers
                // something else.
                ceiling: REFUSALS_WITHOUT_A_FIX,
                target: REFUSALS_WITHOUT_A_FIX,
                why: "A refusal that names no next step leaves the reader to guess, and jails' \
                      whole argument for refusing rather than guessing is that it can say what \
                      would work instead.",
            },
            refusals_without_a_fix(src),
        ),
        (
            Ratchet {
                name: "codec halves outside `impl Codec`",
                rung: "one constructor per type, and the codec calls it",
                // An inherent `encode`/`decode` pair with the codec's
                // signatures and no trait behind it is a type on the wire no
                // generic helper can reach, and an inherent `encode` paired
                // with a free `decode_*` function is the same split with
                // nothing naming it.
                ceiling: 0,
                target: 0,
                why: "A pair of methods with the codec's signatures and no trait behind them is \
                      a type on the wire that no generic helper can reach, so each such type \
                      gets its own copy of every collection loop and its own chance to forget \
                      the sortedness check.",
            },
            inherent_codec_halves(src),
        ),
        (
            Ratchet {
                name: "`fs::write` sites outside the apply layer",
                rung: "one write layer (`apply`)",
                ceiling: 0,
                target: 0,
                why: "Writing is the one thing that must have a single owner, or `--pretend` \
                      cannot be trusted: a write path beside the choke point has a hole \
                      exactly where a capability updates a file it previously wrote.",
            },
            write_sites_outside_apply(src),
        ),
        (
            Ratchet {
                name: "mutations that bypass the executor",
                rung: "every mutation through the executor",
                // Raw `std::fs::*` outside the write layer *and* direct
                // `apply::` calls outside any transaction, because a gate
                // measuring only the first spelling reads green while the
                // second writes straight to disk where `--pretend` cannot see
                // it. `apply::put_outside_project` and `apply::put_in_scratch`
                // do not count -- see `executor_bypasses`: both say in their
                // own names that they are not writing into a project, and
                // there is no transaction to put such a write into. A function
                // taking a `publish::Tree` does not count either: a `Tree`
                // comes from a `Publication` and nowhere else, so it cannot
                // reach a published project, and its absolute-path verbs
                // *check* containment rather than assuming it. What is left is
                // a short list, and each entry is a real decision somebody has
                // to make rather than a migration nobody did.
                ceiling: MUTATION_CEILING,
                target: 0,
                why: "A gate measuring one spelling of `fs::write` reads green while other \
                      calls mutate the project under other names. A direct `apply::` call is \
                      the same hole under a better name: it writes outside any transaction, \
                      so `--pretend` cannot see it.",
            },
            mutation_sites(src, MUTATION_APIS) + executor_bypasses(src),
        ),
        (
            Ratchet {
                name: "`doctor` module lines (target withdrawn — §8.0.1)",
                rung: "Move Method (`doctor` derives from `plan`)",
                // The whole `doctor` neighbourhood, and it is a list of
                // independent checks -- the one shape where length is not
                // complexity. The rung it is held against is about `doctor`
                // re-deriving facts another module owns, which a check that
                // asks the machine (`environment.rs`), asks `compat`, or asks
                // `Project::is_modelled` does not do. Raise it once, with the
                // reason, when a check is added: a ceiling quietly absorbing a
                // rise is how a ratchet becomes decoration.
                ceiling: 1505,
                // Withdrawn, not reached: none of the hand-written checks is a
                // re-encoded dependency fact, so a lower target measures a
                // saving that is not there. Ratchet against growth.
                target: 1410,
                why: "Feature Envy at module scale: doctor re-derives by reading the project \
                      back off disk the facts `add/*` already own, and the drift between them \
                      is a class nothing catches.",
            },
            // The whole module, not one file: splitting `doctor.rs` into
            // `doctor/mod.rs` + submodules would take a gate that reads one
            // filename to zero, which is gaming rather than closing it. The
            // rung is about how much `doctor` re-derives, and that does not
            // change when the lines move to a sibling file.
            src.iter()
                .filter(|file| {
                    file.path.ends_with(DOCTOR_RS)
                        || file.path.to_string_lossy().contains("doctor/")
                })
                .map(production_lines)
                .sum(),
        ),
        (
            Ratchet {
                name: "production lines in the largest module",
                rung: "Move Module (split by secret)",
                // Whichever module is largest, so a split cannot be satisfied
                // by *moving* a monolith. The shape it keeps out is parse ->
                // dispatch -> write -> side effects in one file. A list of
                // independent checks is the one shape where length is not
                // complexity, and a closed vocabulary growing by one variant
                // -- the variant, its tag, its encode arm and its decode arm,
                // all beside the enum -- is growth rather than accretion.
                // Anything else that crosses the ceiling is split by the
                // secret it has accreted rather than by size, and a cut that
                // leaves two halves both needing the whole picture is not a
                // seam.
                ceiling: 666,
                target: 700,
                why: "The row above can be satisfied by *moving* a monolith rather than \
                      decomposing one, so this asks the question the split is actually for: \
                      is there still a file nobody can hold in their head? It is measured \
                      across every module, so cutting one in half only helps if the halves \
                      are genuinely smaller.",
            },
            src.iter().map(production_lines).max().unwrap_or(0),
        ),
        (
            Ratchet {
                name: "modules with no module doc",
                rung: "written-down reasoning is the mechanism",
                // Closed, and it stays on the board because the gate is what
                // keeps it closed: a new module with no doc is a failing build
                // rather than a slow return to undocumented crates.
                ceiling: 0,
                target: 0,
                why: "Written-down reasoning is how this project stops a decision being \
                      silently reversed. A module with no doc is a module whose reasons live \
                      nowhere, so a field added to it reads as an accepted design.",
            },
            modules_without_a_module_doc(src),
        ),
        (
            Ratchet {
                name: "percent of generated Java that is comment",
                rung: "a template that cannot check its claim says less",
                // Held rather than driven down. The prose that remains is the
                // load-bearing kind -- why the container is a `@Bean`, why
                // `*IT` needs Failsafe, why an NPE is deliberately not fatal
                // -- and every claim in it names something the generated code
                // can be checked against: the repository Javadoc names the
                // *component* rather than the column, and the publisher claims
                // no per-entity ordering it does not give. What the number
                // stops is the next template quietly adding another paragraph
                // nobody can check beside them.
                ceiling: 23,
                target: 23,
                why: "A wrong explanation is believed, and a comment restating a decision is \
                      the fastest thing in a codebase to go stale. Generated prose is worse \
                      again: it is asserted by a template that has no way to confirm it, and \
                      it is copied into every project.",
            },
            template_comment_density(),
        ),
    ]
}
