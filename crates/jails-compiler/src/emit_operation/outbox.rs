//! `deliver outbox`: an event that commits with the row it describes.
//!
//! **The promise this keeps, and why it is a policy rather than a default.**
//! Publishing directly is a write and a publish that can fail independently:
//! the row commits, the broker is down, and the event is simply gone. An
//! outbox writes the event into a table in the *same* transaction as the row,
//! and a relay publishes it afterwards -- so a committed row and an
//! unpublished event cannot disagree, at the cost of a table, a worker and
//! at-least-once delivery a consumer has to deduplicate.
//!
//! **The canonical shape is not the legacy one, and the difference is the
//! whole point.** `spring/outbox.rs` wraps the use case in a second
//! `Outbox<X>UseCase` bean that delegates to a storing one and stages
//! afterwards, because the legacy generator had already written a service it
//! could not reach inside. Here the command *port* is the ABI and
//! `Jdbc<X>Command` is the one implementation of it, so staging goes in the
//! statement's own method under `@Transactional`. One bean, one transaction,
//! and no `@Primary` deciding which of two implementations Spring injects.
//!
//! **The event's identity is minted, never mapped.** Both the command and the
//! target usually carry an `id` of the same type, and taking the row's made
//! the event id equal the resource id -- so the outbox's
//! `on conflict (id) do nothing` silently discarded the *second* event about
//! that resource instead of deduplicating a retried stage. In the model that
//! identity is a `ParameterSource::Typed` component: a payload field the
//! target does not carry, which is exactly what this can supply and a direct
//! publication cannot.

use crate::CompileError;
use jails_contracts::{
    FileKind, FileMode, ProjectPath, Provenance, RenderedFile, RenderedMigration, RenderedTree,
};
use jails_model::{
    AppModel, BuiltinType, Delivery, Operation, OperationKind, Package, ParameterSource, StableId,
    TypeRef,
};
use std::collections::BTreeSet;

const MAIN_ROOT: &str = ".jails/generated/main/java";

const STORE: &str = include_str!("../../../../templates/spring/outbox_store_java.java");
const SINK: &str = include_str!("../../../../templates/spring/outbox_sink_java.java");
const KAFKA_SINK: &str = include_str!("../../../../templates/spring/outbox_kafka_sink_java.java");
const LOGGING_SINK: &str =
    include_str!("../../../../templates/spring/outbox_logging_sink_java.java");
const WORKER: &str = include_str!("../../../../templates/spring/outbox_worker_java.java");

/// Every command in this model that delivers through an outbox.
pub(crate) fn commands(model: &AppModel) -> Vec<&Operation> {
    model
        .operations
        .values()
        .filter(|operation| delivery(operation) == Delivery::Outbox)
        .collect()
}

/// How one operation delivers what it emits. Only a command may say.
pub(crate) fn delivery(operation: &Operation) -> Delivery {
    match &operation.kind {
        OperationKind::Command(command) => command.semantics.delivery,
        _ => Delivery::Direct,
    }
}

/// Whether any outbox command mints an event identity.
///
/// Asked by [`crate::emit_java`], which otherwise emits `TimeOrderedUuid` only
/// for a `uuid7()` field default -- so a model whose sole minter is an event's
/// own id would name a class nothing wrote.
pub(crate) fn mints_identity(model: &AppModel) -> bool {
    commands(model).into_iter().any(|operation| {
        let OperationKind::Command(command) = &operation.kind else {
            return false;
        };
        command.semantics.emits.iter().any(|event_id| {
            model
                .operations
                .get(event_id)
                .and_then(|event| match &event.kind {
                    OperationKind::Event(payload) => Some(payload),
                    _ => None,
                })
                .is_some_and(|payload| {
                    payload.semantics.parameters.iter().any(|parameter| {
                        matches!(
                            &parameter.source,
                            ParameterSource::Typed(TypeRef::Builtin(BuiltinType::Uuid))
                        )
                    })
                })
        })
    })
}

pub(crate) fn lower_and_emit(
    model: &AppModel,
    output: &mut RenderedTree,
) -> Result<(), CompileError> {
    for operation in commands(model) {
        for (path, file) in files(model, operation)? {
            output.insert(path, file).map_err(CompileError::new)?;
        }
    }
    Ok(())
}

/// The outbox table for a command the accepted model does not already carry.
///
/// A migration is irreproducible: re-emitting it appends a second
/// `create table`, and the next `flyway migrate` is where that is found. So
/// the question is which commands are *new*, not which exist.
pub(crate) fn migrations(accepted: Option<&AppModel>, next: &AppModel) -> Vec<RenderedMigration> {
    commands(next)
        .into_iter()
        .filter(|operation| {
            accepted.is_none_or(|accepted| {
                accepted
                    .operations
                    .get(&operation.id)
                    .is_none_or(|before| delivery(before) != Delivery::Outbox)
            })
        })
        .map(|operation| {
            let table = table(operation);
            RenderedMigration {
                logical_name: format!("create_{table}"),
                bytes: migration(&table).into_bytes(),
                semantic_ids: BTreeSet::from([operation.id.as_str().to_string()]),
            }
        })
        .collect()
}

/// The staged-event table for one command, named off the stable label so a
/// renamed Java type does not strand the rows already in it.
pub(crate) fn table(operation: &Operation) -> String {
    format!("{}_outbox", operation.label)
}

/// The `outbox.<command>.*` property prefix the store and worker read.
pub(crate) fn property(operation: &Operation) -> String {
    operation.label.replace('_', "-")
}

/// The single event an outbox command relays, checked.
///
/// Three things have to hold before any of this can be rendered, and each one
/// is a compile error in the generated project rather than a runtime surprise:
/// exactly one event (the store is typed on one payload), a payload component
/// named `id` (the store stages by it), and that component minted rather than
/// projected (the deduplication above).
pub(crate) fn relayed<'a>(
    model: &'a AppModel,
    operation: &Operation,
) -> Result<&'a Operation, CompileError> {
    let OperationKind::Command(command) = &operation.kind else {
        return Err(CompileError::new(format!(
            "`deliver outbox` is a command policy; `{}` is not a command\n       fix: move the policy to the command that writes the row",
            operation.label
        )));
    };
    let [event_id] = command.semantics.emits.as_slice() else {
        return Err(CompileError::new(format!(
            "canonical command `{}` delivers {} events through one outbox\n       fix: `deliver outbox` relays exactly one event -- emit one, or split the command",
            operation.label,
            command.semantics.emits.len()
        )));
    };
    let event = model.operations.get(event_id).ok_or_else(|| {
        CompileError::new(format!(
            "canonical command `{}` emits missing event `{event_id}`\n       fix: declare the event, or remove the `emit`",
            operation.label
        ))
    })?;
    let OperationKind::Event(payload) = &event.kind else {
        return Err(CompileError::new(format!(
            "canonical command `{}` emits non-event operation `{}`\n       fix: `emit` names an event; declare one",
            operation.label, event.label
        )));
    };
    let identity = payload
        .semantics
        .parameters
        .iter()
        .find(|parameter| parameter.name == "id");
    match identity.map(|parameter| &parameter.source) {
        Some(ParameterSource::Typed(TypeRef::Builtin(BuiltinType::Uuid))) => Ok(event),
        Some(_) => Err(CompileError::new(format!(
            "outbox event `{}` projects its `id` from the target row\n       fix: declare `id uuid` on the event so it is minted -- a staged event keyed on the resource id makes `on conflict (id) do nothing` discard the second event about that resource",
            event.label
        ))),
        None => Err(CompileError::new(format!(
            "outbox event `{}` has no `id` component to stage it by\n       fix: declare `id uuid` on the event",
            event.label
        ))),
    }
}

fn files(
    model: &AppModel,
    operation: &Operation,
) -> Result<Vec<(ProjectPath, RenderedFile)>, CompileError> {
    let event = relayed(model, operation)?;
    require(model, "db", operation, "a table to stage events in")?;
    require(model, "json", operation, "a payload encoder (`Json`)")?;

    let usecase = &operation.names.java_type;
    let event_type = crate::emit_java::with_suffix(&event.names.java_type, "Event");
    let jobs = model.project.package_for(Package::Jobs);
    let events = model.project.package_for(Package::DomainEvents);
    let adapters = model.project.package_for(Package::Adapters);
    let event_import = import(&jobs, &events, &event_type);
    let table = table(operation);
    let property = property(operation);

    let store = STORE
        .replace("{{pkg}}", &jobs)
        .replace("{{json_import}}", &import(&jobs, &adapters, "Json"))
        .replace("{{event_import}}", &event_import)
        .replace("{{usecase}}", usecase)
        .replace("{{property}}", &property)
        .replace("{{event}}Event", &event_type)
        .replace("{{table}}", &table);
    let sink = SINK
        .replace("{{pkg}}", &jobs)
        .replace("{{event_import}}", &event_import)
        .replace("{{usecase}}", usecase)
        .replace("{{event}}Event", &event_type);
    let logging = LOGGING_SINK
        .replace("{{pkg}}", &jobs)
        .replace("{{event_import}}", &event_import)
        .replace("{{usecase}}", usecase)
        .replace("{{event}}Event", &event_type);
    let worker = WORKER
        .replace("{{pkg}}", &jobs)
        .replace("{{usecase}}", usecase)
        .replace("{{property}}", &property);

    // **The broker is a destination, not the relay's business.** The worker
    // walks the sink chain and knows nothing about Kafka; declaring the
    // capability is what puts a Kafka sink in that chain, and a project
    // without one keeps the logging sink and its WARN.
    let mut files = Vec::new();
    if model
        .capabilities
        .values()
        .any(|capability| capability.kind == "kafka")
    {
        let messaging = model.project.package_for(Package::Messaging);
        let publisher = format!("{}Publisher", event.names.java_type);
        let kafka = KAFKA_SINK
            .replace("{{pkg}}", &jobs)
            .replace("{{event_import}}", &event_import)
            .replace(
                "{{publisher_import}}",
                &import(&jobs, &messaging, &publisher),
            )
            .replace("{{usecase}}", usecase)
            .replace("{{event}}Publisher", &publisher)
            .replace("{{event}}Event", &event_type);
        files.push(rendered(
            operation,
            "outbox_kafka_sink",
            &jobs,
            &format!("{usecase}KafkaOutboxSink"),
            kafka,
            true,
        )?);
    }

    files.extend([
        rendered(
            operation,
            "outbox_store",
            &jobs,
            &format!("Jdbc{usecase}Outbox"),
            store,
            true,
        )?,
        // The port is managed ABI: the worker names it and every sink the
        // reader writes implements it.
        rendered(
            operation,
            "outbox_sink",
            &jobs,
            &format!("{usecase}OutboxSink"),
            sink,
            false,
        )?,
        rendered(
            operation,
            "outbox_logging_sink",
            &jobs,
            &format!("{usecase}LoggingOutboxSink"),
            logging,
            true,
        )?,
        rendered(
            operation,
            "outbox_worker",
            &jobs,
            &format!("{usecase}OutboxWorker"),
            worker,
            true,
        )?,
    ]);
    Ok(files)
}

/// Refuse when a capability the rendered Java names is not in the model.
///
/// Naming the capability rather than the missing class is deliberate: the
/// reader's fix is a declaration, and `Json.toJson` is a symbol they never
/// asked for.
fn require(
    model: &AppModel,
    kind: &str,
    operation: &Operation,
    why: &str,
) -> Result<(), CompileError> {
    if model
        .capabilities
        .values()
        .any(|capability| capability.kind == kind)
    {
        return Ok(());
    }
    Err(CompileError::new(format!(
        "canonical command `{}` delivers through an outbox, which needs {why}\n       fix: declare `cap {kind}` in the model",
        operation.label
    )))
}

/// One import line, or nothing when the two packages are the same.
fn import(user: &str, owner: &str, class: &str) -> String {
    if user == owner {
        String::new()
    } else {
        format!("import {owner}.{class};\n")
    }
}

fn rendered(
    operation: &Operation,
    suffix: &str,
    package: &str,
    type_name: &str,
    body: String,
    ejectable: bool,
) -> Result<(ProjectPath, RenderedFile), CompileError> {
    let artifact = format!("art_{}_{}", operation.id.as_str(), suffix);
    let path = ProjectPath::parse(format!(
        "{MAIN_ROOT}/{}/{type_name}.java",
        package.replace('.', "/")
    ))
    .map_err(CompileError::new)?;
    Ok((
        path,
        RenderedFile {
            bytes: format!(
                "// Generated by jails from {artifact}. Clean hand edits survive regeneration.\n{body}"
            )
            .into_bytes(),
            kind: FileKind::JavaMain,
            mode: FileMode::Regular,
            provenance: Provenance {
                artifact_id: artifact,
                ejection_id: None,
                ejectable,
                semantic_ids: BTreeSet::from([operation.id.as_str().to_string()]),
                compiler_pass: "outbox".to_string(),
            },
        },
    ))
}

/// The table, and the partial index the relay claims through.
///
/// **The DDL is `templates/sql/outbox.sql`, shared with the legacy engine.**
/// It is one table and the store's statements are one file, so a second copy
/// drifts on exactly the column nobody re-reads -- a `select` naming one the
/// `create table` never had, found by `flyway migrate` in a project that was
/// working yesterday.
fn migration(table: &str) -> String {
    include_str!("../../../../templates/sql/outbox.sql").replace("{{table}}", table)
}
