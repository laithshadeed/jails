//! Entry points into the canonical semantic model and exact plans.

use crate::cli::ModelCommand;
use crate::{Invocation, Output};
use jails_support::{Failure, Result};
use serde_json::json;
use std::path::{Path, PathBuf};

const CHECK_SCHEMA: &str = "jails.model-check.v1";
pub(crate) const JDL_PATH: &str = ".jails/model.jdl";
pub(crate) const TOML_PATH: &str = ".jails/model.toml";

/// The project a command is about: the nearest ancestor that is one.
///
/// **The same walk `jails_spec::spec::paths::find_project_root` does**, plus
/// the two model markers, and that agreement is the whole point. `owns` used
/// to test `.jails/model.jdl` against the *process* directory while the legacy
/// engine walked up to the build file, so the two disagreed about which
/// directory the command was about the moment anybody ran one from a
/// subdirectory: `jails g record` in `src/main/java` of a canonical project
/// dispatched to the legacy engine, wrote Java into the reader's own tree
/// instead of `.jails/generated`, and created a `.jails/ledger.toml` in a
/// project that must never have one.
///
/// Nearest wins, and the model markers are checked at each level before the
/// build marker, so a canonical root is recognised as one. A nested module
/// with its own build file and no model is its own legacy project rather than
/// being claimed by an ancestor's model.
pub(crate) fn project_root() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        if dir.join(JDL_PATH).is_file()
            || dir.join(TOML_PATH).is_file()
            || jails_spec::build::detect(&dir) != jails_spec::build::Build::Bare
        {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// The root every canonical command works against.
///
/// Falls back to the process directory when there is no project at all, so a
/// command run outside one refuses for its own reason rather than for this.
pub(crate) fn root() -> Result<PathBuf> {
    match project_root() {
        Some(root) => Ok(root),
        None => std::env::current_dir()
            .map_err(|error| Failure::Told(format!("could not read current directory: {error}"))),
    }
}

/// Read the model source named by a project-relative path.
///
/// **The path stays relative and only the read is anchored**, because the same
/// value becomes a `ProjectPath` in the exact plan and `ProjectPath` refuses an
/// absolute one. Every canonical mutation reads its model through here, so a
/// command run from a subdirectory reads the project's model instead of
/// reporting that the project has none -- the other half of `project_root`,
/// and the reason that walk is safe to add.
pub(crate) fn read_source(model_path: &Path) -> Result<String> {
    read_source_at(&root()?, model_path)
}

/// The same read, against a project the caller has resolved.
///
/// `jails new --app` replays a manifest into the project it is creating, and
/// the process directory is that project's *parent*.
pub(crate) fn read_source_at(root: &Path, model_path: &Path) -> Result<String> {
    match std::fs::read_to_string(root.join(model_path)) {
        Ok(source) => Ok(source),
        // The same rule as `load_model_at`: a project with no model reads as
        // the model `model init` would write, so the first mutation patches a
        // real seed rather than refusing over the file it is about to create.
        // **The derive's own refusal is what the reader needs**, not a report
        // that a file jails was about to create is missing. Discarding it said
        // "could not read `.jails/model.jdl`: No such file or directory" about
        // a project whose base package could not be read, which names neither
        // the problem nor anything to do about it.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && !owns_at(root) => {
            jails_project::model::Project::load(root)
                .and_then(|project| crate::model_init::derive(&project))
        }
        Err(error) => Err(Failure::Told(format!(
            "could not read canonical model `{}`: {error}",
            model_path.display()
        ))),
    }
}

pub(crate) fn owns() -> bool {
    project_root().is_some_and(|root| owns_at(&root))
}

/// The same question about a root the caller already knows.
///
/// `owns` walks up from the process directory, which is the wrong root for
/// every command holding an explicit one -- `jails new --app` stands in the
/// parent of the project it is creating. Same containment boundary as the
/// `_at` family.
pub(crate) fn owns_at(root: &Path) -> bool {
    root.join(JDL_PATH).is_file() || root.join(TOML_PATH).is_file()
}

/// Give this project a model if it has none, so a mutation has somewhere to go.
///
/// **This is what makes the legacy engine unreachable, and it is the whole of
/// the cutover's last step.** Every project jails creates is canonical from its
/// first command; the case left over was somebody else's repository, which had
/// no model and therefore fell through to the engine being deleted. `model
/// init` was written as the on-ramp for exactly that and then had to be run by
/// hand, which meant the fall-through survived because a reader who did not
/// know the command existed never took it.
///
/// It adopts no line of the reader's Java: what it writes is the app block,
/// every field of it read off the project rather than asked for. What changes
/// is that the *next* generator renders into `.jails/generated` through the
/// compiler, and that is said out loud rather than done quietly -- a reader
/// whose files stop appearing under `src/main/java` with no explanation has
/// been surprised by their tool.
///
/// **A legacy ledger is not initialised over.** `model init` refuses one that
/// holds declarations and sends the reader to `model import`, which is the
/// one-way carry; auto-initialising there would strand a project's whole
/// contents outside the model that now owns it. `--pretend` refuses too: a
/// dry run must not write, and there is no model to plan against.
pub(crate) fn ensure_owned(invocation: Invocation) -> Result<()> {
    // **The invocation's project, not the process directory.** `jails new
    // --app` stands in the *parent* of the project it is creating and replays
    // the manifest through these same frontends, so asking the walk would ask
    // about the wrong tree -- and answer "not canonical" about a project that
    // was seeded a moment ago.
    let root = invocation.root()?;
    if owns_at(&root) {
        return Ok(());
    }
    // **Nothing is written here, and that is the point.** This used to run
    // `model init` as its own transition before the command that needed it, so
    // `jails add csv security` on a plain Maven project created the model,
    // spliced the pom, and only *then* refused `security` -- leaving a project
    // half-converted by a command that failed. The seed is derived in memory
    // by `load_model_at`, and the mutation's own plan carries it as an
    // ordinary `ReplaceModelFile` with no before-image, so a refusal anywhere
    // in that plan writes nothing at all and a success creates the model in
    // the same reviewed transition. What is left here is the one refusal that
    // has to come first, because it is about a model this jails cannot read
    // rather than about the mutation.
    crate::model_init::refuse_if_modelled(&root)?;
    // **That this is a Java project at all is still asked here.** The seed is
    // derived from the project -- its package, its build file, its release --
    // so a directory that is not one has no model to derive and no mutation to
    // apply, and saying so before planning is what turns a report about a
    // missing `.jails/model.jdl` into the answer the reader needs.
    jails_project::model::Project::load(&root).map(|_| ())
}

/// Does this project author its model in JDL?
///
/// **A project with no model yet answers yes**, because the seed
/// `model init` writes is JDL: reading the absence as "TOML" sent the first
/// mutation on a fresh project to render the compatibility dialect, which is
/// the one being removed. `.jails/model.toml` is the only thing that makes
/// this false, and only while it is on disk.
pub(crate) fn owns_jdl() -> bool {
    project_root().is_none_or(|root| owns_jdl_at(&root))
}

pub(crate) fn owns_jdl_at(root: &Path) -> bool {
    !root.join(TOML_PATH).is_file()
}

pub(crate) fn sync(no_start: bool, invocation: Invocation) -> Result<()> {
    // Accepted rather than refused by name: a sync can introduce a compose
    // service, so a script that passes the flag is saying something coherent
    // -- it just happens to be what sync does anyway. See `sync_at`.
    sync_at(&root()?, invocation.without_starting(no_start))
}

/// `jails sync` against a root the caller already holds.
///
/// **`root()` walks up from the process directory, and `jails new` is the one
/// caller for which that is the wrong answer**: the project being created is a
/// scratch tree, and the process is standing in its parent. That is the same
/// edge `--app` hit, and the reason every legacy route already takes a
/// resolved `Project` rather than calling `discover`. Everything below this
/// wrapper already takes an explicit root -- `capture_*`, `materialize*` and
/// `execute` all do -- so this only stops the walk from happening.
pub(crate) fn sync_at(root: &Path, invocation: Invocation) -> Result<()> {
    // **`jails.toml` is still read, and a name it gets wrong is still an
    // error.** The model is what sync applies now, so nothing here acts on
    // that file's capability list -- but a `postgress` sitting in it looks
    // applied and never will be, which is the exact failure a manifest exists
    // to remove. Parsing it is also how `[layout]` is validated, so the read
    // is one the project needs either way.
    jails_project::config::Config::load(root)?;
    let manifest = resolve_manifest_at(root, None)?;
    let (source, model) = load_model_at(root, &manifest, invocation.output)?;
    let bare = model.capabilities.is_empty();
    let bundle = compile_at(root, &manifest, source.as_bytes(), model, Repair::No)?;
    // **A dry run must not write, and this one did.** `sync` is the command a
    // reader reaches for when they are least sure what the tree is about to
    // become -- a merged branch, an edited model -- so it is the last one
    // that should ignore the flag.
    if let Some(path) = &invocation.plan_out {
        crate::model_generate::write_bundle(path, &bundle)?;
    }
    if invocation.pretend || invocation.plan_out.is_some() {
        return crate::model_generate::report_plan(&bundle, &invocation);
    }
    let execution = jails_workspace::execute(root, &bundle).map_err(|error| {
        Failure::Told(format!("could not synchronize canonical model: {error}"))
    })?;
    if invocation.output == Output::Human {
        // The same distinction `add` draws: a sync over a project that is
        // already correct did all its work and changed nothing, which is the
        // answer worth saying rather than three zeroes.
        if execution.files_written == 0 && execution.files_deleted == 0 {
            println!("nothing to do, the project already matches the model");
        } else {
            println!(
                "synchronized {}: {} operations, {} files written, {} files deleted",
                execution.plan_digest.as_str(),
                execution.operations,
                execution.files_written,
                execution.files_deleted
            );
            for line in preview_lines(&bundle)
                .iter()
                .filter(|line| line.trim_start().starts_with("delete"))
            {
                println!("{line}");
            }
        }
        // **A project that has nothing to sync is usually a project that has
        // not declared anything yet**, and the report on its own reads as a
        // tool that did not work. Saying which command puts something in the
        // model costs one line and answers the question the reader is actually
        // about to ask. It is said whether or not files moved: the sync that
        // creates the model on a foreign project writes two and still has an
        // empty model to show for it.
        if bare {
            println!("       no capabilities are declared: `jails add <capability>` declares one");
        }
    } else {
        let value = serde_json::to_value(execution)
            .map_err(|error| Failure::Told(format!("could not encode execution: {error}")))?;
        print_json(&value)?;
    }
    // **Sync converges the files; `add` installs the service.** A compose
    // service that arrives through an edited model is worth naming -- the
    // reader has just gained one and nothing else would say so -- but
    // starting it is not what "make the tree match the model" means, and a
    // convergence command that brings containers up cannot be run casually.
    // `--no-start` is accepted for the scripts that pass it and changes
    // nothing here.
    crate::model_generate::run_follow_up_effects(
        root,
        &bundle,
        &invocation.clone().without_starting(true),
    )
}

pub(crate) fn refuse_legacy_mutation(command: &str, fix: &str) -> Result<()> {
    Err(Failure::Told(format!(
        "canonical project does not route `{command}` through the legacy mutation engine.\n       fix: {fix}"
    )))
}

pub(crate) fn run(command: ModelCommand, invocation: Invocation) -> Result<()> {
    match command {
        ModelCommand::Init => crate::model_init::run(invocation),
        ModelCommand::Check { manifest, frozen } => {
            let manifest = resolve_manifest(manifest.as_deref())?;
            check(&manifest, frozen, invocation.output)
        }
        ModelCommand::Upgrade { to } => crate::model_upgrade::run(to, invocation),
        ModelCommand::Fmt { check } => format(check, invocation),
        ModelCommand::Plan { manifest, bundle } => {
            let manifest = resolve_manifest(manifest.as_deref())?;
            plan(&manifest, bundle.as_deref(), invocation.output)
        }
        ModelCommand::Apply { bundle } => apply(&bundle, invocation.output),
        ModelCommand::Eject { semantic_id } => crate::model_eject::run(semantic_id, invocation),
        ModelCommand::Explain { filter } => crate::model_explain::run(filter, invocation),
    }
}

fn format(check: bool, invocation: Invocation) -> Result<()> {
    let model_path = PathBuf::from(JDL_PATH);
    if !root()?.join(&model_path).is_file() {
        return Err(Failure::Told(format!(
            "`jails model fmt` requires the JDL authoring source `{JDL_PATH}`.\n       fix: import or create a JDL v1 model before formatting"
        )));
    }
    let current_source = read_source(&model_path)?;
    let current_model = jails_model::parse_jdl(&current_source)
        .map_err(|diagnostics| Failure::Told(diagnostics.to_string().trim_end().to_string()))?;
    let next_source = jails_model::format_jdl_v1(&current_source)
        .map_err(|diagnostics| Failure::Told(diagnostics.to_string().trim_end().to_string()))?;
    let next_model = jails_model::parse_jdl(&next_source)
        .map_err(|diagnostics| Failure::Told(diagnostics.to_string().trim_end().to_string()))?;
    if current_model != next_model {
        return Err(Failure::Told(
            "the JDL formatter changed linked model semantics.\n       fix: report this formatter bug; the source was not written"
                .to_string(),
        ));
    }

    if check {
        if current_source != next_source {
            return Err(Failure::Told(format!(
                "canonical formatting differs in `{JDL_PATH}`.\n       fix: run `jails model fmt` and review the exact source update"
            )));
        }
        if invocation.output == Output::Human {
            println!("model format valid: {JDL_PATH}");
        } else {
            print_json(&json!({
                "schema": "jails.model-format.v1",
                "formatted": true,
                "changed": false,
                "manifest": JDL_PATH,
            }))?;
        }
        return Ok(());
    }

    if current_source == next_source {
        if invocation.output == Output::Human {
            println!("model already formatted: {JDL_PATH}");
        } else {
            print_json(&json!({
                "schema": "jails.model-format.v1",
                "formatted": true,
                "changed": false,
                "manifest": JDL_PATH,
            }))?;
        }
        return Ok(());
    }

    crate::model_generate::finish_generation(crate::model_generate::PreparedMutation {
        name: "JDL formatting".to_string(),
        invocation,
        model_path,
        current_source,
        current_model,
        next_source,
        patch: jails_model::ModelPatch::Batch(Vec::new()),
        patch_bytes: br#"{"kind":"format"}"#.to_vec(),
        authored_migration: None,
    })
}

pub(crate) fn apply(bundle_path: &Path, output: Output) -> Result<()> {
    let bytes = std::fs::read(bundle_path).map_err(|error| {
        Failure::Told(format!(
            "could not read exact plan bundle `{}`: {error}\n       fix: pass the file written by `jails model plan --bundle <path>`",
            bundle_path.display()
        ))
    })?;
    let bundle: jails_contracts::PlanBundle = serde_json::from_slice(&bytes).map_err(|error| {
        Failure::Told(format!(
            "could not decode exact plan bundle `{}`: {error}\n       fix: regenerate the bundle with this version of jails",
            bundle_path.display()
        ))
    })?;
    let root = crate::model_command::root()?;
    let execution = jails_workspace::execute(&root, &bundle)
        .map_err(|error| Failure::Told(format!("could not apply exact plan: {error}")))?;
    if output == Output::Human {
        println!(
            "applied {}: {} operations, {} files written, {} files deleted",
            execution.plan_digest.as_str(),
            execution.operations,
            execution.files_written,
            execution.files_deleted
        );
    } else {
        let value = serde_json::to_value(execution)
            .map_err(|error| Failure::Told(format!("could not encode execution: {error}")))?;
        print_json(&value)?;
    }
    Ok(())
}

fn check(manifest: &Path, frozen: bool, output: Output) -> Result<()> {
    let (source, model) = load_model(manifest, output)?;
    let bundle = frozen
        .then(|| compile(manifest, source.as_bytes(), model.clone()))
        .transpose()?;
    if let Some(bundle) = &bundle
        && !bundle.plan.operations.is_empty()
    {
        return frozen_failure(manifest, bundle, output);
    }

    if output == Output::Human {
        println!(
            "model valid{}: {} ({} nodes, {} entities, {} operations)",
            if frozen { " and frozen" } else { "" },
            manifest.display(),
            model.node_count(),
            model.entities.len(),
            model.operations.len()
        );
    } else {
        print_json(&json!({
            "schema": CHECK_SCHEMA,
            "valid": true,
            "frozen": frozen,
            "manifest": manifest,
            "summary": {
                "nodes": model.node_count(),
                "capabilities": model.capabilities.len(),
                "entities": model.entities.len(),
                "operations": model.operations.len(),
            },
            "model": model,
            "plan_digest": bundle.map(|bundle| bundle.plan.digest),
            "diagnostics": [],
        }))?;
    }
    Ok(())
}

/// What this plan would do, one line per path, in the order it would do it.
///
/// **A dry run that prints a count is not a dry run.** The question a reader
/// asks it is which of *their* files it is about to rewrite, and a digest and
/// an operation count answer neither -- the legacy preview listed the paths
/// and losing that was a regression, not a simplification. Verbs are the
/// executor's own distinctions rather than prose: a managed tree publishes,
/// a reader file is patched or removed, a migration is appended and can never
/// be rewritten.
///
/// The managed tree expands to its files. It is one operation carrying a
/// whole after-image, so reporting it as `publish .jails/generated` hides
/// exactly the thing that changed, and the tree manifest is already in the
/// bundle -- no filesystem read, and nothing here can disagree with what
/// apply will write.
/// Every path this bundle removes, managed tree entries included.
///
/// Shared with [`preview_lines`] so the sweep of compiled shadows cannot
/// disagree with the deletions the reader was shown.
pub(crate) fn deleted_paths(
    bundle: &jails_contracts::PlanBundle,
) -> Vec<jails_contracts::ProjectPath> {
    use jails_contracts::PlannedOperation as Op;
    let mut paths = Vec::new();
    for operation in &bundle.plan.operations {
        match operation {
            Op::PublishMergedTree { before, after, .. } => {
                let entries = |digest: &jails_contracts::ContentDigest| {
                    bundle
                        .trees
                        .get(digest)
                        .map(|tree| tree.entries.keys().cloned().collect())
                        .unwrap_or_default()
                };
                let was: std::collections::BTreeSet<_> =
                    before.as_ref().map(entries).unwrap_or_default();
                let now: std::collections::BTreeSet<_> = entries(after);
                paths.extend(was.difference(&now).cloned());
            }
            Op::RemoveReaderFile { path, .. } => paths.push(path.clone()),
            _ => {}
        }
    }
    paths
}

pub(crate) fn preview_lines(bundle: &jails_contracts::PlanBundle) -> Vec<String> {
    use jails_contracts::PlannedOperation as Op;
    let mut lines = Vec::new();
    for operation in &bundle.plan.operations {
        match operation {
            Op::PublishMergedTree {
                root,
                before,
                after,
            } => {
                let was = before
                    .as_ref()
                    .and_then(|digest| bundle.trees.get(digest))
                    .map(|tree| {
                        tree.entries
                            .keys()
                            .collect::<std::collections::BTreeSet<_>>()
                    })
                    .unwrap_or_default();
                let now = bundle
                    .trees
                    .get(after)
                    .map(|tree| {
                        tree.entries
                            .keys()
                            .collect::<std::collections::BTreeSet<_>>()
                    })
                    .unwrap_or_default();
                // Tree entries are already project-relative, so the root is
                // the operation's subject rather than a prefix to prepend.
                let _ = root;
                for path in now.union(&was) {
                    let verb = match (was.contains(*path), now.contains(*path)) {
                        (false, _) => "create",
                        (true, true) => "write",
                        (true, false) => "delete",
                    };
                    lines.push(format!("  {verb:<8}{}", path.as_str()));
                }
            }
            Op::ReplaceModelFile { path, before, .. }
            | Op::ReplaceStateFile { path, before, .. } => {
                let verb = if before.is_some() { "write" } else { "create" };
                lines.push(format!("  {verb:<8}{}", path.as_str()));
            }
            Op::PatchReaderFile { path, before, .. } => {
                let verb = if before.is_some() { "patch" } else { "create" };
                lines.push(format!("  {verb:<8}{}", path.as_str()));
            }
            Op::RemoveReaderFile { path, .. } => {
                lines.push(format!("  {:<8}{}", "delete", path.as_str()));
            }
            Op::AppendMigration { path, .. } => {
                lines.push(format!("  {:<8}{}", "append", path.as_str()));
            }
        }
    }
    lines
}

fn plan(manifest: &Path, bundle_path: Option<&Path>, output: Output) -> Result<()> {
    let (source, model) = load_model(manifest, output)?;
    let bundle = compile(manifest, source.as_bytes(), model)?;
    let encoded = serde_json::to_vec_pretty(&bundle)
        .map_err(|error| Failure::Told(format!("could not encode exact plan: {error}")))?;
    if let Some(path) = bundle_path {
        jails_support::apply::put_outside_project_private_atomic(path, &encoded)?;
    }
    if output == Output::Human {
        println!(
            "plan {}: {} operations, {} managed files{}",
            bundle.plan.digest.as_str(),
            bundle.plan.operations.len(),
            bundle.plan.summary.managed_files,
            bundle_path.map_or_else(String::new, |path| format!(", bundle {}", path.display()))
        );
        for line in preview_lines(&bundle) {
            println!("{line}");
        }
    } else {
        println!("{}", String::from_utf8_lossy(&encoded));
    }
    Ok(())
}

/// Compile and apply a freshly seeded model, printing nothing.
///
/// `jails new` needs the first canonical plan to *run* rather than to be
/// reported: the project it creates has to arrive with its
/// `application.properties` written and `.jails/compiler.lock.json` recording
/// which keys the model owns. Without that lock the very next command sees
/// every key `new` wrote as reader-owned text and refuses to touch it.
pub(crate) fn materialize_seed(root: &Path) -> Result<()> {
    let manifest = resolve_manifest_at(root, None)?;
    let (source, model) = load_model_at(root, &manifest, Output::Human)?;
    let bundle = compile_at(root, &manifest, source.as_bytes(), model, Repair::No)?;
    jails_workspace::execute(root, &bundle)
        .map_err(|error| Failure::Told(format!("could not apply the seeded model: {error}")))?;
    Ok(())
}

fn compile(
    manifest: &Path,
    source: &[u8],
    model: jails_model::AppModel,
) -> Result<jails_contracts::PlanBundle> {
    compile_at(&root()?, manifest, source, model, Repair::No)
}

/// Whether this compilation is `jails resource repair`.
///
/// It rides on `compile_at` rather than on a wrapper beside it because the
/// `root: &Path` ladder gate counts functions, not parameters: a second
/// root-taking entry point is exactly the proliferation §8.0 is watching for,
/// and this is one more value the existing one decides with.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum Repair {
    No,
    DeletedManagedFiles,
}

fn compile_at(
    root: &Path,
    manifest: &Path,
    source: &[u8],
    model: jails_model::AppModel,
    repair: Repair,
) -> Result<jails_contracts::PlanBundle> {
    let reader_paths = jails_compiler::external_project_paths(&model);
    let snapshot =
        jails_workspace::capture_with_reader_paths(root, manifest, source, model, &reader_paths)
            .map_err(|error| Failure::Told(format!("could not capture workspace: {error}")))?;
    let draft = jails_compiler::Compiler::compile(&snapshot, None)
        .map_err(|error| Failure::Told(format!("could not compile application model: {error}")))?;
    // Same capture, same model, same compiler: repair differs only in what
    // materialization does about a managed file that is no longer on disk.
    jails_workspace::materialize(
        &snapshot,
        jails_contracts::CanonicalModelPatch::reconcile(),
        draft,
        jails_compiler::COMPILER_VERSION,
        match repair {
            Repair::No => jails_workspace::Restore::Refuse,
            Repair::DeletedManagedFiles => jails_workspace::Restore::Deleted,
        },
    )
    .map_err(|error| Failure::Told(format!("could not materialize exact plan: {error}")))
}

/// `jails resource repair` on a canonical project.
///
/// **Ordinary compilation with one guard waived**, which is the whole of it:
/// managed output below `.jails/generated` is reproducible from the model, so
/// a file the reader deleted has an exact answer and repair is writing it.
///
/// The legacy engine repairs by re-deriving files from its ledger, and this
/// command refused on a canonical project with a fix line naming `jails sync`.
/// That was a dead end -- `sync` refuses on the same deleted file, so the two
/// commands pointed at each other and neither wrote anything. It takes no
/// `--strategy`: there is one strategy, and it is the model.
pub(crate) fn repair(invocation: Invocation) -> Result<()> {
    let root = root()?;
    let manifest = resolve_manifest_at(&root, None)?;
    let (source, model) = load_model_at(&root, &manifest, invocation.output)?;
    let bundle = compile_at(
        &root,
        &manifest,
        source.as_bytes(),
        model,
        Repair::DeletedManagedFiles,
    )?;
    let execution = jails_workspace::execute(&root, &bundle)
        .map_err(|error| Failure::Told(format!("could not repair managed output: {error}")))?;
    if invocation.output == Output::Human {
        if execution.files_written == 0 && execution.files_deleted == 0 {
            println!("managed output already matches the model: nothing to repair");
        } else {
            println!(
                "repaired {}: {} files written, {} files deleted",
                execution.plan_digest.as_str(),
                execution.files_written,
                execution.files_deleted
            );
        }
        return Ok(());
    }
    let value = serde_json::to_value(execution)
        .map_err(|error| Failure::Told(format!("could not encode execution: {error}")))?;
    print_json(&value)?;
    Ok(())
}

fn frozen_failure(
    manifest: &Path,
    bundle: &jails_contracts::PlanBundle,
    output: Output,
) -> Result<()> {
    let message = format!(
        "managed output differs from model `{}` (plan {})",
        manifest.display(),
        bundle.plan.digest.as_str()
    );
    let fix = "review `jails model plan`, then apply that exact plan";
    if output == Output::Human {
        return Err(Failure::Told(format!("{message}\n       fix: {fix}")));
    }
    print_json(&json!({
        "schema": CHECK_SCHEMA,
        "valid": false,
        "frozen": false,
        "manifest": manifest,
        "plan_digest": bundle.plan.digest,
        "diagnostics": [{
            "code": "model-generated-drift",
            "path": ".jails/generated",
            "message": message,
            "fix": fix,
        }],
    }))?;
    Err(Failure::Reported)
}

pub(crate) fn load_model(
    manifest: &Path,
    output: Output,
) -> Result<(String, jails_model::AppModel)> {
    load_model_at(&root()?, manifest, output)
}

pub(crate) fn load_model_at(
    root: &Path,
    manifest: &Path,
    output: Output,
) -> Result<(String, jails_model::AppModel)> {
    // Joined to the project root, which is a no-op on the absolute path an
    // explicit `--manifest` resolves to. See `resolve_manifest`.
    let source = match std::fs::read_to_string(root.join(manifest)) {
        Ok(source) => source,
        // **A project with no model reads as the model `model init` would
        // write**, so a read-only command has something real to answer from.
        // `--pretend` used to refuse here, which made the dry run of every
        // mutation the one thing a reader could not do on a project jails had
        // not touched yet -- exactly when they most want to see the plan
        // first. Deriving it twice is free and cannot disagree: the seed is a
        // pure function of the project, and `model init` writes this same
        // source. Only a missing default source falls back, so an unreadable
        // model and a mistyped `--manifest` are still errors rather than a
        // silent plan against something the reader did not write.
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound && manifest == Path::new(JDL_PATH) =>
        {
            // A project jails cannot read has no seed to derive, and the
            // honest answer there is still that the model is missing --
            // reporting why the *derivation* failed would answer a question
            // the reader did not ask.
            let derived = jails_project::model::Project::load(root)
                .and_then(|project| crate::model_init::derive(&project));
            match derived {
                Ok(source) => source,
                Err(_) => return io_failure(manifest, &error, output),
            }
        }
        Err(error) => return io_failure(manifest, &error, output),
    };
    let parsed = if manifest.extension().and_then(|value| value.to_str()) == Some("jdl") {
        jails_model::parse_jdl(&source)
    } else {
        jails_model::parse_toml(&source)
    };
    match parsed {
        Ok(model) => Ok((source, model)),
        Err(diagnostics) if output == Output::Human => Err(Failure::Told(
            diagnostics.to_string().trim_end().to_string(),
        )),
        Err(diagnostics) => {
            print_json(&json!({
                "schema": CHECK_SCHEMA,
                "valid": false,
                "manifest": manifest,
                "diagnostics": diagnostics.diagnostics,
            }))?;
            Err(Failure::Reported)
        }
    }
}

/// Which of the two editable sources this project authors its model in.
///
/// **The default is returned project-relative and the *explicit* one
/// absolute**, because the two are relative to different things: a default is
/// a fact about the project, while `--manifest` is a path the reader typed in
/// their own directory. Anchoring the default here instead would put an
/// absolute path into every report and every plan; resolving the explicit one
/// lazily would read it against the project root the moment the command ran
/// from a subdirectory. `load_model` joins the root either way, which is a
/// no-op on an absolute path.
pub(crate) fn resolve_manifest(explicit: Option<&Path>) -> Result<PathBuf> {
    resolve_manifest_at(&root()?, explicit)
}

pub(crate) fn resolve_manifest_at(root: &Path, explicit: Option<&Path>) -> Result<PathBuf> {
    let jdl_path = root.join(JDL_PATH);
    let toml_path = root.join(TOML_PATH);
    let (jdl, toml) = (jdl_path.as_path(), toml_path.as_path());
    if jdl.is_file() && toml.is_file() {
        return Err(Failure::Told(format!(
            "this project has two editable application models: `{JDL_PATH}` and `{TOML_PATH}`.\n       fix: keep the JDL authoring source and remove the TOML compatibility source after reviewing that they describe the same model"
        )));
    }
    if let Some(explicit) = explicit {
        return std::path::absolute(explicit).map_err(|error| {
            Failure::Told(format!(
                "could not resolve `--manifest {}`: {error}",
                explicit.display()
            ))
        });
    }
    if toml.is_file() && !jdl.is_file() {
        return Ok(PathBuf::from(TOML_PATH));
    }
    Ok(PathBuf::from(JDL_PATH))
}

fn io_failure<T>(manifest: &Path, error: &std::io::Error, output: Output) -> Result<T> {
    let message = format!(
        "could not read application model `{}`: {error}",
        manifest.display()
    );
    let fix = "create the model or pass `--manifest <path>`";
    if output == Output::Human {
        return Err(Failure::Told(format!("{message}\n       fix: {fix}")));
    }
    print_json(&json!({
        "schema": CHECK_SCHEMA,
        "valid": false,
        "manifest": manifest,
        "diagnostics": [{
            "code": "model-io",
            "path": "$",
            "message": message,
            "fix": fix,
        }],
    }))?;
    Err(Failure::Reported)
}

pub(crate) fn print_json(value: &serde_json::Value) -> Result<()> {
    let rendered = serde_json::to_string_pretty(value)
        .map_err(|error| Failure::Told(format!("could not encode model report: {error}")))?;
    println!("{rendered}");
    Ok(())
}
