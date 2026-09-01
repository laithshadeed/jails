//! Spring-specific declarative capability data.
//!
//! The generic pack emitter owns merge identity, ejection, dependency
//! reconciliation, and property reconciliation. This module only declares the
//! Spring projections, including their version predicates.

use super::*;
use jails_model::Package;

mod names;
use names::*;

/// The `api` capability's own files, as opposed to the per-operation adapters
/// `emit_http` writes.
///
/// Without these a canonical project got controllers and no way to describe a
/// failure: `ApiException` is the sealed set the advice switches over, and the
/// switch has no `default` -- so a new variant stops the build rather than
/// quietly becoming a 500.
const API_FILES: &[JavaFile] = &[
    JavaFile {
        suffix: "exception",
        template: include_str!("../../../../templates/spring/api_exception_java.java"),
        before_boot: None,
        source_set: SourceSet::Main,
        class_name: api_exception_class,
        template_class: api_exception_class,
    },
    JavaFile {
        suffix: "exception_handler",
        template: include_str!("../../../../templates/spring/api_exception_handler_java.java"),
        before_boot: None,
        source_set: SourceSet::Main,
        class_name: api_exception_handler_class,
        template_class: api_exception_handler_class,
    },
    JavaFile {
        suffix: "exception_handler_test",
        // No classic form: `api` refuses below Boot 3, its advice being built
        // on Framework 6's `ProblemDetail`.
        template: include_str!("../../../../templates/spring/api_exception_handler_test_java.java"),
        before_boot: None,
        source_set: SourceSet::Test,
        class_name: api_exception_handler_test_class,
        template_class: api_exception_handler_test_class,
    },
];

const API_FRAGMENTS: &[Fragment] = &[
    Fragment {
        key: "duplicate_key_import",
        when_capability: "db",
        body: "import org.springframework.dao.DuplicateKeyException;",
    },
    Fragment {
        key: "duplicate_key_handler",
        when_capability: "db",
        body: DUPLICATE_KEY_HANDLER,
    },
    Fragment {
        key: "duplicate_key_test",
        when_capability: "db",
        body: DUPLICATE_KEY_TEST,
    },
    Fragment {
        key: "duplicate_key_route",
        when_capability: "db",
        body: DUPLICATE_KEY_ROUTE,
    },
    Fragment {
        key: "precondition_import",
        when_capability: "db",
        body: "import org.springframework.dao.EmptyResultDataAccessException;\nimport org.springframework.dao.OptimisticLockingFailureException;",
    },
    Fragment {
        key: "precondition_handler",
        when_capability: "db",
        body: PRECONDITION_HANDLER,
    },
    Fragment {
        key: "precondition_test",
        when_capability: "db",
        body: PRECONDITION_TEST,
    },
    Fragment {
        key: "precondition_route",
        when_capability: "db",
        body: PRECONDITION_ROUTE,
    },
];

/// **Spring's own vocabulary, so nothing new has to be declared.** A
/// transition whose `If-Match` did not match raises
/// `OptimisticLockingFailureException` and one whose row is not there raises
/// `EmptyResultDataAccessException` -- both from `spring-dao`, both already on
/// the classpath the moment the JDBC starter is. Mapping them here rather than
/// in each controller is what keeps a generated controller free of HTTP status
/// arithmetic, and what makes a hand-written adapter get the same answer.
const PRECONDITION_HANDLER: &str = r#"
    /**
     * A precondition the caller stated and the row no longer satisfies.
     *
     * <p>412 rather than 409: the caller sent an `If-Match` and it did not
     * match, which is precisely what 412 means. A 500 here is the worse
     * failure it replaces -- alerting pages on it, client libraries retry it,
     * and the retry cannot succeed because the version has moved on.
     */
    @ExceptionHandler(OptimisticLockingFailureException.class)
    public ProblemDetail handleStalePrecondition(OptimisticLockingFailureException failure) {
        return ProblemDetail.forStatusAndDetail(
                HttpStatus.PRECONDITION_FAILED,
                "the resource has changed since the version you sent");
    }

    /**
     * A row the request named and the database does not have.
     *
     * <p>The detail says nothing about which row: an unauthenticated caller
     * learning that an id exists is the difference between 404 and 403, and
     * a generated handler is the wrong place to decide that.
     */
    @ExceptionHandler(EmptyResultDataAccessException.class)
    public ProblemDetail handleMissingRow(EmptyResultDataAccessException failure) {
        return ProblemDetail.forStatusAndDetail(HttpStatus.NOT_FOUND, "no such resource");
    }
"#;

const PRECONDITION_TEST: &str = r#"
    @Test
    void aStalePreconditionBecomesA412() {
        assertThat(mvc.get().uri("/boom/stale")).hasStatus(HttpStatus.PRECONDITION_FAILED);
    }

    @Test
    void aMissingRowBecomesA404() {
        assertThat(mvc.get().uri("/boom/missing")).hasStatus(HttpStatus.NOT_FOUND);
    }
"#;

const PRECONDITION_ROUTE: &str = r#"
        @GetMapping("/boom/stale")
        String stale() {
            throw new OptimisticLockingFailureException("version moved on");
        }

        @GetMapping("/boom/missing")
        String missing() {
            throw new EmptyResultDataAccessException(1);
        }
"#;

const DUPLICATE_KEY_HANDLER: &str = r#"
    /**
     * A unique constraint the database enforced, as the 409 it is.
     *
     * <p>Without this, a duplicate reaches the client as a 500 -- which is
     * what alerting pages on and what a client library retries, so one
     * duplicate becomes an incident and then a retry storm. The row was not
     * written and never will be; that is a conflict, not a server fault.
     *
     * <p>The detail deliberately does not name the column. Spring's message
     * carries the constraint name from the driver, which is a schema
     * identifier rather than anything a caller can act on -- and echoing it
     * tells an unauthenticated client the shape of your database.
     */
    @ExceptionHandler(DuplicateKeyException.class)
    public ProblemDetail handleDuplicateKey(DuplicateKeyException failure) {
        return ProblemDetail.forStatusAndDetail(
                HttpStatus.CONFLICT, "a resource with those values already exists");
    }
"#;

const DUPLICATE_KEY_TEST: &str = r#"
    @Test
    void aDuplicateKeyBecomesA409() {
        // The database rejected a unique constraint; that is a conflict, not
        // a server fault.
        assertThat(mvc.get().uri("/boom/duplicate")).hasStatus(HttpStatus.CONFLICT);
    }
"#;

const DUPLICATE_KEY_ROUTE: &str = r#"
        @GetMapping("/boom/duplicate")
        String duplicate() {
            throw new DuplicateKeyException("unique constraint violated");
        }
"#;

const ACTUATOR_FILES: &[JavaFile] = &[JavaFile {
    suffix: "endpoints_test",
    template: include_str!("../../../../templates/spring/actuator_test_java.java"),
    before_boot: None,
    source_set: SourceSet::Test,
    class_name: actuator_test_class,
    template_class: actuator_test_class,
}];

const CACHE_FILES: &[JavaFile] = &[
    JavaFile {
        suffix: "config",
        template: include_str!("../../../../templates/spring/cache_config_java.java"),
        before_boot: None,
        source_set: SourceSet::Main,
        class_name: cache_config_class,
        template_class: cache_config_class,
    },
    JavaFile {
        suffix: "test",
        template: include_str!("../../../../templates/spring/cache_test_java.java"),
        before_boot: None,
        source_set: SourceSet::Test,
        class_name: cache_test_class,
        template_class: cache_test_class,
    },
];

const CORS_FILES: &[JavaFile] = &[
    JavaFile {
        suffix: "config",
        template: include_str!("../../../../templates/spring/cors_config_java.java"),
        before_boot: None,
        source_set: SourceSet::Main,
        class_name: cors_config_class,
        template_class: cors_config_class,
    },
    JavaFile {
        suffix: "test",
        template: include_str!("../../../../templates/spring/cors_config_test_java.java"),
        before_boot: Some((
            4,
            include_str!("../../../../templates/spring/cors_config_test_classic_java.java"),
        )),
        source_set: SourceSet::Test,
        class_name: cors_test_class,
        template_class: cors_test_class,
    },
];

const OBSERVABILITY_FILES: &[JavaFile] = &[
    JavaFile {
        suffix: "metrics_config",
        template: include_str!("../../../../templates/spring/metrics_config_java.java"),
        before_boot: None,
        source_set: SourceSet::Main,
        class_name: metrics_config_class,
        template_class: metrics_config_class,
    },
    JavaFile {
        suffix: "app_metrics",
        template: include_str!("../../../../templates/spring/app_metrics_java.java"),
        before_boot: None,
        source_set: SourceSet::Main,
        class_name: app_metrics_class,
        template_class: app_metrics_class,
    },
    JavaFile {
        suffix: "app_metrics_test",
        template: include_str!("../../../../templates/spring/app_metrics_test_java.java"),
        before_boot: None,
        source_set: SourceSet::Test,
        class_name: app_metrics_test_class,
        template_class: app_metrics_test_class,
    },
    JavaFile {
        suffix: "prometheus_scrape_test",
        template: include_str!("../../../../templates/spring/prometheus_scrape_test_java.java"),
        before_boot: None,
        source_set: SourceSet::Test,
        class_name: prometheus_scrape_test_class,
        template_class: prometheus_scrape_test_class,
    },
];

const SECURITY_FILES: &[JavaFile] = &[
    JavaFile {
        suffix: "config",
        template: include_str!("../../../../templates/spring/security_config_java.java"),
        before_boot: None,
        source_set: SourceSet::Main,
        class_name: security_config_class,
        template_class: security_config_class,
    },
    JavaFile {
        suffix: "production_config",
        template: include_str!("../../../../templates/spring/production_security_config_java.java"),
        before_boot: None,
        source_set: SourceSet::Main,
        class_name: production_security_config_class,
        template_class: production_security_config_class,
    },
    JavaFile {
        suffix: "scope_authorizer",
        template: include_str!("../../../../templates/spring/scope_authorizer_java.java"),
        before_boot: None,
        source_set: SourceSet::Main,
        class_name: scope_authorizer_class,
        template_class: scope_authorizer_class,
    },
    JavaFile {
        suffix: "config_test",
        template: include_str!("../../../../templates/spring/security_test_java.java"),
        before_boot: None,
        source_set: SourceSet::Test,
        class_name: security_config_test_class,
        template_class: security_config_test_class,
    },
    JavaFile {
        suffix: "scope_authorizer_test",
        template: include_str!("../../../../templates/spring/scope_authorizer_test_java.java"),
        before_boot: None,
        source_set: SourceSet::Test,
        class_name: scope_authorizer_test_class,
        template_class: scope_authorizer_test_class,
    },
];

const SSE_FILES: &[JavaFile] = &[
    JavaFile {
        suffix: "hub",
        template: include_str!("../../../../templates/spring/sse_hub_java.java"),
        before_boot: None,
        source_set: SourceSet::Main,
        class_name: event_hub_class,
        template_class: event_name,
    },
    JavaFile {
        suffix: "scheduling",
        template: include_str!("../../../../templates/spring/scheduling_config_java.java"),
        before_boot: None,
        source_set: SourceSet::Main,
        class_name: scheduling_config_class,
        template_class: scheduling_config_class,
    },
    JavaFile {
        suffix: "controller",
        template: include_str!("../../../../templates/spring/sse_controller_java.java"),
        before_boot: None,
        source_set: SourceSet::Main,
        class_name: event_stream_controller_class,
        template_class: event_name,
    },
    JavaFile {
        suffix: "hub_test",
        template: include_str!("../../../../templates/spring/sse_hub_test_java.java"),
        before_boot: None,
        source_set: SourceSet::Test,
        class_name: event_hub_test_class,
        template_class: event_name,
    },
];

const REDIS_FILES: &[JavaFile] = &[
    JavaFile {
        suffix: "store",
        template: include_str!("../../../../templates/spring/key_value_store_java.java"),
        before_boot: None,
        source_set: SourceSet::Main,
        class_name: key_value_store_class,
        template_class: key_value_store_class,
    },
    JavaFile {
        suffix: "store_it",
        template: include_str!("../../../../templates/spring/key_value_store_it_java.java"),
        before_boot: None,
        source_set: SourceSet::IntegrationTest,
        class_name: key_value_store_it_class,
        template_class: key_value_store_class,
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

const SECURITY_DEPENDENCIES: &[DependencySpec] = &[
    DependencySpec {
        group: "org.springframework.boot",
        artifact: "spring-boot-starter-security",
        version: None,
        scope: DependencyScope::Compile,
        spring_managed_version: true,
        only_when_build_exists: false,
        optional: false,
        boot: BootCondition::Any,
    },
    DependencySpec {
        group: "org.springframework.boot",
        artifact: "spring-boot-starter-oauth2-resource-server",
        version: None,
        scope: DependencyScope::Compile,
        spring_managed_version: true,
        only_when_build_exists: false,
        optional: false,
        boot: BootCondition::Any,
    },
    DependencySpec {
        group: "org.springframework.security",
        artifact: "spring-security-test",
        version: None,
        scope: DependencyScope::Test,
        spring_managed_version: true,
        only_when_build_exists: false,
        optional: false,
        boot: BootCondition::Any,
    },
    DependencySpec {
        group: "org.springframework.boot",
        artifact: "spring-boot-starter-webmvc-test",
        version: None,
        scope: DependencyScope::Test,
        spring_managed_version: true,
        only_when_build_exists: false,
        optional: false,
        boot: BootCondition::AtLeast(4),
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

const SSE_PACKAGE_OVERRIDES: &[PackageOverride] = &[PackageOverride {
    suffix: "controller",
    project_subpackage: Package::Web,
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

pub(super) const ACTUATOR_PACK: Pack = Pack {
    fragments: NO_FRAGMENTS,
    files: ACTUATOR_FILES,
    files_when: BootCondition::Any,
    resources: NO_RESOURCES,
    dependencies: ACTUATOR_DEPENDENCIES,
    properties: ACTUATOR_PROPERTIES,
    compose_services: NO_COMPOSE_SERVICES,
    build_features: NO_BUILD_FEATURES,
    default_package: root_package,
    package_overrides: NO_PACKAGE_OVERRIDES,
    minimum_boot: None,
};

pub(super) const CACHE_PACK: Pack = Pack {
    fragments: NO_FRAGMENTS,
    files: CACHE_FILES,
    files_when: BootCondition::Any,
    resources: NO_RESOURCES,
    dependencies: CACHE_DEPENDENCIES,
    properties: CACHE_PROPERTIES,
    compose_services: NO_COMPOSE_SERVICES,
    build_features: NO_BUILD_FEATURES,
    default_package: root_package,
    package_overrides: NO_PACKAGE_OVERRIDES,
    minimum_boot: None,
};

/// `k8s` emits its chart through `project_file.rs`; this is the one setting
/// that belongs in the application rather than in the chart.
///
/// Kubernetes supplies `POD_NAME` from `metadata.name` -- the configmap in the
/// chart is what puts it in the environment -- so this tags every replica
/// separately. Without it a burn-rate alert cannot tell which pod is failing,
/// which is the question an alert exists to answer.
pub(super) const K8S_PACK: Pack = Pack {
    fragments: NO_FRAGMENTS,
    files: &[],
    files_when: BootCondition::Any,
    resources: NO_RESOURCES,
    dependencies: &[],
    properties: K8S_PROPERTIES,
    compose_services: NO_COMPOSE_SERVICES,
    build_features: NO_BUILD_FEATURES,
    default_package: root_package,
    package_overrides: NO_PACKAGE_OVERRIDES,
    minimum_boot: None,
};

const K8S_PROPERTIES: &[PropertySpec] = &[PropertySpec {
    key: "management.metrics.tags.pod.name",
    value: "${POD_NAME:unknown}",
    target: SettingTarget::Main,
    boot: BootCondition::Spring,
}];

pub(super) const API_PACK: Pack = Pack {
    fragments: API_FRAGMENTS,
    files: API_FILES,
    files_when: BootCondition::Any,
    resources: NO_RESOURCES,
    dependencies: API_DEPENDENCIES,
    properties: NO_PROPERTIES,
    compose_services: NO_COMPOSE_SERVICES,
    build_features: NO_BUILD_FEATURES,
    default_package: api_package,
    package_overrides: NO_PACKAGE_OVERRIDES,
    // `ProblemDetail` is Framework 6, which is Boot 3.
    minimum_boot: Some((3, "ProblemDetail")),
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

pub(super) const CORS_PACK: Pack = Pack {
    fragments: NO_FRAGMENTS,
    files: CORS_FILES,
    files_when: BootCondition::Any,
    resources: NO_RESOURCES,
    dependencies: CORS_DEPENDENCIES,
    properties: CORS_PROPERTIES,
    compose_services: NO_COMPOSE_SERVICES,
    build_features: NO_BUILD_FEATURES,
    default_package: root_package,
    package_overrides: NO_PACKAGE_OVERRIDES,
    minimum_boot: None,
};

pub(super) const OBSERVABILITY_PACK: Pack = Pack {
    fragments: NO_FRAGMENTS,
    files: OBSERVABILITY_FILES,
    files_when: BootCondition::Any,
    resources: NO_RESOURCES,
    dependencies: OBSERVABILITY_DEPENDENCIES,
    properties: OBSERVABILITY_PROPERTIES,
    compose_services: NO_COMPOSE_SERVICES,
    build_features: NO_BUILD_FEATURES,
    default_package: root_package,
    package_overrides: NO_PACKAGE_OVERRIDES,
    minimum_boot: None,
};

pub(super) const SECURITY_PACK: Pack = Pack {
    fragments: NO_FRAGMENTS,
    files: SECURITY_FILES,
    files_when: BootCondition::Any,
    resources: NO_RESOURCES,
    dependencies: SECURITY_DEPENDENCIES,
    properties: NO_PROPERTIES,
    compose_services: NO_COMPOSE_SERVICES,
    build_features: NO_BUILD_FEATURES,
    default_package: root_package,
    package_overrides: NO_PACKAGE_OVERRIDES,
    minimum_boot: Some((3, "requestMatchers")),
};

pub(super) const SSE_PACK: Pack = Pack {
    fragments: NO_FRAGMENTS,
    files: SSE_FILES,
    files_when: BootCondition::Any,
    resources: NO_RESOURCES,
    dependencies: SSE_DEPENDENCIES,
    properties: SSE_PROPERTIES,
    compose_services: NO_COMPOSE_SERVICES,
    build_features: NO_BUILD_FEATURES,
    default_package: root_package,
    package_overrides: SSE_PACKAGE_OVERRIDES,
    minimum_boot: None,
};

pub(super) const REDIS_PACK: Pack = Pack {
    fragments: NO_FRAGMENTS,
    files: REDIS_FILES,
    files_when: BootCondition::Any,
    resources: NO_RESOURCES,
    dependencies: REDIS_DEPENDENCIES,
    properties: REDIS_PROPERTIES,
    compose_services: REDIS_COMPOSE,
    build_features: NO_BUILD_FEATURES,
    default_package: adapters_package,
    package_overrides: NO_PACKAGE_OVERRIDES,
    minimum_boot: None,
};
