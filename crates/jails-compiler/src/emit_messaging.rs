//! The Kafka slice a declared event gets, when the project declares a broker.
//!
//! **This is the payload half.** `cap kafka` owns everything topic-agnostic -- the
//! `DefaultErrorHandler`, the dead-letter routing, the
//! `ErrorHandlingDeserializer` -- because a capability cannot know a topic
//! name and must not guess one. An `event` declaration is where the name and
//! the payload type come from, so the publisher, the listener and the proof
//! that a published record comes back live here.
//!
//! Nothing is emitted for a project that has not declared the capability: an
//! `event` is a domain fact first, and a project with no broker still wants
//! the record and the `publishEvent` the operation already makes.
//!
//! The slice is one [`Recipe`] over the event operation. What is structural
//! -- the partition key and the Javadoc that tells the truth about it, the
//! sample argument list the proofs construct the payload with -- is a
//! [`Fragment`] named on the row.

use crate::CompileError;
use crate::emit_operation::Key;
use crate::recipe::{
    BootCondition, Fragment, Import, JavaFile, Naming, Placement, Recipe, Rendered, SourceSet,
};
use jails_contracts::RenderedTree;
use jails_model::{AppModel, Operation, OperationKind, Package};

pub(crate) fn emit(
    model: &AppModel,
    output: &mut RenderedTree,
    snapshot: &jails_contracts::WorkspaceSnapshot,
) -> Result<(), CompileError> {
    if !crate::recipe::declares(model, "kafka") {
        return Ok(());
    }
    for operation in model.operations.values() {
        if !matches!(operation.kind, OperationKind::Event(_)) {
            continue;
        }
        crate::recipe::render(model, operation, &EVENT, snapshot, output)?;
    }
    Ok(())
}

/// A main-source file in the `messaging` layer.
const fn main(
    role: &'static str,
    template: crate::Template,
    class: Naming<Operation>,
    ejectable: bool,
) -> JavaFile<Operation> {
    JavaFile {
        role,
        template,
        before_boot: None,
        // Every file here names the payload record, and gets the import
        // unless it is already in this package.
        imports: &[Import::Keyed(Package::DomainEvents, Key::Event)],
        only_when: None,
        source_set: SourceSet::Main,
        placement: Placement::Layer(Package::Messaging),
        ejectable,
        class,
        template_class: Naming::Fixed(""),
    }
}

/// **The handler port is the reaction's home, and it lives beside the
/// listener rather than in `ports.events`.** That package is the *outbound*
/// publish port an entity's `events` facet emits; an inbound handler is the
/// other direction, and filing both under one name would make `TaskEvents`
/// and `ClosedHandler` look like halves of one interface.
///
/// The port is managed ABI: the listener's constructor names it, so ejecting
/// it would leave the compiler emitting a class against a type it no longer
/// controls. The implementations the reader writes are theirs already --
/// jails never generates one.
const EVENT: Recipe<Operation> = Recipe {
    substitutions: &[("KAFKA_TESTCONTAINERS_CONFIG", "KafkaTestcontainersConfig")],
    keys: &[Key::Topic, Key::Event],
    fragments: &[
        Fragment::Rendered {
            key: "ordering",
            render: ordering,
        },
        Fragment::Rendered {
            key: "send",
            render: send,
        },
        Fragment::Rendered {
            key: "event_args",
            render: event_args,
        },
        Fragment::Rendered {
            key: "disabled",
            render: disabled,
        },
    ],
    requires: &[],
    files: &[
        main(
            "publisher",
            crate::template!("spring/publisher_java.java"),
            Naming::Suffix("Publisher"),
            true,
        ),
        main(
            "handler",
            crate::template!("spring/event_handler_java.java"),
            Naming::Suffix("Handler"),
            false,
        ),
        main(
            "listener",
            crate::template!("spring/listener_java.java"),
            Naming::Suffix("Listener"),
            true,
        ),
        // **The listener test needs no broker and no `id`.** The proof below
        // waits for a record to come back and matches it by id, so it is
        // emitted only for a payload that has one; delegation to the port is
        // a property of an ordinary object, so it is proved for every event.
        JavaFile {
            source_set: SourceSet::Test,
            ..main(
                "listener_test",
                crate::template!("spring/listener_test_java.java"),
                Naming::Suffix("ListenerTest"),
                true,
            )
        },
        // **The proof needs a component to wait for.** Its probe replays the
        // whole topic from the start of its own consumer group, so it matches
        // the record by id -- and asserting on whichever record arrived first
        // would make the test pass or fail on what a neighbouring test
        // published. A payload with no `id` gets the publisher and the
        // listener and no proof, rather than a proof that is flaky by
        // construction.
        JavaFile {
            imports: &[
                Import::Keyed(Package::DomainEvents, Key::Event),
                Import::From(Package::Base, "KafkaTestcontainersConfig"),
            ],
            only_when: Some(has_id),
            source_set: SourceSet::Test,
            ..main(
                "messaging_it",
                crate::template!("spring/messaging_it_java.java"),
                Naming::Suffix("MessagingIT"),
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
    default_package: messaging_package,
    pass: "messaging",
    minimum_boot: None,
};

fn messaging_package(model: &AppModel, _: &Operation) -> String {
    model.project.package_for(Package::Messaging)
}

/// Whether the payload carries an `id` component the proof can match on.
fn has_id(model: &AppModel, operation: &Operation) -> bool {
    payload_names(model, operation).is_ok_and(|names| names.iter().any(|name| name == "id"))
}

fn payload_names(model: &AppModel, operation: &Operation) -> Result<Vec<String>, CompileError> {
    let OperationKind::Event(event) = &operation.kind else {
        unreachable!("only events reach here");
    };
    crate::emit_java::event_component_names(model, event)
}

/// `{{ordering}}`: the Javadoc that says truthfully what the partition key is.
fn ordering(model: &AppModel, operation: &Operation) -> Result<Rendered, CompileError> {
    Ok(partition_key(model, operation)?.javadoc.into())
}

/// `{{send}}`: the `kafka.send(..)` call, keyed or not.
fn send(model: &AppModel, operation: &Operation) -> Result<Rendered, CompileError> {
    Ok(match partition_key(model, operation)?.expression {
        Some(expression) => format!("kafka.send(topic, {expression}, event)"),
        None => "kafka.send(topic, event)".to_string(),
    }
    .into())
}

/// `{{event_args}}`: one `<Name>Event(...)` argument list for the proofs.
///
/// **Both proofs construct the payload, so both need what constructing it
/// names.** The sample is `UUID.fromString(..)`, `Instant.parse(..)` and the
/// rest, and a test given only the event record's own import compiles
/// exactly as long as every component happens to be a `String`.
fn event_args(model: &AppModel, operation: &Operation) -> Result<Rendered, CompileError> {
    let (arguments, _, imports) = crate::emit_component::http_sink::sample(model, operation)?;
    Ok(Rendered {
        text: arguments,
        imports,
    })
}

/// `{{disabled}}`: the annotation a proof carries when jails cannot construct
/// a project-owned component of the payload, and the import it needs.
fn disabled(model: &AppModel, operation: &Operation) -> Result<Rendered, CompileError> {
    let (_, disabled, _) = crate::emit_component::http_sink::sample(model, operation)?;
    Ok(match disabled {
        true => Rendered {
            text:
                "@Disabled(\"jails cannot construct a project-owned component of this payload\")\n"
                    .to_string(),
            imports: ["org.junit.jupiter.api.Disabled".to_string()].into(),
        },
        false => String::new().into(),
    })
}

/// Which component of the payload decides the partition, with the Javadoc that
/// says so truthfully.
///
/// **Kafka orders records within a partition only, so the key is the whole
/// ordering guarantee.** Keying on the event's own id spreads every record
/// about one entity across every partition -- the exact behaviour a comment
/// claiming "ordering per entity" would be denying. An event declared `on` an
/// entity keys on that entity's identity component when the payload carries
/// one; otherwise the key is the event id and the Javadoc says plainly that
/// there is no per-entity order.
struct PartitionKey {
    expression: Option<String>,
    javadoc: String,
}

fn partition_key(model: &AppModel, operation: &Operation) -> Result<PartitionKey, CompileError> {
    let OperationKind::Event(event) = &operation.kind else {
        unreachable!("only events reach here");
    };
    let components = crate::emit_java::event_component_names(model, event)?;
    let entity_key = event.on.as_ref().and_then(|id| {
        let entity = model.entities.get(id)?;
        let wanted = format!("{}Id", lower_first(&entity.names.java_type));
        components
            .iter()
            .find(|component| *component == &wanted)
            .cloned()
            .map(|component| (component, entity.names.java_type.clone()))
    });
    if let Some((component, entity)) = entity_key {
        return Ok(PartitionKey {
            expression: Some(format!("String.valueOf(event.{component}())")),
            javadoc: format!(
                " * <p>Keyed on {{@code {component}}}, so every record about one {entity}\n * lands on one partition and Kafka's per-partition order is that\n * {entity}'s order.\n *"
            ),
        });
    }
    if components.iter().any(|component| component == "id") {
        return Ok(PartitionKey {
            expression: Some("String.valueOf(event.id())".to_string()),
            javadoc: " * <p>Keyed on the event's own id, which spreads records about one\n * entity across every partition: there is <em>no</em> per-entity order\n * here. Declare the event on an entity whose identity the payload\n * carries to get one.\n *".to_string(),
        });
    }
    // **A payload with nothing to key on is published without one**, rather
    // than refused. A marker event -- `event Shipped` with no components --
    // is a legal declaration, and the honest rendering says the records
    // round-robin and no two of them have an order between them.
    Ok(PartitionKey {
        expression: None,
        javadoc: " * <p>Published with no key, because the payload carries nothing that\n * identifies what it is about: records round-robin across every\n * partition and no two of them have an order between them. Give the\n * event an `id` component, or declare it on an entity whose identity\n * it carries.\n *".to_string(),
    })
}

fn lower_first(value: &str) -> String {
    let mut characters = value.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_ascii_lowercase().to_string() + characters.as_str()
    })
}

#[cfg(test)]
mod tests {
    use jails_contracts::{BuildSystem, WorkspaceSnapshot};

    /// The Kafka slice for one event, with no `id` component anywhere in it.
    ///
    /// **The missing `id` is the point.** A listener that logs `event.id()`
    /// unconditionally generates a class that does not compile for an event
    /// declared without that component, and the proof `MessagingIT` is gated
    /// on `id` -- it matches the record it waits for by one -- so no other
    /// test in the tree compiles this shape.
    fn slice(source: &str) -> std::collections::BTreeMap<String, String> {
        let model = jails_model::parse_jdl(source).unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.spring_boot = Some("4.1.0".to_string());
        snapshot.project.build_system = BuildSystem::Maven;
        crate::Compiler::compile(
            &snapshot,
            &snapshot.model.model,
            &jails_model::Evolution::none(),
        )
        .unwrap()
        .generated
        .files
        .values()
        .map(|file| {
            (
                file.provenance.artifact_id.clone(),
                String::from_utf8(file.bytes.clone()).unwrap(),
            )
        })
        .collect()
    }

    const NO_ID: &str = "jdl 1\n\napp Notes @id(project_notes) {\n  pkg com.example.notes\n  java 26\n  platform spring\n  build maven\n  storage none\n}\n\ncap kafka\n\nentity Task @id(ent_task) {\n  id: uuid @id(fld_task_id) @pk\n  title: string @id(fld_task_title)\n\n  event Closed(title) @id(op_closed)\n}\n";

    /// An event whose sample needs imports: `UUID`, `Instant`, `URI`.
    const RICH: &str = "jdl 1\n\napp Notes @id(project_notes) {\n  pkg com.example.notes\n  java 26\n  platform spring\n  build maven\n  storage none\n}\n\ncap kafka\n\nentity Page @id(ent_page) {\n  id: uuid @id(fld_page_id) @pk\n  title: string @id(fld_page_title)\n\n  event Discovered(id, url: uri, at: instant) @id(op_discovered)\n}\n";

    /// **Constructing the payload is what a proof needs imports for.**
    ///
    /// The sample is `UUID.fromString(..)`, `URI.create(..)`,
    /// `Instant.parse(..)` -- so a test handed only the event record's own
    /// import compiles exactly as long as every component happens to be a
    /// `String`, and the first payload with a real type does not.
    #[test]
    fn every_proof_imports_what_constructing_the_payload_names() {
        let slice = slice(RICH);
        let test = &slice["art_op_discovered_listener_test"];
        for import in [
            "import java.net.URI;",
            "import java.time.Instant;",
            "import java.util.UUID;",
            "import com.example.notes.domain.events.DiscoveredEvent;",
        ] {
            assert!(
                test.contains(import),
                "the unit proof is missing `{import}`:\n{test}"
            );
        }
        // Whatever the sample names, the two proofs name the same things: a
        // second answer here is how they drift.
        let broker = &slice["art_op_discovered_messaging_it"];
        for import in ["import java.net.URI;", "import java.time.Instant;"] {
            assert!(broker.contains(import), "{broker}");
        }
    }

    /// The listener hands the record to a port instead of logging and dropping
    /// it, and says so when no handler is registered.
    #[test]
    fn a_consumed_event_reaches_a_port_rather_than_the_log() {
        let slice = slice(NO_ID);
        let listener = &slice["art_op_closed_listener"];
        assert!(
            listener.contains("public ClosedListener(List<ClosedHandler> handlers)"),
            "{listener}"
        );
        assert!(listener.contains("handler.handle(event);"), "{listener}");
        // The empty case is warned about rather than being indistinguishable
        // from a working consumer.
        assert!(listener.contains("handlers.isEmpty()"), "{listener}");
        assert!(listener.contains("log.warn("), "{listener}");
        // A log-and-drop body compiles and passes the broker proof, because
        // that proof has its own probe listener and never observes this class
        // at all.
        assert!(!listener.contains("TODO: hand this to"), "{listener}");
    }

    /// The port is ABI: the listener's constructor names it.
    #[test]
    fn the_handler_port_is_managed_rather_than_ejectable() {
        let model = jails_model::parse_jdl(NO_ID).unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.spring_boot = Some("4.1.0".to_string());
        snapshot.project.build_system = BuildSystem::Maven;
        let draft = crate::Compiler::compile(
            &snapshot,
            &snapshot.model.model,
            &jails_model::Evolution::none(),
        )
        .unwrap();
        let handler = draft
            .generated
            .files
            .values()
            .find(|file| file.provenance.artifact_id == "art_op_closed_handler")
            .expect("the handler port is emitted");
        assert!(!handler.provenance.ejectable);
        let source = String::from_utf8(handler.bytes.clone()).unwrap();
        assert!(source.contains("interface ClosedHandler"), "{source}");
        assert!(
            source.contains("void handle(ClosedEvent event);"),
            "{source}"
        );
        // It names the payload type, so it needs the import when the event
        // record is not in the messaging package.
        assert!(
            source.contains("import com.example.notes.domain.events.ClosedEvent;"),
            "{source}"
        );
    }

    /// Nothing in the slice names a component the payload does not carry.
    #[test]
    fn an_event_without_an_id_still_emits_a_slice_that_compiles() {
        let slice = slice(NO_ID);
        for artifact in ["art_op_closed_listener", "art_op_closed_listener_test"] {
            let source = &slice[artifact];
            assert!(
                !source.contains("event.id()"),
                "`{artifact}` calls `id()` on a payload that has none:\n{source}"
            );
        }
        // The broker proof waits for a record by id, so it is the one file
        // that is correctly absent for this payload rather than emitted and
        // broken.
        assert!(!slice.contains_key("art_op_closed_messaging_it"));
        // The unit proof needs no id and is emitted for every event.
        let test = &slice["art_op_closed_listener_test"];
        assert!(
            test.contains("new ClosedListener(List.of(first::add"),
            "{test}"
        );
        assert!(test.contains("doesNotThrowAnyException"), "{test}");
    }
}
