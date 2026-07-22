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
    let output = jails_cmd(&workdir, None).args(["completion", "bash"]).output().unwrap();
    assert!(output.status.success());
    let script = String::from_utf8_lossy(&output.stdout);
    assert!(script.contains("_jails()"));
    assert!(script.contains("complete -F _jails"));
}

/// Regression test: `kind` used to be a plain String, so `jails generate
/// <TAB>` had nothing to offer but filenames, and the `g`/`d` aliases were
/// declared with `alias` (hidden from clap_complete) instead of
/// `visible_alias`, so `jails g <TAB>` fell back to top-level subcommand
/// names instead of the artifact-kind list.
#[test]
fn completion_offers_artifact_kinds_for_generate_destroy_and_their_aliases() {
    let workdir = temp_dir("completion-kinds");
    let output = jails_cmd(&workdir, None).args(["completion", "bash"]).output().unwrap();
    let script = String::from_utf8_lossy(&output.stdout);

    let generate_opts = opts_line_for(&script, "jails__subcmd__generate)");
    let destroy_opts = opts_line_for(&script, "jails__subcmd__destroy)");
    for kind in ["scaffold", "controller", "service", "repository", "entity", "test"] {
        assert!(generate_opts.contains(kind), "expected generate's opts ({generate_opts:?}) to include {kind}");
        assert!(destroy_opts.contains(kind), "expected destroy's opts ({destroy_opts:?}) to include {kind}");
    }

    assert!(script.contains("jails,g)"), "expected the `g` alias to transition completion state");
    assert!(script.contains("jails,d)"), "expected the `d` alias to transition completion state");
}

/// Pulls the `opts="..."` line right after a `<marker>)` case arm.
fn opts_line_for<'a>(script: &'a str, marker: &str) -> &'a str {
    let start = script.find(marker).unwrap_or_else(|| panic!("marker {marker} not found in completion script"));
    script[start..].lines().find(|l| l.trim_start().starts_with("opts=")).unwrap()
}

#[test]
fn new_cli_creates_expected_project_layout() {
    let workdir = temp_dir("new-cli-layout");
    let status = jails_cmd(&workdir, None).args(["new-cli", "demo"]).status().unwrap();
    assert!(status.success());

    let root = workdir.join("demo");
    assert!(root.join("pom.xml").is_file());
    assert!(root.join("src/main/java/com/example/demo/App.java").is_file());
    assert!(root.join("src/test/java/com/example/demo/AppTest.java").is_file());
}

#[test]
fn new_cli_fails_if_the_directory_already_exists() {
    let workdir = temp_dir("new-cli-exists");
    fs::create_dir(workdir.join("demo")).unwrap();
    let output = jails_cmd(&workdir, None).args(["new-cli", "demo"]).output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("already exists"));
}

#[test]
fn generate_standalone_and_destroy_roundtrip() {
    let root = temp_dir("standalone-roundtrip");
    write_project_skeleton(&root);

    let status = jails_cmd(&root, None).args(["generate", "controller", "comment"]).status().unwrap();
    assert!(status.success());
    let file = root.join("src/main/java/com/example/demo/CommentController.java");
    assert!(file.is_file());
    let contents = fs::read_to_string(&file).unwrap();
    assert!(contents.contains("public class CommentController"));
    // Rails generates a test alongside `generate controller`; jails matches that.
    let test_file = root.join("src/test/java/com/example/demo/CommentControllerTest.java");
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
fn generate_scaffold_writes_all_five_files() {
    let root = temp_dir("scaffold-files");
    write_project_skeleton(&root);

    let status = jails_cmd(&root, None)
        .args(["generate", "scaffold", "Post", "title:string", "body:text"])
        .status()
        .unwrap();
    assert!(status.success());

    let pkg = root.join("src/main/java/com/example/demo");
    assert!(pkg.join("Post.java").is_file());
    assert!(pkg.join("PostRepository.java").is_file());
    assert!(pkg.join("PostService.java").is_file());
    assert!(pkg.join("PostController.java").is_file());
    assert!(root.join("src/test/java/com/example/demo/PostControllerTest.java").is_file());
}

#[test]
fn destroy_without_force_prompts_and_aborts_on_no() {
    let root = temp_dir("destroy-prompt");
    write_project_skeleton(&root);
    jails_cmd(&root, None).args(["generate", "controller", "comment"]).status().unwrap();
    let file = root.join("src/main/java/com/example/demo/CommentController.java");
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
    assert!(file.is_file(), "file should survive a declined confirmation");
}

#[test]
fn generate_errors_on_duplicate_file() {
    let root = temp_dir("duplicate");
    write_project_skeleton(&root);
    jails_cmd(&root, None).args(["generate", "service", "comment"]).status().unwrap();
    let output = jails_cmd(&root, None).args(["generate", "service", "comment"]).output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("already exists"));
}

#[test]
fn generate_errors_on_unknown_field_type() {
    let root = temp_dir("unknown-field-type");
    write_project_skeleton(&root);
    let output = jails_cmd(&root, None)
        .args(["generate", "entity", "widget", "id:uuid"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown field type"));
}

#[test]
fn generate_errors_outside_a_project() {
    let root = temp_dir("no-project");
    let output = jails_cmd(&root, None).args(["generate", "entity", "widget"]).output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("pom.xml"));
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

// ---- mocked mvn/mvnd: verify jails' own command-construction logic
// (which binary, which args) without needing real Maven. ----

#[test]
fn test_command_prefers_mvnd_when_present_and_passes_the_filter() {
    let root = temp_dir("mock-test-mvnd");
    write_project_skeleton(&root);
    let fake_dir = temp_dir("mock-test-mvnd-bin");
    let log = fake_dir.join("log.txt");
    write_fake_maven(&fake_dir, &["mvn", "mvnd"], &log);

    let status = jails_cmd(&root, Some(&fake_dir)).args(["test", "PostTest"]).status().unwrap();
    assert!(status.success());

    let invocation = read_log(&log);
    assert!(invocation.contains("/mvnd "), "expected mvnd to be preferred: {invocation}");
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

    let status = jails_cmd(&root, Some(&fake_dir)).args(["test"]).status().unwrap();
    assert!(status.success());
    assert!(read_log(&log).contains("/mvn "));
}

#[test]
fn build_command_invokes_mvn_package() {
    let root = temp_dir("mock-build");
    write_project_skeleton(&root);
    let fake_dir = temp_dir("mock-build-bin");
    let log = fake_dir.join("log.txt");
    write_fake_maven(&fake_dir, &["mvn"], &log);

    let status = jails_cmd(&root, Some(&fake_dir)).arg("build").status().unwrap();
    assert!(status.success());
    assert!(read_log(&log).contains("package"));
}

#[test]
fn run_command_uses_spring_boot_run_for_spring_projects() {
    let root = temp_dir("mock-run-spring");
    let pkg_dir = root.join("src/main/java/com/example/demo");
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::write(root.join("pom.xml"), "<project>org.springframework.boot</project>").unwrap();
    let fake_dir = temp_dir("mock-run-spring-bin");
    let log = fake_dir.join("log.txt");
    write_fake_maven(&fake_dir, &["mvn"], &log);

    let status = jails_cmd(&root, Some(&fake_dir)).arg("run").status().unwrap();
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
    jails_cmd(&root, Some(&fake_dir)).arg("run").status().unwrap();
    assert!(read_log(&log).contains("compile"));
}

#[test]
fn run_no_build_skips_mvn_and_runs_an_existing_jar_for_spring_projects() {
    let root = temp_dir("no-build-spring");
    fs::create_dir_all(root.join("target")).unwrap();
    fs::write(root.join("pom.xml"), "<project>org.springframework.boot</project>").unwrap();
    fs::write(root.join("target/demo.jar"), "not a real jar, just needs to exist").unwrap();

    let fake_dir = temp_dir("no-build-spring-bin");
    let log = fake_dir.join("log.txt");
    write_fake_maven(&fake_dir, &["mvn", "java"], &log);

    let status = jails_cmd(&root, Some(&fake_dir)).args(["run", "--no-build"]).status().unwrap();
    assert!(status.success());

    let invocation = read_log(&log);
    assert!(invocation.contains("/java "), "expected java to run: {invocation}");
    assert!(invocation.contains("-jar"));
    assert!(invocation.contains("demo.jar"));
    assert!(!invocation.contains("/mvn "), "mvn should never run with --no-build: {invocation}");
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

    let output = jails_cmd(&root, None).args(["run", "--no-build"]).output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("jails build"), "expected a hint to build first: {stderr}");
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
    let status = std::process::Command::new("javac").arg("-d").arg(&classes).arg(&source).status().unwrap();
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
    let output = jails_cmd_with_path(&root, &path).args(["run", "--no-build"]).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("no-build-ran"));
}

// ---- real Maven + JDK 26, no network beyond Maven Central artifact
// resolution (never start.spring.io): verify the actual bar from
// prompt.md -- "does new-cli produce a project that passes mvn test?" and
// "does generate scaffold produce a project that compiles?". Skipped
// automatically if mvn isn't on PATH. ----

#[test]
fn new_cli_project_passes_real_mvn_test() {
    if !real_mvn_available() {
        eprintln!("skipping: mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let workdir = temp_dir("real-new-cli-test");
    jails_cmd_with_path(&workdir, &path).args(["new-cli", "demo"]).status().unwrap();
    let root = workdir.join("demo");

    let status = jails_cmd_with_path(&root, &path).arg("test").status().unwrap();
    assert!(status.success(), "mvn test failed for a freshly generated new-cli project");
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
        .args(["generate", "scaffold", "Post", "title:string", "body:text", "published:boolean"])
        .status()
        .unwrap();
    assert!(status.success());

    let status = jails_cmd_with_path(&root, &path).arg("test").status().unwrap();
    assert!(status.success(), "mvn test failed for a freshly scaffolded Post resource");
}

/// Regression coverage for the reported bug (standalone `generate
/// controller` not producing a test) plus real-compile verification of the
/// new controller/service/entity companion test templates.
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
        vec!["generate", "entity", "Tag", "name:string", "createdAt:datetime"],
    ] {
        let status = jails_cmd_with_path(&root, &path).args(&args).status().unwrap();
        assert!(status.success(), "{args:?} failed");
    }

    let status = jails_cmd_with_path(&root, &path).arg("test").status().unwrap();
    assert!(status.success(), "mvn test failed for the standalone-generated companion tests");
}
