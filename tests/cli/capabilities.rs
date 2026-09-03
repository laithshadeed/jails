//! `jails add`, `remove` and `sync`: growing a project by a whole slice.

use super::*;

#[test]
fn add_db_no_start_skips_docker_compose_up() {
    let root = temp_dir("add-db-no-start");
    write_spring_fixture(&root);
    let fake = temp_dir("add-db-no-start-bin");
    let log = fake.join("log.txt");
    write_fake_maven(&fake, &["docker"], &log);

    let output = jails_cmd(&root, Some(&fake))
        .args(["add", "db", "--no-start"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(root.join("compose.yaml").is_file());
    assert!(
        read_log(&log).is_empty(),
        "docker must not be invoked with --no-start: {}",
        read_log(&log)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("jails start"), "{stdout}");
}

/// A side effect that failed says which flag avoids it, on its own line.
///
/// `jails add db` on a machine with no container engine writes every file
/// and exits 1 -- which in `for c in db api cors json sse; do jails add $c ||
/// fail; done` reads as a failed install of the capability that succeeded.
/// The status stays 1, because the services really are not running; the
/// `fix:` names `--no-start`, which makes the same command exit 0.
#[test]
fn an_effect_that_failed_names_the_flag_that_avoids_it() {
    let root = temp_dir("effect-failed-fix");
    write_spring_fixture(&root);
    let fake = temp_dir("effect-failed-fix-bin");
    let log = fake.join("log.txt");
    // No `docker` on PATH at all.
    write_fake_maven(&fake, &[], &log);

    let output = jails_cmd(&root, Some(&fake))
        .args(["add", "db"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!output.status.success(), "{stdout}");
    assert!(stdout.contains("(failed)"), "{stdout}");
    // The project is complete: the status is about the effect and says so.
    assert!(root.join("compose.yaml").is_file());
    assert!(stdout.contains("are written and durable"), "{stdout}");
    assert!(stdout.contains("fix: "), "{stdout}");
    assert!(stdout.contains("`--no-start`"), "{stdout}");

    // And the flag it names really does make the same command succeed.
    let root = temp_dir("effect-failed-fix-ok");
    write_spring_fixture(&root);
    let output = jails_cmd(&root, Some(&fake))
        .args(["add", "db", "--no-start"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("fix: "),
        "a run that worked must not carry a repair line"
    );
}

#[test]
fn add_errors_outside_a_project() {
    let root = temp_dir("add-no-project");
    fs::create_dir_all(&root).unwrap();
    let output = jails_cmd(&root, None)
        .args(["add", "csv"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let told = String::from_utf8_lossy(&output.stderr);
    assert!(told.contains("not a Java project"), "{told}");
    assert!(told.contains("jails new"), "{told}");
}

/// The bar is what the *generated code* needs, not what jails defaults new
/// projects to. 17 has no records-with-sealed-switch, so it is refused; 21 is
/// the floor and must be accepted even though TARGET_RELEASE is higher.
#[test]
fn add_refuses_a_project_targeting_an_older_release() {
    let root = temp_dir("add-old-release");
    write_release_fixture(&root, "17");

    let output = jails_cmd(&root, None)
        .args(["add", "csv"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("targets Java 17"), "{stderr}");
    assert!(
        stderr.contains("21"),
        "the message should name the floor, not the default: {stderr}"
    );
    // The pom is left exactly as it was.
    assert!(
        !fs::read_to_string(root.join("pom.xml"))
            .unwrap()
            .contains("commons-csv")
    );
}

#[test]
fn add_accepts_a_project_pinned_to_an_lts_below_the_jails_default() {
    let root = temp_dir("add-lts-release");
    write_release_fixture(&root, "21");

    let output = jails_cmd(&root, None)
        .args(["add", "csv"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        fs::read_to_string(root.join("pom.xml"))
            .unwrap()
            .contains("commons-csv")
    );
}

/// Re-running an installed capability on a project that has a compose
/// service leaves every later mutating command -- `add`, `sync`, `g record`
/// -- working.
///
/// The shape needs all three: a capability that declares a compose service,
/// a second capability, and that second one run twice, because the compose
/// service is what makes the repeated run non-trivial while changing no
/// declaration.
#[test]
fn reinstalling_a_capability_beside_a_compose_service_leaves_the_project_usable() {
    let root = temp_dir("add-twice-with-compose");
    write_spring_fixture(&root);

    for arguments in [
        &["add", "db", "--no-start"][..],
        &["add", "cors"][..],
        &["add", "cors"][..],
    ] {
        let output = jails_cmd(&root, None).args(arguments).output().unwrap();
        assert!(
            output.status.success(),
            "{arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // What is asserted is the commands *after* the second install, not that
    // install.
    for arguments in [
        &["sync", "--no-start"][..],
        &["g", "record", "Note", "title:string!"][..],
    ] {
        let output = jails_cmd(&root, None).args(arguments).output().unwrap();
        assert!(
            output.status.success(),
            "{arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // And `doctor` reports no unfinished transaction.
    let doctor = jails_cmd(&root, None).arg("doctor").output().unwrap();
    let report = String::from_utf8_lossy(&doctor.stdout);
    assert!(
        !report.contains("started and did not finish"),
        "doctor still reports an interrupted transaction:\n{report}"
    );
}

/// `g event` never writes Kafka code into a project with no Kafka.
///
/// An event declaration is a payload record and nothing else; the listener,
/// the error handler and the dead-letter routing belong to the `kafka`
/// capability, so a project without it gets no line of Spring Kafka. There is
/// no arrangement of commands that writes an import the build cannot resolve.
#[test]
fn generating_an_event_without_kafka_refuses_and_names_the_capability() {
    let root = temp_dir("event-without-kafka");
    write_spring_fixture(&root);

    let written = jails_cmd(&root, None)
        .args(["g", "event", "Shipped"])
        .output()
        .unwrap();
    // No refusal and no Spring Kafka: the payload record is all an event
    // declaration is without the capability.
    assert!(written.status.success(), "{written:?}");
    let payload = common::read_generated(
        &root,
        "src/main/java/com/example/demo/domain/events/ShippedEvent.java",
    );
    assert!(!payload.contains("springframework.kafka"), "{payload}");
    assert!(
        !root
            .join("src/main/java/com/example/demo/messaging")
            .exists(),
        "an event without the capability still wrote the messaging package"
    );

    // And it is a precondition, not a ban: with the capability installed the
    // same command works.
    let installed = jails_cmd(&root, None)
        .args(["add", "kafka", "--no-start"])
        .output()
        .unwrap();
    assert!(
        installed.status.success(),
        "{}",
        String::from_utf8_lossy(&installed.stderr)
    );
    let generated = jails_cmd(&root, None)
        .args(["g", "event", "Shipped"])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
}

#[test]
fn add_dry_run_changes_nothing() {
    let root = temp_dir("add-dry-run");
    write_spring_fixture(&root);
    let before = fs::read_to_string(root.join("pom.xml")).unwrap();

    let output = jails_cmd(&root, None)
        .args(["add", "csv", "--dry-run"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    // A preview is the prepared transition, projected: `plan <id> apply`
    // followed by one line per operation. It names the pom it would rewrite
    // and the file it would create, which is what a dry run is for.
    assert!(stdout.contains("plan "), "{stdout}");
    assert!(stdout.contains("pom.xml"), "{stdout}");
    assert!(stdout.contains("CsvReader.java"), "{stdout}");

    assert_eq!(before, fs::read_to_string(root.join("pom.xml")).unwrap());
    assert!(
        !root
            .join("src/main/java/com/example/demo/CsvReader.java")
            .exists()
    );
}

#[test]
fn add_is_idempotent() {
    let root = temp_dir("add-idempotent");
    write_spring_fixture(&root);

    assert!(
        jails_cmd(&root, None)
            .args(["add", "csv"])
            .status()
            .unwrap()
            .success()
    );
    let after_first = fs::read_to_string(root.join("pom.xml")).unwrap();

    let output = jails_cmd(&root, None)
        .args(["add", "csv"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    // "Nothing happened" and "everything happened and changed nothing" are
    // kept apart, and only the second has files to name. A second `add csv`
    // is the first.
    assert!(stdout.contains("nothing to do"), "{stdout}");
    assert_eq!(
        after_first,
        fs::read_to_string(root.join("pom.xml")).unwrap(),
        "second add rewrote the pom"
    );
    assert_eq!(
        1,
        after_first.matches("commons-csv").count(),
        "duplicate dependency"
    );
}

#[test]
fn add_name_override_renames_the_generated_class() {
    let root = temp_dir("add-named");
    write_spring_fixture(&root);

    assert!(
        jails_cmd(&root, None)
            .args(["add", "csv", "--name", "transaction"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        common::generated(
            &root,
            "src/main/java/com/example/demo/adapters/TransactionReader.java"
        )
        .exists()
    );
    assert!(
        common::generated(
            &root,
            "src/test/java/com/example/demo/adapters/TransactionReaderTest.java"
        )
        .exists()
    );
}

/// The bar that matters: does `add csv` leave a project that actually
/// compiles and passes its tests? Needs real Maven and a JDK new enough for
/// the release jails targets.
#[test]
fn add_csv_produces_a_project_that_compiles_and_passes_tests() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    if !real_java_supports_target_release() {
        skip(&format!(
            "javac on PATH does not support --release {TARGET_RELEASE}"
        ));
        return;
    }
    let path = real_path_without_mvnd();
    let workdir = temp_dir("real-add-csv");
    jails_cmd_with_path(&workdir, &path)
        .args(["new-cli", "demo"])
        .status()
        .unwrap();
    let root = workdir.join("demo");

    let status = jails_cmd_with_path(&root, &path)
        .args(["add", "csv"])
        .status()
        .unwrap();
    assert!(status.success());

    let verified = verified_plain_toolbox(&path);
    assert!(
        verified
            .join("target/classes/com/example/demo/adapters/CsvReader.class")
            .is_file()
    );
}

#[test]
fn add_sqlite_writes_a_first_migration_and_both_classes() {
    let root = temp_dir("add-sqlite-files");
    write_spring_fixture(&root);

    assert!(
        jails_cmd(&root, None)
            .args(["add", "sqlite"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        common::generated(
            &root,
            "src/main/java/com/example/demo/adapters/Database.java"
        )
        .is_file()
    );
    assert!(
        common::generated(
            &root,
            "src/main/java/com/example/demo/adapters/Migrations.java"
        )
        .is_file()
    );
    assert!(
        common::generated(
            &root,
            "src/test/java/com/example/demo/adapters/DatabaseTest.java"
        )
        .is_file()
    );
    // One migration naming rule across the tool, `V<version>__<what>.sql`,
    // which is what lets the materializer allocate the next version from the
    // history it observed rather than from a name a capability chose. sqlite's
    // runner is jails' own `Migrations`, which sorts by filename and does not
    // care -- but Flyway, which `storage postgres` uses, reads only that shape,
    // and two conventions in one `db/migration` directory is how a migration
    // comes to sit there being ignored.
    assert!(
        root.join("src/main/resources/db/migration/V001__sqlite_init.sql")
            .is_file()
    );
}

#[test]
fn add_db_installs_postgres_flyway_and_testcontainers_without_an_orm() {
    let root = temp_dir("add-db-files");
    write_spring_fixture(&root);
    let fake = temp_dir("add-db-bin");
    let log = fake.join("log.txt");
    write_fake_maven(&fake, &["docker"], &log);

    let output = jails_cmd(&root, Some(&fake))
        .args(["add", "db"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    for artifact in [
        "postgresql",
        "flyway-core",
        "flyway-database-postgresql",
        "testcontainers-postgresql",
        "testcontainers-junit-jupiter",
    ] {
        assert!(pom.contains(artifact), "missing {artifact}: {pom}");
    }
    assert!(
        !pom.contains("hibernate") && !pom.contains("jpa"),
        "db must not pull in an ORM: {pom}"
    );
    assert!(
        root.join("src/main/resources/db/migration/.gitkeep")
            .is_file()
    );
    let compose = fs::read_to_string(root.join("compose.yaml")).unwrap();
    assert!(compose.contains("postgres:17-alpine"), "{compose}");
    assert!(compose.contains("# jails:db"), "{compose}");
    // The compose document handed to `--file` is the frozen one the plan
    // published, not the live `compose.yaml`. The effect is attempted after
    // the plan publishes, so between the two somebody may edit the file;
    // running against what they wrote would start services this transition
    // never described, and a retry would not repeat what the first attempt
    // did. `--project-directory` is what keeps relative paths in that
    // document resolving against the project.
    let invocation = read_log(&log);
    assert!(
        invocation.contains("compose") && invocation.contains("up -d"),
        "expected docker compose up: {invocation}"
    );
    assert!(
        invocation.contains("postgres"),
        "expected the postgres service named: {invocation}"
    );
    assert!(
        invocation.contains(&format!("--project-directory {}", root.display())),
        "expected the project directory: {invocation}"
    );
    // The frozen document is staged *outside* the project, so what is checked
    // is the property rather than a location: whatever `--file` names, it is
    // not the live `compose.yaml` this transition just published.
    let live = format!("--file {}", root.join("compose.yaml").display());
    assert!(
        !invocation.contains(&live),
        "expected the frozen document rather than the live compose.yaml: {invocation}"
    );
    assert!(
        !root.join(".jails/objects").exists(),
        "a canonical project must not grow a legacy object store"
    );
}

#[test]
fn add_db_on_spring_wires_docker_compose_support() {
    let root = temp_dir("add-db-spring");
    write_spring_fixture(&root);
    let fake = temp_dir("add-db-spring-bin");
    let log = fake.join("log.txt");
    write_fake_maven(&fake, &["docker"], &log);

    assert!(
        jails_cmd(&root, Some(&fake))
            .args(["add", "db", "--no-start"])
            .status()
            .unwrap()
            .success()
    );
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("spring-boot-starter-jdbc"));
    assert!(pom.contains("spring-boot-docker-compose"));
    assert!(
        pom.contains("spring-boot-testcontainers"),
        "@ServiceConnection and the container-bean lifecycle live there: {pom}"
    );
    assert!(pom.contains("<optional>true</optional>"));
    let config = common::generated(
        &root,
        "src/test/java/com/example/demo/TestcontainersConfig.java",
    );
    assert!(config.is_file(), "missing {}", config.display());
    let config_src = fs::read_to_string(&config).unwrap();
    assert!(
        !config_src.contains("ApplicationContextInitializer"),
        "the global initializer made every slice start a container; it is gone: {config_src}"
    );
    assert!(config_src.contains("@ServiceConnection"), "{config_src}");
    assert!(config_src.contains("@TestConfiguration"), "{config_src}");
    assert!(
        !root
            .join("src/test/resources/META-INF/spring.factories")
            .is_file(),
        "the container is imported now, not registered globally"
    );
    // The @SpringBootTest that came with the project has to be wired, or JDBC
    // auto-config fails it with "Failed to determine a suitable driver class"
    // on a test the user never wrote.
    let tests = common::read_generated(
        &root,
        "src/test/java/com/example/demo/DemoApplicationTests.java",
    );
    assert!(
        tests.contains("@Import(TestcontainersConfig.class)"),
        "{tests}"
    );
    let properties =
        fs::read_to_string(root.join("src/main/resources/application.properties")).unwrap();
    assert!(
        properties.contains("spring.persistence.exceptiontranslation.enabled=false"),
        "{properties}"
    );

    // A compiled shadow of a file `remove` is about to delete. `mvn test` is
    // incremental, so a `.class` left under `target/test-classes` after its
    // source is gone goes on being loaded, and the removal looks like it did
    // not happen.
    let stale_class = root.join("target/test-classes/com/example/demo/TestcontainersConfig.class");
    fs::create_dir_all(stale_class.parent().unwrap()).unwrap();
    fs::write(&stale_class, []).unwrap();

    assert!(
        jails_cmd(&root, Some(&fake))
            .args(["remove", "db", "--force"])
            .status()
            .unwrap()
            .success()
    );
    let tests = common::read_generated(
        &root,
        "src/test/java/com/example/demo/DemoApplicationTests.java",
    );
    assert!(!tests.contains("TestcontainersConfig"), "{tests}");
    assert!(!config.is_file());
    assert!(
        !root
            .join("src/test/resources/META-INF/spring.factories")
            .is_file()
    );
    assert!(
        !root
            .join("src/main/resources/application.properties")
            .is_file(),
        "fixture had no properties file; remove should delete the one add created"
    );
    assert!(
        !stale_class.is_file(),
        "remove db must drop the compiled initializer or incremental tests keep loading it"
    );
}

#[test]
fn remove_db_refuses_while_a_scaffold_still_needs_it() {
    let root = temp_dir("remove-db-with-scaffold");
    write_spring_fixture(&root);

    assert!(
        jails_cmd(&root, None)
            .args(["add", "db", "--no-start"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        jails_cmd(&root, None)
            .args(["g", "scaffold", "Article", "id:uuid@pk", "title:string!"])
            .status()
            .unwrap()
            .success()
    );
    let before = snapshot_tree(&root);
    let refused = jails_cmd(&root, None)
        .args(["remove", "db", "--force"])
        .output()
        .unwrap();

    assert!(!refused.status.success(), "{refused:?}");
    let stderr = String::from_utf8_lossy(&refused.stderr);
    // The refusal names the accepted storage rather than the declaration that
    // still wants it: retiring a table is a schema-evolution step with a
    // forward migration, and that is the fix it points at.
    assert!(stderr.contains("abandon accepted storage"), "{stderr}");
    assert!(stderr.contains("fix: "), "{stderr}");
    assert_eq!(snapshot_tree(&root), before, "refusal mutated the project");
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("spring-boot-starter-jdbc"), "{pom}");
    assert!(
        common::generated(
            &root,
            "src/main/java/com/example/demo/adapters/JdbcArticleRepository.java"
        )
        .is_file()
    );
}

/// `add db` over a project whose container config is already there rewrites it
/// as an importable `@TestConfiguration` and splices the `@Import` into every
/// `@SpringBootTest` -- including one in a different package, which needs the
/// import statement too.
#[test]
fn add_db_on_spring_wires_every_test_through_an_imported_configuration() {
    let root = temp_dir("add-db-spring-migrate");
    write_spring_fixture(&root);
    let fake = temp_dir("add-db-spring-migrate-bin");
    let log = fake.join("log.txt");
    write_fake_maven(&fake, &["docker"], &log);

    fs::write(
        common::generated(
            &root,
            "src/test/java/com/example/demo/PostgresContainerConfig.java",
        ),
        r#"package com.example.demo;

import org.springframework.beans.factory.support.BeanDefinitionRegistry;
import org.springframework.boot.test.context.TestConfiguration;
import org.springframework.boot.testcontainers.service.connection.ServiceConnection;
import org.springframework.context.ApplicationContextInitializer;
import org.springframework.context.ConfigurableApplicationContext;
import org.springframework.context.annotation.AnnotatedBeanDefinitionReader;
import org.springframework.context.annotation.Bean;
import org.testcontainers.postgresql.PostgreSQLContainer;

public class PostgresContainerConfig
        implements ApplicationContextInitializer<ConfigurableApplicationContext> {

    @Override
    public void initialize(ConfigurableApplicationContext context) {
        if (context instanceof BeanDefinitionRegistry registry) {
            new AnnotatedBeanDefinitionReader(registry).register(Containers.class);
        }
    }

    @TestConfiguration(proxyBeanMethods = false)
    public static class Containers {

        @Bean
        @ServiceConnection
        PostgreSQLContainer postgresContainer() {
            return new PostgreSQLContainer("postgres:17-alpine");
        }
    }
}
"#,
    )
    .unwrap();
    let api = common::generated(&root, "src/test/java/com/example/demo/api");
    fs::create_dir_all(&api).unwrap();
    fs::write(
        api.join("ExtraSliceTest.java"),
        r#"package com.example.demo.api;

import org.junit.jupiter.api.Test;
import org.springframework.boot.test.context.SpringBootTest;

@SpringBootTest
class ExtraSliceTest {

    @Test
    void contextLoads() {}
}
"#,
    )
    .unwrap();

    assert!(
        jails_cmd(&root, Some(&fake))
            .args(["add", "db", "--no-start"])
            .status()
            .unwrap()
            .success()
    );

    let config = common::read_generated(
        &root,
        "src/test/java/com/example/demo/TestcontainersConfig.java",
    );
    assert!(
        !config.contains("ApplicationContextInitializer"),
        "the global registration is what this migration removes: {config}"
    );
    assert!(config.contains("@ServiceConnection"), "{config}");

    // Both @SpringBootTest classes get the import, including the one in a
    // different package -- which needs the extra import statement too.
    let tests = common::read_generated(
        &root,
        "src/test/java/com/example/demo/DemoApplicationTests.java",
    );
    assert!(
        tests.contains("@Import(TestcontainersConfig.class)"),
        "{tests}"
    );
    let slice = fs::read_to_string(api.join("ExtraSliceTest.java")).unwrap();
    assert!(
        slice.contains("@Import(TestcontainersConfig.class)"),
        "{slice}"
    );
    assert!(
        slice.contains("import com.example.demo.TestcontainersConfig;"),
        "a test in another package needs the config imported by name: {slice}"
    );
}

/// The failure `jails check` actually hits after `add db` on a Spring project:
/// Docker Compose is skipped in tests, so JDBC auto-config has no URL. A
/// test-classpath ApplicationContextInitializer is what makes every
/// `@SpringBootTest` (and therefore `mvn verify`) green.
#[test]
fn add_db_on_spring_makes_context_loads_pass() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    if !real_java_supports_target_release() {
        skip(&format!(
            "javac on PATH does not support --release {TARGET_RELEASE}"
        ));
        return;
    }
    if !real_docker_available() {
        skip("docker daemon not available");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-add-db-spring");
    write_spring_fixture(&root);
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    let pom = pom.replace(
        "<java.version>26</java.version>",
        &format!("<java.version>{TARGET_RELEASE}</java.version>"),
    );
    fs::write(root.join("pom.xml"), pom).unwrap();

    // The failure `add db` actually hits in a real app: JDBC auto-config
    // CGLIB-proxies every `@Repository`, and jails-style classes are `final`.
    fs::write(
        common::generated(
            &root,
            "src/main/java/com/example/demo/InMemoryThingRepository.java",
        ),
        r#"package com.example.demo;

import org.springframework.stereotype.Repository;

@Repository
public final class InMemoryThingRepository {}
"#,
    )
    .unwrap();

    // `add db` wires every @SpringBootTest that exists when the capability is
    // reconciled, so this cross-package test goes in first; created
    // afterwards it would depend on a PostgreSQL listening on localhost:5432.
    let api = common::generated(&root, "src/test/java/com/example/demo/api");
    fs::create_dir_all(&api).unwrap();
    fs::write(
        api.join("ExtraSliceTest.java"),
        r#"package com.example.demo.api;

import org.junit.jupiter.api.Test;
import org.springframework.boot.test.context.SpringBootTest;

@SpringBootTest
class ExtraSliceTest {

    @Test
    void contextLoads() {}
}
"#,
    )
    .unwrap();

    let status = jails_cmd_with_path(&root, &path)
        .args(["add", "db", "--no-start"])
        .status()
        .unwrap();
    assert!(status.success(), "add db failed");

    let extra = fs::read_to_string(api.join("ExtraSliceTest.java")).unwrap();
    assert!(
        extra.contains("@Import(TestcontainersConfig.class)"),
        "cross-package SpringBootTest was not wired: {extra}"
    );
    assert!(
        extra.contains("import com.example.demo.TestcontainersConfig;"),
        "cross-package config import is missing: {extra}"
    );

    let status = jails_cmd_with_path(&root, &path)
        .arg("test")
        .status()
        .unwrap();
    assert!(
        status.success(),
        "mvn test failed after `jails add db` on a Spring project (every existing @SpringBootTest needs the imported container config)"
    );
}

#[test]
fn add_kafka_stacks_onto_db_compose_and_remove_undoes_one_side() {
    let root = temp_dir("add-kafka-stack");
    write_spring_fixture(&root);
    let fake = temp_dir("add-kafka-bin");
    let log = fake.join("log.txt");
    write_fake_maven(&fake, &["docker"], &log);

    assert!(
        jails_cmd(&root, Some(&fake))
            .args(["add", "db", "kafka", "--no-start"])
            .status()
            .unwrap()
            .success()
    );
    let compose = fs::read_to_string(root.join("compose.yaml")).unwrap();
    assert!(compose.contains("  postgres:"));
    assert!(compose.contains("  kafka:"));
    assert!(compose.contains("apache/kafka:4.1.0"));
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("kafka-clients"));
    assert!(pom.contains("postgresql"));

    assert!(
        jails_cmd(&root, Some(&fake))
            .args(["remove", "db", "--force"])
            .status()
            .unwrap()
            .success()
    );
    let compose = fs::read_to_string(root.join("compose.yaml")).unwrap();
    assert!(!compose.contains("postgres:"), "{compose}");
    assert!(compose.contains("  kafka:"), "{compose}");
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(
        !pom.contains("<artifactId>postgresql</artifactId>"),
        "{pom}"
    );
    assert!(pom.contains("kafka-clients"));
    assert!(root.join("compose.yaml").is_file());

    assert!(
        jails_cmd(&root, Some(&fake))
            .args(["remove", "kafka", "--force"])
            .status()
            .unwrap()
            .success()
    );
    assert!(!root.join("compose.yaml").exists());
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(!pom.contains("kafka-clients"));
}

#[test]
fn remove_is_the_inverse_of_add_csv() {
    let root = temp_dir("remove-csv");
    write_spring_fixture(&root);
    assert!(
        jails_cmd(&root, None)
            .args(["add", "csv"])
            .status()
            .unwrap()
            .success()
    );
    let reader = common::generated(
        &root,
        "src/main/java/com/example/demo/adapters/CsvReader.java",
    );
    assert!(reader.is_file(), "{}", common::managed_listing(&root));

    // `--force` because nothing is connected to answer the prompt: a piped
    // command that cannot be asked has not consented, and the alternative --
    // treating silence as a no and exiting 0 -- is a script that believes it
    // removed something.
    let output = jails_cmd(&root, None)
        .args(["remove", "csv", "--force"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!reader.exists());
    assert!(
        !root
            .join("src/test/java/com/example/demo/adapters/CsvReaderTest.java")
            .exists()
    );
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(!pom.contains("commons-csv"), "{pom}");
}

/// The prompt names the files, and `--force` is what skips it.
#[test]
fn remove_without_force_prompts_and_aborts_on_no() {
    let root = temp_dir("remove-prompt");
    write_spring_fixture(&root);
    assert!(
        jails_cmd(&root, None)
            .args(["add", "csv"])
            .status()
            .unwrap()
            .success()
    );
    let reader = common::generated(
        &root,
        "src/main/java/com/example/demo/adapters/CsvReader.java",
    );
    let written = fs::read_to_string(&reader).unwrap();

    let mut child = jails_cmd(&root, None)
        .args(["remove", "csv"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"n\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("aborted"), "{stdout}");
    assert!(stdout.contains("CsvReader.java"), "{stdout}");
    assert_eq!(
        fs::read_to_string(&reader).unwrap(),
        written,
        "an aborted remove must leave the files"
    );

    // And `--force` is what does not ask.
    assert!(
        jails_cmd(&root, None)
            .args(["remove", "csv", "--force"])
            .status()
            .unwrap()
            .success()
    );
    assert!(!reader.exists(), "the forced removal left its file");
}

/// Capabilities have to compose: adding all three must leave one pom with
/// three dependencies and no clobbered files.
#[test]
fn capabilities_stack_without_clobbering_each_other() {
    let root = temp_dir("add-stacked");
    write_plain_fixture(&root);

    for capability in ["csv", "sqlite", "json"] {
        let output = jails_cmd(&root, None)
            .args(["add", capability])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "add {capability}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    for artifact in ["commons-csv", "sqlite-jdbc", "jackson-databind"] {
        assert_eq!(
            1,
            pom.matches(artifact).count(),
            "expected exactly one {artifact} dependency"
        );
    }
    let pkg = common::generated(&root, "src/main/java/com/example/demo");
    assert!(pkg.join("adapters/CsvReader.java").is_file());
    assert!(pkg.join("adapters/Database.java").is_file());
    assert!(pkg.join("adapters/Json.java").is_file());
}

#[test]
fn add_accepts_multiple_capabilities_in_one_invocation() {
    let root = temp_dir("add-multiple");
    write_plain_fixture(&root);
    let fake = temp_dir("add-multiple-bin");
    let log = fake.join("log.txt");
    write_fake_maven(&fake, &["docker"], &log);

    let output = jails_cmd(&root, Some(&fake))
        .args(["add", "db", "json", "testkit", "--no-start"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    for artifact in ["postgresql", "jackson-databind"] {
        let declaration = format!("<artifactId>{artifact}</artifactId>");
        assert_eq!(
            1,
            pom.matches(&declaration).count(),
            "missing {artifact}: {pom}"
        );
    }
    let main = common::generated(&root, "src/main/java/com/example/demo");
    assert!(main.join("adapters/Json.java").is_file());
    let test = common::generated(&root, "src/test/java/com/example/demo");
    assert!(test.join("testkit/Clocks.java").is_file());
}

/// The real bar for the whole `add` surface: every capability, stacked into
/// one project, compiles and passes its generated tests.
#[test]
fn every_capability_together_produces_a_project_that_compiles_and_passes_tests() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    if !real_java_supports_target_release() {
        skip(&format!(
            "javac on PATH does not support --release {TARGET_RELEASE}"
        ));
        return;
    }
    let path = real_path_without_mvnd();
    let root = verified_plain_toolbox(&path);
    for class in ["CsvReader", "Database", "Json"] {
        assert!(
            root.join(format!(
                "target/classes/com/example/demo/adapters/{class}.class"
            ))
            .is_file(),
            "{class} was not compiled in the stacked capability matrix"
        );
    }
}

/// `add cors` renders one test for Boot 2 and another for the current
/// default, and both branches are compiled:
/// `a_boot_2_project_gets_the_classic_mockmvc_for_every_generated_web_test`
/// runs real Maven over the classic `MockMvc` variant, and this runs it over
/// the `MockMvcTester` branch every project `jails new` produces gets.
///
/// The origin is the other half. `.invalid` is reserved by RFC 2606 so the
/// placeholder is unmistakably a value somebody has to replace -- and the test
/// must still pass once they replace it, or the capability ships a red build
/// the moment it is configured. So the test reads the configured origin rather
/// than restating the placeholder.
#[test]
fn add_cors_on_the_default_boot_version_compiles_and_runs_its_own_test() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    if !real_java_supports_target_release() {
        skip(&format!(
            "javac on PATH does not support --release {TARGET_RELEASE}"
        ));
        return;
    }
    let path = real_path_without_mvnd();
    let root = verified_spring_toolbox(&path);
    let test = fs::read_to_string(common::generated(
        root,
        "src/test/java/com/example/demo/CorsConfigTest.java",
    ))
    .expect("add cors did not write its test into the Boot 4 toolbox");
    assert!(
        test.contains("servlet.assertj.MockMvcTester"),
        "the toolbox rendered the legacy variant, so the default branch is still unexecuted"
    );
    assert!(
        root.join("target/test-classes/com/example/demo/CorsConfigTest.class")
            .is_file(),
        "the Boot 4 CORS test was never compiled"
    );
    // Configured, not restated: the reader has to replace `.invalid`, and the
    // test has to survive it.
    assert!(
        !test.contains("\"https://example.invalid\""),
        "the test hardcodes the placeholder origin, so editing the property makes it red"
    );
}

/// `add db` has a Boot floor, and never produces an unresolvable build: a
/// coordinate spliced with no version on a parent that does not manage it
/// (`org.springframework.boot:spring-boot-flyway:`, trailing colon) fails
/// every goal, leaving the project worse off than before the command ran.
///
/// Three boundaries, each checked in `deps/spring-boot`:
/// `spring-boot-testcontainers` and `spring-boot-docker-compose` appear at
/// 3.1, `flyway-database-postgresql` becomes managed at 3.3, and
/// `spring-boot-flyway` exists only at 4.0. Below 4 the auto-configuration is
/// still in `spring-boot-autoconfigure`, so the module must not be named at
/// all; below 3.1 there is no honest answer and `db` refuses.
#[test]
fn add_db_matches_the_modules_this_boot_version_has_or_refuses_by_name() {
    let parent = temp_dir("add-db-boot-floor");
    let build = |name: &str, boot: &str| {
        let created = jails_cmd(&parent, None)
            .args([
                "new",
                name,
                "--gradle",
                "--offline",
                "--boot",
                boot,
                "--java",
                "21",
                "--package",
                "com.acme.svc",
                "--no-devtools",
                "--no-git",
            ])
            .output()
            .unwrap();
        assert!(
            created.status.success(),
            "{}",
            String::from_utf8_lossy(&created.stderr)
        );
        parent.join(name)
    };

    let old = build("two", "2.7.18");
    let refused = jails_cmd(&old, None)
        .args(["add", "db", "--no-start"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    let said = String::from_utf8_lossy(&refused.stderr);
    assert!(said.contains("spring-boot-testcontainers"), "{said}");
    assert!(said.contains("Spring Boot 2.7"), "{said}");
    assert!(said.contains("jails add sqlite"), "{said}");
    assert!(
        !old.join("build.gradle").exists()
            || !fs::read_to_string(old.join("build.gradle"))
                .unwrap()
                .contains("flyway"),
        "the refusal still spliced a dependency"
    );

    // A supported Boot 3: Flyway's auto-configuration is in
    // `spring-boot-autoconfigure`, so the split-out module must not be named.
    // Both Flyway artifacts go in versionless here, because 3.3 is exactly the
    // release whose BOM starts managing `flyway-database-postgresql`.
    // Inventing a version Boot already manages is how the pair comes to
    // disagree with the rest of the curated set.
    let supported = build("three", "3.3.5");
    assert!(
        jails_cmd(&supported, None)
            .args(["add", "db", "--no-start"])
            .status()
            .unwrap()
            .success()
    );
    let gradle = fs::read_to_string(supported.join("build.gradle")).unwrap();
    assert!(!gradle.contains("spring-boot-flyway"), "{gradle}");
    assert!(gradle.contains("org.flywaydb:flyway-core'"), "{gradle}");
    assert!(
        gradle.contains("org.flywaydb:flyway-database-postgresql'"),
        "{gradle}"
    );
    assert!(gradle.contains("spring-boot-testcontainers"), "{gradle}");

    // And below that boundary the pin is back, because nothing else supplies
    // one and a versionless Gradle dependency simply fails to resolve.
    let unmanaged = build("early", "3.2.12");
    assert!(
        jails_cmd(&unmanaged, None)
            .args(["add", "db", "--no-start"])
            .status()
            .unwrap()
            .success()
    );
    let gradle = fs::read_to_string(unmanaged.join("build.gradle")).unwrap();
    assert!(gradle.contains("org.flywaydb:flyway-core:"), "{gradle}");
    assert!(
        gradle.contains("org.flywaydb:flyway-database-postgresql:"),
        "{gradle}"
    );
}

/// The Spring flavor branch: `add json` must *omit* the version so Spring
/// Boot's parent supplies its curated Jackson, and the result must still
/// compile. The shared Spring fixture stays pinned at an older release (it
/// exists to test `generate`, which is release-agnostic), so this raises it
/// to the release `add` requires.
#[test]
fn add_json_on_a_spring_project_defers_to_the_parents_version_and_compiles() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    if !real_java_supports_target_release() {
        skip(&format!(
            "javac on PATH does not support --release {TARGET_RELEASE}"
        ));
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-add-json-spring");
    write_spring_fixture(&root);
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    let pom = pom.replace(
        "<java.version>26</java.version>",
        &format!("<java.version>{TARGET_RELEASE}</java.version>"),
    );
    fs::write(root.join("pom.xml"), pom).unwrap();

    let status = jails_cmd_with_path(&root, &path)
        .args(["add", "json"])
        .status()
        .unwrap();
    assert!(status.success());

    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    let block_start = pom.find("jackson-databind").unwrap();
    let block_end = pom[block_start..].find("</dependency>").unwrap() + block_start;
    assert!(
        !pom[block_start..block_end].contains("<version>"),
        "should defer to the parent's managed version"
    );

    let verified = verified_spring_toolbox(&path);
    assert!(verified.join("target/test-classes").is_dir());
}

#[test]
fn add_db_upgrades_an_out_of_date_properties_block() {
    // `add` promises to write whatever is missing. A project whose properties
    // hold only the exception-translation property inside a marked block must
    // gain the datasource keys; reporting that as "exists" would leave it
    // permanently without them.
    let root = temp_dir("db-properties-upgrade");
    write_spring_fixture(&root);
    let fake = root.join("fake-bin");
    write_fake_maven(&fake, &["mvn", "mvnd", "docker"], &root.join("mvn.log"));
    let properties = root.join("src/main/resources/application.properties");
    fs::create_dir_all(properties.parent().unwrap()).unwrap();
    fs::write(
        &properties,
        "spring.application.name=demo\n\
         # jails:db\n\
         spring.persistence.exceptiontranslation.enabled=false\n\
         # /jails:db\n",
    )
    .unwrap();

    assert!(
        jails_cmd(&root, Some(&fake))
            .args(["add", "db", "--no-start"])
            .status()
            .unwrap()
            .success()
    );

    let next = fs::read_to_string(&properties).unwrap();
    assert!(next.contains("spring.application.name=demo"), "{next}");
    assert!(
        next.contains("spring.datasource.url=jdbc:postgresql://"),
        "{next}"
    );
    assert!(
        next.contains("spring.docker.compose.enabled=false"),
        "{next}"
    );
    // The markers dissolve. A capability's properties are claimed one key at
    // a time, so a marked block around them is adopted and its comment lines
    // go -- and the key inside it is written once, by the capability that
    // owns it, rather than twice.
    assert!(!next.contains("# jails:db"), "{next}");
    assert_eq!(
        next.matches("spring.persistence.exceptiontranslation.enabled=false")
            .count(),
        1,
        "{next}"
    );
}

#[test]
fn add_api_generates_problem_detail_handling_that_compiles_and_passes() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-add-api");
    write_spring_fixture(&root);

    let status = jails_cmd_with_path(&root, &path)
        .args(["add", "api"])
        .status()
        .unwrap();
    assert!(status.success());

    let handler = common::read_generated(
        &root,
        "src/main/java/com/example/demo/api/ApiExceptionHandler.java",
    );
    // Spring's own base class, so framework exceptions keep their statuses.
    assert!(
        handler.contains("extends ResponseEntityExceptionHandler"),
        "{handler}"
    );
    // RFC 9457, not a hand-rolled error envelope.
    assert!(
        handler.contains("ProblemDetail.forStatusAndDetail"),
        "{handler}"
    );
    // Field errors ride in an extension member rather than a bespoke shape.
    assert!(
        handler.contains("problem.setProperty(\"fields\""),
        "{handler}"
    );
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("spring-boot-starter-validation"), "{pom}");

    let verified = verified_spring_toolbox(&path);
    assert!(verified.join("target/test-classes").is_dir());
}

#[test]
fn add_cache_switches_caching_on_and_proves_it() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-add-cache");
    write_spring_fixture(&root);

    assert!(
        jails_cmd_with_path(&root, &path)
            .args(["add", "cache"])
            .status()
            .unwrap()
            .success()
    );

    let properties =
        fs::read_to_string(root.join("src/main/resources/application.properties")).unwrap();
    // A cache with no bound is a memory leak with a friendly name.
    assert!(properties.contains("maximumSize="), "{properties}");
    // No `# jails:cache` marker: each property is owned by key and retired by
    // key, so there is no block boundary. The settings are what the
    // capability claims.
    assert!(properties.contains("spring.cache.type="), "{properties}");

    let verified = verified_spring_toolbox(&path);
    assert!(verified.join("target/test-classes").is_dir());
}

#[test]
fn add_actuator_exposes_health_and_nothing_dangerous() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-add-actuator");
    write_spring_fixture(&root);

    assert!(
        jails_cmd_with_path(&root, &path)
            .args(["add", "actuator"])
            .status()
            .unwrap()
            .success()
    );

    let properties =
        fs::read_to_string(root.join("src/main/resources/application.properties")).unwrap();
    assert!(
        properties.contains(
            "management.endpoints.web.exposure.include=health,info,prometheus,threaddump"
        )
    );
    assert!(properties.contains("management.server.port=8081"));
    assert!(properties.contains("management.endpoints.web.base-path=/management"));
    assert!(properties.contains("management.endpoint.health.cache.time-to-live=5s"));
    assert!(properties.contains("info.app.name=@project.name@"));
    // `*` publishes heapdump and the resolved environment; never generate it.
    assert!(!properties.contains("include=*"), "{properties}");

    let verified = verified_spring_toolbox(&path);
    assert!(verified.join("target/test-classes").is_dir());
}

#[test]
fn add_observability_serves_a_prometheus_scrape() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-add-observability");
    write_spring_fixture(&root);

    assert!(
        jails_cmd_with_path(&root, &path)
            .args(["add", "observability"])
            .status()
            .unwrap()
            .success()
    );

    let properties =
        fs::read_to_string(root.join("src/main/resources/application.properties")).unwrap();
    assert!(properties.contains("exposure.include=health,info,prometheus,threaddump"));
    assert!(properties.contains("management.server.port=8081"));
    assert!(properties.contains("management.endpoints.web.base-path=/management"));
    assert!(properties.contains(
        "management.metrics.distribution.slo.http.server.requests=100ms,250ms,500ms,1s,2s,5s,10s"
    ));
    assert!(properties.contains("management.tracing.propagation.type=w3c"));
    assert!(properties.contains("server.tomcat.accesslog.directory=/dev"));
    assert!(properties.contains("management.server.tomcat.accesslog.prefix=stdout"));
    assert!(!properties.contains("include=*"), "{properties}");

    let verified = verified_spring_toolbox(&path);
    // The generated PrometheusScrapeTest is what proves the endpoint serves;
    // a green run with that class never loaded would prove nothing.
    let surefire = fs::read_dir(verified.join("target/surefire-reports"))
        .unwrap()
        .filter_map(|e| e.ok())
        .any(|e| {
            e.file_name()
                .to_string_lossy()
                .contains("PrometheusScrapeTest")
        });
    assert!(surefire, "PrometheusScrapeTest did not run");
}

#[test]
fn adding_actuator_after_observability_keeps_prometheus_exposed() {
    let root = temp_dir("observability-then-actuator");
    write_spring_fixture(&root);

    for capability in ["observability", "actuator"] {
        assert!(
            jails_cmd(&root, None)
                .args(["add", capability])
                .status()
                .unwrap()
                .success()
        );
    }

    // Properties are last-wins and `actuator` was added second, so without the
    // union its narrower list would silently un-expose the scrape endpoint.
    let properties =
        fs::read_to_string(root.join("src/main/resources/application.properties")).unwrap();
    for line in properties
        .lines()
        .filter(|l| l.starts_with("management.endpoints.web.exposure.include="))
    {
        assert!(line.contains("prometheus"), "{properties}");
    }
}

#[test]
fn a_spring_capability_is_refused_in_a_plain_maven_project() {
    let root = temp_dir("api-plain-maven");
    fs::write(
        root.join("pom.xml"),
        "<project><artifactId>x</artifactId>\
         <properties><maven.compiler.release>27</maven.compiler.release></properties></project>",
    )
    .unwrap();
    fs::create_dir_all(common::generated(&root, "src/main/java/com/example/demo")).unwrap();
    fs::write(
        common::generated(&root, "src/main/java/com/example/demo/App.java"),
        "package com.example.demo;\npublic class App {}\n",
    )
    .unwrap();

    let output = jails_cmd(&root, None)
        .args(["add", "api"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Spring Boot capability"), "{stderr}");
    assert!(stderr.contains("jails add http"), "{stderr}");
}

#[test]
fn capability_property_blocks_do_not_clobber_each_other() {
    let root = temp_dir("property-blocks");
    write_spring_fixture(&root);
    let fake = root.join("fake-bin");
    write_fake_maven(&fake, &["mvn", "mvnd", "docker"], &root.join("mvn.log"));

    for capability in ["cache", "actuator"] {
        assert!(
            jails_cmd(&root, Some(&fake))
                .args(["add", capability])
                .status()
                .unwrap()
                .success()
        );
    }
    let properties =
        fs::read_to_string(root.join("src/main/resources/application.properties")).unwrap();
    // Both capabilities' settings are present, and neither wrapped the
    // other's: each key is owned on its own, so there is no block for a
    // second capability to clobber.
    assert!(properties.contains("spring.cache.type="), "{properties}");
    assert!(
        properties.contains("management.endpoints.web.exposure.include="),
        "{properties}"
    );

    // Removing one leaves the other exactly as it was.
    assert!(
        jails_cmd(&root, Some(&fake))
            .args(["remove", "cache", "--force"])
            .status()
            .unwrap()
            .success()
    );
    let after = fs::read_to_string(root.join("src/main/resources/application.properties")).unwrap();
    assert!(!after.contains("spring.cache.type"), "{after}");
    assert!(
        after.contains("management.endpoints.web.exposure.include"),
        "{after}"
    );
}

#[test]
fn add_kafka_and_generate_event_compile_against_real_spring() {
    // Compile-only for the messaging slice: its test is an `IT`, so Failsafe
    // runs it in `verify` (it starts a broker, which costs seconds). What
    // this pins is that the generated code is valid against the real Spring
    // Kafka API -- including the Jackson-prefixed serializers, since the
    // older pair is deprecated for removal.
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-kafka-slice");
    write_spring_fixture(&root);
    let fake = root.join("fake-bin");
    write_fake_maven(&fake, &["docker"], &root.join("docker.log"));

    assert!(
        jails_cmd(&root, Some(&fake))
            .args(["add", "kafka", "--no-start"])
            .status()
            .unwrap()
            .success()
    );
    let properties =
        fs::read_to_string(root.join("src/main/resources/application.properties")).unwrap();
    assert!(
        properties.contains("auto-offset-reset=earliest"),
        "{properties}"
    );
    assert!(
        properties.contains("JacksonJsonDeserializer"),
        "{properties}"
    );
    assert!(
        !properties.contains("serializer.JsonDeserializer"),
        "{properties}"
    );
    // Both the base package and a wildcard under it: the match is neither a
    // prefix nor recursive, so `com.example.demo` alone rejects the payload
    // `g event` writes into `com.example.demo.messaging`.
    assert!(
        properties.contains("trusted.packages=com.example.demo,com.example.demo.*"),
        "{properties}"
    );
    // The consumer group is the artifactId, not the checkout directory: a
    // group is a durable identity in the broker, and two clones of one
    // service under different directory names would otherwise each receive
    // every message instead of splitting the work.
    assert!(
        properties.contains("spring.kafka.consumer.group-id=demo"),
        "{properties}"
    );

    assert!(
        jails_cmd_with_path(&root, &path)
            .args([
                "generate",
                "event",
                "PayoutSettled",
                "id:uuid",
                "payoutId:uuid",
                "amount:decimal",
                "occurredAt:instant",
            ])
            .status()
            .unwrap()
            .success()
    );

    let listener = common::read_generated(
        &root,
        "src/main/java/com/example/demo/messaging/PayoutSettledListener.java",
    );
    // No catch: swallowing here commits an offset for a message that was
    // never processed, which is data loss wearing a success badge.
    assert!(!listener.contains("catch ("), "{listener}");

    let publisher = common::read_generated(
        &root,
        "src/main/java/com/example/demo/messaging/PayoutSettledPublisher.java",
    );
    // Keyed sends: ordering is per partition, and a null key round-robins.
    assert!(
        publisher.contains("kafka.send(topic, String.valueOf(event.id()), event)"),
        "{publisher}"
    );

    let event = common::read_generated(
        &root,
        "src/main/java/com/example/demo/messaging/PayoutSettledEvent.java",
    );
    // One component per line, in declaration order: a Java record's positional
    // constructor is ABI, so the order is what the assertion is about.
    let components = event
        .lines()
        .skip_while(|line| !line.starts_with("public record PayoutSettledEvent("))
        .skip(1)
        .take_while(|line| !line.starts_with(')'))
        .map(|line| line.trim().trim_end_matches(',').to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        components,
        [
            "UUID id",
            "UUID payoutId",
            "BigDecimal amount",
            "Instant occurredAt"
        ],
        "{event}"
    );

    // `test` runs Surefire only, so the IT is compiled but not executed.
    let verified = verified_spring_services_toolbox(&path);
    assert!(
        verified
            .join("target/test-classes/com/example/demo/messaging/PayoutSettledMessagingIT.class")
            .is_file(),
        "the shared Spring toolbox did not compile the Kafka integration test"
    );
}

#[test]
fn add_security_writes_an_explicit_chain_that_denies_by_default() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-add-security");
    write_spring_fixture(&root);

    // Actuator first: the chain permits `/management/health` and the test
    // asserts it, so the endpoint has to exist.
    for capability in ["actuator", "security"] {
        assert!(
            jails_cmd_with_path(&root, &path)
                .args(["add", capability])
                .status()
                .unwrap()
                .success()
        );
    }

    let config =
        common::read_generated(&root, "src/main/java/com/example/demo/SecurityConfig.java");
    // Default deny: a new endpoint is protected until someone says otherwise.
    assert!(config.contains(".anyRequest()"), "{config}");
    assert!(config.contains(".authenticated()"), "{config}");
    // CSRF is only disabled alongside STATELESS -- the two are safe together
    // and unsafe apart, so neither should appear without the other.
    assert!(
        config.contains("SessionCreationPolicy.STATELESS"),
        "{config}"
    );
    assert!(
        config.contains("csrf(AbstractHttpConfigurer::disable)"),
        "{config}"
    );
    // Only health is public. `env` and `heapdump` must not be.
    assert!(config.contains("/management/health/**"), "{config}");
    assert!(!config.contains("/management/**"), "{config}");

    // The dependency the generated test needs, spliced by the same rule that
    // supplies AssertJ and Failsafe. Boot 4 moved `@WebMvcTest` into
    // `spring-boot-webmvc-test`, which `spring-boot-starter-test` does not
    // bring in -- so without this the project compiles every production
    // source and then stops on the test jails itself wrote, `mvn verify`
    // runs no test at all, and the generated Dockerfile fails too, because
    // `-DskipTests` still compiles test sources.
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("spring-boot-starter-webmvc-test"), "{pom}");

    let verified = verified_spring_services_toolbox(&path);
    assert!(verified.join("target/test-classes").is_dir());
    // And it really compiled and ran, rather than being skipped: the services
    // toolbox runs `mvn test` over exactly this capability set.
    assert!(
        verified
            .join("target/test-classes/com/example/demo/SecurityConfigTest.class")
            .is_file(),
        "the generated security test did not compile"
    );
}

#[test]
fn add_redis_wires_a_ttl_enforcing_store_and_a_compose_service() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-add-redis");
    write_spring_fixture(&root);
    let fake = root.join("fake-bin");
    write_fake_maven(&fake, &["docker"], &root.join("docker.log"));

    assert!(
        jails_cmd(&root, Some(&fake))
            .args(["add", "redis", "--no-start"])
            .status()
            .unwrap()
            .success()
    );

    let compose = fs::read_to_string(root.join("compose.yaml")).unwrap();
    assert!(compose.contains("redis:7-alpine"), "{compose}");
    // A cache with a volume hides the one bug caches reliably have: code
    // that only works because something was already cached.
    assert!(!compose.contains("redis-data"), "{compose}");

    let store = common::read_generated(
        &root,
        "src/main/java/com/example/demo/adapters/KeyValueStore.java",
    );
    // Every write carries a lifetime. `set(k, v)` with no expiry stores a key
    // forever, which is a memory leak that survives restarts.
    assert!(store.contains("set(key, value, ttl)"), "{store}");
    assert!(!store.contains("set(key, value)"), "{store}");

    // The IT compiles here; it is run by `verify`, not `test`, because it
    // starts a container.
    let verified = verified_spring_services_toolbox(&path);
    assert!(
        verified
            .join("target/test-classes/com/example/demo/adapters/KeyValueStoreIT.class")
            .is_file(),
        "the shared Spring toolbox did not compile the Redis integration test"
    );
}

#[test]
fn add_help_lists_worked_examples() {
    let workdir = temp_dir("add-help");
    let output = jails_cmd(&workdir, None)
        .args(["add", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("jails add db kafka redis"), "{help}");
    assert!(help.contains("is the exact inverse"), "{help}");
}

/// `jails add csv security` on a plain Maven project: `security` is Spring-only
/// and is refused, and `csv` is not applied either. Planning is pure and is
/// where that refusal lives, so every requested capability is planned before
/// any is applied.
#[test]
fn add_preflights_every_capability_before_applying_any_of_them() {
    let root = temp_dir("add-preflight");
    write_plain_fixture(&root);
    let before = fs::read_to_string(root.join("pom.xml")).unwrap();

    let output = jails_cmd(&root, None)
        .args(["add", "csv", "security"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "security is Spring-only and must be refused on a plain Maven project"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("nothing was written"),
        "the failure should say the other capabilities were not applied: {stderr}"
    );
    assert_eq!(
        fs::read_to_string(root.join("pom.xml")).unwrap(),
        before,
        "csv was applied even though the same command's `security` was refused"
    );
}

/// The order must not matter: a refusal named last still has to stop the ones
/// named before it.
#[test]
fn add_preflight_holds_when_the_refused_capability_is_named_first() {
    let root = temp_dir("add-preflight-order");
    write_plain_fixture(&root);
    let before = fs::read_to_string(root.join("pom.xml")).unwrap();

    let output = jails_cmd(&root, None)
        .args(["add", "security", "csv"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(fs::read_to_string(root.join("pom.xml")).unwrap(), before);
}

/// The loop that makes a manifest trustworthy: `add` records what it applied,
/// so nobody has to maintain the file, and `remove` takes it back out -- left
/// listed, the next `sync` would put back what was just removed.
#[test]
fn add_records_what_it_applied_and_remove_takes_it_back_out() {
    let root = temp_dir("manifest-round-trip");
    write_spring_fixture(&root);

    assert!(
        jails_cmd(&root, None)
            .args(["add", "csv"])
            .status()
            .unwrap()
            .success()
    );
    // The model is the manifest: a project declares what it is made of in the
    // one editable source every later command reads, so there is no second
    // list that can disagree with it.
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(
        model.contains("cap csv @id(cap_csv)"),
        "add did not record the capability it applied:\n{model}"
    );
    assert!(
        !root.join("jails.toml").exists(),
        "a canonical project must not grow a second capability list"
    );

    assert!(
        jails_cmd(&root, None)
            .args(["remove", "csv", "--force"])
            .status()
            .unwrap()
            .success()
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(
        !model.contains("csv"),
        "remove left the capability declared, so the next sync would restore it:\n{model}"
    );
}

/// The case the manifest exists for: a project that declares what it is made
/// of and does not have it yet -- a fresh clone, or one taking a newer jails'
/// output. One command, and the `[layout]` renames apply at the same time.
#[test]
fn sync_applies_what_the_manifest_declares() {
    let root = temp_dir("manifest-sync");
    write_spring_fixture(&root);
    // The model *is* the manifest, and the case this test exists for is a
    // declaration that arrived without its output: somebody edited the model
    // in an editor, or merged a branch that added a capability. `sync` is the
    // command that makes the tree match what the file says.
    common::become_canonical(&root);
    let model = root.join(".jails/model.jdl");
    let declared = format!(
        "{}\ncap csv @id(cap_csv)\n",
        fs::read_to_string(&model).unwrap().trim_end()
    );
    fs::write(&model, declared).unwrap();

    // --pretend first: it answers "what is this project missing?".
    let before_dry_run = snapshot_tree(&root);
    let preview = jails_cmd(&root, None)
        .args(["sync", "--dry-run"])
        .output()
        .unwrap();
    assert!(preview.status.success());
    let shown = String::from_utf8_lossy(&preview.stdout);
    assert!(shown.contains("plan "), "{shown}");
    assert!(shown.contains("create "), "{shown}");
    assert_eq!(
        snapshot_tree(&root),
        before_dry_run,
        "--dry-run wrote files"
    );

    assert!(
        jails_cmd(&root, None)
            .args(["sync"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        common::generated(
            &root,
            "src/main/java/com/example/demo/adapters/CsvReader.java"
        )
        .is_file(),
        "sync did not apply the declared capability:\n{}",
        common::managed_listing(&root)
    );
}

/// Every capability is idempotent, so a sync over a project that is already
/// correct changes nothing and says so rather than reporting work.
#[test]
fn sync_over_a_correct_project_changes_nothing() {
    let root = temp_dir("manifest-sync-idempotent");
    write_spring_fixture(&root);
    jails_cmd(&root, None)
        .args(["add", "csv"])
        .status()
        .unwrap();

    let pom_before = fs::read_to_string(root.join("pom.xml")).unwrap();
    let output = jails_cmd(&root, None).args(["sync"]).output().unwrap();
    assert!(output.status.success());
    let shown = String::from_utf8_lossy(&output.stdout);
    assert!(shown.contains("nothing to do"), "{shown}");
    assert_eq!(
        fs::read_to_string(root.join("pom.xml")).unwrap(),
        pom_before
    );
}

/// A project with no manifest is not an error -- most projects never have
/// one. It says what the file is for instead of failing.
#[test]
fn sync_without_a_manifest_explains_rather_than_fails() {
    let root = temp_dir("manifest-sync-absent");
    write_spring_fixture(&root);

    let output = jails_cmd(&root, None).args(["sync"]).output().unwrap();
    assert!(output.status.success());
    let shown = String::from_utf8_lossy(&output.stdout);
    assert!(shown.contains("no capabilities are declared"), "{shown}");
    assert!(shown.contains("jails add"), "{shown}");
}

/// A capability jails does not know would sit in the file looking applied and
/// never sync, which is the failure a manifest exists to remove.
#[test]
fn sync_refuses_a_manifest_naming_a_capability_that_does_not_exist() {
    let root = temp_dir("manifest-sync-typo");
    write_plain_fixture(&root);
    fs::write(
        root.join("jails.toml"),
        "[project]\ncapabilities = [\"postgress\"]\n",
    )
    .unwrap();

    let output = jails_cmd(&root, None).args(["sync"]).output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown capability `postgress`"),
        "{stderr}"
    );
    assert!(stderr.contains("db"), "should list the real ones: {stderr}");
}

/// "It exists" is not ownership. `remove` deletes every generated file the
/// plan names, and a `CsvReader` someone spent an afternoon on looks exactly
/// like the stub jails wrote.
///
/// jails does not refuse -- `remove` is the documented inverse of `add`, and
/// refusing would make it unusable on the projects that got the most out of
/// it. It must not delete them *silently*.
#[test]
fn remove_names_generated_files_that_were_edited_before_deleting_them() {
    let root = temp_dir("remove-edited-files");
    write_spring_fixture(&root);
    jails_cmd(&root, None)
        .args(["add", "csv"])
        .status()
        .unwrap();

    let generated = common::generated(
        &root,
        "src/main/java/com/example/demo/adapters/CsvReader.java",
    );
    let mut edited = fs::read_to_string(&generated).unwrap();
    edited.push_str("\n// an afternoon of work\n");
    fs::write(&generated, edited).unwrap();

    // --force is the silent path: it skips the confirmation prompt entirely.
    let output = jails_cmd(&root, None)
        .args(["remove", "csv", "--force"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let shown = String::from_utf8_lossy(&output.stdout);
    // Named before it goes. Without `--force` this same list is what the
    // confirmation puts to the reader, so an edit is never lost without being
    // seen first.
    assert!(
        shown.contains("delete ") && shown.contains("CsvReader.java"),
        "an edited generated file was deleted with no mention of it:\n{shown}"
    );
}

/// The counterpart, and the one that keeps the warning worth reading: a
/// project whose generated files are untouched gets no noise.
#[test]
fn remove_says_nothing_about_files_that_were_not_edited() {
    let root = temp_dir("remove-unedited-files");
    write_plain_fixture(&root);
    jails_cmd(&root, None)
        .args(["add", "csv"])
        .status()
        .unwrap();

    let output = jails_cmd(&root, None)
        .args(["remove", "csv", "--force"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let shown = String::from_utf8_lossy(&output.stdout);
    assert!(
        !shown.contains("changed since jails wrote"),
        "warned about a file nobody touched:\n{shown}"
    );
}

/// `--dry-run` is where you look before deciding, so it has to say so too.
#[test]
fn dry_run_remove_names_edited_files() {
    let root = temp_dir("remove-edited-dry-run");
    write_spring_fixture(&root);
    jails_cmd(&root, None)
        .args(["add", "csv"])
        .status()
        .unwrap();

    let generated = common::generated(
        &root,
        "src/main/java/com/example/demo/adapters/CsvReader.java",
    );
    fs::write(
        &generated,
        "package com.example.demo.adapters;\nclass CsvReader {}\n",
    )
    .unwrap();

    // A dry run reports what would happen, and what would happen is a
    // refusal: jails does not throw away bytes it did not write. The file is
    // named, and so is the flag that authorises losing the edits.
    let refused = jails_cmd(&root, None)
        .args(["remove", "csv", "--dry-run"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    let told = String::from_utf8_lossy(&refused.stderr);
    assert!(told.contains("CsvReader.java"), "{told}");
    assert!(told.contains("`--force`"), "{told}");
    assert!(generated.is_file(), "--dry-run deleted the file");

    // And with the authorisation, the same dry run names it before it goes.
    let output = jails_cmd(&root, None)
        .args(["remove", "csv", "--dry-run", "--force"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let shown = String::from_utf8_lossy(&output.stdout);
    assert!(shown.contains("delete "), "{shown}");
    assert!(shown.contains("CsvReader.java"), "{shown}");
    assert!(generated.is_file(), "--dry-run deleted the file");
}

/// Every claim in `EventHub`'s Javadoc is a behavioural one, so the only
/// place they can be checked is against a real JUnit run -- especially the
/// concurrency test, which is the reason the registry is a
/// `ConcurrentHashMap` of `newKeySet()` rather than the obvious `HashMap`.
#[test]
fn add_sse_produces_tests_that_run_and_pass() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-sse");
    write_spring_fixture(&root);

    let status = jails_cmd_with_path(&root, &path)
        .args(["add", "sse", "--no-start"])
        .status()
        .unwrap();
    assert!(status.success(), "add sse failed");

    let verified = verified_spring_toolbox(&path);
    assert_surefire_test_count(verified, "EventHubTest", 4);
}

/// `add mail`. The generated IT starts a container, so only compilation is
/// checked here — but that is the part that catches the Boot 4 API changes,
/// and the IT's shape is copied from Boot's own.
#[test]
fn add_mail_produces_a_project_that_compiles() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-mail");
    write_spring_fixture(&root);

    let output = jails_cmd_with_path(&root, &path)
        .args(["add", "mail", "--no-start"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let verified = verified_spring_services_toolbox(&path);
    assert!(
        verified
            .join("target/test-classes/com/example/demo/MailerIT.class")
            .is_file(),
        "the shared Spring services toolbox did not compile the mail integration test"
    );
}

/// `add dependency` splices one artifact jails has never heard of into the
/// pom surgically.
///
/// Both directions in one test, because the value is not the splice, it is
/// that the splice is *owned*, so `remove` takes exactly it back out and
/// nothing else.
#[test]
fn a_declared_dependency_is_spliced_and_can_be_taken_back_out() {
    let root = temp_dir("declare-dependency");
    write_spring_fixture(&root);
    let before = fs::read_to_string(root.join("pom.xml")).unwrap();

    let output = jails_cmd(&root, None)
        .args([
            "add",
            "dependency",
            "com.h2database:h2",
            "--version",
            "2.3.232",
            "--scope",
            "runtime",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let spliced = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(spliced.contains("<artifactId>h2</artifactId>"), "{spliced}");
    assert!(spliced.contains("<scope>runtime</scope>"), "{spliced}");

    let output = jails_cmd(&root, None)
        .args(["remove", "dependency", "com.h2database:h2"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(root.join("pom.xml")).unwrap(),
        before,
        "retiring a declared dependency must leave the pom byte-identical"
    );
}

/// A coordinate with a version in it is the commonest paste, so the refusal
/// names the flag rather than repeating the shape back.
#[test]
fn a_coordinate_carrying_a_version_is_refused_by_naming_the_flag() {
    let root = temp_dir("declare-coordinate-version");
    write_plain_fixture(&root);
    let output = jails_cmd(&root, None)
        .args(["add", "dependency", "com.h2database:h2:2.3.232"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--version"), "{stderr}");
}

/// A setting nobody's capability owns, and the test-only override that keeps
/// a suite off the application's own datasource.
///
/// `--tests` writes `config/` deliberately. The obvious spelling --
/// `src/test/resources/application.properties` -- shadows the main file
/// wholesale, so this asserts the main file is still standing afterwards.
#[test]
fn a_set_property_is_owned_and_the_test_overlay_is_additive() {
    let root = temp_dir("declare-property");
    write_plain_fixture(&root);

    for args in [
        vec!["set", "server.port=3000"],
        vec!["set", "spring.datasource.url=jdbc:h2:mem:test", "--tests"],
    ] {
        let output = jails_cmd(&root, None).args(&args).output().unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    assert_eq!(
        fs::read_to_string(root.join("src/main/resources/application.properties")).unwrap(),
        "server.port=3000\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("src/test/resources/config/application.properties")).unwrap(),
        "spring.datasource.url=jdbc:h2:mem:test\n"
    );
    assert!(
        !root
            .join("src/test/resources/application.properties")
            .exists(),
        "the overlay must not be the spelling that shadows the main file"
    );

    let output = jails_cmd(&root, None)
        .args(["unset", "server.port"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    // The overlay is a different entity, keyed by its own path: retiring one
    // must not reach the other.
    assert!(
        root.join("src/test/resources/config/application.properties")
            .exists()
    );
}

/// A `@unique` violation answers 409, in whichever order the two capabilities
/// arrived: jails puts `@unique` in the schema and generates an
/// `ApiException.Conflict` documented "Becomes a 409", and the advice connects
/// them, or a duplicate reaches the client as a 500, which is what alerting
/// pages on and what client libraries retry.
///
/// The advice can only name `DuplicateKeyException` when `spring-tx` is on the
/// classpath, which arrives with the JDBC starter -- so this is conditional.
/// The compiler recompiles the whole model on every command, so there is no
/// half-planned state and no order to get wrong. The precondition survives
/// where it is real: an *operation* that answers a route through a
/// `JdbcClient` adapter refuses when the model declares no SQL storage,
/// because that project cannot start.
#[test]
fn a_duplicate_key_answers_409_whichever_order_db_and_api_arrived() {
    let handler = "src/main/java/com/example/demo/api/ApiExceptionHandler.java";

    let together = temp_dir("conflict-together");
    write_spring_fixture(&together);
    let fake = temp_dir("conflict-together-bin");
    write_fake_maven(&fake, &["docker"], &fake.join("log.txt"));
    let output = jails_cmd(&together, Some(&fake))
        .args(["add", "db", "api", "--no-start"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let advice = common::read_generated(&together, handler);
    assert!(
        advice.contains("DuplicateKeyException"),
        "one command, both capabilities: the advice must map it\n{advice}"
    );

    // The two orders that are not one command. `db` then `api` is the same
    // project, because the compiler renders from the whole model rather than
    // from what the last command happened to see.
    let forwards = temp_dir("conflict-forwards");
    write_spring_fixture(&forwards);
    for capability in ["db", "api"] {
        let output = jails_cmd(&forwards, Some(&fake))
            .args(["add", capability, "--no-start"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "add {capability}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(
        common::read_generated(&forwards, handler),
        advice,
        "one command and two must render the same advice"
    );

    // And so is `api` then `db`.
    let backwards = temp_dir("conflict-backwards");
    write_spring_fixture(&backwards);
    for capability in ["api", "db"] {
        let output = jails_cmd(&backwards, Some(&fake))
            .args(["add", capability, "--no-start"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "add {capability}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(
        common::read_generated(&backwards, handler),
        advice,
        "the compiler renders from the whole model, so neither order is special"
    );
}
