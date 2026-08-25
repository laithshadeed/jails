//! The eleven gates of `abstract.md` §8, as ratchets rather than as a table.
//!
//! `abstract.md` prices every rung of its refactor ladder with a falsifiable
//! gate and says to revert the rung when the gate is missed. Nothing measured
//! them, and §8.1 recorded the consequence: `root: &Path` rose 21% across four
//! commits while `src/model/mod.rs` was being added *beside* the primitive
//! rather than instead of it, and nothing said so. Prose did not move the
//! number; `tests/genericity.rs` moved the vocabulary problem only once it put
//! a failure in the build.
//!
//! So each row below is a **ratchet**, and it bites in both directions:
//!
//! - **Rising above the ceiling fails.** That is the regression guard.
//! - **Falling below the ceiling also fails**, telling you to record the new
//!   number. That is what makes progress permanent: an improvement that is not
//!   written down here is an improvement the next change may silently undo.
//!
//! `target` is the number `abstract.md` §8 actually asks for. A ceiling equal
//! to its target is a finished rung; the test prints the gap for every row, so
//! `cargo test --test architecture -- --nocapture` is the ladder's progress
//! report.
//!
//! Measurement is over **blanked** Rust: comments and string literals are
//! replaced by spaces of the same length, so a `fn` inside one of `spring.rs`'s
//! inline Java bodies cannot be counted as a function, and a `root: &Path`
//! written in a doc comment cannot be counted as a parameter. That is
//! `src/java.rs`'s own trick, applied to Rust for the same reason.

use std::fs;
use std::path::{Path, PathBuf};

/// One measurable gate from `abstract.md` §8.
struct Ratchet {
    /// What is being counted, phrased as the thing that should shrink.
    name: &'static str,
    /// Which rung of `abstract.md` §7 closes it.
    rung: &'static str,
    /// Today's recorded number. May only ever be lowered.
    ceiling: usize,
    /// What `abstract.md` §8 asks for. `ceiling == target` is a closed gate.
    target: usize,
    /// Why the number matters, printed when the row fails.
    why: &'static str,
}

fn gates() -> Vec<(Ratchet, usize)> {
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
                ceiling: 80,
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
            root_path_parameters(&src),
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
            rederivers(&src)
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
            over_five_params(&src, "spring.rs"),
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
            body_carrying_structs(&src),
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
            file_tuple_types(&src),
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
            type_aliases(&src),
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
            count_matches(&src, "dry_run || pretend") + count_matches(&src, "pretend || dry_run"),
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
            count_matches(&src, "KIND_FILES") + count_matches(&src, "NO_FILE_TABLE"),
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
                .map(|file| file.production.matches("\\\"version\\\": ").count())
                .sum(),
        ),
        (
            Ratchet {
                name: "`# jails:` block literals outside `codemod.rs`",
                rung: "16 — collect the splice primitives (plan.md §11)",
                // `compose.rs`, `add.rs`, `add/database.rs`,
                // `add/test_wiring.rs` and `doctor.rs` each built and parsed
                // the marked-block format with their own `format!`. Five
                // owners of one format is `process.rs` before it was
                // extracted, with the same consequence waiting: a change to
                // the markers, or to the rule about the trailing newline, has
                // to be made in five places and will be made in four.
                ceiling: 0,
                target: 0,
                why: "The marked block is how jails edits a file the reader owns, and it is \
                      what makes `remove` the exact inverse of `add`. A second implementation \
                      of it is a second answer to what `remove db` deletes.",
            },
            src.iter()
                .filter(|file| !file.path.ends_with("codemod.rs"))
                .map(|file| {
                    file.production.matches("# jails:").count()
                        + file.production.matches("# /jails:").count()
                })
                .sum(),
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
            refusals_without_a_fix(&src),
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
            inherent_codec_halves(&src),
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
            write_sites_outside_apply(&src),
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
            mutation_sites(&src, MUTATION_APIS) + executor_bypasses(&src),
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
                ceiling: 1479,
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
                    file.path.ends_with("doctor.rs")
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
                ceiling: 489,
                target: 2500,
                why: "Logical cohesion: one file for everything sharing the `require_spring` \
                      precondition. abstract.md §6.2 says turning that precondition into data \
                      dissolves the file along real seams. Counted as lines of *decisions*: \
                      test modules are blanked, which is the number plan.md §6.2 C sets its \
                      2,500 target against.",
            },
            src.iter()
                .find(|file| file.path.ends_with("spring.rs"))
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
                ceiling: 662,
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
            inline_java_bodies(&src),
        ),
    ]
}

#[test]
fn the_abstract_md_ladder_gates_are_ratchets_that_only_move_down() {
    let rows = gates();
    let mut rose = Vec::new();
    let mut fell = Vec::new();

    println!("\nabstract.md §7 ladder — gate status\n");
    println!(
        "{:<52} {:>7} {:>9} {:>8}  rung",
        "gate", "now", "ceiling", "target"
    );
    for (gate, actual) in &rows {
        let mark = if *actual > gate.ceiling {
            "ROSE"
        } else if *actual < gate.ceiling {
            "FELL"
        } else if gate.ceiling <= gate.target {
            "done"
        } else {
            "held"
        };
        println!(
            "{:<52} {:>7} {:>9} {:>8}  {:<6} {}",
            gate.name, actual, gate.ceiling, gate.target, mark, gate.rung
        );
        if *actual > gate.ceiling {
            rose.push((gate, *actual));
        } else if *actual < gate.ceiling {
            fell.push((gate, *actual));
        }
    }
    println!();

    let mut report = String::new();
    for (gate, actual) in &rose {
        report.push_str(&format!(
            "\nREGRESSED: {} is {actual}, above the recorded ceiling of {}.\n  rung {}\n  {}\n  \
             Either bring the number back down, or -- if the rise is deliberate and \
             justified -- say why in the commit and raise the ceiling in the same change.\n",
            gate.name, gate.ceiling, gate.rung, gate.why
        ));
    }
    for (gate, actual) in &fell {
        report.push_str(&format!(
            "\nIMPROVED, RECORD IT: {} is {actual}, below the recorded ceiling of {}.\n  \
             rung {}\n  Lower this row's `ceiling` to {actual} in tests/architecture.rs. An \
             improvement that is not recorded here is one the next change may silently \
             undo, which is exactly the failure abstract.md §8.1 documented.\n",
            gate.name, gate.ceiling, gate.rung
        ));
    }
    assert!(report.is_empty(), "{report}");
}

/// Every gate that has reached its target should stay at it.
/// The declared list must name exactly the readers that are really there.
///
/// Both directions. A name that has gone means the function was fixed or
/// renamed and the reason is now permission for nothing; a reader that is not
/// named is one nobody decided about.
/// The layering the workspace split is built on, as a test, so it holds before
/// the crates physically exist and keeps holding for module-level edges the
/// compiler will never see.
///
/// Every module is assigned the crate it belongs to, and a module may only
/// reference one at its own level or below. That is the whole property: the
/// twelve-module strongly connected component this replaced -- `add`, `compose`,
/// `config`, `generate`, `inspect`, `launcher`, `model`, `project`, `run`,
/// `spring`, `sql`, `why` -- existed because everything below the generators
/// reached up into `generate.rs` for `Field`, `layout` and `find_project_root`.
/// A cycle is a boundary nothing can enforce, and `CLAUDE.md` records what an
/// unenforced boundary produces: `inspect.rs` kept its own copy of the layer
/// list and silently reported a renamed layer as "Other".
///
/// Same-level edges are allowed, including mutual ones: `generate` and `spring`
/// call each other and ship in the same crate, which is a design decision
/// rather than an accident.
#[test]
fn no_module_depends_on_a_layer_above_its_own() {
    let mut offenders = Vec::new();
    let mut assigned: std::collections::BTreeSet<(&str, &str)> = Default::default();
    for file in sources() {
        let Some((krate, owner)) = module_of(&file.path) else {
            continue;
        };
        let row = LAYERS.iter().find(|(c, m, _)| *c == krate && *m == owner);
        let Some(&(_, _, level)) = row else {
            panic!(
                "{} belongs to `{krate}`'s module `{owner}`, which is not assigned a layer \
                 in `LAYERS`. Add it there in the same change that adds the module -- an \
                 unassigned module is an unenforced boundary.",
                file.path.display()
            );
        };
        assigned.insert((
            LAYERS
                .iter()
                .find(|(c, _, _)| *c == krate)
                .map(|(c, _, _)| *c)
                .unwrap_or_default(),
            LAYERS
                .iter()
                .find(|(c, m, _)| *c == krate && *m == owner)
                .map(|(_, m, _)| *m)
                .unwrap_or_default(),
        ));
        // A same-crate reference is a same-level edge by construction, so only
        // the crates above this one can be reached up into.
        for (other_crate, other, other_level) in LAYERS {
            if *other_crate == krate || *other_level <= level {
                continue;
            }
            if file.production.contains(&format!("crate::{other}::"))
                || file.production.contains(&format!("crate::{other};"))
            {
                offenders.push(format!(
                    "  {} ({krate}::{owner}, L{level}) -> {other_crate}::{other} \
                     (L{other_level})",
                    file.path.display()
                ));
            }
        }
    }

    // Both directions, the same rule `SUBPROCESS_CLASSIFICATION` is held to: a
    // row naming a module that is no longer there is permission for nothing,
    // and it hides the fact that the module went. Four such rows were found
    // when this was added -- `ledger` and `migration` had become submodules,
    // `rename` was deleted, and `main.rs` is excluded by `module_of` by design.
    let stale: Vec<String> = LAYERS
        .iter()
        .filter(|(c, m, _)| !assigned.contains(&(*c, *m)))
        .map(|(c, m, _)| format!("  {c}::{m}"))
        .collect();
    assert!(
        stale.is_empty(),
        "`LAYERS` assigns a layer to modules that are not there:\n{}\n\n\
         Take them out. A name in the list that nothing matches is a rule \
         nobody is subject to.",
        stale.join("\n")
    );
    assert!(
        offenders.is_empty(),
        "these modules reach up into a higher layer:\n{}\n\n\
         Move what they need down to the layer that needs it, the way `Field`,\n\
         `layout` and `find_project_root` moved from `generate` to `spec`. Do not\n\
         fix this by moving the module up: that is how one module at a time\n\
         becomes one cycle.",
        offenders.join("\n")
    );
}

/// `LAYERS` must not list one module twice.
///
/// The gate that used to live here forbade two crates from declaring the same
/// top-level module name, because `module_of` identified a file by its
/// basename alone and the layering check would then measure one module against
/// another's level. That was a real hazard -- it happened once, between
/// `jails_spec::spec` and a `spec` module in `jails-protocol`, and nothing
/// reported it.
///
/// It was also **a test choosing production names**: `src/dispatch.rs` shipped
/// as `invoke` for no reason other than that `jails-java` already had a
/// `dispatch`, and its module docs said so. `pending.md` §10.3. `module_of`
/// answers `(crate, module)` now, so the collision cannot arise and the module
/// is called what it is.
///
/// What is left is the one property the pair still has to have: a duplicate row
/// would make the `find` above pick whichever came first.
#[test]
fn layers_lists_each_module_once() {
    let mut names: Vec<(&str, &str)> = LAYERS.iter().map(|(c, m, _)| (*c, *m)).collect();
    names.sort_unstable();
    let before = names.len();
    names.dedup();
    assert_eq!(before, names.len(), "`LAYERS` lists one module twice");
}

/// Which crate each module ships in, lowest first. The 7-crate workspace this
/// documents is `jails-support`, `jails-java`, `jails-spec`, `jails-project`,
/// `jails-generate`, `jails-tooling` and the `jails-cli` binary.
const LAYERS: &[(&str, &str, usize)] = &[
    // jails-support: no jails concepts at all -- writing, running, encoding.
    ("jails-support", "apply", 0),
    ("jails-support", "process", 0),
    ("jails-support", "hermetic", 0),
    ("jails-support", "scratch", 0),
    ("jails-support", "codec", 0),
    ("jails-support", "json", 0),
    ("jails-support", "lock", 0),
    // jails-java: reading Java and rendering templates into it.
    ("jails-java", "annotate", 1),
    ("jails-java", "tidy", 1),
    ("jails-java", "java", 1),
    ("jails-java", "dispatch", 1),
    ("jails-java", "classfile", 1),
    ("jails-java", "identifier", 1),
    ("jails-java", "template", 1),
    // jails-spec: what a jails project is -- where it is, how it is laid out,
    // what a field means, and the closed CLI vocabularies.
    ("jails-spec", "build", 2),
    ("jails-spec", "spec", 2),
    // jails-protocol: the validated values every closed format is built from.
    ("jails-protocol", "durable", 3),
    ("jails-protocol", "intent", 3),
    ("jails-protocol", "observe", 3),
    ("jails-protocol", "vocabulary", 3),
    // jails-state: `.jails/` and what a directory holds. Below the Java
    // project on purpose -- `jails-commit` needs both and neither is about Java.
    ("jails-state", "compat", 4),
    ("jails-state", "listing", 4),
    // jails-project: the resolved project and everything jails records about it.
    ("jails-project", "gradle", 5),
    ("jails-project", "pom", 5),
    ("jails-project", "maven", 5),
    ("jails-project", "capability", 5),
    ("jails-project", "config", 5),
    ("jails-project", "junit", 5),
    ("jails-project", "synonyms", 5),
    ("jails-project", "capture", 5),
    ("jails-project", "codemod", 5),
    ("jails-project", "compose", 5),
    ("jails-project", "model", 5),
    ("jails-project", "project", 5),
    ("jails-project", "projection", 5),
    ("jails-project", "properties", 5),
    ("jails-project", "generated_files", 5),
    ("jails-project", "inspect", 5),
    // jails-generate: everything that decides what Java to write.
    ("jails-generate", "sql", 6),
    ("jails-generate", "generate", 6),
    ("jails-generate", "spring", 6),
    ("jails-generate", "add", 6),
    // jails-prepare: turning desire into an exact executable transition.
    ("jails-prepare", "command", 6),
    ("jails-prepare", "desire", 6),
    ("jails-prepare", "operation", 6),
    ("jails-prepare", "pipeline", 6),
    ("jails-prepare", "prepare", 6),
    ("jails-prepare", "receipt", 6),
    ("jails-prepare", "merge", 6),
    ("jails-prepare", "reconcile", 6),
    ("jails-prepare", "recovery", 6),
    ("jails-prepare", "report", 6),
    ("jails-prepare", "sandbox", 6),
    ("jails-prepare", "serialize", 6),
    ("jails-prepare", "tool", 6),
    // jails-commit: making a prepared transaction durable, and recovering one.
    ("jails-commit", "activate", 7),
    ("jails-commit", "execute", 7),
    ("jails-commit", "fault", 7),
    ("jails-commit", "gc", 7),
    ("jails-commit", "runtime", 7),
    ("jails-commit", "journal", 7),
    ("jails-commit", "outcome", 7),
    ("jails-commit", "recover", 7),
    ("jails-commit", "store", 7),
    // jails-engine: one request, as one transition. Above the executor because
    // it drives it, and below the CLI because it is not about arguments.
    ("jails-engine", "route", 8),
    // jails-report: commands that answer a question. Read-only by contract,
    // and below `jails-drive` so the contract is structural.
    ("jails-report", "doctor", 7),
    ("jails-report", "why", 7),
    ("jails-report", "explain", 7),
    ("jails-report", "commands", 7),
    ("jails-report", "source", 7),
    // jails-drive: commands that start something.
    ("jails-drive", "run", 8),
    ("jails-drive", "launcher", 8),
    ("jails-drive", "testd", 8),
    ("jails-drive", "affected", 8),
    ("jails-drive", "kafka", 8),
    ("jails-drive", "migrate", 8),
    ("jails-drive", "console", 8),
    ("jails-drive", "bench", 8),
    ("jails-drive", "reports", 8),
    ("jails-drive", "lint", 8),
    // jails-cli: the binary and the whole-project lifecycle commands.
    ("jails", "new", 9),
    ("jails", "app", 9),
    ("jails", "dispatch", 9),
    ("jails", "arguments", 9),
];

/// Every module that starts a process, and which R6.6 row it is.
///
/// plan.md §R6.6 fixes the classification "so 'one writer' is not
/// overclaimed": a subprocess can change a project as surely as a write can,
/// and the filesystem gate says nothing about it. `mvn` writes `target/`;
/// `docker compose up` starts a service; `git merge-file` produces bytes a
/// transaction commits. Each is fine — *once somebody has said which it is*.
///
/// The test below fails when a module starts a process and is not named here,
/// which is the audit §R6.6 asks for expressed as a ratchet rather than a
/// list somebody re-derives.
const SUBPROCESS_CLASSIFICATION: &[(&str, &str)] = &[
    // Derived build processes. They may write `target/` and dependency
    // caches, which are excluded from the snapshot and the store, and they
    // run without the project lock after any required commit.
    ("run", "derived build process"),
    ("launcher", "derived build process"),
    ("why", "derived build process"),
    ("testd", "derived build process"),
    ("affected", "derived build process"),
    // External runtime effects. A commit records the desired project files
    // first; reconciling the runtime is an idempotent receipt effect.
    ("compose", "external runtime effect"),
    ("kafka", "external runtime effect"),
    ("migrate", "external runtime effect"),
    ("bench", "external runtime effect"),
    // Read-only clients and probes. Outside both locks, and they claim no
    // filesystem rollback.
    ("console", "read-only client"),
    ("doctor", "read-only probe"),
    // Bootstrap, outside any project transaction: these run before a project
    // exists (§R6.5), inside a scratch tree that is published atomically.
    ("new", "new-project bootstrap"),
    // A three-way merge is transaction preparation, not a renderer and not an
    // effect: it runs `git merge-file` over three scratch inputs to compute
    // bytes the commit then guards like any other. §R5.2 says so explicitly --
    // git appears in the preparation fingerprint and never in a renderer stamp.
    ("merge", "transaction input"),
    // The executor's own runner and tool resolver.
    ("process", "the one executor"),
    ("hermetic", "the one executor"),
    ("sandbox", "the one executor"),
];

/// Which module a file belongs to: `(crate, module)`.
///
/// `crates/jails-generate/src/spring/durable.rs` ->
/// `("jails-generate", "spring")`.
///
/// **The crate half is not decoration.** This used to answer with the basename
/// alone, so `crates/a/src/spec.rs` and `crates/b/src/spec.rs` were one name to
/// every gate here, and the layering check would silently measure one against
/// the other's level. That happened once, between `jails_spec::spec` and a
/// `spec` module in `jails-protocol`, and nothing reported it -- the test
/// passed while checking the wrong thing. A separate gate,
/// `no_two_crates_share_a_module_name`, existed to forbid the collision, which
/// made a *test* the reason a production module could not be called what it is:
/// `src/dispatch.rs` was named `invoke` because `jails-java` already had a
/// `dispatch`. `pending.md` §10.3.
///
/// Identify a module by `(crate, module)` and the constraint goes away.
fn module_of(path: &Path) -> Option<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut crate_name = "jails".to_string();
    let rest = path.strip_prefix(root.join("src")).ok().or_else(|| {
        let from_crates = path.strip_prefix(root.join("crates")).ok()?;
        // crates/<member>/src/<module>...
        let mut parts = from_crates.components();
        crate_name = parts.next()?.as_os_str().to_string_lossy().into_owned();
        let src = parts.next()?;
        (src.as_os_str() == "src").then(|| Path::new(parts.as_path()))
    })?;
    let first = rest.components().next()?.as_os_str().to_str()?;
    if first == "lib.rs" || first == "main.rs" {
        return None;
    }
    Some((
        crate_name,
        first.strip_suffix(".rs").unwrap_or(first).to_string(),
    ))
}

/// §R6.6: "The audit must leave no unclassified production mutation."
///
/// A subprocess changes things a write gate cannot see. This fails when a
/// module starts one and nobody has said which row it belongs to — and it
/// fails the other way too, when a classified module stops starting
/// processes, because a classification nobody prunes claims more than it
/// describes.
#[test]
fn every_module_that_starts_a_process_is_classified() {
    let src = sources();
    let mut starts: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for file in &src {
        // Both spellings: a direct `Command::new`, and the shared
        // `CommandSpec` executor that most callers rightly use instead.
        // Counting only one would classify half the surface.
        let spawns = [
            "Command::new",
            "CommandSpec",
            "process::run",
            "hermetic::run",
        ]
        .iter()
        .any(|spelling| file.production.contains(spelling));
        if spawns && let Some((_, module)) = module_of(&file.path) {
            starts.insert(module);
        }
    }
    let classified: std::collections::BTreeSet<String> = SUBPROCESS_CLASSIFICATION
        .iter()
        .map(|(name, _)| (*name).to_string())
        .collect();

    let unclassified: Vec<&String> = starts.difference(&classified).collect();
    assert!(
        unclassified.is_empty(),
        "\n\n{unclassified:?} start a subprocess and are not in \
         SUBPROCESS_CLASSIFICATION.\n\
         plan.md §R6.6 fixes the rows: derived build process, external runtime effect, \
         read-only client/probe, new-project bootstrap, transaction input, or the one \
         executor. Say which, so `one writer` is not overclaimed.\n"
    );

    let stale: Vec<&String> = classified.difference(&starts).collect();
    assert!(
        stale.is_empty(),
        "\n\n{stale:?} are classified as starting a subprocess and no longer do.\n\
         Take them out -- a classification nobody prunes claims more than it describes.\n"
    );
}

#[test]
fn every_fresh_read_of_the_pom_is_a_decision_somebody_wrote_down() {
    let src = sources();
    let found: std::collections::BTreeSet<String> =
        rederivers(&src).into_iter().map(|(_, name)| name).collect();
    let declared: std::collections::BTreeSet<String> = A_FRESH_READ_IS_CORRECT
        .iter()
        .map(|(name, _)| (*name).to_string())
        .collect();

    assert_eq!(
        found, declared,
        "\n\nA root-taking function reads the pom for a fact a resolved `Project` holds.\n\
         Either pass it the `Project` -- which is rung 1 -- or, if reading again is \
         genuinely correct, add it to A_FRESH_READ_IS_CORRECT with the reason.\n\
         A name in the list that is no longer found has to come out: a reason nobody \
         needs is permission nobody asked for."
    );
    assert!(
        A_FRESH_READ_IS_CORRECT
            .iter()
            .all(|(_, why)| why.len() > 40),
        "every reason has to say why a `Project` would be wrong, not merely that it is allowed"
    );
}

/// Production scratch trees go through `ScratchDir`, which is the only thing
/// that creates one exclusively.
///
/// plan.md §3.2. `env::temp_dir().join(pid + timestamp)` followed by
/// `create_dir_all` is not exclusive in either half: two callers can read the
/// same clock, and `create_dir_all` treats "it already exists" as success. That
/// handed one test another's tree, and in `app/reconcile.rs` it would have
/// merged a regenerated intent against somebody else's base.
///
/// Test modules are exempt only because `Source::production` blanks them, and
/// they are being converted separately; a production site has no exemption.
#[test]
fn production_scratch_directories_are_exclusively_created() {
    let mut offenders = Vec::new();
    for file in sources() {
        if file.path.ends_with("scratch.rs") {
            continue;
        }
        if file.production.contains("env::temp_dir()") {
            offenders.push(file.path.display().to_string());
        }
    }
    assert!(
        offenders.is_empty(),
        "these production sites name a scratch directory instead of reserving one:\n  {}\n\n\
         Use `jails_support::scratch::ScratchDir`. A pid and a timestamp are not \
         unique, and `create_dir_all` succeeds on a directory that is already \
         someone else's.",
        offenders.join("\n  ")
    );
}

#[test]
fn a_gate_that_reached_its_target_is_never_reopened() {
    for (gate, actual) in gates() {
        if gate.ceiling <= gate.target {
            assert!(
                actual <= gate.target,
                "{} closed at {} and is now {actual}. A closed gate reopening means the \
                 rung was reverted without the ladder being updated.\n  {}",
                gate.name,
                gate.target,
                gate.why
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Measurement
// ---------------------------------------------------------------------------

struct Source {
    path: PathBuf,
    /// Comments and string literals replaced by spaces of the same length --
    /// so byte offsets and line counts still index the original -- with every
    /// `#[cfg(test)] mod … { … }` body also blanked.
    ///
    /// abstract.md §3 states its line counts exclude `mod tests`, and every
    /// gate here means the same thing: a fixture that writes a scratch `pom.xml`
    /// is not a production `fs::write`, and a test helper taking `root: &Path`
    /// is not the primitive being propagated. Counting them makes the ladder
    /// punish the tests that prove a rung did not change behaviour.
    production: String,
}

/// Every production Rust file in the workspace, not only the binary's own.
///
/// The binary is one crate of seven. A scanner that walked `src/` alone would
/// keep reporting green while the code it gates moved into `crates/*/src` --
/// the same failure as a skipped tier-3 test, which the suite also reports as
/// passing unless something insists otherwise.
fn sources() -> Vec<Source> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    collect(root.join("src").as_path(), &mut files);
    let crates = root.join("crates");
    if crates.is_dir() {
        let mut members: Vec<PathBuf> = fs::read_dir(&crates)
            .expect("failed to read crates/")
            .map(|entry| entry.expect("failed to read a crates/ entry").path())
            .collect();
        members.sort();
        for member in members {
            let src = member.join("src");
            if src.is_dir() {
                collect(&src, &mut files);
            }
        }
    }
    assert!(
        files.len() > 30,
        "the workspace scanner found only {} files -- it has lost track of where \
         the code lives, and every gate below would report green over code it \
         never read",
        files.len()
    );
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files
}

fn collect(dir: &Path, out: &mut Vec<Source>) {
    let entries =
        fs::read_dir(dir).unwrap_or_else(|e| panic!("failed to read {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("failed to read a directory entry").path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
            let production = without_test_modules(&blank(&source));
            out.push(Source { path, production });
        }
    }
}

/// Replace comments and string literals with spaces of the same length.
///
/// The same trick as `src/java.rs::blanked`, and for the same reason: a scan
/// must not be fooled by `// root: &Path` or by the word `fn` inside one of
/// `spring.rs`'s inline Java bodies, while offsets and line numbers still line
/// up with the file on disk.
fn blank(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len());
    let mut i = 0;
    while i < bytes.len() {
        let rest = &source[i..];
        if rest.starts_with("//") {
            while i < bytes.len() && bytes[i] != b'\n' {
                out.push(' ');
                i += 1;
            }
        } else if rest.starts_with("/*") {
            let mut depth = 0;
            while i < bytes.len() {
                if source[i..].starts_with("/*") {
                    depth += 1;
                    out.push_str("  ");
                    i += 2;
                } else if source[i..].starts_with("*/") {
                    depth -= 1;
                    out.push_str("  ");
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    out.push(if bytes[i] == b'\n' { '\n' } else { ' ' });
                    i += 1;
                }
            }
        } else if rest.starts_with('r') && rest[1..].starts_with(['#', '"']) {
            // A raw string: r"..", r#".."#, r##".."##, and so on.
            let hashes = rest[1..].bytes().take_while(|b| *b == b'#').count();
            if rest[1 + hashes..].starts_with('"') {
                let close = format!("\"{}", "#".repeat(hashes));
                let open = 1 + hashes + 1;
                out.push_str(&" ".repeat(open));
                i += open;
                while i < bytes.len() && !source[i..].starts_with(&close) {
                    out.push(if bytes[i] == b'\n' { '\n' } else { ' ' });
                    i += 1;
                }
                let closing = close.len().min(bytes.len().saturating_sub(i));
                out.push_str(&" ".repeat(closing));
                i += closing;
            } else {
                out.push('r');
                i += 1;
            }
        } else if bytes[i] == b'"' {
            out.push(' ');
            i += 1;
            while i < bytes.len() && bytes[i] != b'"' {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    // A trailing backslash is Rust's line continuation, and
                    // eating its newline would make every line count here read
                    // low -- which is the whole measurement for two gates.
                    out.push(' ');
                    out.push(if bytes[i + 1] == b'\n' { '\n' } else { ' ' });
                    i += 2;
                    continue;
                }
                out.push(if bytes[i] == b'\n' { '\n' } else { ' ' });
                i += 1;
            }
            if i < bytes.len() {
                out.push(' ');
                i += 1;
            }
        } else if bytes[i] == b'\'' && char_literal_len(&source[i..]).is_some() {
            let len = char_literal_len(&source[i..]).expect("checked above");
            out.push_str(&" ".repeat(len));
            i += len;
        } else {
            let ch = source[i..].chars().next().expect("in bounds");
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

/// Blank the body of every `#[cfg(test)]` module, preserving offsets.
fn without_test_modules(blanked: &str) -> String {
    let bytes = blanked.as_bytes();
    let mut out = blanked.to_string();
    let mut search = 0;
    while let Some(offset) = blanked[search..].find("#[cfg(test)]") {
        let at = search + offset;
        search = at + "#[cfg(test)]".len();
        // Only a module has a body worth erasing; `#[cfg(test)]` on a single
        // helper fn is erased by the same brace walk, which is also correct.
        let Some(open) = blanked[search..].find('{').map(|i| search + i) else {
            break;
        };
        let mut depth = 0usize;
        let mut close = open;
        for (index, byte) in bytes.iter().enumerate().skip(open) {
            match byte {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        close = index;
                        break;
                    }
                }
                _ => {}
            }
        }
        if close <= open {
            break;
        }
        let blanked_body: String = blanked[at..=close]
            .chars()
            .map(|c| if c == '\n' { '\n' } else { ' ' })
            .collect();
        out.replace_range(at..=close, &blanked_body);
        search = close;
    }
    out
}

/// The length of a `'a'`-style character literal, or `None` for a lifetime.
fn char_literal_len(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    if bytes.first() != Some(&b'\'') {
        return None;
    }
    if bytes.get(1) == Some(&b'\\') {
        return rest[1..].find('\'').map(|end| end + 2);
    }
    let ch = rest[1..].chars().next()?;
    let end = 1 + ch.len_utf8();
    (bytes.get(end) == Some(&b'\'')).then_some(end + 1)
}

/// `root: &Path` in a **parameter** position, in either spelling.
///
/// `&std::path::Path` counts too. Measuring one spelling means a conversion
/// that removes the other reads as no progress at all, which is how a gate
/// quietly stops describing the thing it is named after.
///
/// Deliberately not a plain substring count: a recipe mid-migration binds
/// `let root: &Path = slice.project().root();` inside its body, which is the
/// primitive being *contained* rather than propagated. Counting those made the
/// gate read flat while the parameter it exists to remove was disappearing --
/// a measurement that cannot see the improvement it is asking for.
fn root_path_parameters(src: &[Source]) -> usize {
    src.iter()
        .map(|file| {
            ["root: &Path", "root: &std::path::Path"]
                .iter()
                .flat_map(|spelling| file.production.match_indices(spelling))
                .filter(|(at, _)| !file.production[..*at].trim_end().ends_with("let"))
                // `module_root: &Path` and `workspace_root: &Path` are not
                // this parameter. Counting them inflated the number by six and
                // made `project.rs` -- which walks a reactor and *must* read
                // each pom along the way -- look like the disease.
                .filter(|(at, _)| {
                    file.production[..*at]
                        .chars()
                        .next_back()
                        .is_none_or(|before| !before.is_alphanumeric() && before != '_')
                })
                .count()
        })
        .sum()
}

/// Root-taking pom readers where reading again is the *correct* behaviour, and
/// passing the caller's `Project` would be the bug.
///
/// The distinction rung 1 is actually about: envy is asking the disk for a fact
/// somebody already resolved. These four ask the disk because the resolved
/// answer is **stale or absent** -- jails has been splicing the pom in the same
/// run, or there is no project yet. Declared rather than counted, because a
/// number nobody can reach is a gate nobody reads; the pattern is
/// `SILENT_WITHOUT_A_RECORD`'s, and a stale entry here fails the test below the
/// same way.
const A_FRESH_READ_IS_CORRECT: &[(&str, &str)] = &[
    (
        "apply_in",
        "`jails new --app` has just created the project on disk, so there is no earlier \
         `Project` for this to be a second read of. Passing one in would mean resolving it \
         at the call site instead, which is the same read one frame up",
    ),
    (
        "ensure_console_launcher",
        "the same: it checks for and splices `junit-platform-console`",
    ),
    (
        "ensure_package_info",
        "reached from `write_new_file`, whose nine callers include `new` -- which is \
         creating the very pom this asks about, so there is no project to have resolved",
    ),
    (
        "read_build_file",
        "it is the read a `Project` is *constructed from*, not a second one taken beside \
         it. Both `load` and `inspect` go through it precisely so there is one answer: \
         `inspect` reading `pom.xml` unconditionally while `load` had learned about \
         `build.gradle` is what made `doctor` report a Gradle project as having no build \
         file at all",
    ),
];

/// Functions that exist *to* turn a path into project facts, so re-deriving is
/// their whole job rather than envy of someone else's.
///
/// `new` is here in full: it runs before a project exists to resolve, which is
/// the one situation where there is no `Project` to have been passed.
const DERIVATION_IS_THE_JOB: &[&str] = &[
    "load",
    "inspect",
    "base_package",
    "project_with_pom",
    "verify_requested_deps",
    "add_jspecify",
    "write_agents",
    "ensure_enforcer",
];

/// `(file, function)` for every `root: &Path` function that goes back to disk
/// for something a resolved `Project` already holds.
fn rederivers(src: &[Source]) -> Vec<(String, String)> {
    // Applied to `root` specifically. Without the argument this counted
    // `reconcile_intent`, which loads a `Project` for each of two *scratch*
    // copies of the tree -- the opposite of envy, since there is no resolved
    // project for those roots to have been passed.
    const FACTS: &[&str] = &[
        "pom::read(root)",
        "base_package(root)",
        "Project::load(root)",
        "Project::inspect(root)",
        "Config::load(root)",
    ];
    let mut found = Vec::new();
    for file in src {
        let lines: Vec<&str> = file.production.lines().collect();
        let mut index = 0;
        while index < lines.len() {
            let trimmed = lines[index].trim_start();
            if !trimmed.contains("fn ") {
                index += 1;
                continue;
            }
            let indent = lines[index].len() - trimmed.len();
            let close = format!("{}}}", " ".repeat(indent));
            let Some(end) = (index..lines.len()).find(|at| lines[*at] == close) else {
                index += 1;
                continue;
            };
            let body = lines[index..=end].join("\n");
            let signature = body.split('{').next().unwrap_or_default();
            let name = signature
                .split("fn ")
                .nth(1)
                .and_then(|rest| rest.split(['(', '<']).next())
                .unwrap_or_default()
                .to_string();
            let takes_root = signature.match_indices("root: &Path").any(|(at, _)| {
                signature[..at]
                    .chars()
                    .next_back()
                    .is_none_or(|before| !before.is_alphanumeric() && before != '_')
            });
            if takes_root
                && FACTS.iter().any(|fact| body.contains(fact))
                && !DERIVATION_IS_THE_JOB.contains(&name.as_str())
            {
                found.push((file.path.display().to_string(), name));
            }
            index = end + 1;
        }
    }
    found
}

/// Lines of a file that are neither blank nor inside a `#[cfg(test)]` module.
fn production_lines(file: &Source) -> usize {
    file.production
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count()
}

fn count_matches(src: &[Source], needle: &str) -> usize {
    src.iter()
        .map(|file| file.production.matches(needle).count())
        .sum()
}

/// Count `fn` declarations in one file whose parameter list exceeds five.
fn over_five_params(src: &[Source], file_suffix: &str) -> usize {
    src.iter()
        .filter(|file| file.path.ends_with(file_suffix))
        .map(|file| {
            fn_param_counts(&file.production)
                .into_iter()
                .filter(|(_, count)| *count > 5)
                .count()
        })
        .sum()
}

/// Every `fn` in blanked Rust, with the number of top-level parameters.
fn fn_param_counts(blanked: &str) -> Vec<(String, usize)> {
    let bytes = blanked.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(offset) = blanked[i..].find("fn ") {
        let at = i + offset;
        let boundary = at == 0 || !is_ident(bytes[at - 1]);
        i = at + 3;
        if !boundary {
            continue;
        }
        let after = &blanked[i..];
        let name: String = after
            .trim_start()
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            continue;
        }
        // Skip any generic parameter list, then find the argument list.
        let Some(open) = find_param_list(&blanked[i..]) else {
            continue;
        };
        let start = i + open;
        let Some(end) = matching_paren(blanked, start) else {
            continue;
        };
        out.push((name, top_level_commas(&blanked[start + 1..end])));
        i = end;
    }
    out
}

fn is_ident(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// The offset of the `(` opening a function's parameter list, skipping any
/// generic parameters, which may themselves contain parentheses in a bound.
fn find_param_list(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    let mut i = 0;
    let mut angle = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'<' => angle += 1,
            b'>' => angle = angle.saturating_sub(1),
            b'(' if angle == 0 => return Some(i),
            b'{' | b';' if angle == 0 => return None,
            _ => {}
        }
        i += 1;
    }
    None
}

fn matching_paren(source: &str, open: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 0usize;
    for (offset, byte) in bytes.iter().enumerate().skip(open) {
        match byte {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(offset);
                }
            }
            _ => {}
        }
    }
    None
}

/// The number of parameters in a blanked argument list.
///
/// Counted as non-empty top-level segments, not as commas plus one: Rust
/// permits a trailing comma, and every multi-line signature in this codebase
/// has one, so counting commas overstates every wrapped signature by exactly
/// one while leaving single-line ones correct. That is the worst shape of
/// measurement bug -- consistent enough to look right.
fn top_level_commas(inner: &str) -> usize {
    let mut depth = 0i32;
    let mut params = 0;
    let mut segment_has_content = false;
    for ch in inner.chars() {
        match ch {
            '(' | '[' | '<' | '{' => depth += 1,
            ')' | ']' | '>' | '}' => depth -= 1,
            ',' if depth == 0 => {
                if segment_has_content {
                    params += 1;
                }
                segment_has_content = false;
                continue;
            }
            _ => {}
        }
        if !ch.is_whitespace() {
            segment_has_content = true;
        }
    }
    params + usize::from(segment_has_content)
}

/// A declaration of `keyword`, at any visibility.
///
/// Spelling the visibilities out cost a gate its sight: the workspace split
/// turned `pub(crate) struct` into `pub struct` inside a moved crate, and a
/// scanner matching only the first two spellings reported zero — an improvement
/// that had not happened, over code it could no longer see.
fn is_item(line: &str, keyword: &str) -> bool {
    let rest = line
        .strip_prefix("pub(crate) ")
        .or_else(|| line.strip_prefix("pub(super) "))
        .or_else(|| line.strip_prefix("pub "))
        .unwrap_or(line);
    rest.starts_with(keyword) && rest[keyword.len()..].starts_with(' ')
}

fn body_carrying_structs(src: &[Source]) -> usize {
    let mut found = 0;
    for file in src {
        let mut in_struct = false;
        for line in file.production.lines() {
            let trimmed = line.trim();
            if is_item(trimmed, "struct") {
                in_struct = trimmed.ends_with('{');
                continue;
            }
            if in_struct {
                if trimmed == "}" {
                    in_struct = false;
                } else if trimmed.contains("contents: String") || trimmed.contains("body: String") {
                    found += 1;
                    in_struct = false;
                }
            }
        }
    }
    found
}

/// Positional `(PathBuf, String, ..)` tuples standing in for `model::Artifact`.
fn file_tuple_types(src: &[Source]) -> usize {
    src.iter()
        .map(|file| {
            file.production.matches("(PathBuf, String").count()
                + file
                    .production
                    .matches("(std::path::PathBuf, String")
                    .count()
        })
        .sum()
}

/// `type X = Change;`-style aliases for the one shared shape.
fn type_aliases(src: &[Source]) -> usize {
    src.iter()
        .map(|file| {
            file.production
                .lines()
                .filter(|line| {
                    let line = line.trim();
                    is_item(line, "type")
                })
                .filter(|line| line.contains("= Change;") || line.contains("= Artifact;"))
                .count()
                + file.production.matches("Artifact as NewFile").count()
                + file.production.matches("Change as Plan").count()
        })
        .sum()
}

/// The bypass count reads a literal, so the literal must be the only spelling.
///
/// `use jails_support::apply::put;` would let a call be written as bare `put(`,
/// which `executor_bypasses` cannot see -- and a gate that can be stepped around
/// by an import is the failure mode this whole file exists to prevent, arriving
/// through the door it was built to watch.
#[test]
fn no_bare_apply_verb_imports() {
    let offenders: Vec<_> = sources()
        .into_iter()
        .filter(|file| {
            file.production.lines().any(|line| {
                let line = line.trim();
                line.starts_with("use ") && (line.contains("apply::{") || line.contains("apply::*"))
            })
        })
        .map(|file| file.path.display().to_string())
        .collect();
    assert!(
        offenders.is_empty(),
        "these files import `apply`'s verbs by name, so their writes are invisible to the \
         `mutations that bypass the executor` gate. Spell the call `apply::put(..)` in full:\n  {}",
        offenders.join("\n  ")
    );
}

/// An `encode`/`decode` half that is not a `Codec` method.
///
/// The signatures are what identify one: `encode(&self, encoder: &mut Encoder)`
/// and `decode(decoder: &mut Decoder<'_>)`. A method with either shape sitting
/// in an inherent `impl` is a value on the wire that `Encoder::seq`,
/// `Encoder::set` and `Encoder::map` cannot be used on, so its collection
/// handling has to be written out again by hand.
fn inherent_codec_halves(src: &[Source]) -> usize {
    let mut count = 0;
    for file in src {
        let mut in_codec_impl = false;
        for line in file.production.lines() {
            // `trim_start`, because `digest_newtype!` and `logical_id!`
            // expand to `impl Codec for $name` indented inside a
            // `macro_rules!` body -- and a scanner that read column zero
            // would report six perfectly good trait impls as violations.
            let head = line.trim_start();
            if head.starts_with("impl ") {
                in_codec_impl = head.starts_with("impl Codec for ");
            }
            // A declaration, not a definition: the trait's own two lines.
            let is_half = !head.ends_with(';')
                && (head.contains("fn encode(&self, encoder: &mut Encoder)")
                    || head.contains("fn decode(decoder: &mut Decoder<'_>)"));
            if is_half && !in_codec_impl {
                count += 1;
            }
        }
    }
    count
}

/// Where the count stands today. Lowered per message, never by a sweep.
const REFUSALS_WITHOUT_A_FIX: usize = 443;

/// A refusal that builds a message and does not say what to do next.
///
/// Located on the blanked production text -- so `#[cfg(test)]` bodies are out
/// and parentheses inside string literals cannot confuse the scan -- and then
/// *read* from the raw file at the same byte offsets, because the message is
/// exactly what blanking erases.
///
/// Only calls whose argument contains a string literal count. `Err(error)`,
/// `Err(Failure::Reported)` and `Err(CommitError::Io(..))` are forwarding a
/// refusal somebody else worded, and a `fix:` is that somebody's job.
fn refusals_without_a_fix(src: &[Source]) -> usize {
    let mut count = 0;
    for file in src {
        let raw = fs::read_to_string(&file.path).expect("this file was read once already");
        if raw.len() != file.production.len() {
            // Blanking preserves length; if it ever stops, this gate is reading
            // the wrong bytes and should say so rather than report a number.
            panic!("{} blanked to a different length", file.path.display());
        }
        let bytes = file.production.as_bytes();
        for (at, _) in file.production.match_indices("Err(") {
            // `.map_err(` and similar are not refusal sites of their own.
            if at > 0 && (bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_') {
                continue;
            }
            let Some(end) = matching_paren(&file.production, at + 3) else {
                continue;
            };
            let argument = &raw[at + 4..end];
            if !argument.contains('"') {
                continue;
            }
            if !argument.contains("fix:") {
                count += 1;
            }
        }
    }
    count
}

fn write_sites_outside_apply(src: &[Source]) -> usize {
    mutation_sites(src, &["fs::write"])
}

/// Every API that changes the filesystem, wherever it is spelled.
///
/// plan.md §R6.4: the gate "currently bans only literal `fs::write`; it must
/// expand to `write`, `OpenOptions` write modes, `remove_file/remove_dir`,
/// `copy`, `rename`, hard links, directory creation, permissions and mutating
/// subprocesses." The reason the narrow version was not enough is visible in
/// the count: `fs::write` was at zero while a dozen other calls mutated the
/// project through other names, so the gate read green over exactly the
/// surface R6 has to migrate.
/// Where the count stands today. Lowered by each migrated surface.
const MUTATION_CEILING: usize = 6;

const MUTATION_APIS: &[&str] = &[
    "fs::write",
    "fs::remove_file",
    "fs::remove_dir",
    "fs::remove_dir_all",
    "fs::copy",
    "fs::rename",
    "fs::hard_link",
    "fs::create_dir",
    "fs::create_dir_all",
    "fs::set_permissions",
    "set_len(",
    "create_new(true)",
];

/// Writes that reach the filesystem outside a transaction.
///
/// `apply::*` is the *write layer*, not the executor. It is one owner for the
/// bytes -- which is what the `fs::write` gate above buys -- but a call to it
/// from a generator happens immediately: nothing journals it, `--pretend`
/// cannot report it, and a failure half way through a capability leaves the
/// project in a state no `continue` or `abort` can reach. The executor
/// (`execute.rs` + `activate.rs`, driven from `jails-commit`) is what supplies
/// those three, and R6.4's rung is that every mutation goes through it.
///
/// So this counts the calls that do not. `apply::` is spelled in full at every
/// call site -- there is no `use apply::put` anywhere in the workspace, which
/// `no_bare_apply_verb_imports` holds -- so the literal is the count.
fn executor_bypasses(src: &[Source]) -> usize {
    src.iter()
        .filter(|file| !owns_writing(&file.path))
        .map(|file| {
            let all = file.production.matches("apply::").count();
            // Two verbs say in their own names that they are not writing into a
            // project, which is what this row is about. `put_outside_project`
            // is deliberately long so `jails setup`'s `~/.testcontainers
            // .properties` and `testd`'s daemon source cannot be reached by
            // accident from anything editing a project; `put_in_scratch` writes
            // a tree jails created empty moments earlier and removes when the
            // run ends. Counting them made the gate ask for something that
            // would be wrong to do -- there is no transaction to put a write
            // outside every project into.
            let exempt = file
                .production
                .matches("apply::put_outside_project")
                .count()
                + file.production.matches("apply::put_in_scratch").count();
            all - exempt
        })
        .sum()
}

fn mutation_sites(src: &[Source], apis: &[&str]) -> usize {
    src.iter()
        .filter(|file| !owns_writing(&file.path))
        .map(|file| {
            apis.iter()
                .map(|api| whole_calls(&file.production, api))
                .sum::<usize>()
        })
        .sum()
}

/// Count a call name, not a prefix of one.
///
/// `fs::create_dir_all` contains `fs::create_dir`, and `fs::remove_dir_all`
/// contains `fs::remove_dir`, so a substring count reports every such call
/// twice. A gate that inflates its own number is a gate whose progress cannot
/// be read.
fn whole_calls(source: &str, name: &str) -> usize {
    source
        .match_indices(name)
        .filter(|(at, _)| {
            source[at + name.len()..]
                .chars()
                .next()
                .map(|next| !next.is_alphanumeric() && next != '_')
                .unwrap_or(true)
        })
        .count()
}

/// The modules whose *subject* is changing the filesystem.
///
/// `apply` is the project's write layer. `store`, `journal` and `execute` are
/// the executor's: R4's whole point is that a commit publishes bytes through
/// a protocol, and that protocol is made of exactly these calls. `scratch`
/// and `sandbox` own trees jails creates and destroys within one run.
fn owns_writing(path: &Path) -> bool {
    let owns = [
        "apply",
        "store.rs",
        "journal.rs",
        "execute.rs",
        // The half of the executor that actually moves bytes. It was inside
        // `execute.rs` until that module outgrew the size ceiling; splitting a
        // file must not change what the project's write layer *is*.
        "activate.rs",
        "scratch.rs",
        "sandbox.rs",
        "recover.rs",
        "gc.rs",
        "lock.rs",
        // `jails new` has no project to lock and no ledger to journal, so the
        // guarantee the executor gives is bought a different way: everything
        // lands in a reserved scratch that is published by one `rename` or
        // discarded entire. This module owns that, and `Tree` is what makes
        // it checkable -- a `Tree` comes from a `Publication` and nowhere
        // else, and its absolute-path verbs refuse a write outside the tree.
        // `pending.md` §5.
        "publish.rs",
    ];
    path.components().any(|part| part.as_os_str() == "apply")
        || owns
            .iter()
            .any(|name| path.file_name().map(|file| file == *name).unwrap_or(false))
}

fn inline_java_bodies(src: &[Source]) -> usize {
    // Counted on the *raw* source: `blank` deliberately erases these bodies,
    // which is what makes them invisible to every other measurement here.
    src.iter()
        .filter(|file| file.path.ends_with("spring.rs"))
        .map(|file| {
            fs::read_to_string(&file.path)
                .expect("spring.rs was read once already")
                .matches("r#\"package ")
                .count()
        })
        .sum()
}

#[cfg(test)]
mod blanking_tests {
    use super::*;

    /// Which module is currently the largest, printed for the ladder board.
    #[test]
    fn report_the_largest_modules() {
        let src = sources();
        let mut rows: Vec<(usize, String)> = src
            .iter()
            .map(|file| (production_lines(file), file.path.display().to_string()))
            .collect();
        rows.sort_by_key(|row| std::cmp::Reverse(row.0));
        println!("\nlargest production modules");
        for (lines, path) in rows.iter().take(8) {
            println!("  {lines:5}  {path}");
        }
    }

    #[test]
    fn blanking_erases_comments_and_literals_but_preserves_offsets() {
        let source = "let a = \"root: &Path\"; // root: &Path\nlet b = 1;\n";
        let blanked = blank(source);
        assert_eq!(blanked.len(), source.len(), "offsets must still line up");
        assert_eq!(blanked.lines().count(), source.lines().count());
        assert!(
            !blanked.contains("root: &Path"),
            "a literal and a comment both hid a countable token: {blanked:?}"
        );
    }

    #[test]
    fn blanking_erases_raw_strings_holding_java() {
        let source = "let java = r#\"package a; class B { void fn(int x) {} }\"#;\n";
        let blanked = blank(source);
        assert_eq!(blanked.len(), source.len());
        assert!(!blanked.contains("package"), "{blanked:?}");
        assert!(
            fn_param_counts(&blanked).is_empty(),
            "a Java method must not be counted as a Rust fn"
        );
    }

    #[test]
    fn parameters_are_counted_at_the_top_level_only() {
        let counts = fn_param_counts(
            "fn a(x: Result<A, B>, y: (u8, u8)) {}\nfn b() {}\nfn c<T: Into<X>>(t: T) {}\n",
        );
        assert_eq!(counts[0], ("a".to_string(), 2), "{counts:?}");
        assert_eq!(counts[1], ("b".to_string(), 0), "{counts:?}");
        assert_eq!(counts[2], ("c".to_string(), 1), "{counts:?}");
    }

    #[test]
    fn a_trailing_comma_does_not_invent_a_parameter() {
        let wrapped = fn_param_counts("fn a(\n    x: u8,\n    y: u8,\n) {}\n");
        let inline = fn_param_counts("fn a(x: u8, y: u8) {}\n");
        assert_eq!(wrapped[0].1, 2, "{wrapped:?}");
        assert_eq!(inline[0].1, 2, "{inline:?}");
    }

    #[test]
    fn a_lifetime_is_not_read_as_a_character_literal() {
        let source = "fn a<'a>(x: &'a str, y: char) {}\n";
        let blanked = blank(source);
        assert_eq!(blanked, source, "nothing here should be blanked");
        assert_eq!(fn_param_counts(&blanked)[0].1, 2);
    }
}
