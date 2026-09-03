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
//! **One bean, one transaction.** The command *port* is the ABI and
//! `Jdbc<X>Command` is the one implementation of it, so staging goes in the
//! statement's own method under `@Transactional`. A second `Outbox<X>` bean
//! delegating to a storing one and staging afterwards would need `@Primary`
//! deciding which of two implementations Spring injects.
//!
//! **The event's identity is minted, never mapped.** Both the command and the
//! target usually carry an `id` of the same type, and taking the row's makes
//! the event id equal the resource id -- so the outbox's
//! `on conflict (id) do nothing` silently discards the *second* event about
//! that resource instead of deduplicating a retried stage. In the model that
//! identity is a `ParameterSource::Typed` component: a payload field the
//! target does not carry, which is exactly what this can supply and a direct
//! publication cannot.

use crate::Diagnostic;
use crate::emit_operation::Key;
use crate::recipe::{
    BootCondition, Import, JavaFile, Naming, Need, Placement, Recipe, SourceSet, Want,
};
use jails_contracts::{RenderedMigration, RenderedTree};
use jails_model::{
    AppModel, BuiltinType, Delivery, Operation, OperationKind, Package, ParameterSource, StableId,
    TypeRef,
};
use std::collections::BTreeSet;

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

pub(crate) fn emit(
    model: &AppModel,
    output: &mut RenderedTree,
    snapshot: &jails_contracts::WorkspaceSnapshot,
) -> Result<(), Diagnostic> {
    for operation in commands(model) {
        crate::recipe::render(model, operation, &OUTBOX, snapshot, output)?;
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
) -> Result<&'a Operation, Diagnostic> {
    let OperationKind::Command(command) = &operation.kind else {
        return Err(Diagnostic::new(
            "compile-outbox-policy-on-non-command",
            format!("$.operations.{}", operation.label),
            format!(
                "`deliver outbox` is a command policy; `{}` is not a command",
                operation.label
            ),
            "move the policy to the command that writes the row",
        ));
    };
    let [event_id] = command.semantics.emits.as_slice() else {
        return Err(Diagnostic::new(
            "compile-outbox-relays-many-events",
            format!("$.operations.{}", operation.label),
            format!(
                "canonical command `{}` delivers {} events through one outbox",
                operation.label,
                command.semantics.emits.len()
            ),
            "`deliver outbox` relays exactly one event -- emit one, or split the command",
        ));
    };
    let event = model.operations.get(event_id).ok_or_else(|| {
        Diagnostic::new(
            "compile-outbox-event-missing",
            format!("$.operations.{}", operation.label),
            format!(
                "canonical command `{}` emits missing event `{event_id}`",
                operation.label
            ),
            "declare the event, or remove the `emit`",
        )
    })?;
    let OperationKind::Event(payload) = &event.kind else {
        return Err(Diagnostic::new(
            "compile-outbox-emits-non-event",
            format!("$.operations.{}", operation.label),
            format!(
                "canonical command `{}` emits non-event operation `{}`",
                operation.label, event.label
            ),
            "`emit` names an event; declare one",
        ));
    };
    let identity = payload
        .semantics
        .parameters
        .iter()
        .find(|parameter| parameter.name == "id");
    match identity.map(|parameter| &parameter.source) {
        Some(ParameterSource::Typed(TypeRef::Builtin(BuiltinType::Uuid))) => Ok(event),
        Some(_) => Err(Diagnostic::new(
            "compile-outbox-event-id-projected",
            format!("$.operations.{}", event.label),
            format!(
                "outbox event `{}` projects its `id` from the target row",
                event.label
            ),
            "declare `id uuid` on the event so it is minted -- a staged event keyed on the resource id makes `on conflict (id) do nothing` discard the second event about that resource",
        )),
        None => Err(Diagnostic::new(
            "compile-outbox-event-without-id",
            format!("$.operations.{}", event.label),
            format!(
                "outbox event `{}` has no `id` component to stage it by",
                event.label
            ),
            "declare `id uuid` on the event",
        )),
    }
}

/// The outbox of one command, as a recipe over the command operation.
///
/// **The broker is a destination, not the relay's business.** The worker
/// walks the sink chain and knows nothing about Kafka; declaring the
/// capability is what puts a Kafka sink in that chain, and a project without
/// one keeps the logging sink and its WARN. The port is managed ABI: the
/// worker names it and every sink the reader writes implements it.
///
/// Naming the capability rather than the missing class in a refusal is
/// deliberate: the reader's fix is a declaration, and `Json.toJson` is a
/// symbol they never asked for.
const OUTBOX: Recipe<Operation> = Recipe {
    substitutions: &[],
    keys: &[
        Key::Usecase,
        Key::Property,
        Key::Table("_outbox"),
        Key::Event,
        Key::Publisher,
    ],
    fragments: &[],
    requires: &[
        Need {
            want: Want::Capability("db"),
            why: "delivers through an outbox, which needs a table to stage events in",
        },
        Need {
            want: Want::Capability("json"),
            why: "delivers through an outbox, which needs a payload encoder (`Json`)",
        },
    ],
    files: &[
        JavaFile {
            imports: &[
                Import::Keyed(Package::DomainEvents, Key::Event),
                Import::From(Package::Adapters, "Json"),
            ],
            ..staged(
                "outbox_store",
                crate::template!("spring/outbox_store_java.java"),
                Naming::Wrap("Jdbc", "Outbox"),
                true,
            )
        },
        staged(
            "outbox_sink",
            crate::template!("spring/outbox_sink_java.java"),
            Naming::Suffix("OutboxSink"),
            false,
        ),
        staged(
            "outbox_logging_sink",
            crate::template!("spring/outbox_logging_sink_java.java"),
            Naming::Suffix("LoggingOutboxSink"),
            true,
        ),
        JavaFile {
            imports: &[
                Import::Keyed(Package::DomainEvents, Key::Event),
                Import::Keyed(Package::Messaging, Key::Publisher),
            ],
            only_when: Some(has_broker),
            ..staged(
                "outbox_kafka_sink",
                crate::template!("spring/outbox_kafka_sink_java.java"),
                Naming::Suffix("KafkaOutboxSink"),
                true,
            )
        },
        JavaFile {
            imports: &[],
            ..staged(
                "outbox_worker",
                crate::template!("spring/outbox_worker_java.java"),
                Naming::Suffix("OutboxWorker"),
                true,
            )
        },
    ],
    files_when: BootCondition::Any,
    resources: &[],
    dependencies: &[],
    properties: &[],
    compose_services: &[],
    build_features: &[],
    default_package: jobs_package,
    pass: "outbox",
    minimum_boot: None,
};

/// A main-source file in the `jobs` layer that names the staged event's
/// record type.
const fn staged(
    role: &'static str,
    template: crate::Template,
    class: Naming<Operation>,
    ejectable: bool,
) -> JavaFile<Operation> {
    JavaFile {
        role,
        template,
        before_boot: None,
        imports: &[Import::Keyed(Package::DomainEvents, Key::Event)],
        only_when: None,
        source_set: SourceSet::Main,
        placement: Placement::Layer(Package::Jobs),
        ejectable,
        class,
        template_class: Naming::Fixed(""),
    }
}

fn jobs_package(model: &AppModel, _: &Operation) -> String {
    model.project.package_for(Package::Jobs)
}

fn has_broker(model: &AppModel, _: &Operation) -> bool {
    crate::recipe::declares(model, "kafka")
}

/// The table, and the partial index the relay claims through.
///
/// **The DDL is `templates/sql/outbox.sql`.**
/// It is one table and the store's statements are one file, so a second copy
/// drifts on exactly the column nobody re-reads -- a `select` naming one the
/// `create table` never had, found by `flyway migrate` in a project that was
/// working yesterday.
fn migration(table: &str) -> String {
    crate::template!("sql/outbox.sql")
        .built_in
        .replace("{{table}}", table)
}
