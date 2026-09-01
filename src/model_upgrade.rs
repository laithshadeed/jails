//! `jails model upgrade --to 1`: the pre-v1 draft, moved onto JDL v1.
//!
//! `jdl-sol.md` §22 mandates this command and the shape it takes: it "produces
//! a diff and requires normal review/apply before replacing the source". That
//! is the ordinary canonical mutation flow, so the whole command is a source
//! rewrite handed to [`crate::model_generate::finish_generation`] -- the same
//! capture, compile, materialize and preview-or-apply every other canonical
//! mutation goes through. Nothing here writes.
//!
//! **The two axes are read here, not in the translator.** §22 requires the
//! upgrade to "inspect the selected module once and materialize `platform
//! spring|plain` and `build maven|gradle`", and a module with an unsupported
//! build language "aborts upgrade with a diagnostic; it is never guessed".
//! `BuildSystem::Unknown` is that abort. The translator takes the answer as a
//! value so it cannot reach for the filesystem itself.
//!
//! **The patch is `ReplaceModel`, and that is not a shortcut.** The two
//! dialects do not link to the same model: v1 materializes projections from
//! `use`, links operation parameters, and reads `storage` as a capability.
//! No sequence of field- and entity-level patches describes that, and
//! `jails_model::upgrade` has already proved every stable ID and physical name
//! survives -- which is the property an ordinary patch would have been
//! carrying.

use crate::Invocation;
use crate::model_command::{JDL_PATH, TOML_PATH};
use crate::model_generate::{
    CarryAcross, PreparedMutation, finish_carry_across, finish_generation,
};
use jails_contracts::BuildSystem;
use jails_model::{JdlAxes, JdlBuild, JdlPlatform, ModelPatch};
use jails_support::{Failure, Result};
use std::path::PathBuf;

pub(crate) fn run(to: u16, invocation: Invocation) -> Result<()> {
    if to != 1 {
        return Err(Failure::Told(format!(
            "there is no JDL version {to}.\n       fix: run `jails model upgrade --to 1`"
        )));
    }
    let root = crate::model_command::root()?;
    // Relative, because it becomes a `ProjectPath` in the plan; every read of
    // it is anchored to `root`. See `model_command::project_root`.
    let model_path = PathBuf::from(JDL_PATH);
    if !root.join(&model_path).is_file() {
        // §22: "Legacy TOML model state is imported into the same v1 AST
        // through a separate one-shot command." This is that command, and the
        // TOML case is a different translation from the pre-v1 JDL one: there
        // is no source to rewrite, only a model to render.
        if root.join(TOML_PATH).is_file() {
            return carry_toml_across(invocation);
        }
        return Err(Failure::Told(format!(
            "`jails model upgrade` requires the JDL authoring source `{JDL_PATH}`.\n       fix: run `jails model init` for a project jails did not create, or write `{JDL_PATH}` directly"
        )));
    }
    let current_source = crate::model_command::read_source(&model_path)?;
    let current_model = jails_model::parse_jdl(&current_source)
        .map_err(|diagnostics| Failure::Told(diagnostics.to_string().trim_end().to_string()))?;

    let build = jails_workspace::observe_build_system(&root);
    let spring_boot = jails_workspace::observe_spring_boot(&root, build);
    let axes = axes(build, spring_boot.as_deref())?;
    let upgraded = jails_model::upgrade_jdl(&current_source, axes)
        .map_err(|diagnostics| Failure::Told(diagnostics.to_string().trim_end().to_string()))?;
    if invocation.output == crate::Output::Human {
        for note in &upgraded.notes {
            println!("note: {note}");
        }
    }
    let next_source = upgraded.source;
    // Formatting is `model fmt`'s job everywhere else, but an upgraded source
    // is the one file nobody wrote by hand: leaving it unformatted would make
    // the reviewed diff carry layout noise on top of the translation, and the
    // very next `jails model fmt --check` would fail on a file jails produced.
    let next_source = jails_model::format_jdl_v1(&next_source)
        .map_err(|diagnostics| Failure::Told(diagnostics.to_string().trim_end().to_string()))?;
    let next_model = jails_model::parse_jdl(&next_source)
        .map_err(|diagnostics| Failure::Told(diagnostics.to_string().trim_end().to_string()))?;

    finish_generation(PreparedMutation {
        name: "JDL v1 upgrade".to_string(),
        invocation,
        model_path,
        current_source,
        current_model,
        next_source,
        patch: ModelPatch::ReplaceModel(Box::new(next_model)),
        patch_bytes: br#"{"kind":"upgrade","to":1}"#.to_vec(),
        authored_migration: None,
    })
}

/// The one-shot route off `.jails/model.toml`.
///
/// **The two upgrades are different translations and share only their
/// ending.** The pre-v1 JDL one rewrites *source*: it has a document with
/// comments and an order the reader chose, so `upgrade_jdl` edits the text and
/// proves identity survived. There is no TOML text worth preserving -- its
/// tables are unordered and nobody reads it as prose -- so this one links the
/// model and renders it, which is why it needs `render_jdl_v1` rather than a
/// second text rewriter.
///
/// **It retires the TOML in the same plan, and that is the whole point.**
/// Writing `.jails/model.jdl` and leaving `.jails/model.toml` behind is two
/// editable model sources, which `docs/00-contracts.md` forbids -- and a crash
/// between two plans would leave that state permanently. One plan, both
/// operations, or neither.
fn carry_toml_across(invocation: Invocation) -> Result<()> {
    let toml_path = PathBuf::from(TOML_PATH);
    let current_source = crate::model_command::read_source(&toml_path)?;
    let current_model = jails_model::parse_toml(&current_source)
        .map_err(|diagnostics| Failure::Told(diagnostics.to_string().trim_end().to_string()))?;

    // **The axes are observed, never defaulted.** §22 requires the upgrade to
    // "inspect the selected module once and materialize `platform
    // spring|plain` and `build maven|gradle`", and a module whose build cannot
    // be observed "aborts upgrade with a diagnostic; it is never guessed".
    // `.jails/model.toml` carries neither axis, and `ProjectIntent` defaults
    // them to `spring`/`maven` -- so taking the model's word marks every plain
    // project Spring. Caught by upgrading a `new-cli` project and reading the
    // header it wrote.
    // Off the `Invocation` rather than as a `root: &Path` parameter: the
    // resolved root is already threaded everywhere, and re-deriving it from a
    // primitive is the rung `root: &Path` counts.
    let root = invocation.root()?;
    let build = jails_workspace::observe_build_system(&root);
    let spring_boot = jails_workspace::observe_spring_boot(&root, build);
    let observed = axes(build, spring_boot.as_deref())?;

    // **The one translation that means something, made explicit.** §22: the
    // upgrade "produces a diff and requires normal review" because `dialect
    // postgresql` becomes `storage postgres`, and v1 reads a SQL storage axis
    // as a capability -- so a TOML model that declared a dialect without one
    // gains a JDBC adapter. The renderer refuses to add it silently, so the
    // capability is materialised here, where it can be said out loud.
    let mut rendered_from = current_model.clone();
    rendered_from.project.platform = match observed.platform {
        JdlPlatform::Spring => "spring",
        JdlPlatform::Plain => "plain",
    }
    .to_string();
    rendered_from.project.build = match observed.build {
        JdlBuild::Maven => "maven",
        JdlBuild::Gradle => "gradle",
    }
    .to_string();
    let mut gained = None;
    if let Some(kind) = jails_model::storage_capability(&current_model.project.dialect)
        && !current_model
            .capabilities
            .values()
            .any(|capability| capability.kind == kind)
    {
        let id = jails_model::CapabilityId::parse(format!("cap_{kind}"))
            .map_err(|error| Failure::Told(format!("could not assign capability id: {error}")))?;
        rendered_from
            .apply(jails_model::ModelPatch::AddCapability(
                jails_model::Capability {
                    id,
                    label: kind.to_string(),
                    kind: kind.to_string(),
                    name: None,
                    java_package: None,
                },
            ))
            .map_err(|error| Failure::Told(format!("could not materialize `{kind}`: {error}")))?;
        gained = Some(kind);
    }

    // Rendered from the linked model, and `render_jdl_v1` parses and links
    // what it wrote before returning it -- so a construct it cannot state
    // refuses here rather than producing a model that quietly lost a field.
    let next_source = jails_model::render_jdl_v1(&rendered_from)
        .map_err(|diagnostics| Failure::Told(diagnostics.to_string().trim_end().to_string()))?;
    let next_model = jails_model::parse_jdl(&next_source)
        .map_err(|diagnostics| Failure::Told(diagnostics.to_string().trim_end().to_string()))?;
    if invocation.output == crate::Output::Human {
        println!("note: `{TOML_PATH}` is retired by this plan; `{JDL_PATH}` replaces it");
        if let Some(kind) = gained {
            println!(
                "note: `storage {}` materializes the `{kind}` capability, which this model did not declare",
                current_model.project.dialect
            );
        }
    }
    finish_carry_across(
        PreparedMutation {
            name: "JDL v1 upgrade".to_string(),
            invocation,
            model_path: toml_path,
            current_source,
            current_model,
            next_source,
            patch: ModelPatch::ReplaceModel(Box::new(next_model)),
            patch_bytes: br#"{"kind":"upgrade","to":1,"from":"toml"}"#.to_vec(),
            authored_migration: None,
        },
        CarryAcross {
            writes_to: PathBuf::from(JDL_PATH),
            retires: vec![jails_contracts::ProjectPath::parse(TOML_PATH).map_err(Failure::Told)?],
        },
    )
}

/// The `platform` and `build` axes, from facts the workspace already observes.
///
/// **The facts are read by `jails-workspace`, not here.** They are the same two
/// the very next capture will record, and a second pair of `is_file` calls in
/// this file would be a second answer to a question the snapshot already
/// answers -- which is exactly how a project ends up upgraded to `build maven`
/// and then compiled as Gradle.
///
/// Spring is evidence, not a default: a module with no Boot parent is `plain`,
/// which is a fact about it rather than an absence of one. Only the build
/// language can be genuinely unobservable, and §22 makes that an abort --
/// "a module with conflicting build evidence, an unsupported build language, or
/// ambiguous platform evidence aborts upgrade with a diagnostic; it is never
/// guessed."
pub(crate) fn axes(build: BuildSystem, spring_boot: Option<&str>) -> Result<JdlAxes> {
    let build = match build {
        BuildSystem::Maven => JdlBuild::Maven,
        BuildSystem::Gradle => JdlBuild::Gradle,
        BuildSystem::Unknown => {
            return Err(Failure::Told(
                "this module has no `pom.xml` and no Gradle build script, or has both, so its `build` axis cannot be observed.\n       fix: JDL v1 requires `build maven` or `build gradle`; leave exactly one build file in place, then upgrade"
                    .to_string(),
            ));
        }
    };
    Ok(JdlAxes {
        platform: if spring_boot.is_some() {
            JdlPlatform::Spring
        } else {
            JdlPlatform::Plain
        },
        build,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unobservable_build_language_aborts_rather_than_defaulting_to_maven() {
        let error = axes(BuildSystem::Unknown, Some("4.0.0")).unwrap_err();
        assert!(format!("{error}").contains("`build` axis cannot be observed"));
    }

    #[test]
    fn a_module_with_no_boot_parent_is_plain_rather_than_unobservable() {
        let axes = axes(BuildSystem::Maven, None).unwrap();
        assert_eq!(axes.platform, JdlPlatform::Plain);
        assert_eq!(axes.build, JdlBuild::Maven);
    }
}
