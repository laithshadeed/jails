//! The Kafka slice a declared event gets, when the project declares a broker.
//!
//! **The line is the one `CLAUDE.md` draws and this side of it is the payload
//! half.** `cap kafka` owns everything topic-agnostic -- the
//! `DefaultErrorHandler`, the dead-letter routing, the
//! `ErrorHandlingDeserializer` -- because a capability cannot know a topic
//! name and must not guess one. An `event` declaration is where the name and
//! the payload type come from, so the publisher, the listener and the proof
//! that a published record comes back live here.
//!
//! Nothing is emitted for a project that has not declared the capability: an
//! `event` is a domain fact first, and a project with no broker still wants
//! the record and the `publishEvent` the operation already makes.

use crate::CompileError;
use jails_contracts::{FileKind, FileMode, ProjectPath, Provenance, RenderedFile, RenderedTree};
use jails_model::{AppModel, Operation, OperationKind, Package, StableId as _};
use std::collections::BTreeSet;

const MAIN_ROOT: &str = ".jails/generated/main/java";
const TEST_ROOT: &str = ".jails/generated/test/java";

const PUBLISHER: crate::Template = crate::template!("spring/publisher_java.java");
const LISTENER: crate::Template = crate::template!("spring/listener_java.java");
const HANDLER: crate::Template = crate::template!("spring/event_handler_java.java");
const LISTENER_TEST: crate::Template = crate::template!("spring/listener_test_java.java");
const IT: crate::Template = crate::template!("spring/messaging_it_java.java");

pub(crate) fn lower_and_emit(
    model: &AppModel,
    output: &mut RenderedTree,
    observed: &crate::emit::Observed<'_>,
) -> Result<(), CompileError> {
    if !model
        .capabilities
        .values()
        .any(|capability| capability.kind == "kafka")
    {
        return Ok(());
    }
    for operation in model.operations.values() {
        if !matches!(operation.kind, OperationKind::Event(_)) {
            continue;
        }
        for (path, file) in files(model, operation, observed.templates)? {
            output.insert(path, file).map_err(CompileError::new)?;
        }
    }
    Ok(())
}

/// `payout_settled` as `payout-settled`, which is the topic.
///
/// The stable label rather than the Java name, so renaming the type leaves
/// the deployed topic alone -- the same rule every other projection follows.
pub(crate) fn topic(operation: &Operation) -> String {
    operation.label.replace('_', "-")
}

fn files(
    model: &AppModel,
    operation: &Operation,
    templates: &jails_contracts::TemplateOverrides,
) -> Result<Vec<(ProjectPath, RenderedFile)>, CompileError> {
    let name = &operation.names.java_type;
    let event_type = crate::emit_java::with_suffix(name, "Event");
    let messaging = model.project.package_for(Package::Messaging);
    let events = model.project.package_for(Package::DomainEvents);
    let topic = topic(operation);
    let event_import = import(&messaging, &events, &event_type);
    let key = partition_key(model, operation)?;

    let publisher = PUBLISHER
        .resolve(templates)?
        .replace("{{pkg}}", &messaging)
        .replace(
            "import java.util.concurrent.CompletableFuture;",
            &format!("{event_import}import java.util.concurrent.CompletableFuture;"),
        )
        .replace("{{ordering}}", &key.javadoc)
        .replace(
            "kafka.send(topic, {{key}}, event)",
            &match &key.expression {
                Some(expression) => format!("kafka.send(topic, {expression}, event)"),
                None => "kafka.send(topic, event)".to_string(),
            },
        )
        .replace("{{topic}}", &topic)
        .replace("{{name}}Event", &event_type)
        .replace("{{name}}", name);
    // **The handler port is the reaction's home, and it lives beside the
    // listener rather than in `ports.events`.** That package is the *outbound*
    // publish port an entity's `events` facet emits; an inbound handler is the
    // other direction, and filing both under one name would make
    // `TaskEvents` and `ClosedHandler` look like halves of one interface.
    let handler_type = format!("{name}Handler");
    let handler = HANDLER
        .resolve(templates)?
        .replace("{{pkg}}", &messaging)
        // The port names the payload type in its own signature, so it needs
        // the import whenever the event record is not in this package.
        .replace(
            "{{event_import}}",
            &match event_import.is_empty() {
                true => String::new(),
                false => format!("{event_import}\n"),
            },
        )
        .replace("{{name}}Event", &event_type)
        .replace("{{name}}", name);
    let listener = LISTENER
        .resolve(templates)?
        .replace("{{pkg}}", &messaging)
        .replace(
            "import java.util.List;",
            &format!("{event_import}import java.util.List;"),
        )
        .replace("{{topic}}", &topic)
        .replace("{{name}}Event", &event_type)
        .replace("{{name}}", name);

    // **The proof needs a component to wait for.** Its probe replays the whole
    // topic from the start of its own consumer group, so it matches the record
    // by id -- and asserting on whichever record arrived first would make the
    // test pass or fail on what a neighbouring test published. A payload with
    // no `id` gets the publisher and the listener and no proof, rather than a
    // proof that is flaky by construction.
    let mut files = vec![
        rendered(
            operation,
            "publisher",
            &messaging,
            &format!("{name}Publisher"),
            publisher,
            FileKind::JavaMain,
        )?,
        // Managed ABI: the listener's constructor names it, so ejecting the
        // port would leave the compiler emitting a class against a type it no
        // longer controls. The implementations the reader writes are theirs
        // already -- jails never generates one.
        port(operation, &messaging, &handler_type, handler)?,
        rendered(
            operation,
            "listener",
            &messaging,
            &format!("{name}Listener"),
            listener,
            FileKind::JavaMain,
        )?,
    ];
    let (arguments, disabled, sample_imports) =
        crate::emit_component::http_sink::sample(model, operation)?;
    let disabled_import = match disabled {
        true => "import org.junit.jupiter.api.Disabled;\n",
        false => "",
    };
    let disabled_annotation = match disabled {
        true => "@Disabled(\"jails cannot construct a project-owned component of this payload\")\n",
        false => "",
    };

    // **Both proofs construct the payload, so both need what constructing it
    // names.** The sample is `UUID.fromString(..)`, `Instant.parse(..)` and
    // the rest, and a test given only the event record's own import compiles
    // exactly as long as every component happens to be a `String`.
    let mut imports = sample_imports
        .iter()
        .map(|import| format!("import {import};\n"))
        .collect::<Vec<_>>();
    if !event_import.is_empty() {
        imports.push(event_import.clone());
    }
    imports.sort();
    let imports = imports.concat();

    // **The listener test needs no broker and no `id`.** The proof below waits
    // for a record to come back and matches it by id, so it is emitted only
    // for a payload that has one; delegation to the port is a property of an
    // ordinary object, so it is proved for every event.
    let listener_test = LISTENER_TEST
        .resolve(templates)?
        .replace("{{pkg}}", &messaging)
        .replace("{{event_imports}}", &imports)
        .replace("{{disabled_import}}", disabled_import)
        .replace("{{disabled}}", disabled_annotation)
        .replace("{{event_args}}", &arguments)
        .replace("{{name}}Event", &event_type)
        .replace("{{name}}", name);
    files.push(rendered(
        operation,
        "listener_test",
        &messaging,
        &format!("{name}ListenerTest"),
        listener_test,
        FileKind::JavaTest,
    )?);

    if !crate::emit_java::event_component_names(
        model,
        match &operation.kind {
            OperationKind::Event(event) => event,
            _ => unreachable!("only events reach here"),
        },
    )?
    .iter()
    .any(|component| component == "id")
    {
        return Ok(files);
    }

    let test = IT
        .resolve(templates)?
        .replace("{{pkg}}", &messaging)
        .replace("{{event_imports}}", &imports)
        .replace("{{disabled_import}}", disabled_import)
        .replace("{{disabled}}", disabled_annotation)
        .replace(
            "{{kafka_testcontainers_import}}",
            &import(
                &messaging,
                &model.project.package_for(Package::Base),
                "KafkaTestcontainersConfig",
            ),
        )
        .replace(
            "{{KAFKA_TESTCONTAINERS_CONFIG}}",
            "KafkaTestcontainersConfig",
        )
        .replace("{{event_args}}", &arguments)
        .replace("{{topic}}", &topic)
        .replace("{{name}}Event", &event_type)
        .replace("{{name}}", name);

    files.push(rendered(
        operation,
        "messaging_it",
        &messaging,
        &format!("{name}MessagingIT"),
        test,
        FileKind::JavaTest,
    )?);
    Ok(files)
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

fn import(user: &str, owner: &str, class: &str) -> String {
    match user == owner {
        true => String::new(),
        false => format!("import {owner}.{class};\n"),
    }
}

/// The handler port, which is ABI rather than an adapter.
///
/// [`rendered`] marks everything it writes ejectable, and that is right for the
/// publisher, the listener and the proof -- they are implementations a reader
/// may take over. The port is not: the listener's constructor names it, so
/// transferring ownership would leave the compiler emitting a class against a
/// type it no longer controls. Same rule as a repository port.
fn port(
    operation: &Operation,
    package: &str,
    type_name: &str,
    body: String,
) -> Result<(ProjectPath, RenderedFile), CompileError> {
    let (path, mut file) = rendered(
        operation,
        "handler",
        package,
        type_name,
        body,
        FileKind::JavaMain,
    )?;
    file.provenance.ejectable = false;
    Ok((path, file))
}

fn rendered(
    operation: &Operation,
    suffix: &str,
    package: &str,
    type_name: &str,
    body: String,
    kind: FileKind,
) -> Result<(ProjectPath, RenderedFile), CompileError> {
    let artifact = format!("art_{}_{suffix}", operation.id.as_str());
    let root = match kind {
        FileKind::JavaTest => TEST_ROOT,
        _ => MAIN_ROOT,
    };
    let path = ProjectPath::parse(format!(
        "{root}/{}/{type_name}.java",
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
            kind,
            mode: FileMode::Regular,
            provenance: Provenance {
                artifact_id: artifact.clone(),
                ejection_id: None,
                ejectable: true,
                semantic_ids: BTreeSet::from([operation.id.as_str().to_string()]),
                compiler_pass: "messaging".to_string(),
            },
        },
    ))
}

#[cfg(test)]
mod tests {
    use jails_contracts::{BuildSystem, WorkspaceSnapshot};

    /// The Kafka slice for one event, with no `id` component anywhere in it.
    ///
    /// **The missing `id` is the point.** `docs/20-generated-java.md` P6.6 §8
    /// is about the listener discarding the event, and fixing that surfaced a
    /// second defect underneath: the listener logged `event.id()`
    /// unconditionally, so an event declared without that component generated
    /// a class that did not compile. The proof `MessagingIT` is gated on `id`
    /// -- it matches the record it waits for by one -- so no test in the tree
    /// had ever compiled this shape.
    fn slice(source: &str) -> std::collections::BTreeMap<String, String> {
        let model = jails_model::parse_jdl(source).unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.spring_boot = Some("4.1.0".to_string());
        snapshot.project.build_system = BuildSystem::Maven;
        crate::Compiler::compile(&snapshot, None)
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
    /// `String`, and the first payload with a real type does not. The broker
    /// proof already assembled this list; the unit proof was written without
    /// it and passed every check but a compiler.
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
        // Whatever the sample names, the two proofs name the same things: the
        // broker proof is the one that had the list, and a second answer here
        // is how they drift.
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
        // The old body. A regression to it compiles and passes the broker
        // proof, because that proof has its own probe listener and never
        // observes this class at all.
        assert!(!listener.contains("TODO: hand this to"), "{listener}");
    }

    /// The port is ABI: the listener's constructor names it.
    #[test]
    fn the_handler_port_is_managed_rather_than_ejectable() {
        let model = jails_model::parse_jdl(NO_ID).unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.spring_boot = Some("4.1.0".to_string());
        snapshot.project.build_system = BuildSystem::Maven;
        let draft = crate::Compiler::compile(&snapshot, None).unwrap();
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
