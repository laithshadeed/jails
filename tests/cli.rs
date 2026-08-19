mod common;

use common::*;
use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::process::Stdio;

// ---- offline, filesystem-only: exercise the real binary against real
// temp dirs, no Maven involved. ----

#[test]
fn completion_prints_a_bash_completion_script() {
    let workdir = temp_dir("completion");
    let output = jails_cmd(&workdir, None)
        .args(["completion", "bash"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let script = String::from_utf8_lossy(&output.stdout);
    assert!(script.contains("_jails()"));
    assert!(script.contains("complete -F _jails"));
}

#[test]
fn about_describes_a_synthetic_nested_maven_reactor() {
    let root = temp_dir("about-reactor");
    fs::write(
        root.join("pom.xml"),
        "<project><groupId>dev.example</groupId><artifactId>sample-parent</artifactId><properties><java.version>26</java.version></properties><dependencyManagement><dependencies><dependency><groupId>org.springframework.boot</groupId><artifactId>spring-boot-dependencies</artifactId></dependency></dependencies></dependencyManagement><modules><module>sample-core</module><module>sample-web</module></modules></project>",
    )
    .unwrap();
    for module in ["sample-core", "sample-web"] {
        let module_root = root.join(module);
        fs::create_dir_all(module_root.join("src/main/java/dev/example")).unwrap();
        fs::write(
            module_root.join("pom.xml"),
            format!("<project><parent><groupId>dev.example</groupId><artifactId>sample-parent</artifactId></parent><artifactId>{module}</artifactId></project>"),
        )
        .unwrap();
    }
    fs::write(root.join("mvnw"), "#!/bin/sh\n").unwrap();
    let cwd = root.join("sample-web/src/main/java/dev/example");

    let output = jails_cmd(&cwd, None).arg("about").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Workspace: sample-parent"));
    assert!(stdout.contains("Module: sample-web"));
    assert!(stdout.contains("Java: 26"));
    assert!(stdout.contains("Framework: Spring Boot"));
    assert!(stdout.contains("Modules (2):"));

    let output = jails_cmd(&cwd, None)
        .args(["info", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json = String::from_utf8_lossy(&output.stdout);
    assert!(json.contains("\"schema_version\": 1"));
    assert!(json.contains("\"artifact_id\": \"sample-parent\""));
    assert!(json.contains("\"artifact_id\": \"sample-web\""));
    assert!(json.contains("\"java_release\": 26"));
    assert!(json.contains("\"spring_boot\": true"));
}

#[test]
fn about_errors_outside_a_maven_project() {
    let root = temp_dir("about-no-project");
    let output = jails_cmd(&root, None).arg("about").output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("pom.xml"));
}

/// Regression test: `kind` used to be a plain String, so `jails generate
/// <TAB>` had nothing to offer but filenames, and the `g`/`d` aliases were
/// declared with `alias` (hidden from clap_complete) instead of
/// `visible_alias`, so `jails g <TAB>` fell back to top-level subcommand
/// names instead of the artifact-kind list.
#[test]
fn completion_offers_artifact_kinds_for_generate_destroy_and_their_aliases() {
    let workdir = temp_dir("completion-kinds");
    let output = jails_cmd(&workdir, None)
        .args(["completion", "bash"])
        .output()
        .unwrap();
    let script = String::from_utf8_lossy(&output.stdout);

    let generate_opts = opts_line_for(&script, "jails__subcmd__generate)");
    let destroy_opts = opts_line_for(&script, "jails__subcmd__destroy)");
    for kind in [
        "scaffold",
        "controller",
        "service",
        "repo",
        "record",
        "interface",
        "migration",
        "integration-test",
        "command",
        "test",
    ] {
        assert!(
            generate_opts.contains(kind),
            "expected generate's opts ({generate_opts:?}) to include {kind}"
        );
        assert!(
            destroy_opts.contains(kind),
            "expected destroy's opts ({destroy_opts:?}) to include {kind}"
        );
    }

    assert!(
        script.contains("jails,g)"),
        "expected the `g` alias to transition completion state"
    );
    assert!(
        script.contains("jails,d)"),
        "expected the `d` alias to transition completion state"
    );
}

/// Pulls the `opts="..."` line right after a `<marker>)` case arm.
fn opts_line_for<'a>(script: &'a str, marker: &str) -> &'a str {
    let start = script
        .find(marker)
        .unwrap_or_else(|| panic!("marker {marker} not found in completion script"));
    script[start..]
        .lines()
        .find(|l| l.trim_start().starts_with("opts="))
        .unwrap()
}

#[test]
fn new_cli_creates_expected_project_layout() {
    let workdir = temp_dir("new-cli-layout");
    let status = jails_cmd(&workdir, None)
        .args(["new-cli", "demo"])
        .status()
        .unwrap();
    assert!(status.success());

    let root = workdir.join("demo");
    assert!(root.join("pom.xml").is_file());
    assert!(
        root.join("src/main/java/com/example/demo/App.java")
            .is_file()
    );
    assert!(
        root.join("src/test/java/com/example/demo/AppTest.java")
            .is_file()
    );
}

#[test]
fn new_cli_fails_if_the_directory_already_exists() {
    let workdir = temp_dir("new-cli-exists");
    fs::create_dir(workdir.join("demo")).unwrap();
    let output = jails_cmd(&workdir, None)
        .args(["new-cli", "demo"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("already exists"));
}

#[test]
fn generate_standalone_and_destroy_roundtrip() {
    let root = temp_dir("standalone-roundtrip");
    write_project_skeleton(&root);

    let status = jails_cmd(&root, None)
        .args(["generate", "controller", "comment"])
        .status()
        .unwrap();
    assert!(status.success());
    let file = root.join("src/main/java/com/example/demo/web/CommentController.java");
    assert!(file.is_file());
    let contents = fs::read_to_string(&file).unwrap();
    assert!(contents.contains("public class CommentController"));
    // Rails generates a test alongside `generate controller`; jails matches that.
    let test_file = root.join("src/test/java/com/example/demo/web/CommentControllerTest.java");
    assert!(test_file.is_file(), "expected {}", test_file.display());

    let status = jails_cmd(&root, None)
        .args(["destroy", "controller", "comment", "--force"])
        .status()
        .unwrap();
    assert!(status.success());
    assert!(!file.is_file());
    assert!(!test_file.is_file());
}

#[test]
fn generate_scaffold_writes_a_raw_jdbc_slice() {
    let root = temp_dir("scaffold-files");
    write_project_skeleton(&root);

    let status = jails_cmd(&root, None)
        .args(["generate", "scaffold", "Post", "title:string", "body:text"])
        .status()
        .unwrap();
    assert!(status.success());

    let pkg = root.join("src/main/java/com/example/demo");
    assert!(pkg.join("domain/Post.java").is_file());
    assert!(pkg.join("app/PostRepository.java").is_file());
    assert!(pkg.join("adapters/JdbcPostRepository.java").is_file());
    assert!(pkg.join("service/PostService.java").is_file());
    assert!(pkg.join("web/PostController.java").is_file());
    assert!(
        root.join("src/test/java/com/example/demo/adapters/JdbcPostRepositoryIT.java")
            .is_file()
    );
    assert!(
        root.join("src/test/java/com/example/demo/web/PostControllerTest.java")
            .is_file()
    );
}

#[test]
fn destroy_without_force_prompts_and_aborts_on_no() {
    let root = temp_dir("destroy-prompt");
    write_project_skeleton(&root);
    jails_cmd(&root, None)
        .args(["generate", "controller", "comment"])
        .status()
        .unwrap();
    let file = root.join("src/main/java/com/example/demo/web/CommentController.java");
    assert!(file.is_file());

    let mut child = jails_cmd(&root, None)
        .args(["destroy", "controller", "comment"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"n\n").unwrap();
    let status = child.wait().unwrap();

    assert!(status.success());
    assert!(
        file.is_file(),
        "file should survive a declined confirmation"
    );
}

#[test]
fn generate_errors_on_duplicate_file() {
    let root = temp_dir("duplicate");
    write_project_skeleton(&root);
    jails_cmd(&root, None)
        .args(["generate", "service", "comment"])
        .status()
        .unwrap();
    let output = jails_cmd(&root, None)
        .args(["generate", "service", "comment"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("already exists"));
}

#[test]
fn generate_errors_on_unknown_field_type() {
    let root = temp_dir("unknown-field-type");
    write_project_skeleton(&root);
    let output = jails_cmd(&root, None)
        .args(["generate", "record", "widget", "id:nope"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown field type"));
}

#[test]
fn generate_errors_outside_a_project() {
    let root = temp_dir("no-project");
    let output = jails_cmd(&root, None)
        .args(["generate", "record", "widget"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("pom.xml"));
}

#[test]
fn short_generators_cover_raw_sql_and_test_seams() {
    let root = temp_dir("simple-generators");
    write_project_skeleton(&root);

    for args in [
        vec!["g", "interface", "IdGenerator"],
        vec!["g", "integration-test", "DatabaseSmoke"],
        vec!["g", "migration", "createRewardCore"],
        vec!["g", "mig", "add_outbox"],
        vec!["g", "repository", "Reward"],
    ] {
        let output = jails_cmd(&root, None).args(&args).output().unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    assert!(
        root.join("src/main/java/com/example/demo/IdGenerator.java")
            .is_file()
    );
    assert!(
        root.join("src/test/java/com/example/demo/DatabaseSmokeIT.java")
            .is_file()
    );
    assert!(
        root.join("src/main/resources/db/migration/V001__create_reward_core.sql")
            .is_file()
    );
    assert!(
        root.join("src/main/resources/db/migration/V002__add_outbox.sql")
            .is_file()
    );
    assert!(
        root.join("src/main/java/com/example/demo/app/RewardRepository.java")
            .is_file()
    );
    let adapter = fs::read_to_string(
        root.join("src/main/java/com/example/demo/adapters/JdbcRewardRepository.java"),
    )
    .unwrap();
    assert!(adapter.contains("PreparedStatement") || adapter.contains("prepareStatement"));
    assert!(
        adapter.contains("\"\"\""),
        "raw SQL should be emitted as text blocks: {adapter}"
    );
    assert!(!adapter.contains("org.springframework"));
}

/// A minimal project skeleton (pom.xml + an *Application.java) good enough
/// for generate/destroy's path resolution -- not a real, resolvable Maven
/// project, since these tests never invoke Maven.
fn write_project_skeleton(root: &std::path::Path) {
    let pkg_dir = root.join("src/main/java/com/example/demo");
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::write(root.join("pom.xml"), "<project></project>").unwrap();
    fs::write(
        pkg_dir.join("DemoApplication.java"),
        "package com.example.demo;\n\npublic class DemoApplication {}\n",
    )
    .unwrap();
}

/// Like `write_project_skeleton`, but with the pom `add` actually reads: a
/// declared release level and a `<dependencies>` element to splice into.
/// Still never handed to Maven.
fn write_plain_fixture(root: &std::path::Path) {
    let pkg_dir = root.join("src/main/java/com/example/demo");
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::write(
        root.join("pom.xml"),
        format!(
            "<project>\n    <groupId>com.example</groupId>\n    <artifactId>demo</artifactId>\n    <properties>\n        <maven.compiler.release>{TARGET_RELEASE}</maven.compiler.release>\n    </properties>\n    <dependencies>\n        <dependency>\n            <groupId>org.junit.jupiter</groupId>\n            <artifactId>junit-jupiter</artifactId>\n            <version>5.11.4</version>\n            <scope>test</scope>\n        </dependency>\n    </dependencies>\n</project>\n"
        ),
    )
    .unwrap();
    fs::write(
        pkg_dir.join("DemoApplication.java"),
        "package com.example.demo;\n\npublic class DemoApplication {}\n",
    )
    .unwrap();
}

// ---- mocked mvn/mvnd: verify jails' own command-construction logic
// (which binary, which args) without needing real Maven. ----

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
fn run_command_uses_spring_boot_run_for_spring_projects() {
    let root = temp_dir("mock-run-spring");
    let pkg_dir = root.join("src/main/java/com/example/demo");
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::write(
        root.join("pom.xml"),
        "<project>org.springframework.boot</project>",
    )
    .unwrap();
    let fake_dir = temp_dir("mock-run-spring-bin");
    let log = fake_dir.join("log.txt");
    write_fake_maven(&fake_dir, &["mvn"], &log);

    let status = jails_cmd(&root, Some(&fake_dir))
        .arg("run")
        .status()
        .unwrap();
    assert!(status.success());
    assert!(read_log(&log).contains("spring-boot:run"));
}

#[test]
fn run_starts_compose_services_when_compose_yaml_exists() {
    let root = temp_dir("mock-run-compose");
    let pkg_dir = root.join("src/main/java/com/example/demo");
    fs::create_dir_all(&pkg_dir).unwrap();
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
        .arg("run")
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
fn completion_script_lists_db_and_console_and_their_aliases() {
    let workdir = temp_dir("completion-db-console");
    let output = jails_cmd(&workdir, None)
        .args(["completion", "bash"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let script = String::from_utf8_lossy(&output.stdout);
    assert!(script.contains("dbconsole"), "{script}");
    assert!(script.contains("console"), "missing console");
    assert!(
        script.contains("jails,c)") || script.contains("jails,c,"),
        "visible alias `c` should complete"
    );
    assert!(
        script.contains("jails,dbconsole)") || script.contains("jails,dbconsole,"),
        "visible alias `dbconsole` should complete"
    );
}

#[test]
fn add_db_no_start_skips_docker_compose_up() {
    let root = temp_dir("add-db-no-start");
    write_plain_fixture(&root);
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
fn run_no_build_skips_mvn_and_runs_an_existing_jar_for_spring_projects() {
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
    assert!(status.success());

    let invocation = read_log(&log);
    assert!(
        invocation.contains("/java "),
        "expected java to run: {invocation}"
    );
    assert!(invocation.contains("-jar"));
    assert!(invocation.contains("demo.jar"));
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
        eprintln!("skipping: java/javac not found on PATH");
        return;
    }
    let root = temp_dir("no-build-plain-real");
    let pkg_dir = root.join("src/main/java/com/example/demo");
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::write(root.join("pom.xml"), "<project></project>").unwrap();
    let source = pkg_dir.join("App.java");
    fs::write(
        &source,
        "package com.example.demo;\n\npublic class App {\n    public static void main(String[] args) {\n        System.out.println(\"no-build-ran\");\n    }\n}\n",
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
        .args(["run", "--no-build"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("no-build-ran"));
}

// ---- real Maven + JDK 26, no network beyond Maven Central artifact
// resolution (never start.spring.io): verify the actual bar the tool
// exists for -- "does new-cli produce a project that passes mvn test?" and
// "does generate scaffold produce a project that compiles?". Skipped
// automatically if mvn isn't on PATH. ----

#[test]
fn new_cli_project_passes_real_mvn_test() {
    if !real_mvn_available() {
        eprintln!("skipping: mvn not found on PATH");
        return;
    }
    if !real_java_supports_target_release() {
        eprintln!("skipping: javac on PATH does not support --release {TARGET_RELEASE}");
        return;
    }
    let path = real_path_without_mvnd();
    let workdir = temp_dir("real-new-cli-test");
    jails_cmd_with_path(&workdir, &path)
        .args(["new-cli", "demo"])
        .status()
        .unwrap();
    let root = workdir.join("demo");

    let status = jails_cmd_with_path(&root, &path)
        .arg("test")
        .status()
        .unwrap();
    assert!(
        status.success(),
        "mvn test failed for a freshly generated new-cli project"
    );
}

#[test]
fn generate_scaffold_produces_a_project_that_compiles_and_passes_tests() {
    if !real_mvn_available() {
        eprintln!("skipping: mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-scaffold-compiles");
    write_spring_fixture(&root);

    let status = jails_cmd_with_path(&root, &path)
        .args([
            "generate",
            "scaffold",
            "Post",
            "title:string",
            "body:text",
            "published:boolean",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let status = jails_cmd_with_path(&root, &path)
        .arg("test")
        .status()
        .unwrap();
    assert!(
        status.success(),
        "mvn test failed for a freshly scaffolded Post resource"
    );
}

/// Regression coverage for the reported bug (standalone `generate
/// controller` not producing a test) plus real-compile verification of the
/// new controller/service/record companion test templates.
#[test]
fn standalone_generators_companion_tests_compile_and_pass() {
    if !real_mvn_available() {
        eprintln!("skipping: mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-standalone-companion-tests");
    write_spring_fixture(&root);

    for args in [
        vec!["generate", "controller", "Health"],
        vec!["generate", "service", "Billing"],
        vec![
            "generate",
            "record",
            "Tag",
            "name:string",
            "createdAt:datetime",
        ],
    ] {
        let status = jails_cmd_with_path(&root, &path)
            .args(&args)
            .status()
            .unwrap();
        assert!(status.success(), "{args:?} failed");
    }

    let status = jails_cmd_with_path(&root, &path)
        .arg("test")
        .status()
        .unwrap();
    assert!(
        status.success(),
        "mvn test failed for the standalone-generated companion tests"
    );
}

/// `record`, `command` and `class` are the plain-Java kinds, so the bar for
/// them is a `new-cli` project -- no Spring anywhere -- that still compiles and
/// passes the tests they generate.
#[test]
fn record_and_command_compile_and_pass_in_a_plain_cli_project() {
    if !real_mvn_available() {
        eprintln!("skipping: mvn not found on PATH");
        return;
    }
    if !real_java_supports_target_release() {
        eprintln!("skipping: javac on PATH does not support --release {TARGET_RELEASE}");
        return;
    }
    let path = real_path_without_mvnd();
    let workdir = temp_dir("real-record-command");
    jails_cmd_with_path(&workdir, &path)
        .args(["new-cli", "demo"])
        .status()
        .unwrap();
    let root = workdir.join("demo");

    for args in [
        vec![
            "generate",
            "record",
            "Money",
            "amount:long",
            "currency:string",
            "on:date",
        ],
        vec!["generate", "command", "Greet"],
        vec!["generate", "class", "MoneyMoved"],
    ] {
        let status = jails_cmd_with_path(&root, &path)
            .args(&args)
            .status()
            .unwrap();
        assert!(status.success(), "{args:?} failed");
    }

    // `class` is the one kind that lands in the base package rather than a
    // subpackage -- a wrong `place()` here would compile and still be wrong.
    assert!(
        root.join("src/main/java/com/example/demo/MoneyMoved.java")
            .exists()
    );
    assert!(
        root.join("src/test/java/com/example/demo/MoneyMovedTest.java")
            .exists()
    );

    let status = jails_cmd_with_path(&root, &path)
        .arg("test")
        .status()
        .unwrap();
    assert!(
        status.success(),
        "mvn test failed for a generated record + command + class"
    );
}

// ---- add ----

/// `jails add <TAB>` only offers completions because `Capability` is a
/// `clap::ValueEnum` and the alias is `visible_alias` -- a hidden `alias` is
/// invisible to clap_complete's bash generator (the same bug `generate`'s `g`
/// hit). This guards both.
#[test]
fn completion_script_lists_add_and_its_capabilities() {
    let root = temp_dir("completion-add");
    fs::create_dir_all(&root).unwrap();
    let output = jails_cmd(&root, None)
        .args(["completion", "bash"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let script = String::from_utf8_lossy(&output.stdout);

    assert!(
        script.contains("add"),
        "completion script never mentions `add`"
    );
    for capability in ["db", "kafka", "csv", "sqlite", "json"] {
        assert!(
            script.contains(capability),
            "completion script never mentions the {capability} capability"
        );
    }
    assert!(
        script.contains("remove"),
        "completion script never mentions `remove`"
    );
    assert!(
        script.contains("jails,rm)"),
        "expected the `rm` alias to transition completion state: {script}"
    );
    assert!(
        script.contains("start"),
        "completion script never mentions `start`"
    );
    assert!(
        script.contains("stop"),
        "completion script never mentions `stop`"
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
    assert!(String::from_utf8_lossy(&output.stderr).contains("no pom.xml"));
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

fn write_release_fixture(root: &std::path::Path, release: &str) {
    let pkg_dir = root.join("src/main/java/com/example/demo");
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::write(
        root.join("pom.xml"),
        format!("<project>\n    <properties>\n        <maven.compiler.release>{release}</maven.compiler.release>\n    </properties>\n    <dependencies>\n    </dependencies>\n</project>\n"),
    )
    .unwrap();
    fs::write(
        pkg_dir.join("DemoApplication.java"),
        "package com.example.demo;\npublic class DemoApplication {}\n",
    )
    .unwrap();
}

#[test]
fn add_dry_run_changes_nothing() {
    let root = temp_dir("add-dry-run");
    write_plain_fixture(&root);
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
    assert!(stdout.contains("would add dependency"), "{stdout}");
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
    write_plain_fixture(&root);

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
    assert!(stdout.contains("exists"), "{stdout}");
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
    write_plain_fixture(&root);

    assert!(
        jails_cmd(&root, None)
            .args(["add", "csv", "--name", "transaction"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        root.join("src/main/java/com/example/demo/adapters/TransactionReader.java")
            .exists()
    );
    assert!(
        root.join("src/test/java/com/example/demo/adapters/TransactionReaderTest.java")
            .exists()
    );
}

/// The bar that matters: does `add csv` leave a project that actually
/// compiles and passes its tests? Needs real Maven and a JDK new enough for
/// the release jails targets.
#[test]
fn add_csv_produces_a_project_that_compiles_and_passes_tests() {
    if !real_mvn_available() {
        eprintln!("skipping: mvn not found on PATH");
        return;
    }
    if !real_java_supports_target_release() {
        eprintln!("skipping: javac on PATH does not support --release {TARGET_RELEASE}");
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

    let status = jails_cmd_with_path(&root, &path)
        .arg("test")
        .status()
        .unwrap();
    assert!(status.success(), "mvn test failed after `jails add csv`");
}

/// tests/common/mod.rs cannot import `pom::TARGET_RELEASE` (integration tests
/// link against the binary, not a library), so it keeps its own copy. This
/// makes the duplication safe.
#[test]
fn target_release_matches_the_binary() {
    let source = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/pom.rs")).unwrap();
    let declared = format!(r#"pub const TARGET_RELEASE: &str = "{TARGET_RELEASE}";"#);
    assert!(
        source.contains(&declared),
        "src/pom.rs no longer declares TARGET_RELEASE = {TARGET_RELEASE}"
    );
}

#[test]
fn add_sqlite_writes_a_first_migration_and_both_classes() {
    let root = temp_dir("add-sqlite-files");
    write_plain_fixture(&root);

    assert!(
        jails_cmd(&root, None)
            .args(["add", "sqlite"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        root.join("src/main/java/com/example/demo/adapters/Database.java")
            .is_file()
    );
    assert!(
        root.join("src/main/java/com/example/demo/adapters/Migrations.java")
            .is_file()
    );
    assert!(
        root.join("src/test/java/com/example/demo/adapters/DatabaseTest.java")
            .is_file()
    );
    assert!(
        root.join("src/main/resources/db/migration/001_init.sql")
            .is_file()
    );
}

#[test]
fn add_db_installs_postgres_flyway_and_testcontainers_without_an_orm() {
    let root = temp_dir("add-db-files");
    write_plain_fixture(&root);
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
    let invocation = read_log(&log);
    assert!(
        invocation.contains("compose up -d postgres"),
        "expected docker compose up: {invocation}"
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
            .args(["add", "db"])
            .status()
            .unwrap()
            .success()
    );
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("spring-boot-starter-jdbc"));
    assert!(pom.contains("spring-boot-docker-compose"));
    assert!(
        !pom.contains("spring-boot-testcontainers"),
        "initializer talks to Testcontainers directly: {pom}"
    );
    assert!(pom.contains("<optional>true</optional>"));
    let config = root.join("src/test/java/com/example/demo/PostgresContainerConfig.java");
    assert!(config.is_file(), "missing {}", config.display());
    let config_src = fs::read_to_string(&config).unwrap();
    assert!(
        config_src.contains("ApplicationContextInitializer"),
        "{config_src}"
    );
    let factories =
        fs::read_to_string(root.join("src/test/resources/META-INF/spring.factories")).unwrap();
    assert!(
        factories.contains("com.example.demo.PostgresContainerConfig"),
        "{factories}"
    );
    let tests =
        fs::read_to_string(root.join("src/test/java/com/example/demo/DemoApplicationTests.java"))
            .unwrap();
    assert!(
        !tests.contains("PostgresContainerConfig"),
        "tests stay untouched; the initializer is registered globally: {tests}"
    );
    let properties =
        fs::read_to_string(root.join("src/main/resources/application.properties")).unwrap();
    assert!(
        properties.contains("spring.persistence.exceptiontranslation.enabled=false"),
        "{properties}"
    );

    let stale_class =
        root.join("target/test-classes/com/example/demo/PostgresContainerConfig.class");
    fs::create_dir_all(stale_class.parent().unwrap()).unwrap();
    fs::write(&stale_class, []).unwrap();
    let stale_factories = root.join("target/test-classes/META-INF/spring.factories");
    fs::create_dir_all(stale_factories.parent().unwrap()).unwrap();
    fs::write(&stale_factories, "leftover\n").unwrap();

    assert!(
        jails_cmd(&root, Some(&fake))
            .args(["remove", "db", "--force"])
            .status()
            .unwrap()
            .success()
    );
    let tests =
        fs::read_to_string(root.join("src/test/java/com/example/demo/DemoApplicationTests.java"))
            .unwrap();
    assert!(!tests.contains("PostgresContainerConfig"), "{tests}");
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
    assert!(!stale_factories.is_file());
}

/// Re-running `add db` on a project that still has the old `@ServiceConnection`
/// + `@Import` wiring must replace the config, register spring.factories, and
/// take the import back out of existing tests.
#[test]
fn add_db_on_spring_migrates_legacy_service_connection() {
    let root = temp_dir("add-db-spring-migrate");
    write_spring_fixture(&root);
    let fake = temp_dir("add-db-spring-migrate-bin");
    let log = fake.join("log.txt");
    write_fake_maven(&fake, &["docker"], &log);

    fs::write(
        root.join("src/test/java/com/example/demo/PostgresContainerConfig.java"),
        r#"package com.example.demo;

import org.springframework.boot.test.context.TestConfiguration;
import org.springframework.boot.testcontainers.service.connection.ServiceConnection;
import org.springframework.context.annotation.Bean;
import org.testcontainers.postgresql.PostgreSQLContainer;

@TestConfiguration(proxyBeanMethods = false)
public class PostgresContainerConfig {

    @Bean
    @ServiceConnection
    PostgreSQLContainer postgres() {
        return new PostgreSQLContainer("postgres:17-alpine");
    }
}
"#,
    )
    .unwrap();
    fs::write(
        root.join("src/test/java/com/example/demo/DemoApplicationTests.java"),
        r#"package com.example.demo;

import org.junit.jupiter.api.Test;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;

@Import(PostgresContainerConfig.class)
@SpringBootTest
class DemoApplicationTests {

    @Test
    void contextLoads() {}
}
"#,
    )
    .unwrap();
    let api = root.join("src/test/java/com/example/demo/api");
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
            .args(["add", "db"])
            .status()
            .unwrap()
            .success()
    );
    let config = fs::read_to_string(
        root.join("src/test/java/com/example/demo/PostgresContainerConfig.java"),
    )
    .unwrap();
    assert!(config.contains("ApplicationContextInitializer"), "{config}");
    assert!(!config.contains("@ServiceConnection"), "{config}");
    let tests =
        fs::read_to_string(root.join("src/test/java/com/example/demo/DemoApplicationTests.java"))
            .unwrap();
    assert!(!tests.contains("@Import"), "{tests}");
    let factories =
        fs::read_to_string(root.join("src/test/resources/META-INF/spring.factories")).unwrap();
    assert!(
        factories.contains("com.example.demo.PostgresContainerConfig"),
        "{factories}"
    );
}

/// The failure `jails check` actually hits after `add db` on a Spring project:
/// Docker Compose is skipped in tests, so JDBC auto-config has no URL. A
/// test-classpath ApplicationContextInitializer is what makes every
/// `@SpringBootTest` (and therefore `mvn verify`) green.
#[test]
fn add_db_on_spring_makes_context_loads_pass() {
    if !real_mvn_available() {
        eprintln!("skipping: mvn not found on PATH");
        return;
    }
    if !real_java_supports_target_release() {
        eprintln!("skipping: javac on PATH does not support --release {TARGET_RELEASE}");
        return;
    }
    if !real_docker_available() {
        eprintln!("skipping: docker daemon not available");
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
        root.join("src/main/java/com/example/demo/InMemoryThingRepository.java"),
        r#"package com.example.demo;

import org.springframework.stereotype.Repository;

@Repository
public final class InMemoryThingRepository {}
"#,
    )
    .unwrap();

    let status = jails_cmd_with_path(&root, &path)
        .args(["add", "db", "--no-start"])
        .status()
        .unwrap();
    assert!(status.success(), "add db failed");

    let api = root.join("src/test/java/com/example/demo/api");
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
        .arg("test")
        .status()
        .unwrap();
    assert!(
        status.success(),
        "mvn test failed after `jails add db` on a Spring project (every @SpringBootTest needs the initializer)"
    );
}

#[test]
fn add_kafka_stacks_onto_db_compose_and_remove_undoes_one_side() {
    let root = temp_dir("add-kafka-stack");
    write_plain_fixture(&root);
    let fake = temp_dir("add-kafka-bin");
    let log = fake.join("log.txt");
    write_fake_maven(&fake, &["docker"], &log);

    assert!(
        jails_cmd(&root, Some(&fake))
            .args(["add", "db", "kafka"])
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
    write_plain_fixture(&root);
    assert!(
        jails_cmd(&root, None)
            .args(["add", "csv"])
            .status()
            .unwrap()
            .success()
    );
    let reader = root.join("src/main/java/com/example/demo/adapters/CsvReader.java");
    assert!(reader.is_file());

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

#[test]
fn remove_without_force_prompts_and_aborts_on_no() {
    let root = temp_dir("remove-prompt");
    write_plain_fixture(&root);
    assert!(
        jails_cmd(&root, None)
            .args(["add", "csv"])
            .status()
            .unwrap()
            .success()
    );

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
    assert!(
        root.join("src/main/java/com/example/demo/adapters/CsvReader.java")
            .is_file(),
        "aborted remove must leave the files"
    );
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
    let pkg = root.join("src/main/java/com/example/demo");
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
        .args(["add", "db", "json", "testkit"])
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
    let main = root.join("src/main/java/com/example/demo");
    assert!(main.join("adapters/Json.java").is_file());
    let test = root.join("src/test/java/com/example/demo");
    assert!(test.join("testkit/Clocks.java").is_file());
}

/// The real bar for the whole `add` surface: every capability, stacked into
/// one project, compiles and passes its generated tests.
#[test]
fn every_capability_together_produces_a_project_that_compiles_and_passes_tests() {
    if !real_mvn_available() {
        eprintln!("skipping: mvn not found on PATH");
        return;
    }
    if !real_java_supports_target_release() {
        eprintln!("skipping: javac on PATH does not support --release {TARGET_RELEASE}");
        return;
    }
    let path = real_path_without_mvnd();
    let workdir = temp_dir("real-add-all");
    jails_cmd_with_path(&workdir, &path)
        .args(["new-cli", "demo"])
        .status()
        .unwrap();
    let root = workdir.join("demo");

    for capability in ["csv", "sqlite", "json"] {
        let status = jails_cmd_with_path(&root, &path)
            .args(["add", capability])
            .status()
            .unwrap();
        assert!(status.success(), "add {capability} failed");
    }

    let status = jails_cmd_with_path(&root, &path)
        .arg("test")
        .status()
        .unwrap();
    assert!(
        status.success(),
        "mvn test failed after adding csv + sqlite + json"
    );
}

/// The whole toolbox at once: every capability and every generator in one
/// project, then its own suite. This is the only tier that answers "does what
/// jails writes actually compile and pass" for the generated *test* code as
/// well as the main code -- a template that emits an uncompilable assertion
/// looks perfectly fine to every other tier.
#[test]
fn every_generator_and_capability_together_compiles_and_passes_tests() {
    if !real_mvn_available() {
        eprintln!("skipping: mvn not found on PATH");
        return;
    }
    if !real_java_supports_target_release() {
        eprintln!("skipping: javac on PATH does not support --release {TARGET_RELEASE}");
        return;
    }
    let path = real_path_without_mvnd();
    let workdir = temp_dir("real-everything");
    jails_cmd_with_path(&workdir, &path)
        .args(["new-cli", "tool"])
        .status()
        .unwrap();
    let root = workdir.join("tool");

    for capability in ["csv", "sqlite", "json", "testkit", "fake", "http"] {
        let status = jails_cmd_with_path(&root, &path)
            .args(["add", capability])
            .status()
            .unwrap();
        assert!(status.success(), "add {capability} failed");
    }

    for args in [
        vec!["generate", "command", "import"],
        vec![
            "generate",
            "value",
            "money",
            "amount:long",
            "currency:string",
        ],
        vec!["generate", "record", "txn", "id:string", "on:date"],
        // Every component primitive: the compact constructor and its import
        // must both be omitted, or this does not compile.
        vec!["generate", "record", "tally", "hits:int", "total:long"],
    ] {
        let status = jails_cmd_with_path(&root, &path)
            .args(&args)
            .status()
            .unwrap();
        assert!(status.success(), "{args:?} failed");
    }

    fs::write(
        root.join("brief.md"),
        "# Brief\n\n## Acceptance criteria\n\n- parses a `quoted` value\n- rejects **blank** ids\n",
    )
    .unwrap();
    let status = jails_cmd_with_path(&root, &path)
        .args(["generate", "cases", "brief.md"])
        .status()
        .unwrap();
    assert!(status.success(), "generate cases failed");

    let status = jails_cmd_with_path(&root, &path)
        .arg("test")
        .status()
        .unwrap();
    assert!(
        status.success(),
        "the generated project failed its own tests"
    );
}

/// The generators composing: an enum and a record, then a value type that
/// references both by name. Proves the three halves of the field syntax --
/// capitalised = a type this project owns, `!`/`?` optionality, and the
/// enum-aware sample values -- produce a project that actually compiles.
#[test]
fn generators_compose_through_user_owned_field_types() {
    if !real_mvn_available() {
        eprintln!("skipping: mvn not found on PATH");
        return;
    }
    if !real_java_supports_target_release() {
        eprintln!("skipping: javac on PATH does not support --release {TARGET_RELEASE}");
        return;
    }
    let path = real_path_without_mvnd();
    let workdir = temp_dir("real-compose");
    jails_cmd_with_path(&workdir, &path)
        .args(["new-cli", "gym"])
        .status()
        .unwrap();
    let root = workdir.join("gym");

    for args in [
        vec!["generate", "enum", "currency", "GBP", "EUR"],
        vec![
            "generate",
            "record",
            "sourceRef",
            "system:string",
            "externalId:string",
        ],
        vec![
            "generate",
            "value",
            "canonicalTransaction",
            "id:string!",
            "date:date",
            "amountMinor:long",
            "currency:Currency",
            "source:SourceRef",
            "note:string?",
        ],
    ] {
        let status = jails_cmd_with_path(&root, &path)
            .args(&args)
            .status()
            .unwrap();
        assert!(status.success(), "{args:?} failed");
    }

    let value = fs::read_to_string(
        root.join("src/main/java/com/example/gym/domain/CanonicalTransaction.java"),
    )
    .unwrap();
    assert!(
        value.contains("Currency currency"),
        "an owned type is used verbatim: {value}"
    );
    assert!(value.contains("SourceRef source"), "{value}");
    assert!(
        value.contains("long amountMinor"),
        "built-ins stay primitive: {value}"
    );
    assert!(
        value.contains(r#"throw new IllegalArgumentException("id must not be blank")"#),
        "! means non-blank: {value}"
    );
    assert!(
        value.contains("Optional<String> note"),
        "? puts absence in the type: {value}"
    );
    assert!(
        value.contains("requireNonNullElse(note, Optional.empty())"),
        "a null Optional is normalised: {value}"
    );

    // `!` is a text rule; asking for it on a date is a mistake worth naming
    // rather than silently ignoring.
    let output = jails_cmd_with_path(&root, &path)
        .args(["generate", "value", "bad", "when:date!"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("only applies to text"));

    // An enum-typed component can be sampled; one whose constructor jails
    // cannot know must disable the test rather than guess.
    let test = fs::read_to_string(
        root.join("src/test/java/com/example/gym/domain/CanonicalTransactionTest.java"),
    )
    .unwrap();
    assert!(test.contains("Currency.values()[0]"), "{test}");
    assert!(
        test.contains("@Disabled"),
        "an unfabricable component must disable the test: {test}"
    );

    let status = jails_cmd_with_path(&root, &path)
        .arg("test")
        .status()
        .unwrap();
    assert!(
        status.success(),
        "the composed project failed to compile or test"
    );
}

/// The end-to-end path the tool exists for: generate a command, and have it
/// reachable by name with its arguments. Covers three things no unit test can
/// -- that `generate command` really registered itself in the dispatcher, that
/// the project compiles, and that `run --` forwards argv to the program.
#[test]
fn a_generated_command_is_reachable_by_name_through_jails_run() {
    if !real_mvn_available() {
        eprintln!("skipping: mvn not found on PATH");
        return;
    }
    if !real_java_supports_target_release() {
        eprintln!("skipping: javac on PATH does not support --release {TARGET_RELEASE}");
        return;
    }
    let path = real_path_without_mvnd();
    let workdir = temp_dir("real-run-args");
    jails_cmd_with_path(&workdir, &path)
        .args(["new-cli", "tool"])
        .status()
        .unwrap();
    let root = workdir.join("tool");

    let status = jails_cmd_with_path(&root, &path)
        .args(["generate", "command", "greet"])
        .status()
        .unwrap();
    assert!(status.success());

    let output = jails_cmd_with_path(&root, &path)
        .args(["run", "--", "greet", "world"])
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
    let output = jails_cmd_with_path(&root, &path)
        .arg("run")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("greet"),
        "help should list the registered command: {stdout}"
    );
}

/// `add format` installs a formatter that checks the build. If jails' own
/// output does not already satisfy it, a freshly generated project fails
/// `jails check` on the first run -- a bad first impression, and the reason
/// import order is normalised at write time.
#[test]
fn a_freshly_generated_project_passes_check_with_no_manual_formatting() {
    if !real_mvn_available() {
        eprintln!("skipping: mvn not found on PATH");
        return;
    }
    if !real_java_supports_target_release() {
        eprintln!("skipping: javac on PATH does not support --release {TARGET_RELEASE}");
        return;
    }
    let path = real_path_without_mvnd();
    let workdir = temp_dir("real-check-clean");
    jails_cmd_with_path(&workdir, &path)
        .args(["new-cli", "tool"])
        .status()
        .unwrap();
    let root = workdir.join("tool");

    for args in [
        vec!["generate", "command", "import"],
        vec![
            "generate",
            "value",
            "money",
            "amount:long",
            "currency:string",
        ],
        vec!["add", "testkit"],
    ] {
        let status = jails_cmd_with_path(&root, &path)
            .args(&args)
            .status()
            .unwrap();
        assert!(status.success(), "{args:?} failed");
    }

    // `add format` is allowed to refuse: palantir-java-format cannot always run
    // on the JDK that happens to be on PATH. What it is *not* allowed to do is
    // leave a project that no longer builds -- so `check` must pass either way.
    let formatted = jails_cmd_with_path(&root, &path)
        .args(["add", "format"])
        .status()
        .unwrap()
        .success();

    let status = jails_cmd_with_path(&root, &path)
        .arg("check")
        .status()
        .unwrap();
    assert!(
        status.success(),
        "`jails check` failed on a freshly generated project (formatter installed: {formatted})"
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
        eprintln!("skipping: mvn not found on PATH");
        return;
    }
    if !real_java_supports_target_release() {
        eprintln!("skipping: javac on PATH does not support --release {TARGET_RELEASE}");
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

    let status = jails_cmd_with_path(&root, &path)
        .arg("test")
        .status()
        .unwrap();
    assert!(
        status.success(),
        "mvn test failed after `jails add json` on a Spring project"
    );
}

// ---- observation and refactoring commands (doctor / routes / beans /
// rename / why). All offline: they read source and configuration, never
// Maven. ----

/// A minimal Spring-shaped project: an application class, a controller, a
/// service, and a repository interface with an implementation. Enough for
/// `routes`, `beans` and `rename` to have something real to say.
fn write_inspectable_project(root: &Path) {
    fs::write(
        root.join("pom.xml"),
        "<project><parent><groupId>org.springframework.boot</groupId>\
         <artifactId>spring-boot-starter-parent</artifactId></parent>\
         <artifactId>shop</artifactId>\
         <properties><maven.compiler.release>27</maven.compiler.release></properties></project>",
    )
    .unwrap();
    let main = root.join("src/main/java/dev/example/shop");
    fs::create_dir_all(main.join("api")).unwrap();
    fs::create_dir_all(main.join("domain")).unwrap();
    fs::write(
        main.join("ShopApplication.java"),
        "package dev.example.shop;\npublic class ShopApplication {}\n",
    )
    .unwrap();
    fs::write(
        main.join("api/OrderController.java"),
        "package dev.example.shop.api;\n\
         @RestController\n\
         @RequestMapping(\"/orders\")\n\
         public final class OrderController {\n\
         \x20   public OrderController(OrderService service) {}\n\
         \x20   @GetMapping(\"/{id}\")\n\
         \x20   public Order byId(String id) { return null; }\n\
         \x20   @PostMapping\n\
         \x20   public Order create(Order order) { return null; }\n\
         }\n",
    )
    .unwrap();
    fs::write(
        main.join("domain/Order.java"),
        "package dev.example.shop.domain;\npublic record Order(String id) {}\n",
    )
    .unwrap();
    fs::write(
        main.join("domain/OrderService.java"),
        "package dev.example.shop.domain;\n\
         @Service\n\
         public final class OrderService {\n\
         \x20   public OrderService(Order seed) {}\n\
         }\n",
    )
    .unwrap();
}

#[test]
fn routes_lists_mappings_with_the_type_level_prefix_applied() {
    let root = temp_dir("routes");
    write_inspectable_project(&root);

    let output = jails_cmd(&root, None).arg("routes").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("/orders/{id}"), "{stdout}");
    assert!(stdout.contains("OrderController#byId"), "{stdout}");
    assert!(stdout.contains("POST"), "{stdout}");
    assert!(stdout.contains("2 route(s)"), "{stdout}");
}

#[test]
fn routes_json_is_machine_readable() {
    let root = temp_dir("routes-json");
    write_inspectable_project(&root);

    let output = jails_cmd(&root, None)
        .args(["routes", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with(r#"{"version":1,"routes":["#), "{stdout}");
    assert!(stdout.contains(r#""verb":"GET""#), "{stdout}");
}

#[test]
fn beans_reports_a_dependency_no_bean_supplies() {
    let root = temp_dir("beans");
    write_inspectable_project(&root);

    let output = jails_cmd(&root, None).arg("beans").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("@RestController"), "{stdout}");
    assert!(stdout.contains("OrderService"), "{stdout}");
    // Order is a project type with no stereotype, so OrderService's
    // dependency on it cannot be satisfied -- exactly the static half of
    // "required a bean of type ... that could not be found".
    assert!(stdout.contains("NO BEAN"), "{stdout}");
}

#[test]
fn beans_filters_on_a_pattern() {
    let root = temp_dir("beans-filter");
    write_inspectable_project(&root);

    let output = jails_cmd(&root, None)
        .args(["beans", "controller"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("OrderController"), "{stdout}");
    assert!(!stdout.contains("@Service"), "{stdout}");
}

#[test]
fn doctor_reports_a_jdk_older_than_the_target_release() {
    let root = temp_dir("doctor-jdk");
    write_inspectable_project(&root);

    let output = jails_cmd(&root, None).arg("doctor").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    // The project targets release 27 and declares no compose services, so
    // the report must at least name the project and reach a verdict.
    assert!(stdout.contains("project"), "{stdout}");
    assert!(stdout.contains("checks"), "{stdout}");
    // Every failing line has to carry a fix; a diagnosis without an action
    // has only moved the work.
    for line in stdout.lines().filter(|l| l.starts_with("FAIL")) {
        let title = line.split_whitespace().nth(1).unwrap_or_default();
        assert!(
            stdout.contains("fix:"),
            "FAIL line for {title} carries no fix: {stdout}"
        );
    }
}

#[test]
fn doctor_exits_non_zero_when_a_check_fails() {
    let root = temp_dir("doctor-exit");
    // No pom.xml at all below this directory is not the case under test --
    // an empty project *with* a pom is: it has no src/main/java.
    fs::write(root.join("pom.xml"), "<project><artifactId>x</artifactId></project>").unwrap();

    let output = jails_cmd(&root, None).arg("doctor").output().unwrap();
    assert!(!output.status.success(), "doctor should fail on a broken project");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("FAIL"), "{stdout}");
    // The report is the message; a redundant `jails: ` line under it is not.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("jails: "), "{stderr}");
}

#[test]
fn why_explains_a_missing_bean_read_from_stdin() {
    let root = temp_dir("why-bean");
    write_inspectable_project(&root);

    let mut child = jails_cmd(&root, None)
        .arg("why")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(
            b"Parameter 0 of constructor in dev.example.shop.api.OrderController required a bean \
              of type 'dev.example.shop.domain.OrderRepository' that could not be found.",
        )
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("OrderRepository"), "{stdout}");
    assert!(stdout.contains("jails beans"), "{stdout}");
}

#[test]
fn why_reads_a_log_file_and_says_so_when_it_does_not_recognise_one() {
    let root = temp_dir("why-file");
    write_inspectable_project(&root);
    let log = root.join("failure.log");
    fs::write(&log, "something entirely novel went wrong").unwrap();

    let output = jails_cmd(&root, None)
        .arg("why")
        .arg(&log)
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("does not recognise"), "{stdout}");
    assert!(stdout.contains("jails doctor"), "{stdout}");
}

#[test]
fn rename_moves_the_type_its_companion_and_every_reference() {
    let root = temp_dir("rename");
    write_inspectable_project(&root);
    let tests = root.join("src/test/java/dev/example/shop/domain");
    fs::create_dir_all(&tests).unwrap();
    fs::write(
        tests.join("OrderTest.java"),
        "package dev.example.shop.domain;\n\
         class OrderTest {\n\
         \x20   void works() { var o = new Order(\"Order lookup failed\"); }\n\
         }\n",
    )
    .unwrap();

    let output = jails_cmd(&root, None)
        .args(["rename", "Order", "Purchase", "--force"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);

    let domain = root.join("src/main/java/dev/example/shop/domain");
    assert!(domain.join("Purchase.java").is_file());
    assert!(!domain.join("Order.java").exists());
    assert!(tests.join("PurchaseTest.java").is_file());
    assert!(!tests.join("OrderTest.java").exists());

    // A reference in another file follows...
    let service = fs::read_to_string(domain.join("OrderService.java")).unwrap();
    assert!(service.contains("Purchase seed"), "{service}");
    // ...but a type that merely starts with the same letters does not.
    assert!(domain.join("OrderService.java").is_file());
    // ...and neither does a string literal.
    let renamed_test = fs::read_to_string(tests.join("PurchaseTest.java")).unwrap();
    assert!(renamed_test.contains("new Purchase("), "{renamed_test}");
    assert!(
        renamed_test.contains("\"Order lookup failed\""),
        "{renamed_test}"
    );
}

#[test]
fn rename_refuses_a_package_qualified_name() {
    let root = temp_dir("rename-qualified");
    write_inspectable_project(&root);

    let output = jails_cmd(&root, None)
        .args(["rename", "dev.example.shop.domain.Order", "Purchase", "--force"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("simple name"), "{stderr}");
}

#[test]
fn rename_dry_run_writes_nothing() {
    let root = temp_dir("rename-dry");
    write_inspectable_project(&root);

    let output = jails_cmd(&root, None)
        .args(["rename", "Order", "Purchase", "--dry-run"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("nothing was written"), "{stdout}");
    assert!(
        root.join("src/main/java/dev/example/shop/domain/Order.java")
            .is_file()
    );
}

#[test]
fn a_scaffold_with_database_types_compiles_including_its_derived_jdbc_adapter() {
    // The tier that answers the question the tool exists for. A unit test on
    // the SQL mapping cannot catch a generated expression that is merely
    // *nearly* right: `Timestamp.from(x.createdAt())` has the receiver in
    // the middle, so gluing the receiver on the front yields
    // `x.Timestamp.from(createdAt())` -- which reads fine and does not
    // compile. Only javac finds that.
    if !real_mvn_available() {
        eprintln!("skipping: mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-scaffold-jdbc");
    write_spring_fixture(&root);

    // The enum has to exist before the record names it, or the mapping falls
    // back to "unmappable" and the interesting branch never runs.
    let status = jails_cmd_with_path(&root, &path)
        .args(["generate", "enum", "Currency", "GBP", "USD"])
        .status()
        .unwrap();
    assert!(status.success());

    let status = jails_cmd_with_path(&root, &path)
        .args([
            "generate",
            "scaffold",
            "Payout",
            "id:uuid",
            "amount:bigdecimal",
            "currency:Currency",
            "paidAt:instant",
            "note:string?",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let adapter = fs::read_to_string(
        root.join("src/main/java/com/example/demo/adapters/JdbcPayoutRepository.java"),
    )
    .unwrap();
    // Derived, not left as a TODO.
    assert!(!adapter.contains("UnsupportedOperationException"), "{adapter}");
    assert!(adapter.contains("Timestamp.from(payout.paidAt())"), "{adapter}");
    assert!(adapter.contains("Currency.valueOf(rows.getString(\"currency\"))"), "{adapter}");
    // An Optional component is unwrapped on the way out and rebuilt on the way in.
    assert!(adapter.contains("Optional.ofNullable(rows.getString(\"note\"))"), "{adapter}");
    assert!(adapter.contains("payout.note().orElse(null)"), "{adapter}");
    // The column list is shared by the select and the insert, so they agree.
    assert!(adapter.contains("insert into payouts (id, amount, currency, paid_at, note)"), "{adapter}");

    let status = jails_cmd_with_path(&root, &path)
        .arg("test")
        .status()
        .unwrap();
    assert!(
        status.success(),
        "mvn test failed for a scaffold whose JDBC adapter jails derived"
    );
}

#[test]
fn a_scaffold_emits_a_migration_whose_columns_match_the_adapter() {
    let root = temp_dir("scaffold-migration");
    write_spring_fixture(&root);
    // `add db` is what creates this directory; jails emits a migration only
    // when the project has somewhere to put one.
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();

    let output = jails_cmd(&root, None)
        .args([
            "generate",
            "scaffold",
            "Payout",
            "id:uuid",
            "amount:bigdecimal",
            "paidAt:instant",
            "note:string?",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");

    let migration = fs::read_to_string(
        root.join("src/main/resources/db/migration/V001__create_payouts.sql"),
    )
    .unwrap();
    assert!(migration.contains("create table payouts ("), "{migration}");
    assert!(migration.contains("uuid") && migration.contains("numeric"), "{migration}");
    // An Instant needs a zone-aware column or it comes back reinterpreted.
    assert!(migration.contains("timestamptz not null"), "{migration}");
    // The nullable component is the only one without `not null`.
    assert!(migration.contains("text,"), "{migration}");
    assert_eq!(
        migration.matches("not null").count(),
        3,
        "only the nullable component may lack `not null`: {migration}"
    );
    assert!(migration.contains("primary key (id)"), "{migration}");

    // The same column names the adapter selects and inserts.
    let adapter = fs::read_to_string(
        root.join("src/main/java/com/example/demo/adapters/JdbcPayoutRepository.java"),
    )
    .unwrap();
    for column in ["id", "amount", "paid_at", "note"] {
        assert!(migration.contains(column), "migration missing {column}");
        assert!(adapter.contains(column), "adapter missing {column}");
    }
}

#[test]
fn a_project_without_a_migration_directory_gets_no_migration() {
    let root = temp_dir("scaffold-no-migration");
    write_spring_fixture(&root);

    let output = jails_cmd(&root, None)
        .args(["generate", "scaffold", "Payout", "id:uuid"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(!root.join("src/main/resources/db/migration").exists());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("created migration"), "{stdout}");
}
