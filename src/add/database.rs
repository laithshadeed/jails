//! `add db` and `add sqlite`: the two persistence capabilities.
//!
//! Both are raw SQL by contract -- no ORM, no generated schema. `db` is the
//! larger one because a working database is more than a driver: Flyway *and*
//! its Boot auto-configuration module, a compose service, the datasource
//! properties read back out of that service, and a `TestcontainersConfig`
//! imported into every `@SpringBootTest` already on disk.

use super::test_wiring::remove_jails_db_block;
use super::*;
use crate::spring::TESTCONTAINERS_JUNIT;
use jails_support::{apply, codemod};

// ---------------------------------------------------------------------------
// db -- PostgreSQL, Flyway, and real integration tests; deliberately no ORM
// ---------------------------------------------------------------------------

pub(super) const SPRING_JDBC: Dependency = Dependency {
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

    Ok(Change {
        deps,
        files,
        compose: vec![compose::POSTGRES],
        spring_test_import,
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
        crate::template::template!("add/testcontainers_config_java.java"),
        &[
            ("pkg", pkg),
            ("TESTCONTAINERS_CONFIG", TESTCONTAINERS_CONFIG),
            ("POSTGRES_IMAGE", POSTGRES_IMAGE),
        ],
    )
}

/// Whether a previously generated container config should be rewritten.
///
/// Three generations exist now. The first was a `@TestConfiguration` that
/// needed an `@Import`; the second an `ApplicationContextInitializer` that
/// injected `spring.datasource.*` by hand; the third the initializer holding a
/// nested `@ServiceConnection` bean. The current shape is back to an imported
/// `@TestConfiguration`, on purpose -- see [`testcontainers_config_java`] --
/// so the marker to look for is the *absence* of the initializer plus the
/// presence of `@ServiceConnection`.
pub(super) fn should_replace_postgres_test_config(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    if name != "PostgresContainerConfig.java" && name != "TestcontainersConfig.java" {
        return false;
    }
    fs::read_to_string(path).is_ok_and(|s| {
        !s.contains("ServiceConnection") || s.contains("ApplicationContextInitializer")
    })
}

pub(super) fn spring_factories_path(root: &Path) -> PathBuf {
    root.join("src/test/resources/META-INF/spring.factories")
}

pub(super) fn application_properties_path(root: &Path) -> PathBuf {
    root.join("src/main/resources/application.properties")
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
pub(super) fn application_properties_block(connect: &compose::PostgresConnect) -> String {
    let compose::PostgresConnect {
        host,
        port,
        user,
        password,
        database,
    } = connect;
    format!(
        "# jails:db\n\
         {EXCEPTION_TRANSLATION_PROPERTY}\n\
         spring.datasource.url=jdbc:postgresql://{host}:{port}/{database}\n\
         spring.datasource.username={user}\n\
         spring.datasource.password={password}\n\
         spring.datasource.hikari.pool-name=primary\n\
         spring.datasource.hikari.maximum-pool-size=20\n\
         spring.datasource.hikari.connection-timeout=1000\n\
         spring.datasource.hikari.initialization-fail-timeout=1\n\
         spring.datasource.hikari.transaction-isolation=TRANSACTION_READ_COMMITTED\n\
         # Refuse a read replica now, instead of failing on the first write.\n\
         spring.datasource.hikari.connection-init-sql=SELECT 1/(1-pg_is_in_recovery()::int)\n\
         server.shutdown=graceful\n\
         spring.lifecycle.timeout-per-shutdown-phase=30s\n\
         {COMPOSE_LIFECYCLE_COMMENT}\n\
         {COMPOSE_DISABLED_PROPERTY}\n\
         # /jails:db\n"
    )
}

/// Splice a capability's own `application.properties` lines into a marked
/// block. Generic in the label so every capability owns exactly its own
/// lines and `remove` can take them back without touching a neighbour's --
/// the same rule `compose.yaml` already follows for services.
pub(super) fn install_capability_properties(
    root: &Path,
    label: &str,
    lines: &[String],
    dry_run: bool,
) -> Result<bool> {
    if lines.is_empty() {
        return Ok(false);
    }
    let path = application_properties_path(root);
    let existing = if path.exists() {
        fs::read_to_string(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))?
    } else {
        String::new()
    };
    let marked = codemod::Marked::new(label);
    if marked.present_in(&existing) {
        println!("  exists  {}", rel(root, &path));
        return Ok(false);
    }
    let block = marked.render(&format!("{}\n", lines.join("\n")));
    let next = if existing.trim().is_empty() {
        block
    } else {
        format!("{}\n{block}", existing.trim_end())
    };
    if dry_run {
        for line in lines {
            println!("  would set  {line} in {}", rel(root, &path));
        }
        return Ok(true);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }
    apply::put(&path, next)?;
    for line in lines {
        println!("  set     {line}");
    }
    Ok(true)
}

/// Remove one capability's marked property block, leaving every other line
/// -- including another capability's block -- exactly as it was.
/// Lines inside a `# jails:<label>` block that jails did not write.
///
/// The marked block is how `remove` knows what to take back out, and it is
/// also, inevitably, where people tune the capability -- it is the block with
/// the capability's name on it. A real project ended up with twenty
/// hand-written Kafka properties inside jails' markers (an
/// `ErrorHandlingDeserializer`, `acks=all`, a KIP-848 opt-in), every one of
/// which `remove kafka` would have deleted without a word.
///
/// jails cannot refuse to remove them -- they are inside the block it owns --
/// but it must not delete them silently. Naming them at the confirmation
/// prompt turns an invisible loss into a decision.
///
/// Comments and blank lines are ignored: a comment inside the block is
/// usually jails' own explanation of the property below it.
pub(super) fn unowned_properties(existing: &str, label: &str, owned: &[String]) -> Vec<String> {
    let Some(body) = codemod::Marked::new(label).body_in(existing) else {
        return Vec::new();
    };
    body.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter(|line| !owned.iter().any(|owned| owned.trim() == *line))
        .map(str::to_owned)
        .collect()
}

/// Warn about hand-written properties inside the block about to be deleted.
/// Files this capability generated that no longer match what jails would
/// write -- so they have been edited since, and deleting them loses work.
///
/// The same problem as `unowned_properties`, one level up. `remove` deletes
/// every file the plan names that exists, and "it exists" is not ownership:
/// a `CsvReader` someone spent an afternoon on looks exactly like the stub
/// jails wrote. A real project had ~20 hand-written properties inside jails'
/// own markers; a hand-finished generated class is the same discovery waiting
/// to happen, and costs more.
///
/// Re-rendering as the evidence is imperfect and deliberately so: jails keeps
/// no manifest of what it wrote, so a file generated by an *older* jails
/// reads as edited. That errs toward warning about a file that is fine, which
/// is the safe direction -- the cost is a line of output, and the cost of the
/// other mistake is someone's afternoon.
pub(super) fn edited_files(plan: &Change) -> Vec<&PathBuf> {
    plan.files
        .iter()
        .filter(|f| fs::read_to_string(&f.path).is_ok_and(|on_disk| on_disk != f.contents))
        .map(|f| &f.path)
        .collect()
}

/// Name every generated file that has been edited, at the confirmation prompt
/// and in `--dry-run`.
///
/// jails does not refuse: these are files it generated and `remove` is the
/// documented inverse, so refusing would make the command unusable on exactly
/// the projects that got the most use out of it. It must not delete them
/// *silently*, which is the same line `report_unowned_properties` draws.
pub(super) fn report_edited_files(root: &Path, plan: &Change) {
    let edited = edited_files(plan);
    if edited.is_empty() {
        return;
    }
    println!(
        "  !! {} generated file{} changed since jails wrote {}",
        edited.len(),
        if edited.len() == 1 { "" } else { "s" },
        if edited.len() == 1 { "it" } else { "them" }
    );
    for path in &edited {
        println!("     {}", rel(root, path));
    }
    println!("     these will be deleted -- copy out anything you need first");
}

pub(super) fn report_unowned_properties(root: &Path, label: &str, owned: &[String]) {
    let Ok(existing) = fs::read_to_string(application_properties_path(root)) else {
        return;
    };
    let unowned = unowned_properties(&existing, label, owned);
    if unowned.is_empty() {
        return;
    }
    println!(
        "  !! {} propert{} inside the # jails:{label} block were not written by jails",
        unowned.len(),
        if unowned.len() == 1 { "y" } else { "ies" }
    );
    for line in &unowned {
        println!("     {line}");
    }
    println!("     these will be deleted with the block -- copy them out first if you need them");
}

pub(super) fn remove_capability_properties(root: &Path, label: &str) -> Result<()> {
    let path = application_properties_path(root);
    let Ok(existing) = fs::read_to_string(&path) else {
        return Ok(());
    };
    let Some(out) = codemod::Marked::new(label).strip_from(&existing) else {
        return Ok(());
    };
    if out.trim().is_empty() {
        // The file existed only for this block; leaving an empty file behind
        // is litter.
        let _ = fs::remove_file(&path);
        println!("  removed {}", rel(root, &path));
        return Ok(());
    }
    apply::put(&path, out)?;
    println!("  updated {}", rel(root, &path));
    Ok(())
}

pub(super) fn install_db_properties(root: &Path, dry_run: bool) -> Result<bool> {
    let path = application_properties_path(root);
    let existing = if path.exists() {
        fs::read_to_string(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))?
    } else {
        String::new()
    };
    // An older jails wrote a block with only the exception-translation
    // property in it. `add` promises to write whatever is missing, so an
    // out-of-date block is replaced rather than reported as already present
    // -- otherwise a project generated last week silently never gains the
    // datasource it now needs.
    let has_block = existing.contains(EXCEPTION_TRANSLATION_PROPERTY);
    let current = existing.contains("spring.datasource.url=");
    if has_block && current {
        println!("  exists  {}", rel(root, &path));
        return Ok(false);
    }
    let existing = if has_block {
        remove_jails_db_block(&existing, EXCEPTION_TRANSLATION_PROPERTY).unwrap_or(existing)
    } else {
        existing
    };
    // Read back from compose.yaml rather than assuming the defaults: `add
    // db` writes that file, but a project may have edited the port or the
    // credentials since, and a datasource pointing at the wrong one is worse
    // than none.
    let connect = compose::read(root)
        .ok()
        .and_then(|yaml| compose::postgres_connect(&yaml))
        .unwrap_or_else(compose::PostgresConnect::defaults);
    let next = if existing.trim().is_empty() {
        application_properties_block(&connect)
    } else {
        format!(
            "{}\n{}",
            existing.trim_end(),
            application_properties_block(&connect)
        )
    };
    if dry_run {
        println!(
            "  would set  {EXCEPTION_TRANSLATION_PROPERTY} in {}",
            rel(root, &path)
        );
        return Ok(true);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }
    apply::put(&path, next)?;
    println!("  properties  {}", rel(root, &path));
    Ok(true)
}

pub(super) fn uninstall_db_properties(root: &Path) -> Result<()> {
    let path = application_properties_path(root);
    if !path.exists() {
        return Ok(());
    }
    let existing =
        fs::read_to_string(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let Some(next) = remove_jails_db_block(&existing, EXCEPTION_TRANSLATION_PROPERTY) else {
        return Ok(());
    };
    if next.trim().is_empty() {
        fs::remove_file(&path).map_err(|e| format!("failed to remove {}: {e}", path.display()))?;
        println!("  delete  {}", rel(root, &path));
    } else {
        apply::put(&path, next)?;
        println!("  unsplice  {}", rel(root, &path));
    }
    Ok(())
}

/// Drop the matching class/resource under `target/` so incremental
/// `mvn test` (what `jails test` runs) does not keep using a deleted file.
pub(super) fn delete_maven_output(root: &Path, src: &Path) {
    let Some(out) = maven_output_for(root, src) else {
        return;
    };
    if out.exists() {
        let _ = fs::remove_file(&out);
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

pub(super) const SPRING_FACTORIES_KEY: &str =
    "org.springframework.context.ApplicationContextInitializer";

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
        crate::template::template!("add/database_java.java"),
        &[("pkg", pkg), ("class", class)],
    )
}

pub(super) fn migrations_java(pkg: &str, class: &str) -> String {
    crate::template::render(
        crate::template::template!("add/migrations_java.java"),
        &[("pkg", pkg), ("class", class)],
    )
}

pub(super) fn database_test_java(pkg: &str, database: &str, migrations: &str) -> String {
    crate::template::render(
        crate::template::template!("add/database_test_java.java"),
        &[
            ("pkg", pkg),
            ("database", database),
            ("migrations", migrations),
        ],
    )
}
