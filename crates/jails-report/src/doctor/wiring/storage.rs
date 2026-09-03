//! The checks that ask whether this project's *data path* is wired up.
//!
//! Apart from [`super::wiring`] by what the question is about rather than by
//! size: everything here asks whether the project can reach a database and
//! whether its schema will be there when it does -- the container a
//! `@SpringBootTest` needs, which repository adapter is the bean, whether
//! `spring.sql.init` was switched on and left with nothing to run. The
//! siblings ask about the serving path instead.
//!
//! These are hand-written rather than derived from a capability's plan. A
//! derived check knows a dependency is missing; it does not know that a
//! `spring.factories` left behind starts a second container for every test.
//! Those are interaction facts no plan carries.

use super::super::environment::tcp_reachable;
use super::super::{Check, Status};
use super::property_value;
use crate::compose;
use crate::project::Project;
use std::path::Path;
use std::time::Duration;

pub(crate) fn database_checks(project: &Project) -> Vec<Check> {
    let root: &Path = project.root();
    let mut checks = Vec::new();
    let yaml = compose::read(root).unwrap_or_default();
    let Some(conn) = compose::postgres_connect(&yaml) else {
        if project.has_dependency("org.postgresql", "postgresql") {
            checks.push(
                Check::new(
                    Status::Fail,
                    "postgres",
                    "the PostgreSQL driver is a dependency but compose.yaml has no postgres service",
                )
                .fix("jails add db"),
            );
        }
        return checks;
    };

    // A TCP connect is the honest test: the container can be "running" while
    // postgres inside it is still replaying WAL and refusing connections,
    // which is exactly the window a `jails run` right after `jails start`
    // lands in.
    let reachable = tcp_reachable(&conn.host, conn.port, Duration::from_millis(750));
    checks.push(if reachable {
        Check::new(
            Status::Ok,
            "postgres",
            format!(
                "accepting connections on {}:{} (db {}, user {})",
                conn.host, conn.port, conn.database, conn.user
            ),
        )
    } else {
        Check::new(
            Status::Fail,
            "postgres",
            format!(
                "nothing accepting connections on {}:{}",
                conn.host, conn.port
            ),
        )
        .fix("jails start db")
    });

    let migrations = root.join("src/main/resources/db/migration");
    let count = std::fs::read_dir(&migrations)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.path().extension().is_some_and(|x| x == "sql"))
                .count()
        })
        .unwrap_or(0);
    let has_flyway = project.has_dependency("org.flywaydb", "flyway-core");
    // Counting the files answers the wrong question. `flyway-core` alone runs
    // nothing on Boot 4 -- the auto-configuration lives in the separate
    // `spring-boot-flyway` module -- and the failure is silent: no error, no
    // warning, no Flyway log line, then `relation "..." does not exist`. So
    // the check is "will these run", not "do these exist".
    let is_spring = project.is_spring_boot();
    let has_boot_flyway = project.has_dependency("org.springframework.boot", "spring-boot-flyway");
    if is_spring && has_flyway && !has_boot_flyway {
        checks.push(
            Check::new(
                Status::Fail,
                "migrations",
                format!(
                    "{count} .sql file(s) and flyway-core, but not spring-boot-flyway -- \
                     Boot 4 keeps Flyway's auto-configuration in that module, so nothing \
                     runs them and nothing says so"
                ),
            )
            .fix("jails add db (idempotent -- it re-writes only what is missing)"),
        );
    } else {
        checks.push(match (has_flyway, count) {
        (true, 0) => Check::new(
            Status::Warn,
            "migrations",
            "Flyway is on the classpath but db/migration holds no .sql files -- the schema will be empty",
        )
        .fix("jails g migration create_<table>"),
        (true, n) => Check::new(
            Status::Ok,
            "migrations",
            format!("{n} migration(s) in src/main/resources/db/migration"),
        ),
        (false, n) if n > 0 => Check::new(
            Status::Warn,
            "migrations",
            format!("{n} .sql file(s) in db/migration but Flyway is not a dependency -- nothing will run them"),
        )
        .fix("jails add db"),
        (false, _) => Check::new(Status::Skip, "migrations", "no Flyway, no migration directory"),
    });
    }

    // The two pieces of test-side wiring `add db` installs on Spring. Both
    // are invisible until a @SpringBootTest fails with "Failed to determine
    // a suitable driver class", and both are easy to lose to a rebase.
    if project.is_spring_boot() {
        // What matters is that a container bean exists *and* that every
        // @SpringBootTest can see one. Checking the file alone would pass on
        // a project where a rebase dropped the @Import and every context test
        // is red; checking the imports alone would pass on a project whose
        // config file was deleted.
        let (container_config, unimported) = test_container_wiring(root);
        checks.push(match (&container_config, unimported.as_slice()) {
            (None, _) => Check::new(
                Status::Fail,
                "test datasource",
                "Spring + postgres, but no @ServiceConnection container config on the test \
                 classpath -- @SpringBootTest will fail with \"Failed to determine a suitable \
                 driver class\"",
            )
            .fix("jails add db (idempotent -- it re-writes only what is missing)"),
            (Some(class), []) => Check::new(
                Status::Ok,
                "test datasource",
                format!("{class} declares an @ServiceConnection container, imported where needed"),
            ),
            (Some(class), missing) => Check::new(
                Status::Fail,
                "test datasource",
                format!(
                    "{} @SpringBootTest class(es) do not import {class} -- they will fail with \
                     \"Failed to determine a suitable driver class\": {}",
                    missing.len(),
                    missing.join(", ")
                ),
            )
            .fix("jails add db (idempotent -- it re-writes only what is missing)"),
        });

        let properties = root.join("src/main/resources/application.properties");
        let translation_disabled = std::fs::read_to_string(&properties)
            .map(|t| t.contains("spring.persistence.exceptiontranslation.enabled=false"))
            .unwrap_or(false);
        if !translation_disabled {
            checks.push(
                Check::new(
                    Status::Warn,
                    "exception translation",
                    "spring.persistence.exceptiontranslation.enabled is not disabled -- JDBC \
                     auto-config will CGLIB-proxy every @Repository, which fails on final classes",
                )
                .fix("add spring.persistence.exceptiontranslation.enabled=false to application.properties"),
            );
        }
    }
    checks
}

/// Does this test import `class`, however the annotation is spelled?
///
/// Not a substring match on `@Import(Foo.class)`: Spring's `@Import` is not
/// repeatable, so jails' own splicer *merges* -- a test that also needs its
/// own containers ends up with
/// `@Import({SomeIT.Containers.class, TestcontainersConfig.class})`, which the
/// literal form misses. A check that goes red on correctly wired code is
/// worse than no check, because the fix it names changes nothing.
pub(crate) fn imports_config(text: &str, class: &str) -> bool {
    crate::java::annotations(text)
        .iter()
        .filter(|a| a.name == "Import")
        .any(|a| {
            a.args
                .split(',')
                .map(|member| {
                    member
                        .trim()
                        .trim_start_matches(['{', '('])
                        .trim_end_matches(['}', ')'])
                        .trim()
                })
                .any(|member| {
                    member == format!("{class}.class")
                        || member.ends_with(&format!(".{class}.class"))
                })
        })
}

/// The JDBC container types Spring maps to a `DataSource`.
///
/// A closed list rather than "anything ending in Container", because that
/// would read a Kafka, Redis or Toxiproxy container as a database and put the
/// project's datasource check on the wrong class. jails itself only ever
/// writes PostgreSQL; the rest are here because `doctor` runs on projects
/// jails did not create.
const JDBC_CONTAINERS: &[&str] = &[
    "PostgreSQLContainer",
    "MySQLContainer",
    "MariaDBContainer",
    "OracleContainer",
    "MSSQLServerContainer",
    "JdbcDatabaseContainer",
];

/// The test-side container wiring: which class declares the container, and
/// which `@SpringBootTest` classes cannot see it.
///
/// Deliberately textual, like the rest of jails' Java reading -- it answers on
/// a project that does not compile, which is the case that matters when
/// something is already broken.
pub(crate) fn test_container_wiring(root: &Path) -> (Option<String>, Vec<String>) {
    // One tree: managed tests are written beside the reader's own.
    let tests = [root.join("src/test/java")]
        .into_iter()
        .filter(|tree| tree.is_dir())
        .collect::<Vec<_>>();
    // The config this check is about, and only that one. `add kafka` writes a
    // `@TestConfiguration` with `@ServiceConnection` too, and taking whichever
    // the walk saw last would make `doctor` report every `@SpringBootTest` in
    // the project as missing an import of `KafkaTestcontainersConfig` -- under
    // the heading "test datasource", with `jails add db` as the fix, on a
    // project where `add db` is installed and correct.
    //
    // The discriminator is the container's *type*, because the invariant this
    // check exists for is specific to JDBC: once `spring-boot-starter-jdbc` is
    // present, auto-configuration demands a `DataSource` for every
    // `@SpringBootTest`, including ones that never touch a database. A broker
    // has no equivalent demand.
    let config = tests
        .iter()
        .flat_map(|tree| crate::java::types_annotated_with(tree, "TestConfiguration"))
        .filter(|found| {
            crate::java::annotations(&found.source)
                .iter()
                .any(|annotation| annotation.name == "ServiceConnection")
                && crate::java::declares_any_type(&found.source, JDBC_CONTAINERS)
        })
        .filter_map(|found| found.type_name().map(str::to_string))
        .next_back();

    let unimported = match &config {
        Some(class) => tests
            .iter()
            .flat_map(|tree| crate::java::types_annotated_with(tree, "SpringBootTest"))
            .filter_map(|found| {
                let stem = found.type_name()?.to_string();
                // The config class does not import itself.
                (stem != *class && !imports_config(&found.source, class)).then_some(stem)
            })
            .collect(),
        None => Vec::new(),
    };
    (config, unimported)
}

/// A project with a real database whose repository bean is still the
/// in-memory one.
///
/// This is the quiet half of the two-adapter problem. Two annotated
/// adapters is loud -- Spring refuses to start and `jails beans` reports the
/// ambiguity. *One*, on the in-memory adapter, with a `DataSource` sitting
/// right there, starts perfectly and serves every request out of a
/// `ConcurrentHashMap` that is empty on each boot. Nothing fails. The data
/// just is not there, and the first person to notice is whoever asks why
/// last week's records are gone.
///
/// It is the state a project lands in by scaffolding first and running
/// `jails add db` second, because `add db` does not rewrite an
/// already-generated scaffold -- deliberately: those files may have been
/// edited by hand since, and silently regenerating them would cost more than
/// this check does.
pub(crate) fn in_memory_adapter_check(project: &Project) -> Option<Check> {
    let root: &Path = project.root();
    if !project.has_dependency("org.springframework.boot", "spring-boot-starter-jdbc") {
        return None;
    }
    let mut in_memory_beans = Vec::new();
    let mut stack = vec![root.join("src/main/java")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            if path.extension().is_none_or(|e| e != "java") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            // The annotation on a declaration, not the word in a Javadoc.
            let annotated = ["@Component", "@Repository"].iter().any(|a| {
                text.contains(&format!("{a}\npublic class"))
                    || text.contains(&format!("{a}\nclass"))
            });
            if annotated {
                in_memory_beans.push(stem.to_string());
            }
        }
    }
    if in_memory_beans.is_empty() {
        return None;
    }
    in_memory_beans.sort();
    Some(
        Check::new(
            Status::Fail,
            "repository bean",
            format!(
                "this project has a DataSource, but {} is still a bean -- \
                 the application starts and serves every request from memory, losing \
                 everything on restart, with no error anywhere",
                in_memory_beans.join(", ")
            ),
        )
        .fix(
            "re-generate the adapter so the JDBC one is the bean: \
             jails destroy repo <Name> && jails g repo <Name> <fields...>",
        ),
    )
}

/// `spring.sql.init.mode` is set and there is a schema for it to run.
///
/// The failure this catches is silent by design of Spring's own defaults: with
/// `spring.sql.init.mode=always` and no readable `schema.sql`, the context
/// starts perfectly. The tables are simply absent, and the first query to need
/// one fails in front of a user.
///
/// Deliberately **not** a SQL parser. jails does not read SQL and should not
/// start: what it can say exactly is whether the mechanism was switched on and
/// left with nothing to run, which is the whole of the reported failure.
pub(crate) fn sql_init_checks(project: &Project) -> Vec<Check> {
    let root: &Path = project.root();
    let properties =
        std::fs::read_to_string(root.join("src/main/resources/application.properties"))
            .unwrap_or_default();
    let Some(mode) = property_value(&properties, "spring.sql.init.mode") else {
        return Vec::new();
    };
    if mode == "never" {
        return Vec::new();
    }
    // Spring reads `schema.sql` and `data.sql` from the classpath root, and
    // either alone is enough to make the setting mean something.
    let present: Vec<&str> = ["schema.sql", "data.sql"]
        .into_iter()
        .filter(|name| root.join("src/main/resources").join(name).is_file())
        .collect();
    if present.is_empty() {
        return vec![
            Check::new(
                Status::Fail,
                "sql init",
                format!(
                    "spring.sql.init.mode={mode}, and there is no schema.sql or data.sql to \
                     run. The context still starts -- the tables are simply absent, and the \
                     first query to need one fails in front of a user"
                ),
            )
            .fix("add src/main/resources/schema.sql, or set spring.sql.init.mode=never"),
        ];
    }
    // Two schema authorities, and the older one wins the race. This is where
    // an adopted project lands: the checkout arrives with an H2 `schema.sql`
    // and `spring.sql.init.mode=always`, `jails add db` brings Flyway and a
    // PostgreSQL, and Spring then runs the H2 script against PostgreSQL before
    // Flyway sees the database. `INTEGER PRIMARY KEY AUTO_INCREMENT` is not
    // PostgreSQL, so the context fails to start -- and the failure names a
    // script the reader did not know was still running. jails knows both
    // facts, so it says so rather than reporting `ok` over them.
    if let Some(migrations) = flyway_migrations(project) {
        return vec![
            Check::new(
                Status::Fail,
                "sql init",
                format!(
                    "spring.sql.init.mode={mode} runs {}, and {migrations} Flyway migration(s) describe the same schema. Spring runs the script first, so a script written for the old database fails against the new one before the context starts",
                    present.join(" and ")
                ),
            )
            .fix(
                "set spring.sql.init.mode=never once the migrations describe the schema;                  `jails migrate --check` applies them to a scratch database first",
            ),
        ];
    }
    vec![Check::new(
        Status::Ok,
        "sql init",
        format!(
            "spring.sql.init.mode={mode}, running {}",
            present.join(" and ")
        ),
    )]
}

/// How many Flyway migrations this project has.
///
/// Counted rather than asked of the pom: the question here is whether a second
/// description of the schema exists, and a `db/migration` full of `.sql` is
/// that whether or not the dependency is wired -- the wiring has its own check
/// two functions up.
///
fn flyway_migrations(project: &Project) -> Option<usize> {
    let count = std::fs::read_dir(project.root().join("src/main/resources/db/migration"))
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.path().extension().is_some_and(|x| x == "sql"))
                .count()
        })
        .unwrap_or(0);
    (count > 0).then_some(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, at: &str, body: &str) {
        let path = root.join(at);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    /// The check is about the datasource, and a broker's container config is
    /// not one.
    ///
    /// `add db` and `add kafka` both write a `@TestConfiguration` carrying
    /// `@ServiceConnection`. Taking whichever the directory walk saw last
    /// would make `doctor` report every `@SpringBootTest` in the project as
    /// missing an import of `KafkaTestcontainersConfig`, under the heading
    /// "test datasource", and offer `jails add db` as the fix -- on a project
    /// where `add db` is installed and correct.
    #[test]
    fn the_datasource_check_ignores_a_broker_container_config() {
        let root = jails_support::scratch::ScratchDir::in_temp("jails-wiring-kafka")
            .unwrap()
            .keep();
        write(
            &root,
            "src/test/java/com/example/TestcontainersConfig.java",
            "package com.example;\n\n@TestConfiguration\nclass TestcontainersConfig {\n    \
             @Bean\n    @ServiceConnection\n    PostgreSQLContainer postgres() { return null; }\n}\n",
        );
        write(
            &root,
            "src/test/java/com/example/KafkaTestcontainersConfig.java",
            "package com.example;\n\n@TestConfiguration\nclass KafkaTestcontainersConfig {\n    \
             @Bean\n    @ServiceConnection\n    KafkaContainer broker() { return null; }\n}\n",
        );
        write(
            &root,
            "src/test/java/com/example/DemoApplicationTests.java",
            "package com.example;\n\n@SpringBootTest\n@Import(TestcontainersConfig.class)\n\
             class DemoApplicationTests {}\n",
        );

        let (config, unimported) = test_container_wiring(&root);

        assert_eq!(config.as_deref(), Some("TestcontainersConfig"));
        assert!(
            unimported.is_empty(),
            "the test imports the datasource config it needs: {unimported:?}"
        );
    }

    /// A container named only in a Javadoc example does not make the class
    /// that mentions it a datasource config.
    #[test]
    fn a_container_named_in_a_comment_is_not_one_the_class_declares() {
        assert!(!crate::java::declares_any_type(
            "// use PostgreSQLContainer here\nclass Nothing {}\n",
            JDBC_CONTAINERS
        ));
        assert!(crate::java::declares_any_type(
            "class Real { PostgreSQLContainer c; }\n",
            JDBC_CONTAINERS
        ));
    }

    /// Two descriptions of one schema, and the older one wins the race.
    ///
    /// This is where an adopted project lands: the checkout arrives with an H2
    /// `schema.sql` and `spring.sql.init.mode=always`, `jails add db` brings
    /// Flyway and a PostgreSQL, and Spring runs the H2 script against
    /// PostgreSQL before Flyway sees the database. `INTEGER PRIMARY KEY
    /// AUTO_INCREMENT` is not PostgreSQL, so the context fails to start --
    /// naming a script the reader did not know was still running.
    #[test]
    fn a_schema_script_beside_flyway_migrations_is_reported_as_two_authorities() {
        let root = jails_support::scratch::ScratchDir::in_temp("jails-wiring-sql-init")
            .unwrap()
            .keep();
        write(
            &root,
            "src/main/resources/application.properties",
            "spring.sql.init.mode=always\n",
        );
        write(
            &root,
            "src/main/resources/schema.sql",
            "create table t ();\n",
        );
        // `Project::load` infers the base package from a source file.
        write(
            &root,
            "src/main/java/com/example/Application.java",
            "package com.example;\n\nclass Application {}\n",
        );

        // With no migrations there is one authority and nothing to report.
        let project = crate::project::Project::load(&root).unwrap();
        let checks = sql_init_checks(&project);
        assert_eq!(checks.len(), 1);
        assert!(
            matches!(checks[0].status, Status::Ok),
            "{}",
            checks[0].detail
        );

        write(
            &root,
            "src/main/resources/db/migration/V001__create.sql",
            "create table t ();\n",
        );
        let project = crate::project::Project::load(&root).unwrap();
        let checks = sql_init_checks(&project);
        assert!(
            matches!(checks[0].status, Status::Fail),
            "{}",
            checks[0].detail
        );
        let reported = format!("{} {}", checks[0].detail, checks[0].fix);
        assert!(reported.contains("1 Flyway migration"), "{reported}");
        // Every FAIL carries a next step, and this one is the exact property.
        assert!(
            reported.contains("spring.sql.init.mode=never"),
            "{reported}"
        );
    }
}
