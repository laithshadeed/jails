//! `add kafka` and `generate event`: the broker, and what needs a payload type.
//!
//! The line between them is the point, and `CLAUDE.md` states it: **`add kafka`
//! cannot know a topic name and must not guess one.** The capability owns
//! everything topic-agnostic — the `DefaultErrorHandler`, the DLT routing, the
//! `ErrorHandlingDeserializer` — and `g event` owns what needs a payload type:
//! the `NewTopic` beans and `spring.json.value.default.type`.
//!
//! The dead-letter destination is named **explicitly** in the recoverer.
//! `DeadLetterPublishingRecoverer` defaults to `<topic>-dlt` and the source
//! partition number, so a project declaring `<topic>.DLT` finds it empty with
//! only a WARN to say so.

use super::*;

pub const KAFKA_TESTCONTAINERS_CONFIG: &str = "KafkaTestcontainersConfig";

/// The Spring Kafka properties that make publish-and-consume actually work.
///
/// Every one of these is a thing people discover by losing an afternoon:
///
/// - `auto-offset-reset=earliest`: a consumer joining a group for the first
///   time otherwise starts at the *end* of the topic, so anything published
///   before it joined is invisible. This is the single most common reason a
///   Kafka integration test hangs and then fails with nothing consumed.
/// - `JacksonJsonSerializer`/`JacksonJsonDeserializer`: the defaults are
///   `StringSerializer`, so a record payload arrives as its `toString()` and
///   comes back unparseable. Note the `Jackson` prefix -- the older
///   `JsonSerializer`/`JsonDeserializer` pair is deprecated for removal since
///   Spring Kafka 4.0, which moved to Jackson 3.
/// - `spring.json.trusted.packages`: the deserializer refuses to instantiate
///   a type outside the trusted list, and reports it as a deserialization
///   failure rather than a configuration one.
pub fn kafka_properties(base: &str, group: &str) -> Vec<String> {
    vec![
        "spring.kafka.bootstrap-servers=localhost:9092".to_string(),
        format!("spring.kafka.consumer.group-id={group}"),
        "spring.kafka.consumer.auto-offset-reset=earliest".to_string(),
        "spring.kafka.producer.value-serializer=org.springframework.kafka.support.serializer.JacksonJsonSerializer".to_string(),
        // Both the base package *and* a wildcard for everything under it.
        // The check is `PatternMatchUtils.simpleMatch` against the class's
        // package name, so it is neither a prefix match nor recursive:
        // `com.example.app` alone rejects `com.example.app.messaging` --
        // where `jails g event` puts the payload -- and the failure surfaces
        // as a SerializationException reading "is not in the trusted
        // packages", which sounds like a security setting rather than a
        // missing dot-star. The wildcard alone would not match the base
        // package itself, hence both.
        format!("spring.kafka.consumer.properties.spring.json.trusted.packages={base},{base}.*"),
        // KIP-848. The broker default since Kafka 4.0, but the *client*
        // default is still `classic`, so a project that does not opt in keeps
        // the stop-the-world rebalance -- every consumer in the group stops
        // while one joins. Nothing reports this; it just stays slow.
        "spring.kafka.consumer.properties.group.protocol=consumer".to_string(),
        // Durability over throughput. `acks=all` waits for the in-sync
        // replicas, and idempotence stops a producer retry from writing the
        // record twice. Both are stated rather than inherited because the
        // defaults have moved between client versions.
        "spring.kafka.producer.acks=all".to_string(),
        "spring.kafka.producer.properties.enable.idempotence=true".to_string(),
    ]
    .into_iter()
    .chain(kafka_deserializer_properties())
    .collect()
}

/// The deserializer half, kept apart because it is what makes a poison
/// message survivable.
///
/// A record that will not deserialize will not deserialize on the next
/// attempt either. Left as a plain `JacksonJsonDeserializer`, the failure is
/// thrown *inside* the consumer before any error handler can see it as a
/// record, so the container retries the same bad offset forever and the
/// partition stops. `ErrorHandlingDeserializer` catches it and hands the
/// error along as the record's value, which is the only shape
/// `DefaultErrorHandler` can route to a dead-letter topic.
///
/// Separate from [`kafka_properties`] only for readability -- `add kafka`
/// writes both, and one without the other is the bug.
fn kafka_deserializer_properties() -> Vec<String> {
    vec![
        "spring.kafka.consumer.value-deserializer=org.springframework.kafka.support.serializer.ErrorHandlingDeserializer".to_string(),
        "spring.kafka.consumer.properties.spring.deserializer.value.delegate.class=org.springframework.kafka.support.serializer.JacksonJsonDeserializer".to_string(),
    ]
}

/// What happens to a record that does not process cleanly.
///
/// This is the half of a Kafka integration that nobody writes on day one and
/// everybody needs on day two. Spring Kafka's default is to retry a failing
/// record ten times and then *log and move on*, and the shape of the failure
/// decides which half of that is wrong:
///
/// - A record that will not deserialize, or that names an enum constant this
///   service does not have, fails identically on every attempt. Retrying it
///   is a loop that costs the whole partition, and the only symptom is
///   consumer lag with no new errors after the first.
/// - A database that is briefly unavailable is the opposite case, and the one
///   the backoff exists for.
///
/// So the classification is the load-bearing part, not the backoff.
///
/// It is expressed as *one* marker exception rather than a list of JDK types,
/// for two reasons that come out of
/// `deps/spring-kafka/.../listener/ExceptionClassifier.java`:
///
/// - The framework already treats `DeserializationException`,
///   `MessageConversionException`, `ConversionException`,
///   `MethodArgumentResolutionException` and `ClassCastException` as fatal
///   (`defaultFatalExceptionsList`). Re-listing one of those reads as if the
///   generated list were the whole policy, and hides the other four.
/// - Naming `NullPointerException` there is worse than redundant. An NPE is a
///   bug in the listener, not a bad record; classifying it permanent commits
///   the offset and destroys the repeating failure that would have surfaced
///   it. Only the domain knows what is genuinely unprocessable, so only the
///   domain gets to say so -- see [`non_retryable_exception_java`].
///
/// No `NewTopic` beans here: `add kafka` does not know what this service's
/// topics are called. `jails g event <Name>` declares them, because it does.
fn kafka_config_java(pkg: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/kafka_config_java.java"),
        &[("pkg", pkg)],
    )
}

/// The domain's own "no retry will ever fix this".
///
/// Deliberately unlike [`api_exception_java`], which is sealed, abstract and
/// stack-trace-free. This one is open -- callers throw and subclass it -- and it
/// keeps its stack trace, because it wraps a real cause and that cause is what
/// a human reads out of the dead-letter headers.
fn non_retryable_exception_java(pkg: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/non_retryable_exception_java.java"),
        &[("pkg", pkg)],
    )
}

/// A test that the poison-message path is actually wired, without a broker.
///
/// The container-backed version belongs to `g event`; this one exists so that
/// `add kafka` keeps the promise `jails add --help` makes -- a dependency,
/// the code that uses it, *and a test that proves it works*.
fn kafka_config_test_java(pkg: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/kafka_config_test_java.java"),
        &[("pkg", pkg)],
    )
}

/// A broker shared by only the integration tests that import it.
///
/// Keeping this separate from the event-specific test lets an outbox IT and a
/// messaging IT use the same dynamic broker in one Failsafe JVM. Registering
/// it globally would make every unrelated `@SpringBootTest` start Kafka.
fn kafka_testcontainers_config_java(pkg: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/kafka_testcontainers_config_java.java"),
        &[
            ("pkg", pkg),
            ("KAFKA_TESTCONTAINERS_CONFIG", KAFKA_TESTCONTAINERS_CONFIG),
        ],
    )
}

/// The files `add kafka` writes on a Spring project.
pub fn kafka_files(root: &Path, pkg: &str, base: &str) -> Vec<Artifact> {
    vec![
        Artifact {
            kind: "kafka config",
            path: crate::generate::main_dir(root, pkg).join("KafkaConfig.java"),
            contents: kafka_config_java(pkg),
        },
        Artifact {
            kind: "non-retryable exception",
            path: crate::generate::main_dir(root, pkg).join("NonRetryableException.java"),
            contents: non_retryable_exception_java(pkg),
        },
        Artifact {
            kind: "kafka config test",
            path: crate::generate::test_dir(root, pkg).join("KafkaConfigTest.java"),
            contents: kafka_config_test_java(pkg),
        },
        Artifact {
            kind: "Kafka testcontainers config",
            path: crate::generate::test_dir(root, base)
                .join(format!("{KAFKA_TESTCONTAINERS_CONFIG}.java")),
            contents: kafka_testcontainers_config_java(base),
        },
    ]
}

pub fn event_files(
    slice: &Slice,
    name: &str,
    fields: &[crate::generate::Field],
) -> Result<Vec<Artifact>> {
    let root: &Path = slice.project().root();
    let pkg: &str = &slice.placed(Layer::Messaging);
    let domain: &str = &slice.placed(Layer::Domain);
    let main = crate::generate::main_dir(root, pkg);
    let test = crate::generate::test_dir(root, pkg);
    let topic = crate::sql::snake_case(name).replace('_', "-");
    let id = fields.iter().find(|field| field.name == "id");
    if !fields.is_empty() && id.is_none() {
        return Err(format!(
            "an event payload needs a stable `id` field for deduplication and Kafka partitioning.\n       \
             Add `id:string!` or `id:uuid` to `jails g event {name} ...`."
        ));
    }
    if id.is_some_and(|field| field.optionality == crate::generate::Optionality::Nullable) {
        return Err(
            "an event `id` cannot be optional: a null key loses per-entity ordering".to_string(),
        );
    }
    let key = id
        .filter(|field| field.java_type != "String")
        .map(|_| "String.valueOf(event.id())")
        .unwrap_or("event.id()");
    Ok(vec![
        Artifact {
            kind: "event",
            path: main.join(format!("{name}Event.java")),
            contents: event_java(pkg, domain, name, fields),
        },
        Artifact {
            kind: "publisher",
            path: main.join(format!("{name}Publisher.java")),
            contents: publisher_java(pkg, name, &topic, key),
        },
        Artifact {
            kind: "listener",
            path: main.join(format!("{name}Listener.java")),
            contents: listener_java(pkg, name, &topic),
        },
        Artifact {
            kind: "messaging integration test",
            path: test.join(format!("{name}MessagingIT.java")),
            contents: messaging_it_java(slice, name, &topic, fields),
        },
    ])
}

fn event_java(pkg: &str, domain: &str, name: &str, fields: &[crate::generate::Field]) -> String {
    if fields.is_empty() {
        return crate::template::render(
            crate::template_here!("spring/event_java.java"),
            &[("pkg", pkg), ("name", name)],
        );
    }

    let event = format!("{name}Event");
    let mut source = crate::generate::record_java(pkg, &event, fields);
    let mut imports = fields
        .iter()
        .filter(|field| field.owned && domain != pkg)
        .map(|field| format!("import {domain}.{};", field.java_type))
        .collect::<Vec<_>>();
    imports.sort();
    imports.dedup();
    if !imports.is_empty() {
        let package = format!("package {pkg};\n");
        let replacement = format!("{package}\n{}\n", imports.join("\n"));
        source = source.replacen(&package, &replacement, 1);
        source = jails_java::tidy::normalize_imports(&source);
    }
    source.replace(
        &format!(" * An immutable {event} value."),
        &format!(" * Immutable payload published as {event}."),
    )
}

fn publisher_java(pkg: &str, name: &str, topic: &str, key: &str) -> String {
    let source = crate::template::render(
        crate::template_here!("spring/publisher_java.java"),
        &[("pkg", pkg), ("name", name), ("topic", topic)],
    );
    source.replace(
        "kafka.send(topic, event.id(), event)",
        &format!("kafka.send(topic, {key}, event)"),
    )
}

fn listener_java(pkg: &str, name: &str, topic: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/listener_java.java"),
        &[("pkg", pkg), ("name", name), ("topic", topic)],
    )
}

fn messaging_it_java(
    slice: &Slice,
    name: &str,
    topic: &str,
    fields: &[crate::generate::Field],
) -> String {
    let root: &Path = slice.project().root();
    let pkg: &str = &slice.placed(Layer::Messaging);
    let base: String = slice.root_package();
    let domain: &str = &slice.placed(Layer::Domain);
    let (event_imports, disabled_import, disabled, event_args, expected_id) = if fields.is_empty() {
        (
            "import java.time.Instant;\n".to_string(),
            String::new(),
            String::new(),
            "\"probe-1\", Instant.parse(\"2024-01-01T00:00:00Z\")".to_string(),
            "\"probe-1\"".to_string(),
        )
    } else {
        let samples = fields
            .iter()
            .map(|field| crate::generate::sample_value(field, root, domain))
            .collect::<Vec<_>>();
        let missing = fields
            .iter()
            .zip(&samples)
            .filter(|(_, sample)| sample.is_none())
            .map(|(field, _)| field.name.as_str())
            .collect::<Vec<_>>();
        let expected_id = fields
            .iter()
            .zip(&samples)
            .find(|(field, _)| field.name == "id")
            .and_then(|(_, sample)| sample.clone())
            .unwrap_or_else(|| "null /* TODO: an event id sample */".to_string());
        let event_args = fields
            .iter()
            .zip(samples)
            .map(|(field, sample)| {
                sample.unwrap_or_else(|| format!("null /* TODO: a {} */", field.java_type))
            })
            .collect::<Vec<_>>()
            .join(",\n                ");
        let mut imports = fields
            .iter()
            .flat_map(|field| field.imports.iter().copied().map(str::to_string))
            .collect::<Vec<_>>();
        imports.extend(
            fields
                .iter()
                .filter(|field| field.owned && domain != pkg)
                .map(|field| format!("{domain}.{}", field.java_type)),
        );
        if fields
            .iter()
            .any(|field| field.optionality == crate::generate::Optionality::Nullable)
        {
            imports.push("java.util.Optional".to_string());
        }
        imports.sort();
        imports.dedup();
        let imports = imports
            .into_iter()
            .map(|import| format!("import {import};\n"))
            .collect::<String>();
        if missing.is_empty() {
            (
                imports,
                String::new(),
                String::new(),
                event_args,
                expected_id,
            )
        } else {
            (
                imports,
                "import org.junit.jupiter.api.Disabled;\n".to_string(),
                format!(
                    "@Disabled(\"todo: supply a sample for {} -- jails cannot build the full event\")\n",
                    missing.join(", ")
                ),
                event_args,
                expected_id,
            )
        }
    };
    crate::template::render(
        crate::template_here!("spring/messaging_it_java.java"),
        &[
            ("pkg", pkg),
            ("name", name),
            ("topic", topic),
            ("event_imports", &event_imports),
            ("disabled_import", &disabled_import),
            ("disabled", &disabled),
            (
                "kafka_testcontainers_import",
                &crate::generate::import_of(pkg, &base, KAFKA_TESTCONTAINERS_CONFIG),
            ),
            ("KAFKA_TESTCONTAINERS_CONFIG", KAFKA_TESTCONTAINERS_CONFIG),
            ("event_args", &event_args),
            ("expected_id", &expected_id),
        ],
    )
}
