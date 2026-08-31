//! The commands that drive a toolchain: `run`, `test`, `build`, `clean`,
//! `check`, `start`/`stop`, `db`, `console`, `testd`, `bench` and `adopt`.
//! Most are tier 2 -- a fake `mvn` that logs its argv -- so read them for
//! *which command was constructed*, not for what Maven then did.

use super::*;

/// The four things `jails test` can do that `mvn test` cannot without you
/// assembling the arguments: rerun the failures, stop at the first one, name
/// what took the time, and resolve a file and a line.
///
/// Driven against a fake `mvn` and hand-written reports, because what is
/// under test is the *argument construction and the report reading*, not
/// Maven. The real-toolchain half is covered by the projects that actually
/// run tests.
#[test]
fn test_flags_rerun_failures_stop_early_and_name_the_slowest() {
    let root = temp_dir("test-flags");
    write_project_skeleton(&root);
    let fake_dir = temp_dir("test-flags-bin");
    let log = fake_dir.join("log.txt");
    write_fake_maven(&fake_dir, &["mvn"], &log);

    // What Surefire would have left behind: one pass, one failure, one
    // error, one skip.
    let reports = root.join("target/surefire-reports");
    fs::create_dir_all(&reports).unwrap();
    fs::write(
        reports.join("TEST-com.example.demo.PayoutTest.xml"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuite name="com.example.demo.PayoutTest">
  <testcase name="settles" classname="com.example.demo.PayoutTest" time="2.50"/>
  <testcase name="rejectsNull" classname="com.example.demo.PayoutTest" time="0.10">
    <failure message="boom">stack</failure>
  </testcase>
  <testcase name="todo" classname="com.example.demo.PayoutTest" time="0">
    <skipped/>
  </testcase>
</testsuite>
"#,
    )
    .unwrap();

    let output = jails_cmd(&root, Some(&fake_dir))
        .args(["test", "--failed"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("rerunning 1 failed test"),
        "--failed should say what it is about to do: {stdout}"
    );
    let invocation = read_log(&log);
    assert!(
        invocation.contains("-Dtest=PayoutTest#rejectsNull"),
        "--failed reruns exactly the failure, not the class: {invocation}"
    );
    assert!(
        !invocation.contains("todo"),
        "a skipped test is not a failure: {invocation}"
    );

    let output = jails_cmd(&root, Some(&fake_dir))
        .args(["test", "--slowest", "2"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("slowest 2 test(s)"), "{stdout}");
    assert!(
        stdout.contains("2.50s  com.example.demo.PayoutTest#settles"),
        "{stdout}"
    );

    let _ = jails_cmd(&root, Some(&fake_dir))
        .args(["test", "--fail-fast"])
        .status()
        .unwrap();
    let invocation = read_log(&log);
    assert!(
        invocation.contains("-Dsurefire.skipAfterFailureCount=1"),
        "{invocation}"
    );

    // A file and a line: JUnit has no FileSelector, so jails resolves the
    // enclosing @Test itself. This is what an editor keybinding sends.
    let test_file = common::generated(&root, "src/test/java/com/example/demo/PayoutTest.java");
    fs::create_dir_all(test_file.parent().unwrap()).unwrap();
    fs::write(
        &test_file,
        "package com.example.demo;

import org.junit.jupiter.api.Test;

class PayoutTest {

    @Test
    void settles() {
        assertThat(true).isTrue();
    }
}
",
    )
    .unwrap();
    let _ = jails_cmd(&root, Some(&fake_dir))
        .args(["test", "src/test/java/com/example/demo/PayoutTest.java:9"])
        .status()
        .unwrap();
    let invocation = read_log(&log);
    assert!(
        invocation.contains("-Dtest=PayoutTest#settles"),
        "a line inside a test runs that test: {invocation}"
    );
}

#[test]
fn test_command_prefers_mvnd_when_present_and_passes_the_filter() {
    let root = temp_dir("mock-test-mvnd");
    write_project_skeleton(&root);
    let fake_dir = temp_dir("mock-test-mvnd-bin");
    let log = fake_dir.join("log.txt");
    write_fake_maven(&fake_dir, &["mvn", "mvnd"], &log);

    let status = jails_cmd(&root, Some(&fake_dir))
        .args(["test", "PostTest"])
        .status()
        .unwrap();
    assert!(status.success());

    let invocation = read_log(&log);
    assert!(
        invocation.contains("/mvnd "),
        "expected mvnd to be preferred: {invocation}"
    );
    assert!(invocation.contains("test"));
    assert!(invocation.contains("-Dtest=PostTest"));
}

#[test]
fn test_command_falls_back_to_mvn_when_mvnd_is_absent() {
    let root = temp_dir("mock-test-mvn-only");
    write_project_skeleton(&root);
    let fake_dir = temp_dir("mock-test-mvn-only-bin");
    let log = fake_dir.join("log.txt");
    write_fake_maven(&fake_dir, &["mvn"], &log);

    let status = jails_cmd(&root, Some(&fake_dir))
        .args(["test"])
        .status()
        .unwrap();
    assert!(status.success());
    assert!(read_log(&log).contains("/mvn "));
}

#[test]
fn test_command_infers_unit_and_integration_test_names() {
    let root = temp_dir("mock-test-inference");
    write_project_skeleton(&root);
    let fake_dir = temp_dir("mock-test-inference-bin");
    let log = fake_dir.join("log.txt");
    write_fake_maven(&fake_dir, &["mvn"], &log);

    assert!(
        jails_cmd(&root, Some(&fake_dir))
            .args(["test", "Money"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        jails_cmd(&root, Some(&fake_dir))
            .args(["test", "RewardSchemaIT"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        jails_cmd(&root, Some(&fake_dir))
            .args(["test", "BankApplicationTests"])
            .status()
            .unwrap()
            .success()
    );

    // `Class#method` is the case that was silently wrong: the `Test` suffix
    // was appended to the whole filter (`Payout#settlesTest`), and the
    // Failsafe routing decision was then taken on that mangled string, so
    // `PayoutIT#settles` went to Surefire -- which does not run `*IT` -- and
    // Maven reported success having executed nothing.
    for filter in ["Payout#settles", "PayoutIT#settles"] {
        assert!(
            jails_cmd(&root, Some(&fake_dir))
                .args(["test", filter])
                .status()
                .unwrap()
                .success()
        );
    }

    let invocation = read_log(&log);
    assert!(invocation.contains("test -Dtest=MoneyTest"), "{invocation}");
    assert!(
        invocation.contains("verify -Dit.test=RewardSchemaIT"),
        "{invocation}"
    );
    assert!(
        invocation.contains("test -Dtest=BankApplicationTests"),
        "{invocation}"
    );
    assert!(
        invocation.contains("test -Dtest=PayoutTest#settles"),
        "the suffix belongs to the class, not the method: {invocation}"
    );
    assert!(
        invocation.contains("verify -Dit.test=PayoutIT#settles"),
        "an *IT is Failsafe's whatever the method is called: {invocation}"
    );
    // A filter that matches nothing is "no tests ran", not a build failure
    // with a stack trace.
    assert!(
        invocation.contains("-Dsurefire.failIfNoSpecifiedTests=false"),
        "{invocation}"
    );
}

#[test]
fn test_command_partitions_multiple_selectors_without_dropping_one() {
    let root = temp_dir("mock-test-partitions");
    write_project_skeleton(&root);
    let fake_dir = temp_dir("mock-test-partitions-bin");
    let log = fake_dir.join("log.txt");
    write_fake_maven(&fake_dir, &["mvn"], &log);

    let status = jails_cmd(&root, Some(&fake_dir))
        .args(["test", "Invoice", "PayoutIT#settles", "--engine", "build"])
        .status()
        .unwrap();
    assert!(status.success());
    let invocation = read_log(&log);
    assert!(
        invocation.contains("test -Dtest=InvoiceTest"),
        "{invocation}"
    );
    assert!(
        invocation.contains("verify -Dit.test=PayoutIT#settles"),
        "{invocation}"
    );
}

#[test]
fn test_command_scope_and_repeat_are_owned_by_the_coordinator() {
    let root = temp_dir("mock-test-scope-repeat");
    write_project_skeleton(&root);
    let fake_dir = temp_dir("mock-test-scope-repeat-bin");
    let log = fake_dir.join("log.txt");
    write_fake_maven(&fake_dir, &["mvn"], &log);

    let status = jails_cmd(&root, Some(&fake_dir))
        .args([
            "test",
            "--scope",
            "integration",
            "--engine",
            "build",
            "--repeat",
            "2",
        ])
        .status()
        .unwrap();
    assert!(status.success());
    let invocation = read_log(&log);
    assert_eq!(invocation.lines().count(), 2, "{invocation}");
    assert!(
        invocation
            .lines()
            .all(|line| line.contains("verify -Dsurefire.skip=true")),
        "{invocation}"
    );
}

#[test]
fn test_command_explains_the_canonical_partition() {
    let root = temp_dir("mock-test-explain");
    write_project_skeleton(&root);
    let fake_dir = temp_dir("mock-test-explain-bin");
    let log = fake_dir.join("log.txt");
    write_fake_maven(&fake_dir, &["mvn"], &log);

    let output = jails_cmd(&root, Some(&fake_dir))
        .args([
            "test",
            "LedgerTest",
            "--engine",
            "build",
            "--explain-selection",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("engine policy: Build"), "{stdout}");
    assert!(stdout.contains("Maven: LedgerTest"), "{stdout}");
    assert!(stdout.contains("reason: Requested"), "{stdout}");
}

#[test]
fn compile_none_never_compiles_ineligible_warm_tests_and_strict_warm_refuses() {
    let root = temp_dir("mock-test-isolation");
    write_plain_fixture(&root);
    let source = common::generated(&root, "src/test/java/com/example/demo/ContextTest.java");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(
        &source,
        "package com.example.demo;\n@SpringBootTest class ContextTest {}\n",
    )
    .unwrap();
    let fake_dir = temp_dir("mock-test-isolation-bin");
    let log = fake_dir.join("log.txt");
    write_fake_maven(&fake_dir, &["mvn"], &log);

    let automatic = jails_cmd(&root, Some(&fake_dir))
        .args([
            "test",
            "ContextTest",
            "--engine",
            "auto",
            "--compile",
            "none",
        ])
        .output()
        .unwrap();
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&automatic.stdout),
        String::from_utf8_lossy(&automatic.stderr)
    );
    assert!(!automatic.status.success(), "{report}");
    assert!(
        report.contains("automatic warm execution is ineligible")
            && report.contains("compile explicitly"),
        "{report}"
    );
    assert!(
        read_log(&log).is_empty(),
        "--compile none must not invoke Maven"
    );

    let strict = jails_cmd(&root, Some(&fake_dir))
        .args([
            "test",
            "ContextTest",
            "--engine",
            "warm",
            "--compile",
            "none",
        ])
        .output()
        .unwrap();
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&strict.stdout),
        String::from_utf8_lossy(&strict.stderr)
    );
    assert!(!strict.status.success());
    assert!(
        report.contains("strict warm execution is ineligible")
            && report.contains("Spring application context")
            && report.contains("fix:"),
        "{report}"
    );
}

#[test]
fn project_maven_wrapper_wins_over_path_maven() {
    let root = temp_dir("mock-wrapper-first");
    write_project_skeleton(&root);
    let fake_dir = temp_dir("mock-wrapper-first-bin");
    let path_log = fake_dir.join("path-log.txt");
    let wrapper_log = root.join("wrapper-log.txt");
    write_fake_maven(&fake_dir, &["mvn", "mvnd"], &path_log);
    fs::write(
        root.join("mvnw"),
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$0 $*\" >> '{}'\n",
            wrapper_log.display()
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(root.join("mvnw")).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(root.join("mvnw"), permissions).unwrap();
    }

    assert!(
        jails_cmd(&root, Some(&fake_dir))
            .arg("check")
            .status()
            .unwrap()
            .success()
    );
    assert!(read_log(&wrapper_log).contains("clean verify"));
    assert!(!path_log.exists() || read_log(&path_log).is_empty());
}

#[test]
fn build_command_invokes_mvn_package() {
    let root = temp_dir("mock-build");
    write_project_skeleton(&root);
    let fake_dir = temp_dir("mock-build-bin");
    let log = fake_dir.join("log.txt");
    write_fake_maven(&fake_dir, &["mvn"], &log);

    let status = jails_cmd(&root, Some(&fake_dir))
        .arg("build")
        .status()
        .unwrap();
    assert!(status.success());
    assert!(read_log(&log).contains("package"));
}

#[test]
fn clean_command_invokes_mvn_clean() {
    let root = temp_dir("mock-wipe-target");
    write_project_skeleton(&root);
    let fake_dir = temp_dir("mock-wipe-target-bin");
    let log = fake_dir.join("log.txt");
    write_fake_maven(&fake_dir, &["mvn"], &log);

    let status = jails_cmd(&root, Some(&fake_dir))
        .arg("clean")
        .status()
        .unwrap();
    assert!(status.success());
    assert!(read_log(&log).contains("clean"));
    assert!(
        !read_log(&log).contains("verify"),
        "clean is only clean, not clean verify: {}",
        read_log(&log)
    );
}

#[test]
fn check_command_invokes_mvn_clean_verify() {
    let root = temp_dir("mock-check");
    write_project_skeleton(&root);
    let fake_dir = temp_dir("mock-check-bin");
    let log = fake_dir.join("log.txt");
    write_fake_maven(&fake_dir, &["mvn"], &log);

    let status = jails_cmd(&root, Some(&fake_dir))
        .arg("check")
        .status()
        .unwrap();
    assert!(status.success());
    assert!(
        read_log(&log).contains("clean verify"),
        "check must wipe target so deleted tests cannot linger: {}",
        read_log(&log)
    );
}

#[test]
fn build_tool_launcher_uses_spring_boot_run_for_spring_projects() {
    let root = temp_dir("mock-run-spring");
    let pkg_dir = common::generated(&root, "src/main/java/com/example/demo");
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::write(
        pkg_dir.join("App.java"),
        "package com.example.demo;\npublic class App { public static void main(String[] args) {} }\n",
    )
    .unwrap();
    fs::write(
        root.join("pom.xml"),
        "<project>org.springframework.boot</project>",
    )
    .unwrap();
    let fake_dir = temp_dir("mock-run-spring-bin");
    let log = fake_dir.join("log.txt");
    write_fake_maven(&fake_dir, &["mvn"], &log);

    let status = jails_cmd(&root, Some(&fake_dir))
        .args([
            "run",
            "--launcher",
            "build-tool",
            "--profile",
            "dev,test",
            "--",
            "two words",
            "quote's",
            r"back\slash",
        ])
        .status()
        .unwrap();
    assert!(status.success());
    let invocation = read_log(&log);
    assert!(invocation.contains("spring-boot:run"), "{invocation}");
    assert!(
        invocation.contains(
            r#"-Dspring-boot.run.arguments='--spring.profiles.active=dev,test' 'two words' 'quote'"'"'s' 'back\slash'"#
        ),
        "build-tool launch changed the tokenized application argv: {invocation}"
    );
}

#[test]
fn gradle_build_tool_launcher_preserves_the_same_application_vector() {
    let root = temp_dir("mock-run-gradle-spring");
    fs::write(root.join("settings.gradle"), "rootProject.name = 'demo'\n").unwrap();
    fs::write(root.join("build.gradle"), "plugins { id 'java' }\n").unwrap();
    let pkg_dir = common::generated(&root, "src/main/java/com/example/demo");
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::write(
        pkg_dir.join("App.java"),
        "package com.example.demo;\npublic class App { public static void main(String[] args) {} }\n",
    )
    .unwrap();
    let fake_dir = temp_dir("mock-run-gradle-spring-bin");
    let log = fake_dir.join("log.txt");
    write_fake_maven(&fake_dir, &["gradle"], &log);

    let status = jails_cmd(&root, Some(&fake_dir))
        .args([
            "run",
            "--launcher",
            "build-tool",
            "--profile",
            "dev,test",
            "--",
            "two words",
            "quote's",
            r"back\slash",
        ])
        .status()
        .unwrap();
    assert!(status.success());
    let invocation = read_log(&log);
    assert!(invocation.contains("bootRun"), "{invocation}");
    assert!(
        invocation.contains(
            r#"--args='--spring.profiles.active=dev,test' 'two words' 'quote'"'"'s' 'back\slash'"#
        ),
        "Gradle launch changed the tokenized application argv: {invocation}"
    );
}

#[test]
fn run_starts_compose_services_only_when_explicitly_requested() {
    let root = temp_dir("mock-run-compose");
    let pkg_dir = common::generated(&root, "src/main/java/com/example/demo");
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::write(
        pkg_dir.join("App.java"),
        "package com.example.demo;\npublic class App { public static void main(String[] args) {} }\n",
    )
    .unwrap();
    fs::write(
        root.join("pom.xml"),
        "<project>org.springframework.boot</project>",
    )
    .unwrap();
    // A real port with a real listener on it, declared in the compose file
    // jails reads. `run --services start` will not launch Spring until the
    // declared PostgreSQL accepts TCP connections, and a fake `docker` that
    // exits 0 starts no container -- so without this the command spends its
    // whole thirty-second readiness budget failing to reach a server that was
    // never going to exist, for a test whose question is only whether compose
    // went up before Spring. See `common::listening_loopback_port`.
    let (_postgres, postgres_port) = listening_loopback_port();
    fs::write(
        root.join("compose.yaml"),
        // The block-sequence spelling `add db` itself writes: the host port
        // is read off a `- "host:container"` item, and an inline flow
        // sequence is not that shape.
        format!(
            "services:\n  postgres:\n    image: postgres:17-alpine\n    ports:\n      - \"{postgres_port}:5432\"\n"
        ),
    )
    .unwrap();
    let fake_dir = temp_dir("mock-run-compose-bin");
    let log = fake_dir.join("log.txt");
    write_fake_maven(&fake_dir, &["docker", "mvn"], &log);

    let status = jails_cmd(&root, Some(&fake_dir))
        .args(["run", "--launcher", "build-tool", "--services", "start"])
        .status()
        .unwrap();
    assert!(status.success());
    let invocation = read_log(&log);
    assert!(
        invocation.contains("compose up -d --wait --wait-timeout 120"),
        "expected docker compose up before spring-boot:run: {invocation}"
    );
    assert!(invocation.contains("spring-boot:run"));
}

#[test]
fn migrate_check_does_not_restart_a_database_that_already_answers() {
    let root = temp_dir("migrate-ready-postgres");
    write_project_skeleton(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();
    fs::write(
        root.join("src/main/resources/db/migration/V001__ready.sql"),
        "select 1;\n",
    )
    .unwrap();
    fs::write(
        root.join("compose.yaml"),
        "services:\n  postgres:\n    image: postgres:17-alpine\n    ports: [\"5432:5432\"]\n    environment:\n      POSTGRES_DB: app\n      POSTGRES_USER: app\n      POSTGRES_PASSWORD: app\n",
    )
    .unwrap();
    let fake = temp_dir("migrate-ready-postgres-bin");
    let log = fake.join("log.txt");
    write_fake_maven(&fake, &["docker", "psql"], &log);
    // `migrate --check` sends SQL on stdin. A fake that exits without
    // consuming it races the parent writer and can produce EPIPE under the
    // full parallel suite, even though the command contract is otherwise
    // satisfied. Model psql's stdin behavior so this remains deterministic.
    fs::write(
        fake.join("psql"),
        format!(
            "#!/bin/sh\necho \"$0 $*\" >> \"{}\"\nwhile IFS= read -r _; do :; done\nexit 0\n",
            log.display()
        ),
    )
    .unwrap();
    set_executable(&fake.join("psql"));

    let output = jails_cmd(&root, Some(&fake))
        .args(["migrate", "--check"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let invocation = read_log(&log);
    assert!(!invocation.contains("compose up"), "{invocation}");
    assert!(invocation.contains("psql"), "{invocation}");
}

#[test]
fn start_and_stop_drive_docker_compose() {
    let root = temp_dir("mock-start-stop");
    write_project_skeleton(&root);
    fs::write(
        root.join("compose.yaml"),
        "services:\n  postgres:\n    image: postgres:17-alpine\n  kafka:\n    image: apache/kafka:4.1.0\n",
    )
    .unwrap();
    let fake_dir = temp_dir("mock-start-stop-bin");
    let log = fake_dir.join("log.txt");
    write_fake_maven(&fake_dir, &["docker"], &log);

    assert!(
        jails_cmd(&root, Some(&fake_dir))
            .args(["start", "db"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        jails_cmd(&root, Some(&fake_dir))
            .args(["stop", "kafka"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        jails_cmd(&root, Some(&fake_dir))
            .arg("start")
            .status()
            .unwrap()
            .success()
    );
    assert!(
        jails_cmd(&root, Some(&fake_dir))
            .arg("stop")
            .status()
            .unwrap()
            .success()
    );

    let invocation = read_log(&log);
    let lines: Vec<&str> = invocation.lines().collect();
    assert!(
        lines
            .iter()
            .any(|l| l.ends_with("compose up -d --wait --wait-timeout 120 postgres")),
        "start db: {invocation}"
    );
    assert!(
        lines.iter().any(|l| l.ends_with("compose stop kafka")),
        "stop kafka: {invocation}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.ends_with("compose up -d --wait --wait-timeout 120")),
        "bare start should be `up -d --wait` with no service filter: {invocation}"
    );
    assert!(
        lines.iter().any(|l| l.ends_with("compose stop")),
        "bare stop should be `stop` with no service filter: {invocation}"
    );
}

#[test]
fn start_errors_when_there_is_no_compose_file() {
    let root = temp_dir("start-no-compose");
    write_project_skeleton(&root);
    let fake_dir = temp_dir("start-no-compose-bin");
    let log = fake_dir.join("log.txt");
    write_fake_maven(&fake_dir, &["docker"], &log);

    let output = jails_cmd(&root, Some(&fake_dir))
        .arg("start")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("compose.yaml"), "{stderr}");
    assert!(read_log(&log).is_empty(), "docker must not be invoked");
}

#[test]
fn db_opens_psql_against_compose_postgres() {
    let root = temp_dir("db-psql");
    write_plain_fixture(&root);
    let fake = temp_dir("db-psql-bin");
    let log = fake.join("log.txt");
    write_fake_maven(&fake, &["docker", "psql"], &log);
    fs::write(
        fake.join("psql"),
        format!(
            "#!/bin/sh\necho \"PGPASSWORD=$PGPASSWORD $*\" >> \"{}\"\nexit 0\n",
            log.display()
        ),
    )
    .unwrap();

    assert!(
        jails_cmd(&root, Some(&fake))
            .args(["add", "db", "--no-start"])
            .status()
            .unwrap()
            .success()
    );
    fs::write(&log, "").unwrap();

    assert!(
        jails_cmd(&root, Some(&fake))
            .arg("db")
            .status()
            .unwrap()
            .success()
    );
    let invocation = read_log(&log);
    assert!(
        invocation.contains("compose up -d --wait --wait-timeout 120 postgres"),
        "db should start postgres first: {invocation}"
    );
    assert!(
        invocation.contains("PGPASSWORD=app -h localhost -p 5432 -U app -d app"),
        "{invocation}"
    );

    fs::write(&log, "").unwrap();
    assert!(
        jails_cmd(&root, Some(&fake))
            .args(["db", "--no-start"])
            .status()
            .unwrap()
            .success()
    );
    let invocation = read_log(&log);
    assert!(
        !invocation.contains("compose up"),
        "--no-start must not bring compose up: {invocation}"
    );
    assert!(
        invocation.contains("-h localhost -p 5432 -U app -d app"),
        "{invocation}"
    );
}

#[test]
fn db_without_postgres_explains_add_db() {
    let root = temp_dir("db-missing");
    write_plain_fixture(&root);
    let fake = temp_dir("db-missing-bin");
    let log = fake.join("log.txt");
    write_fake_maven(&fake, &["psql"], &log);
    let output = jails_cmd(&root, Some(&fake)).arg("db").output().unwrap();
    assert!(!output.status.success());
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(err.contains("add db"), "{err}");
}

#[test]
fn db_with_a_file_uses_sqlite3() {
    let root = temp_dir("db-sqlite");
    write_plain_fixture(&root);
    fs::write(root.join("app.db"), "").unwrap();
    let fake = temp_dir("db-sqlite-bin");
    let log = fake.join("log.txt");
    write_fake_maven(&fake, &["sqlite3"], &log);
    assert!(
        jails_cmd(&root, Some(&fake))
            .args(["db", "app.db"])
            .status()
            .unwrap()
            .success()
    );
    let invocation = read_log(&log);
    assert!(
        invocation.contains("sqlite3") && invocation.contains("app.db"),
        "{invocation}"
    );
}

#[test]
fn console_launches_jshell_with_the_project_classpath() {
    let root = temp_dir("console-jshell");
    write_plain_fixture(&root);
    fs::create_dir_all(root.join("target/classes/com/example/demo")).unwrap();
    fs::write(
        root.join("target/classes/com/example/demo/DemoApplication.class"),
        "compiled",
    )
    .unwrap();
    let fake = temp_dir("console-jshell-bin");
    let log = fake.join("log.txt");
    write_fake_maven(&fake, &["mvn", "java", "jshell"], &log);
    // **The fake `mvn` writes the file it is told to, because the real one
    // does.** This used to seed `target/jails-runtime-classpath` by hand so
    // the read after the resolve had something to find, and that seed is now
    // indistinguishable from an already-resolved classpath: `jails console`
    // reuses one that is newer than the pom, so the resolve this test exists
    // to observe was correctly skipped and the test failed on its absence. A
    // stand-in that answers the question differently from the tool it stands
    // in for will eventually be believed over the tool.
    fs::write(
        fake.join("mvn"),
        format!(
            "#!/bin/sh\necho \"$0 $*\" >> \"{}\"\nfor arg in \"$@\"; do\n  case \"$arg\" in\n    -Dmdep.outputFile=*) : > \"${{arg#-Dmdep.outputFile=}}\" ;;\n  esac\ndone\nexit 0\n",
            log.display()
        ),
    )
    .unwrap();
    set_executable(&fake.join("mvn"));
    fs::write(
        fake.join("java"),
        format!(
            "#!/bin/sh\necho 'openjdk version \"26\"' >&2\necho \"$0 $*\" >> \"{}\"\nexit 0\n",
            log.display()
        ),
    )
    .unwrap();
    assert!(
        jails_cmd(&root, Some(&fake))
            .env_remove("JAVA_HOME")
            .args(["console", "--no-build", "--main", "com.example.demo.App",])
            .status()
            .unwrap()
            .success()
    );
    let invocation = read_log(&log);
    assert!(
        invocation.contains("dependency:build-classpath"),
        "{invocation}"
    );
    assert!(
        invocation.contains("jshell")
            && invocation.contains("--execution local")
            && invocation.contains("--class-path")
            && invocation.contains("--startup"),
        "{invocation}"
    );
    assert!(
        !invocation.contains(" compile") && !invocation.contains("/mvn compile"),
        "--no-build should skip compile: {invocation}"
    );
}

#[test]
fn gradle_console_uses_the_shared_existing_runtime_classpath() {
    let root = temp_dir("console-gradle-jshell");
    fs::write(root.join("settings.gradle"), "rootProject.name = 'demo'\n").unwrap();
    fs::write(
        root.join("build.gradle"),
        "plugins { id 'java' }\nsourceCompatibility = 26\n",
    )
    .unwrap();
    let source = common::generated(&root, "src/main/java/com/example/demo/DemoApplication.java");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(
        &source,
        "package com.example.demo;\npublic class DemoApplication {}\n",
    )
    .unwrap();
    let classes = root.join("build/classes/java/main/com/example/demo");
    fs::create_dir_all(&classes).unwrap();
    fs::write(classes.join("DemoApplication.class"), "compiled").unwrap();
    fs::create_dir_all(root.join("build/resources/main")).unwrap();

    let fake = temp_dir("console-gradle-jshell-bin");
    let log = fake.join("log.txt");
    write_fake_maven(&fake, &["gradle", "java", "jshell"], &log);
    fs::write(
        fake.join("java"),
        format!(
            "#!/bin/sh\necho 'openjdk version \"26\"' >&2\necho \"$0 $*\" >> \"{}\"\nexit 0\n",
            log.display()
        ),
    )
    .unwrap();
    fs::write(
        fake.join("gradle"),
        format!(
            "#!/bin/sh\necho \"$0 $*\" >> \"{}\"\necho JAILS_RUNTIME_CLASSPATH=\nexit 0\n",
            log.display()
        ),
    )
    .unwrap();

    let output = jails_cmd(&root, Some(&fake))
        .env_remove("JAVA_HOME")
        .args(["console", "--main", "com.example.demo.DemoApplication"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let invocation = read_log(&log);
    assert!(
        invocation.contains("gradle -q jailsRuntimeClasspath"),
        "{invocation}"
    );
    assert!(
        invocation.contains("jshell --execution local"),
        "{invocation}"
    );
    assert!(invocation.contains("--class-path"), "{invocation}");
    assert!(
        !invocation.contains(" classes"),
        "existing-output console must not compile: {invocation}"
    );
}

#[test]
fn run_command_compiles_before_attempting_a_plain_main_class() {
    let root = temp_dir("mock-run-plain");
    let pkg_dir = common::generated(&root, "src/main/java/com/example/demo");
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::write(root.join("pom.xml"), "<project></project>").unwrap();
    fs::write(
        pkg_dir.join("App.java"),
        "package com.example.demo;\n\npublic class App {\n    public static void main(String[] args) {}\n}\n",
    )
    .unwrap();
    let fake_dir = temp_dir("mock-run-plain-bin");
    let log = fake_dir.join("log.txt");
    write_fake_maven(&fake_dir, &["mvn"], &log);

    // The fake `mvn compile` is a no-op, so target/classes stays empty and
    // the subsequent `java -cp target/classes ...` step genuinely fails --
    // this only asserts jails attempted the compile step first, not that
    // the whole pipeline succeeds (that needs the real toolchain; see the
    // `#[ignore]`d tests below).
    jails_cmd(&root, Some(&fake_dir))
        .arg("run")
        .status()
        .unwrap();
    assert!(read_log(&log).contains("compile"));
}

#[test]
fn run_no_build_refuses_an_unproven_jar_instead_of_running_whatever_exists() {
    let root = temp_dir("no-build-spring");
    fs::create_dir_all(root.join("target")).unwrap();
    fs::write(
        root.join("pom.xml"),
        "<project>org.springframework.boot</project>",
    )
    .unwrap();
    fs::write(
        root.join("target/demo.jar"),
        "not a real jar, just needs to exist",
    )
    .unwrap();

    let fake_dir = temp_dir("no-build-spring-bin");
    let log = fake_dir.join("log.txt");
    write_fake_maven(&fake_dir, &["mvn", "java"], &log);

    let status = jails_cmd(&root, Some(&fake_dir))
        .args(["run", "--no-build"])
        .status()
        .unwrap();
    assert!(!status.success());

    let invocation = read_log(&log);
    assert!(
        !invocation.contains("/java "),
        "an arbitrary jar must not run: {invocation}"
    );
    assert!(
        !invocation.contains("/mvn "),
        "mvn should never run with --no-build: {invocation}"
    );
}

#[test]
fn run_no_build_errors_clearly_when_target_is_missing() {
    let root = temp_dir("no-build-missing-plain");
    let pkg_dir = common::generated(&root, "src/main/java/com/example/demo");
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::write(root.join("pom.xml"), "<project></project>").unwrap();
    fs::write(
        pkg_dir.join("App.java"),
        "package com.example.demo;\n\npublic class App {\n    public static void main(String[] args) {}\n}\n",
    )
    .unwrap();

    let output = jails_cmd(&root, None)
        .args(["run", "--no-build"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("jails build"),
        "expected a hint to build first: {stderr}"
    );
}

#[test]
fn run_no_build_runs_already_compiled_plain_classes_without_mvn() {
    if !real_java_available() {
        skip("java/javac not found on PATH");
        return;
    }
    let root = temp_dir("no-build-plain-real");
    let pkg_dir = common::generated(&root, "src/main/java/com/example/demo");
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::write(root.join("pom.xml"), "<project></project>").unwrap();
    let source = pkg_dir.join("App.java");
    fs::write(
        &source,
        "package com.example.demo;\n\npublic class App {\n    public static void main(String[] args) {\n        System.out.println(\"no-build-ran:\" + String.join(\"|\", args));\n    }\n}\n",
    )
    .unwrap();

    let classes = root.join("target/classes");
    fs::create_dir_all(&classes).unwrap();
    let status = std::process::Command::new("javac")
        .arg("-d")
        .arg(&classes)
        .arg(&source)
        .status()
        .unwrap();
    assert!(status.success());

    // A fake mvn that always fails -- proves --no-build never shells out to
    // it; if it did, this test would fail instead of just being redundant.
    let fake_dir = temp_dir("no-build-plain-real-bin");
    fs::create_dir_all(&fake_dir).unwrap();
    fs::write(fake_dir.join("mvn"), "#!/bin/sh\nexit 1\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(fake_dir.join("mvn")).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(fake_dir.join("mvn"), perms).unwrap();
    }

    let real_java_dir = real_path_without_mvnd();
    let path = format!("{}:{real_java_dir}", fake_dir.display());
    let output = jails_cmd_with_path(&root, &path)
        .args([
            "run",
            "--no-build",
            "--profile",
            "dev",
            "--",
            "hello world",
            "--flag",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("no-build-ran:--spring.profiles.active=dev|hello world|--flag"),
        "direct launch must preserve every argv boundary: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        root.join(".jails/run/runtime-classpath-v2").is_file(),
        "the direct launcher must persist its content-addressed classpath"
    );

    fs::write(
        &source,
        "package com.example.demo;\nclass App { public static void main(String[] args) {} }\n",
    )
    .unwrap();
    let stale = jails_cmd_with_path(&root, &path)
        .args(["run", "--compile", "none", "--launcher", "classpath"])
        .output()
        .unwrap();
    assert!(!stale.status.success());
    assert!(
        String::from_utf8_lossy(&stale.stderr).contains("classes are stale"),
        "{}",
        String::from_utf8_lossy(&stale.stderr)
    );
}

#[test]
fn direct_launch_refuses_a_selected_jdk_older_than_the_project_release() {
    let root = temp_dir("run-old-jdk");
    let source = common::generated(&root, "src/main/java/com/example/demo/App.java");
    let class = root.join("target/classes/com/example/demo/App.class");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::create_dir_all(class.parent().unwrap()).unwrap();
    fs::write(
        root.join("pom.xml"),
        "<project><properties><maven.compiler.release>26</maven.compiler.release></properties></project>\n",
    )
    .unwrap();
    fs::write(
        &source,
        "package com.example.demo;\npublic class App { public static void main(String[] args) {} }\n",
    )
    .unwrap();
    fs::write(&class, "compiled").unwrap();
    let tools = temp_dir("run-old-jdk-tools");
    let log = tools.join("java.log");
    write_fake_maven(&tools, &["java"], &log);
    fs::write(
        tools.join("java"),
        format!(
            "#!/bin/sh\necho \"$*\" >> \"{}\"\nif [ \"$1\" = \"-version\" ]; then echo 'openjdk version \"21.0.8\"' >&2; exit 0; fi\nexit 99\n",
            log.display()
        ),
    )
    .unwrap();

    let output = jails_cmd(&root, Some(&tools))
        .env_remove("JAVA_HOME")
        .args(["run", "--launcher", "classpath", "--compile", "none"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(
        stderr.contains("Java 21 cannot run project release 26"),
        "{stderr}"
    );
    let log = read_log(&log);
    assert!(log.contains("-version"), "{log}");
    assert!(
        !log.contains("-cp"),
        "an incompatible JDK launched the app: {log}"
    );
}

#[test]
fn jar_launch_reuses_only_a_byte_current_proved_artifact() {
    let root = temp_dir("run-proved-jar");
    let source = common::generated(&root, "src/main/java/com/example/demo/App.java");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(root.join("pom.xml"), "<project/>\n").unwrap();
    fs::write(&source, "package com.example.demo;\nclass App {}\n").unwrap();
    let tools = temp_dir("run-proved-jar-tools");
    let log = tools.join("tools.log");
    write_fake_maven(&tools, &["mvn", "java"], &log);
    fs::write(
        tools.join("mvn"),
        format!(
            "#!/bin/sh\necho \"mvn $*\" >> \"{}\"\n/bin/mkdir -p target\necho packaged > target/app.jar\n",
            log.display()
        ),
    )
    .unwrap();
    fs::write(
        tools.join("java"),
        format!(
            "#!/bin/sh\necho \"java $*\" >> \"{}\"\nif [ \"$1\" = \"-version\" ]; then echo 'openjdk version \"26\"' >&2; fi\nexit 0\n",
            log.display()
        ),
    )
    .unwrap();

    let built = jails_cmd(&root, Some(&tools))
        .env_remove("JAVA_HOME")
        .args([
            "run",
            "--launcher",
            "jar",
            "--compile",
            "build",
            "--services",
            "none",
            "--",
            "hello world",
        ])
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}{}",
        String::from_utf8_lossy(&built.stdout),
        String::from_utf8_lossy(&built.stderr)
    );
    assert!(root.join(".jails/run/packaged-artifact-v1").is_file());

    let reused = jails_cmd(&root, Some(&tools))
        .env_remove("JAVA_HOME")
        .args([
            "run",
            "--launcher",
            "jar",
            "--compile",
            "none",
            "--services",
            "none",
        ])
        .output()
        .unwrap();
    assert!(reused.status.success());
    let before_stale = read_log(&log);
    let package_runs = before_stale.matches("mvn package").count();
    assert_eq!(
        package_runs, 1,
        "compile none rebuilt the jar: {before_stale}"
    );

    fs::write(
        &source,
        "package com.example.demo;\nclass App { int changed; }\n",
    )
    .unwrap();
    let stale = jails_cmd(&root, Some(&tools))
        .env_remove("JAVA_HOME")
        .args([
            "run",
            "--launcher",
            "jar",
            "--compile",
            "none",
            "--services",
            "none",
        ])
        .output()
        .unwrap();
    assert!(!stale.status.success());
    assert!(
        String::from_utf8_lossy(&stale.stderr).contains("packaged artifact is stale"),
        "{}",
        String::from_utf8_lossy(&stale.stderr)
    );
    assert_eq!(
        read_log(&log).matches("java -jar").count(),
        2,
        "the stale jar was launched"
    );
}

/// The end-to-end path the tool exists for: generate a command, and have it
/// reachable by name with its arguments. Covers three things no unit test can
/// -- that `generate command` really registered itself in the dispatcher, that
/// the project compiles, and that `run --` forwards argv to the program.
#[test]
fn a_generated_command_is_reachable_by_name_through_jails_run() {
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

    let output = jails_cmd_with_path(root, &path)
        .args(["run", "--no-build", "--", "greet", "world"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("world"),
        "the command never saw its argument: {stdout}"
    );

    // And with no arguments at all, the dispatcher lists what it knows rather
    // than failing -- `new-cli`'s App.java is a dispatcher, not a stub.
    let output = jails_cmd_with_path(root, &path)
        .args(["run", "--no-build"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("greet"),
        "help should list the registered command: {stdout}"
    );
}

/// `plan.md` §12. Before this, **zero** of jails' ~30 commands worked on a
/// project it did not create -- and the whole gate was eleven lines looking for
/// `pom.xml`, not anything the commands actually do.
#[test]
fn a_gradle_project_gets_the_commands_that_do_not_need_maven() {
    let root = temp_dir("foreign-build");
    let main = common::generated(&root, "src/main/java/com/acme/shop");
    fs::create_dir_all(&main).unwrap();
    // A multi-module Gradle build: only `settings.gradle` at the top.
    fs::write(root.join("settings.gradle"), "rootProject.name = 'shop'\n").unwrap();

    fs::write(
        main.join("ShopApplication.java"),
        "package com.acme.shop;\n\npublic class ShopApplication {}\n",
    )
    .unwrap();

    // Reading commands work.
    let stats = jails_cmd(&root, None).arg("stats").output().unwrap();
    assert!(
        stats.status.success(),
        "{}{}",
        String::from_utf8_lossy(&stats.stdout),
        String::from_utf8_lossy(&stats.stderr)
    );

    // Generating refuses, and names the one thing this build never said:
    // which Java release the code jails writes has to compile against. A
    // multi-module root declares it per module, so there is no answer here
    // and no defensible default -- jails' own target on a project whose
    // modules build with 17 is code that does not compile.
    let generated = jails_cmd(&root, None)
        .args(["generate", "record", "Order", "id:uuid@pk", "total:long"])
        .output()
        .unwrap();
    assert!(!generated.status.success());
    let refusal = String::from_utf8_lossy(&generated.stderr);
    assert!(refusal.contains("Java release"), "{refusal}");
    assert!(refusal.contains("Gradle toolchain"), "{refusal}");
    assert!(
        !common::generated(&root, "src/main/java/com/acme/shop/domain/Order.java").is_file(),
        "the refusal still wrote the record"
    );

    // The Maven-inherent ones refuse, and the refusal names a way forward.
    for command in ["test", "build", "check", "clean"] {
        let refused = jails_cmd(&root, None).arg(command).output().unwrap();
        let stderr = String::from_utf8_lossy(&refused.stderr);
        assert!(!refused.status.success(), "`{command}` should refuse");
        assert!(stderr.contains("built by Gradle"), "{command}: {stderr}");
        assert!(stderr.contains("routes"), "{command}: {stderr}");
    }
    let refused = jails_cmd(&root, None)
        .args(["add", "db", "--no-start"])
        .output()
        .unwrap();
    assert!(!refused.status.success(), "add must not half-install");

    // doctor says so first, and does not report on a pom that is not there.
    let doctor = jails_cmd(&root, None).arg("doctor").output().unwrap();
    let report = String::from_utf8_lossy(&doctor.stdout);
    assert!(report.contains("built by Gradle"), "{report}");
    assert!(
        !report.contains("pom.xml is missing"),
        "a confident wrong report is worse than a refusal: {report}"
    );
}

#[test]
fn gradle_json_is_one_report_and_timeout_is_bounded_without_tool_noise() {
    let root = temp_dir("gradle-json-test");
    fs::write(root.join("settings.gradle"), "rootProject.name = 'demo'\n").unwrap();
    fs::write(root.join("build.gradle"), "plugins { id 'java' }\n").unwrap();
    let tools = temp_dir("gradle-json-tools");
    let log = tools.join("log.txt");
    write_fake_maven(&tools, &["gradle"], &log);
    fs::write(
        tools.join("gradle"),
        format!(
            "#!/bin/sh\necho GRADLE-TOOL-NOISE\necho \"$0 $*\" >> \"{}\"\nexit 0\n",
            log.display()
        ),
    )
    .unwrap();

    let output = jails_cmd(&root, Some(&tools))
        .args(["test", "--engine", "build", "--output", "json"])
        .output()
        .unwrap();
    let json = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{json}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        json.lines().count(),
        1,
        "JSON stdout must be one report: {json}"
    );
    assert!(
        !json.contains("GRADLE-TOOL-NOISE"),
        "Gradle leaked into JSON: {json}"
    );

    let child_pid_file = tools.join("child.pid");
    fs::write(
        tools.join("gradle"),
        format!(
            "#!/bin/sh\n/bin/sleep 30 &\necho \"$!\" > \"{}\"\nwait\n",
            child_pid_file.display()
        ),
    )
    .unwrap();
    let started = std::time::Instant::now();
    let timed = jails_cmd(&root, Some(&tools))
        .args([
            "test",
            "--engine",
            "build",
            "--timeout",
            "1s",
            "--output",
            "json",
        ])
        .output()
        .unwrap();
    let elapsed = started.elapsed();
    let json = String::from_utf8_lossy(&timed.stdout);
    assert!(
        !timed.status.success(),
        "the timed build unexpectedly passed: {json}"
    );
    assert_eq!(
        json.lines().count(),
        1,
        "timed JSON must be one report: {json}"
    );
    assert!(
        json.contains("\"passed\":false"),
        "timeout verdict was lost: {json}"
    );
    assert!(
        elapsed < std::time::Duration::from_secs(10),
        "the 1s Gradle timeout took {elapsed:?}"
    );
    let child_pid = fs::read_to_string(&child_pid_file)
        .unwrap()
        .trim()
        .parse::<u32>()
        .unwrap();
    let child_proc = PathBuf::from(format!("/proc/{child_pid}"));
    let reaped_by = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while child_proc.exists() && std::time::Instant::now() < reaped_by {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(
        !child_proc.exists(),
        "the timed-out Gradle process left child pid {child_pid} alive"
    );
}

#[test]
fn maven_json_is_one_report_and_timeout_is_bounded_without_tool_noise() {
    let root = temp_dir("maven-json-test");
    write_plain_fixture(&root);
    let tools = temp_dir("maven-json-tools");
    let log = tools.join("log.txt");
    write_fake_maven(&tools, &["mvn"], &log);
    fs::write(
        tools.join("mvn"),
        "#!/bin/sh\necho MAVEN-TOOL-NOISE\nexit 0\n",
    )
    .unwrap();

    let output = jails_cmd(&root, Some(&tools))
        .args(["test", "--engine", "build", "--output", "json"])
        .output()
        .unwrap();
    let json = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{json}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        json.lines().count(),
        1,
        "JSON stdout must be one report: {json}"
    );
    assert!(
        !json.contains("MAVEN-TOOL-NOISE"),
        "Maven leaked into JSON: {json}"
    );

    fs::write(tools.join("mvn"), "#!/bin/sh\n/bin/sleep 30\n").unwrap();
    let started = std::time::Instant::now();
    let timed = jails_cmd(&root, Some(&tools))
        .args([
            "test",
            "--engine",
            "build",
            "--timeout",
            "1s",
            "--output",
            "json",
        ])
        .output()
        .unwrap();
    let elapsed = started.elapsed();
    let json = String::from_utf8_lossy(&timed.stdout);
    assert!(
        !timed.status.success(),
        "the timed build unexpectedly passed: {json}"
    );
    assert_eq!(
        json.lines().count(),
        1,
        "timed JSON must be one report: {json}"
    );
    assert!(
        json.contains("\"passed\":false"),
        "timeout verdict was lost: {json}"
    );
    assert!(
        elapsed < std::time::Duration::from_secs(10),
        "the 1s Maven timeout took {elapsed:?}"
    );
}

/// `jails modernize` against the exact file it was discovered on.
///
/// The fixture is `minicom-15-01-2026/spring`'s `build.gradle`, unedited:
/// Spring Boot 2.7.18 through the legacy `buildscript` spelling, a project-level
/// `sourceCompatibility`, no test block, and a Gradle 8.5 wrapper. Every
/// assertion below is a `./gradlew build` on JDK 26 that failed without it,
/// discovered in this order -- unknown property `sourceCompatibility`, then
/// "did not discover any tests", then `Unknown data type: "DATETIME"`.
///
/// It is one commit, not five, because the edits are interdependent: a wrapper
/// bumped without the toolchain block fails evaluation, and a toolchain bumped
/// without the wrapper fails on an unsupported class file version. A run that
/// stopped halfway would leave a build broken in a way neither half explains.
#[test]
fn modernize_takes_a_boot_2_gradle_project_to_the_versions_jails_generates_against() {
    let root = temp_dir("modernize");
    write_project_skeleton(&root);
    fs::remove_file(root.join("pom.xml")).unwrap();
    fs::write(
        root.join("build.gradle"),
        concat!(
            "buildscript {\n    repositories {\n        mavenCentral()\n    }\n",
            "    dependencies {\n        classpath(\"org.springframework.boot:",
            "spring-boot-gradle-plugin:2.7.18\")\n    }\n}\n\n",
            "apply plugin: 'java'\napply plugin: 'org.springframework.boot'\n\n",
            "sourceCompatibility = 21\ntargetCompatibility = 21\n\n",
            "dependencies {\n    implementation 'org.springframework.boot:",
            "spring-boot-starter-data-jdbc'\n    runtimeOnly 'com.h2database:h2'\n}\n",
        ),
    )
    .unwrap();
    fs::create_dir_all(root.join("gradle/wrapper")).unwrap();
    fs::write(
        root.join("gradle/wrapper/gradle-wrapper.properties"),
        "distributionUrl=https\\://services.gradle.org/distributions/gradle-8.5-all.zip\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("src/main/resources")).unwrap();
    fs::write(
        root.join("src/main/resources/schema.sql"),
        "create table users (\n  id integer primary key,\n  created_at datetime\n);\n",
    )
    .unwrap();
    let base = common::generated(&root, "src/main/java/com/example/demo");
    fs::create_dir_all(&base).unwrap();
    fs::write(
        base.join("Reader.java"),
        "package com.example.demo;\n\nimport com.fasterxml.jackson.databind.ObjectMapper;\n\n\
         public class Reader {\n    ObjectMapper mapper = new ObjectMapper();\n}\n",
    )
    .unwrap();
    // A second file whose Jackson use is *not* a rename: `JsonProcessingException`
    // has zero occurrences under `tools/` in `deps/jackson-databind`, because it
    // became unchecked and moved -- so a `throws` naming it changes shape and
    // renaming the package alone would leave code that looks migrated and does
    // not compile.
    fs::write(
        base.join("Legacy.java"),
        "package com.example.demo;\n\nimport com.fasterxml.jackson.core.JsonProcessingException;\n\
         import com.fasterxml.jackson.databind.ObjectMapper;\n\n\
         public class Legacy {\n    String go(Object o) throws JsonProcessingException {\n        \
         return new ObjectMapper().writeValueAsString(o);\n    }\n}\n",
    )
    .unwrap();

    let preview = jails_cmd(&root, None)
        .args(["modernize", "--pretend"])
        .output()
        .unwrap();
    let preview = String::from_utf8_lossy(&preview.stdout).to_string();
    assert!(preview.contains("nothing was written"), "{preview}");
    assert!(
        fs::read_to_string(root.join("build.gradle"))
            .unwrap()
            .contains("2.7.18"),
        "--pretend wrote the file"
    );

    let output = jails_cmd(&root, None).arg("modernize").output().unwrap();
    let report = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(
        output.status.success(),
        "{report}{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let build = fs::read_to_string(root.join("build.gradle")).unwrap();
    assert!(build.contains("spring-boot-gradle-plugin:4.1.0"), "{build}");
    assert!(!build.contains("sourceCompatibility"), "{build}");
    assert!(
        build.contains("languageVersion = JavaLanguageVersion.of(26)"),
        "{build}"
    );
    assert!(build.contains("useJUnitPlatform()"), "{build}");
    // The rest of the file is untouched: this is a file the reader owns.
    assert!(build.contains("apply plugin: 'eclipse'") || build.contains("apply plugin: 'java'"));
    assert!(build.contains("runtimeOnly 'com.h2database:h2'"), "{build}");

    let wrapper =
        fs::read_to_string(root.join("gradle/wrapper/gradle-wrapper.properties")).unwrap();
    assert!(wrapper.contains("gradle-9.7.0-all.zip"), "{wrapper}");

    // Gated on H2 actually being this project's driver, and case-preserving.
    let schema = fs::read_to_string(root.join("src/main/resources/schema.sql")).unwrap();
    assert!(schema.contains("created_at timestamp"), "{schema}");

    // Rewritten when the rename *is* the whole migration: every type
    // `Reader.java` names exists in 3.x under the same name, `new
    // ObjectMapper()` included
    // (`deps/jackson-databind` `tools/jackson/databind/ObjectMapper.java:276`).
    // Refusing this file left a project jails had just moved to Boot 4 unable
    // to compile over one import.
    let reader = fs::read_to_string(base.join("Reader.java")).unwrap();
    assert!(
        reader.contains("tools.jackson.databind.ObjectMapper"),
        "{reader}"
    );
    assert!(!reader.contains("com.fasterxml"), "{reader}");
    assert!(
        report.contains("com.fasterxml.jackson -> tools.jackson"),
        "{report}"
    );

    // Reported and never rewritten when the API changed, which is the half the
    // blanket refusal was right about.
    assert!(report.contains("Jackson 2"), "{report}");
    assert!(report.contains("Legacy.java"), "{report}");
    assert!(
        !report.contains("tools.jackson in src/main/java/com/example/demo/Legacy.java"),
        "{report}"
    );
    let legacy = fs::read_to_string(base.join("Legacy.java")).unwrap();
    assert!(legacy.contains("com.fasterxml.jackson"), "{legacy}");

    // Idempotent, and it says what was already right rather than just "no".
    let again = jails_cmd(&root, None).arg("modernize").output().unwrap();
    let again = format!(
        "{}{}",
        String::from_utf8_lossy(&again.stdout),
        String::from_utf8_lossy(&again.stderr)
    );
    assert!(again.contains("nothing to modernize"), "{again}");
    assert!(again.contains("already 4.1.0"), "{again}");
}

/// `plan.md` §12: `jails adopt` writes a `[layout]` table, not new machinery.
/// The proof is that a *reporting* command's answer changes with no code path
/// of its own — `stats` counted a renamed web package as "Other".
#[test]
fn adopt_teaches_jails_where_an_existing_project_keeps_things() {
    let root = temp_dir("adopt");
    write_plain_fixture(&root);
    let base = common::generated(&root, "src/main/java/com/example/demo");
    for (dir, class) in [
        ("controllers", "OrderController"),
        ("persistence", "JdbcOrderRepository"),
        ("util", "Strings"),
    ] {
        fs::create_dir_all(base.join(dir)).unwrap();
        fs::write(
            base.join(dir).join(format!("{class}.java")),
            format!("package com.example.demo.{dir};\n\npublic class {class} {{}}\n"),
        )
        .unwrap();
    }

    let before = jails_cmd(&root, None).arg("stats").output().unwrap();
    let before = String::from_utf8_lossy(&before.stdout).to_string();
    assert!(before.contains("Other"), "{before}");

    let preview = jails_cmd(&root, None)
        .args(["adopt", "--pretend"])
        .output()
        .unwrap();
    let preview = String::from_utf8_lossy(&preview.stdout).to_string();
    assert!(preview.contains("nothing was written"), "{preview}");
    assert!(!root.join("jails.toml").exists());

    let output = jails_cmd(&root, None).arg("adopt").output().unwrap();
    let report = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(
        output.status.success(),
        "{report}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(report.contains("web"), "{report}");
    assert!(report.contains("controllers"), "{report}");
    // Reported, never guessed at.
    assert!(report.contains("util"), "{report}");

    let toml = fs::read_to_string(root.join("jails.toml")).unwrap();
    assert!(toml.contains("web = \"controllers\""), "{toml}");
    assert!(toml.contains("adapters = \"persistence\""), "{toml}");
    assert!(
        !toml.contains("capabilities"),
        "adopt must never write the list `sync` acts on: {toml}"
    );

    let after = jails_cmd(&root, None).arg("stats").output().unwrap();
    let after = String::from_utf8_lossy(&after.stdout).to_string();
    assert!(after.contains("Web"), "{after}");
    assert!(after.contains("Adapters"), "{after}");
}

/// `plan.md` §10.2. The measured finding is recorded in §19.1: `--fast` does
/// not beat `mvnd`, so what this test pins is not speed but the two properties
/// that make the path safe to offer at all.
#[test]
fn test_fast_is_a_visible_alias_for_the_complete_auto_engine() {
    let root = temp_dir("test-fast");
    write_plain_fixture(&root);

    // Nothing compiled: auto repairs through the build tool and explains why
    // the warm partition was not selected. The alias never narrows the suite.
    let cold = jails_cmd(&root, None)
        .args(["test", "--fast"])
        .output()
        .unwrap();
    let report = String::from_utf8_lossy(&cold.stdout);
    assert!(
        report.contains("`--fast` normalized to auto")
            && report.contains("compiled test outputs are stale"),
        "the alias must expose the complete auto-engine decision: {report}"
    );
}

#[test]
fn auto_engine_merges_warm_and_build_partitions_without_losing_a_selector() {
    if !real_mvn_available() || !real_java_supports_target_release() {
        skip("mvn or a new enough JDK not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("mixed-test-partitions");
    write_plain_fixture(&root);
    let tests = common::generated(&root, "src/test/java/com/example/demo");
    fs::create_dir_all(&tests).unwrap();
    fs::write(
        tests.join("PlainTest.java"),
        "package com.example.demo;\nimport org.junit.jupiter.api.Test;\nclass PlainTest { @Test void plain() {} }\n",
    )
    .unwrap();
    fs::write(
        tests.join("GlobalTest.java"),
        "package com.example.demo;\nimport org.junit.jupiter.api.Test;\nclass GlobalTest { @Test void global() { System.setProperty(\"jails.mixed\", \"yes\"); } }\n",
    )
    .unwrap();

    let prepared = jails_cmd_with_path(&root, &path)
        .args(["test", "--fast"])
        .output()
        .unwrap();
    if !prepared.status.success() {
        skip("could not prepare the mixed-engine fixture");
        return;
    }

    let output = jails_cmd_with_path(&root, &path)
        .args(["test", "PlainTest", "GlobalTest", "--output", "json"])
        .output()
        .unwrap();
    let json = String::from_utf8_lossy(&output.stdout);
    let _ = jails_cmd_with_path(&root, &path)
        .args(["testd", "--stop"])
        .output();
    assert!(
        output.status.success(),
        "{json}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        json.lines().count(),
        1,
        "watch-free JSON is one report: {json}"
    );
    assert!(
        json.contains("\"selector\":\"com.example.demo.GlobalTest#global\"")
            && json.contains("\"engine\":\"maven\"")
            && json.contains("\"selector\":\"com.example.demo.PlainTest#plain\"")
            && json.contains("\"engine\":\"testd-v2\""),
        "the merged report must contain both disjoint partitions: {json}"
    );
}

#[test]
fn a_timed_warm_run_cancels_the_request_and_recycles_the_daemon() {
    if !real_mvn_available() || !real_java_supports_target_release() {
        skip("mvn or a new enough JDK not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("timed-warm-test");
    write_plain_fixture(&root);
    let tests = common::generated(&root, "src/test/java/com/example/demo");
    fs::create_dir_all(&tests).unwrap();
    fs::write(
        tests.join("SlowTest.java"),
        "package com.example.demo;\nimport org.junit.jupiter.api.Test;\nclass SlowTest { @Test void slow() throws Exception { Thread.sleep(30_000); } }\n",
    )
    .unwrap();
    // **The warm-up compiles `SlowTest` without running it, and leaves a
    // daemon up.** What is being proved below is that a one-second budget
    // cancels a request still in flight, so the fixture's test sleeps thirty
    // seconds and the warm-up must not be the thing that waits for it: naming
    // `PingTest` as the selector leaves `SlowTest` compiled and unrun, which
    // is what the timed run needs since it passes `--compile none`. A bare
    // `jails test --fast` sits through all thirty, which measured 33.7s at the
    // tail of `tests/cli` on an otherwise idle box -- straight onto the
    // suite's wall clock.
    fs::write(
        tests.join("PingTest.java"),
        "package com.example.demo;\nimport org.junit.jupiter.api.Test;\nclass PingTest { @Test void ping() {} }\n",
    )
    .unwrap();

    let prepared = jails_cmd_with_path(&root, &path)
        .args(["test", "--fast", "PingTest"])
        .output()
        .unwrap();
    if !prepared.status.success() {
        skip("could not prepare the timed warm-test fixture");
        return;
    }

    // **The daemon has to be up before the clock starts.** `--fast` is the
    // launcher, not `testd`, so without this the timed run pays a cold JVM
    // boot and the daemon's own `--scan-class-path` warm-up inside the window
    // it is measuring -- which is not what a one-second budget is about, and
    // which drifts past any wall-clock bound as soon as the machine is busy.
    let daemon = jails_cmd_with_path(&root, &path)
        .args(["test", "PingTest", "--engine", "warm", "--compile", "none"])
        .output()
        .unwrap();
    if !daemon.status.success() {
        skip("could not start testd for the timed warm-test fixture");
        return;
    }

    let started = std::time::Instant::now();
    let output = jails_cmd_with_path(&root, &path)
        .args([
            "test",
            "SlowTest",
            "--engine",
            "warm",
            "--compile",
            "none",
            "--timeout",
            "1s",
        ])
        .output()
        .unwrap();
    let elapsed = started.elapsed();
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !output.status.success(),
        "the timed test unexpectedly passed: {report}"
    );
    assert!(
        report.contains("active request was cancelled") && report.contains("testd was recycled"),
        "the timeout must explain both cancellation and isolation cleanup: {report}"
    );
    assert!(
        elapsed < std::time::Duration::from_secs(10),
        "the 1s timeout took {elapsed:?}"
    );

    let status = jails_cmd_with_path(&root, &path)
        .args(["testd", "--status"])
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&status.stdout).contains("not running"),
        "the cancelled daemon must not retain a test thread: {}{}",
        String::from_utf8_lossy(&status.stdout),
        String::from_utf8_lossy(&status.stderr)
    );
}

/// `plan.md` item 13, and the two things about a daemon that must be true
/// before speed matters at all.
///
/// The first is that it refuses rather than runs when the classes are older
/// than their sources -- a resident JVM makes a green-over-deleted-code report
/// *faster*, not less wrong.
///
/// The second is the one that cannot be checked by reading: that a run really
/// does see a class recompiled since the daemon started. The daemon holds the
/// dependencies on its own classpath and hands only `target/classes` and
/// `target/test-classes` to JUnit as `--class-path`, so JUnit builds a child
/// loader for them per run. Put the outputs on the daemon's classpath too and
/// parent-first delegation serves the *stale* class forever, silently -- which
/// looks exactly like a working daemon until someone notices a fixed test
/// still failing.
#[test]
fn testd_refuses_stale_classes_and_sees_a_recompile_after_it_started() {
    if !real_mvn_available() || !real_java_supports_target_release() {
        skip("mvn or a new enough JDK not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-testd");
    write_plain_fixture(&root);
    // The plain fixture ships no test, and a daemon that finds nothing to run
    // would satisfy every assertion below for the wrong reason.
    let test_dir = common::generated(&root, "src/test/java/com/example/demo");
    std::fs::create_dir_all(&test_dir).unwrap();
    let test_source = test_dir.join("AppTest.java");
    std::fs::write(
        &test_source,
        "package com.example.demo;\n\nimport org.junit.jupiter.api.Test;\n\n         class AppTest {\n    @Test\n    void passes() {}\n}\n",
    )
    .unwrap();

    // Nothing compiled: refused, and the refusal names the way out.
    let cold = jails_cmd_with_path(&root, &path)
        .args(["testd"])
        .output()
        .unwrap();
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&cold.stdout),
        String::from_utf8_lossy(&cold.stderr)
    );
    assert!(
        report.contains("testd not taken") && report.contains("fix:"),
        "an uncompiled project must be refused, with a fix: {report}"
    );

    // `--fast` splices the console launcher, pinned to this project's JUnit,
    // and compiles. `testd` shares that dependency rather than having its own
    // idea of which JUnit this is.
    let prepared = jails_cmd_with_path(&root, &path)
        .args(["test", "--fast"])
        .output()
        .unwrap();
    if !prepared.status.success() {
        skip("could not prepare the fixture with `test --fast`");
        return;
    }

    let first = jails_cmd_with_path(&root, &path)
        .args(["testd"])
        .output()
        .unwrap();
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(
        first.status.success(),
        "the first daemon run failed: {report}"
    );
    assert!(
        report.contains("tests successful"),
        "the daemon must report a real JUnit run: {report}"
    );
    let normalized = jails_cmd_with_path(&root, &path)
        .args(["test", "--engine", "warm", "--compile", "none", "--json"])
        .output()
        .unwrap();
    let json = String::from_utf8_lossy(&normalized.stdout);
    assert!(normalized.status.success(), "{json}");
    assert!(
        json.contains("\"selector\":\"com.example.demo.AppTest#passes\"")
            && json.contains("\"engine\":\"testd-v2\"")
            && json.contains("\"duration_us\":"),
        "the warm engine must return actual normalized cases: {json}"
    );

    // Now add a failing test *after* the daemon is up, recompile, and run
    // again. A daemon serving cached classes would still report success here.
    std::fs::write(
        &test_source,
        "package com.example.demo;\n\nimport org.junit.jupiter.api.Test;\n         import static org.junit.jupiter.api.Assertions.fail;\n\n         class AppTest {\n    @Test\n    void passes() {}\n\n         \x20   @Test\n    void addedAfterTheDaemonStarted() {\n        fail(\"seen\");\n    }\n}\n",
    )
    .unwrap();

    let compiled = real_maven_cmd(&root, &path)
        .args(["-q", "-o", "test-compile"])
        .status();
    if !matches!(compiled, Ok(status) if status.success()) {
        let _ = jails_cmd_with_path(&root, &path)
            .args(["testd", "--stop"])
            .output();
        skip("offline Maven could not recompile the fixture");
        return;
    }

    let after = jails_cmd_with_path(&root, &path)
        .args(["testd"])
        .output()
        .unwrap();
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&after.stdout),
        String::from_utf8_lossy(&after.stderr)
    );
    let _ = jails_cmd_with_path(&root, &path)
        .args(["testd", "--stop"])
        .output();
    assert!(
        !after.status.success() && report.contains("tests failed"),
        "the daemon must see a class recompiled after it started: {report}"
    );
}

/// `plan.md` §10.2 step 3. The property worth an integration test is not that
/// the selection is small -- it is that it is small *and* transitive: a change
/// to a record two hops from a test still selects that test. A one-hop version
/// looks correct on any scaffold and quietly misses the failure.
#[test]
fn testd_affected_selects_transitively_and_widens_when_it_cannot_know() {
    if !real_mvn_available() || !real_java_supports_target_release() {
        skip("mvn or a new enough JDK not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-affected");
    write_plain_fixture(&root);

    // Money <- Order <- OrderTest, and a Money change must reach OrderTest.
    for (relative, body) in [
        (
            "src/main/java/com/example/demo/Money.java",
            "package com.example.demo;\n\npublic record Money(long amount) {}\n",
        ),
        (
            "src/main/java/com/example/demo/Order.java",
            "package com.example.demo;\n\npublic record Order(Money total) {}\n",
        ),
        (
            "src/test/java/com/example/demo/OrderTest.java",
            "package com.example.demo;\n\nimport org.junit.jupiter.api.Test;\n\n             class OrderTest {\n    @Test\n    void holds() { new Order(new Money(1)); }\n}\n",
        ),
        (
            "src/test/java/com/example/demo/UnrelatedTest.java",
            "package com.example.demo;\n\nimport org.junit.jupiter.api.Test;\n\n             class UnrelatedTest {\n    @Test\n    void alone() {}\n}\n",
        ),
    ] {
        let file = root.join(relative);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, body).unwrap();
    }

    let prepared = jails_cmd_with_path(&root, &path)
        .args(["test", "--fast"])
        .output()
        .unwrap();
    if !prepared.status.success() {
        skip("could not prepare the fixture with `test --fast`");
        return;
    }

    // No git here, so the selector must widen rather than select nothing --
    // and say which unknown it hit.
    let blind = jails_cmd_with_path(&root, &path)
        .args(["testd", "--affected"])
        .output()
        .unwrap();
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&blind.stdout),
        String::from_utf8_lossy(&blind.stderr)
    );
    let _ = jails_cmd_with_path(&root, &path)
        .args(["testd", "--stop"])
        .output();
    assert!(
        report.contains("running everything") && report.contains("git"),
        "without git the selector must widen and name the reason: {report}"
    );

    // Now give it a git repository and the transitive question can be asked.
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(&root)
            .env("PATH", &path)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .output()
    };
    if !matches!(git(&["init", "-q"]), Ok(out) if out.status.success()) {
        skip("git not available");
        return;
    }
    let _ = git(&["add", "-A"]);
    if !matches!(git(&["commit", "-qm", "base"]), Ok(out) if out.status.success()) {
        skip("git could not commit the fixture");
        return;
    }

    // Change Money, which OrderTest reaches only *through* Order. Recompile
    // first, or the staleness gate refuses before the selector is consulted.
    let money = common::generated(&root, "src/main/java/com/example/demo/Money.java");
    std::fs::write(
        &money,
        "package com.example.demo;\n\npublic record Money(long amount) {\n    // edited\n}\n",
    )
    .unwrap();
    let compiled = real_maven_cmd(&root, &path)
        .args(["-q", "-o", "test-compile"])
        .status();
    if !matches!(compiled, Ok(status) if status.success()) {
        skip("offline Maven could not recompile the fixture");
        return;
    }

    let selected = jails_cmd_with_path(&root, &path)
        .args(["testd", "--affected"])
        .output()
        .unwrap();
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&selected.stdout),
        String::from_utf8_lossy(&selected.stderr)
    );
    let _ = jails_cmd_with_path(&root, &path)
        .args(["testd", "--stop"])
        .output();
    assert!(selected.status.success(), "{report}");
    // One class, and it is the one two hops away. `UnrelatedTest` proves the
    // selection is a selection: a version that ran everything would also
    // satisfy "OrderTest ran".
    assert!(
        report.contains("1 test class(es) reachable"),
        "a Money change must select exactly OrderTest, transitively: {report}"
    );
    assert!(
        report.contains("1 tests successful"),
        "and it must actually run: {report}"
    );
}

/// `plan.md` §17 item 5b. k6 is not installed here, so what is checked is the
/// two refusals and the command jails would run — the tier-2 shape, which is
/// the honest one when the tool under test is absent.
#[test]
fn bench_refuses_without_a_load_test_and_without_k6() {
    let root = temp_dir("bench");
    write_spring_fixture(&root);

    let output = jails_cmd(&root, None).arg("bench").output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(stderr.contains("jails add loadtest"), "{stderr}");

    // `add loadtest` derives its route list from the project, so there has to
    // be a route.
    let status = jails_cmd(&root, None)
        .args(["generate", "controller", "Health"])
        .status()
        .unwrap();
    assert!(status.success());
    let status = jails_cmd(&root, None)
        .args(["add", "loadtest", "--no-start"])
        .status()
        .unwrap();
    assert!(status.success());

    // A PATH with no k6 on it, whatever this machine happens to have.
    let empty = root.join("no-tools");
    fs::create_dir_all(&empty).unwrap();
    let output = jails_cmd(&root, Some(&empty))
        .arg("bench")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(stderr.contains("k6 is not on PATH"), "{stderr}");
    assert!(stderr.contains("mise use -g k6"), "{stderr}");
}

/// The profile is printed before the run, because a latency number without the
/// load that produced it is not a measurement.
#[test]
fn bench_states_the_profile_it_is_about_to_run() {
    let root = temp_dir("bench-profile");
    write_spring_fixture(&root);
    let status = jails_cmd(&root, None)
        .args(["generate", "controller", "Health"])
        .status()
        .unwrap();
    assert!(status.success());
    let status = jails_cmd(&root, None)
        .args(["add", "loadtest", "--no-start"])
        .status()
        .unwrap();
    assert!(status.success());

    let tools = root.join("tools");
    write_fake_maven(&tools, &["k6"], &root.join("k6.log"));

    let output = jails_cmd(&root, Some(&tools))
        .args(["bench", "--vus", "40", "--duration", "2m"])
        .output()
        .unwrap();
    let report = String::from_utf8_lossy(&output.stdout);
    assert!(
        report.contains("40 virtual users for 2m"),
        "the profile has to be stated: {report}"
    );

    let argv = fs::read_to_string(root.join("k6.log")).unwrap();
    assert!(argv.contains("run"), "{argv}");
    assert!(argv.contains("load-tests/load-test.js"), "{argv}");
}

/// A project jails did not write: adopted, edited, generated into, and built.
///
/// `simplify-sol.md`'s G5 asks for *sanitized adopted and reader-edited
/// Spring/plain projects*, and this is the gap the proof corpus had: every
/// manifest in `examples/proof-policy.tsv` is jails' own output, so nothing in
/// the suite proved the tool against a codebase it did not generate. A
/// generator can be perfectly correct about its own layout and still be wrong
/// about somebody else's.
///
/// The pom here is hand-written rather than `write_plain_fixture`'s: a
/// different groupId, a different artifactId, a source layout jails would not
/// have chosen, and classes with bodies. Nothing in it came from jails.
///
/// Three things are proved, in order. Adoption reads the foreign layout;
/// generation into it produces Java that a real compiler accepts *beside* the
/// reader's; and the reader's files come back byte-identical. The last is the
/// one that would be easiest to break and hardest to notice.
#[test]
fn an_adopted_reader_written_project_generates_compiles_and_keeps_its_own_bytes() {
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
    let root = temp_dir("adopted-foreign-project");
    // The fixture lives in `tests/common` because `tests/differential.rs` runs
    // the same project through both implementations. Two copies of a project
    // that is *defined* by being foreign would drift into two different
    // foreignnesses.
    write_adopted_fixture(&root, Adopted::Plain);
    let before = adopted_reader_bytes(&root, Adopted::Plain);

    // 1. Adoption reads a layout jails did not choose.
    let adopted = jails_cmd_with_path(&root, &path)
        .arg("adopt")
        .output()
        .unwrap();
    assert!(
        adopted.status.success(),
        "adopt failed on a reader-written project: {}",
        String::from_utf8_lossy(&adopted.stderr)
    );
    let report = String::from_utf8_lossy(&adopted.stdout);
    assert!(
        report.contains("web") && report.contains("persistence"),
        "adopt did not recognise the reader's directories: {report}"
    );

    // 2. Generation lands beside the reader's code and compiles with it.
    let generated = jails_cmd_with_path(&root, &path)
        .args(["g", "record", "Receipt", "id:uuid", "total:long"])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "generate failed in an adopted project: {}",
        String::from_utf8_lossy(&generated.stderr)
    );

    let built = real_maven_cmd(&root, &path)
        .args(["-B", "test"])
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "an adopted project did not build after `g record`:\n{}\n{}",
        String::from_utf8_lossy(&built.stdout),
        String::from_utf8_lossy(&built.stderr)
    );

    // 3. The reader's bytes are the reader's.
    assert_eq!(
        adopted_reader_bytes(&root, Adopted::Plain),
        before,
        "jails rewrote a file it did not author"
    );

    // 4. Rerun is idempotent, which is what `simplify-sol.md`'s differential
    // list asks for. Re-declaring the same record is not a collision to refuse
    // -- identity is the entity, so this is an update that changes nothing --
    // and the check that matters is that it *says* so and writes nothing.
    let generated_at =
        common::generated(&root, "src/main/java/net/acme/legacy/domain/Receipt.java");
    let generated_before = fs::read_to_string(&generated_at).unwrap();
    let again = jails_cmd_with_path(&root, &path)
        .args(["g", "record", "Receipt", "id:uuid", "total:long"])
        .output()
        .unwrap();
    assert!(
        again.status.success(),
        "a repeated generate failed instead of settling: {}",
        String::from_utf8_lossy(&again.stderr)
    );
    assert!(
        String::from_utf8_lossy(&again.stdout).contains("nothing to do"),
        "a repeated generate did not report itself a no-op: {}",
        String::from_utf8_lossy(&again.stdout)
    );
    assert_eq!(
        fs::read_to_string(&generated_at).unwrap(),
        generated_before,
        "a repeated generate rewrote its own output"
    );
    assert_eq!(
        adopted_reader_bytes(&root, Adopted::Plain),
        before,
        "a repeated generate rewrote a file it did not author"
    );
}
