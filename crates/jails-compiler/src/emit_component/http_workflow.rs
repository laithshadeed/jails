//! `component http-workflow <Name> { on <fetcher> }`: a durable graph
//! traversal.
//!
//! **Everything about it is in PostgreSQL, and that is the whole design.** A
//! traversal that keeps its frontier in memory loses the run when the process
//! dies, and re-runs it from the seed when the process comes back -- so the
//! frontier is a table with expiring leases, the URL is the primary key, the
//! retry count is a column, and a cancellation is a persisted flag rather than
//! a thread interrupt. A restart resumes; it does not restart.
//!
//! **It fetches through the `fetcher` component rather than a client of its
//! own**, and that is not tidiness. Traversal follows links a *remote page*
//! supplied, so every URL after the seed is attacker-chosen: the one outbound
//! call that can be aimed at the host it runs on. `fetcher` is the port whose
//! whole contract is refusing that, and pointing the workflow at anything else
//! would put the bound back in the hands of whoever wrote the page.
//!
//! Four artifacts and no more: the workflow, an HTTP control plane to start
//! and cancel runs, an integration test that drives a real traversal, and the
//! three tables. The scheduling config is shared, so it comes from
//! [`super::job::scheduling`] like every other scheduled bean's.

use super::{Emitted, Package, java, package};
use crate::CompileError;
use jails_contracts::RenderedMigration;
use jails_model::{AppModel, Component, ComponentKind, ComponentReference, StableId};
use std::collections::BTreeSet;

/// Micrometer for the traversal counters. The JDBC half comes from `storage`.
pub(super) const DEPENDENCIES: &[(&str, &str)] =
    &[("org.springframework.boot", "spring-boot-starter-actuator")];

const WORKFLOW: &str = include_str!("../../../../templates/spring/http_workflow_java.java");
const CONTROLLER: &str =
    include_str!("../../../../templates/spring/http_workflow_controller_java.java");
const TEST: &str = include_str!("../../../../templates/spring/http_workflow_it_java.java");
const MIGRATION: &str = include_str!("../../../../templates/sql/http_workflow.sql");

pub(super) fn files(model: &AppModel, component: &Component) -> Result<Vec<Emitted>, CompileError> {
    let fetcher = fetcher(model, component)?;
    if !super::has_database(model) {
        return Err(CompileError::new(format!(
            "http workflow `{}` keeps its whole frontier in PostgreSQL\n       fix: declare `storage postgres` in the model",
            component.label
        )));
    }

    let name = &component.name;
    let jobs = package(model, Package::Jobs);
    let web = package(model, Package::Web);
    let clients = package(model, Package::Clients);
    let table = table(component);
    let property = component.label.replace('_', "-");
    let substitute = |template: &str| {
        template
            .replace("{{pkg}}", &jobs)
            .replace("{{web}}", &web)
            .replace("{{clients}}", &clients)
            .replace("{{name}}", name)
            .replace("{{fetcher}}", &fetcher.name)
            .replace("{{table}}", &table)
            .replace("{{property}}", &property)
    };
    Ok(vec![
        java(
            component,
            "workflow",
            &jobs,
            &format!("{name}Workflow"),
            false,
            true,
            substitute(WORKFLOW),
        )?,
        java(
            component,
            "controller",
            &web,
            &format!("{name}WorkflowController"),
            false,
            true,
            substitute(CONTROLLER),
        )?,
        java(
            component,
            "test",
            &jobs,
            &format!("{name}WorkflowIT"),
            true,
            true,
            substitute(TEST),
        )?,
    ])
}

/// The three tables a workflow this model does not already have.
pub(super) fn migrations(accepted: Option<&AppModel>, next: &AppModel) -> Vec<RenderedMigration> {
    next.components
        .values()
        .filter(|component| component.kind == ComponentKind::HttpWorkflow)
        .filter(|component| {
            accepted.is_none_or(|accepted| !accepted.components.contains_key(&component.id))
        })
        .map(|component| {
            let table = table(component);
            RenderedMigration {
                logical_name: format!("create_{table}_workflow"),
                bytes: MIGRATION.replace("{{table}}", &table).into_bytes(),
                semantic_ids: BTreeSet::from([component.id.as_str().to_string()]),
            }
        })
        .collect()
}

/// The table prefix, from the stable label so a renamed Java type leaves the
/// rows of a running traversal where they are.
fn table(component: &Component) -> String {
    component.label.clone()
}

/// The bounded fetcher this workflow reaches the network through.
///
/// Refusing anything else is the security property, not a type check: every
/// URL after the seed came off a page somebody else wrote, and `fetcher` is
/// the component whose whole contract is refusing to follow one to a private
/// address.
fn fetcher<'a>(model: &'a AppModel, component: &Component) -> Result<&'a Component, CompileError> {
    let Some(ComponentReference::Component(id)) = component.on.as_ref() else {
        return Err(CompileError::new(format!(
            "http workflow `{}` has nothing to fetch through\n       fix: point `on` at a `fetcher` component",
            component.label
        )));
    };
    let fetcher = model.components.get(id).ok_or_else(|| {
        CompileError::new(format!(
            "http workflow `{}` references missing component `{id}`\n       fix: declare the fetcher it traverses through",
            component.label
        ))
    })?;
    if fetcher.kind != ComponentKind::Fetcher {
        return Err(CompileError::new(format!(
            "http workflow `{}` traverses through `{}`, which is a {} rather than a fetcher\n       fix: a traversal follows links a remote page supplied, so it must go through the port that bounds them",
            component.label,
            fetcher.label,
            fetcher.kind.label()
        )));
    }
    Ok(fetcher)
}
