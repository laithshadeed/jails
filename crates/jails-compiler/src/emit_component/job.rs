//! `component job <Name>`: work on a schedule.
//!
//! Two files per job and **one for the whole model**. `SchedulingConfig` turns
//! scheduling on and sizes its pool, and every job in a project needs the same
//! one — so it is emitted once from [`super::lower_and_emit`] rather than by
//! each job, because a managed tree refuses two units writing one path and a
//! second job would otherwise fail the compile.
//!
//! **The pool size is the reason the config is generated at all.** Spring's
//! default `spring.task.scheduling.pool.size` is 1, so a second job waits for
//! the first however unrelated they are, and a job that hangs stops every
//! other one in the application. Nothing reports that: the jobs simply do not
//! run. `CLAUDE.md` records the same defaulted-wrong shape for `g auth`, and
//! the answer is the same — generate the fix and let the test hold it.

use super::{Emitted, Package, java, package};
use crate::CompileError;
use jails_contracts::{FileKind, FileMode, ProjectPath, Provenance, RenderedFile};
use jails_model::{AppModel, Component, StableId};
use std::collections::BTreeSet;

const JOB: crate::Template = crate::template!("spring/job_java.java");
const TEST: crate::Template = crate::template!("spring/job_test_java.java");
const SCHEDULING: crate::Template = crate::template!("spring/scheduling_config_java.java");

pub(super) fn files(
    model: &AppModel,
    component: &Component,
    templates: &jails_contracts::TemplateOverrides,
) -> Result<Vec<Emitted>, CompileError> {
    let name = &component.name;
    let pkg = package(model, Package::Jobs);
    let property = component.label.replace('_', "-");
    let substitute = |template: crate::Template| -> Result<String, CompileError> {
        let template = template.resolve(templates)?;
        Ok(template
            .replace("{{pkg}}", &pkg)
            .replace("{{name}}", name)
            .replace("{{property}}", &property))
    };
    Ok(vec![
        java(
            component,
            "job",
            &pkg,
            &format!("{name}Job"),
            false,
            true,
            substitute(JOB)?,
        )?,
        java(
            component,
            "test",
            &pkg,
            &format!("{name}JobTest"),
            true,
            true,
            substitute(TEST)?,
        )?,
    ])
}

/// The one `SchedulingConfig` every job in this model shares, or `None` when
/// there are no jobs.
///
/// Its `semantic_ids` name every job rather than one, which is what the set is
/// for: removing a single job must not retire a file the others still need,
/// and the merge history has to survive that.
pub(super) fn scheduling(
    model: &AppModel,
    templates: &jails_contracts::TemplateOverrides,
) -> Result<Option<Emitted>, CompileError> {
    let mut owners = model
        .components
        .values()
        .filter(|component| {
            // Presence sweeps a table on a schedule, so it needs the same
            // config: without `@EnableScheduling` the sweep annotation is
            // inert and nothing says so -- the table just grows a row per
            // crashed node forever.
            matches!(
                component.kind,
                jails_model::ComponentKind::Job
                    | jails_model::ComponentKind::Presence
                    // A traversal claims its frontier on a schedule, and
                    // without the config the claim never runs: the run sits
                    // QUEUED forever and nothing says why.
                    | jails_model::ComponentKind::HttpWorkflow
                    // A durable job's worker drains its queue on a schedule.
                    // Without the config nothing claims: items sit PENDING
                    // forever and the only symptom is work that never happens.
                    | jails_model::ComponentKind::DurableJob
            )
        })
        .map(|component| component.id.as_str().to_string())
        .collect::<BTreeSet<_>>();
    // An outbox relay is a `@Scheduled` bean, so it fails the same silent way:
    // without the config the worker is never invoked, rows stage forever and
    // the only symptom is a topic that stays empty. It is not a component, so
    // it cannot come out of the filter above -- but it is an owner, and the
    // set is what stops removing one owner retiring a file the others need.
    owners.extend(
        crate::emit_operation::outbox::commands(model)
            .into_iter()
            .map(|operation| operation.id.as_str().to_string()),
    );
    if owners.is_empty() {
        return Ok(None);
    }
    let pkg = package(model, Package::Jobs);
    let artifact = "art_app_scheduling_config".to_string();
    let path = ProjectPath::parse(format!(
        "{}/{}/SchedulingConfig.java",
        super::MAIN_ROOT,
        pkg.replace('.', "/")
    ))
    .map_err(CompileError::new)?;
    Ok(Some(Emitted {
        path,
        file: RenderedFile {
            bytes: format!(
                "// Generated by jails from {artifact}. Clean hand edits survive regeneration.\n{}",
                SCHEDULING.resolve(templates)?.replace("{{pkg}}", &pkg)
            )
            .into_bytes(),
            kind: FileKind::JavaMain,
            mode: FileMode::Regular,
            provenance: Provenance {
                artifact_id: artifact,
                ejection_id: None,
                ejectable: true,
                semantic_ids: owners,
                compiler_pass: "components".to_string(),
            },
        },
    }))
}
