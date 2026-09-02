//! Spring-specific declarative capability data.
//!
//! The generic pack emitter owns merge identity, ejection, dependency
//! reconciliation, and property reconciliation. This module only declares the
//! Spring projections, including their version predicates.

use super::*;
use jails_model::Package;

mod api;
mod security;

pub(super) use api::{API_FILES, API_FRAGMENTS};
pub(super) use security::SECURITY_PACK;

/// The `api` capability's own files, as opposed to the per-operation adapters
/// `emit_http` writes.
///
/// Without these a project has controllers and no way to describe a
/// failure: `ApiException` is the sealed set the advice switches over, and the
/// switch has no `default` -- so a new variant stops the build rather than
/// quietly becoming a 500.
const ACTUATOR_FILES: &[JavaFile<Capability>] = &[JavaFile {
    role: "endpoints_test",
    template: crate::template!("spring/actuator_test_java.java"),
    before_boot: None,
    imports: &[],
    only_when: None,
    source_set: SourceSet::Test,
    placement: Placement::Default,
    ejectable: true,
    class: Naming::Fixed("ActuatorEndpointsTest"),
    template_class: Naming::Fixed("ActuatorEndpointsTest"),
}];

const CACHE_FILES: &[JavaFile<Capability>] = &[
    JavaFile {
        role: "config",
        template: crate::template!("spring/cache_config_java.java"),
        before_boot: None,
        imports: &[],
        only_when: None,
        source_set: SourceSet::Main,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::Fixed("CacheConfig"),
        template_class: Naming::Fixed("CacheConfig"),
    },
    JavaFile {
        role: "test",
        template: crate::template!("spring/cache_test_java.java"),
        before_boot: None,
        imports: &[],
        only_when: None,
        source_set: SourceSet::Test,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::Fixed("CacheConfigTest"),
        template_class: Naming::Fixed("CacheConfigTest"),
    },
];

const CORS_FILES: &[JavaFile<Capability>] = &[
    JavaFile {
        role: "config",
        template: crate::template!("spring/cors_config_java.java"),
        before_boot: None,
        imports: &[],
        only_when: None,
        source_set: SourceSet::Main,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::Fixed("CorsConfig"),
        template_class: Naming::Fixed("CorsConfig"),
    },
    JavaFile {
        role: "test",
        template: crate::template!("spring/cors_config_test_java.java"),
        before_boot: Some((
            4,
            crate::template!("spring/cors_config_test_classic_java.java"),
        )),
        imports: &[Import::Moved(AUTOCONFIGURE_MOCKMVC)],
        only_when: None,
        source_set: SourceSet::Test,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::Fixed("CorsConfigTest"),
        template_class: Naming::Fixed("CorsConfigTest"),
    },
];

const OBSERVABILITY_FILES: &[JavaFile<Capability>] = &[
    JavaFile {
        role: "metrics_config",
        template: crate::template!("spring/metrics_config_java.java"),
        before_boot: None,
        imports: &[Import::Moved(METER_REGISTRY_CUSTOMIZER)],
        only_when: None,
        source_set: SourceSet::Main,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::Fixed("MetricsConfig"),
        template_class: Naming::Fixed("MetricsConfig"),
    },
    JavaFile {
        role: "app_metrics",
        template: crate::template!("spring/app_metrics_java.java"),
        before_boot: None,
        imports: &[],
        only_when: None,
        source_set: SourceSet::Main,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::Fixed("AppMetrics"),
        template_class: Naming::Fixed("AppMetrics"),
    },
    JavaFile {
        role: "app_metrics_test",
        template: crate::template!("spring/app_metrics_test_java.java"),
        before_boot: None,
        imports: &[],
        only_when: None,
        source_set: SourceSet::Test,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::Fixed("AppMetricsTest"),
        template_class: Naming::Fixed("AppMetricsTest"),
    },
    JavaFile {
        role: "prometheus_scrape_test",
        template: crate::template!("spring/prometheus_scrape_test_java.java"),
        before_boot: None,
        imports: &[],
        only_when: None,
        source_set: SourceSet::Test,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::Fixed("PrometheusScrapeTest"),
        template_class: Naming::Fixed("PrometheusScrapeTest"),
    },
];

const SSE_FILES: &[JavaFile<Capability>] = &[
    JavaFile {
        role: "hub",
        template: crate::template!("spring/sse_hub_java.java"),
        before_boot: None,
        imports: &[],
        only_when: None,
        source_set: SourceSet::Main,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::Fixed("EventHub"),
        template_class: Naming::Fixed("Event"),
    },
    JavaFile {
        role: "scheduling",
        template: crate::template!("spring/scheduling_config_java.java"),
        before_boot: None,
        imports: &[],
        only_when: None,
        source_set: SourceSet::Main,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::Fixed("SchedulingConfig"),
        template_class: Naming::Fixed("SchedulingConfig"),
    },
    JavaFile {
        role: "controller",
        template: crate::template!("spring/sse_controller_java.java"),
        before_boot: None,
        // The controller holds the hub, and `package_overrides` files it under
        // `web` while the hub stays in the base package.
        imports: &[Import::Own("EventHub")],
        only_when: None,
        source_set: SourceSet::Main,
        placement: Placement::Layer(Package::Web),
        ejectable: true,
        class: Naming::Fixed("EventStreamController"),
        template_class: Naming::Fixed("Event"),
    },
    JavaFile {
        role: "hub_test",
        template: crate::template!("spring/sse_hub_test_java.java"),
        before_boot: None,
        imports: &[],
        only_when: None,
        source_set: SourceSet::Test,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::Fixed("EventHubTest"),
        template_class: Naming::Fixed("Event"),
    },
];

const REDIS_FILES: &[JavaFile<Capability>] = &[
    JavaFile {
        role: "store",
        template: crate::template!("spring/key_value_store_java.java"),
        before_boot: None,
        imports: &[],
        only_when: None,
        source_set: SourceSet::Main,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::Fixed("KeyValueStore"),
        template_class: Naming::Fixed("KeyValueStore"),
    },
    JavaFile {
        role: "store_it",
        template: crate::template!("spring/key_value_store_it_java.java"),
        before_boot: None,
        imports: &[],
        only_when: None,
        source_set: SourceSet::IntegrationTest,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::Fixed("KeyValueStoreIT"),
        template_class: Naming::Fixed("KeyValueStore"),
    },
];

const ACTUATOR_DEPENDENCIES: &[DependencySpec] = &[DependencySpec {
    group: "org.springframework.boot",
    artifact: "spring-boot-starter-actuator",
    version: None,
    scope: DependencyScope::Compile,
    spring_managed_version: true,
    only_when_build_exists: false,
    optional: false,
    boot: BootCondition::Any,
}];

const CACHE_DEPENDENCIES: &[DependencySpec] = &[
    DependencySpec {
        group: "org.springframework.boot",
        artifact: "spring-boot-starter-cache",
        version: None,
        scope: DependencyScope::Compile,
        spring_managed_version: true,
        only_when_build_exists: false,
        optional: false,
        boot: BootCondition::Any,
    },
    DependencySpec {
        group: "com.github.ben-manes.caffeine",
        artifact: "caffeine",
        version: None,
        scope: DependencyScope::Compile,
        spring_managed_version: true,
        only_when_build_exists: false,
        optional: false,
        boot: BootCondition::Any,
    },
];

const CORS_DEPENDENCIES: &[DependencySpec] = &[DependencySpec {
    group: "org.springframework.boot",
    artifact: "spring-boot-starter-webmvc-test",
    version: None,
    scope: DependencyScope::Test,
    spring_managed_version: true,
    only_when_build_exists: false,
    optional: false,
    boot: BootCondition::AtLeast(4),
}];

const OBSERVABILITY_DEPENDENCIES: &[DependencySpec] = &[
    DependencySpec {
        group: "org.springframework.boot",
        artifact: "spring-boot-starter-actuator",
        version: None,
        scope: DependencyScope::Compile,
        spring_managed_version: true,
        only_when_build_exists: false,
        optional: false,
        boot: BootCondition::Any,
    },
    DependencySpec {
        group: "io.micrometer",
        artifact: "micrometer-registry-prometheus",
        version: None,
        scope: DependencyScope::Compile,
        spring_managed_version: true,
        only_when_build_exists: false,
        optional: false,
        boot: BootCondition::Any,
    },
];

const SSE_DEPENDENCIES: &[DependencySpec] = &[DependencySpec {
    group: "org.springframework.boot",
    artifact: "spring-boot-starter-web",
    version: None,
    scope: DependencyScope::Compile,
    spring_managed_version: true,
    only_when_build_exists: false,
    optional: false,
    boot: BootCondition::Any,
}];

const REDIS_DEPENDENCIES: &[DependencySpec] = &[
    DependencySpec {
        group: "org.springframework.boot",
        artifact: "spring-boot-starter-data-redis",
        version: None,
        scope: DependencyScope::Compile,
        spring_managed_version: true,
        only_when_build_exists: false,
        optional: false,
        boot: BootCondition::Any,
    },
    DependencySpec {
        group: "org.testcontainers",
        artifact: "testcontainers",
        version: Some("2.0.5"),
        scope: DependencyScope::Test,
        spring_managed_version: false,
        only_when_build_exists: false,
        optional: false,
        boot: BootCondition::Any,
    },
    DependencySpec {
        group: "org.springframework.boot",
        artifact: "spring-boot-testcontainers",
        version: None,
        scope: DependencyScope::Test,
        spring_managed_version: true,
        only_when_build_exists: false,
        optional: false,
        boot: BootCondition::Any,
    },
];

const ACTUATOR_PROPERTIES: &[PropertySpec] = &[
    PropertySpec {
        key: "management.endpoints.web.exposure.include",
        value: "health,info,prometheus,threaddump",
        target: SettingTarget::Main,
        boot: BootCondition::Any,
    },
    PropertySpec {
        key: "management.server.port",
        value: "8081",
        target: SettingTarget::Main,
        boot: BootCondition::Any,
    },
    PropertySpec {
        key: "management.endpoints.web.base-path",
        value: "/management",
        target: SettingTarget::Main,
        boot: BootCondition::Any,
    },
    PropertySpec {
        key: "management.endpoint.health.cache.time-to-live",
        value: "5s",
        target: SettingTarget::Main,
        boot: BootCondition::Any,
    },
    PropertySpec {
        key: "management.endpoint.health.group.liveness.include",
        value: "ping",
        target: SettingTarget::Main,
        boot: BootCondition::Any,
    },
    PropertySpec {
        key: "management.endpoint.health.group.readiness.include",
        value: "ping",
        target: SettingTarget::Main,
        boot: BootCondition::Any,
    },
    PropertySpec {
        key: "management.endpoint.health.show-details",
        value: "when-authorized",
        target: SettingTarget::Main,
        boot: BootCondition::Any,
    },
    PropertySpec {
        key: "info.app.name",
        value: "@project.name@",
        target: SettingTarget::Main,
        boot: BootCondition::Any,
    },
    PropertySpec {
        key: "info.app.version",
        value: "@project.version@",
        target: SettingTarget::Main,
        boot: BootCondition::Any,
    },
];

const CACHE_PROPERTIES: &[PropertySpec] = &[
    PropertySpec {
        key: "spring.cache.type",
        value: "caffeine",
        target: SettingTarget::Main,
        boot: BootCondition::Any,
    },
    PropertySpec {
        key: "spring.cache.caffeine.spec",
        value: "maximumSize=1000,expireAfterWrite=60s",
        target: SettingTarget::Main,
        boot: BootCondition::Any,
    },
];

const CORS_PROPERTIES: &[PropertySpec] = &[PropertySpec {
    key: "app.cors.allowed-origins",
    value: "https://example.invalid",
    target: SettingTarget::Main,
    boot: BootCondition::Any,
}];

const OBSERVABILITY_PROPERTIES: &[PropertySpec] = &[
    property(
        "management.endpoints.web.exposure.include",
        "health,info,prometheus,threaddump",
    ),
    property("management.server.port", "8081"),
    property("management.endpoints.web.base-path", "/management"),
    property("management.endpoint.health.cache.time-to-live", "5s"),
    property("management.endpoint.health.group.liveness.include", "ping"),
    property("management.endpoint.health.group.readiness.include", "ping"),
    property("management.endpoint.health.show-details", "when-authorized"),
    property(
        "management.metrics.distribution.slo.http.server.requests",
        "100ms,250ms,500ms,1s,2s,5s,10s",
    ),
    property(
        "management.metrics.distribution.percentiles-histogram.http.server.requests",
        "false",
    ),
    property(
        "management.metrics.distribution.percentiles.http.server.requests",
        "0.5,0.9,0.95,0.99",
    ),
    property(
        "management.metrics.distribution.minimum-expected-value.http.server.requests",
        "1ms",
    ),
    property(
        "management.metrics.distribution.maximum-expected-value.http.server.requests",
        "10s",
    ),
    property("management.tracing.propagation.type", "w3c"),
    property("management.tracing.sampling.probability", "0.1"),
    property(
        "management.tracing.baggage.correlation.fields",
        "request-id",
    ),
    property("management.tracing.baggage.tag-fields", "request-id"),
    property("management.tracing.baggage.local-fields", "request-id"),
    property("server.tomcat.accesslog.enabled", "true"),
    property("server.tomcat.accesslog.directory", "/dev"),
    property("server.tomcat.accesslog.prefix", "stdout"),
    property("server.tomcat.accesslog.suffix", ""),
    property("server.tomcat.accesslog.file-date-format", ""),
    property("server.tomcat.accesslog.buffered", "false"),
    property("management.server.tomcat.accesslog.prefix", "stdout"),
];

const SSE_PROPERTIES: &[PropertySpec] = &[PropertySpec {
    key: "spring.task.scheduling.pool.size",
    value: "4",
    target: SettingTarget::Main,
    boot: BootCondition::Any,
}];

const REDIS_PROPERTIES: &[PropertySpec] = &[
    property("spring.data.redis.host", "localhost"),
    property("spring.data.redis.port", "6379"),
    property("app.redis.default-ttl", "PT10M"),
];

const REDIS_COMPOSE: &[ComposeService] = &[ComposeService {
    name: "redis",
    marker: "redis",
    body: "image: redis:7-alpine\nports:\n  - \"6379:6379\"\nhealthcheck:\n  test: [\"CMD\", \"redis-cli\", \"ping\"]\n  interval: 2s\n  timeout: 5s\n  retries: 10",
}];

/// A setting that only means something under Spring Boot.
///
/// Every `spring.*` key is one: `storage postgres` works on a plain Maven
/// project -- `java.sql` is in the JDK -- and writing Boot's datasource keys
/// into its `application.properties` would leave a file full of settings
/// nothing reads.
pub(super) const fn spring_property(key: &'static str, value: &'static str) -> PropertySpec {
    PropertySpec {
        key,
        value,
        target: SettingTarget::Main,
        boot: BootCondition::Spring,
    }
}

pub(super) const fn property(key: &'static str, value: &'static str) -> PropertySpec {
    PropertySpec {
        key,
        value,
        target: SettingTarget::Main,
        boot: BootCondition::Any,
    }
}

pub(super) const ACTUATOR_PACK: Recipe<Capability> = Recipe {
    substitutions: NO_SUBSTITUTIONS,
    fragments: NO_FRAGMENTS,
    keys: &[],
    requires: &[],
    files: ACTUATOR_FILES,
    files_when: BootCondition::Any,
    resources: NO_RESOURCES,
    dependencies: ACTUATOR_DEPENDENCIES,
    properties: ACTUATOR_PROPERTIES,
    compose_services: NO_COMPOSE_SERVICES,
    build_features: NO_BUILD_FEATURES,
    default_package: root_package,
    minimum_boot: None,
    pass: "",
};

pub(super) const CACHE_PACK: Recipe<Capability> = Recipe {
    substitutions: NO_SUBSTITUTIONS,
    fragments: NO_FRAGMENTS,
    keys: &[],
    requires: &[],
    files: CACHE_FILES,
    files_when: BootCondition::Any,
    resources: NO_RESOURCES,
    dependencies: CACHE_DEPENDENCIES,
    properties: CACHE_PROPERTIES,
    compose_services: NO_COMPOSE_SERVICES,
    build_features: NO_BUILD_FEATURES,
    default_package: root_package,
    minimum_boot: None,
    pass: "",
};

/// `k8s` emits its chart through `project_file.rs`; this is the one setting
/// that belongs in the application rather than in the chart.
///
/// Kubernetes supplies `POD_NAME` from `metadata.name` -- the configmap in the
/// chart is what puts it in the environment -- so this tags every replica
/// separately. Without it a burn-rate alert cannot tell which pod is failing,
/// which is the question an alert exists to answer.
pub(super) const K8S_PACK: Recipe<Capability> = Recipe {
    substitutions: NO_SUBSTITUTIONS,
    fragments: NO_FRAGMENTS,
    keys: &[],
    requires: &[],
    files: &[],
    files_when: BootCondition::Any,
    resources: NO_RESOURCES,
    dependencies: &[],
    properties: K8S_PROPERTIES,
    compose_services: NO_COMPOSE_SERVICES,
    build_features: NO_BUILD_FEATURES,
    default_package: root_package,
    minimum_boot: None,
    pass: "",
};

const K8S_PROPERTIES: &[PropertySpec] = &[PropertySpec {
    key: "management.metrics.tags.pod.name",
    value: "${POD_NAME:unknown}",
    target: SettingTarget::Main,
    boot: BootCondition::Spring,
}];

pub(super) const API_PACK: Recipe<Capability> = Recipe {
    substitutions: NO_SUBSTITUTIONS,
    fragments: API_FRAGMENTS,
    keys: &[],
    requires: &[],
    files: API_FILES,
    files_when: BootCondition::Any,
    resources: NO_RESOURCES,
    dependencies: API_DEPENDENCIES,
    properties: NO_PROPERTIES,
    compose_services: NO_COMPOSE_SERVICES,
    build_features: NO_BUILD_FEATURES,
    default_package: api_package,
    // `ProblemDetail` is Framework 6, which is Boot 3.
    minimum_boot: Some((3, "ProblemDetail")),
    pass: "",
};

const API_DEPENDENCIES: &[DependencySpec] = &[DependencySpec {
    group: "org.springframework.boot",
    artifact: "spring-boot-starter-validation",
    version: None,
    scope: DependencyScope::Compile,
    spring_managed_version: true,
    only_when_build_exists: false,
    optional: false,
    boot: BootCondition::Spring,
}];

pub(super) const CORS_PACK: Recipe<Capability> = Recipe {
    substitutions: NO_SUBSTITUTIONS,
    fragments: NO_FRAGMENTS,
    keys: &[],
    requires: &[],
    files: CORS_FILES,
    files_when: BootCondition::Any,
    resources: NO_RESOURCES,
    dependencies: CORS_DEPENDENCIES,
    properties: CORS_PROPERTIES,
    compose_services: NO_COMPOSE_SERVICES,
    build_features: NO_BUILD_FEATURES,
    default_package: root_package,
    minimum_boot: None,
    pass: "",
};

pub(super) const OBSERVABILITY_PACK: Recipe<Capability> = Recipe {
    substitutions: NO_SUBSTITUTIONS,
    fragments: NO_FRAGMENTS,
    keys: &[],
    requires: &[],
    files: OBSERVABILITY_FILES,
    files_when: BootCondition::Any,
    resources: NO_RESOURCES,
    dependencies: OBSERVABILITY_DEPENDENCIES,
    properties: OBSERVABILITY_PROPERTIES,
    compose_services: NO_COMPOSE_SERVICES,
    build_features: NO_BUILD_FEATURES,
    default_package: root_package,
    minimum_boot: None,
    pass: "",
};

pub(super) const SSE_PACK: Recipe<Capability> = Recipe {
    substitutions: &[("path", "events")],
    fragments: NO_FRAGMENTS,
    keys: &[],
    requires: &[],
    files: SSE_FILES,
    files_when: BootCondition::Any,
    resources: NO_RESOURCES,
    dependencies: SSE_DEPENDENCIES,
    properties: SSE_PROPERTIES,
    compose_services: NO_COMPOSE_SERVICES,
    build_features: NO_BUILD_FEATURES,
    default_package: root_package,
    minimum_boot: None,
    pass: "",
};

pub(super) const REDIS_PACK: Recipe<Capability> = Recipe {
    substitutions: &[("REDIS_IMAGE", "redis:7-alpine")],
    fragments: NO_FRAGMENTS,
    keys: &[],
    requires: &[],
    files: REDIS_FILES,
    files_when: BootCondition::Any,
    resources: NO_RESOURCES,
    dependencies: REDIS_DEPENDENCIES,
    properties: REDIS_PROPERTIES,
    compose_services: REDIS_COMPOSE,
    build_features: NO_BUILD_FEATURES,
    default_package: adapters_package,
    minimum_boot: None,
    pass: "",
};
