//! **The board**: what is counted, and what it may not exceed.
//!
//! One row per rung of `abstract.md` §8, each a ratchet that fails in both
//! directions -- see the crate docs. The *how* of every count lives in
//! [`crate::measure`]; this file is the numbers and the reasons beside them, so
//! a change to a ceiling is a change to this file and nothing else.

use crate::measure::*;

/// One measurable gate from `abstract.md` §8.
pub(crate) struct Ratchet {
    /// What is being counted, phrased as the thing that should shrink.
    pub(crate) name: &'static str,
    /// Which rung of `abstract.md` §7 closes it.
    pub(crate) rung: &'static str,
    /// Today's recorded number. May only ever be lowered.
    pub(crate) ceiling: usize,
    /// What `abstract.md` §8 asks for. `ceiling == target` is a closed gate.
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
                rung: "1 — Introduce Parameter Object (`Project`)",
                // 142 -> 144 was a measurement correction, not a regression:
                // the `&std::path::Path` spelling was never being counted.
                // 137 -> 140 for `apply_in`, `project_at` and `seed_manifest`,
                // which are entry points that *resolve* a `Project` from a
                // root -- the cure rather than the disease, and what the
                // target of 40 rather than 0 leaves room for. The disease is
                // a `root` threaded through a call graph so each level can
                // re-derive facts; these hand one to `Project::load` and stop.
                // 139 -> 143 for plan.md §12's reach work: `build::detect`,
                // `build::require_maven_at`, `config::record_layout` and
                // `adopt::report`. All four ask "what is at this path" before
                // any `Project` exists -- `detect` is what `Project::load`
                // itself calls first -- so a `Project` parameter is not
                // available to them, and this is the containment boundary
                // rather than the disease. `run.rs`'s eight call sites fold
                // into one `maven_root`, which is why it is four and not five.
                //
                // 142 -> 143 for `testd::socket_path`, which names a unix
                // socket after a project directory. That is the boundary case
                // again rather than the disease: it does path arithmetic on a
                // directory and reads nothing out of the project, so a
                // `Project` parameter would be a heavier value carrying facts
                // it must not use. The sibling that *did* look like the
                // disease was rewritten instead -- `pom_moved_since` takes the
                // pom it stats, not the root it would have had to derive it
                // from -- so this is one rise, not two.
                //
                // 143 -> 145 for `affected::select` and its
                // `affected::changed_sources`. Same category once more: one
                // walks `target/` for class files and the source tree for
                // paths, the other runs `git status` with that directory as
                // its cwd. Neither reads a fact out of the project, so a
                // `Project` would hand them exactly the re-derivation this
                // gate is against. Worth saying plainly: three of the last
                // five rises here are directory-walkers, which is the pattern
                // §8.0 predicted would keep the raw count away from zero and
                // is why the target was withdrawn rather than chased.
                //
                // 146 -> 147 for `compat::read`, which is the read-only
                // machine-state facade: it answers "what state is at this
                // path" *before* a `Project` exists, which is the same
                // category as `build::detect` above and the thing
                // `Project::load` itself needs first.
                //
                // 147 -> 148 for `capture::canonical_root`, which is the
                // boundary that turns a path into the resolved root every
                // other function in that module takes. It is the one place a
                // `&Path` may still arrive: a boundary handed the parameter
                // object would have nothing left to resolve. `capture` itself
                // takes `&CanonicalRoot` precisely so this stays at one.
                //
                // 148 -> 147: `generate_cases` took a root and now takes the
                // `Project` it was already being called with, because
                // extracting `plan_cases` beside it would otherwise have made
                // two functions where there was one.
                //
                // 145 -> 146 for `ProjectHandle::at`, which is the executor's
                // constructor: the one place a path becomes the resolved
                // handle every commit step then takes. That is the cure this
                // rung asks for, not the disease -- nothing downstream of it
                // sees a `&Path` at all.
                // 147 -> 137: ten recipes that read a file off disk to
                // decide what to generate -- `sample_value`, `factory_java`,
                // `record_test`, `value_test`, `scaffold_requests`,
                // `first_enum_constant`, `fields_from_spec_or_record` and the
                // two http-workflow preconditions -- now ask the resolved
                // `Project` instead. That is not a cosmetic signature change:
                // in an aggregate `app apply` the file they were reading has
                // not been written yet, so a disk read refuses a manifest
                // whose steps are perfectly well ordered. The projection is
                // the only thing that can answer, and taking a `Project` is
                // what reaching it looks like.
                //
                // 137 -> 126 with the dispatch flip: `src/adopt.rs`,
                // `src/app/reconcile.rs`, `src/app/shadow.rs` and V1's
                // app-state reader were all root-taking and all went, because
                // the routes take a resolved `Project`.
                //
                // 126 -> 100 when V1 and the schema-1 reader were deleted.
                // The direct write path was where a bare root travelled
                // furthest: `add::add_in`, `generate::generate_in_project`,
                // `destroy`, `shrink`, `test_wiring` and the whole
                // `generated_files` registry all took one.
                //
                // 100 -> 103 for `run/gradlew.rs`. Its three functions decide
                // which binary to invoke and where to invoke it, which is a
                // question about a *directory* -- the same shape
                // `maven::binary` has had all along. A `Project` would carry
                // no fact they read.
                //
                // 103 -> 104 for `ProjectContext::gradle`, which *constructs*
                // the context from a root. There is no resolved project to
                // read it off: this is the thing being resolved.
                //
                // 104 -> 105 for `new::drop_initializr_help`. `new` runs
                // *before* there is a project to resolve -- it is unpacking
                // the zip that will become one -- so a root is the only thing
                // it can be given.
                //
                // 105 -> 106 for `reports::failed_patterns`, which joins the
                // three readers already here. `reports.rs` reads a directory
                // of XML and knows nothing about a Java project; a `Project`
                // would carry no fact it reads.
                //
                // 106 -> 105 when `run::fmt` was deleted. It was public and
                // unreachable: `jails fmt` has gone through the transaction
                // route since V2, so this was a second, non-transactional way
                // to run the formatter that nothing called.
                //
                // 105 -> 103 when `tooling::rename` was deleted for the same
                // reason: zero production callers, `jails rename` having gone
                // through the transaction route since V2. Both of its
                // `root: &Path` helpers went with it.
                //
                // 103 -> 96 when closing the crate APIs (`pending.md` §7.2) let
                // `dead_code` reach the V1 file-level writers it had been
                // hiding: `config::record_capability`/`forget_capability`/
                // `edit_capabilities`/`record_layout`, `apply::atomically` and
                // four sibling verbs. Every one took a `root` and read the file
                // itself, which is precisely the disease this row counts -- the
                // projection holds the text and splices it, so the root-taking
                // wrapper had nothing left to do.
                //
                // 94 -> 81 when `jails new`'s thirteen root-taking helpers
                // started taking a `publish::Tree` instead. That is this row's
                // cure rather than a coincidence: a `root` threaded through a
                // call graph so each level can re-derive facts is the disease,
                // and a `Tree` is the parameter object that says which tree.
                //
                // 96 -> 94 with `generate/write.rs`'s V1 half: `ensure_assertj`,
                // `ensure_webmvc_test`, `ensure_dependency` and
                // `apply_build_change`. `route::support` states the same
                // dependencies as claims now, so these were the write path that
                // no route takes.
                //
                // 80 -> 77 when `write_new_file` and `ensure_package_info`
                // followed the same cure as the thirteen above: they take the
                // `apply::Tree` they are writing into rather than the root of a
                // project that does not exist yet. `pending.md` §7.7.
                ceiling: 72,
                // Withdrawn, not reached. abstract.md §8.0: the count includes
                // modules whose subject *is* a path, so 40 read as a demand to
                // stop writing modules. The row below is rung 1's condition;
                // this one stays a ratchet against growth, which is why the
                // target tracks the ceiling rather than sitting under it.
                target: 142,
                why: "Every one is a fact re-derived from a primitive instead of read off \
                      the resolved `Project`. This is the count abstract.md §8.1 watched \
                      rise from 161 to 195 with nothing to say so.",
            },
            root_path_parameters(src),
        ),
        (
            Ratchet {
                name: "undeclared root-taking readers of the pom",
                rung: "1 — Introduce Parameter Object (`Project`)",
                // The row above counts every `root: &Path`, and by now that
                // includes modules whose whole subject *is* a path --
                // `build.rs` asks what builds a directory, `ledger.rs` and
                // `launcher.rs` read files under one. Those are the
                // containment, not the disease, and the raw count rises every
                // time one is added, which makes its target of 40 read as a
                // demand to stop writing modules.
                //
                // So this is the disease itself, in abstract.md §2's words: a
                // function handed a primitive that goes back to disk for a
                // fact the resolved `Project` already holds. It measures the
                // *undeclared* ones, because the four that survive are each a
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
                name: "functions in `spring.rs` taking over 5 parameters",
                rung: "1 — Introduce Parameter Object (`Layers`)",
                ceiling: 0,
                target: 0,
                why: "The layer packages travel one at a time because `Layers` is not a \
                      value: a Data Clump producing connascence of position at degree 12, \
                      which is the highest-cost coupling in Page-Jones's ranking.",
            },
            over_five_params(src, SPRING_RS),
        ),
        (
            Ratchet {
                name: "structs in `src/` with a `contents`/`body` field",
                rung: "2 — Extract Class (one `Change`)",
                ceiling: 1,
                target: 1,
                why: "abstract.md §4.1 found four shapes for `a file to write`, two of them \
                      in the same file. Exactly one struct may carry a body, and it is \
                      `model::Artifact`.",
            },
            body_carrying_structs(src),
        ),
        (
            Ratchet {
                name: "ad-hoc `(path, body, label)` file tuples",
                rung: "2 — Extract Class (one `Change`)",
                ceiling: 0,
                target: 0,
                why: "The fourth of abstract.md §4.1's four shapes, and the one still \
                      standing: 14 `*_files` functions in `spring.rs` returning a positional \
                      tuple where `model::Artifact` says the same thing by name. Swap two \
                      fields and it compiles and emits wrong Java.",
            },
            file_tuple_types(src),
        ),
        (
            Ratchet {
                name: "aliases hiding the one `Change`/`Artifact` type",
                rung: "2 — Extract Class (one `Change`)",
                ceiling: 0,
                target: 0,
                why: "`NewFile` and `SpringSlice` now point at the one shape, which is the \
                      migration working. They are scaffolding, not a destination: two names \
                      for one type is how the four shapes got there in the first place.",
            },
            type_aliases(src),
        ),
        (
            Ratchet {
                name: "`dry_run || pretend` sites",
                rung: "3 — Command with undo (one `describe`)",
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
                rung: "4–5 — Separate Query from Modifier; derive `destroy`",
                ceiling: 0,
                target: 0,
                why: "A second transcription of the file list the generator right next door \
                      already computes. `tests/agreement.rs` polices it; abstract.md §9 says \
                      a test that polices duplication is a receipt for a decision not made.",
            },
            count_matches(src, "KIND_FILES") + count_matches(src, "NO_FILE_TABLE"),
        ),
        (
            Ratchet {
                name: "JSON payloads spelling their version anything but `schema_version`",
                rung: "plan.md §14 — one vocabulary across the machine-readable surface",
                // `about`, `routes`, `beans` and `why` said `schema_version`;
                // `commands`, `doctor`, `test`, `stats` and `notes` said
                // `version`. Nine emitters, two spellings, and an editor
                // integration reading two of them has to know which is which.
                //
                // The *numbers* stay per-payload on purpose: each payload has
                // its own schema and its own history, so one global number
                // would bump `routes --json` because `doctor --json` gained a
                // field.
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
                rung: "16 — collect the splice primitives (plan.md §11)",
                // `compose.rs`, `add.rs`, `add/database.rs`,
                // `add/test_wiring.rs` and `doctor.rs` each built and parsed
                // the marked-block format with their own `format!`. Five
                // owners of one format is `process.rs` before it was
                // extracted, with the same consequence waiting: a change to
                // the markers, or to the rule about the trailing newline, has
                // to be made in five places and will be made in four.
                //
                // **This row read zero for its whole life and could not have
                // read anything else.** It counted `file.production`, which
                // has every string literal blanked to spaces before a gate
                // sees it -- and a `# jails:` marker only ever appears inside
                // one. Ceiling and target were both zero, so a vacuous gate
                // and a held line printed the same word. It counts
                // `file.literals` now, and the true number is 13.
                //
                // Three of those are what the row was written to prevent, and
                // all three are in the new tree:
                // `jails-compiler/src/emit_capability/reader_facet.rs`,
                // `jails-workspace/src/documents.rs` and
                // `jails-workspace/src/reader_facet.rs` each build the block
                // with their own `format!`. They are not careless: `codemod`
                // is in `jails-project`, and neither new crate depends on it,
                // so reuse was not available. Closing this row means giving
                // the splice a home both trees can reach -- which is the same
                // "one document backend" the Maven row above is about -- not
                // deleting three `format!`s.
                //
                // The rest are reads and messages: `compose.rs` asking
                // whether a marker is present, `resource.rs` refusing one
                // inside canonical YAML, `doctor.rs` writing a `# jails:`
                // note into `~/.testcontainers.properties`. They know the
                // format too, and they are why the target is 0 rather than 3.
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
                name: "production files parsing Maven XML with their own scanner",
                rung: "R3.8 — one document backend",
                // `simplify-sol.md`'s deletion map: *duplicate Maven XML
                // scanners -> one document backend*, deleting "second
                // scanners and field-name lies".
                //
                // Five today, and the target of one is the document's ask
                // rather than a number reachable in one change: four of the
                // five are the strangler migration -- `pom.rs` being replaced,
                // `jails-workspace/src/documents.rs` replacing it -- so the
                // duplication is deliberate until the cutover, and the row is
                // what makes that cutover measurable rather than asserted.
                //
                // Until then what it buys is that a *seventh* answer cannot
                // appear. `CLAUDE.md` already records what a second scanner
                // costs: `java::types_annotated_with` was three walks, two
                // matching a raw substring, which is how `doctor` came to
                // name the wrong container config and then report every other
                // test as missing an import of it.
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
                rung: "R3.7 — one semantics row per builtin type",
                // `simplify-sol.md`'s deletion map: *repeated `Layer`, route
                // and name tables -> small typed registries with derived
                // projections*, deleting "synchronized enum/label/package
                // tables".
                //
                // 17 -> 4. Seven matches over `BuiltinType` became field
                // reads on `BuiltinSemantics`, and `BuiltinType::semantics`
                // is an exhaustive match -- so a builtin added to the enum
                // does not compile until somebody writes what it means. The
                // seven were each exhaustive too, which is exactly why this
                // was invisible: the compiler forced seven edits and checked
                // that none of them agreed.
                //
                // One of them, `link_default`, was phrased as a negation --
                // a string default was allowed for anything not in a list of
                // six numeric types -- so a new builtin silently accepted a
                // default it cannot parse. `LiteralKind` states it
                // positively, which turns that into a compile error.
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
                rung: "R3.6 — planning is a function of the capture",
                // `simplify-sol.md`'s first fitness rule. `jails-compiler`,
                // `jails-model` and `jails-contracts` depend on nothing that
                // can read a disk or start a process, and this says so as a
                // number rather than as a dependency list somebody has to
                // read -- a `use std::fs` inside a pass compiles fine today
                // and only shows up later as a plan that depended on a file
                // nobody recorded reading.
                //
                // Gated at zero *while it is zero*. A purity property is
                // cheap to keep and expensive to recover: once three passes
                // read files, the fix is no longer a lint, it is a redesign
                // of what capture returns.
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
                rung: "R3.5 — one owner per persisted format",
                // `simplify-sol.md`'s deletion map, row 8: *hand
                // codecs/tags/JSON serializers -> generated wire/report
                // schemas*, and its fitness rule *every persisted union tag
                // and field number is generated and golden-tested*.
                //
                // 210 -> 147 when `#[derive(Codec)]` landed. The derive is
                // normative about the two things a hand codec kept getting a
                // choice over: a struct encodes its fields in declaration
                // order, and an enum's tag is explicit
                // (`#[codec(tag = N)]`), never a Rust discriminant, so
                // reordering variants cannot renumber the wire. Both were
                // already true of every impl it replaced -- that is what let
                // the 62 golden ledgers stay byte-identical across the
                // change, and what makes them the test this row leans on.
                //
                // **The target is withdrawn, not reached.** What is left is
                // not more of the same: a codec that enforces key ordering
                // while it decodes, one that counts a recursion depth, one
                // that re-parses a value through its constructor so a
                // recovered journal cannot carry what the CLI would reject.
                // Those are decisions about *this* format, and a derive that
                // grew attributes for each would be a worse restatement of
                // the same code. Demanding zero would read as "express
                // validation as an attribute", which is the optional-field
                // soup the same document argues against.
                //
                // What the row is for is that the number cannot *rise*: a new
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
                rung: "R3.4 — a refusal says what to do next",
                // `pending.md` §6.5: the `fix:` convention is real and
                // load-bearing -- every `doctor` FAIL is supposed to carry one,
                // and an integration test says so -- but it is a substring
                // convention over free text, so it could only ever be checked
                // where somebody grepped for it. `doctor` was that one place.
                //
                // This counts it everywhere: a `return Err(..)` or `Err(..)`
                // whose argument builds a message, and whose message has no
                // `fix:` in it. Measured for the first time here, so the
                // ceiling is today's number and the target is what the
                // convention actually claims.
                //
                // **The target is withdrawn, not reached**, for the same reason
                // §8.0 withdrew `root: &Path`'s: the count includes refusals
                // that genuinely have no next step to name. A decoder rejecting
                // a corrupt tag, a length over its cap, a duplicate row in a
                // receipt -- these can only say what they found, and a `fix:`
                // line on one would be an invented instruction. Demanding zero
                // would read as "put a fix line on everything", which is worse
                // than the drift it is trying to stop.
                //
                // What the row is for is that the number cannot *rise*: a new
                // refusal has to either carry a fix or lower something else.
                // Separating the two kinds is per-message work, not a sweep,
                // and it is what brings this down.
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
                rung: "R1.1 — one constructor per type, and the codec calls it",
                // 130 -> 4 when the `Codec` trait was declared and the
                // inherent pairs moved onto it. `lib.rs` had been *stating*
                // this property in prose because the language could not hold
                // it: 126 types carried an `encode`/`decode` pair with
                // byte-identical signatures and nothing connected them, so
                // nothing generic could be written over them and seven named
                // monomorphisations were written instead.
                //
                // Zero. `InputPrecondition` was the last one and was the
                // seven named copies in a different disguise -- an inherent
                // `encode` method paired with a free `decode_precondition`
                // function, which is the same split with nothing naming it.
                ceiling: 0,
                target: 0,
                why: "A pair of methods with the codec's signatures and no trait behind them is \
                      a type on the wire that no generic helper can reach -- which is how seven \
                      hand-written copies of one collection loop came to exist, and how a set \
                      whose author forgot the sortedness check got no check at all.",
            },
            inherent_codec_halves(src),
        ),
        (
            Ratchet {
                name: "`fs::write` sites outside the apply layer",
                rung: "6 — `Edit` + `apply/codemod.rs`",
                ceiling: 0,
                target: 0,
                why: "Writing is the one thing that must have a single owner, or `--pretend` \
                      cannot be trusted and a ledger hung off `write_new_file` has a hole \
                      exactly where a capability updates a file it previously wrote.",
            },
            write_sites_outside_apply(src),
        ),
        (
            Ratchet {
                name: "mutations that bypass the executor",
                rung: "R6.4 — every mutation through the executor",
                // plan.md §R6.4 names the surfaces that must move -- `app`,
                // `new`, `rename`, `compose`, `generated_files`,
                // `generate::remove`, `add::shrink`, `add::database`,
                // `add::test_wiring`, `testd` and `console` -- and this is what
                // makes the migration countable rather than a list somebody
                // ticks off. Each migrated surface lowers it; the ceiling comes
                // down with it.
                //
                // 0 -> 56: the measurement was wrong, not the migration. It
                // counted raw `std::fs::*` outside the write layer, which the
                // `apply` module had already driven to zero -- so this row read
                // `done` on the rung "every mutation through the executor"
                // while fifty-six `apply::` calls still wrote straight to disk
                // outside any transaction. Every one of those surfaces R6 names
                // was still there; the gate simply could not see them, because
                // moving `fs::write` behind `apply::put` is the *first* step of
                // the migration and this gate was measuring only that step.
                //
                // Fixing the measurement rather than the ceiling is the rule
                // every other row here is built on: a gate that reads green
                // over unfinished work is worse than no gate, because it also
                // certifies the work as finished.
                //
                // 56 -> 54: `jails-tooling/src/rename.rs` deleted. It had zero
                // production callers -- `main` routes `jails rename` through
                // `jails_engine::route::maintenance::rename`, which commits it
                // as one transition -- so its two `apply::` calls were a
                // migration nobody had to do.
                //
                // 54 -> 52: `config::edit_capabilities` and `record_layout`
                // deleted, same reason. Both wrote `jails.toml` directly; the
                // projection has spliced it since V2.
                //
                // 52 -> 46: `jails-generate`'s own V1 write half, which is what
                // `pending.md` §7.7 calls "the largest crate holds one job and
                // one leftover". `generate/write.rs`'s four dependency-ensuring
                // functions, `generate/scaffold.rs`'s `field_spec`,
                // `generate_field` and `prepared_artifact_contents`, and
                // `spring/durable.rs`'s install/uninstall pair. Every one had
                // a V2 counterpart that states the same thing as a claim --
                // `route::support`, `route::field`, `SemanticEdit::MarkedBlock`
                // -- and no caller at all, which only `dead_code` could say
                // once the crate's API stopped being `pub` by default.
                //
                // 46 -> 11, and 33 of the 35 were a **measurement** correction
                // rather than a migration. `src/new.rs` and
                // `src/new/gradle_project.rs` write the skeleton with no
                // project to lock and no ledger to journal, and every byte of
                // it lands in a reserved scratch that `publish.rs` renames into
                // place or discards entire -- the same guarantee the executor
                // gives, bought the way §R6.5 describes and documented there
                // since it was written. This gate could not see that, because
                // `root: &Path` is a path like any other.
                //
                // `publish::Tree` is what made it visible. A `Tree` comes from
                // a `Publication` and nowhere else, so a function taking one
                // cannot reach a published project, and its absolute-path verbs
                // *check* containment rather than assuming it -- a write
                // outside the staging tree is a refusal. `publish.rs` joins the
                // write layer on the strength of that, not on a promise.
                //
                // `pending.md` §5 also claimed `new --app` ran `app apply`
                // "through a mechanism with no journal, no recovery and no
                // conflict detection". That was stale: `app::apply_in` builds
                // `route::Run::committing`, and a `jails new-cli --app` run
                // leaves a `.jails/` holding a ledger, objects, receipts and
                // transactions. Re-measured before acting on it.
                //
                // 11 -> 6: `apply::put_outside_project` and
                // `apply::put_in_scratch` stop counting. See
                // `executor_bypasses` for why -- both say in their own names
                // that they are not writing into a project, and there is no
                // transaction to put a write outside every project into.
                //
                // What is left is six, and each is a real decision somebody
                // has to make: `generate/write.rs`'s `create` and its
                // `package-info` write (both on the `jails new` path, but not
                // yet through `publish::Tree`), `add/database.rs`'s delete
                // under `target/` (derived output, excluded from the snapshot,
                // and arguably the same row `SUBPROCESS_CLASSIFICATION` calls
                // "derived build process"), `run.rs`'s
                // `ensure_console_launcher` splicing `pom.xml` for
                // `test --fast`, `console.rs`'s classpath directory, and
                // `testd.rs`'s. `pending.md` §7.7.
                ceiling: MUTATION_CEILING,
                target: 0,
                why: "The narrow `fs::write` gate read green while a dozen other calls mutated \
                      the project through other names -- which is exactly the surface R6 has to \
                      migrate, and exactly what a gate measuring one spelling could not see. \
                      A direct `apply::` call is the same hole under a better name: it writes \
                      outside any transaction, so `--pretend` cannot see it and the journal \
                      cannot undo it.",
            },
            mutation_sites(src, MUTATION_APIS) + executor_bypasses(src),
        ),
        (
            Ratchet {
                name: "`doctor` module lines (target withdrawn — §8.0.1)",
                rung: "9 — Move Method (`doctor` derives from `plan`)",
                // Rose 1123 -> 1140 while rung 1 ran (every check that took
                // `(root, pom_text)` now takes one `Project`), then 1140 ->
                // 1253 for `capability_drift_checks`, which is rung 9's
                // *additive* half: `doctor` finally re-plans each recorded
                // capability through `add::plan_for` and reports the delta,
                // catching a drift class nothing caught before.
                //
                // The subtractive half is what reaches 700, and it is not a
                // line-deletion exercise: the hand-written checks still cover
                // projects with no `jails.toml` capability list at all, where
                // a derived check has nothing to derive from. Removing them
                // wholesale would trade a real coverage class for a number.
                // 1253 -> 1289 for `--json`, which renders the *same*
                // `Vec<Check>` the human report prints rather than re-deriving
                // it -- the one shape that cannot describe a different run.
                // 1289 -> 1312 for `template_override_checks`: plan.md §6.6
                // states template overrides' cost as "an overridden template is
                // not golden-tested", and names this check as the mitigation.
                // Additive coverage again, not re-derivation.
                //
                // 1312 -> 1328 for the split into `doctor/environment.rs` (asks
                // the machine) and `doctor/wiring.rs` (asks the project): two
                // module headers and their imports. This gate sums the whole
                // module on purpose, so a split costs a little rather than
                // reading as a 700-line win -- which is exactly what it would
                // have done had it kept matching one filename.
                //
                // 1328 -> 1340 for plan.md §12: `doctor` names the real build
                // tool and stops reporting on a pom that is not there. Not
                // optional -- fifteen greens over a build jails cannot see is
                // §8.9's failure in a new disguise.
                //
                // 1340 -> 1404 for `hot_reload_checks`, and this is the rise
                // the §8.0.1 audit predicted would be legitimate. plan.md
                // §19.5 measured that jdt.ls writes `.class` files into
                // `target/classes` with no Maven run, which killed the `jails
                // dev` supervisor (§10.3, item 14) outright: both halves of
                // the loop already shipped. What did not exist was any way to
                // learn it is broken, and every way it breaks is silent --
                // `restart.enabled=false`, or a `restart.trigger-file` that
                // makes a recompiled class be seen and deliberately ignored.
                // Nothing in `add::plan_for` carries that; devtools is not a
                // jails capability at all. So this is a check that *replaces*
                // a feature, which is the cheapest direction this gate can
                // move in even while the number rises.
                //
                // 1404 -> 1411, and this one is embarrassing rather than
                // interesting: `cargo fmt` reflowed `hot_reload_checks` after
                // the 1404 above was measured, and the commit went out without
                // the board being re-read. No behaviour changed. Recorded as
                // its own step anyway, because a ceiling quietly absorbing a
                // second rise is how a ratchet becomes decoration.
                //
                // 1411 -> 1410 with the workspace split. `doctor` reaches the
                // lower crates through the root package's facade re-export
                // rather than importing each one, so the net is one import
                // line fewer than before. Recorded in the same change, under
                // the same rule the 1404 -> 1411 step above was recorded by:
                // an improvement nobody writes down is one the next rise
                // silently absorbs.
                //
                // 1410 -> 1402, net, from one change with two halves.
                //
                // Down 19: `doctor` had its own walk of `src/test/java`
                // collecting annotated classes, and so did `add`'s test wiring
                // and the V2 translation -- three copies, two of which matched
                // a raw substring and so read the `@SpringBootTest` in
                // `TestcontainersConfig`'s Javadoc as a declaration. That is
                // what made the `test datasource` check name Kafka's config
                // and offer `jails add db` to fix it.
                // `java::types_annotated_with` is the one reader now, and this
                // module shrank by using it rather than by losing anything.
                //
                // Up 11: `maven` now reports *which* Maven it chose -- an
                // explicit `JAILS_MAVEN`, the wrapper, or `mvn` because mvnd
                // is installed and could not start. Which one ran is the
                // difference between a build and a registry error before Maven
                // starts, and the report is what makes that answerable without
                // reading jails' source.
                //
                // 1402 -> 1407 for the unowned schema-1 rows §R2.5's adoption
                // needs a reader to be able to *find*: a `LegacyKey` is 64 hex
                // 1407 -> 1402 when the adoptable-row listing went with the
                // rest of the schema-1 handling. It was five lines here and 77
                // of 77 warnings on the example applications.
                //
                // 1402 -> 1443 for Gradle. Four checks had Maven baked into
                // them as fact rather than as one build among two -- the
                // headline flavour, the build-tool check, the JDK check and
                // Jackson -- and each needed a branch plus the wording that
                // makes its `fix:` line something a Gradle reader can carry
                // out. Raised once, with the reason, rather than left to be
                // discovered as a contradiction.
                //
                // 1443 -> 1479 for `sql_init_checks`. `spring.sql.init.mode`
                // with no `schema.sql` starts perfectly and leaves the tables
                // absent, so the first query to need one fails in front of a
                // user -- a silent failure `doctor` exists to make loud.
                //
                // 1479 -> 1481, and 488 -> 489 in the row below, for the same
                // reason as the largest-module row: `Result`'s error type is
                // `Failure` now, so `Err(format!(..))` sites gained `.into()`
                // and rustfmt wrapped a few of them. `pending.md` §6.5.
                //
                // 1481 -> 1479 when `jails-tooling` split into `jails-report`
                // and `jails-drive` (§7.6): `doctor` stopped importing
                // `crate::run`, which was its only reason to name the crate
                // that starts processes.
                //
                // 1479 -> 1524 for `pending.md` §1.1's `duplicate_key_check`.
                // The check itself is short; most of the rise is the comment
                // saying why a project can be in this state at all -- `add api`
                // before `add db` is the ordinary way somebody grows a project,
                // and the repair is `jails sync` rather than a rule against the
                // order. A check whose reason is not written down is one the
                // next reader deletes as noise.
                //
                // 1477 -> 1479 for the two lines that call the coherence
                // checks -- `migration_seal_checks` and
                // `schema_lineage_checks`. Both bodies live in their own
                // modules (`managed_drift.rs`, `schema_lineage.rs`), so what
                // this row sees is only the two `checks.extend(..)` calls.
                // That is the shape rung 9 wants: `doctor` keeps the report and
                // the derivation moves out. 1479 -> 1480 for the third, same
                // shape: `disabled_generated_tests`, whose body is in
                // `managed_drift.rs`. 1480 -> 1481 for the fourth,
                // `empty_migration_checks`, same file and same reason.
                //
                // 1481 -> 1485. `cors_checks` matched only
                // `spring-boot-starter-webmvc`, so on every project written
                // before Boot 4 renamed that starter it reported nothing --
                // including `minicom-15-01-2026`, whose `@EnableWebMvc` was
                // silently discarding every `spring.jackson.*` property in the
                // file. Four lines: the second spelling, and the sentence
                // saying what the override costs, which is the half a reader
                // acts on.
                //
                // 1485 -> 1509. A Gradle wrapper pins a distribution, and a
                // distribution cannot launch on an arbitrarily new JDK -- it
                // dies compiling its own build script, before reading the
                // project. `doctor` reported `ok gradle project wrapper` and
                // `ok jdk` over exactly that, which is the one situation this
                // command exists to prevent (bugs.md B52). 24 lines: read the
                // wrapper's pin, ask `gradle::launches_on`, and name the JDK
                // to use. It sits in `environment.rs` beside the rest of "ask
                // the machine", and the fact it asks lives in `gradle.rs`
                // where the other Gradle version facts already are -- so this
                // row grows by the question, not by the answer.
                //
                // 1509 -> 1521. `doctor` reported `25 checks, all clear` over
                // a `.jails/ledger.toml` that every mutating command refused
                // to read (bugs.md B47) -- green on the one machine state
                // nothing can act on, in the command a reader runs when
                // something is wrong. 12 lines, and the same shape as the row
                // above: `compat` already classifies the store into exactly
                // three answers and this asks it, so what grows here is the
                // question. `Absent` is deliberately not a finding.
                //
                // 1521 -> 1565 for H2's URL grammar, in `doctor/h2.rs` rather
                // than in `wiring.rs`: that module asks whether a capability
                // is wired up, and this reads one property against facts that
                // live in `deps/h2database` rather than in the project.
                // `AUTO_SERVER=TRUE` with `DB_CLOSE_ON_EXIT=FALSE` is a hard
                // startup failure -- `Database.java:282` throws
                // `getUnsupportedException` on exactly that pair -- and a
                // reader arrives at it honestly, since the first is what you
                // add to get a console and the second is what a tutorial adds.
                // The warn arm is the one that costs the lines: a file-backed
                // H2 without `AUTO_SERVER` works and cannot be inspected,
                // which is a different answer from broken and has to say so.
                //
                // 1565 -> 1567. The devtools `fix:` named a coordinate and
                // left the reader to work out the command, while `run
                // --watch`'s sibling message told them to `jails new` -- a
                // command that exists, so the oracle over `fix:` lines passed
                // it, and one that creates a different project rather than
                // repairing this one. Two lines, because the runnable command
                // does not fit on one.
                //
                // 1567 -> 1590: one new check, and the check is the point. A
                // legacy checkout arrives with an H2 `schema.sql` and
                // `spring.sql.init.mode=always`; `jails add db` brings Flyway
                // and a PostgreSQL, and Spring then runs the H2 script against
                // PostgreSQL before Flyway sees the database. Both facts were
                // already read here and `doctor` reported `ok` over each of
                // them separately. This row is a list of independent checks --
                // the one shape where length is not complexity -- and the rung
                // it is held against is about `doctor` re-deriving what `add`
                // owns, which this does not do.
                ceiling: 1590,
                // Withdrawn, not reached. abstract.md §8.0.1 audits all ten
                // checks: none is a re-encoded dependency fact, so the 700
                // measured a saving that is not there. Ratchet against growth.
                target: 1410,
                why: "Feature Envy at module scale: doctor re-derives by reading the project \
                      back off disk the facts `add/*` already own, and the drift between them \
                      is a class nothing catches.",
            },
            // The whole module, not one file: splitting `doctor.rs` into
            // `doctor/mod.rs` + submodules would take a gate that reads one
            // filename to zero, which is gaming rather than closing it. Rung 9
            // is about how much `doctor` re-derives, and that does not change
            // when the lines move to a sibling file.
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
                name: "`spring.rs` lines",
                rung: "10–11 — Extract Class; split by secret",
                // Re-baselined once, from 3449, when the row below reached 0.
                // While Java still lived inline, `blank` erased those bodies and
                // this number could not see them -- so it flattered the file.
                // With every body in `templates/spring/*.java`, what remains is
                // genuinely Rust: the `render` call and its key list. The raw
                // file fell 6,624 -> 5,517 in the same change and 4,596 lines
                // became real Java an editor can check. This number is now
                // honest, and only rung 11's split moves it.
                // Closed by rung 11's split: `workflow`, `durable`, `http` and
                // `schema` are now their own modules under `src/spring/`, and
                // what is left here is the shared precondition plus the
                // capability slices. The row below guards against the obvious
                // way to cheat this one -- moving a monolith rather than
                // decomposing it.
                // 1,268 -> 688 when rung 11 finished: `dto.rs`, `messaging.rs`
                // and `security.rs` left, and what remains is the shared
                // preconditions plus the three small capabilities that have no
                // second reader.
                // 666 -> 479: `resource.rs` left, holding the Spring artifacts
                // `g scaffold` emits -- the service, both controller shapes,
                // their tests and the in-memory adapter. One secret nothing
                // else here shares; `scope_controller_parts` stayed because
                // `g query` reads it too.
                // 479 -> 478: `add actuator` stopped writing
                // `info.app.description=@project.description@`, a generated
                // line whose value is always the empty string.
                //
                // 478 -> 480 for `add h2`, and the two lines are the `mod h2;`
                // and `pub use h2::*;` that declare it. The capability itself
                // is `spring/h2.rs`; this file gains exactly the fixed cost of
                // a split, which is the shape this ratchet is asking for.
                // 480 -> 488 for `require_mockmvc_tester`. `spring.rs`'s stated
                // job after the split is "the shared precondition and the
                // helpers used by more than one kind", and this is the second
                // precondition beside `require_spring`: seven generators write
                // a test against an API that is Spring Framework 6.2, and
                // `jails new --gradle --boot 2.x` made older projects reachable
                // for the first time. Putting it anywhere else would give two
                // owners to "is this project new enough".
                //
                // 489 -> 522 for §1.1: `handles_duplicate_keys`, the two
                // rendered blocks and their reasons. It belongs here for the
                // same reason `mockmvc_template` does -- it is a question about
                // *this project* that more than one template asks -- and the
                // row is at a fifth of its target, so this is growth the gate
                // is meant to allow rather than absorb.
                //
                // 522 -> 531 for §1.2, which *deleted* more of this file than
                // it added: `require_mockmvc_tester`, the refusal seven
                // generators made on a Boot 2 project, is gone because all nine
                // of those tests have a classic `MockMvc` form now.
                // `mockmvc_template` replaced it at about the same size, and
                // the rise is the two render call sites it made multi-line.
                // Most of what left was doc comment, which this row does not
                // count.
                //
                // 531 -> 543 once §1.2's Boot 2 run said what the floor
                // actually is. `require_mockmvc_tester` came back as
                // `require_jakarta_spring` -- narrower, three kinds instead of
                // seven, and named for `ProblemDetail`, `requestMatchers` and
                // `JdbcClient` rather than for a test entry point -- beside
                // `mockmvc_template` and `validation_package`, which is a third
                // version question about *this project* that more than one
                // template asks. Three such questions in one file is the
                // logical cohesion this row's `why` describes rather than a
                // regression; the row is at a fifth of its target.
                //
                // 545 -> 547 for `mod socket;`, 547 -> 549 for `mod presence;`,
                // 549 -> 551 for `mod seed;` and 551 -> 553 for
                // `mod endpoint;`, each with its re-export -- the same two
                // lines a new submodule always costs here. 553 -> 554 for
                // `mod support;`, which needs no re-export: `TestSupport` is
                // named by three generators in this crate, and asking where a
                // project's container config lives is a question about test
                // wiring rather than a thing `require_spring` gates. `Endpoint` went
                // into a module rather than into this file precisely because
                // this row asked: as three lines of struct plus an impl it
                // took the number to 564. The row is a
                // measure of what `spring.rs` *holds*, and it holds nothing
                // more than it did.
                //
                // 556 -> 558 for `mod join;` and its re-export: which component
                // of a child references a parent is one question, and it was
                // answered inside `query` while `usecase --via` needed the
                // same answer from the other side. Two copies of a derivation
                // is two derivations, and the one that differs is the one
                // nobody tested. Two lines for a shared secret, again.
                //
                // 554 -> 556 for `mod pin;` and its re-export: resolving a
                // `--set` literal against the declared type of the component
                // it pins is a secret of its own -- two recipes ask it, and
                // the answer needs the enum on disk rather than the request.
                // The same two lines every submodule costs here, and the same
                // shape this row wants rather than the one it stops.
                // 543 -> 545 for `mod identity;` and its re-export. That is
                // the *cure* this row asks for showing up as two lines: the
                // time-ordered identifier is a new secret and it went into
                // its own module rather than into this file. A submodule
                // declaration costing two lines is not the growth the gate
                // exists to stop.
                ceiling: 558,
                target: 2500,
                why: "Logical cohesion: one file for everything sharing the `require_spring` \
                      precondition. abstract.md §6.2 says turning that precondition into data \
                      dissolves the file along real seams. Counted as lines of *decisions*: \
                      test modules are blanked, which is the number plan.md §6.2 C sets its \
                      2,500 target against.",
            },
            src.iter()
                .find(|file| file.path.ends_with(SPRING_RS))
                .map_or(0, production_lines),
        ),
        (
            Ratchet {
                name: "production lines in the largest module",
                rung: "11 — Move Module (split by secret)",
                // 2390, and it is `generate.rs` rather than anything under
                // `src/spring/` -- so the split decomposed rather than
                // relocated, and the next monolith was already the one
                // abstract.md §3.2 calls Ousterhout's named anti-pattern
                // verbatim: parse -> dispatch -> write -> side effects.
                // 666 -> 643 when `spring/resource.rs` left `spring.rs`: the
                // largest module is no longer `spring.rs`, and the file that
                // now holds the record is 57 lines from the target.
                //
                // 643 -> 646 when `run.rs` learned there are two build tools.
                // The Gradle driving itself did *not* land here -- it is
                // `run/gradlew.rs`, split by secret -- so what is left is the
                // three lines of dispatch that decide which of the two a
                // command means.
                //
                // 646 -> 643 when `watch` and `jails gradle` pushed `run.rs`
                // to 661 and the answer was a split rather than a raise:
                // `run/fingerprint.rs` holds "what changed on disk since last
                // time", which is a different secret from "how do I invoke the
                // build tool" -- nothing in it knows what Maven or Gradle is.
                // `pom.rs` holds the record again, at the number it had before
                // Gradle existed.
                // 643 -> 644. `pom.rs` gained `TARGET_BOOT`, the Spring Boot
                // line jails' templates are written against, beside
                // `TARGET_RELEASE` which it is the exact counterpart of. It was
                // spelled only inside `templates/new/offline_pom.xml`, where
                // nothing could read it -- and `jails new --gradle` has to
                // *name* a Boot version in the build file it writes, so leaving
                // it there would have been a second literal and two fixtures
                // bootstrapping different Boot versions with nothing saying so.
                // One line of the rise is the constant; the rest is the reason,
                // which is the trade this gate exists to make visible.
                //
                // 644 -> 647, and the largest module is now `projection.rs`
                // rather than `pom.rs`. Two retirement arms gained a
                // `match self.build` they should always have had:
                // `ResourceKey::MavenDependency` opened `pom.xml`
                // unconditionally, so on a Gradle project it found no file and
                // reported the claim retired while the dependency stayed in
                // `build.gradle`; `ResourceKey::MavenMainClass` handed Groovy
                // to the XML rewriter with the same result. The *installing*
                // edits had both branches already, which is what made the
                // asymmetry invisible. Three production lines for two silent
                // wrong answers is the trade -- four after `cargo fmt` split
                // one of them, which is the number that counts.
                //
                // 648 -> 662 for `pending.md` §6.5: `Result`'s error type is
                // `Failure` rather than `String`, so every `return Err(format!
                // (..))` in the workspace gained `.into()` and rustfmt put the
                // closing `)` on its own line. Fourteen lines of that landed
                // here, which is the largest single file's share of a
                // workspace-wide type change and not a design regression. It
                // is also why §8.1 lists this file: it has been the largest
                // module since the Gradle branches went in, and the honest
                // answer to the next rise is the split, not another ceiling.
                //
                // 662 -> 649, and that is what happened. §3's build-feature key
                // pushed `projection.rs` to 665; rather than raise this, its
                // two per-key arm lists -- `apply_edit` and `retire`, 429 lines
                // between them -- moved to `projection/edit.rs`. The seam is
                // real: what is left is *state* (the overlay, the facts, the
                // reads) and what moved is the rendering, where the two arm
                // lists have to be read against each other. The largest module
                // is `doctor.rs`'s neighbourhood again rather than this file.
                //
                // 649 -> 658, and the file is `doctor/wiring.rs` rather than
                // `projection.rs`: §1.1's `duplicate_key_check`, which catches
                // a project whose `ApiExceptionHandler` predates its database
                // and therefore answers 500 to a duplicate. `wiring.rs` is a
                // list of independent checks, which is the one shape where
                // length is not complexity -- but it is the largest module
                // now, and the next rise there is the split.
                //
                // 658 -> 660, and `pom.rs` holds the record again. It took
                // `jails modernize`'s two Maven edits to 694, and the answer
                // was the split this row asks for -- `pom/retarget.rs`, the
                // edits nothing but the upgrade makes -- which brought it back
                // to 660. What is left of the rise is `mod retarget;` and its
                // re-export: the two lines a new submodule always costs, and
                // the shape this gate wants rather than the shape it stops.
                //
                // 660 -> 669, and the file is `intent/request.rs`. One new
                // `CanonicalMutationRequest` variant costs eleven lines there
                // -- the variant, its tag, its encode arm and its decode arm
                // -- and all four have to live beside the enum, because the
                // codec rule is that every wire decoder calls the same
                // constructor. That is a closed vocabulary growing by one
                // command (`modernize`), not a module accreting decisions; the
                // splittable part of a request already lives in its own
                // `RequestV1` type, and this one carries a set of paths.
                //
                // 669 -> 687, and the file is `doctor/wiring.rs`. One new
                // check, and the row above records why it belongs there: a
                // list of independent checks is the one shape where length is
                // not complexity. The split this row asks for is real and is
                // `doctor`'s to make -- environment, wiring, drift -- and it
                // is already made; what grew is one of the three halves.
                //
                // 669 -> 670, and the file is `src/main.rs`. One new generator
                // flag (`--set`) costs exactly two lines there -- the name in
                // the destructuring and the name in the `Intent` -- because
                // `main.rs` is dispatch only and every flag has to be carried
                // across it by hand.
                //
                // 670 -> 669, and that cost is gone: `--if-match` took
                // `main.rs` to 672 and the answer was the split this row asks
                // for. `generate`'s twenty arguments are a `clap::Args` struct
                // in `src/cli/generate_args.rs` now, with the conversion to
                // the engine's `Intent` written beside the field names rather
                // than at the dispatch site, so `main.rs` names the command
                // once and a twenty-first flag costs it nothing. The largest
                // module is `spring/workflow.rs` again, unchanged.
                ceiling: 687,
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
                name: "inline Java bodies in `spring.rs` (`r#\"package `)",
                rung: "10 — templates out of `spring.rs`",
                ceiling: 0,
                target: 0,
                why: "Every brace doubled, and no editor or compiler can check it. This is \
                      the exact tax `src/template.rs` exists to remove.",
            },
            inline_java_bodies(src),
        ),
        (
            Ratchet {
                name: "percent of generated Java that is comment",
                rung: "P6.4 — a template that cannot check its claim says less",
                // 22 is what the templates measure today. Extracting the
                // pre-existing Toxiproxy Java from Rust raw strings made that
                // generated prose visible to this scanner; the golden-output
                // test proves the extraction did not add a line to generated
                // projects. This is a measurement-boundary correction, not a
                // reopened allowance for new template prose.
                //
                // The four false claims `modern.md` §11.2 found were repaired at their
                // source: the repository Javadoc names the *component* rather
                // than the column, the publisher no longer claims per-entity
                // ordering it does not give, the transition says nothing about
                // scope where there is none in the SQL, and the in-memory
                // adapter keys on the same component the JDBC one does.
                //
                // Held rather than driven down. The prose that remains is the
                // load-bearing kind `modern.md` §12 names and asks to keep --
                // why the container is a `@Bean`, why `*IT` needs Failsafe,
                // why an NPE is deliberately not fatal. What the number stops
                // is the next template quietly adding another paragraph nobody
                // can check beside them.
                ceiling: 22,
                target: 22,
                why: "A wrong explanation is believed, and a comment restating a decision is                       the fastest thing in a codebase to go stale. Generated prose is worse                       again: it is asserted by a template that has no way to confirm it, and                       it is copied into every project.",
            },
            template_comment_density(),
        ),
    ]
}
