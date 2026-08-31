//! One-way adoption of legacy declarations into the canonical compiler.

use crate::model_generate::{report_plan, write_bundle};
use crate::model_resource::java_to_label;
use crate::{Invocation, Output};
use jails_contracts::{CanonicalModelPatch, DocumentIntent, ModelFileUpdate, ProjectPath};
use jails_protocol::entity::{EntityId as LegacyEntityId, EntitySpec};
use jails_spec::spec::kind::{ArtifactKind, Dialect};
use jails_state::compat::MachineState;
use jails_support::{Failure, Result};
use std::collections::BTreeSet;
use std::path::Path;

const MODEL_PATH: &str = ".jails/model.jdl";
const MANAGED_JAVA: &str = ".jails/generated/main/java";
const READER_JAVA: &str = "src/main/java";

type Adoption = (ProjectPath, ProjectPath, Vec<u8>);
type Translation = (String, Vec<Adoption>);

pub(crate) fn run(invocation: Invocation) -> Result<()> {
    let root = crate::model_command::root()?;
    if root.join(MODEL_PATH).exists() || root.join(crate::model_command::TOML_PATH).exists() {
        return Err(Failure::Told(
            "this project already has a canonical model.\n       fix: use `jails sync` or edit `.jails/model.jdl`; import is one-way"
                .to_string(),
        ));
    }
    if root.join(".jails/compiler.lock.json").exists() || root.join(".jails/generated").exists() {
        return Err(Failure::Told(
            "canonical output exists without a model.\n       fix: restore the matching `.jails/model.jdl` or remove the orphaned canonical output before importing"
                .to_string(),
        ));
    }

    let state = jails_state::compat::read(&root);
    let legacy_state = match state {
        MachineState::Current(state) => state,
        MachineState::Absent => {
            return Err(Failure::Told(
                "this project has no legacy state to import.\n       fix: create `.jails/model.jdl` directly for a previously unmanaged project"
                    .to_string(),
            ));
        }
        MachineState::Unreadable(why) => return Err(Failure::Told(why)),
    };
    if legacy_state.pending_conflict.is_some() {
        return Err(Failure::Told(
            "the legacy state records an unresolved reconciliation conflict.\n       fix: finish or undo that legacy operation before importing"
                .to_string(),
        ));
    }

    let project = jails_project::model::Project::load(&root)?;
    let (draft_source, adoptions) = translate(&project, &legacy_state)?;
    // **Import renders the pre-v1 draft and upgrades it, rather than rendering
    // v1 directly.** `jdl-sol.md` §22 already owns the translation between the
    // two dialects, and it proves what a second renderer here could only
    // assert: every stable id and physical name the legacy declarations carry
    // is in the v1 model under the same id, with the same Java and SQL names.
    // A project imported and then compiled has to agree with the one that was
    // there, and that is the check.
    let build = jails_workspace::observe_build_system(&root);
    let axes = crate::model_upgrade::axes(
        build,
        jails_workspace::observe_spring_boot(&root, build).as_deref(),
    )?;
    let upgraded = jails_model::upgrade_jdl(&draft_source, axes)
        .map_err(|diagnostics| Failure::Told(diagnostics.to_string().trim_end().to_string()))?;
    if invocation.output == Output::Human {
        for note in &upgraded.notes {
            println!("note: {note}");
        }
    }
    let source = jails_model::format_jdl_v1(&upgraded.source)
        .map_err(|diagnostics| Failure::Told(diagnostics.to_string().trim_end().to_string()))?;
    let model = jails_model::parse_jdl(&source)
        .map_err(|diagnostics| Failure::Told(diagnostics.to_string().trim_end().to_string()))?;
    let reader_paths = adoptions
        .iter()
        .map(|(source, _, _)| source.clone())
        .collect::<Vec<_>>();
    let model_path = Path::new(MODEL_PATH);
    let snapshot =
        jails_workspace::capture_import(&root, model_path, source.as_bytes(), model, &reader_paths)
            .map_err(|error| Failure::Told(format!("could not capture legacy import: {error}")))?;
    let mut draft = jails_compiler::Compiler::compile(&snapshot, None)
        .map_err(|error| Failure::Told(format!("could not compile imported model: {error}")))?;
    draft.reader_document_intents.extend(
        adoptions
            .into_iter()
            .map(|(source, path, base)| DocumentIntent::AdoptJava { source, path, base }),
    );
    draft.summary.reader_document_intents = draft.reader_document_intents.len();
    let patch_bytes = serde_json::to_vec(&serde_json::json!({
        "kind": "import-legacy",
        "generation": legacy_state.generation,
    }))
    .map_err(|error| Failure::Told(format!("could not encode import patch: {error}")))?;
    let bundle = jails_workspace::materialize_with_model(
        &snapshot,
        CanonicalModelPatch {
            schema: "jails.model-patch.v1".to_string(),
            bytes: patch_bytes,
        },
        draft,
        Some(ModelFileUpdate {
            path: ProjectPath::parse(MODEL_PATH).map_err(Failure::Told)?,
            bytes: source.into_bytes(),
        }),
        jails_compiler::COMPILER_VERSION,
        jails_workspace::Restore::Refuse,
    )
    .map_err(|error| Failure::Told(format!("could not materialize legacy import: {error}")))?;

    if let Some(path) = &invocation.plan_out {
        write_bundle(path, &bundle)?;
    }
    if invocation.pretend || invocation.plan_out.is_some() {
        return report_plan(&bundle, &invocation);
    }
    let execution = jails_workspace::execute(&root, &bundle)
        .map_err(|error| Failure::Told(format!("could not apply legacy import: {error}")))?;
    if invocation.output == Output::Human {
        println!(
            "imported legacy generation {}: {} files written, {} reader files adopted",
            execution.plan_digest.as_str(),
            execution.files_written,
            execution.files_deleted
        );
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&execution)
                .map_err(|error| Failure::Told(format!("could not encode execution: {error}")))?
        );
    }
    Ok(())
}

fn translate(
    project: &jails_project::model::Project,
    legacy_state: &jails_protocol::envelope::LedgerV2,
) -> Result<Translation> {
    let java_release = project.java_release().ok_or_else(|| {
        Failure::Told(
            "the project Java release cannot be read from its build.\n       fix: declare a Maven release/source level or Gradle toolchain before importing"
                .to_string(),
        )
    })?;
    let project_label = project
        .root()
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("application");
    let stable_label = stable_label(project_label);
    let project_name = java_name(project_label);
    let dialect = match project.sql_dialect() {
        Dialect::Postgres => "postgresql",
        Dialect::H2 => "h2",
    };
    let mut source = format!(
        "application {project_name} @id(project_{stable_label})\npackage {}\njava {java_release}\ndialect {dialect}\n",
        project.base(),
    );
    let expected_identity_package = project.base();
    let domain_package = format!("{}.domain", project.base());
    let mut labels = BTreeSet::new();
    let mut adoptions = Vec::new();

    for applied in &legacy_state.applied {
        let (intent, spec) = match (&applied.id, &applied.version.spec) {
            (LegacyEntityId::Intent(intent), EntitySpec::Intent(spec))
                if matches!(intent.recipe, ArtifactKind::Record | ArtifactKind::Enum) =>
            {
                (intent, spec)
            }
            (LegacyEntityId::Intent(intent), _) => {
                return Err(unsupported(&format!(
                    "generated `{}` `{}`",
                    format!("{:?}", intent.recipe).to_ascii_lowercase(),
                    intent.name
                )));
            }
            (other, _) => return Err(unsupported(&format!("legacy declaration `{other:?}`"))),
        };
        if intent.package.as_str() != expected_identity_package {
            return Err(Failure::Told(format!(
                "{} `{}` has a custom legacy identity package `{}`, but this project base is `{expected_identity_package}`.\n       fix: keep using the legacy engine until package projections are model data",
                recipe_name(intent.recipe),
                intent.name,
                intent.package.as_str()
            )));
        }
        let label = java_to_label(intent.name.as_str());
        if !labels.insert(label.clone()) {
            return Err(Failure::Told(format!(
                "two legacy types map to canonical entity label `{label}`.\n       fix: rename one type before importing"
            )));
        }
        let arguments = spec.arguments.canonical();
        source.push('\n');
        match intent.recipe {
            ArtifactKind::Record => {
                source.push_str(&crate::model_generate_jdl::entity_declaration(
                    intent.name.as_str(),
                    &label,
                    false,
                    &arguments,
                    false,
                )?)
            }
            ArtifactKind::Enum => source.push_str(&crate::model_generate_jdl::enum_declaration(
                intent.name.as_str(),
                &label,
                &arguments,
                false,
            )?),
            _ => unreachable!("the supported recipe guard is exhaustive"),
        }
        let package_path = domain_package.replace('.', "/");
        let file = format!("{package_path}/{}.java", intent.name.as_str());
        let reader = ProjectPath::parse(format!("{READER_JAVA}/{file}")).map_err(Failure::Told)?;
        adoptions.push(adoption(
            project,
            legacy_state,
            reader,
            ProjectPath::parse(format!("{MANAGED_JAVA}/{file}")).map_err(Failure::Told)?,
            &format!("legacy {} `{}`", recipe_name(intent.recipe), intent.name),
        )?);
        if intent.recipe == ArtifactKind::Enum {
            let converter_file = format!(
                "{}/web/{}Converter.java",
                project.base().replace('.', "/"),
                intent.name.as_str()
            );
            let converter_reader = ProjectPath::parse(format!("{READER_JAVA}/{converter_file}"))
                .map_err(Failure::Told)?;
            if legacy_state
                .outputs
                .iter()
                .any(|output| output.path.as_str() == converter_reader.as_str())
            {
                adoptions.push(adoption(
                    project,
                    legacy_state,
                    converter_reader,
                    ProjectPath::parse(format!("{MANAGED_JAVA}/{converter_file}"))
                        .map_err(Failure::Told)?,
                    &format!("legacy enum converter for `{}`", intent.name),
                )?);
            }
        }
    }
    if adoptions.is_empty() {
        return Err(Failure::Told(
            "the legacy state contains no supported record or enum declarations.\n       fix: wait for the importer to cover this project's generators or create a reviewed model manually"
                .to_string(),
        ));
    }
    Ok((source, adoptions))
}

fn adoption(
    project: &jails_project::model::Project,
    legacy_state: &jails_protocol::envelope::LedgerV2,
    reader: ProjectPath,
    managed: ProjectPath,
    subject: &str,
) -> Result<(ProjectPath, ProjectPath, Vec<u8>)> {
    let output = legacy_state
        .outputs
        .iter()
        .find(|output| output.path.as_str() == reader.as_str())
        .ok_or_else(|| {
            Failure::Told(format!(
                "{subject} has no recorded base image for `{reader}`.\n       fix: regenerate it cleanly with the legacy engine before importing"
            ))
        })?;
    let base = jails_commit::store::read_object(
        &project.root().join(".jails/objects"),
        &output.base.object.id,
    )
    .map_err(|error| {
        Failure::Told(format!(
            "could not read legacy merge base for `{reader}`: {error}\n       fix: restore the legacy object store before importing"
        ))
    })?;
    if base.len() as u64 != output.base.object.len {
        return Err(Failure::Told(format!(
            "legacy merge base for `{reader}` has the wrong length.\n       fix: restore the legacy object store before importing"
        )));
    }
    Ok((reader, managed, base))
}

fn recipe_name(recipe: ArtifactKind) -> &'static str {
    match recipe {
        ArtifactKind::Record => "record",
        ArtifactKind::Enum => "enum",
        _ => "declaration",
    }
}

fn unsupported(subject: &str) -> Failure {
    Failure::Told(format!(
        "{subject} has no lossless canonical importer yet.\n       fix: import only after every recorded declaration can move in one exact plan"
    ))
}

fn stable_label(value: &str) -> String {
    let label = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    if label.starts_with(|character: char| character.is_ascii_lowercase()) {
        label
    } else {
        format!("app_{label}")
    }
}

fn java_name(value: &str) -> String {
    let name = value
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            characters.next().map_or_else(String::new, |first| {
                first.to_ascii_uppercase().to_string() + characters.as_str()
            })
        })
        .collect::<String>();
    if name.starts_with(|character: char| character.is_ascii_alphabetic()) {
        name
    } else {
        format!("Application{name}")
    }
}
