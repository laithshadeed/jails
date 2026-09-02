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
//! changes the queue's payload with it. A separate `<Name>Work` record would
//! be a second declaration of one thing, needing a drift check that exists
//! only because there are two.
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

const QUEUE: crate::Template = crate::template!("spring/durable_queue_port_java.java");
const STORE: crate::Template = crate::template!("spring/durable_store_jdbc_java.java");
const WORKER: crate::Template = crate::template!("spring/durable_worker_canonical_java.java");
const CONTROLLER: crate::Template =
    crate::template!("spring/durable_controller_canonical_java.java");
const TEST: crate::Template = crate::template!("spring/durable_job_canonical_it_java.java");
const MIGRATION: crate::Template = crate::template!("sql/durable_job.sql");

pub(super) fn files(
    model: &AppModel,
    component: &Component,
    templates: &jails_contracts::TemplateOverrides,
) -> Result<Vec<Emitted>, CompileError> {
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
    let (arguments, conflict, sample_imports) = sample(model, command)?;
    let sample_imports = sample_imports
        .iter()
        .filter(|import| *import != "java.util.UUID")
        .map(|import| format!("import {import};\n"))
        .collect::<String>();

    let queue = QUEUE
        .resolve(templates)?
        .replace("{{pkg}}", &jobs)
        .replace("{{input_import}}", &input(&jobs))
        .replace("{{name}}", name)
        .replace("{{usecase}}Command", &port);
    let store = STORE
        .resolve(templates)?
        .replace("{{pkg}}", &jobs)
        .replace("{{json_import}}", &import(&jobs, &adapters, "Json"))
        .replace("{{input_import}}", &input(&jobs))
        .replace("{{name}}", name)
        .replace("{{usecase}}Command", &port)
        .replace("{{property}}", &property)
        .replace("{{table}}", &table);
    // **The tenancy the enqueue proved, replayed from the payload it stored.**
    // A scoped command reads its claims from an `ExecutionContext` the request
    // boundary built after `ScopeAuthorizer` proved them; a worker running out
    // of band has nobody to prove anything, but the values it needs were
    // proven when the work was enqueued and are in the row. Rebuilding the
    // context from them is what replaying the command means -- calling it
    // without one does not compile.
    let (context, context_import) = worker_context(model, command, target)?;
    let worker = WORKER
        .resolve(templates)?
        .replace("{{pkg}}", &jobs)
        .replace("{{input_import}}", &input(&jobs))
        .replace("{{repository_import}}", &repository_import(&jobs))
        .replace("{{context_import}}", &context_import)
        .replace("{{context}}", &context)
        .replace("{{name}}", name)
        .replace("{{usecase}}Command", &port)
        .replace("{{target}}", &target.names.java_type)
        .replace("{{property}}", &property);
    let controller = CONTROLLER
        .resolve(templates)?
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
        .resolve(templates)?
        .replace("{{pkg}}", &jobs)
        .replace("{{input_import}}", &input(&jobs))
        .replace("{{repository_import}}", &repository_import(&jobs))
        .replace("{{sample_imports}}", &sample_imports)
        // Before `{{name}}` and `{{usecase}}Command`: the conflict block
        // carries both, and substituting them first would leave a rendered
        // test naming `{{usecase}}Command` literally.
        .replace("{{conflict_test}}", &conflict)
        .replace("{{args}}", &arguments)
        .replace("{{table}}", &table)
        .replace("{{results_table}}", &target.names.sql_table)
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
                bytes: MIGRATION.built_in.replace("{{table}}", &table).into_bytes(),
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
fn sample(
    model: &AppModel,
    command: &Operation,
) -> Result<(String, String, BTreeSet<String>), CompileError> {
    let OperationKind::Command(spec) = &command.kind else {
        unreachable!("`queued` has already checked the kind");
    };
    let mut arguments = Vec::new();
    let mut alternates = Vec::new();
    // **What the sample expressions name.** `URI.create(...)` and
    // `Instant.parse(...)` are Java types, and this template's import list is
    // otherwise fixed -- so without these a payload carrying a `uri` compiles
    // everywhere except here, where the symbol is undefined.
    let mut imports = BTreeSet::new();
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
                imports.extend(builtin.semantics().java_import.map(str::to_string));
                arguments.push(builtin.semantics().sample.to_string());
                alternates.push(builtin.semantics().alternate.map(str::to_string));
            }
            // **A declared enum is one jails can spell**, and it is the case
            // this exists for: `PaymentMethod` is a component of the very
            // command the job replays, so refusing it would refuse the job
            // outright. Fully qualified rather than imported -- the sample
            // lands inside a template whose import list is fixed, and a
            // constant reference needs no import to compile.
            TypeRef::External(name) => {
                let constants = enum_constants(model, &name).ok_or_else(|| {
                    CompileError::new(format!(
                        "durable job cannot enqueue `{}`: its input carries `{name}`, which jails cannot serialize or sample\n       fix: use builtin-typed or enum command inputs, or write the queue by hand",
                        command.label
                    ))
                })?;
                let domain = model.project.package_for(Package::Domain);
                let qualified = |constant: &str| format!("{domain}.{name}.{constant}");
                let Some(first) = constants.first() else {
                    return Err(CompileError::new(format!(
                        "durable job cannot enqueue `{}`: its input carries `{name}`, an enum with no constants\n       fix: declare at least one constant",
                        command.label
                    )));
                };
                arguments.push(qualified(first));
                // The *different* payload the idempotency-conflict test needs.
                // `None` where the enum has one constant, so the test says why
                // rather than inventing a value.
                alternates.push(constants.get(1).map(|second| qualified(second)));
            }
        }
    }
    let conflict = conflict_test(&arguments, &alternates);
    Ok((arguments.join(", "), conflict, imports))
}

/// The `ExecutionContext` argument a scoped command needs, and its import.
///
/// Empty when the target carries no `@scope` field, which is the ordinary case
/// and the only one this can emit.
///
/// **A scoped command is refused rather than replayed**, and that is a
/// decision rather than a gap. `@scope` means the value is proved at the
/// request boundary by `ScopeAuthorizer` and read from the context, never from
/// the caller's input -- so the tenancy is exactly what the queue row does not
/// hold. A worker that manufactured a context from the payload would be
/// asserting a claim nobody proved, which is the privilege escalation the
/// marker exists to prevent; one that stored the proven claims would be a
/// different queue, with a column and a contract this has not been given.
/// Saying so is the honest answer until somebody designs that queue.
fn worker_context(
    model: &AppModel,
    command: &Operation,
    target: &jails_model::Entity,
) -> Result<(String, String), CompileError> {
    let scoped = target
        .fields
        .iter()
        .filter(|field| field.semantics.scope.is_some())
        .collect::<Vec<_>>();
    if scoped.is_empty() {
        return Ok((String::new(), String::new()));
    }
    let _ = model;
    Err(CompileError::new(format!(
        "durable job cannot replay `{}`: `{}` is scoped by `{}`, and a worker running out of band has nobody to prove a claim to\n       fix: enqueue an unscoped command, or write the worker by hand and decide there how the proven tenancy is carried",
        command.label, target.label, scoped[0].label
    )))
}

/// The constants of a declared enum, or `None` when this names something else.
fn enum_constants(model: &AppModel, java_type: &str) -> Option<Vec<String>> {
    let entity = model
        .entities
        .values()
        .find(|entity| entity.active && entity.names.java_type == java_type)?;
    entity.facets.contains(&jails_model::Facet::Enum).then(|| {
        entity
            .enum_constants
            .iter()
            .map(|constant| constant.java_name.clone())
            .collect()
    })
}

/// The idempotency test, or a note saying why there is none.
fn conflict_test(arguments: &[String], alternates: &[Option<String>]) -> String {
    let Some(index) = alternates.iter().position(Option::is_some) else {
        return "    // No idempotency-conflict test: this command's input has no\n    // component jails can vary, so a second, *different* payload is not\n    // something it can construct. The store still refuses one.\n"
            .to_string();
    };
    let mut other = arguments.to_vec();
    other[index] = alternates[index]
        .clone()
        .expect("the position above found a Some");
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
