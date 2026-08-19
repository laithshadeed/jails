mod common;

use common::*;
use std::fs;
use std::io::Write as _;
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
    assert!(read_log(&wrapper_log).contains("verify"));
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
    for capability in ["db", "csv", "sqlite", "json"] {
        assert!(
            script.contains(capability),
            "completion script never mentions the {capability} capability"
        );
    }
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

    let output = jails_cmd(&root, None).args(["add", "db"]).output().unwrap();
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
        root.join("src/main/resources/db/migration/.gitkeep")
            .is_file()
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

    let output = jails_cmd(&root, None)
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
