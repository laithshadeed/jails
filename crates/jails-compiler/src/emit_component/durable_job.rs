//! `component durable-job <Name> { on <command> yields <entity> }`: work that
//! survives the process that accepted it.
//!
//! **The pair with `job` is the whole design.** A `job` is a `@Scheduled`
//! method: fine until the process dies mid-item, at which point the work is
//! simply gone and nothing reports it. A durable job puts the queue in a
//! table, so what expires is a lease rather than the work, and the next
//! process picks the item up.
//!
//! **It executes an existing command rather than a body of its own.** The
//! payload is that command's own `Input` record, so a queued item cannot
//! describe work the command could not do, and adding a field to the command
//! changes the queue's payload with it. The legacy generator instead wrote a
//! `<Name>Work` record whose fields had to *exactly* match the command's, in
//! declaration order, and refused when they drifted -- a check that exists
//! only because there were two declarations of one thing.
//!
//! **The recovery check is the subtle part.** A process can die after the
//! command's transaction commits and before the queue row is acknowledged, and
//! the expired lease then hands the same item to the next worker. The shared
//! id is the proof that the effect already happened: without it, at-least-once
//! *delivery* becomes at-least-once *effect*, which for a command that creates
//! a row is a duplicate on every restart. That is why `yields` names the
//! entity -- the worker asks its repository whether the row is already there.

use super::{Emitted, Package, java, package};
use crate::CompileError;
use jails_contracts::RenderedMigration;
use jails_model::{
    AppModel, Component, ComponentKind, ComponentReference, Entity, Facet, Operation,
    OperationKind, ParameterSource, StableId, TypeRef,
};
use std::collections::BTreeSet;

const QUEUE: &str = include_str!("../../../../templates/spring/durable_queue_port_java.java");
const STORE: &str = include_str!("../../../../templates/spring/durable_store_jdbc_java.java");
const WORKER: &str =
    include_str!("../../../../templates/spring/durable_worker_canonical_java.java");
const CONTROLLER: &str =
    include_str!("../../../../templates/spring/durable_controller_canonical_java.java");
const TEST: &str = include_str!("../../../../templates/spring/durable_job_canonical_it_java.java");
const MIGRATION: &str = include_str!("../../../../templates/sql/durable_job.sql");

pub(super) fn files(model: &AppModel, component: &Component) -> Result<Vec<Emitted>, CompileError> {
    let command = queued(model, component)?;
    let target = produced(model, component)?;
    if !super::has_database(model) {
        return Err(CompileError::new(format!(
            "durable job `{}` keeps its queue in PostgreSQL\n       fix: declare `storage postgres` in the model",
            component.label
        )));
    }
    if !model
        .capabilities
        .values()
        .any(|capability| capability.kind == "json")
    {
        return Err(CompileError::new(format!(
            "durable job `{}` stores its payload with `Json`\n       fix: declare `cap json` in the model",
            component.label
        )));
    }

    let name = &component.name;
    // The port type, whole. `{{usecase}}Command` is substituted rather than
    // `{{usecase}}`, so a command already named `…Command` is not doubled --
    // the same rule `with_suffix` exists for everywhere else.
    let port = crate::emit_java::with_suffix(&command.names.java_type, "Command");
    let jobs = package(model, Package::Jobs);
    let web = package(model, Package::Web);
    let commands = package(model, Package::ApplicationCommands);
    let repository = package(model, Package::Repository);
    let adapters = package(model, Package::Adapters);
    let table = table(component);
    let property = component.label.replace('_', "-");
    let path = component
        .route
        .as_ref()
        .map_or_else(|| format!("/jobs/{property}"), |route| route.path.clone());
    let input = |user: &str| import(user, &commands, &port);
    let repository_import = |user: &str| {
        import(
            user,
            &repository,
            &format!("{}Repository", target.names.java_type),
        )
    };
    let (arguments, conflict) = sample(model, command)?;

    let queue = QUEUE
        .replace("{{pkg}}", &jobs)
        .replace("{{input_import}}", &input(&jobs))
        .replace("{{name}}", name)
        .replace("{{usecase}}Command", &port);
    let store = STORE
        .replace("{{pkg}}", &jobs)
        .replace("{{json_import}}", &import(&jobs, &adapters, "Json"))
        .replace("{{input_import}}", &input(&jobs))
        .replace("{{name}}", name)
        .replace("{{usecase}}Command", &port)
        .replace("{{property}}", &property)
        .replace("{{table}}", &table);
    let worker = WORKER
        .replace("{{pkg}}", &jobs)
        .replace("{{input_import}}", &input(&jobs))
        .replace("{{repository_import}}", &repository_import(&jobs))
        .replace("{{name}}", name)
        .replace("{{usecase}}Command", &port)
        .replace("{{target}}", &target.names.java_type)
        .replace("{{property}}", &property);
    let controller = CONTROLLER
        .replace("{{web}}", &web)
        .replace(
            "{{queue_import}}",
            &import(&web, &jobs, &format!("{name}Queue")),
        )
        .replace("{{input_import}}", &input(&web))
        .replace("{{name}}", name)
        .replace("{{usecase}}Command", &port)
        .replace("{{path}}", &path);
    let test = TEST
        .replace("{{pkg}}", &jobs)
        .replace("{{input_import}}", &input(&jobs))
        .replace("{{repository_import}}", &repository_import(&jobs))
        // Before `{{name}}` and `{{usecase}}Command`: the conflict block
        // carries both, and substituting them first would leave a rendered
        // test naming `{{usecase}}Command` literally.
        .replace("{{conflict_test}}", &conflict)
        .replace("{{args}}", &arguments)
        .replace("{{name}}", name)
        .replace("{{target}}", &target.names.java_type)
        .replace("{{usecase}}Command", &port);

    Ok(vec![
        // The queue is managed ABI: the store implements it, the worker drains
        // it, and anything that wants work done later names it.
        java(
            component,
            "queue",
            &jobs,
            &format!("{name}Queue"),
            false,
            false,
            queue,
        )?,
        java(
            component,
            "store",
            &jobs,
            &format!("Jdbc{name}Store"),
            false,
            true,
            store,
        )?,
        java(
            component,
            "worker",
            &jobs,
            &format!("{name}Worker"),
            false,
            true,
            worker,
        )?,
        java(
            component,
            "controller",
            &web,
            &format!("{name}JobController"),
            false,
            true,
            controller,
        )?,
        java(
            component,
            "test",
            &jobs,
            &format!("{name}JobIT"),
            true,
            true,
            test,
        )?,
    ])
}

/// The queue table for a durable job this model does not already have.
pub(super) fn migrations(accepted: Option<&AppModel>, next: &AppModel) -> Vec<RenderedMigration> {
    next.components
        .values()
        .filter(|component| component.kind == ComponentKind::DurableJob)
        .filter(|component| {
            accepted.is_none_or(|accepted| !accepted.components.contains_key(&component.id))
        })
        .map(|component| {
            let table = table(component);
            RenderedMigration {
                logical_name: format!("create_{table}"),
                bytes: MIGRATION.replace("{{table}}", &table).into_bytes(),
                semantic_ids: BTreeSet::from([component.id.as_str().to_string()]),
            }
        })
        .collect()
}

/// The queue table, from the stable label so a renamed Java type leaves the
/// items already waiting in it where they are.
fn table(component: &Component) -> String {
    format!("{}_jobs", component.label)
}

/// The command this job runs later.
fn queued<'a>(model: &'a AppModel, component: &Component) -> Result<&'a Operation, CompileError> {
    let Some(ComponentReference::Operation(id)) = component.on.as_ref() else {
        return Err(CompileError::new(format!(
            "durable job `{}` has no work to do\n       fix: point `on` at the command it runs",
            component.label
        )));
    };
    let command = model.operations.get(id).ok_or_else(|| {
        CompileError::new(format!(
            "durable job `{}` references missing operation `{id}`\n       fix: declare the command it runs",
            component.label
        ))
    })?;
    if !matches!(command.kind, OperationKind::Command(_)) {
        return Err(CompileError::new(format!(
            "durable job `{}` queues `{}`, which is not a command\n       fix: a queued item is work with an effect; point `on` at a command",
            component.label, command.label
        )));
    }
    Ok(command)
}

/// The entity the queued command creates, and the reason it must be named.
///
/// The worker asks this entity's repository whether the row is already there
/// before executing, which is what keeps at-least-once *delivery* from
/// becoming at-least-once *effect* across a process that died between the
/// command's commit and the queue's acknowledgement.
fn produced<'a>(model: &'a AppModel, component: &Component) -> Result<&'a Entity, CompileError> {
    let Some(ComponentReference::Entity(id)) = component.yields.as_ref() else {
        return Err(CompileError::new(format!(
            "durable job `{}` has no way to tell a retry from a repeat\n       fix: point `yields` at the entity its command creates",
            component.label
        )));
    };
    let entity = model.entities.get(id).ok_or_else(|| {
        CompileError::new(format!(
            "durable job `{}` references missing entity `{id}`",
            component.label
        ))
    })?;
    if !entity.active || !entity.facets.contains(&Facet::Repository) {
        return Err(CompileError::new(format!(
            "durable job `{}` proves its recovery through `{}`, which has no repository\n       fix: add `use repo` to that entity -- the worker asks it whether the effect already happened",
            component.label, entity.label
        )));
    }
    Ok(entity)
}

/// One `Input(...)` argument list for the generated integration test, and the
/// conflict test when a second, different payload can be built.
///
/// The conflict test needs a payload that *differs*, and with no component
/// there is nothing to vary -- so rather than assert a conflict that cannot
/// happen, it is omitted with the reason in its place. A command with no input
/// is unusual but legal.
fn sample(model: &AppModel, command: &Operation) -> Result<(String, String), CompileError> {
    let OperationKind::Command(spec) = &command.kind else {
        unreachable!("`queued` has already checked the kind");
    };
    let mut arguments = Vec::new();
    let mut alternates = Vec::new();
    for parameter in &spec.semantics.parameters {
        if !parameter.required || parameter.optional_filter {
            arguments.push("java.util.Optional.empty()".to_string());
            alternates.push(None);
            continue;
        }
        let ty = match &parameter.source {
            ParameterSource::Typed(ty) => ty.clone(),
            ParameterSource::Field(visible) => {
                let owner = crate::emit_java::entity(model, &visible.entity)?;
                owner
                    .field(&visible.field)
                    .ok_or_else(|| {
                        CompileError::new(format!(
                            "durable job queues `{}`, which references missing field `{}`",
                            command.label, visible.field
                        ))
                    })?
                    .ty
                    .clone()
            }
        };
        match ty {
            TypeRef::Builtin(builtin) => {
                arguments.push(builtin.semantics().sample.to_string());
                alternates.push(builtin.semantics().alternate);
            }
            TypeRef::External(name) => {
                return Err(CompileError::new(format!(
                    "durable job cannot enqueue `{}`: its input carries `{name}`, which jails cannot serialize or sample\n       fix: use builtin-typed command inputs, or write the queue by hand",
                    command.label
                )));
            }
        }
    }
    let conflict = conflict_test(&arguments, &alternates);
    Ok((arguments.join(", "), conflict))
}

/// The idempotency test, or a note saying why there is none.
fn conflict_test(arguments: &[String], alternates: &[Option<&'static str>]) -> String {
    let Some(index) = alternates.iter().position(Option::is_some) else {
        return "    // No idempotency-conflict test: this command's input has no\n    // component jails can vary, so a second, *different* payload is not\n    // something it can construct. The store still refuses one.\n"
            .to_string();
    };
    let mut other = arguments.to_vec();
    other[index] = alternates[index]
        .expect("the position above found a Some")
        .to_string();
    format!(
        "    /**\n     * The same id twice is the same request; the same id with different work\n     * is a mistake, and reporting it is what makes the first case safe.\n     */\n    @Test\n    void reusingAnIdRequiresTheSamePayload() {{\n        var id = UUID.randomUUID();\n        store.enqueue(id, sample());\n        store.enqueue(id, sample());\n        assertThat(store.status(id).orElseThrow().attempts()).isZero();\n\n        assertThatThrownBy(() -> store.enqueue(id, new {{{{usecase}}}}Command.Input({})))\n                .isInstanceOf({{{{name}}}}Queue.IdempotencyConflictException.class);\n    }}\n",
        other.join(", ")
    )
}

/// One import line, or nothing when the two packages are the same.
fn import(user: &str, owner: &str, class: &str) -> String {
    if user == owner {
        String::new()
    } else {
        format!("import {owner}.{class};\n")
    }
}
