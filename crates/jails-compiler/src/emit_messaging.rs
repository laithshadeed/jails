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

const PUBLISHER: &str = include_str!("../../../templates/spring/publisher_java.java");
const LISTENER: &str = include_str!("../../../templates/spring/listener_java.java");
const IT: &str = include_str!("../../../templates/spring/messaging_it_java.java");

pub(crate) fn lower_and_emit(
    model: &AppModel,
    output: &mut RenderedTree,
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
        for (path, file) in files(model, operation)? {
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
) -> Result<Vec<(ProjectPath, RenderedFile)>, CompileError> {
    let name = &operation.names.java_type;
    let event_type = crate::emit_java::with_suffix(name, "Event");
    let messaging = model.project.package_for(Package::Messaging);
    let events = model.project.package_for(Package::DomainEvents);
    let topic = topic(operation);
    let event_import = import(&messaging, &events, &event_type);
    let key = partition_key(model, operation)?;

    let publisher = PUBLISHER
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
    let listener = LISTENER
        .replace("{{pkg}}", &messaging)
        .replace(
            "import org.slf4j.Logger;",
            &format!("{event_import}import org.slf4j.Logger;"),
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
        rendered(
            operation,
            "listener",
            &messaging,
            &format!("{name}Listener"),
            listener,
            FileKind::JavaMain,
        )?,
    ];
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

    let (arguments, disabled, sample_imports) =
        crate::emit_component::http_sink::sample(model, operation)?;
    let mut imports = sample_imports
        .iter()
        .map(|import| format!("import {import};\n"))
        .collect::<Vec<_>>();
    if !event_import.is_empty() {
        imports.push(event_import.clone());
    }
    imports.sort();
    let test = IT
        .replace("{{pkg}}", &messaging)
        .replace("{{event_imports}}", &imports.concat())
        .replace(
            "{{disabled_import}}",
            match disabled {
                true => "import org.junit.jupiter.api.Disabled;\n",
                false => "",
            },
        )
        .replace(
            "{{disabled}}",
            match disabled {
                true => "@Disabled(\"jails cannot construct a project-owned component of this payload\")\n",
                false => "",
            },
        )
        .replace(
            "{{kafka_testcontainers_import}}",
            &import(
                &messaging,
                &model.project.package_for(Package::Base),
                "KafkaTestcontainersConfig",
            ),
        )
        .replace("{{KAFKA_TESTCONTAINERS_CONFIG}}", "KafkaTestcontainersConfig")
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
