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
        || relative == "src/schema_command/render.rs"
        || relative == "src/editor_command.rs"
        || relative == "src/contract_command.rs"
        || relative == "src/tool_command.rs"
        || relative == "src/model_command.rs"
        || relative == "src/model_generate.rs"
        // The two halves `model_generate` was split into, and both are here for
        // its reason rather than a new one. `report` is the preview, the
        // disabled-test list and the deletion prompt -- the whole of what a
        // reader is shown about a plan; `effects` is what is said on the way
        // past once the transition is durable, where a service that is not up
        // or a formatter that could not run has to be named or nobody learns
        // of it.
        || relative == "src/model_generate/report.rs"
        || relative == "src/model_generate/effects.rs"
        // Gives a project jails never created its first model, and saying so
        // is part of the command: a reader who has just made their repository
        // canonical needs to know that generation has moved to
        // `.jails/generated` and their own sources have not.
        || relative == "src/model_init.rs"
        // A read-only report whose entire contract is terminal output:
        // JDL v1 §18.4 asks that a derived name be *inspectable*, and a
        // command that returned the records to a caller with nowhere to print
        // them would satisfy the type and not the requirement.
        || relative == "src/model_explain.rs"
        // The canonical half of `resource status`, and a read-only report
        // whose whole contract is the four authority lines it prints. The
        // legacy half is already allowed through `jails-report`.
        || relative == "src/model_status.rs"
        || relative == "src/parse_error.rs"
        // `app init` prints the file it seeded and what to do with it, and
        // `app plan` prints the manifest's declarations -- a plan that
        // returned them to a caller with nowhere to print them would satisfy
        // the type and not the requirement.
        || relative == "src/app.rs"
        // The textual rename's whole contract is the list it prints before it
        // asks for `--force`: every file it will edit and every one it will
        // move, so `--dry-run` is a review rather than a promise.
        || relative == "src/rename_source.rs"
        || relative.starts_with("src/new/")
        || relative == "crates/jails-support/src/lib.rs"
        || relative == "crates/jails-support/src/process.rs"
        || relative == "crates/jails-project/src/template.rs"
        || relative == "crates/jails-project/src/compose.rs"
        || relative == "crates/jails-project/src/inspect.rs"
        || relative == "crates/jails-project/src/project.rs"
        || relative.starts_with("crates/jails-drive/src/")
        || relative.starts_with("crates/jails-report/src/")
        // The two commands that run *before* a project has a model. Both
        // report what they read and what they would change, and both are
        // interactive by contract -- `adopt` prints a classification the
        // reader is meant to check before it writes a `[layout]` table.
        || relative == "src/adopt.rs"
        || relative == "src/modernize.rs"
        || relative == "src/sql_command.rs"
        || relative == "src/contract_command.rs"
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

    println!("\narchitecture gates — status\n");
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
             rung {}\n  Lower this row's `ceiling` to {actual} in tests/architecture/board.rs. An \
             improvement that is not recorded here is one the next change may silently \
             undo.\n",
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
/// The crate layering, as a test, so it holds for module-level edges the
/// compiler will never see.
///
/// Every module is assigned the crate it belongs to, and a module may only
/// reference one at its own level or below. A cycle is a boundary nothing can
/// enforce, and an unenforced boundary is how a module comes to keep its own
/// copy of a shared list and silently report against it.
///
/// Same-level edges are allowed, including mutual ones: two modules that call
/// each other and ship in the same crate is a design decision rather than an
/// accident.
/// Every gate that names a file must name one that is there.
///
/// A gate keyed by path has two failure modes, and only one of them is loud.
/// Pointing at the *wrong* file drags rows red. Pointing at a file that does
/// not exist is silent: the exclusion excludes nothing, or the measurement
/// measures nothing, and the row keeps printing a number nobody can tell from
/// a real one.
#[test]
fn every_path_a_gate_names_is_a_file_the_scanner_found() {
    let files = sources();
    for (constant, path) in [
        ("BUILTIN_RS", BUILTIN_RS),
        ("CODEMOD_RS", CODEMOD_RS),
        ("GIT_RS", GIT_RS),
        ("DOCTOR_RS", DOCTOR_RS),
        ("SCRATCH_RS", SCRATCH_RS),
    ] {
        assert!(
            files.iter().any(|file| file.path.ends_with(path)),
            "`{constant}` names `{path}`, which the workspace scanner did not find. \
             Either the file moved and the constant did not, or it was deleted -- and \
             until one of those is fixed, every gate keyed by it is measuring nothing \
             while reporting a number."
        );
    }
}

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
            // `crate::model` names this crate's `model` when it has one. It
            // cannot simultaneously name another crate's module with the same
            // basename. Module identity is `(crate, module)`; resolving by the
            // second half alone collides same-named modules in two crates.
            if LAYERS.iter().any(|(candidate_crate, candidate, _)| {
                *candidate_crate == krate && *candidate == *other
            }) {
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
    // and it hides the fact that the module went. `main.rs` is excluded by
    // `module_of` by design.
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
/// `module_of` answers `(crate, module)`, so two crates may declare the same
/// top-level module name and a production module is called what it is. What
/// the pair still has to have is uniqueness: a duplicate row would make the
/// `find` above pick whichever came first.
#[test]
fn layers_lists_each_module_once() {
    let mut names: Vec<(&str, &str)> = LAYERS.iter().map(|(c, m, _)| (*c, *m)).collect();
    names.sort_unstable();
    let before = names.len();
    names.dedup();
    assert_eq!(before, names.len(), "`LAYERS` lists one module twice");
}

/// Which crate each module ships in, lowest first. Every module is
/// classified, so deleting one cannot silently loosen a boundary.
const LAYERS: &[(&str, &str, usize)] = &[
    ("jails-model", "app", 2),
    ("jails-model", "artifact", 2),
    ("jails-model", "build", 2),
    ("jails-model", "builtin", 2),
    ("jails-model", "capability", 2),
    ("jails-model", "component", 2),
    ("jails-model", "constraint", 2),
    ("jails-model", "layout", 2),
    // jails-support: no jails concepts at all -- writing, running, encoding.
    // `jails-codemod` depends on nothing at all -- it knows one text format
    // and no more -- so it sits beside the support primitives, where every
    // crate on either ladder can reach it.
    ("jails-codemod", "annotate", 0),
    ("jails-codemod", "dispatch", 0),
    ("jails-codemod", "marked", 0),
    ("jails-codemod", "text", 0),
    ("jails-codemod", "tidy", 0),
    ("jails-support", "apply", 0),
    ("jails-support", "process", 0),
    ("jails-support", "hermetic", 0),
    ("jails-support", "unified", 0),
    ("jails-support", "scratch", 0),
    ("jails-support", "digest", 0),
    ("jails-support", "git", 0),
    ("jails-support", "identifier", 0),
    ("jails-support", "identity", 0),
    ("jails-support", "json", 0),
    ("jails-support", "lock", 0),
    // jails-spec: what a jails project is -- where it is, how it is laid out,
    // what a field means, and the closed CLI vocabularies.
    ("jails-spec", "build", 2),
    ("jails-spec", "release", 2),
    ("jails-spec", "spec", 2),
    // Canonical semantic model: closed source schema -> linked stable IDs.
    ("jails-model", "diagnostic", 2),
    ("jails-model", "dependency", 2),
    ("jails-model", "derived", 2),
    ("jails-model", "ejection", 2),
    ("jails-model", "enum_constant", 2),
    ("jails-model", "evolution", 2),
    ("jails-model", "facet", 2),
    ("jails-model", "field_syntax", 2),
    ("jails-model", "guard", 2),
    ("jails-model", "id", 2),
    ("jails-model", "index", 2),
    ("jails-model", "jdl", 2),
    ("jails-model", "projection", 2),
    ("jails-model", "relation", 2),
    ("jails-model", "linker", 2),
    ("jails-model", "model", 2),
    ("jails-model", "naming", 2),
    ("jails-model", "operation", 2),
    ("jails-model", "source", 2),
    ("jails-model", "setting", 2),
    ("jails-model", "unit", 2),
    // Portable values shared by the pure compiler and filesystem boundary.
    ("jails-contracts", "draft", 3),
    ("jails-contracts", "path", 3),
    ("jails-contracts", "plan", 3),
    ("jails-contracts", "snapshot", 3),
    ("jails-contracts", "templates", 3),
    // Pure lowering: semantic world -> desired artifact tree.
    ("jails-compiler", "emit_dto", 4),
    ("jails-compiler", "emit_architecture", 4),
    ("jails-compiler", "emit_capability", 4),
    ("jails-compiler", "emit_component", 4),
    ("jails-compiler", "emit_enum", 4),
    ("jails-compiler", "emit_companion_test", 4),
    ("jails-compiler", "emit_factory", 4),
    ("jails-compiler", "emit_http", 4),
    ("jails-compiler", "emit_java", 4),
    ("jails-compiler", "emit_messaging", 4),
    ("jails-compiler", "emit_mockmvc", 4),
    ("jails-compiler", "emit_operation", 4),
    ("jails-compiler", "emit_relation", 4),
    ("jails-compiler", "emit_resource_http", 4),
    ("jails-compiler", "emit", 4),
    ("jails-compiler", "emit_seed", 4),
    ("jails-compiler", "ejectable", 4),
    ("jails-compiler", "template", 4),
    ("jails-compiler", "emit_sql", 4),
    ("jails-compiler", "refuse", 4),
    ("jails-compiler", "plan_effects", 4),
    ("jails-compiler", "storage", 4),
    ("jails-compiler", "emit_unit", 4),
    // The only canonical materialization/execution owner. It sits above
    // `jails-project`, which captures what it materializes.
    ("jails-workspace", "execute", 6),
    ("jails-workspace", "fault", 6),
    ("jails-workspace", "materialize", 6),
    ("jails-workspace", "reader_facet", 6),
    ("jails-workspace", "reconcile", 6),
    ("jails-workspace", "verify", 6),
    // jails-protocol: the validated values every closed format is built from.
    // jails-state: `.jails/` and what a directory holds. Below the Java
    // project on purpose -- `jails-commit` needs both and neither is about Java.
    // jails-project: the reader. It captures every external fact once
    // (`capture` produces `ProjectFacts` and the captured files, `documents`
    // holds the adapters over the reader's own files and the one Maven
    // reader, `merge` is the three-way merge they share) and resolves the
    // `Project` every command above reads its facts from.
    ("jails-project", "capture", 5),
    ("jails-project", "documents", 5),
    ("jails-project", "merge", 5),
    ("jails-project", "gradle", 5),
    ("jails-project", "maven", 5),
    ("jails-project", "capability", 5),
    ("jails-project", "config", 5),
    ("jails-project", "synonyms", 5),
    ("jails-project", "compose", 5),
    ("jails-project", "feature", 5),
    ("jails-project", "modernize", 5),
    ("jails-project", "project", 5),
    ("jails-project", "properties", 5),
    ("jails-project", "inspect", 5),
    // The Java reader, the class-file reader and the template renderer.
    // Folded in from their own crate (S53.8) once nothing below this crate
    // needed them; `jails-drive`, `jails-report` and the binary reach them
    // through this crate's facade.
    ("jails-project", "java", 5),
    ("jails-project", "classfile", 5),
    ("jails-project", "template", 5),
    // jails-report: commands that answer a question. Read-only by contract,
    // and below `jails-drive` so the contract is structural.
    ("jails-report", "doctor", 7),
    ("jails-report", "diagnostic", 7),
    ("jails-report", "why", 7),
    ("jails-report", "why_subject", 7),
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
    ("jails-drive", "doctor", 8),
    ("jails-drive", "baseline", 8),
    ("jails-drive", "bench", 8),
    ("jails-drive", "reports", 8),
    ("jails-drive", "lint", 8),
    ("jails-drive", "testing", 8),
    // jails-cli: the binary and the whole-project lifecycle commands.
    ("jails", "new", 9),
    ("jails", "adopt", 9),
    ("jails", "modernize", 9),
    ("jails", "app", 9),
    ("jails", "editor_command", 9),
    ("jails", "contract_command", 9),
    ("jails", "tool_command", 9),
    ("jails", "model_command", 9),
    ("jails", "model_capability", 9),
    ("jails", "model_destroy", 9),
    ("jails", "model_eject", 9),
    ("jails", "model_doctor", 9),
    ("jails", "model_explain", 9),
    ("jails", "model_field_evolution", 9),
    ("jails", "model_generate", 9),
    ("jails", "model_generate_jdl", 9),
    ("jails", "model_init", 9),
    ("jails", "model_index", 9),
    ("jails", "model_jdl_edit", 9),
    ("jails", "model_rename", 9),
    ("jails", "model_resource", 9),
    ("jails", "model_setting", 9),
    ("jails", "model_status", 9),
    ("jails", "model_migration", 9),
    ("jails", "canonical_support", 9),
    ("jails", "parse_error", 9),
    ("jails", "facade", 9),
    ("jails", "template_macro", 9),
    ("jails", "cli", 9),
    ("jails", "dispatch", 9),
    ("jails", "plan_command", 9),
    ("jails", "rename_source", 9),
    ("jails", "arguments", 9),
];

#[test]
fn canonical_compiler_is_pure_after_capture() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/jails-compiler/src");
    let banned = [
        "std::fs",
        "std::env",
        "std::process",
        "std::path",
        "PathBuf",
        "Command::new",
    ];
    let mut offenders = Vec::new();
    for file in sources().iter().filter(|file| file.path.starts_with(&root)) {
        for name in banned {
            if file.production.contains(name) {
                offenders.push(format!("  {}: {name}", file.path.display()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "the canonical compiler reached through its WorkspaceSnapshot boundary:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn canonical_workspace_has_one_mutation_owner() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/jails-workspace/src");
    let mut offenders = Vec::new();
    for file in sources().iter().filter(|file| {
        file.path.starts_with(&root)
            && file
                .path
                .file_name()
                .is_none_or(|name| name != "execute.rs")
    }) {
        let count = mutation_sites(std::slice::from_ref(file), MUTATION_APIS);
        if count != 0 {
            offenders.push(format!("  {}: {count} mutation calls", file.path.display()));
        }
    }
    assert!(
        offenders.is_empty(),
        "canonical workspace mutation escaped execute.rs:\n{}",
        offenders.join("\n")
    );
}

/// Every module that starts a process, and which row it is.
///
/// A subprocess can change a project as surely as a write can, and the
/// filesystem gate says nothing about it. `mvn` writes `target/`;
/// `docker compose up` starts a service; `git merge-file` produces bytes a
/// transaction commits. Each is fine -- *once somebody has said which it is*.
/// The test below fails when a module starts a process and is not named here.
const SUBPROCESS_CLASSIFICATION: &[(&str, &str)] = &[
    // Derived build processes. They may write `target/` and dependency
    // caches, which are excluded from the snapshot and the store, and they
    // run without the project lock after any required commit.
    ("run", "derived build process"),
    ("launcher", "derived build process"),
    ("why", "derived build process"),
    ("testd", "derived build process"),
    ("affected", "derived build process"),
    // `architecture baseline` runs the generated ArchUnit suite and nothing
    // else. What it produces is a store ArchUnit writes, which is the same
    // shape as a build output: jails asks for it, does not write it, and
    // holds no lock over it.
    ("baseline", "derived build process"),
    // External runtime effects. A commit records the desired project files
    // first; reconciling the runtime is an idempotent receipt effect.
    ("compose", "external runtime effect"),
    ("kafka", "external runtime effect"),
    ("migrate", "external runtime effect"),
    ("bench", "external runtime effect"),
    // Read-only clients and probes. Outside both locks, and they claim no
    // filesystem rollback.
    ("console", "read-only client"),
    ("contract_command", "read-only client"),
    ("tool_command", "read-only client"),
    ("doctor", "read-only probe"),
    // Asks `git merge-file` what it can do, on three throwaway files in a
    // scratch directory, and reads the exit status. It writes nothing the
    // project can see and holds no lock -- the merge it informs is the
    // `transaction input` row below.
    ("git", "read-only probe"),
    // Bootstrap, outside any project transaction: these run before a project
    // exists, inside a scratch tree that is published atomically.
    ("new", "new-project bootstrap"),
    // A three-way merge is transaction preparation, not a renderer and not an
    // effect: it runs `git merge-file` over three scratch inputs to compute
    // bytes the commit then guards like any other: git appears in the
    // preparation fingerprint and never in a renderer stamp.
    ("merge", "transaction input"),
    // The executor's own runner and tool resolver.
    ("process", "the one executor"),
    ("hermetic", "the one executor"),
];

/// Which module a file belongs to: `(crate, module)`.
///
/// `crates/jails-compiler/src/emit_java/facet.rs` ->
/// `("jails-compiler", "emit_java")`.
///
/// **The crate half is not decoration.** By basename alone,
/// `crates/a/src/spec.rs` and `crates/b/src/spec.rs` are one name to every
/// gate here, and the layering check silently measures one against the
/// other's level. Identifying a module by `(crate, module)` also lets a
/// production module be called what it is when another crate has a module of
/// that name.
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

/// No unclassified production mutation.
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
    for file in src {
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
         The rows are: derived build process, external runtime effect, \
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
        rederivers(src).into_iter().map(|(_, name)| name).collect();
    let declared: std::collections::BTreeSet<String> = A_FRESH_READ_IS_CORRECT
        .iter()
        .map(|(name, _)| (*name).to_string())
        .collect();

    assert_eq!(
        found, declared,
        "\n\nA root-taking function reads the pom for a fact a resolved `Project` holds.\n\
         Either pass it the `Project`, or, if reading again is \
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
/// `env::temp_dir().join(pid + timestamp)` followed by `create_dir_all` is not
/// exclusive in either half: two callers can read the same clock, and
/// `create_dir_all` treats "it already exists" as success, so one caller is
/// handed another's tree.
///
/// Test modules are exempt only because `Source::production` blanks them; a
/// production site has no exemption.
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

/// Every file that renders one Java shape for an old framework version and a
/// different one for the current default names the tier-3 test that executes
/// the branch it takes **on the default**, and that test exists.
///
/// A tier-3 test pinned to the old-version branch reports green for the
/// default branch it never touched: the same failure mode as a skipped tier-3
/// test, one level up, and neither is visible in the count.
///
/// The table is small on purpose and the scanner is what keeps it honest: a
/// new sniff site fails this until somebody names where its default branch is
/// executed. Naming a test that does not exist fails too, so the entry cannot
/// decay into a comment.
const DEFAULT_BRANCH_IS_EXECUTED: &[(&str, &str)] = &[
    (
        // `javax` below Boot 3 and `jakarta` at or above it. The default is
        // the Jakarta branch, and this compiles a generated request DTO with
        // its validation annotations against the real toolchain.
        "crates/jails-compiler/src/emit_dto.rs",
        "generate_scaffold_produces_a_project_that_compiles_and_passes_tests",
    ),
    (
        "crates/jails-compiler/src/emit_capability.rs",
        "canonical_observability_pack_merges_ejects_and_serves_prometheus",
    ),
    (
        "crates/jails-compiler/src/refuse.rs",
        "canonical_security_pack_merges_ejects_and_keeps_cors_buildable",
    ),
    // The controller's companion test picks `MockMvcTester` on Boot 4 and
    // `perform(...)` below it, and `@AutoConfigureMockMvc`'s package moved in
    // the same release. The named test drives a canonical controller through
    // real `mvn test` on the 4.1.0 fixture, so the default branch is the one
    // that compiles and runs.
    (
        "crates/jails-compiler/src/emit_unit.rs",
        "canonical_controller_merges_both_files_and_refuses_overlapping_route_edits",
    ),
    // Same test, for the other half of that rendering: Boot 4 split the
    // servlet test slice out of `spring-boot-starter-test`, so the controller
    // test needs `spring-boot-starter-webmvc-test` declared. The fixture
    // deliberately does not declare it -- see `SPRING_FIXTURE_POM` -- so a
    // missing dependency fails there rather than being supplied by the
    // fixture, which is the exact hole that note records.
    (
        "crates/jails-compiler/src/lib.rs",
        "canonical_controller_merges_both_files_and_refuses_overlapping_route_edits",
    ),
    // The one MockMvc dialect: `MockMvcTester` on Boot 4, `perform(...)`
    // below it, for every generated test that drives a route. The named test
    // drives a canonical `api` project -- a command, a query and a transition,
    // one of them scoped -- through real `mvn test` on the 4.1.0 fixture, so
    // the default branch is the one that compiles and answers a request.
    (
        "crates/jails-compiler/src/emit_mockmvc.rs",
        "scoped_execution_context_survives_evolution_and_binds_tenant_at_runtime",
    ),
    // The scaffold's controller test picks `MockMvcTester` on Boot 4 and
    // standalone `perform(...)` below it. The named test drives the generated
    // collection through real `mvn test` on the 4.1.0 fixture.
    // The DTO's request record picks `jakarta.validation` on Boot 3+ and
    // `javax.validation` below it, through `validation_package(boot_major(..))`.
    // The named test builds a canonical DTO on the Boot 4 fixture with real
    // Maven, so the default branch is the one that has to resolve.
    (
        "crates/jails-compiler/src/emit_dto.rs",
        "canonical_dto_evolves_three_merge_managed_abi_files_without_losing_reader_edits",
    ),
    (
        "crates/jails-compiler/src/emit_resource_http.rs",
        "canonical_scaffold_http_compiles_and_passes_on_real_maven",
    ),
    (
        "crates/jails-project/src/gradle.rs",
        "the_boot_version_is_read_from_the_modern_plugins_block",
    ),
    // The one reader of `pom.xml`: `spring_boot_major_of` answers 3 for a pom
    // with no readable Boot parent, and every package name that moved in Boot
    // 4 is chosen from it. The named test compiles a scaffold on the Boot 4
    // fixture with real Maven, so the default branch is the one that resolves.
    (
        "crates/jails-project/src/documents/pom.rs",
        "generate_scaffold_produces_a_project_that_compiles_and_passes_tests",
    ),
    (
        "src/new/gradle_project.rs",
        "new_gradle_at_a_current_boot_uses_the_plugins_block_and_a_readable_dependency_list",
    ),
    (
        "src/new/spring.rs",
        "a_freshly_generated_project_passes_check_with_no_manual_formatting",
    ),
];

#[test]
fn every_version_sniffed_rendering_names_where_its_default_branch_runs() {
    let found: std::collections::BTreeSet<String> = sources()
        .iter()
        .filter(|file| file.production.contains("boot_major"))
        .map(|file| {
            file.path
                .strip_prefix(std::path::Path::new(env!("CARGO_MANIFEST_DIR")))
                .unwrap_or(&file.path)
                .display()
                .to_string()
        })
        .collect();
    let declared: std::collections::BTreeSet<String> = DEFAULT_BRANCH_IS_EXECUTED
        .iter()
        .map(|(path, _)| (*path).to_string())
        .collect();
    assert_eq!(
        found, declared,
        "\n\nA file renders a different Java shape per framework version. Name the \
         real-toolchain test that runs the branch it takes on the current default in \
         DEFAULT_BRANCH_IS_EXECUTED, or -- if the sniff is gone -- take the entry out."
    );

    let harness = harness_text();
    for (path, test) in DEFAULT_BRANCH_IS_EXECUTED {
        assert!(
            harness.contains(&format!("fn {test}(")),
            "{path} names `{test}`, and no test by that name exists. An entry that \
             points at nothing is worse than none: it reads as coverage."
        );
    }
}

/// Every Rust file under `tests/` and every workspace source, concatenated.
///
/// Both, because a covering test is not always an integration test: the two
/// files that only *read* a version are pinned by colocated unit tests, and a
/// gate that could not see those would push their entries towards naming an
/// integration test that does not exercise them.
fn harness_text() -> String {
    fn walk(dir: &std::path::Path, out: &mut String) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut paths: Vec<std::path::PathBuf> =
            entries.flatten().map(|entry| entry.path()).collect();
        paths.sort();
        for path in paths {
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|extension| extension == "rs")
                && let Ok(text) = std::fs::read_to_string(&path)
            {
                out.push_str(&text);
            }
        }
    }
    let mut out = String::new();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    walk(&root.join("tests"), &mut out);
    walk(&root.join("src"), &mut out);
    walk(&root.join("crates"), &mut out);
    assert!(
        out.len() > 100_000,
        "the harness scanner read only {} bytes -- it has lost track of where the \
         tests live",
        out.len()
    );
    out
}

/// **Every command path reaches at least one journey.**
///
/// `cli::feature_inventory_covers_the_live_clap_tree_exactly_once` pins the
/// inventory against the live `clap::Command`, so the *list* cannot drift;
/// this holds the other half, so a command cannot be inventoried, advertised
/// in `jails commands`, and invoked by no test at all.
///
/// **A floor rather than a hard requirement**, because some command paths
/// genuinely have no test here and pretending otherwise would mean either a
/// permanently red build or a fake test. They are named below with the reason,
/// so the gate fails in both directions that matter: coverage may not fall,
/// and a *new* uncovered command is a failure rather than a silent addition
/// to the list.
#[test]
fn every_inventoried_command_path_is_invoked_by_a_test() {
    /// Command paths with no journey, and why. Each is an operational command
    /// that drives something this suite has no way to stand up: `kafka *`
    /// runs the broker image's own CLI *inside* a compose container, and
    /// `test daemon *` talks to a resident JVM over a unix socket. Testing
    /// them means starting the real thing, which is a tier-3 fixture nobody
    /// has written rather than an oversight.
    const UNJOURNEYED: &[&str] = &[
        "kafka topics",
        "kafka describe",
        "kafka send",
        "kafka poison",
        "kafka tail",
        "kafka dlt",
        "kafka lag",
        "kafka reset",
        "test daemon restart",
        "test daemon status",
    ];

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let inventory = std::fs::read_to_string(root.join("docs/feature-inventory.tsv"))
        .expect("the feature inventory is checked in");
    let commands: Vec<&str> = inventory
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split('\t').next())
        .collect();
    assert!(
        commands.len() > 90,
        "the inventory reader found only {} command paths -- it has lost the \
         file and this gate would pass over anything",
        commands.len()
    );

    let mut corpus = String::new();
    collect_rust_sources(&root.join("tests"), &mut corpus);
    assert!(
        corpus.len() > 500_000,
        "the test-source scan read only {} bytes -- it has lost the suite",
        corpus.len()
    );

    let mut uncovered = Vec::new();
    for command in &commands {
        if !is_invoked(&corpus, command) {
            uncovered.push(*command);
        }
    }

    let unexpected: Vec<&&str> = uncovered
        .iter()
        .filter(|command| !UNJOURNEYED.contains(command))
        .collect();
    assert!(
        unexpected.is_empty(),
        "these command paths are advertised and invoked by no test:\n{}\n\n\
         Add a journey, or -- if it genuinely cannot be driven here -- name it \
         in `UNJOURNEYED` with the reason. G2 wants every live command path \
         mapped to at least one checked-in journey.",
        unexpected
            .iter()
            .map(|command| format!("  {command}"))
            .collect::<Vec<_>>()
            .join("\n")
    );

    let recovered: Vec<&&str> = UNJOURNEYED
        .iter()
        .filter(|command| !uncovered.contains(command))
        .collect();
    assert!(
        recovered.is_empty(),
        "these command paths now have a journey and should come out of \
         `UNJOURNEYED`:\n{}\n\n\
         An exemption that is no longer needed is permission for nothing, and \
         leaving it means the next command that loses its journey is hidden \
         behind it.",
        recovered
            .iter()
            .map(|command| format!("  {command}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Whether the test corpus invokes one command path.
///
/// A multi-word path is matched as the argument *sequence* it is typed as,
/// which is exact. A single word is matched only in an argument position --
/// `.arg("sync")`, `["sync"`, `"sync",` -- because a bare `"sync"` anywhere
/// in half a megabyte of test source would match prose in an assertion
/// message and count a command nothing runs.
fn is_invoked(corpus: &str, command: &str) -> bool {
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.len() > 1 {
        return corpus
            .match_indices(&format!("\"{}\"", parts[0]))
            .any(|(at, _)| {
                let mut rest = &corpus[at + parts[0].len() + 2..];
                parts[1..].iter().all(|part| {
                    let trimmed = rest.trim_start();
                    let Some(trimmed) = trimmed.strip_prefix(',') else {
                        return false;
                    };
                    let trimmed = trimmed.trim_start();
                    match trimmed.strip_prefix(&format!("\"{part}\"")) {
                        Some(remainder) => {
                            rest = remainder;
                            true
                        }
                        None => false,
                    }
                })
            });
    }
    let quoted = format!("\"{command}\"");
    corpus.match_indices(&quoted).any(|(at, _)| {
        let before = corpus[..at].trim_end();
        let after = corpus[at + quoted.len()..].trim_start();
        before.ends_with(".arg(")
            || before.ends_with('[')
            || before.ends_with(',')
            || after.starts_with(',')
            || after.starts_with(']')
    })
}

/// Every `.rs` file under `dir`, concatenated. Raw text, not blanked: the
/// callers look for names that live inside `#[cfg(test)]` bodies, which
/// [`measure::sources`] erases.
fn collect_rust_sources(dir: &Path, out: &mut String) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rust_sources(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "rs")
            && let Ok(text) = std::fs::read_to_string(&path)
        {
            out.push_str(&text);
            out.push('\n');
        }
    }
}

/// Every file that runs this project's automation: scripts, hooks, workflows.
///
/// One scan, because the three rot the same way. Each names Rust targets,
/// other scripts and `mise` tasks in plain text, and a rename carries to none
/// of them.
fn automation_files() -> Vec<(std::path::PathBuf, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut found = Vec::new();
    for directory in ["scripts", ".githooks", ".github/workflows"] {
        let Ok(entries) = std::fs::read_dir(root.join(directory)) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            found.push((path, text));
        }
    }
    // A scanner that has lost the tree reports exactly what a clean one does.
    assert!(
        found.len() > 4,
        "the automation scan found only {} files -- it has stopped reading them",
        found.len()
    );
    found
}

/// Every `cargo test --test <target>` this project's automation runs is a real
/// target.
///
/// A shell script naming a Rust target is exactly the kind of edge `cargo`
/// cannot check and a rename does not carry: a script that exits non-zero on
/// a command nobody runs is indistinguishable from one that passes. The
/// workflows are read for the same reason and it is a sharper one: a
/// scheduled job is read by nobody until it has already not run.
#[test]
fn every_test_target_a_script_names_exists() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let targets: std::collections::BTreeSet<String> = std::fs::read_dir(root.join("tests"))
        .expect("tests/ exists")
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_str()?.to_string();
            // `tests/<name>.rs` and `tests/<name>/main.rs` are both one target.
            if path.is_dir() && path.join("main.rs").is_file() {
                Some(name)
            } else {
                name.strip_suffix(".rs").map(str::to_string)
            }
        })
        .collect();
    assert!(
        targets.len() > 5,
        "the target scan found only {targets:?} -- it has stopped reading tests/"
    );

    let mut missing = Vec::new();
    for (path, text) in automation_files() {
        for (at, _) in text.match_indices("--test ") {
            let named: String = text[at + "--test ".len()..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
                .collect();
            if !named.is_empty() && !targets.contains(&named) {
                missing.push(format!("{}: --test {named}", path.display()));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "these run a cargo test target that does not exist:\n  {}\n\n\
         Rename the reference with the harness, or the script silently stops \
         testing anything.",
        missing.join("\n  ")
    );
}

/// Every script and `mise` task this project's automation names exists.
///
/// One level out from the target check above: `.githooks/pre-push` and
/// both workflows are three files that reach the suite by *name* rather than
/// by a path `cargo` resolves. A renamed script or a renamed task fails them
/// at the moment they run, which for a weekly scheduled job is a week later
/// and for a hook is on somebody else's push.
///
/// Only references this repository owns are checked. A `mise` task is matched
/// against `mise.toml`'s own `[tasks.<name>]` headers, so the two cannot
/// disagree about which tasks exist.
#[test]
fn every_script_and_task_the_automation_names_exists() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = std::fs::read_to_string(root.join("mise.toml")).expect("mise.toml exists");
    let tasks: std::collections::BTreeSet<&str> = manifest
        .lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("[tasks.")
                .and_then(|rest| rest.strip_suffix(']'))
        })
        .collect();
    assert!(
        tasks.contains("verify-rewrite"),
        "the task scan found {tasks:?} -- it has stopped reading mise.toml"
    );

    let mut missing = Vec::new();
    for (path, text) in automation_files() {
        for (at, _) in text.match_indices("mise run ") {
            let named: String = text[at + "mise run ".len()..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
                .collect();
            if !named.is_empty() && !tasks.contains(named.as_str()) {
                missing.push(format!("{}: mise run {named}", path.display()));
            }
        }
        for (at, _) in text.match_indices("scripts/") {
            let named: String = text[at + "scripts/".len()..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
                .collect();
            // A bare `scripts/` in prose names no file; only a reference that
            // looks like one is a reference.
            if named.contains('.') && !root.join("scripts").join(&named).is_file() {
                missing.push(format!("{}: scripts/{named}", path.display()));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "these name a script or a mise task that does not exist:\n  {}\n\n\
         A hook or a scheduled workflow reaches the suite by name, so a rename \
         that misses one is only reported the next time it runs.",
        missing.join("\n  ")
    );
}

/// Every markdown file under `docs/` plus the three root documents, with
/// fenced code blocks removed.
///
/// The fences are dropped because every rule below reads a backticked token as
/// a name somebody is citing, and a fenced block is a command line rather than
/// a citation -- `git show <commit>^:deleted.md` names a file that is supposed
/// to be gone.
fn document_prose() -> Vec<(std::path::PathBuf, String)> {
    fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "md") {
                out.push(path);
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut paths = Vec::new();
    walk(&root.join("docs"), &mut paths);
    for name in ["CLAUDE.md", "ARCHITECTURE.md", "README.md"] {
        paths.push(root.join(name));
    }
    paths.sort();
    // A scanner that has lost the tree reports exactly what a clean one does.
    assert!(
        paths.len() >= 6,
        "the document scan found only {} files -- it has stopped reading them",
        paths.len()
    );
    paths
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let mut kept = String::with_capacity(text.len());
            let mut fenced = false;
            for line in text.lines() {
                if line.trim_start().starts_with("```") {
                    fenced = !fenced;
                    kept.push('\n');
                    continue;
                }
                // Blank the fenced line rather than dropping it, so a reported
                // line number still indexes the file on disk.
                if !fenced {
                    kept.push_str(line);
                }
                kept.push('\n');
            }
            (path, kept)
        })
        .collect()
}

/// Whether each 1-indexed line sits in a paragraph that cites a commit.
///
/// A document has to be able to *name* something that is gone -- the deletion
/// map is most of `docs/00-contracts.md`, and workstream C cannot record that
/// a crate was deleted without writing its name. The convention these
/// documents already use for that is the one `git log --diff-filter=D` needs:
/// give the commit. So a paragraph carrying a commit hash is read as history,
/// and the two name rules below do not fire inside it. A paragraph, not a
/// line, because this prose wraps at 78 columns and the name and the hash
/// routinely land on different ones.
fn lines_in_a_paragraph_citing_a_commit(text: &str) -> Vec<bool> {
    fn is_commit(word: &str) -> bool {
        let trimmed = word.trim_matches(|c: char| !c.is_ascii_alphanumeric());
        (7..=40).contains(&trimmed.len())
            && trimmed.chars().all(|c| c.is_ascii_hexdigit())
            && trimmed.chars().any(|c| c.is_ascii_digit())
            && trimmed.chars().any(|c| c.is_ascii_alphabetic())
    }
    let lines: Vec<&str> = text.lines().collect();
    let mut flags = vec![false; lines.len() + 2];
    let mut start = 0;
    while start < lines.len() {
        if lines[start].trim().is_empty() {
            start += 1;
            continue;
        }
        let mut end = start;
        while end < lines.len() && !lines[end].trim().is_empty() {
            end += 1;
        }
        let historical = lines[start..end]
            .iter()
            .any(|line| line.split_whitespace().any(is_commit));
        if historical {
            for flag in flags.iter_mut().take(end + 1).skip(start + 1) {
                *flag = true;
            }
        }
        start = end;
    }
    flags
}

/// The backticked tokens of one document, with the line each was found on.
fn backticked(text: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    let bytes = text.as_bytes();
    let mut line = 1;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\n' {
            line += 1;
            i += 1;
            continue;
        }
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        let start = i + 1;
        let Some(end) = text[start..].find('`').map(|o| start + o) else {
            break;
        };
        let token = &text[start..end];
        if !token.contains('\n') {
            found.push((line, token.to_string()));
        }
        line += token.matches('\n').count();
        i = end + 1;
    }
    found
}

/// Every file, part, crate and test name the documents cite is one that exists.
///
/// `docs/00-contracts.md` is the file every workstream reads first, so a
/// reference in it that names nothing sends whoever followed it looking for a
/// section that is not there. A reference nothing checks is a reference that
/// rots, and checking it costs one scan of a handful of files.
///
/// Four rules, each over `docs/**/*.md` with fenced blocks removed:
///
/// - a `docs/<name>.md` path is a file that exists;
/// - a cited `Part <n>` has a `# Part <n>` heading somewhere in the set;
/// - a backticked `jails-<crate>` or `jails_<crate>` names a crate that
///   exists;
/// - a backticked snake_case identifier carrying three or more underscores is
///   a `fn` in the tree. That is how these documents write a test they claim
///   holds a rule -- `rules::canonical_compiler_is_pure_after_capture` and
///   its siblings.
///
/// The last two rules skip any paragraph that cites a commit hash, because a
/// document must be able to name what was deleted -- see
/// [`lines_in_a_paragraph_citing_a_commit`]. Give the commit and the name
/// stands; give neither and the reference has to resolve.
///
/// The last rule reads any token of that shape as a Rust name, so a document
/// wanting to backtick a Java or SQL identifier carrying three underscores
/// should spell it without the backticks. Nothing under `docs/` does.
///
/// `CLAUDE.md`, `ARCHITECTURE.md` and `README.md` are scanned with `docs/`.
#[test]
fn every_cross_reference_in_the_documents_resolves() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let documents = document_prose();

    let crates: std::collections::BTreeSet<String> = std::fs::read_dir(root.join("crates"))
        .expect("failed to read crates/")
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(crates.len() > 10, "the crate scan found {}", crates.len());

    let mut rust = String::new();
    for directory in ["crates", "src", "tests"] {
        collect_rust_sources(&root.join(directory), &mut rust);
    }
    assert!(
        rust.len() > 1_000_000,
        "the Rust scan read only {} bytes -- it has lost the tree",
        rust.len()
    );

    let parts: std::collections::BTreeSet<String> = documents
        .iter()
        .flat_map(|(_, text)| text.lines())
        .filter_map(|line| {
            let heading = line.trim_start().strip_prefix('#')?;
            let title = heading.trim_start_matches('#').trim();
            let number = title.strip_prefix("Part ")?;
            Some(
                number
                    .split_whitespace()
                    .next()?
                    .trim_end_matches(|c: char| !c.is_ascii_digit())
                    .to_string(),
            )
        })
        .filter(|number| !number.is_empty())
        .collect();
    assert!(
        !parts.is_empty(),
        "no `# Part <n>` heading was found at all"
    );

    let mut dangling = Vec::new();
    for (path, text) in &documents {
        let name = path
            .strip_prefix(root)
            .unwrap_or(path)
            .display()
            .to_string();

        for (offset, _) in text.match_indices("docs/") {
            let rest = &text[offset + "docs/".len()..];
            let end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || "._/-".contains(c)))
                .unwrap_or(rest.len());
            // Trailing full stops belong to the sentence, not the path: a
            // reference ending one reads as `…/name.md.` and would be skipped
            // by the `.md` check below, missing exactly the citations that end
            // a sentence.
            let cited = rest[..end].trim_end_matches('.');
            if cited.ends_with(".md") && !root.join("docs").join(cited).is_file() {
                dangling.push(format!(
                    "{name}:{} names docs/{cited}, which is not a file",
                    text[..offset].matches('\n').count() + 1
                ));
            }
        }

        for (offset, _) in text.match_indices("Part ") {
            let rest = &text[offset + "Part ".len()..];
            let end = rest
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(rest.len());
            if end == 0 {
                continue;
            }
            let cited = &rest[..end];
            if !parts.contains(cited) {
                dangling.push(format!(
                    "{name}:{} cites Part {cited}, which has no heading in docs/",
                    text[..offset].matches('\n').count() + 1
                ));
            }
        }

        let historical = lines_in_a_paragraph_citing_a_commit(text);
        for (line, token) in backticked(text) {
            if historical.get(line).copied().unwrap_or(false) {
                continue;
            }
            let head: String = token
                .split([':', '/'])
                .next()
                .unwrap_or_default()
                .replace('_', "-");
            if head.starts_with("jails-")
                && head.len() > "jails-".len()
                && head.chars().all(|c| c.is_ascii_lowercase() || c == '-')
                && !crates.contains(&head)
            {
                dangling.push(format!(
                    "{name}:{line} names the crate `{head}`, which does not exist"
                ));
            }

            let last = token.rsplit("::").next().unwrap_or_default();
            let looks_like_a_test = last.starts_with(|c: char| c.is_ascii_lowercase())
                && last
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
                && last.matches('_').count() >= 3;
            if looks_like_a_test && !rust.contains(&format!("fn {last}")) {
                dangling.push(format!(
                    "{name}:{line} names `{last}`, and no `fn {last}` exists"
                ));
            }
        }
    }

    assert!(
        dangling.is_empty(),
        "{} document reference(s) name something that does not exist. A reference \
         nothing checks is one that rots, which is why this is a gate rather than a \
         proofread:\n  {}",
        dangling.len(),
        dangling.join("\n  ")
    );
}

/// Every diagnostic code belongs to the crate that owns its phase.
///
/// **JDL v1 §18.3 asks for one diagnostic contract, and the way it is lost is
/// a third vocabulary**: a `model-*` code appearing in an emitter because that
/// prefix is the one already in the tree to copy. Then two crates own one
/// namespace and nothing says which pass a code came from.
///
/// A code says which pass refused, so the prefix is owned by the crate that
/// owns the pass: `JDL####` and `model-*` are `jails-model`'s, `compile-*` is
/// `jails-compiler`'s, `plan-*` is `jails-workspace`'s. The rule is checked
/// over *string literals* -- a code only ever appears inside one, and blanked
/// source would report zero however wrong the tree was.
#[test]
fn every_diagnostic_code_belongs_to_the_crate_that_owns_its_phase() {
    const OWNERS: &[(&str, &str)] = &[
        ("JDL", "jails-model"),
        ("model-", "jails-model"),
        ("compile-", "jails-compiler"),
        ("workspace-", "jails-workspace"),
    ];
    // The root binary reports on the model in the linker's own vocabulary --
    // `model-io` when it cannot read the file, `model-generated-drift` when
    // the committed tree disagrees with this compilation -- and both are in
    // the JSON a reader parses. What this gate defends against is an
    // *emitter* copying `model-` because it is the prefix already there.
    const ALSO_OWNS_MODEL: &str = "/src/model_command.rs";
    let code = regex_lite_codes;
    let mut offenders = Vec::new();
    let mut seen = 0_usize;
    for file in sources() {
        let path = file.path.to_string_lossy().into_owned();
        // The gate itself names every prefix, and `diagnostic.rs` documents
        // the table; neither is a code site.
        if path.ends_with("tests/architecture/rules.rs")
            || path.ends_with("jails-model/src/diagnostic.rs")
        {
            continue;
        }
        for literal in code(&file.literals) {
            let Some((prefix, owner)) = OWNERS
                .iter()
                .find(|(prefix, _)| literal.starts_with(prefix))
                .copied()
            else {
                continue;
            };
            seen += 1;
            if prefix == "model-" && path.ends_with(ALSO_OWNS_MODEL) {
                continue;
            }
            if !path.contains(&format!("crates/{owner}/")) {
                offenders.push(format!(
                    "  {path}: `{literal}` is {owner}'s `{prefix}` namespace"
                ));
            }
        }
    }
    assert!(
        seen > 100,
        "the scanner found only {seen} diagnostic codes -- it has stopped reading them, and \
         would report the same clean result over a tree that had gone wrong"
    );
    assert!(
        offenders.is_empty(),
        "a diagnostic code escaped the crate that owns its phase:\n{}",
        offenders.join("\n")
    );
}

/// Every string literal shaped like a diagnostic code.
///
/// Deliberately shape-based rather than call-site based: a code reaches
/// `Diagnostic::new` through `linker.problem`, `problem`, `here` and a handful
/// of wrappers, and a gate that enumerated those would go quietly blind the
/// first time somebody added a sixth.
fn regex_lite_codes(literals: &str) -> Vec<String> {
    let mut found = Vec::new();
    let bytes = literals.as_bytes();
    let mut at = 0;
    while let Some(open) = literals[at..].find('"') {
        let start = at + open + 1;
        let Some(len) = literals[start..].find('"') else {
            break;
        };
        let value = &literals[start..start + len];
        at = start + len + 1;
        let _ = bytes;
        let looks_like_a_code = (value.starts_with("JDL")
            && value.len() == 7
            && value[3..].bytes().all(|byte| byte.is_ascii_digit()))
            || (value.len() > 6
                && value.contains('-')
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte == b'-')
                && ["model-", "compile-", "workspace-"]
                    .iter()
                    .any(|prefix| value.starts_with(prefix)));
        if looks_like_a_code {
            found.push(value.to_string());
        }
    }
    found
}
