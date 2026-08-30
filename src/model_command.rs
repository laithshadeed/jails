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
    std::fs::read_to_string(root()?.join(model_path)).map_err(|error| {
        Failure::Told(format!(
            "could not read canonical model `{}`: {error}",
            model_path.display()
        ))
    })
}

pub(crate) fn owns() -> bool {
    project_root()
        .is_some_and(|root| root.join(JDL_PATH).is_file() || root.join(TOML_PATH).is_file())
}

pub(crate) fn owns_jdl() -> bool {
    project_root().is_some_and(|root| root.join(JDL_PATH).is_file())
}

pub(crate) fn sync(no_start: bool, invocation: Invocation) -> Result<()> {
    if no_start {
        return Err(Failure::Told(
            "canonical sync has no external service effects and does not accept `--no-start`.\n       fix: run `jails sync` without the flag"
                .to_string(),
        ));
    }
    sync_at(&root()?, invocation)
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
    let manifest = resolve_manifest_at(root, None)?;
    let (source, model) = load_model_at(root, &manifest, invocation.output)?;
    let bundle = compile_at(root, &manifest, source.as_bytes(), model)?;
    let execution = jails_workspace::execute(root, &bundle).map_err(|error| {
        Failure::Told(format!("could not synchronize canonical model: {error}"))
    })?;
    if invocation.output == Output::Human {
        println!(
            "synchronized {}: {} operations, {} files written, {} files deleted",
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

pub(crate) fn refuse_legacy_mutation(command: &str, fix: &str) -> Result<()> {
    Err(Failure::Told(format!(
        "canonical project does not route `{command}` through the legacy mutation engine.\n       fix: {fix}"
    )))
}

pub(crate) fn run(command: ModelCommand, invocation: Invocation) -> Result<()> {
    match command {
        ModelCommand::Import => crate::model_import::run(invocation),
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

fn apply(bundle_path: &Path, output: Output) -> Result<()> {
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
    let bundle = compile_at(root, &manifest, source.as_bytes(), model)?;
    jails_workspace::execute(root, &bundle)
        .map_err(|error| Failure::Told(format!("could not apply the seeded model: {error}")))?;
    Ok(())
}

fn compile(
    manifest: &Path,
    source: &[u8],
    model: jails_model::AppModel,
) -> Result<jails_contracts::PlanBundle> {
    compile_at(&root()?, manifest, source, model)
}

fn compile_at(
    root: &Path,
    manifest: &Path,
    source: &[u8],
    model: jails_model::AppModel,
) -> Result<jails_contracts::PlanBundle> {
    let reader_paths = jails_compiler::external_project_paths(&model);
    let snapshot =
        jails_workspace::capture_with_reader_paths(root, manifest, source, model, &reader_paths)
            .map_err(|error| Failure::Told(format!("could not capture workspace: {error}")))?;
    let draft = jails_compiler::Compiler::compile(&snapshot, None)
        .map_err(|error| Failure::Told(format!("could not compile application model: {error}")))?;
    jails_workspace::materialize(
        &snapshot,
        jails_contracts::CanonicalModelPatch::reconcile(),
        draft,
        jails_compiler::COMPILER_VERSION,
    )
    .map_err(|error| Failure::Told(format!("could not materialize exact plan: {error}")))
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
