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
    let test_file = root.join("src/test/java/com/example/demo/PayoutTest.java");
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
    let source = root.join("src/test/java/com/example/demo/ContextTest.java");
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
    let pkg_dir = root.join("src/main/java/com/example/demo");
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
        .args(["run", "--launcher", "build-tool"])
        .status()
        .unwrap();
    assert!(status.success());
    assert!(read_log(&log).contains("spring-boot:run"));
}

#[test]
fn run_starts_compose_services_only_when_explicitly_requested() {
    let root = temp_dir("mock-run-compose");
    let pkg_dir = root.join("src/main/java/com/example/demo");
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
    fs::write(
        root.join("compose.yaml"),
        "services:\n  postgres:\n    image: postgres:17-alpine\n",
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
        invocation.contains("compose up -d"),
        "expected docker compose up before spring-boot:run: {invocation}"
    );
    assert!(invocation.contains("spring-boot:run"));
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
        lines.iter().any(|l| l.ends_with("compose up -d postgres")),
        "start db: {invocation}"
    );
    assert!(
        lines.iter().any(|l| l.ends_with("compose stop kafka")),
        "stop kafka: {invocation}"
    );
    assert!(
        lines.iter().any(|l| l.ends_with("compose up -d")),
        "bare start should be `up -d` with no service filter: {invocation}"
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
        invocation.contains("compose up -d postgres"),
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
    let fake = temp_dir("console-jshell-bin");
    let log = fake.join("log.txt");
    write_fake_maven(&fake, &["mvn", "jshell"], &log);
    assert!(
        jails_cmd(&root, Some(&fake))
            .args(["console", "--no-build"])
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
        invocation.contains("jshell") && invocation.contains("--class-path"),
        "{invocation}"
    );
    assert!(
        !invocation.contains(" compile") && !invocation.contains("/mvn compile"),
        "--no-build should skip compile: {invocation}"
    );
}

#[test]
fn run_command_compiles_before_attempting_a_plain_main_class() {
    let root = temp_dir("mock-run-plain");
    let pkg_dir = root.join("src/main/java/com/example/demo");
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
    let pkg_dir = root.join("src/main/java/com/example/demo");
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
    let pkg_dir = root.join("src/main/java/com/example/demo");
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
        root.join(".jails/run/runtime-classpath-v1").is_file(),
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
    let main = root.join("src/main/java/com/acme/shop");
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

    // Generating works, and says what the missing pom cost this output.
    let generated = jails_cmd(&root, None)
        .args(["generate", "record", "Order", "id:uuid@pk", "total:long"])
        .output()
        .unwrap();
    let report = String::from_utf8_lossy(&generated.stdout);
    assert!(
        generated.status.success(),
        "{report}{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    assert!(
        root.join("src/main/java/com/acme/shop/domain/Order.java")
            .is_file()
    );
    assert!(report.contains("Gradle project"), "{report}");
    assert!(report.contains("plain JDBC"), "{report}");

    // The Maven-inherent ones refuse, and the refusal names a way forward.
    for command in ["test", "build", "check", "clean"] {
        let refused = jails_cmd(&root, None).arg(command).output().unwrap();
        let stderr = String::from_utf8_lossy(&refused.stderr);
        assert!(!refused.status.success(), "`{command}` should refuse");
        assert!(stderr.contains("built by Gradle"), "{command}: {stderr}");
        assert!(stderr.contains("routes"), "{command}: {stderr}");
    }
    let refused = jails_cmd(&root, None).args(["add", "db"]).output().unwrap();
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

/// `plan.md` §12: `jails adopt` writes a `[layout]` table, not new machinery.
/// The proof is that a *reporting* command's answer changes with no code path
/// of its own — `stats` counted a renamed web package as "Other".
#[test]
fn adopt_teaches_jails_where_an_existing_project_keeps_things() {
    let root = temp_dir("adopt");
    write_plain_fixture(&root);
    let base = root.join("src/main/java/com/example/demo");
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
    let tests = root.join("src/test/java/com/example/demo");
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
    let test_dir = root.join("src/test/java/com/example/demo");
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
    let money = root.join("src/main/java/com/example/demo/Money.java");
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
