//! **The rules**: properties that are true or false, not numbers.
//!
//! A module may not reach up a layer. Every module that starts a subprocess is
//! classified. Every fresh read of the POM is a decision somebody wrote down.
//! A scratch directory is reserved, never named. None of these has a ceiling —
//! they hold or they do not.
//!
//! The two tables the layering rests on live here too, because they *are* the
//! rule: `LAYERS` says which crate a module ships in and
//! `SUBPROCESS_CLASSIFICATION` says which row a process-starting module is.

use crate::board::gates;
use crate::measure::*;
use std::path::Path;

/// Modules whose contract is to render a command result or transparently
/// forward a child process' output. A production `print*` macro anywhere else
/// bypasses the result/report protocol and is indistinguishable from forgotten
/// debug output to both human and JSON callers.
fn owns_terminal_output(path: &Path) -> bool {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let relative = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");

    relative == "src/main.rs"
        || relative == "src/dispatch.rs"
        || relative == "src/new.rs"
        || relative == "src/sql_command.rs"
        || relative == "src/schema_command.rs"
        || relative == "src/editor_command.rs"
        || relative.starts_with("src/new/")
        || relative == "crates/jails-support/src/lib.rs"
        || relative == "crates/jails-support/src/process.rs"
        || relative == "crates/jails-java/src/template.rs"
        || relative == "crates/jails-project/src/compose.rs"
        || relative == "crates/jails-project/src/inspect.rs"
        || relative == "crates/jails-project/src/project.rs"
        || relative == "crates/jails-generate/src/generate.rs"
        || relative == "crates/jails-generate/src/generate/recipes.rs"
        || relative.starts_with("crates/jails-drive/src/")
        || relative.starts_with("crates/jails-report/src/")
        || relative == "crates/jails-engine/src/route/capability.rs"
        || relative.starts_with("crates/jails-engine/src/route/maintenance/")
}

#[test]
fn only_deliberate_output_modules_print_to_the_terminal() {
    let mut offenders = Vec::new();
    for file in sources() {
        if owns_terminal_output(&file.path) {
            continue;
        }
        for spelling in ["println!", "dbg!"] {
            for (at, _) in file.production.match_indices(spelling) {
                let line = file.production[..at]
                    .bytes()
                    .filter(|byte| *byte == b'\n')
                    .count()
                    + 1;
                offenders.push(format!("  {}:{line}: {spelling}", file.path.display()));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "production code printed outside a deliberate terminal-output module:\n{}\n\n\
         Return structured data or a diagnostic to the owning report/CLI layer. If a module's \
         public contract genuinely is terminal output, classify it in `owns_terminal_output` \
         with that boundary change.",
        offenders.join("\n")
    );
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
    ("jails-protocol", "compatibility", 3),
    ("jails-protocol", "durable", 3),
    ("jails-protocol", "intent", 3),
    ("jails-protocol", "observe", 3),
    ("jails-protocol", "vocabulary", 3),
    // jails-state: `.jails/` and what a directory holds. Below the Java
    // project on purpose -- `jails-commit` needs both and neither is about Java.
    ("jails-state", "compat", 4),
    ("jails-state", "listing", 4),
    // jails-project: the resolved project and everything jails records about it.
    ("jails-project", "application_manifest", 5),
    ("jails-project", "query_workspace", 5),
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
    ("jails-project", "query_compiler", 5),
    ("jails-project", "schema", 5),
    ("jails-project", "generated_files", 5),
    ("jails-project", "inspect", 5),
    // jails-generate: everything that decides what Java to write.
    ("jails-generate", "sql", 6),
    ("jails-generate", "generate", 6),
    ("jails-generate", "named_query", 6),
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
    ("jails-prepare", "review", 6),
    ("jails-prepare", "sandbox", 6),
    ("jails-prepare", "serialize", 6),
    ("jails-prepare", "timing", 6),
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
    ("jails-report", "lifecycle_status", 7),
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
    ("jails-drive", "live_sql", 8),
    // jails-cli: the binary and the whole-project lifecycle commands.
    ("jails", "new", 9),
    ("jails", "app", 9),
    ("jails", "sql_command", 9),
    ("jails", "schema_command", 9),
    ("jails", "editor_command", 9),
    ("jails", "cli", 9),
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
    ("live_sql", "read-only probe"),
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
        if file.path.ends_with(SCRATCH_RS) {
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
