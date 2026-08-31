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
        || relative == "src/contract_command.rs"
        || relative == "src/tool_command.rs"
        || relative == "src/model_command.rs"
        || relative == "src/model_generate.rs"
        // The other half of the same door. `model_import` carries a legacy
        // ledger across; this one gives a project jails never created its
        // first model, and saying so is part of the command: a reader who has
        // just made their repository canonical needs to know that generation
        // has moved to `.jails/generated` and their own sources have not.
        || relative == "src/model_init.rs"
        // Sibling of `model_import.rs` and classified for the same reason: a
        // CLI command module whose contract includes telling the reader what
        // the upgrade changes about the model before the plan is shown. §22
        // requires that review step, and two of the translations mean
        // something a reviewer should not have to spot in the diff.
        || relative == "src/model_upgrade.rs"
        // A read-only report whose entire contract is terminal output:
        // `jdl-sol.md` §18.4 asks that a derived name be *inspectable*, and a
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
        || relative == "crates/jails-java/src/template.rs"
        || relative == "crates/jails-project/src/compose.rs"
        || relative == "crates/jails-project/src/inspect.rs"
        || relative == "crates/jails-project/src/project.rs"
        || relative == "crates/jails-generate/src/generate.rs"
        || relative == "crates/jails-generate/src/generate/recipes.rs"
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
             rung {}\n  Lower this row's `ceiling` to {actual} in tests/architecture/board.rs. An \
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
/// Every gate that names a file must name one that is there.
///
/// A gate keyed by path has two failure modes, and only one of them is loud.
/// Pointing at the *wrong* file drags rows red, which `SPRING_RS`'s comment
/// records. Pointing at a file that no longer exists is silent: the exclusion
/// excludes nothing, or the measurement measures nothing, and the row keeps
/// printing a number nobody can tell from a real one.
///
/// `CODEMOD_RS` spent a whole change stale -- it still said
/// `jails-project/src/codemod.rs` after the splice moved to its own crate --
/// and nothing failed. It was harmless there by luck: the owner's own markers
/// are all in comments, so excluding nothing excluded nothing that counted.
#[test]
fn every_path_a_gate_names_is_a_file_the_scanner_found() {
    let files = sources();
    for (constant, path) in [
        ("BUILTIN_RS", BUILTIN_RS),
        ("WIRE_RS", WIRE_RS),
        ("SPRING_RS", SPRING_RS),
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
            // second half alone recreates the collision this table was changed
            // to eliminate.
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

/// Which crate each module ships in, lowest first. Legacy and canonical
/// compiler modules coexist during cutover; every module stays classified so
/// deleting the legacy half cannot silently loosen a boundary.
const LAYERS: &[(&str, &str, usize)] = &[
    ("jails-model", "app", 2),
    ("jails-model", "builtin", 2),
    ("jails-model", "capability", 2),
    ("jails-model", "component", 2),
    ("jails-model", "constraint", 2),
    ("jails-model", "layout", 2),
    // jails-support: no jails concepts at all -- writing, running, encoding.
    // `jails-codemod` depends on nothing at all -- it knows one text format
    // and no more -- so it sits beside the support primitives rather than in
    // the project layer it came from. It moved out of `jails-project` because
    // three more implementations of the marked block had appeared in crates
    // that could not depend on it.
    ("jails-codemod", "annotate", 0),
    ("jails-codemod", "dispatch", 0),
    ("jails-codemod", "marked", 0),
    ("jails-codemod", "text", 0),
    ("jails-codemod", "tidy", 0),
    ("jails-support", "apply", 0),
    ("jails-support", "process", 0),
    ("jails-support", "hermetic", 0),
    ("jails-support", "scratch", 0),
    ("jails-support", "codec", 0),
    ("jails-support", "git", 0),
    ("jails-support", "identifier", 0),
    ("jails-support", "identity", 0),
    ("jails-support", "json", 0),
    ("jails-support", "lock", 0),
    // jails-java: reading Java and rendering templates into it.
    ("jails-java", "java", 1),
    ("jails-java", "classfile", 1),
    ("jails-java", "template", 1),
    // jails-spec: what a jails project is -- where it is, how it is laid out,
    // what a field means, and the closed CLI vocabularies.
    ("jails-spec", "build", 2),
    ("jails-spec", "spec", 2),
    // Canonical semantic model: closed source schema -> linked stable IDs.
    ("jails-model", "diagnostic", 2),
    ("jails-model", "dependency", 2),
    ("jails-model", "derived", 2),
    ("jails-model", "ejection", 2),
    ("jails-model", "enum_constant", 2),
    ("jails-model", "facet", 2),
    ("jails-model", "id", 2),
    ("jails-model", "index", 2),
    ("jails-model", "jdl", 2),
    ("jails-model", "patch", 2),
    ("jails-model", "projection", 2),
    ("jails-model", "relation", 2),
    ("jails-model", "linker", 2),
    ("jails-model", "model", 2),
    ("jails-model", "model_apply", 2),
    ("jails-model", "naming", 2),
    ("jails-model", "operation", 2),
    ("jails-model", "source", 2),
    ("jails-model", "setting", 2),
    ("jails-model", "syntax_edit", 2),
    ("jails-model", "unit", 2),
    // Portable values shared by the pure compiler and filesystem boundary.
    ("jails-contracts", "draft", 3),
    ("jails-contracts", "path", 3),
    ("jails-contracts", "plan", 3),
    ("jails-contracts", "snapshot", 3),
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
    ("jails-compiler", "emit_operation", 4),
    ("jails-compiler", "emit_resource_http", 4),
    ("jails-compiler", "emit", 4),
    ("jails-compiler", "emit_seed", 4),
    ("jails-compiler", "emit_sql", 4),
    ("jails-compiler", "refuse", 4),
    ("jails-compiler", "storage", 4),
    ("jails-compiler", "emit_unit", 4),
    // The only canonical filesystem capture/materialization/execution owner.
    ("jails-workspace", "capture", 5),
    ("jails-workspace", "documents", 5),
    ("jails-workspace", "execute", 5),
    ("jails-workspace", "fault", 5),
    ("jails-workspace", "materialize", 5),
    ("jails-workspace", "merge", 5),
    ("jails-workspace", "reader_facet", 5),
    ("jails-workspace", "reconcile", 5),
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
    ("jails-project", "compose", 5),
    ("jails-project", "named_query", 5),
    ("jails-project", "model", 5),
    ("jails-project", "modernize", 5),
    ("jails-project", "project", 5),
    ("jails-project", "projection", 5),
    ("jails-project", "properties", 5),
    ("jails-project", "query_compiler", 5),
    ("jails-project", "schema", 5),
    ("jails-project", "generated_files", 5),
    ("jails-project", "inspect", 5),
    // jails-generate: everything that decides what Java to write.
    ("jails-generate", "architecture", 6),
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
    ("jails-prepare", "prepared_after", 6),
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
    // jails-report: commands that answer a question. Read-only by contract,
    // and below `jails-drive` so the contract is structural.
    ("jails-report", "doctor", 7),
    ("jails-report", "schema_lineage", 7),
    ("jails-report", "diagnostic", 7),
    ("jails-report", "why", 7),
    ("jails-report", "why_subject", 7),
    ("jails-report", "explain", 7),
    ("jails-report", "commands", 7),
    ("jails-report", "source", 7),
    ("jails-report", "lifecycle_status", 7),
    ("jails-report", "managed_drift", 7),
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
    ("jails-drive", "datasource", 8),
    ("jails-drive", "live_sql", 8),
    ("jails-drive", "testing", 8),
    // jails-cli: the binary and the whole-project lifecycle commands.
    ("jails", "new", 9),
    ("jails", "adopt", 9),
    ("jails", "modernize", 9),
    ("jails", "app", 9),
    ("jails", "sql_command", 9),
    ("jails", "schema_command", 9),
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
    ("jails", "model_field_parse", 9),
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
    ("jails", "model_upgrade", 9),
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
    ("live_sql", "read-only probe"),
    ("doctor", "read-only probe"),
    // Asks `git merge-file` what it can do, on three throwaway files in a
    // scratch directory, and reads the exit status. It writes nothing the
    // project can see and holds no lock -- the merge it informs is the
    // `transaction input` row below.
    ("git", "read-only probe"),
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
        rederivers(src).into_iter().map(|(_, name)| name).collect();
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

/// Every file that renders one Java shape for an old framework version and a
/// different one for the current default names the tier-3 test that executes
/// the branch it takes **on the default**, and that test exists.
///
/// modern.md §11.6, generalised. `add cors` *is* run through real `mvn test`
/// -- against a Boot 2 fixture, so it renders its *classic* `MockMvc` variant
/// and the assertion proves the modern one was not chosen. The Boot 4 branch,
/// which is what every real project gets, had never been compiled, let alone
/// run. A tier-3 test pinned to the legacy branch reports green for a branch
/// it never touched: the same failure mode as a skipped tier-3 test, one level
/// up, and neither is visible in the count.
///
/// The table is small on purpose and the scanner is what keeps it honest: a
/// new sniff site fails this until somebody names where its default branch is
/// executed. Naming a test that does not exist fails too, so the entry cannot
/// decay into a comment.
const DEFAULT_BRANCH_IS_EXECUTED: &[(&str, &str)] = &[
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
    // The operation controller's companion test, which renders `MockMvcTester`
    // on Boot 4 and standalone `MockMvcBuilders` below it. The named test
    // drives a canonical `api` project -- a command, a query and a transition,
    // one of them scoped -- through real `mvn test` on the 4.1.0 fixture, so
    // the default branch is the one that compiles and answers a request.
    (
        "crates/jails-compiler/src/emit_http/proof.rs",
        "scoped_execution_context_survives_evolution_and_binds_tenant_at_runtime",
    ),
    // The scaffold's controller test picks `MockMvcTester` on Boot 4 and
    // standalone `perform(...)` below it. The named test drives the generated
    // collection through real `mvn test` on the 4.1.0 fixture.
    (
        "crates/jails-compiler/src/emit_resource_http.rs",
        "canonical_scaffold_http_compiles_and_passes_on_real_maven",
    ),
    (
        "crates/jails-generate/src/generate/recipes.rs",
        "generate_scaffold_produces_a_project_that_compiles_and_passes_tests",
    ),
    (
        "crates/jails-generate/src/generate/web.rs",
        "generate_scaffold_produces_a_project_that_compiles_and_passes_tests",
    ),
    (
        "crates/jails-generate/src/spring.rs",
        "standalone_generators_companion_tests_compile_and_pass",
    ),
    // Both capabilities are installed into the same Boot 4 toolbox that test
    // asserts against, and `add h2`'s default branch is a console module that
    // exists only on Boot 4.
    (
        "crates/jails-generate/src/spring/h2.rs",
        "add_cors_on_the_default_boot_version_compiles_and_runs_its_own_test",
    ),
    (
        "crates/jails-generate/src/spring/security.rs",
        "add_cors_on_the_default_boot_version_compiles_and_runs_its_own_test",
    ),
    (
        "crates/jails-project/src/gradle.rs",
        "the_boot_version_is_read_from_the_modern_plugins_block",
    ),
    (
        "crates/jails-project/src/model/mod.rs",
        "generate_scaffold_produces_a_project_that_compiles_and_passes_tests",
    ),
    (
        "crates/jails-project/src/pom.rs",
        "generate_scaffold_produces_a_project_that_compiles_and_passes_tests",
    ),
    (
        "crates/jails-project/src/project.rs",
        "the_boot_version_is_read_from_the_modern_plugins_block",
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

/// The two layer lists are one list, in two crates that cannot see each other.
///
/// `jails-model`'s `Layer::ALL` is what the compiler will rename;
/// `jails-spec`'s `Layer::ALL` is what the legacy engine renames and what
/// `jails.toml`'s parser accepts. A layer in one and not the other is a rename
/// that half of jails honours -- which is `bugs.md` B59 in the other
/// direction, and the reason that entry exists at all.
///
/// They are written out separately because `jails-model` sits below
/// `jails-spec` and may not depend on it. This test is where they meet.
#[test]
fn the_compilers_renameable_layers_are_the_engines_layers() {
    let engine: Vec<&str> = jails_spec::spec::layout::Layer::ALL
        .iter()
        .map(|layer| layer.package())
        .collect();
    let compiler: Vec<&str> = jails_model::Layer::ALL
        .iter()
        .map(|layer| layer.package())
        .collect();
    assert_eq!(
        compiler, engine,
        "the compiler and the engine disagree about which layers a project may rename"
    );
}

/// **G2's other half: every command path reaches at least one journey.**
///
/// `simplify-sol.md`'s G2 asks that "all 100 live command paths map to at
/// least one checked-in journey". Half of that was already held --
/// `cli::feature_inventory_covers_the_live_clap_tree_exactly_once` pins the
/// inventory against the live `clap::Command`, so the *list* cannot drift.
/// Nothing checked the other half, so a command could be inventoried,
/// advertised in `jails commands`, and invoked by no test at all.
///
/// **A floor rather than a hard requirement**, because ten command paths
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
        commands.len() > 100,
        "the inventory reader found only {} command paths -- it has lost the \
         file and this gate would pass over anything",
        commands.len()
    );

    let mut corpus = String::new();
    collect_test_sources(&root.join("tests"), &mut corpus);
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

fn collect_test_sources(dir: &Path, out: &mut String) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_test_sources(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "rs")
            && let Ok(text) = std::fs::read_to_string(&path)
        {
            out.push_str(&text);
            out.push('\n');
        }
    }
}

/// The two pluralizers agree, word for word.
///
/// **There are two, and there has to be until the cutover.** `jdl-sol.md`
/// §9.7 specifies one table-naming rule; the legacy ladder implements it in
/// `jails-protocol::SqlName::conventional_table` and the canonical one in
/// `jails_model::plural_snake_case`, and the two ladders cannot depend on each
/// other. `CLAUDE.md`'s rule about a second pluraliser is exactly right about
/// what happens when they drift -- a route served `/categorys` over a table
/// called `categories`, from two functions forty lines apart -- so what
/// replaces "one owner" here is this: one *rule*, two implementations, and a
/// gate that fails the moment they answer differently.
///
/// It matters more than a style disagreement would. `jails model import`
/// carries a legacy project onto the canonical path, and a canonical
/// pluralizer that said `task` where the legacy one said `tasks` pointed every
/// generated statement at a table the database does not have (`audit.md`
/// A2.6).
///
/// Delete this test when `jails-protocol`'s copy goes, not before.
#[test]
fn both_pluralizers_answer_the_same_for_every_specified_rule() {
    // Every branch of §9.7, plus the compounds that make the "final word"
    // rule observable, plus the words a guesser would get wrong.
    const WORDS: &[&str] = &[
        "reward",
        "work_item",
        "address",
        "box",
        "quiz",
        "batch",
        "dish",
        "category",
        "toy",
        "knife",
        "shelf",
        "cliff",
        "status",
        "person",
        "child",
        "man",
        "woman",
        "foot",
        "tooth",
        "goose",
        "mouse",
        "support_person",
        "pocket_knife",
        "equipment",
        "information",
        "money",
        "news",
        "series",
        "species",
        "staff",
        "audio",
        "metadata",
        "data",
        "ox",
        "note",
        "task",
        "invoice",
        "company",
        "party",
        "day",
    ];
    let mut disagreements = Vec::new();
    for word in WORDS {
        let canonical = jails_model::plural_snake_case(word);
        let name = jails_protocol::identity::Name::parse(word)
            .unwrap_or_else(|error| panic!("`{word}` is not a valid name: {error}"));
        let legacy = jails_protocol::identity::SqlName::conventional_table(&name)
            .as_str()
            .to_string();
        if canonical != legacy {
            disagreements.push(format!(
                "  {word}: canonical `{canonical}`, legacy `{legacy}`"
            ));
        }
    }
    assert!(
        disagreements.is_empty(),
        "the canonical and legacy pluralizers disagree:\n{}\n\n\
         Both implement `jdl-sol.md` §9.7 and both are used on projects that \
         cross between them, so a disagreement renames a table under a running \
         application. Fix whichever one departs from the spec.",
        disagreements.join("\n")
    );
}
