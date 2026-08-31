//! The two capability packs the storage axis materializes.
//!
//! Split out of `spring.rs` by the secret every pack here shares and no other
//! pack does: **a reader never names one.** `cap db` and `cap h2` are not
//! spellings JDL v1 accepts — `storage postgres` and `storage h2` are, and the
//! linker materializes the capability from the axis. So these two are the only
//! packs whose identity comes from the `app` block rather than from a
//! declaration, which is also why their absence was invisible for so long.

use super::spring::property;
use super::*;
use jails_model::Package;

fn h2_test_class(_: &Capability) -> String {
    "H2DatabaseTest".to_string()
}

const H2_FILES: &[JavaFile] = &[JavaFile {
    suffix: "test",
    template: include_str!("../../../../templates/spring/h2_database_test_java.java"),
    before_boot: None,
    source_set: SourceSet::Test,
    class_name: h2_test_class,
    template_class: h2_test_class,
}];

const H2_DEPENDENCIES: &[DependencySpec] = &[
    DependencySpec {
        group: "org.springframework.boot",
        artifact: "spring-boot-starter-jdbc",
        version: None,
        scope: DependencyScope::Compile,
        spring_managed_version: true,
        only_when_build_exists: false,
        boot: BootCondition::Any,
    },
    DependencySpec {
        group: "com.h2database",
        artifact: "h2",
        version: None,
        scope: DependencyScope::Runtime,
        spring_managed_version: true,
        only_when_build_exists: false,
        boot: BootCondition::Any,
    },
    DependencySpec {
        group: "org.springframework.boot",
        artifact: "spring-boot-h2console",
        version: None,
        scope: DependencyScope::Compile,
        spring_managed_version: true,
        only_when_build_exists: false,
        boot: BootCondition::AtLeast(4),
    },
];

const H2_PROPERTIES: &[PropertySpec] = &[
    PropertySpec {
        key: "spring.datasource.url",
        value: "jdbc:h2:file:./data/app;AUTO_SERVER=TRUE",
        target: SettingTarget::Main,
        boot: BootCondition::Any,
    },
    PropertySpec {
        key: "spring.h2.console.enabled",
        value: "true",
        target: SettingTarget::Main,
        boot: BootCondition::Any,
    },
    PropertySpec {
        key: "spring.h2.console.path",
        value: "/h2-console",
        target: SettingTarget::Main,
        boot: BootCondition::Any,
    },
    PropertySpec {
        key: "spring.persistence.exceptiontranslation.enabled",
        value: "false",
        target: SettingTarget::Main,
        boot: BootCondition::AtLeast(4),
    },
    PropertySpec {
        key: "spring.dao.exceptiontranslation.enabled",
        value: "false",
        target: SettingTarget::Main,
        boot: BootCondition::Before(4),
    },
    PropertySpec {
        key: "spring.datasource.url",
        value: "jdbc:h2:mem:test;DB_CLOSE_DELAY=-1",
        target: SettingTarget::Test,
        boot: BootCondition::Any,
    },
];

/// PostgreSQL's *test* half, which is the half that was missing.
///
/// The main half -- the JDBC starter, the driver and Flyway -- comes from
/// `storage::storage_dependencies`, because the storage axis decides it. What
/// did not exist was any of this, and its absence is the failure `CLAUDE.md`
/// records at length: once `spring-boot-starter-jdbc` is present, JDBC
/// auto-configuration demands a `DataSource` for **every** `@SpringBootTest`,
/// including the `contextLoads` test that shipped with the project. So a
/// canonical project that declared `storage postgres` and touched nothing else
/// failed `mvn verify` on a test nobody wrote, with "Failed to determine a
/// suitable driver class".
const DB_FILES: &[JavaFile] = &[JavaFile {
    suffix: "testcontainers_config",
    template: include_str!("../../../../templates/add/testcontainers_config_java.java"),
    before_boot: None,
    source_set: SourceSet::Test,
    class_name: db_testcontainers_config_class,
    template_class: db_testcontainers_config_class,
}];

fn db_testcontainers_config_class(_: &Capability) -> String {
    "TestcontainersConfig".to_string()
}

const DB_DEPENDENCIES: &[DependencySpec] = &[
    // **The module that reads `compose.yaml` at startup.** `storage postgres`
    // writes the compose service the application connects to, and without
    // this Boot never looks at it: on a machine whose engine Spring *can*
    // drive, the connection details it supplies are what makes `jails run`
    // work with no properties at all. The properties beside it stay, because
    // they are what carries the machines where it cannot -- see
    // `DB_PROPERTIES`, and note it is switched off there by default for
    // exactly that reason. Declaring it anyway is what lets a reader turn it
    // back on without also having to know which artifact to add.
    DependencySpec {
        group: "org.springframework.boot",
        artifact: "spring-boot-docker-compose",
        version: None,
        scope: DependencyScope::Runtime,
        spring_managed_version: true,
        only_when_build_exists: false,
        boot: BootCondition::Spring,
    },
    // Contributes `TestcontainersLifecycleApplicationContextInitializer` from
    // its own `spring.factories`, which is why nothing calls `start()`.
    DependencySpec {
        group: "org.springframework.boot",
        artifact: "spring-boot-testcontainers",
        version: None,
        scope: DependencyScope::Test,
        spring_managed_version: true,
        only_when_build_exists: false,
        boot: BootCondition::Spring,
    },
    // Pinned, because Testcontainers 2.0 renamed every module
    // (`postgresql` -> `testcontainers-postgresql`) and the Boot parent does
    // not manage the new names.
    DependencySpec {
        group: "org.testcontainers",
        artifact: "testcontainers-postgresql",
        version: Some("2.0.5"),
        scope: DependencyScope::Test,
        spring_managed_version: false,
        only_when_build_exists: false,
        boot: BootCondition::Spring,
    },
    DependencySpec {
        group: "org.testcontainers",
        artifact: "testcontainers-junit-jupiter",
        version: Some("2.0.5"),
        scope: DependencyScope::Test,
        spring_managed_version: false,
        only_when_build_exists: false,
        boot: BootCondition::Spring,
    },
];

/// The application's own datasource, and the two settings that are not tuning.
///
/// Spring's docker-compose module supplies connection details where it works
/// and they take precedence, so these are redundant there and load-bearing
/// everywhere else -- without them the application dies at startup on any
/// machine whose compose provider Spring cannot drive.
///
/// `spring.persistence.exceptiontranslation.enabled=false` is not tuning:
/// JDBC auto-configuration registers persistence-exception translation, which
/// CGLIB-proxies every `@Repository` and fails on a `final` class. jails
/// generates raw SQL and no ORM, so the translation has nothing to translate.
///
/// `spring.docker.compose.enabled=false` is not tuning either: jails starts
/// compose itself in `run` and `start`, and Boot's module shells out with
/// Docker Compose v2 syntax that podman-compose rejects, killing startup.
const DB_PROPERTIES: &[PropertySpec] = &[
    property("spring.persistence.exceptiontranslation.enabled", "false"),
    property(
        "spring.datasource.url",
        "jdbc:postgresql://localhost:5432/app",
    ),
    property("spring.datasource.username", "app"),
    property("spring.datasource.password", "app"),
    property("spring.datasource.hikari.pool-name", "primary"),
    property("spring.datasource.hikari.maximum-pool-size", "20"),
    property("spring.datasource.hikari.connection-timeout", "1000"),
    property("spring.datasource.hikari.initialization-fail-timeout", "1"),
    property(
        "spring.datasource.hikari.transaction-isolation",
        "TRANSACTION_READ_COMMITTED",
    ),
    // Refuse a read replica now, instead of failing on the first write.
    property(
        "spring.datasource.hikari.connection-init-sql",
        "SELECT 1/(1-pg_is_in_recovery()::int)",
    ),
    property("server.shutdown", "graceful"),
    property("spring.lifecycle.timeout-per-shutdown-phase", "30s"),
    property("spring.docker.compose.enabled", "false"),
];

const DB_COMPOSE: &[ComposeService] = &[ComposeService {
    name: "postgres",
    marker: "db",
    body: "image: postgres:17-alpine\nenvironment:\n  POSTGRES_DB: app\n  POSTGRES_USER: app\n  POSTGRES_PASSWORD: app\nports:\n  - \"5432:5432\"\nhealthcheck:\n  test: [\"CMD-SHELL\", \"pg_isready -U app -d app\"]\n  interval: 2s\n  timeout: 5s\n  retries: 10",
}];

const DB_PACKAGE_OVERRIDES: &[PackageOverride] = &[PackageOverride {
    suffix: "testcontainers_config",
    project_subpackage: Package::Base,
}];

/// The test half of `storage postgres`.
///
/// `files_when: Spring` because every file and dependency in it is Spring's:
/// a plain Maven project with `storage postgres` gets the driver and Flyway
/// from `storage::storage_dependencies` and nothing here.
pub(super) const DB_PACK: Pack = Pack {
    fragments: NO_FRAGMENTS,
    files: DB_FILES,
    files_when: BootCondition::Spring,
    resources: NO_RESOURCES,
    dependencies: DB_DEPENDENCIES,
    properties: DB_PROPERTIES,
    compose_services: DB_COMPOSE,
    build_features: NO_BUILD_FEATURES,
    default_package: adapters_package,
    package_overrides: DB_PACKAGE_OVERRIDES,
    minimum_boot: None,
};

pub(super) const H2_PACK: Pack = Pack {
    fragments: NO_FRAGMENTS,
    files: H2_FILES,
    files_when: BootCondition::Any,
    resources: NO_RESOURCES,
    dependencies: H2_DEPENDENCIES,
    properties: H2_PROPERTIES,
    compose_services: NO_COMPOSE_SERVICES,
    build_features: NO_BUILD_FEATURES,
    default_package: adapters_package,
    package_overrides: NO_PACKAGE_OVERRIDES,
    minimum_boot: None,
};
