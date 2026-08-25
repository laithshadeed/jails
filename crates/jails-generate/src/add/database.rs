//! `add db` and `add sqlite`: the two persistence capabilities.
//!
//! Both are raw SQL by contract -- no ORM, no generated schema. `db` is the
//! larger one because a working database is more than a driver: Flyway *and*
//! its Boot auto-configuration module, a compose service, the datasource
//! properties read back out of that service, and a `TestcontainersConfig`
//! imported into every `@SpringBootTest` already on disk.

use super::*;
use crate::spring::TESTCONTAINERS_JUNIT;

// ---------------------------------------------------------------------------
// db -- PostgreSQL, Flyway, and real integration tests; deliberately no ORM
// ---------------------------------------------------------------------------

pub(crate) const SPRING_JDBC: Dependency = Dependency {
    group_id: "org.springframework.boot",
    artifact_id: "spring-boot-starter-jdbc",
    version: None,
    scope: None,
    optional: false,
};
pub(super) const POSTGRES_MANAGED: Dependency = Dependency {
    group_id: "org.postgresql",
    artifact_id: "postgresql",
    version: None,
    scope: Some("runtime"),
    optional: false,
};
pub(super) const POSTGRES_PINNED: Dependency = Dependency {
    group_id: "org.postgresql",
    artifact_id: "postgresql",
    version: Some("42.7.11"),
    scope: Some("runtime"),
    optional: false,
};
/// Flyway's Boot integration, which is a *different artifact* from Flyway.
///
/// Boot 4 split auto-configuration into ~130 modules, and there is no Flyway
/// class in `spring-boot-autoconfigure` at all. With only `flyway-core` on the
/// classpath the migrations are never run and nothing says so: no error, no
/// warning, not one Flyway log line -- and then `relation "..." does not
/// exist` from the first query, which reads like a broken migration rather
/// than an absent one.
///
/// The general rule this is one instance of: in Boot 4 the technology jar and
/// the auto-configuration jar are separate dependencies, and a capability that
/// ships only the former ships something that does not run.
pub(super) const SPRING_BOOT_FLYWAY: Dependency = Dependency {
    group_id: "org.springframework.boot",
    artifact_id: "spring-boot-flyway",
    version: None,
    scope: None,
    optional: false,
};
pub(super) const FLYWAY_CORE_MANAGED: Dependency = Dependency {
    group_id: "org.flywaydb",
    artifact_id: "flyway-core",
    version: None,
    scope: None,
    optional: false,
};
pub(super) const FLYWAY_POSTGRES_MANAGED: Dependency = Dependency {
    group_id: "org.flywaydb",
    artifact_id: "flyway-database-postgresql",
    version: None,
    scope: None,
    optional: false,
};
pub(super) const FLYWAY_CORE_PINNED: Dependency = Dependency {
    group_id: "org.flywaydb",
    artifact_id: "flyway-core",
    version: Some("12.8.1"),
    scope: None,
    optional: false,
};
pub(super) const FLYWAY_POSTGRES_PINNED: Dependency = Dependency {
    group_id: "org.flywaydb",
    artifact_id: "flyway-database-postgresql",
    version: Some("12.8.1"),
    scope: None,
    optional: false,
};
pub(super) const TESTCONTAINERS_POSTGRES: Dependency = Dependency {
    group_id: "org.testcontainers",
    artifact_id: "testcontainers-postgresql",
    version: Some("2.0.5"),
    scope: Some("test"),
    optional: false,
};
pub(super) const SPRING_TESTCONTAINERS: Dependency = Dependency {
    group_id: "org.springframework.boot",
    artifact_id: "spring-boot-testcontainers",
    version: None,
    scope: Some("test"),
    optional: false,
};

pub(super) const POSTGRES_IMAGE: &str = "postgres:17-alpine";
pub(super) const TESTCONTAINERS_CONFIG: &str = "TestcontainersConfig";

pub(super) fn db_plan(slice: &Slice) -> Result<Change> {
    let root: &Path = slice.root();
    let flavor: Flavor = slice.flavor();
    let pkg: &str = &slice.root_package();
    let mut deps = match flavor {
        Flavor::SpringBoot => vec![
            SPRING_JDBC,
            POSTGRES_MANAGED,
            SPRING_BOOT_FLYWAY,
            FLYWAY_CORE_MANAGED,
            FLYWAY_POSTGRES_MANAGED,
        ],
        Flavor::PlainMaven => vec![POSTGRES_PINNED, FLYWAY_CORE_PINNED, FLYWAY_POSTGRES_PINNED],
    };
    deps.extend([TESTCONTAINERS_POSTGRES, TESTCONTAINERS_JUNIT]);
    if flavor == Flavor::SpringBoot {
        // `@ServiceConnection` and the lifecycle initializer that starts a
        // container declared as a bean both live in this module.
        deps.push(SPRING_TESTCONTAINERS);
    }

    let mut files = vec![Artifact {
        kind: "capability file",
        path: root.join("src/main/resources/db/migration/.gitkeep"),
        contents: String::new(),
    }];
    let spring_test_import = if flavor == Flavor::SpringBoot {
        files.push(Artifact {
            kind: "capability file",
            path: test_dir(root, pkg).join(format!("{TESTCONTAINERS_CONFIG}.java")),
            contents: testcontainers_config_java(pkg),
        });
        Some(SpringTestImport {
            pkg: pkg.to_string(),
            class: TESTCONTAINERS_CONFIG,
        })
    } else {
        None
    };

    // Read back from compose.yaml where there is one, exactly as the write
    // path does: `add db` writes that file, but a project may have edited the
    // port or the credentials since, and a datasource pointing at the wrong
    // one is worse than none. On a first run there is no file yet and the
    // defaults are what this same plan is about to write.
    let properties = if flavor == Flavor::SpringBoot {
        let connect = compose::read(root)
            .ok()
            .and_then(|yaml| compose::postgres_connect(&yaml))
            .unwrap_or_else(compose::PostgresConnect::defaults);
        db_property_lines(&connect)
    } else {
        Vec::new()
    };

    Ok(Change {
        deps,
        files,
        compose: vec![compose::POSTGRES],
        spring_test_import,
        properties,
        ..Change::default()
    })
}

/// The test-side database wiring: a container declared as a Spring bean.
///
/// `@ServiceConnection` is how a container's url, username and password reach
/// auto-configuration, and a container that is a `@Bean` is started and
/// stopped with the context -- `spring-boot-testcontainers` contributes
/// `TestcontainersLifecycleApplicationContextInitializer` from its own
/// `spring.factories`, so nothing here calls `start()`. Boot's reference docs
/// prefer this over a `@Testcontainers`/`@Container` static field, because
/// Spring caches a context beyond the container's JUnit-managed lifetime and
/// later tests then fail against a stopped container.
///
/// ## Why this is imported rather than registered globally
///
/// jails used to register this from a test-classpath `spring.factories`, so
/// that every `@SpringBootTest` got a DataSource without an annotation. That
/// solved a real problem -- once `spring-boot-starter-jdbc` is present, JDBC
/// auto-config demands a DataSource for *every* context, including a test
/// that never queries -- and created a worse one: **every** test paid for a
/// PostgreSQL container, including pure slices and `@WebMvcTest`s that have
/// no business touching a database. A test suite that starts a database it
/// does not use is slow in a way that is nobody's fault and never fixed.
///
/// So the container is imported by the tests that need it, and `add db`
/// splices that `@Import` into the `@SpringBootTest` classes already in the
/// project (see [`import_into_spring_boot_tests`]) -- which is what keeps the
/// original problem from coming back as a mysterious "Failed to determine a
/// suitable driver class" on a test the user did not write.
pub(super) fn testcontainers_config_java(pkg: &str) -> String {
    crate::template::render(
        crate::template_here!("add/testcontainers_config_java.java"),
        &[
            ("pkg", pkg),
            ("TESTCONTAINERS_CONFIG", TESTCONTAINERS_CONFIG),
            ("POSTGRES_IMAGE", POSTGRES_IMAGE),
        ],
    )
}

/// JDBC auto-config registers a CGLIB proxy around every `@Repository`.
/// jails (and the code it generates) uses `final` classes, so that proxy
/// cannot be created. Exception translation is a JPA concern anyway -- this
/// capability is raw SQL.
pub(super) const EXCEPTION_TRANSLATION_PROPERTY: &str =
    "spring.persistence.exceptiontranslation.enabled=false";

/// jails already owns the compose lifecycle -- `jails run` and `jails start`
/// bring the services up, and `jails stop` takes them down -- so Spring's own
/// docker-compose module has no job left to do in a jails project. Leaving it
/// on is not merely redundant: it shells out to the compose provider with
/// Docker Compose v2 syntax (`--ansi never`, `config --format=json`) that
/// podman-compose rejects, and the application then dies during startup
/// before any of its own code runs. Flip this to `true` to hand compose back
/// to Spring.
pub(super) const COMPOSE_DISABLED_PROPERTY: &str = "spring.docker.compose.enabled=false";
pub(super) const COMPOSE_LIFECYCLE_COMMENT: &str =
    "# jails starts compose itself (jails run / jails start).";

/// The application's own datasource, pointing at the compose service `add
/// db` just wrote.
///
/// Spring Boot can discover this itself through `spring-boot-docker-compose`,
/// and where that works these properties are simply overridden by it --
/// connection details take precedence over properties. Writing them anyway
/// buys two things. The application starts on a machine whose compose
/// provider Spring cannot drive (`spring-boot-docker-compose` shells out
/// with Docker Compose v2 syntax that podman-compose rejects, and the app
/// dies during startup). And the connection is visible in the project rather
/// than materialising from a module, which is the same reason this
/// capability emits SQL you can read instead of an ORM.
/// What `add db` sets in `application.properties`, as the lines themselves.
///
/// One list, read by both engines. V1 renders it into a marked block; V2
/// states each line as a `Property` resource this capability owns, which is
/// what lets `remove db` take back exactly the keys it set and leave a
/// hand-written neighbour alone.
pub(super) fn db_property_lines(connect: &compose::PostgresConnect) -> Vec<String> {
    let compose::PostgresConnect {
        host,
        port,
        user,
        password,
        database,
    } = connect;
    vec![
        EXCEPTION_TRANSLATION_PROPERTY.to_string(),
        format!("spring.datasource.url=jdbc:postgresql://{host}:{port}/{database}"),
        format!("spring.datasource.username={user}"),
        format!("spring.datasource.password={password}"),
        "spring.datasource.hikari.pool-name=primary".to_string(),
        "spring.datasource.hikari.maximum-pool-size=20".to_string(),
        "spring.datasource.hikari.connection-timeout=1000".to_string(),
        "spring.datasource.hikari.initialization-fail-timeout=1".to_string(),
        "spring.datasource.hikari.transaction-isolation=TRANSACTION_READ_COMMITTED".to_string(),
        "# Refuse a read replica now, instead of failing on the first write.".to_string(),
        "spring.datasource.hikari.connection-init-sql=SELECT 1/(1-pg_is_in_recovery()::int)"
            .to_string(),
        "server.shutdown=graceful".to_string(),
        "spring.lifecycle.timeout-per-shutdown-phase=30s".to_string(),
        COMPOSE_LIFECYCLE_COMMENT.to_string(),
        COMPOSE_DISABLED_PROPERTY.to_string(),
    ]
}

/// Drop the matching class/resource under `target/` so incremental
/// `mvn test` (what `jails test` runs) does not keep using a deleted file.
pub(super) fn delete_maven_output(root: &Path, src: &Path) {
    let Some(out) = maven_output_for(root, src) else {
        return;
    };
    if out.exists() {
        let _ = jails_support::apply::remove_derived(&out);
    }
}

pub(super) fn maven_output_for(root: &Path, src: &Path) -> Option<PathBuf> {
    let rel = src.strip_prefix(root).ok()?;
    let mut parts = rel.iter();
    if parts.next()?.to_str()? != "src" {
        return None;
    }
    let scope = parts.next()?.to_str()?;
    let kind = parts.next()?.to_str()?;
    let rest: PathBuf = parts.collect();
    let target_root = match (scope, kind) {
        ("main", "java") | ("main", "resources") => root.join("target/classes"),
        ("test", "java") | ("test", "resources") => root.join("target/test-classes"),
        _ => return None,
    };
    let mut out = target_root.join(rest);
    if out.extension().is_some_and(|e| e == "java") {
        out.set_extension("class");
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// sqlite
// ---------------------------------------------------------------------------

pub(super) const SQLITE_JDBC: Dependency = Dependency {
    group_id: "org.xerial",
    artifact_id: "sqlite-jdbc",
    version: Some("3.49.1.0"),
    scope: None,
    optional: false,
};

/// Deliberately the same code in both flavors. `java.sql` is part of the
/// standard library, so a plain JDBC connection plus a migration runner needs
/// nothing beyond the driver or the fiddliness of a persistence framework.
/// A Spring project can inject the record wherever it needs a connection.
pub(super) fn sqlite_plan(slice: &Slice, name: Option<&str>) -> Result<Change> {
    let root: &Path = slice.root();
    let pkg: &str = &slice.placed(Layer::Adapters);

    let base = name.map(capitalize).unwrap_or_default();
    let database = format!("{base}Database");
    let migrations = format!("{base}Migrations");

    Ok(Change {
        deps: vec![SQLITE_JDBC],
        files: vec![
            Artifact {
                kind: "capability file",
                path: main_dir(root, pkg).join(format!("{database}.java")),
                contents: database_java(pkg, &database),
            },
            Artifact {
                kind: "capability file",
                path: main_dir(root, pkg).join(format!("{migrations}.java")),
                contents: migrations_java(pkg, &migrations),
            },
            Artifact {
                kind: "capability file",
                path: root.join("src/main/resources/db/migration/001_init.sql"),
                contents: FIRST_MIGRATION.to_string(),
            },
            Artifact {
                kind: "capability file",
                path: test_dir(root, pkg).join(format!("{database}Test.java")),
                contents: database_test_java(pkg, &database, &migrations),
            },
        ],
        ..Change::default()
    })
}

pub(super) const FIRST_MIGRATION: &str =
    "-- Applied once, in filename order, by Migrations.applyAll.
create table if not exists item (
    id integer primary key autoincrement,
    name text not null,
    qty integer not null default 0
);
";

pub(super) fn database_java(pkg: &str, class: &str) -> String {
    crate::template::render(
        crate::template_here!("add/database_java.java"),
        &[("pkg", pkg), ("class", class)],
    )
}

pub(super) fn migrations_java(pkg: &str, class: &str) -> String {
    crate::template::render(
        crate::template_here!("add/migrations_java.java"),
        &[("pkg", pkg), ("class", class)],
    )
}

pub(super) fn database_test_java(pkg: &str, database: &str, migrations: &str) -> String {
    crate::template::render(
        crate::template_here!("add/database_test_java.java"),
        &[
            ("pkg", pkg),
            ("database", database),
            ("migrations", migrations),
        ],
    )
}
