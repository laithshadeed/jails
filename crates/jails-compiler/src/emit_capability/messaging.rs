//! Declarative messaging capability packs.

use super::*;
use jails_model::Package;

const KAFKA_FILES: &[JavaFile] = &[
    JavaFile {
        suffix: "config",
        template: crate::template!("spring/kafka_config_java.java"),
        before_boot: None,
        imports: &[],
        source_set: SourceSet::Main,
        class_name: kafka_config_class,
        template_class: kafka_config_class,
    },
    JavaFile {
        suffix: "non_retryable",
        template: crate::template!("spring/non_retryable_exception_java.java"),
        before_boot: None,
        imports: &[],
        source_set: SourceSet::Main,
        class_name: non_retryable_exception_class,
        template_class: non_retryable_exception_class,
    },
    JavaFile {
        suffix: "config_test",
        template: crate::template!("spring/kafka_config_test_java.java"),
        before_boot: None,
        imports: &[],
        source_set: SourceSet::Test,
        class_name: kafka_config_test_class,
        template_class: kafka_config_test_class,
    },
    JavaFile {
        suffix: "testcontainers_config",
        template: crate::template!("spring/kafka_testcontainers_config_java.java"),
        before_boot: None,
        imports: &[],
        source_set: SourceSet::Test,
        class_name: kafka_testcontainers_config_class,
        template_class: kafka_testcontainers_config_class,
    },
];

const MAIL_FILES: &[JavaFile] = &[
    JavaFile {
        suffix: "mailer",
        template: crate::template!("spring/mailer_java.java"),
        before_boot: None,
        imports: &[],
        source_set: SourceSet::Main,
        class_name: mailer_class,
        template_class: mailer_class,
    },
    JavaFile {
        suffix: "mailer_it",
        template: crate::template!("spring/mailer_it_java.java"),
        before_boot: None,
        imports: &[],
        source_set: SourceSet::IntegrationTest,
        class_name: mailer_it_class,
        template_class: mailer_class,
    },
];

const KAFKA_DEPENDENCIES: &[DependencySpec] = &[
    DependencySpec {
        group: "org.springframework.boot",
        artifact: "spring-boot-starter-kafka",
        version: None,
        scope: DependencyScope::Compile,
        spring_managed_version: true,
        only_when_build_exists: false,
        optional: false,
        boot: BootCondition::Spring,
    },
    DependencySpec {
        group: "io.micrometer",
        artifact: "micrometer-core",
        version: None,
        scope: DependencyScope::Compile,
        spring_managed_version: true,
        only_when_build_exists: false,
        optional: false,
        boot: BootCondition::Spring,
    },
    DependencySpec {
        group: "org.springframework.boot",
        artifact: "spring-boot-testcontainers",
        version: None,
        scope: DependencyScope::Test,
        spring_managed_version: true,
        only_when_build_exists: false,
        optional: false,
        boot: BootCondition::Spring,
    },
    DependencySpec {
        group: "org.testcontainers",
        artifact: "testcontainers-kafka",
        version: Some("2.0.5"),
        scope: DependencyScope::Test,
        spring_managed_version: false,
        only_when_build_exists: false,
        optional: false,
        boot: BootCondition::Spring,
    },
    DependencySpec {
        group: "org.testcontainers",
        artifact: "testcontainers-junit-jupiter",
        version: Some("2.0.5"),
        scope: DependencyScope::Test,
        spring_managed_version: false,
        only_when_build_exists: false,
        optional: false,
        boot: BootCondition::Spring,
    },
    DependencySpec {
        group: "org.awaitility",
        artifact: "awaitility",
        version: None,
        scope: DependencyScope::Test,
        spring_managed_version: true,
        only_when_build_exists: false,
        optional: false,
        boot: BootCondition::Spring,
    },
    DependencySpec {
        group: "org.apache.kafka",
        artifact: "kafka-clients",
        version: Some("4.1.0"),
        scope: DependencyScope::Compile,
        spring_managed_version: false,
        only_when_build_exists: false,
        optional: false,
        boot: BootCondition::Plain,
    },
    // **Declared on Spring too, and managed there.** The generated config
    // imports `org.apache.kafka.common.TopicPartition` directly, and reaching
    // it through `spring-kafka`'s transitive graph is a dependency this
    // project has and does not state -- one exclusion or one starter swap
    // away from a compile error in a file the reader did not write. The Boot
    // parent manages the version, so stating it costs nothing and pins
    // nothing.
    DependencySpec {
        group: "org.apache.kafka",
        artifact: "kafka-clients",
        version: None,
        scope: DependencyScope::Compile,
        spring_managed_version: true,
        only_when_build_exists: false,
        optional: false,
        boot: BootCondition::Spring,
    },
];

const MAIL_DEPENDENCIES: &[DependencySpec] = &[
    dependency(
        "org.springframework.boot",
        "spring-boot-starter-mail",
        None,
        DependencyScope::Compile,
        true,
        BootCondition::Spring,
    ),
    dependency(
        "org.springframework.boot",
        "spring-boot-starter-mail-test",
        None,
        DependencyScope::Test,
        true,
        BootCondition::AtLeast(4),
    ),
    dependency(
        "org.springframework.boot",
        "spring-boot-starter-test",
        None,
        DependencyScope::Test,
        true,
        BootCondition::Before(4),
    ),
    dependency(
        "org.awaitility",
        "awaitility",
        None,
        DependencyScope::Test,
        true,
        BootCondition::Spring,
    ),
    dependency(
        "org.testcontainers",
        "testcontainers",
        Some("2.0.5"),
        DependencyScope::Test,
        false,
        BootCondition::Spring,
    ),
    dependency(
        "org.testcontainers",
        "testcontainers-junit-jupiter",
        Some("2.0.5"),
        DependencyScope::Test,
        false,
        BootCondition::Spring,
    ),
];

const KAFKA_PROPERTIES: &[PropertySpec] = &[
    property("spring.kafka.bootstrap-servers", "localhost:9092"),
    property("spring.kafka.consumer.group-id", "{{project_group}}"),
    property("spring.kafka.consumer.auto-offset-reset", "earliest"),
    property(
        "spring.kafka.producer.value-serializer",
        "org.springframework.kafka.support.serializer.JacksonJsonSerializer",
    ),
    property(
        "spring.kafka.consumer.properties.spring.json.trusted.packages",
        "{{base_package}},{{base_package}}.*",
    ),
    property(
        "spring.kafka.consumer.properties.group.protocol",
        "consumer",
    ),
    property("spring.kafka.producer.acks", "all"),
    property(
        "spring.kafka.producer.properties.enable.idempotence",
        "true",
    ),
    property(
        "spring.kafka.consumer.value-deserializer",
        "org.springframework.kafka.support.serializer.ErrorHandlingDeserializer",
    ),
    property(
        "spring.kafka.consumer.properties.spring.deserializer.value.delegate.class",
        "org.springframework.kafka.support.serializer.JacksonJsonDeserializer",
    ),
];

const MAIL_PROPERTIES: &[PropertySpec] = &[
    property("spring.mail.host", "localhost"),
    property("spring.mail.port", "1025"),
    property("app.mail.from", "no-reply@example.com"),
];

const KAFKA_COMPOSE: &[ComposeService] = &[ComposeService {
    name: "kafka",
    marker: "kafka",
    body: "image: apache/kafka:4.1.0\nhostname: kafka\nports:\n  - \"9092:9092\"\nenvironment:\n  KAFKA_NODE_ID: 1\n  KAFKA_PROCESS_ROLES: broker,controller\n  KAFKA_LISTENERS: CONTROLLER://:29093,PLAINTEXT_HOST://:9092,PLAINTEXT://:19092\n  KAFKA_ADVERTISED_LISTENERS: PLAINTEXT_HOST://localhost:9092,PLAINTEXT://kafka:19092\n  KAFKA_LISTENER_SECURITY_PROTOCOL_MAP: CONTROLLER:PLAINTEXT,PLAINTEXT:PLAINTEXT,PLAINTEXT_HOST:PLAINTEXT\n  KAFKA_INTER_BROKER_LISTENER_NAME: PLAINTEXT\n  KAFKA_CONTROLLER_LISTENER_NAMES: CONTROLLER\n  KAFKA_CONTROLLER_QUORUM_VOTERS: 1@kafka:29093\n  CLUSTER_ID: 4L6g3nShT-eMCtK--X86sw\n  KAFKA_OFFSETS_TOPIC_REPLICATION_FACTOR: 1\n  KAFKA_TRANSACTION_STATE_LOG_REPLICATION_FACTOR: 1\n  KAFKA_TRANSACTION_STATE_LOG_MIN_ISR: 1\n  KAFKA_GROUP_INITIAL_REBALANCE_DELAY_MS: 0\n  KAFKA_LOG_DIRS: /tmp/kraft-combined-logs\nhealthcheck:\n  test: [\"CMD-SHELL\", \"/opt/kafka/bin/kafka-topics.sh --bootstrap-server localhost:9092 --list\"]\n  interval: 5s\n  timeout: 10s\n  retries: 10",
}];

const MAIL_COMPOSE: &[ComposeService] = &[ComposeService {
    name: "mailpit",
    marker: "mail",
    body: "image: axllent/mailpit:v1.21\nenvironment:\n  MP_SMTP_AUTH_ACCEPT_ANY: \"true\"\n  MP_SMTP_AUTH_ALLOW_INSECURE: \"true\"\n  MP_POP3_AUTH: user:pass\nports:\n  - \"1025:1025\"\n  - \"1110:1110\"\n  - \"8025:8025\"",
}];

const KAFKA_PACKAGE_OVERRIDES: &[PackageOverride] = &[PackageOverride {
    suffix: "testcontainers_config",
    project_subpackage: Package::Base,
}];

pub(super) const KAFKA_PACK: Pack = Pack {
    substitutions: NO_SUBSTITUTIONS,
    fragments: NO_FRAGMENTS,
    files: KAFKA_FILES,
    files_when: BootCondition::Spring,
    resources: NO_RESOURCES,
    dependencies: KAFKA_DEPENDENCIES,
    properties: KAFKA_PROPERTIES,
    compose_services: KAFKA_COMPOSE,
    build_features: NO_BUILD_FEATURES,
    default_package: messaging_package,
    package_overrides: KAFKA_PACKAGE_OVERRIDES,
    minimum_boot: None,
};

pub(super) const MAIL_PACK: Pack = Pack {
    substitutions: &[("image", "axllent/mailpit:v1.21")],
    fragments: NO_FRAGMENTS,
    files: MAIL_FILES,
    files_when: BootCondition::Spring,
    resources: NO_RESOURCES,
    dependencies: MAIL_DEPENDENCIES,
    properties: MAIL_PROPERTIES,
    compose_services: MAIL_COMPOSE,
    build_features: NO_BUILD_FEATURES,
    default_package: application_package,
    package_overrides: NO_PACKAGE_OVERRIDES,
    minimum_boot: None,
};

const fn dependency(
    group: &'static str,
    artifact: &'static str,
    version: Option<&'static str>,
    scope: DependencyScope,
    spring_managed_version: bool,
    boot: BootCondition,
) -> DependencySpec {
    DependencySpec {
        group,
        artifact,
        version,
        scope,
        spring_managed_version,
        only_when_build_exists: false,
        optional: false,
        boot,
    }
}

const fn property(key: &'static str, value: &'static str) -> PropertySpec {
    PropertySpec {
        key,
        value,
        target: SettingTarget::Main,
        boot: BootCondition::Spring,
    }
}

fn messaging_package(model: &AppModel) -> String {
    model.project.package_for(Package::Messaging)
}

fn application_package(model: &AppModel) -> String {
    model.project.base_package.clone()
}

fn kafka_config_class(_: &Capability) -> String {
    "KafkaConfig".to_string()
}

fn non_retryable_exception_class(_: &Capability) -> String {
    "NonRetryableException".to_string()
}

fn kafka_config_test_class(_: &Capability) -> String {
    "KafkaConfigTest".to_string()
}

fn kafka_testcontainers_config_class(_: &Capability) -> String {
    "KafkaTestcontainersConfig".to_string()
}

fn mailer_class(_: &Capability) -> String {
    "Mailer".to_string()
}

fn mailer_it_class(_: &Capability) -> String {
    "MailerIT".to_string()
}
