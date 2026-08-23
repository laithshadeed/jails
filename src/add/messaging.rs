//! `add kafka`: the broker slice.
//!
//! Owns everything topic-agnostic -- the `DefaultErrorHandler`, the DLT
//! routing, `ErrorHandlingDeserializer`, the dead-letter counter. What needs
//! a payload type (`NewTopic` beans, `spring.json.value.default.type`)
//! belongs to `g event` instead: `add kafka` cannot know a topic name, and a
//! generated one for a guessed name is worse than none.

use super::*;
use crate::spring::TESTCONTAINERS_JUNIT;

// ---------------------------------------------------------------------------
// kafka
// ---------------------------------------------------------------------------

pub(super) const SPRING_KAFKA: Dependency = Dependency {
    group_id: "org.springframework.boot",
    artifact_id: "spring-boot-starter-kafka",
    version: None,
    scope: None,
    optional: false,
};
pub(super) const KAFKA_CLIENTS: Dependency = Dependency {
    group_id: "org.apache.kafka",
    artifact_id: "kafka-clients",
    version: Some("4.1.0"),
    scope: None,
    optional: false,
};
/// Without this no test can touch a broker, which is why `add kafka` used to
/// produce a capability with no possible test.
pub(super) const TESTCONTAINERS_KAFKA: Dependency = Dependency {
    group_id: "org.testcontainers",
    artifact_id: "testcontainers-kafka",
    version: Some("2.0.5"),
    scope: Some("test"),
    optional: false,
};
/// The `MeterRegistry` *API*, so the generated error handler can count
/// dead-lettered records.
///
/// Needed explicitly, which is not obvious: `spring-kafka` declares
/// micrometer-core as `optionalApi` and `spring-boot-kafka` declares it
/// `optional`, and neither kind is inherited by a downstream consumer. Without
/// this line `KafkaConfig` does not compile.
///
/// The API only. No registry *bean* is auto-configured without Actuator --
/// `MetricsAutoConfiguration` and `CompositeMeterRegistryAutoConfiguration` are
/// `@ConditionalOnClass` inside a module that only
/// `spring-boot-starter-actuator` puts on the classpath. That is why the
/// generated bean takes an `ObjectProvider<MeterRegistry>` rather than a
/// `MeterRegistry`: asking for a broker should not drag in Actuator and its
/// endpoints. `jails add observability` is what supplies the registry.
pub(super) const MICROMETER_CORE: Dependency = Dependency {
    group_id: "io.micrometer",
    artifact_id: "micrometer-core",
    version: None,
    scope: None,
    optional: false,
};
/// Consuming is asynchronous, so every meaningful Kafka test waits for
/// something. Without a waiting primitive the generated test is a `Thread.sleep`
/// that is either flaky or slow.
pub(super) const AWAITILITY: Dependency = Dependency {
    group_id: "org.awaitility",
    artifact_id: "awaitility",
    version: None,
    scope: Some("test"),
    optional: false,
};

pub(super) fn kafka_plan(slice: &Slice) -> Result<Change> {
    let root: &Path = slice.root();
    let flavor: Flavor = slice.flavor();
    let pkg: &str = &slice.placed(Layer::Messaging);
    // Spring projects also get the properties that make publish-and-consume
    // work at all. Without them the broker is up, the code compiles, and
    // nothing is ever received -- see `spring::kafka_properties` for why each
    // one is there.
    let properties = match flavor {
        Flavor::SpringBoot => {
            let base = base_package(root)?;
            // The artifactId, not the directory name: a consumer group is a
            // shared, durable identity in the broker, and naming it after
            // whatever the checkout happens to be called gives two clones of
            // the same service two different groups -- so both receive every
            // message instead of splitting the work.
            let group = pom::read(root)
                .ok()
                .and_then(|pom| crate::project::artifact_id(&pom))
                .unwrap_or_else(|| "app".to_string());
            crate::spring::kafka_properties(&base, &group)
        }
        Flavor::PlainMaven => Vec::new(),
    };
    // The poison-message path is Spring-only: it is Spring Kafka's
    // `DefaultErrorHandler` that routes a bad record, and a plain
    // `kafka-clients` consumer has no equivalent to generate.
    let (deps, files) = match flavor {
        Flavor::SpringBoot => (
            vec![
                SPRING_KAFKA,
                MICROMETER_CORE,
                SPRING_TESTCONTAINERS,
                TESTCONTAINERS_KAFKA,
                TESTCONTAINERS_JUNIT,
                AWAITILITY,
            ],
            crate::spring::kafka_files(root, pkg),
        ),
        Flavor::PlainMaven => (vec![KAFKA_CLIENTS], Vec::new()),
    };

    Ok(Change {
        deps,
        files,
        compose: vec![compose::KAFKA],
        properties,
        ..Change::default()
    })
}
