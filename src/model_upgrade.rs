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
use crate::model_generate::{PreparedMutation, finish_generation};
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
        // A project on the TOML compatibility input has its own one-shot route.
        // §22: "Legacy TOML model state is imported into the same v1 AST
        // through a separate one-shot command."
        let fix = if root.join(TOML_PATH).is_file() {
            format!(
                "`{TOML_PATH}` is the temporary compatibility input, not a JDL source; there is no in-place upgrade for it"
            )
        } else {
            "run `jails model import` first, or write `.jails/model.jdl` directly".to_string()
        };
        return Err(Failure::Told(format!(
            "`jails model upgrade` requires the JDL authoring source `{JDL_PATH}`.\n       fix: {fix}"
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
