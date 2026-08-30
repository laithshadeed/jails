//! Entry points into the canonical semantic model and exact plans.

use crate::cli::ModelCommand;
use crate::{Invocation, Output};
use jails_support::{Failure, Result};
use serde_json::json;
use std::path::{Path, PathBuf};

const CHECK_SCHEMA: &str = "jails.model-check.v1";
pub(crate) const JDL_PATH: &str = ".jails/model.jdl";
pub(crate) const TOML_PATH: &str = ".jails/model.toml";

pub(crate) fn owns() -> bool {
    Path::new(JDL_PATH).is_file() || Path::new(TOML_PATH).is_file()
}

pub(crate) fn owns_jdl() -> bool {
    Path::new(JDL_PATH).is_file()
}

pub(crate) fn sync(no_start: bool, invocation: Invocation) -> Result<()> {
    if no_start {
        return Err(Failure::Told(
            "canonical sync has no external service effects and does not accept `--no-start`.\n       fix: run `jails sync` without the flag"
                .to_string(),
        ));
    }
    let manifest = resolve_manifest(None)?;
    let (source, model) = load_model(&manifest, invocation.output)?;
    let bundle = compile(&manifest, source.as_bytes(), model)?;
    let root = std::env::current_dir()
        .map_err(|error| Failure::Told(format!("could not read current directory: {error}")))?;
    let execution = jails_workspace::execute(&root, &bundle).map_err(|error| {
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
    }
}

fn format(check: bool, invocation: Invocation) -> Result<()> {
    let model_path = PathBuf::from(JDL_PATH);
    if !model_path.is_file() {
        return Err(Failure::Told(format!(
            "`jails model fmt` requires the JDL authoring source `{JDL_PATH}`.\n       fix: import or create a JDL v1 model before formatting"
        )));
    }
    let current_source = std::fs::read_to_string(&model_path).map_err(|error| {
        Failure::Told(format!(
            "could not read canonical model `{JDL_PATH}`: {error}"
        ))
    })?;
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
    let root = std::env::current_dir()
        .map_err(|error| Failure::Told(format!("could not read current directory: {error}")))?;
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

fn compile(
    manifest: &Path,
    source: &[u8],
    model: jails_model::AppModel,
) -> Result<jails_contracts::PlanBundle> {
    let root = std::env::current_dir()
        .map_err(|error| Failure::Told(format!("could not read current directory: {error}")))?;
    let reader_paths = jails_compiler::external_project_paths(&model);
    let snapshot =
        jails_workspace::capture_with_reader_paths(&root, manifest, source, model, &reader_paths)
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

fn load_model(manifest: &Path, output: Output) -> Result<(String, jails_model::AppModel)> {
    let source = match std::fs::read_to_string(manifest) {
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

fn resolve_manifest(explicit: Option<&Path>) -> Result<PathBuf> {
    let jdl = Path::new(JDL_PATH);
    let toml = Path::new(TOML_PATH);
    if jdl.is_file() && toml.is_file() {
        return Err(Failure::Told(format!(
            "this project has two editable application models: `{JDL_PATH}` and `{TOML_PATH}`.\n       fix: keep the JDL authoring source and remove the TOML compatibility source after reviewing that they describe the same model"
        )));
    }
    if let Some(explicit) = explicit {
        return Ok(explicit.to_path_buf());
    }
    if jdl.is_file() {
        return Ok(jdl.to_path_buf());
    }
    if toml.is_file() {
        return Ok(toml.to_path_buf());
    }
    Ok(jdl.to_path_buf())
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

fn print_json(value: &serde_json::Value) -> Result<()> {
    let rendered = serde_json::to_string_pretty(value)
        .map_err(|error| Failure::Told(format!("could not encode model report: {error}")))?;
    println!("{rendered}");
    Ok(())
}
