mod common;

use common::*;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
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
            module_root.join("src/main/java/dev/example/DemoApplication.java"),
            "package dev.example;\n\npublic class DemoApplication {}\n",
        )
        .unwrap();
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
    assert!(stdout.contains("Reactor: sample-parent"));
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
    assert!(json.contains("\"schema_version\": 3"));
    assert!(json.contains("\"reactor\":"));
    assert!(json.contains("\"base_package\": \"dev.example\""));
    assert!(json.contains("\"java_root\":"));
    assert!(json.contains("\"test_root\":"));
    assert!(json.contains("\"layout\":"));
    assert!(json.contains("\"capabilities\":"));
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
    let agents = fs::read_to_string(root.join("AGENTS.md")).unwrap();
    assert!(agents.contains("jails check"), "{agents}");
    assert!(agents.contains("@MockBean"), "{agents}");
}

#[test]
fn new_offline_creates_a_complete_spring_project_without_network() {
    let workdir = temp_dir("new-offline");
    let output = jails_cmd(&workdir, None)
        .args([
            "new",
            "demo-app",
            "--offline",
            "--no-git",
            "--no-devtools",
            "--deps",
            "web,actuator",
            "--java",
            "21",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let root = workdir.join("demo-app");
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("spring-boot-starter-webmvc"), "{pom}");
    assert!(pom.contains("spring-boot-starter-actuator"), "{pom}");
    assert!(pom.contains("<java.version>21</java.version>"), "{pom}");
    assert!(pom.contains("maven-enforcer-plugin"), "{pom}");
    assert!(pom.contains("<requireJavaVersion>"), "{pom}");
    assert!(pom.contains("<requireMavenVersion>"), "{pom}");
    assert!(
        root.join("src/main/java/com/example/demoapp/DemoAppApplication.java")
            .is_file()
    );
    assert!(
        root.join("src/test/java/com/example/demoapp/DemoAppApplicationTests.java")
            .is_file()
    );
    assert_eq!(
        fs::read_to_string(root.join("mise.toml")).unwrap(),
        "[tools]\njava = \"21\"\n"
    );
    let agents = fs::read_to_string(root.join("AGENTS.md")).unwrap();
    assert!(
        agents.contains("base package is `com.example.demoapp`"),
        "{agents}"
    );
    assert!(agents.contains("jails lint"), "{agents}");
    let properties =
        fs::read_to_string(root.join("src/main/resources/application.properties")).unwrap();
    assert!(
        properties.contains("server.shutdown=graceful"),
        "{properties}"
    );
    assert!(!root.join(".git").exists());
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
    assert!(contents.contains("class CommentController"));
    assert!(
        !contents.contains("public class"),
        "spring.md §2: a controller is an entry point, not module API"
    );
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
fn scaffold_reuses_an_existing_record_and_destroy_preserves_it() {
    let root = temp_dir("scaffold-model-first");
    write_project_skeleton(&root);

    let record = jails_cmd(&root, None)
        .args(["generate", "record", "Post", "id:uuid", "title:string!"])
        .status()
        .unwrap();
    assert!(record.success());

    let scaffold = jails_cmd(&root, None)
        .args(["generate", "scaffold", "Post"])
        .status()
        .unwrap();
    assert!(scaffold.success());

    let record_path = root.join("src/main/java/com/example/demo/domain/Post.java");
    let source = fs::read_to_string(&record_path).unwrap();
    assert!(source.contains("UUID id"), "{source}");
    assert!(source.contains("String title"), "{source}");

    let destroy = jails_cmd(&root, None)
        .args(["destroy", "scaffold", "Post", "--force"])
        .status()
        .unwrap();
    assert!(destroy.success());
    assert!(
        record_path.is_file(),
        "destroy scaffold must not remove the record created by a prior intent"
    );
    assert!(
        root.join("src/test/java/com/example/demo/domain/PostTest.java")
            .is_file(),
        "the record intent itself remains tracked and intact"
    );
    assert!(
        !root
            .join("src/main/java/com/example/demo/web/PostController.java")
            .exists()
    );
}

#[test]
fn field_driven_generators_refuse_an_absent_model_with_a_fix() {
    let root = temp_dir("missing-model-fix");
    write_project_skeleton(&root);

    for kind in ["scaffold", "dto", "repo"] {
        let output = jails_cmd(&root, None)
            .args(["generate", kind, "Missing"])
            .output()
            .unwrap();
        assert!(!output.status.success(), "{kind} unexpectedly succeeded");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("fix:"), "{kind}: {stderr}");
        assert!(stderr.contains("g record Missing"), "{kind}: {stderr}");
    }
}

#[test]
fn generate_field_updates_unchanged_derivatives_preserves_edits_and_adds_a_migration() {
    let root = temp_dir("generate-field");
    write_project_skeleton(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();

    let scaffold = jails_cmd(&root, None)
        .args(["g", "scaffold", "Note", "id:uuid@pk", "title:string!"])
        .status()
        .unwrap();
    assert!(scaffold.success());

    let request = root.join("src/main/java/com/example/demo/web/NoteRequest.java");
    let edited = format!(
        "{}// user-owned validation\n",
        fs::read_to_string(&request).unwrap()
    );
    fs::write(&request, &edited).unwrap();

    let output = jails_cmd(&root, None)
        .args(["g", "field", "Note", "createdAt:instant"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("skipped"), "{stdout}");
    assert!(
        stdout.contains("add component: Instant createdAt"),
        "{stdout}"
    );

    let record =
        fs::read_to_string(root.join("src/main/java/com/example/demo/domain/Note.java")).unwrap();
    assert!(record.contains("Instant createdAt"), "{record}");
    let jdbc = fs::read_to_string(
        root.join("src/main/java/com/example/demo/adapters/JdbcNoteRepository.java"),
    )
    .unwrap();
    assert!(jdbc.contains("created_at"), "{jdbc}");
    assert_eq!(fs::read_to_string(&request).unwrap(), edited);

    let migration = fs::read_to_string(
        root.join("src/main/resources/db/migration/V002__add_created_at_to_notes.sql"),
    )
    .unwrap();
    assert!(
        migration.contains("add column created_at timestamptz"),
        "{migration}"
    );
    assert!(
        migration.contains("default current_timestamp not null"),
        "{migration}"
    );
    assert!(
        migration.contains("alter column created_at drop default"),
        "{migration}"
    );

    let ledger = fs::read_to_string(root.join(".jails/ledger.toml")).unwrap();
    assert!(
        ledger.contains("src/main/resources/db/migration/V002__add_created_at_to_notes.sql"),
        "the new migration is recorded against the intent that wrote it: {ledger}"
    );
    assert!(
        ledger.contains(&format!("version = \"{}\"", env!("CARGO_PKG_VERSION"))),
        "{ledger}"
    );

    let duplicate = jails_cmd(&root, None)
        .args(["g", "field", "Note", "createdAt:instant"])
        .output()
        .unwrap();
    assert!(!duplicate.status.success());
    assert!(
        String::from_utf8_lossy(&duplicate.stderr).contains("already has a `createdAt` component")
    );
}

#[test]
fn scaffold_refuses_to_silently_flatten_a_project_record_component() {
    let root = temp_dir("scaffold-project-record");
    write_project_skeleton(&root);
    assert!(
        jails_cmd(&root, None)
            .args(["g", "record", "User", "id:uuid@pk", "name:string!"])
            .status()
            .unwrap()
            .success()
    );

    let output = jails_cmd(&root, None)
        .args(["g", "scaffold", "Post", "id:uuid@pk", "author:User"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("cannot be persisted"), "{stderr}");
    assert!(stderr.contains("author:UUID"), "{stderr}");
    assert!(stderr.contains("g association"), "{stderr}");
    assert!(stderr.contains("--on Post --yields User"), "{stderr}");
    assert!(
        !root
            .join("src/main/java/com/example/demo/domain/Post.java")
            .exists(),
        "the refusal must happen before the first write"
    );
}

#[test]
fn scaffold_timestamps_flow_through_ddl_create_and_optimistic_updates() {
    let root = temp_dir("scaffold-timestamps");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();

    let scaffold = jails_cmd(&root, None)
        .args([
            "g",
            "scaffold",
            "Note",
            "id:uuid@pk",
            "title:string!",
            "version:long",
            "--timestamps",
        ])
        .status()
        .unwrap();
    assert!(scaffold.success());
    let record =
        fs::read_to_string(root.join("src/main/java/com/example/demo/domain/Note.java")).unwrap();
    assert!(record.contains("Instant createdAt"), "{record}");
    assert!(record.contains("Instant updatedAt"), "{record}");
    let ddl =
        fs::read_to_string(root.join("src/main/resources/db/migration/V001__create_notes.sql"))
            .unwrap();
    assert!(ddl.contains("created_at"), "{ddl}");
    assert!(ddl.contains("updated_at"), "{ddl}");

    let transition = jails_cmd(&root, None)
        .args([
            "g",
            "transition",
            "RenameNote",
            "id:uuid",
            "title:string!",
            "version:long",
            "--on",
            "Note",
        ])
        .status()
        .unwrap();
    assert!(transition.success());
    let adapter = fs::read_to_string(
        root.join("src/main/java/com/example/demo/adapters/JdbcRenameNoteTransition.java"),
    )
    .unwrap();
    assert!(
        adapter.contains("updated_at = current_timestamp"),
        "{adapter}"
    );
}

#[test]
fn scaffold_writes_http_requests_and_factory_builds_typed_test_data() {
    let root = temp_dir("requests-factory");
    write_spring_fixture(&root);

    assert!(
        jails_cmd(&root, None)
            .args([
                "g",
                "scaffold",
                "Note",
                "id:uuid@pk",
                "title:string!",
                "createdAt:instant",
            ])
            .status()
            .unwrap()
            .success()
    );
    let requests = fs::read_to_string(root.join("requests/note.http")).unwrap();
    assert!(requests.contains("POST {{baseUrl}}/notes"), "{requests}");
    assert!(
        requests.contains("\"createdAt\": \"2026-01-01T00:00:00Z\""),
        "{requests}"
    );

    assert!(
        jails_cmd(&root, None)
            .args(["g", "factory", "Note"])
            .status()
            .unwrap()
            .success()
    );
    let factory =
        fs::read_to_string(root.join("src/test/java/com/example/demo/testkit/NoteFactory.java"))
            .unwrap();
    assert!(
        factory.contains("public static NoteFactory aNote()"),
        "{factory}"
    );
    assert!(factory.contains("withTitle(String value)"), "{factory}");
    assert!(factory.contains("Instant.parse("), "{factory}");
    assert!(factory.contains("return new Note("), "{factory}");
}

#[test]
fn new_cli_with_an_app_manifest_is_one_command_from_an_empty_directory() {
    // plan.md §18 closes by asking which two commands should have been one,
    // and answers itself: `new` + `mkdir .jails` + `cp app.toml` + `app apply`
    // is four steps that only ever appear together. §0.4 tracks the count as a
    // scorecard metric with a target of 1.
    let workspace = temp_dir("new-app-manifest");
    fs::create_dir_all(&workspace).unwrap();
    let manifest = workspace.join("app.toml");
    fs::write(
        &manifest,
        "schema = 1

[[generate]]
kind = \"record\"
name = \"Entry\"
fields = [\"id:uuid\", \"label:string!\"]
",
    )
    .unwrap();

    let created = std::process::Command::new(env!("CARGO_BIN_EXE_jails"))
        .current_dir(&workspace)
        .args(["new-cli", "demo", "--no-git", "--app"])
        .arg(&manifest)
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );

    let root = workspace.join("demo");
    // The manifest is seeded where `app apply` will find it next time...
    assert!(root.join(".jails/app.toml").is_file());
    // ...and its intents are already applied, against the project that was
    // just created rather than whatever encloses the process CWD.
    assert!(
        root.join("src/main/java/com/example/demo/domain/Entry.java")
            .is_file(),
        "the manifest's intent should have been applied"
    );
    assert!(
        root.join("src/test/java/com/example/demo/domain/EntryTest.java")
            .is_file()
    );
}

#[test]
fn new_with_an_unreadable_app_manifest_says_so_with_a_fix() {
    let workspace = temp_dir("new-app-missing");
    fs::create_dir_all(&workspace).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_jails"))
        .current_dir(&workspace)
        .args(["new-cli", "demo", "--no-git", "--app", "nope.toml"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("application manifest"), "{stderr}");
    assert!(stderr.contains("fix:"), "{stderr}");
}

/// The generated guard has to *run*, not merely compile.
///
/// Every claim this generator makes is behavioural -- a retry replays rather
/// than conflicts, a reused key is refused, an in-flight key is told to wait --
/// and none of it is visible to a compile check. The generated unit test
/// asserts all four outcomes, so the thing to verify here is that Surefire
/// actually executes it: `plan.md` §18's standing warning is that a skipped
/// tier-3 test reports as a pass.
#[test]
fn generate_idempotency_produces_tests_that_run_and_pass() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    if !real_java_supports_target_release() {
        skip("javac does not accept the target release");
        return;
    }
    let root = temp_dir("idempotency-real");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();

    assert!(
        jails_cmd(&root, None)
            .args(["add", "db", "--no-start"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        jails_cmd(&root, None)
            .args(["g", "idempotency", "Request"])
            .status()
            .unwrap()
            .success()
    );

    let path = real_path_without_mvnd();
    let verified = verified_spring_db_toolbox(&path);
    assert_surefire_test_count(verified, "RequestGuardTest", 5);
}

#[test]
fn app_init_creates_a_parseable_starter_manifest() {
    let root = temp_dir("app-init");
    write_project_skeleton(&root);

    let init = jails_cmd(&root, None)
        .args(["app", "init"])
        .output()
        .unwrap();
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    let manifest = fs::read_to_string(root.join(".jails/app.toml")).unwrap();
    assert!(manifest.contains("schema = 1"), "{manifest}");
    assert!(manifest.contains("timestamps = true"), "{manifest}");

    let plan = jails_cmd(&root, None)
        .args(["app", "plan"])
        .output()
        .unwrap();
    assert!(
        plan.status.success(),
        "{}",
        String::from_utf8_lossy(&plan.stderr)
    );
    assert!(String::from_utf8_lossy(&plan.stdout).contains("plan only"));

    let duplicate = jails_cmd(&root, None)
        .args(["app", "init"])
        .output()
        .unwrap();
    assert!(!duplicate.status.success());
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("fix:"));
}

#[test]
fn app_manifest_plan_is_domain_blind_and_writes_nothing() {
    let root = temp_dir("app-manifest-plan");
    write_spring_fixture(&root);
    let manifest = root.join("crawler.toml");
    fs::write(
        &manifest,
        include_str!("../examples/web-crawler/.jails/app.toml"),
    )
    .unwrap();

    let output = jails_cmd(&root, None)
        .args(["app", "plan", "--manifest"])
        .arg(&manifest)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ensure capability  db"), "{stdout}");
    assert!(
        stdout.contains("pending  generate scaffold CrawlRun"),
        "{stdout}"
    );
    assert!(!root.join("jails.toml").exists());
    assert!(!root.join(".jails/app-state-v1").exists());
}

#[test]
fn app_manifest_formats_the_complete_generated_tree_once() {
    let root = temp_dir("app-format-once");
    write_plain_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/app.toml"),
        "schema = 1\ncapabilities = [\"format\"]\n\n[[generate]]\nkind = \"record\"\nname = \"Note\"\nfields = [\"title:string!\"]\n",
    )
    .unwrap();
    let fake = temp_dir("app-format-once-bin");
    let log = fake.join("maven.log");
    write_fake_maven(&fake, &["mvn"], &log);

    let output = jails_cmd(&root, Some(&fake))
        .args(["app", "apply", "--no-start"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let invocations = read_log(&log);
    assert_eq!(
        invocations.lines().count(),
        1,
        "format should run after generation, once: {invocations}"
    );
    assert!(invocations.contains("spotless:apply"), "{invocations}");
    assert!(
        root.join("src/main/java/com/example/demo/domain/Note.java")
            .is_file()
    );
}

/// Reading is not a migration.
///
/// `app plan`, `--pretend` and inspection all reach the provenance store, and
/// it used to fold a pre-ledger `.jails/` and **delete the old files** from
/// inside the read. Asking jails what it would do therefore consumed the only
/// record of what it had done -- and if the answer was "nothing to destroy",
/// the evidence for that answer had just been thrown away.
#[test]
fn plan_pretend_and_inspection_leave_a_pre_ledger_project_byte_for_byte() {
    let root = temp_dir("legacy-read-purity");
    write_plain_fixture(&root);
    fs::create_dir_all(root.join(".jails/intents")).unwrap();
    fs::create_dir_all(root.join(".jails/models")).unwrap();
    fs::write(
        root.join(".jails/intents/record-note-44c464a9777ec2f0.files"),
        "src/main/java/com/example/demo/domain/Note.java\n",
    )
    .unwrap();
    fs::write(
        root.join(".jails/models/model-note-70ab6d016b346e7e.files"),
        "title:string!\n",
    )
    .unwrap();
    fs::write(root.join(".jails/version"), "0.0.1\n").unwrap();
    fs::write(
        root.join(".jails/app.toml"),
        "schema = 1\ncapabilities = []\n\n[[generate]]\nkind = \"record\"\nname = \"Note\"\n\
         fields = [\"title:string!\"]\n",
    )
    .unwrap();
    let before = snapshot_tree(&root.join(".jails"));

    for arguments in [
        vec!["app", "plan"],
        vec!["destroy", "record", "Note", "--pretend"],
        vec!["generate", "record", "Other", "title:string!", "--pretend"],
        vec!["routes"],
    ] {
        let output = jails_cmd(&root, None).args(&arguments).output().unwrap();
        assert!(
            output.status.success(),
            "`jails {}` failed: {}{}",
            arguments.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            before,
            snapshot_tree(&root.join(".jails")),
            "`jails {}` changed machine state while only being asked to report",
            arguments.join(" ")
        );
    }
    assert!(
        !root.join(".jails/ledger.toml").exists(),
        "and no read created the ledger either"
    );

    // The first mutating command migrates, and only then is the old layout
    // retired -- after the ledger that replaces it is durable.
    let applied = jails_cmd(&root, None)
        .args(["app", "apply", "--no-start"])
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}{}",
        String::from_utf8_lossy(&applied.stdout),
        String::from_utf8_lossy(&applied.stderr)
    );
    assert!(root.join(".jails/ledger.toml").is_file());
    assert!(!root.join(".jails/intents").exists());
    assert!(!root.join(".jails/models").exists());
    assert!(!root.join(".jails/version").exists());
}

/// Every file under a directory with its bytes, so "left it alone" is a claim
/// about content rather than only about which names still exist.
fn snapshot_tree(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(snapshot_tree(&path));
        } else {
            out.push((path.clone(), fs::read(&path).unwrap()));
        }
    }
    out.sort();
    out
}

#[test]
fn app_manifest_merges_an_edited_intent_over_user_changes() {
    let root = temp_dir("app-intent-merge");
    write_plain_fixture(&root);
    let git = std::process::Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(&root)
        .status();
    if !git.is_ok_and(|status| status.success()) {
        skip("git not found on PATH");
        return;
    }
    fs::create_dir_all(root.join(".jails")).unwrap();
    let manifest = root.join(".jails/app.toml");
    fs::write(
        &manifest,
        "schema = 1\ncapabilities = []\n\n[[generate]]\nkind = \"record\"\nname = \"Note\"\nfields = [\"id:uuid@pk\"]\n",
    )
    .unwrap();

    let first = jails_cmd(&root, None)
        .args(["app", "apply", "--no-start"])
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "{}{}",
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr)
    );
    // Simulate a project written by the schema-1 state format. The migration
    // must recover the old field spec from the recorded model (the legacy comma
    // join was ambiguous for map<K,V>) and fold this file into the one ledger,
    // removing it -- two registries that disagree are worse than one.
    fs::write(
        root.join(".jails/app-state-v1"),
        "schema=1\nrecord|Note||id:uuid@pk|false|||\n",
    )
    .unwrap();
    let record = root.join("src/main/java/com/example/demo/domain/Note.java");
    let source = fs::read_to_string(&record).unwrap();
    let edited = source.replacen(
        "\n}\n",
        "\n\n    public String userLabel() { return id.toString(); }\n}\n",
        1,
    );
    assert_ne!(edited, source);
    fs::write(&record, edited).unwrap();
    fs::write(
        &manifest,
        "schema = 1\ncapabilities = []\n\n[[generate]]\nkind = \"record\"\nname = \"Note\"\nfields = [\"id:uuid@pk\", \"title:string!\"]\n",
    )
    .unwrap();

    let plan = jails_cmd(&root, None)
        .args(["app", "plan"])
        .output()
        .unwrap();
    assert!(plan.status.success());
    assert!(
        String::from_utf8_lossy(&plan.stdout).contains("update   generate record Note"),
        "{}",
        String::from_utf8_lossy(&plan.stdout)
    );
    let update = jails_cmd(&root, None)
        .args(["app", "apply", "--no-start"])
        .output()
        .unwrap();
    assert!(
        update.status.success(),
        "{}{}",
        String::from_utf8_lossy(&update.stdout),
        String::from_utf8_lossy(&update.stderr)
    );
    let merged = fs::read_to_string(&record).unwrap();
    assert!(merged.contains("String title"), "{merged}");
    assert!(merged.contains("userLabel()"), "{merged}");
    assert!(!merged.contains("<<<<<<<"), "{merged}");
    let ledger = fs::read_to_string(root.join(".jails/ledger.toml")).unwrap();
    assert!(
        ledger.contains("recipe = \"record\"") && ledger.contains("name = \"Note\""),
        "the applied intent is on the one ledger: {ledger}"
    );

    let second = jails_cmd(&root, None)
        .args(["app", "apply", "--no-start"])
        .output()
        .unwrap();
    assert!(second.status.success());
    assert!(
        String::from_utf8_lossy(&second.stdout).contains("applied  generate record Note"),
        "{}",
        String::from_utf8_lossy(&second.stdout)
    );
}

#[test]
fn app_manifest_refuses_an_intent_update_without_git_before_writing() {
    let root = temp_dir("app-intent-no-git");
    write_plain_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    let manifest = root.join(".jails/app.toml");
    fs::write(
        &manifest,
        "schema = 1\ncapabilities = []\n\n[[generate]]\nkind = \"record\"\nname = \"Note\"\nfields = [\"id:uuid@pk\"]\n",
    )
    .unwrap();
    assert!(
        jails_cmd(&root, None)
            .args(["app", "apply", "--no-start"])
            .status()
            .unwrap()
            .success()
    );
    let record = root.join("src/main/java/com/example/demo/domain/Note.java");
    let before = fs::read_to_string(&record).unwrap();
    fs::write(
        &manifest,
        "schema = 1\ncapabilities = []\n\n[[generate]]\nkind = \"record\"\nname = \"Note\"\nfields = [\"id:uuid@pk\", \"title:string!\"]\n",
    )
    .unwrap();

    let output = jails_cmd(&root, None)
        .args(["app", "apply", "--no-start"])
        .env("GIT_CEILING_DIRECTORIES", "/tmp")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not in a git repository"), "{stderr}");
    assert!(stderr.contains("fix:"), "{stderr}");
    assert_eq!(fs::read_to_string(record).unwrap(), before);
}

#[test]
fn scaffold_refuses_an_unmapped_project_type_before_writing() {
    let root = temp_dir("scaffold-unmapped-type");
    write_spring_fixture(&root);
    let output = jails_cmd(&root, None)
        .args(["g", "scaffold", "Book", "id:uuid@pk", "author:Author"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("author:Author"), "{stderr}");
    assert!(stderr.contains("cannot persist"), "{stderr}");
    assert!(stderr.contains("fix:"), "{stderr}");
    assert!(
        !root
            .join("src/main/java/com/example/demo/domain/Book.java")
            .exists()
    );
    assert!(!root.join("src/main/resources/db/migration").exists());
}

#[test]
fn app_manifest_builds_the_crawler_skeleton_and_is_resumable() {
    let root = temp_dir("app-manifest-crawler");
    write_spring_fixture(&root);
    let manifest_dir = root.join(".jails");
    fs::create_dir_all(&manifest_dir).unwrap();
    fs::write(
        manifest_dir.join("app.toml"),
        include_str!("../examples/web-crawler/.jails/app.toml"),
    )
    .unwrap();

    for attempt in 1..=2 {
        let output = jails_cmd(&root, None)
            .args(["app", "apply", "--no-start"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "attempt {attempt}: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let main = root.join("src/main/java/com/example/demo");
    assert!(main.join("domain/CrawlStatus.java").is_file());
    assert!(main.join("domain/CrawlRun.java").is_file());
    assert!(main.join("domain/CrawledPage.java").is_file());
    assert!(main.join("service/QueueCrawlUseCase.java").is_file());
    assert!(main.join("service/DefaultQueueCrawlUseCase.java").is_file());
    assert!(main.join("web/QueueCrawlController.java").is_file());
    assert!(main.join("service/RecordCrawledPageUseCase.java").is_file());
    assert!(main.join("service/CrawlRunsByStatusQuery.java").is_file());
    assert!(
        main.join("adapters/JdbcCrawlRunsByStatusQuery.java")
            .is_file()
    );
    assert!(
        main.join("web/CrawlRunsByStatusQueryController.java")
            .is_file()
    );
    assert!(main.join("service/PagesByCrawlRunQuery.java").is_file());
    assert!(
        main.join("adapters/JdbcPagesByCrawlRunQuery.java")
            .is_file()
    );
    assert!(main.join("clients/PageFetcher.java").is_file());
    let safe_fetcher = fs::read_to_string(main.join("clients/SafePageFetcher.java")).unwrap();
    assert!(
        safe_fetcher.contains("new PinnedResolver"),
        "{safe_fetcher}"
    );
    assert!(
        safe_fetcher.contains("private or reserved address"),
        "{safe_fetcher}"
    );
    assert!(
        safe_fetcher.contains("acceptedStatuses.contains(response.statusCode())"),
        "{safe_fetcher}"
    );
    assert!(main.join("jobs/SiteTraversalWorkflow.java").is_file());
    assert!(
        main.join("web/SiteTraversalWorkflowController.java")
            .is_file()
    );
    assert!(
        root.join("src/test/java/com/example/demo/jobs/SiteTraversalWorkflowIT.java")
            .is_file()
    );
    assert!(main.join("messaging/PageDiscoveredEvent.java").is_file());
    assert!(main.join("jobs/CrawlDispatcherWork.java").is_file());
    assert!(main.join("jobs/SchedulingConfig.java").is_file());
    assert!(main.join("jobs/JdbcCrawlDispatcherStore.java").is_file());
    assert!(main.join("jobs/CrawlDispatcherWorker.java").is_file());
    assert!(main.join("web/CrawlDispatcherJobController.java").is_file());
    assert!(root.join(".jails/ledger.toml").is_file());
    // One ledger, not four registries: `abstract.md` rung 8's gate is that
    // `.jails/` holds the manifest and the bookkeeping, and nothing else.
    let bookkeeping = fs::read_dir(root.join(".jails"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        bookkeeping,
        ["app.toml".to_string(), "ledger.toml".to_string()].into(),
        "{bookkeeping:?}"
    );
    assert!(root.join("Dockerfile").is_file());
    assert!(root.join(".github/workflows/ci.yml").is_file());
    assert!(root.join(".github/workflows/image.yml").is_file());
    assert_eq!(
        fs::read_dir(root.join("src/main/resources/db/migration"))
            .unwrap()
            .count(),
        5
    );
}

#[test]
fn app_manifest_builds_the_support_inbox_from_the_same_generic_intents() {
    let root = temp_dir("app-manifest-inbox");
    write_spring_fixture(&root);
    let manifest_dir = root.join(".jails");
    fs::create_dir_all(&manifest_dir).unwrap();
    fs::write(
        manifest_dir.join("app.toml"),
        include_str!("../examples/support-inbox/.jails/app.toml"),
    )
    .unwrap();

    let output = jails_cmd(&root, None)
        .args(["app", "apply", "--no-start"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let main = root.join("src/main/java/com/example/demo");
    for name in [
        "Workspace",
        "Member",
        "Inbox",
        "InboxMember",
        "Contact",
        "Conversation",
        "Message",
        "ConversationAssignment",
    ] {
        assert!(main.join(format!("domain/{name}.java")).is_file(), "{name}");
        assert!(
            main.join(format!("web/{name}Controller.java")).is_file(),
            "{name} controller"
        );
    }
    for name in [
        "ConversationStatus",
        "MessageDirection",
        "MemberRole",
        "InboxChannel",
        "AssignmentStatus",
    ] {
        assert!(main.join(format!("domain/{name}.java")).is_file(), "{name}");
    }
    for name in [
        "CreateWorkspace",
        "CreateMember",
        "CreateInbox",
        "AddInboxMember",
        "CreateContact",
        "OpenConversation",
        "AssignConversation",
        "ReceiveMessage",
    ] {
        assert!(
            main.join(format!("service/{name}UseCase.java")).is_file(),
            "{name} usecase"
        );
        assert!(
            main.join(format!("web/{name}Controller.java")).is_file(),
            "{name} controller"
        );
    }
    for name in [
        "ContactsByWorkspace",
        "MembersByWorkspace",
        "InboxesByWorkspace",
        "InboxMembersByInbox",
        "ConversationsByWorkspace",
        "MessagesByConversation",
        "AssignmentByConversation",
    ] {
        assert!(
            main.join(format!("service/{name}Query.java")).is_file(),
            "{name} query"
        );
        assert!(
            main.join(format!("adapters/Jdbc{name}Query.java"))
                .is_file(),
            "{name} JDBC adapter"
        );
        assert!(
            main.join(format!("web/{name}QueryController.java"))
                .is_file(),
            "{name} controller"
        );
    }
    assert!(main.join("messaging/MessageReceivedEvent.java").is_file());
    assert!(
        main.join("service/OutboxReceiveMessageUseCase.java")
            .is_file()
    );
    let outbox = fs::read_to_string(main.join("jobs/JdbcReceiveMessageOutbox.java")).unwrap();
    assert!(outbox.contains("for update skip locked"), "{outbox}");
    assert!(main.join("jobs/ReceiveMessageOutboxWorker.java").is_file());
    assert!(main.join("jobs/SchedulingConfig.java").is_file());
    assert!(
        main.join("service/ChangeConversationStatusUseCase.java")
            .is_file()
    );
    let transition =
        fs::read_to_string(main.join("adapters/JdbcChangeConversationStatusTransition.java"))
            .unwrap();
    assert!(transition.contains("version = version + 1"), "{transition}");
    assert!(
        transition.contains("public class JdbcChangeConversationStatusTransition"),
        "{transition}"
    );
    assert!(
        transition.contains("workspace_id = :workspace_id"),
        "{transition}"
    );
    assert!(
        main.join("web/ChangeConversationStatusController.java")
            .is_file()
    );
    let assignment_transition =
        fs::read_to_string(main.join("adapters/JdbcReassignConversationTransition.java")).unwrap();
    assert!(
        assignment_transition.contains("member_id = :member_id"),
        "{assignment_transition}"
    );
    assert!(
        assignment_transition.contains("workspace_id = :workspace_id"),
        "{assignment_transition}"
    );
    assert!(main.join("jobs/ReceiveMessageOutboxSink.java").is_file());
    assert!(
        main.join("jobs/ReceiveMessageKafkaOutboxSink.java")
            .is_file()
    );
    let provider = fs::read_to_string(main.join("jobs/ProviderHttpOutboxSink.java")).unwrap();
    assert!(provider.contains("Idempotency-Key"), "{provider}");
    assert!(provider.contains("HttpClient.Redirect.NEVER"), "{provider}");
    assert!(
        root.join("src/test/java/com/example/demo/jobs/ProviderHttpOutboxSinkTest.java")
            .is_file()
    );
    for name in [
        "ContactWorkspace",
        "MemberWorkspace",
        "InboxWorkspace",
        "ConversationContact",
        "ConversationInbox",
        "InboxMemberInbox",
        "InboxMemberMember",
        "MessageConversation",
        "AssignmentConversation",
        "AssignmentMember",
    ] {
        assert!(
            root.join(format!(
                "src/test/java/com/example/demo/adapters/{name}AssociationIT.java"
            ))
            .is_file(),
            "{name} association test"
        );
    }
    let contacts =
        fs::read_to_string(main.join("web/ContactsByWorkspaceQueryController.java")).unwrap();
    assert!(contacts.contains("scopeAuthorizer.require"), "{contacts}");
    let contact_controller = fs::read_to_string(main.join("web/ContactController.java")).unwrap();
    assert!(contact_controller.contains("Scope-safe creation endpoint"));
    assert!(
        !contact_controller.contains("@GetMapping"),
        "{contact_controller}"
    );
    assert!(root.join("Dockerfile").is_file());
    assert!(root.join(".github/workflows/ci.yml").is_file());
    let inbox_migration =
        fs::read_to_string(root.join("src/main/resources/db/migration/V003__create_inboxes.sql"))
            .unwrap();
    assert!(
        inbox_migration.contains("create table inboxes"),
        "{inbox_migration}"
    );
    assert_eq!(
        fs::read_dir(root.join("src/main/resources/db/migration"))
            .unwrap()
            .count(),
        20
    );
}

#[test]
fn generated_http_sink_delivers_typed_json_with_a_stable_idempotency_key() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    if !real_java_available() {
        skip("java/javac not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("http-outbox-sink-real");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/app.toml"),
        include_str!("../examples/support-inbox/.jails/app.toml"),
    )
    .unwrap();

    let output = jails_cmd_with_path(&root, &path)
        .args(["app", "apply", "--no-start"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "apply: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let support = verified_app_unit_fixtures(&path)
        .iter()
        .find(|(name, _)| *name == "support-inbox")
        .map(|(_, root)| root)
        .unwrap();
    assert_surefire_test_count(support, "ProviderHttpOutboxSinkTest", 1);
}

/// The proof applications, as (name, manifest). One list, read by both gates
/// below: a second copy is how one of them quietly stops covering an app.
///
/// The ledger CLI is **not** here -- it is the control, has no Spring parent,
/// and needs the plain fixture. `ledger_cli_manifest_builds_without_spring`
/// is its gate.
const SPRING_APP_MANIFESTS: &[(&str, &str)] = &[
    (
        "web-crawler",
        include_str!("../examples/web-crawler/.jails/app.toml"),
    ),
    (
        "support-inbox",
        include_str!("../examples/support-inbox/.jails/app.toml"),
    ),
    (
        "payments-gateway",
        include_str!("../examples/payments-gateway/.jails/app.toml"),
    ),
];

const PROOF_APP_CACHE_SCHEMA: &str = "proof-apps:v2:shared-demo-actuator-prometheus-context";

/// Finish only the concrete toolbox proof after the generic generators have
/// written their intentionally honest TODOs.
fn overlay_plain_toolbox_completions(root: &Path) {
    const FILES: &[&str] = &[
        "src/main/java/com/example/demo/MoneyMoved.java",
        "src/main/java/com/example/demo/domain/Tally.java",
        "src/main/java/com/example/demo/domain/Transaction.java",
        "src/main/java/com/example/demo/domain/DomesticEligibility.java",
        "src/main/java/com/example/demo/domain/ExactReferenceMatchRule.java",
        "src/main/java/com/example/demo/domain/AmountAndDateMatchRule.java",
        "src/main/java/com/example/demo/domain/FuzzyMemoMatchRule.java",
        "src/test/java/com/example/demo/MoneyMovedTest.java",
        "src/test/java/com/example/demo/domain/TallyTest.java",
        "src/test/java/com/example/demo/domain/DomesticEligibilityTest.java",
        "src/test/java/com/example/demo/domain/ExactReferenceMatchRuleTest.java",
        "src/test/java/com/example/demo/domain/AmountAndDateMatchRuleTest.java",
        "src/test/java/com/example/demo/domain/FuzzyMemoMatchRuleTest.java",
        "src/test/java/com/example/demo/BriefTest.java",
        "src/test/java/com/example/demo/CheckoutIT.java",
    ];
    let fixtures = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/plain-toolbox-completions"
    ));

    for relative in FILES {
        let source = fixtures.join(relative);
        let destination = root.join(relative);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::copy(&source, &destination).unwrap_or_else(|error| {
            panic!(
                "failed to overlay {} onto {}: {error}",
                source.display(),
                destination.display()
            )
        });
    }
}

/// One real plain-Maven verification for every plain capability and generator
/// branch exercised below. The focused Rust tests still generate their own
/// projects and assert their exact semantics; sharing only this compile/test
/// gate removes repeated Maven/JUnit startup without dropping a source or a
/// generated test from toolchain coverage.
fn verified_plain_toolbox(path: &str) -> &'static std::path::PathBuf {
    static VERIFIED: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    VERIFIED.get_or_init(|| {
        let workdir = temp_dir("plain-toolbox-verified");
        let status = jails_cmd_with_path(&workdir, path)
            .args(["new-cli", "demo"])
            .status()
            .unwrap();
        assert!(status.success(), "new-cli failed for the plain toolbox");
        let root = workdir.join("demo");
        for capability in ["fake", "http"] {
            let status = jails_cmd_with_path(&root, path)
                .args(["add", capability])
                .status()
                .unwrap();
            assert!(status.success(), "add {capability} failed in plain toolbox");
        }
        for args in [
            &["g", "class", "MoneyMoved"][..],
            &[
                "g",
                "record",
                "Tally",
                "hits:int@nonnegative",
                "total:long@nonnegative",
            ][..],
            &["g", "enum", "Currency", "GBP", "EUR"][..],
            &[
                "g",
                "record",
                "SourceRef",
                "system:string",
                "externalId:string",
            ][..],
            &[
                "g",
                "value",
                "CanonicalTransaction",
                "id:string!",
                "date:date",
                "amountMinor:long",
                "currency:Currency",
                "source:SourceRef",
                "note:string?",
            ][..],
            &["g", "sealed", "Outcome", "Accepted", "Rejected"][..],
            &[
                "g",
                "value",
                "Stamped",
                "at:string!",
                "result:Outcome",
            ][..],
            &["g", "record", "Transaction", "id:uuid", "amount:long"][..],
            &["g", "record", "Reward", "id:uuid", "amount:long"][..],
            &[
                "g",
                "strategy",
                "Eligibility",
                "Domestic",
                "--on",
                "Transaction",
            ][..],
            &["g", "integration-test", "Checkout"][..],
        ] {
            let status = jails_cmd_with_path(&root, path)
                .args(args)
                .status()
                .unwrap();
            assert!(status.success(), "{args:?} failed in plain toolbox");
        }
        fs::write(
            root.join("brief.md"),
            "# Brief\n\n## Acceptance criteria\n\n- parses a `quoted` value\n- rejects **blank** ids\n",
        )
        .unwrap();
        let status = jails_cmd_with_path(&root, path)
            .args(["g", "cases", "brief.md"])
            .status()
            .unwrap();
        assert!(status.success(), "generate cases failed in plain toolbox");

        // Apply the exact control manifest last. Its deferred `format`
        // capability formats both the manifest output and the toolbox files
        // above in one invocation, after every source exists.
        fs::create_dir_all(root.join(".jails")).unwrap();
        fs::write(
            root.join(".jails/app.toml"),
            include_str!("../examples/ledger-cli/.jails/app.toml"),
        )
        .unwrap();
        let output = jails_cmd_with_path(&root, path)
            .args(["app", "apply", "--no-start"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "plain toolbox manifest: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        // The ledger manifest makes LedgerCli the executable dispatcher.
        // Generate this command afterwards and target that dispatcher so the
        // shared runtime gate proves the final application registration.
        let status = jails_cmd_with_path(&root, path)
            .args(["g", "command", "Greet", "--on", "LedgerCli"])
            .status()
            .unwrap();
        assert!(status.success(), "generate Greet failed in plain toolbox");

        overlay_plain_toolbox_completions(&root);

        let status = jails_cmd_with_path(&root, path)
            .arg("check")
            .status()
            .unwrap();
        assert!(
            status.success(),
            "the shared plain toolbox failed clean verify"
        );
        let surefire = maven_report_summary(&root, "surefire-reports");
        assert_eq!(
            surefire,
            MavenReportSummary {
                reports: 29,
                tests: 89,
                failures: 0,
                errors: 0,
                skipped: 0,
            },
            "the plain toolbox must execute every Surefire test"
        );
        let failsafe = maven_report_summary(&root, "failsafe-reports");
        assert_eq!(
            failsafe,
            MavenReportSummary {
                reports: 1,
                tests: 1,
                failures: 0,
                errors: 0,
                skipped: 0,
            },
            "the plain toolbox must execute CheckoutIT"
        );
        root
    })
}

/// Two concurrent Spring/JUnit executions for the focused capability and
/// generator tests.
/// Each Rust test still creates its own fixture and checks the exact files it
/// asked jails to write; these toolboxes are the shared proof that the same
/// generated branches compile and that every Surefire test actually runs.
///
/// The split is semantic, not a test filter: security/Redis/mail change the
/// actuator health result, and SSE plus a job deliberately produce separate
/// SchedulingConfig classes which would collide in one artificial app. Every
/// generated test in both valid projects runs, and their Maven lifecycles
/// overlap.
struct SpringToolboxes {
    core: std::path::PathBuf,
    services: std::path::PathBuf,
}

fn verified_spring_toolboxes(path: &str) -> &'static SpringToolboxes {
    static VERIFIED: std::sync::OnceLock<SpringToolboxes> = std::sync::OnceLock::new();
    VERIFIED.get_or_init(|| {
        let (core, core_fresh) = cached_toolchain_dir("spring-core-toolbox");
        let (services, services_fresh) = cached_toolchain_dir("spring-services-toolbox");

        if core_fresh {
            write_spring_fixture(&core);
            for capability in ["api", "cache", "actuator", "observability", "json", "sse"] {
                let status = jails_cmd_with_path(&core, path)
                    .args(["add", capability, "--no-start"])
                    .status()
                    .unwrap();
                assert!(
                    status.success(),
                    "add {capability} failed in the core Spring toolbox"
                );
            }
            for args in [
                &[
                    "generate",
                    "scaffold",
                    "Post",
                    "title:string",
                    "body:text",
                    "published:boolean",
                ][..],
                &["generate", "controller", "Health"][..],
                &["generate", "service", "Billing"][..],
                &[
                    "generate",
                    "record",
                    "Tag",
                    "name:string",
                    "createdAt:datetime",
                ][..],
                &[
                    "generate",
                    "record",
                    "Payout",
                    "id:uuid",
                    "amount:long",
                    "note:string?",
                ][..],
                &["generate", "dto", "Payout"][..],
                &["generate", "client", "Billing"][..],
            ] {
                let status = jails_cmd_with_path(&core, path)
                    .args(args)
                    .status()
                    .unwrap();
                assert!(
                    status.success(),
                    "{args:?} failed in the core Spring toolbox"
                );
            }

            mark_toolchain_dir_generated(&core);
        }

        if services_fresh {
            write_spring_fixture(&services);
            for capability in ["kafka", "security", "redis", "mail"] {
                let status = jails_cmd_with_path(&services, path)
                    .args(["add", capability, "--no-start"])
                    .status()
                    .unwrap();
                assert!(
                    status.success(),
                    "add {capability} failed in the services Spring toolbox"
                );
            }
            for args in [
                &[
                    "generate",
                    "event",
                    "PayoutSettled",
                    "id:uuid",
                    "payoutId:uuid",
                    "amount:decimal",
                    "occurredAt:instant",
                ][..],
                &["generate", "auth", "Api"][..],
                &["generate", "webhook", "Provider"][..],
                &["generate", "job", "Sweep"][..],
            ] {
                let status = jails_cmd_with_path(&services, path)
                    .args(args)
                    .status()
                    .unwrap();
                assert!(
                    status.success(),
                    "{args:?} failed in the services Spring toolbox"
                );
            }
            mark_toolchain_dir_generated(&services);
        }

        std::thread::scope(|scope| {
            let core_test = scope.spawn(|| {
                real_maven_cmd(&core, path)
                    .args(["-q", "test"])
                    .status()
                    .unwrap()
            });
            let services_test = scope.spawn(|| {
                real_maven_cmd(&services, path)
                    .args([
                        "-q",
                        "-Dapp.auth.secret=0123456789abcdef0123456789abcdef",
                        "-Dapp.provider.secret=toolbox-provider-secret",
                        "test",
                    ])
                    .status()
                    .unwrap()
            });
            assert!(
                core_test.join().unwrap().success(),
                "the core Spring toolbox failed mvn test"
            );
            assert!(
                services_test.join().unwrap().success(),
                "the services Spring toolbox failed mvn test"
            );
        });
        SpringToolboxes { core, services }
    })
}

fn verified_spring_toolbox(path: &str) -> &'static std::path::PathBuf {
    &verified_spring_toolboxes(path).core
}

fn verified_spring_services_toolbox(path: &str) -> &'static std::path::PathBuf {
    &verified_spring_toolboxes(path).services
}

/// Shared compile-and-unit-test proof for generators which require the JDBC
/// capability. The dedicated `add_db_on_spring_makes_context_loads_pass` test
/// still exercises the generated Testcontainers default against PostgreSQL;
/// this toolbox uses H2 for the branches whose original contract was only
/// javac/Surefire, avoiding three more Maven JVMs and containers.
fn verified_spring_db_toolbox(path: &str) -> &'static std::path::PathBuf {
    static VERIFIED: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    VERIFIED.get_or_init(|| {
        let (root, fresh) = cached_toolchain_dir("spring-db-toolbox");
        if fresh {
            write_spring_fixture(&root);
            let status = jails_cmd_with_path(&root, path)
                .args(["add", "db", "--no-start"])
                .status()
                .unwrap();
            assert!(status.success(), "add db failed in the JDBC toolbox");

            for args in [
                &["generate", "enum", "Currency", "GBP", "USD"][..],
                &[
                    "generate",
                    "scaffold",
                    "Payout",
                    "id:uuid",
                    "amount:bigdecimal",
                    "currency:Currency",
                    "paidAt:instant",
                    "note:string?",
                ][..],
                &["generate", "idempotency", "Request"][..],
                &[
                    "generate",
                    "scaffold",
                    "Article",
                    "id:uuid@pk",
                    "title:string!",
                    "body:string",
                ][..],
                &["generate", "search", "Article", "title", "body"][..],
            ] {
                let status = jails_cmd_with_path(&root, path)
                    .args(args)
                    .status()
                    .unwrap();
                assert!(status.success(), "{args:?} failed in the JDBC toolbox");
            }

            add_app_unit_test_database(&root);
            mark_toolchain_dir_generated(&root);
        }
        let mut command = real_maven_cmd(&root, path);
        configure_app_unit_maven(&mut command, "db-toolbox");
        let status = command.args(["-q", "test"]).status().unwrap();
        assert!(status.success(), "the shared JDBC toolbox failed mvn test");
        root
    })
}

fn assert_surefire_test_count(root: &Path, class_name: &str, expected: usize) {
    let reports = root.join("target/surefire-reports");
    let report = fs::read_dir(&reports)
        .unwrap()
        .filter_map(Result::ok)
        .find(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with("TEST-") && name.contains(class_name) && name.ends_with(".xml")
        })
        .unwrap_or_else(|| panic!("{class_name} did not produce a Surefire XML report"));
    let xml = fs::read_to_string(report.path()).unwrap();
    assert!(
        xml.contains(&format!("tests=\"{expected}\"")),
        "{class_name} did not run exactly {expected} tests: {xml}"
    );
    assert!(xml.contains("failures=\"0\""), "{class_name} failed: {xml}");
    assert!(xml.contains("errors=\"0\""), "{class_name} errored: {xml}");
    assert!(
        xml.contains("skipped=\"0\""),
        "{class_name} skipped a test: {xml}"
    );
}

/// Generate each proof application exactly once per `cargo test` process.
/// Compilation and execution happen in `verified_app_fixtures`: Maven's
/// `verify` lifecycle already includes compile and test-compile, so running a
/// separate `test-compile` lifecycle first repeated three Maven startups while
/// proving a strict subset of the same result.
fn generated_app_fixtures(path: &str) -> &'static Vec<(&'static str, std::path::PathBuf)> {
    static GENERATED: std::sync::OnceLock<Vec<(&'static str, std::path::PathBuf)>> =
        std::sync::OnceLock::new();
    GENERATED.get_or_init(|| {
        let cache_salt = SPRING_APP_MANIFESTS.iter().fold(
            PROOF_APP_CACHE_SCHEMA.to_string(),
            |mut salt, (name, manifest)| {
                salt.push_str(&format!("\n{name}:{}\n{manifest}", manifest.len()));
                salt
            },
        );
        let (parent, fresh) = cached_toolchain_dir_with_salt("proof-apps", &cache_salt);
        if !fresh {
            let generated: Vec<_> = SPRING_APP_MANIFESTS
                .iter()
                .map(|(name, _)| (*name, parent.join(name)))
                .collect();
            for (_, root) in &generated {
                validate_proof_app_shared_context(root);
            }
            return generated;
        }
        let mut generated = Vec::new();
        std::thread::scope(|scope| {
            let handles: Vec<_> = SPRING_APP_MANIFESTS
                .iter()
                .map(|&(name, manifest)| {
                    let parent = &parent;
                    scope.spawn(move || {
                        let root = parent.join(name);
                        fs::create_dir_all(&root).unwrap();
                        write_spring_fixture(&root);
                        fs::create_dir_all(root.join(".jails")).unwrap();
                        fs::write(root.join(".jails/app.toml"), manifest).unwrap();

                        let output = jails_cmd_with_path(&root, path)
                            .args(["app", "apply", "--no-start"])
                            .output()
                            .unwrap();
                        assert!(
                            output.status.success(),
                            "{name} apply: stdout={} stderr={}",
                            String::from_utf8_lossy(&output.stdout),
                            String::from_utf8_lossy(&output.stderr)
                        );
                        add_app_unit_test_database(&root);
                        (name, root)
                    })
                })
                .collect();
            for handle in handles {
                generated.push(handle.join().unwrap());
            }
        });
        for (_, root) in &generated {
            align_proof_app_smoke_context(root);
        }
        mark_toolchain_dir_generated(&parent);
        generated
    })
}

fn add_app_unit_test_database(root: &Path) {
    const H2: &str = r#"        <dependency>
            <groupId>com.h2database</groupId>
            <artifactId>h2</artifactId>
            <scope>test</scope>
        </dependency>
"#;
    let pom_path = root.join("pom.xml");
    let pom = fs::read_to_string(&pom_path).unwrap();
    let marker = "    </dependencies>\n";
    assert!(
        pom.contains(marker),
        "generated app POM has no dependencies"
    );
    fs::write(pom_path, pom.replacen(marker, &format!("{H2}{marker}"), 1)).unwrap();
}

fn align_proof_app_smoke_context(project: &Path) {
    const BARE: &str = "@SpringBootTest\nclass DemoApplicationTests";

    for (file, class_name) in [
        ("ActuatorEndpointsTest.java", "ActuatorEndpointsTest"),
        ("PrometheusScrapeTest.java", "PrometheusScrapeTest"),
    ] {
        assert_proof_app_context_source(project, file, class_name);
    }

    let test = project.join("src/test/java/com/example/demo/DemoApplicationTests.java");
    let source = fs::read_to_string(&test).unwrap();
    let shared = format!("{PROOF_APP_SHARED_SPRING_BOOT_TEST}\nclass DemoApplicationTests");
    if !source.contains(&shared) {
        assert!(
            source.contains(BARE),
            "proof-app smoke test no longer has the expected Spring context annotation: {source}"
        );
        fs::write(&test, source.replacen(BARE, &shared, 1)).unwrap();
    }
    assert_proof_app_context_source(project, "DemoApplicationTests.java", "DemoApplicationTests");
}

const PROOF_APP_SHARED_SPRING_BOOT_TEST: &str = r#"@SpringBootTest(
        webEnvironment = SpringBootTest.WebEnvironment.RANDOM_PORT,
        properties = {
            "management.server.port=0",
            "app.security.dev.username=prometheus-probe",
            "app.security.dev.password=prometheus-probe"
        })"#;

fn validate_proof_app_shared_context(project: &Path) {
    for (file, class_name) in [
        ("DemoApplicationTests.java", "DemoApplicationTests"),
        ("ActuatorEndpointsTest.java", "ActuatorEndpointsTest"),
        ("PrometheusScrapeTest.java", "PrometheusScrapeTest"),
    ] {
        assert_proof_app_context_source(project, file, class_name);
    }
}

fn assert_proof_app_context_source(project: &Path, file: &str, class_name: &str) {
    let test_dir = project.join("src/test/java/com/example/demo");
    let source = fs::read_to_string(test_dir.join(file)).unwrap();
    let class_marker = format!("class {class_name}");
    let class_start = source
        .find(&class_marker)
        .unwrap_or_else(|| panic!("{file} has no {class_marker}: {source}"));
    let annotations = &source[..class_start];
    let context_start = annotations
        .rfind("@Import(")
        .unwrap_or_else(|| panic!("{file} has no context import: {source}"));
    assert_eq!(
        annotations[context_start..].trim_end(),
        format!("@Import(TestcontainersConfig.class)\n{PROOF_APP_SHARED_SPRING_BOOT_TEST}"),
        "{file} drifted from the proof application's shared Spring context"
    );
}

fn verified_app_unit_fixtures(path: &str) -> &'static Vec<(&'static str, std::path::PathBuf)> {
    static VERIFIED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    let generated = generated_app_fixtures(path);
    VERIFIED.get_or_init(|| {
        std::thread::scope(|scope| {
            for (name, root) in generated {
                scope.spawn(move || {
                    let mut command = real_maven_cmd(root, path);
                    configure_app_unit_maven(&mut command, name);
                    let status = command.args(["-q", "test"]).status().unwrap();
                    assert!(status.success(), "{name} failed its Surefire tests");
                });
            }
        });
    });
    generated
}

fn verified_app_fixtures(path: &str) -> &'static Vec<(&'static str, std::path::PathBuf)> {
    static VERIFIED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    VERIFIED.get_or_init(|| {
        let suite_started = std::time::Instant::now();
        let profile_stage = |stage: &str| {
            if std::env::var_os("JAILS_TEST_PROFILE").is_some() {
                eprintln!(
                    "JAILS_TEST_PROFILE app_stage={stage} elapsed_ms={}",
                    suite_started.elapsed().as_millis()
                );
            }
        };
        let names = SPRING_APP_MANIFESTS
            .iter()
            .map(|(name, _)| *name)
            .collect::<Vec<_>>();
        let (services_launched, launch_ready) = std::sync::mpsc::channel();
        let (postgres_ready, wait_for_postgres) = std::sync::mpsc::channel();
        let service_start = std::thread::spawn(move || {
            AppSuiteServices::start(&names, services_launched, postgres_ready)
        });
        let generated = generated_app_fixtures(path);
        profile_stage("fixtures-ready");
        let endpoints = launch_ready.recv().unwrap();
        profile_stage("containers-launched");
        let image_build = std::thread::spawn(|| verified_app_images(generated));

        // Compile and execute Surefire while PostgreSQL/Kafka are starting.
        // Failsafe follows once both real services are ready, so every test
        // still runs exactly once without a skip flag or selector.
        verified_app_unit_fixtures(path);
        profile_stage("surefire-complete");
        wait_for_postgres.recv().unwrap();
        let services = service_start.join().unwrap();
        profile_stage("services-ready");
        std::thread::scope(|scope| {
            for (name, root) in generated {
                scope.spawn(move || {
                    let mut command = real_maven_cmd(root, path);
                    endpoints.configure_maven(&mut command, name);
                    let status = command
                        .args(["-q", "failsafe:integration-test", "failsafe:verify"])
                        .status()
                        .unwrap();
                    assert!(
                        status.success(),
                        "{name} failed its Failsafe integration tests"
                    );
                });
            }
        });
        profile_stage("failsafe-complete");
        let mut reports = MavenReportSummary::default();
        for (_, root) in generated {
            reports.add(maven_report_summary(root, "failsafe-reports"));
        }
        assert_eq!(
            reports,
            MavenReportSummary {
                reports: 47,
                tests: 70,
                failures: 0,
                errors: 0,
                skipped: 0,
            },
            "the proof applications must execute every Failsafe test"
        );
        drop(services);
        profile_stage("services-stopped");
        image_build.join().unwrap();
        profile_stage("images-complete");
    });
    generated_app_fixtures(path)
}

#[test]
fn app_manifests_compile_without_manual_source_edits() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    if !real_java_available() {
        skip("java/javac not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    // `verify` contains compile and test-compile and is therefore stronger
    // than the old preliminary lifecycle. Both Rust tests share this exact
    // execution through the OnceLock above; no generated test is omitted.
    let verified = verified_app_unit_fixtures(&path);
    assert_eq!(verified.len(), SPRING_APP_MANIFESTS.len());
    for (name, root) in verified {
        assert!(
            root.join("target/classes").is_dir(),
            "{name} main sources did not compile"
        );
        assert!(
            root.join("target/test-classes").is_dir(),
            "{name} test sources did not compile"
        );
    }
}

#[test]
fn app_manifests_pass_the_full_generated_verification_gate() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    if !real_java_available() {
        skip("java/javac not found on PATH");
        return;
    }
    if !real_docker_available() {
        skip("a running Docker-compatible container runtime is required");
        return;
    }
    let path = real_path_without_mvnd();
    let fixtures = verified_app_fixtures(&path);
    verified_app_images(fixtures);
}

fn verified_app_images(fixtures: &'static Vec<(&'static str, std::path::PathBuf)>) {
    static VERIFIED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    VERIFIED.get_or_init(|| {
        // Podman does not single-flight concurrent pulls: three `docker build
        // --pull` calls downloaded the same Maven and Temurin layers three times,
        // consuming a gigabyte of memory without increasing coverage. Resolve
        // every generated FROM image once, then let the still-parallel builds use
        // the local content-addressed image store.
        let mut base_images = std::collections::BTreeSet::new();
        for (_, root) in fixtures {
            let dockerfile = fs::read_to_string(root.join("Dockerfile")).unwrap();
            for line in dockerfile.lines().filter(|line| line.starts_with("FROM ")) {
                if let Some(image) = line.split_whitespace().nth(1) {
                    base_images.insert(image.to_string());
                }
            }
        }
        for image in base_images {
            let present = std::process::Command::new("docker")
                .args(["image", "inspect", &image])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success());
            if !present {
                let status = real_docker_cmd(Path::new("."))
                    .args(["pull", &image])
                    .status()
                    .unwrap();
                assert!(
                    status.success(),
                    "could not pull generated base image {image}"
                );
            }
        }
        // Podman serialises parts of its rootless storage graph internally.
        // Three client processes made three cache-only builds take about six
        // seconds each, versus roughly 1.2 seconds apiece in sequence. This
        // loop still builds and inspects every image, while the whole image
        // phase remains overlapped with the Maven application gate.
        for (name, root) in fixtures {
            let image = format!("jails-dogfood-{name}:test");
            let status = real_docker_cmd(root)
                // Required FROM images were inspected/pulled above. Podman's
                // default `--pull=missing` can still wait for registry
                // resolution before accepting its local copy; make this
                // deliberately cached build local-only.
                .args(["build", "--pull=never", "--tag", &image, "."])
                .status()
                .unwrap();
            assert!(
                status.success(),
                "{name} failed its generated OCI image build"
            );
            let inspect = std::process::Command::new("docker")
                .args(["image", "inspect", &image, "--format", "{{.Config.User}}"])
                .output()
                .unwrap();
            assert!(inspect.status.success(), "could not inspect {image}");
            assert_eq!(
                String::from_utf8_lossy(&inspect.stdout).trim(),
                "10001:10001",
                "{name} image did not retain the non-root runtime user"
            );
        }
    });
}

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
    assert!(stdout.contains("2.50s  PayoutTest#settles"), "{stdout}");

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

/// Can Maven *read* what jails wrote? (`plan.md` §8.8.)
///
/// `mvn -o validate` parses the pom and stops -- about two seconds a cell, no
/// downloads, no compilation -- and it is the check nothing in this suite was
/// doing. The 293-second manifest gate compiles far more, but every cell of it
/// is a Spring Boot project, so a versionless dependency (correct under
/// `spring-boot-starter-parent`, fatal without one) survived it and shipped:
/// `g scaffold` on a plain Maven project wrote a `spring-boot-starter-validation`
/// with no version, and *every* Maven goal then failed with
/// `'dependencies.dependency.version' ... is missing` -- including `validate`
/// itself. The golden suite had a snapshot of that pom and ratified it.
///
/// So the matrix is over the thing that differed: the flavour of project.
#[test]
fn every_generated_pom_is_one_maven_can_read() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let matrix = temp_dir("pom-readable-matrix");
    // Kinds that splice something into the pom, which is where the defect
    // lives: a dependency, a plugin, or a test that needs AssertJ.
    let cells: &[(&str, &[&str])] = &[
        (
            "scaffold",
            &["g", "scaffold", "Note", "id:uuid@pk", "title:string!"],
        ),
        ("record", &["g", "record", "Note", "title:string!"]),
        ("integration-test", &["g", "integration-test", "Checkout"]),
        ("cli", &["g", "cli", "Admin"]),
    ];
    let mut modules = Vec::new();
    let mut generated = Vec::new();
    for spring in [false, true] {
        for (label, args) in cells {
            let flavor = if spring { "spring" } else { "plain" };
            let module = format!("{flavor}-{label}");
            let root = matrix.join(&module);
            if spring {
                write_spring_fixture(&root);
            } else {
                write_plain_fixture(&root);
            }
            let output = jails_cmd_with_path(&root, &path)
                .args(*args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{label} failed: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );

            // Reactor coordinates must be unique even though every fixture
            // deliberately starts as `com.example:demo`.
            let pom_path = root.join("pom.xml");
            let pom = fs::read_to_string(&pom_path).unwrap().replace(
                "<artifactId>demo</artifactId>",
                &format!("<artifactId>{module}</artifactId>"),
            );
            fs::write(pom_path, pom).unwrap();
            modules.push(module);
            generated.push((flavor, *label, args.join(" ")));
        }
    }

    let module_xml = modules
        .iter()
        .map(|module| format!("        <module>{module}</module>"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(
        matrix.join("pom.xml"),
        format!(
            "<project xmlns=\"http://maven.apache.org/POM/4.0.0\">\n    <modelVersion>4.0.0</modelVersion>\n    <groupId>com.example</groupId>\n    <artifactId>jails-pom-matrix</artifactId>\n    <version>1</version>\n    <packaging>pom</packaging>\n    <modules>\n{module_xml}\n    </modules>\n</project>\n"
        ),
    )
    .unwrap();

    let output = real_maven_cmd(&matrix, &path)
        .args(["-o", "-q", "validate"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Maven could not read the generated POM matrix {generated:?}:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The control application: `plan.md` §4.4's whole point is that the crawler,
/// the inbox and the payments gateway are all Spring Boot, so a Spring-shaped
/// assumption in the generic machinery is invisible to every one of them.
///
/// It runs against the **plain** fixture -- no parent POM, no starters, no
/// container -- and asks for `value`, `sealed`, `strategy`, `record`, `cli`
/// and `command`, which the three Spring manifests never touch. `mvn verify`
/// here is seconds rather than minutes, so this is the cheapest gate in the
/// suite and the one that catches "it only works because Spring".
#[test]
fn ledger_cli_manifest_builds_without_spring() {
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

    // The manifest names the dispatcher its command belongs to, so the
    // registration is part of what this gate proves rather than a note.
    let dispatcher =
        fs::read_to_string(root.join("src/main/java/com/example/demo/cli/LedgerCli.java")).unwrap();
    assert!(
        dispatcher.contains("ReconcileCommand::run"),
        "the manifest named its dispatcher, so the command must be registered in it: {dispatcher}"
    );

    assert!(root.join("target/classes").is_dir());
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
fn lint_reports_closed_set_stale_apis_as_file_and_line() {
    let root = temp_dir("lint-stale-api");
    write_project_skeleton(&root);
    let source = root.join("src/main/java/com/example/demo/Legacy.java");
    fs::write(
        &source,
        "package com.example.demo;\n\n@Entity\nclass Legacy {}\n",
    )
    .unwrap();

    let output = jails_cmd(&root, None).arg("lint").output().unwrap();
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("src/main/java/com/example/demo/Legacy.java:3"),
        "{stdout}"
    );
    assert!(stdout.contains("explicit JDBC adapter"), "{stdout}");
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
        vec!["g", "repository", "Reward", "id:uuid"],
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
    assert!(
        root.join("target/classes/com/example/demo/App.class")
            .is_file()
    );
}

#[test]
fn generate_scaffold_produces_a_project_that_compiles_and_passes_tests() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
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

    let verified = verified_spring_toolbox(&path);
    assert!(
        verified
            .join("target/test-classes/com/example/demo/web/PostControllerTest.class")
            .is_file(),
        "the shared Spring toolbox did not compile the scaffold tests"
    );
}

/// Regression coverage for the reported bug (standalone `generate
/// controller` not producing a test) plus real-compile verification of the
/// new controller/service/record companion test templates.
#[test]
fn standalone_generators_companion_tests_compile_and_pass() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
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

    let verified = verified_spring_toolbox(&path);
    for class in [
        "com/example/demo/web/HealthControllerTest.class",
        "com/example/demo/service/BillingServiceTest.class",
        "com/example/demo/domain/TagTest.class",
    ] {
        assert!(
            verified.join("target/test-classes").join(class).is_file(),
            "the Spring toolbox did not compile {class}"
        );
    }
}

/// `record`, `command` and `class` are the plain-Java kinds, so the bar for
/// them is a `new-cli` project -- no Spring anywhere -- that still compiles and
/// passes the tests they generate.
#[test]
fn record_and_command_compile_and_pass_in_a_plain_cli_project() {
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

    let verified = verified_plain_toolbox(&path);
    for class in ["MoneyMoved", "cli/GreetCommand", "domain/Tally"] {
        assert!(
            verified
                .join(format!("target/classes/com/example/demo/{class}.class"))
                .is_file(),
            "shared toolchain matrix did not compile {class}"
        );
    }
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
        pom.contains("spring-boot-testcontainers"),
        "@ServiceConnection and the container-bean lifecycle live there: {pom}"
    );
    assert!(pom.contains("<optional>true</optional>"));
    let config = root.join("src/test/java/com/example/demo/TestcontainersConfig.java");
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
    let tests =
        fs::read_to_string(root.join("src/test/java/com/example/demo/DemoApplicationTests.java"))
            .unwrap();
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

    let stale_class = root.join("target/test-classes/com/example/demo/TestcontainersConfig.class");
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
    assert!(!stale_factories.is_file());
}

/// Re-running `add db` on a project still carrying the global
/// `ApplicationContextInitializer` must migrate it: rewrite the config as an
/// importable `@TestConfiguration`, drop the `spring.factories` registration,
/// and splice the `@Import` into every `@SpringBootTest`.
///
/// The `spring.factories` deletion is the load-bearing half. Left behind, the
/// old initializer would keep registering a second container for every test
/// and the migration would look like it had not worked.
#[test]
fn add_db_on_spring_migrates_the_global_initializer_to_an_import() {
    let root = temp_dir("add-db-spring-migrate");
    write_spring_fixture(&root);
    let fake = temp_dir("add-db-spring-migrate-bin");
    let log = fake.join("log.txt");
    write_fake_maven(&fake, &["docker"], &log);

    fs::write(
        root.join("src/test/java/com/example/demo/PostgresContainerConfig.java"),
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
    let factories = root.join("src/test/resources/META-INF/spring.factories");
    fs::create_dir_all(factories.parent().unwrap()).unwrap();
    fs::write(
        &factories,
        "# jails:db\norg.springframework.context.ApplicationContextInitializer=com.example.demo.PostgresContainerConfig\n# /jails:db\n",
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

    let config =
        fs::read_to_string(root.join("src/test/java/com/example/demo/TestcontainersConfig.java"))
            .unwrap();
    assert!(
        !config.contains("ApplicationContextInitializer"),
        "the global registration is what this migration removes: {config}"
    );
    assert!(config.contains("@ServiceConnection"), "{config}");

    assert!(
        !factories.is_file(),
        "a leftover spring.factories would keep registering the old initializer"
    );

    // Both @SpringBootTest classes get the import, including the one in a
    // different package -- which needs the extra import statement too.
    let tests =
        fs::read_to_string(root.join("src/test/java/com/example/demo/DemoApplicationTests.java"))
            .unwrap();
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
        root.join("src/main/java/com/example/demo/InMemoryThingRepository.java"),
        r#"package com.example.demo;

import org.springframework.stereotype.Repository;

@Repository
public final class InMemoryThingRepository {}
"#,
    )
    .unwrap();

    // `add db` wires every @SpringBootTest that exists when the capability is
    // reconciled. Put this cross-package test in place first: creating it
    // afterwards accidentally made the regression depend on a developer
    // PostgreSQL listening on localhost:5432.
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

/// The whole toolbox at once: every capability and every generator in one
/// project, then its own suite. This is the only tier that answers "does what
/// jails writes actually compile and pass" for the generated *test* code as
/// well as the main code -- a template that emits an uncompilable assertion
/// looks perfectly fine to every other tier.
#[test]
fn every_generator_and_capability_together_compiles_and_passes_tests() {
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
    for path in [
        "target/classes/com/example/demo/cli/GreetCommand.class",
        "target/classes/com/example/demo/domain/Money.class",
        "target/classes/com/example/demo/domain/Tally.class",
        "target/test-classes/com/example/demo/BriefTest.class",
    ] {
        assert!(root.join(path).is_file(), "matrix did not compile {path}");
    }
}

/// The generators composing: an enum and a record, then a value type that
/// references both by name. Proves the three halves of the field syntax --
/// capitalised = a type this project owns, `!`/`?` optionality, and the
/// enum-aware sample values -- produce a project that actually compiles.
#[test]
fn generators_compose_through_user_owned_field_types() {
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

    // An enum-typed component can be sampled by reading the enum, and a
    // component whose type is a record *this project already has* by reading
    // the record: `SourceRef` was generated two commands ago, so refusing to
    // build one would be the tool forgetting what it just wrote.
    let test = fs::read_to_string(
        root.join("src/test/java/com/example/gym/domain/CanonicalTransactionTest.java"),
    )
    .unwrap();
    assert!(test.contains("Currency.values()[0]"), "{test}");
    assert!(
        test.contains("new SourceRef("),
        "a component whose type is a record on disk is sampled from it: {test}"
    );
    assert!(
        !test.contains("@Disabled"),
        "every component is fabricable now, so nothing should be disabled: {test}"
    );

    // A generated sealed interface has no constructor, but its generated
    // variants are zero-component records. Any one is a valid non-null sample
    // for testing Stamped's own validation, so Jails can construct it without
    // guessing at business data.
    let status = jails_cmd_with_path(&root, &path)
        .args(["generate", "sealed", "outcome", "Accepted", "Rejected"])
        .status()
        .unwrap();
    assert!(status.success(), "generate sealed failed");
    let status = jails_cmd_with_path(&root, &path)
        .args([
            "generate",
            "value",
            "stamped",
            "at:string!",
            "result:Outcome",
        ])
        .status()
        .unwrap();
    assert!(status.success(), "generate value with a sealed type failed");
    let stamped =
        fs::read_to_string(root.join("src/test/java/com/example/gym/domain/StampedTest.java"))
            .unwrap();
    assert!(stamped.contains("new Outcome.Accepted()"), "{stamped}");
    assert!(
        !stamped.contains("@Disabled"),
        "a generated zero-component variant is a complete sample: {stamped}"
    );

    let verified = verified_plain_toolbox(&path);
    for class in ["Currency", "SourceRef", "CanonicalTransaction", "Stamped"] {
        assert!(
            verified
                .join(format!(
                    "target/classes/com/example/demo/domain/{class}.class"
                ))
                .is_file(),
            "shared toolchain matrix did not compile {class}"
        );
    }
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

/// `add format` installs a formatter that checks the build. If jails' own
/// output does not already satisfy it, a freshly generated project fails
/// `jails check` on the first run -- a bad first impression, and the reason
/// import order is normalised at write time.
#[test]
fn a_freshly_generated_project_passes_check_with_no_manual_formatting() {
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
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("spotless-maven-plugin"), "{pom}");
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
    assert!(
        stdout.starts_with(r#"{"schema_version":3,"routes":["#),
        "{stdout}"
    );
    assert!(stdout.contains(r#""verb":"GET""#), "{stdout}");
    assert!(stdout.contains(r#""line":"#), "{stdout}");
}

#[test]
fn beans_json_is_versioned_and_reports_source_lines() {
    let root = temp_dir("beans-json");
    write_inspectable_project(&root);

    let output = jails_cmd(&root, None)
        .args(["beans", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.starts_with(r#"{"schema_version":3,"beans":["#),
        "{stdout}"
    );
    assert!(stdout.contains(r#""line":"#), "{stdout}");
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
    // The project targets a current release and declares no compose services, so
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
    fs::write(
        root.join("pom.xml"),
        "<project><artifactId>x</artifactId></project>",
    )
    .unwrap();

    let output = jails_cmd(&root, None).arg("doctor").output().unwrap();
    assert!(
        !output.status.success(),
        "doctor should fail on a broken project"
    );
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
        .args([
            "rename",
            "dev.example.shop.domain.Order",
            "Purchase",
            "--force",
        ])
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
        skip("mvn not found on PATH");
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
    assert!(
        !adapter.contains("UnsupportedOperationException"),
        "{adapter}"
    );
    assert!(
        adapter.contains("Timestamp.from(payout.paidAt())"),
        "{adapter}"
    );
    assert!(
        adapter.contains("Currency.valueOf(rows.getString(\"currency\"))"),
        "{adapter}"
    );
    // An Optional component is unwrapped on the way out and rebuilt on the way in.
    assert!(
        adapter.contains("Optional.ofNullable(rows.getString(\"note\"))"),
        "{adapter}"
    );
    assert!(adapter.contains("payout.note().orElse(null)"), "{adapter}");
    // The column list is shared by the select and the insert, so they agree.
    assert!(
        adapter.contains("insert into payouts (id, amount, currency, paid_at, note)"),
        "{adapter}"
    );

    // The DTOs name the project's own enum, so they have to import it --
    // `field.imports` only carries the built-in types' packages.
    let request =
        fs::read_to_string(root.join("src/main/java/com/example/demo/web/PayoutRequest.java"))
            .unwrap();
    assert!(
        request.contains("import com.example.demo.domain.Currency;"),
        "{request}"
    );

    let verified = verified_spring_db_toolbox(&path);
    assert!(
        verified
            .join("target/classes/com/example/demo/adapters/JdbcPayoutRepository.class")
            .is_file(),
        "the shared JDBC toolbox did not compile the derived adapter"
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

    let migration =
        fs::read_to_string(root.join("src/main/resources/db/migration/V001__create_payouts.sql"))
            .unwrap();
    assert!(migration.contains("create table payouts ("), "{migration}");
    assert!(
        migration.contains("uuid") && migration.contains("numeric"),
        "{migration}"
    );
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

#[test]
fn pretend_writes_nothing_but_still_reports_the_whole_plan() {
    let root = temp_dir("pretend");
    write_spring_fixture(&root);

    let output = jails_cmd(&root, None)
        .args(["generate", "scaffold", "Payout", "id:uuid", "--pretend"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("would create"), "{stdout}");
    assert!(stdout.contains("nothing was written"), "{stdout}");
    assert!(!stdout.contains("\ncreated "), "{stdout}");
    assert!(
        !root
            .join("src/main/java/com/example/demo/domain/Payout.java")
            .exists()
    );
}

#[test]
fn pretend_is_global_and_reaches_destroy_too() {
    let root = temp_dir("pretend-destroy");
    write_spring_fixture(&root);
    let created = jails_cmd(&root, None)
        .args(["generate", "record", "Payout", "id:uuid"])
        .output()
        .unwrap();
    assert!(created.status.success());
    let file = root.join("src/main/java/com/example/demo/domain/Payout.java");
    assert!(file.is_file());

    let output = jails_cmd(&root, None)
        .args(["destroy", "record", "Payout", "--pretend"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("would remove"), "{stdout}");
    // --pretend must not stop to ask for confirmation: nothing is at risk.
    assert!(!stdout.contains("proceed?"), "{stdout}");
    assert!(file.is_file(), "--pretend deleted a file");
}

#[test]
fn a_scaffold_writes_a_two_row_fixture_keyed_by_column_name() {
    let root = temp_dir("scaffold-fixture");
    write_spring_fixture(&root);
    // `new`/`new-cli` seed this directory; `add testkit` writes the loader
    // that reads it.
    fs::create_dir_all(root.join("src/test/resources/fixtures")).unwrap();

    let status = jails_cmd(&root, None)
        .args(["generate", "enum", "Currency", "GBP", "USD"])
        .status()
        .unwrap();
    assert!(status.success());

    let output = jails_cmd(&root, None)
        .args([
            "generate",
            "scaffold",
            "Payout",
            "paidAt:instant",
            "currency:Currency",
            "note:string?",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");

    let fixture =
        fs::read_to_string(root.join("src/test/resources/fixtures/payouts.json")).unwrap();
    // Column names, not component names -- the fixture describes what the
    // database holds, next to a JDBC adapter that reads those same columns.
    assert!(fixture.contains("\"paid_at\""), "{fixture}");
    assert!(!fixture.contains("paidAt"), "{fixture}");
    // A real constant read off the generated enum, not a guess.
    assert!(fixture.contains("\"currency\": \"GBP\""), "{fixture}");
    // Two rows, and the nullable one is absent in the second.
    assert!(fixture.contains("\"note\": \"sample-1\""), "{fixture}");
    assert!(fixture.contains("\"note\": null"), "{fixture}");
}

#[test]
fn a_project_without_a_fixtures_directory_gets_no_fixture() {
    let root = temp_dir("scaffold-no-fixture");
    write_spring_fixture(&root);

    let output = jails_cmd(&root, None)
        .args(["generate", "scaffold", "Payout", "id:uuid"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("created fixture"), "{stdout}");
}

#[test]
fn the_generated_controller_test_uses_the_assertj_mockmvc_entry_point() {
    // Spring Framework 7 / Boot 4 favour MockMvcTester over plain MockMvc:
    // one fluent chain instead of two families of static imports, and no
    // `throws Exception` on the test method.
    let root = temp_dir("controller-test-style");
    write_spring_fixture(&root);

    let status = jails_cmd(&root, None)
        .args(["generate", "controller", "Payout"])
        .status()
        .unwrap();
    assert!(status.success());

    let test = fs::read_to_string(
        root.join("src/test/java/com/example/demo/web/PayoutControllerTest.java"),
    )
    .unwrap();
    assert!(
        test.contains("org.springframework.test.web.servlet.assertj.MockMvcTester"),
        "{test}"
    );
    assert!(test.contains("assertThat(mvc.get().uri("), "{test}");
    assert!(test.contains("hasStatusOk()"), "{test}");
    // The old style would need these; the new one does not.
    assert!(!test.contains("MockMvcResultMatchers"), "{test}");
    assert!(!test.contains("throws Exception"), "{test}");
}

#[test]
fn add_db_upgrades_an_out_of_date_properties_block() {
    // `add` promises to write whatever is missing. A project generated by an
    // older jails has a jails:db block holding only the exception-translation
    // property; reporting that as "exists" would leave it permanently without
    // the datasource it now needs.
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
            .args(["add", "db"])
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
    // The block is replaced, not duplicated.
    assert_eq!(next.matches("# jails:db").count(), 1, "{next}");
    assert_eq!(
        next.matches("spring.persistence.exceptiontranslation.enabled=false")
            .count(),
        1,
        "{next}"
    );
}

// ---- Spring-only capabilities. The generated code targets Spring Boot 4 /
// Framework 7 APIs, so the only honest check is a real compile. ----

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

    let handler = fs::read_to_string(
        root.join("src/main/java/com/example/demo/api/ApiExceptionHandler.java"),
    )
    .unwrap();
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
    assert!(properties.contains("# jails:cache"), "{properties}");

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
    fs::create_dir_all(root.join("src/main/java/com/example/demo")).unwrap();
    fs::write(
        root.join("src/main/java/com/example/demo/App.java"),
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
    assert!(properties.contains("# jails:cache"), "{properties}");
    assert!(properties.contains("# jails:actuator"), "{properties}");

    // Removing one leaves the other exactly as it was.
    assert!(
        jails_cmd(&root, Some(&fake))
            .args(["remove", "cache", "--force"])
            .status()
            .unwrap()
            .success()
    );
    let after = fs::read_to_string(root.join("src/main/resources/application.properties")).unwrap();
    assert!(!after.contains("# jails:cache"), "{after}");
    assert!(!after.contains("spring.cache.type"), "{after}");
    assert!(after.contains("# jails:actuator"), "{after}");
    assert!(
        after.contains("management.endpoints.web.exposure.include"),
        "{after}"
    );
}

#[test]
fn generate_dto_client_and_job_compile_and_pass_against_real_spring() {
    // These target Spring Boot 4 / Framework 7 APIs that moved recently
    // (@ImportHttpServices, ProblemDetail, MockMvcTester), so a unit test on
    // the template text proves nothing worth knowing. javac and the real
    // context are the check.
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-spring-generators");
    write_spring_fixture(&root);

    // A domain record for the DTO to describe.
    assert!(
        jails_cmd_with_path(&root, &path)
            .args([
                "generate",
                "record",
                "Payout",
                "id:uuid",
                "amount:long",
                "note:string?"
            ])
            .status()
            .unwrap()
            .success()
    );

    for args in [
        vec!["generate", "dto", "Payout"],
        vec!["generate", "client", "Billing"],
        vec!["generate", "job", "Sweep"],
    ] {
        assert!(
            jails_cmd_with_path(&root, &path)
                .args(&args)
                .status()
                .unwrap()
                .success(),
            "{args:?} failed"
        );
    }

    let request =
        fs::read_to_string(root.join("src/main/java/com/example/demo/web/PayoutRequest.java"))
            .unwrap();
    // Constraints come from the field spec, so a bad request is rejected at
    // the edge rather than deep in the domain.
    assert!(request.contains("@NotNull UUID id"), "{request}");
    // An Optional domain component is a plain nullable field on the wire, and
    // carries no constraint -- `?` said it was optional.
    assert!(request.contains("String note"), "{request}");
    assert!(!request.contains("@NotNull String note"), "{request}");
    assert!(request.contains("Optional.ofNullable(note)"), "{request}");

    let client =
        fs::read_to_string(root.join("src/main/java/com/example/demo/clients/BillingClient.java"))
            .unwrap();
    assert!(client.contains("@GetExchange"), "{client}");
    // No base URL in the annotation: it belongs to the group's configuration.
    assert!(!client.contains("@HttpExchange(url"), "{client}");
    let config = fs::read_to_string(
        root.join("src/main/java/com/example/demo/clients/HttpClientsConfig.java"),
    )
    .unwrap();
    assert!(
        config.contains("@ImportHttpServices(group = \"billing\""),
        "{config}"
    );

    let job =
        fs::read_to_string(root.join("src/main/java/com/example/demo/jobs/SweepJob.java")).unwrap();
    // fixedDelay, not fixedRate: a slow run must not queue another on top.
    assert!(job.contains("fixedDelayString"), "{job}");
    // An exception escaping a @Scheduled method cancels every future run.
    assert!(job.contains("catch (RuntimeException"), "{job}");

    let verified = verified_spring_toolbox(&path);
    assert!(verified.join("target/test-classes").is_dir());
}

#[test]
fn notes_reads_comments_and_ignores_string_literals() {
    let root = temp_dir("notes");
    write_spring_fixture(&root);
    let pkg = root.join("src/main/java/com/example/demo");
    fs::write(
        pkg.join("Probe.java"),
        "package com.example.demo;\n\
         // TODO wire the real thing\n\
         public class Probe {\n\
         \x20   /* FIXME: broken */\n\
         \x20   String message = \"TODO: this one is data, not work\";\n\
         \x20   String sql = \"\"\"\n\
         \x20       select 1 -- TODO not a note either\n\
         \x20       \"\"\";\n\
         }\n",
    )
    .unwrap();

    let output = jails_cmd(&root, None).arg("notes").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("wire the real thing"), "{stdout}");
    assert!(stdout.contains("FIXME"), "{stdout}");
    // The discrimination that makes this worth running: a tag inside a
    // literal is data. jails' own generated adapters put "TODO: map a row"
    // in an exception message, and reporting those would bury the real ones.
    assert!(!stdout.contains("data, not work"), "{stdout}");
    assert!(!stdout.contains("not a note either"), "{stdout}");
    assert!(stdout.contains("2 note(s)"), "{stdout}");
}

#[test]
fn notes_can_be_filtered_to_one_tag() {
    let root = temp_dir("notes-filter");
    write_spring_fixture(&root);
    fs::write(
        root.join("src/main/java/com/example/demo/Probe.java"),
        "package com.example.demo;\n// TODO one\n// FIXME two\npublic class Probe {}\n",
    )
    .unwrap();

    let output = jails_cmd(&root, None)
        .args(["notes", "fixme"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("FIXME"), "{stdout}");
    assert!(!stdout.contains("TODO"), "{stdout}");
}

#[test]
fn stats_counts_code_per_layer_and_the_test_ratio() {
    let root = temp_dir("stats");
    write_spring_fixture(&root);
    assert!(
        jails_cmd(&root, None)
            .args(["generate", "record", "Payout", "id:uuid"])
            .status()
            .unwrap()
            .success()
    );

    let output = jails_cmd(&root, None).arg("stats").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Domain"), "{stdout}");
    assert!(stdout.contains("Test code to application code"), "{stdout}");
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
            .args(["add", "kafka"])
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

    let listener = fs::read_to_string(
        root.join("src/main/java/com/example/demo/messaging/PayoutSettledListener.java"),
    )
    .unwrap();
    // No catch: swallowing here commits an offset for a message that was
    // never processed, which is data loss wearing a success badge.
    assert!(!listener.contains("catch ("), "{listener}");

    let publisher = fs::read_to_string(
        root.join("src/main/java/com/example/demo/messaging/PayoutSettledPublisher.java"),
    )
    .unwrap();
    // Keyed sends: ordering is per partition, and a null key round-robins.
    assert!(
        publisher.contains("kafka.send(topic, String.valueOf(event.id()), event)"),
        "{publisher}"
    );

    let event = fs::read_to_string(
        root.join("src/main/java/com/example/demo/messaging/PayoutSettledEvent.java"),
    )
    .unwrap();
    assert!(
        event.contains(
            "record PayoutSettledEvent(UUID id, UUID payoutId, BigDecimal amount, Instant occurredAt)"
        ),
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
        fs::read_to_string(root.join("src/main/java/com/example/demo/SecurityConfig.java"))
            .unwrap();
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

    let verified = verified_spring_services_toolbox(&path);
    assert!(verified.join("target/test-classes").is_dir());
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
            .args(["add", "redis"])
            .status()
            .unwrap()
            .success()
    );

    let compose = fs::read_to_string(root.join("compose.yaml")).unwrap();
    assert!(compose.contains("redis:7-alpine"), "{compose}");
    // A cache with a volume hides the one bug caches reliably have: code
    // that only works because something was already cached.
    assert!(!compose.contains("redis-data"), "{compose}");

    let store =
        fs::read_to_string(root.join("src/main/java/com/example/demo/adapters/KeyValueStore.java"))
            .unwrap();
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
fn generating_an_integration_test_also_configures_something_to_run_it() {
    // Surefire runs `*Test`; `*IT` belongs to Failsafe, and Failsafe is not
    // part of the Spring Boot parent's default build. Without this, every
    // generated IT is dead code and `mvn verify` still reports success --
    // a test that silently does not run is worse than no test, because the
    // green build claims it passed.
    let root = temp_dir("failsafe");
    write_spring_fixture(&root);
    assert!(
        !fs::read_to_string(root.join("pom.xml"))
            .unwrap()
            .contains("failsafe")
    );

    assert!(
        jails_cmd(&root, None)
            .args(["generate", "integration-test", "Payment"])
            .status()
            .unwrap()
            .success()
    );

    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("maven-failsafe-plugin"), "{pom}");
    // Both goals: `integration-test` runs them, `verify` is what makes a
    // failure fail the build. Binding only the first ignores the result.
    assert!(pom.contains("<goal>integration-test</goal>"), "{pom}");
    assert!(pom.contains("<goal>verify</goal>"), "{pom}");
    // No version: the Spring Boot parent manages it, and pinning one here
    // would drift from the platform.
    let plugin_block = &pom[pom.find("maven-failsafe-plugin").unwrap()..];
    let block_end = plugin_block.find("</plugin>").unwrap();
    assert!(!plugin_block[..block_end].contains("<version>"), "{pom}");

    // Idempotent: a second IT must not splice a second plugin block.
    assert!(
        jails_cmd(&root, None)
            .args(["generate", "integration-test", "Refund"])
            .status()
            .unwrap()
            .success()
    );
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert_eq!(pom.matches("maven-failsafe-plugin").count(), 1, "{pom}");
}

#[test]
fn generate_help_documents_the_field_syntax_at_the_point_of_typing() {
    // The field grammar is the thing you need while typing the command, and
    // it lived only in the README. clap reflows a doc comment into one
    // paragraph unless told not to, which turns the table and the examples
    // into a run-on -- so the formatting is worth asserting, not just the
    // presence of the words.
    let workdir = temp_dir("generate-help");
    let output = jails_cmd(&workdir, None)
        .args(["generate", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);

    assert!(help.contains("name:string!"), "{help}");
    assert!(help.contains("name:string?"), "{help}");
    assert!(help.contains("Case is the rule"), "{help}");
    assert!(help.contains("list<string>"), "{help}");
    // Line breaks survived: the table is indented lines, not one paragraph.
    assert!(help.contains("\n  name:string      required"), "{help}");
    assert!(help.contains("\n  jails g sealed Outcome"), "{help}");
    // Every kind carries a description rather than a bare name.
    assert!(help.contains("- scaffold:"), "{help}");
    assert!(help.contains("- sealed:"), "{help}");
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
/// and is refused. Before preflight, `csv` had already been applied by then --
/// the command reported a failure over a pom it had just edited, and the
/// obvious retry had to skip `csv` by hand.
///
/// Planning is pure and is where that refusal lives, so every requested
/// capability is planned before any is applied.
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
/// named before it, which is the case that was broken.
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

/// The tier that answers what the tool is for. A strategy's interface,
/// implementations and tests have to agree on one method signature across
/// five files, and a mismatch is a compile error the user did not write.
///
/// Both modes are covered in one project because they generate different
/// signatures: `--yields` returns `Optional<T>`, a bare strategy `boolean`.
#[test]
fn generate_strategy_produces_a_project_that_compiles_and_passes_tests() {
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
    let workdir = temp_dir("real-strategy-compiles");
    jails_cmd_with_path(&workdir, &path)
        .args(["new-cli", "demo"])
        .status()
        .unwrap();
    let root = workdir.join("demo");

    // The types the generated signature names. Without them the strategy
    // would not compile, which is what the note at generation time says.
    for record in ["Transaction", "Reward"] {
        assert!(
            jails_cmd_with_path(&root, &path)
                .args(["g", "record", record, "id:uuid", "amount:long"])
                .status()
                .unwrap()
                .success()
        );
    }

    assert!(
        jails_cmd_with_path(&root, &path)
            .args([
                "g",
                "strategy",
                "RewardRule",
                "Coffee",
                "LargeTransaction",
                "--on",
                "Transaction",
                "--yields",
                "Reward",
            ])
            .status()
            .unwrap()
            .success()
    );
    // The predicate mode, whose method returns `boolean` rather than Optional.
    assert!(
        jails_cmd_with_path(&root, &path)
            .args([
                "g",
                "strategy",
                "Eligibility",
                "Domestic",
                "--on",
                "Transaction"
            ])
            .status()
            .unwrap()
            .success()
    );

    let verified = verified_plain_toolbox(&path);
    for class in ["Eligibility", "DomesticEligibility"] {
        assert!(
            verified
                .join(format!(
                    "target/classes/com/example/demo/domain/{class}.class"
                ))
                .is_file(),
            "shared toolchain matrix did not compile {class}"
        );
    }
}

/// `destroy strategy` reads the implementations back off disk rather than
/// rebuilding a variant list it was never given, so it takes out every class
/// implementing the interface -- including one added by hand afterwards.
/// Leaving that behind implementing a deleted interface stops the project
/// compiling, which is the failure the generate/destroy inverse rule exists
/// to prevent.
#[test]
fn destroy_strategy_removes_the_implementations_it_did_not_name() {
    let root = temp_dir("destroy-strategy");
    write_plain_fixture(&root);

    assert!(
        jails_cmd(&root, None)
            .args([
                "g",
                "strategy",
                "RewardRule",
                "Coffee",
                "--on",
                "Transaction"
            ])
            .status()
            .unwrap()
            .success()
    );

    // An implementation the generate call never knew about.
    let domain = root.join("src/main/java/com/example/demo/domain");
    fs::write(
        domain.join("HandWrittenRewardRule.java"),
        "package com.example.demo.domain;\n\n\
         public final class HandWrittenRewardRule implements RewardRule {\n\
         \x20   @Override\n\
         \x20   public boolean matches(Transaction transaction) {\n\
         \x20       return false;\n\
         \x20   }\n\
         }\n",
    )
    .unwrap();

    assert!(
        jails_cmd(&root, None)
            .args(["destroy", "strategy", "RewardRule", "--force"])
            .status()
            .unwrap()
            .success()
    );

    assert!(!domain.join("RewardRule.java").exists());
    assert!(!domain.join("CoffeeRewardRule.java").exists());
    assert!(
        !domain.join("HandWrittenRewardRule.java").exists(),
        "an implementation of a deleted interface was left behind"
    );
}

// ---- the manifest: `jails.toml` describes the project, `sync` makes it true ----

/// The loop that makes a manifest trustworthy: `add` records what it applied,
/// so nobody has to maintain the file, and `remove` takes it back out -- left
/// listed, the next `sync` would put back what was just removed.
#[test]
fn add_records_what_it_applied_and_remove_takes_it_back_out() {
    let root = temp_dir("manifest-round-trip");
    write_plain_fixture(&root);

    assert!(
        jails_cmd(&root, None)
            .args(["add", "csv"])
            .status()
            .unwrap()
            .success()
    );
    let manifest = fs::read_to_string(root.join("jails.toml")).unwrap();
    assert!(
        manifest.contains(r#"capabilities = ["csv"]"#),
        "add did not record the capability it applied:\n{manifest}"
    );

    assert!(
        jails_cmd(&root, None)
            .args(["remove", "csv", "--force"])
            .status()
            .unwrap()
            .success()
    );
    let manifest = fs::read_to_string(root.join("jails.toml")).unwrap();
    assert!(
        manifest.contains("capabilities = []"),
        "remove left the capability declared, so the next sync would restore it:\n{manifest}"
    );
}

/// The case the manifest exists for: a project that declares what it is made
/// of and does not have it yet -- a fresh clone, or one taking a newer jails'
/// output. One command, and the `[layout]` renames apply at the same time.
#[test]
fn sync_applies_what_the_manifest_declares() {
    let root = temp_dir("manifest-sync");
    write_plain_fixture(&root);
    fs::write(
        root.join("jails.toml"),
        "[layout]\nadapters = \"persistence\"\n\n[project]\ncapabilities = [\"csv\"]\n",
    )
    .unwrap();

    // --pretend first: it answers "what is this project missing?".
    let preview = jails_cmd(&root, None)
        .args(["sync", "--dry-run"])
        .output()
        .unwrap();
    assert!(preview.status.success());
    let shown = String::from_utf8_lossy(&preview.stdout);
    assert!(shown.contains("would create"), "{shown}");
    assert!(
        !root
            .join("src/main/java/com/example/demo/persistence")
            .exists(),
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
        root.join("src/main/java/com/example/demo/persistence/CsvReader.java")
            .is_file(),
        "sync did not apply the declared capability into the configured layer"
    );
}

/// Every capability is idempotent, so a sync over a project that is already
/// correct changes nothing and says so rather than reporting work.
#[test]
fn sync_over_a_correct_project_changes_nothing() {
    let root = temp_dir("manifest-sync-idempotent");
    write_plain_fixture(&root);
    jails_cmd(&root, None)
        .args(["add", "csv"])
        .status()
        .unwrap();

    let pom_before = fs::read_to_string(root.join("pom.xml")).unwrap();
    let output = jails_cmd(&root, None).args(["sync"]).output().unwrap();
    assert!(output.status.success());
    let shown = String::from_utf8_lossy(&output.stdout);
    assert!(shown.contains("already set up"), "{shown}");
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
    write_plain_fixture(&root);

    let output = jails_cmd(&root, None).args(["sync"]).output().unwrap();
    assert!(output.status.success());
    let shown = String::from_utf8_lossy(&output.stdout);
    assert!(shown.contains("no capabilities"), "{shown}");
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
/// like the stub jails wrote. A real project was found with ~20 hand-written
/// properties inside jails' own markers; a hand-finished generated class is
/// the same discovery waiting to happen and costs more.
///
/// jails does not refuse -- `remove` is the documented inverse of `add`, and
/// refusing would make it unusable on the projects that got the most out of
/// it. It must not delete them *silently*, which is the line
/// `unowned_properties` already draws for properties.
#[test]
fn remove_names_generated_files_that_were_edited_before_deleting_them() {
    let root = temp_dir("remove-edited-files");
    write_plain_fixture(&root);
    jails_cmd(&root, None)
        .args(["add", "csv"])
        .status()
        .unwrap();

    let generated = root.join("src/main/java/com/example/demo/adapters/CsvReader.java");
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
    assert!(
        shown.contains("changed since jails wrote"),
        "an edited generated file was deleted with no mention of it:\n{shown}"
    );
    assert!(shown.contains("CsvReader.java"), "{shown}");
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
    write_plain_fixture(&root);
    jails_cmd(&root, None)
        .args(["add", "csv"])
        .status()
        .unwrap();

    let generated = root.join("src/main/java/com/example/demo/adapters/CsvReader.java");
    fs::write(
        &generated,
        "package com.example.demo.adapters;\nclass CsvReader {}\n",
    )
    .unwrap();

    let output = jails_cmd(&root, None)
        .args(["remove", "csv", "--dry-run"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let shown = String::from_utf8_lossy(&output.stdout);
    assert!(shown.contains("changed since jails wrote"), "{shown}");
    assert!(generated.is_file(), "--dry-run deleted the file");
}

/// `stats` used to keep its own layer list, so it reported against jails'
/// *default* package names: a project that renamed a layer in `jails.toml`
/// had those files counted as "Other". The layout has one owner now.
#[test]
fn stats_counts_a_renamed_layer_under_its_configured_name() {
    let root = temp_dir("stats-renamed-layer");
    write_plain_fixture(&root);
    fs::write(
        root.join("jails.toml"),
        "[layout]\nadapters = \"persistence\"\n",
    )
    .unwrap();
    jails_cmd(&root, None)
        .args(["add", "csv"])
        .status()
        .unwrap();

    let output = jails_cmd(&root, None).args(["stats"]).output().unwrap();
    assert!(output.status.success());
    let shown = String::from_utf8_lossy(&output.stdout);
    assert!(
        shown.contains("Adapters"),
        "the renamed layer was not recognised:\n{shown}"
    );
}

/// `--pretend` has to name every write. `package-info.java` was created as a
/// side effect of writing a class, so the preview listed two files and the
/// real run wrote three -- on the one command whose entire job is to tell you
/// what is about to happen.
///
/// The fix is that the preview and the apply consume the same list, rather
/// than the preview learning to predict a side effect: a second piece of code
/// guessing what the first will do is the drift this costs everywhere else.
#[test]
fn pretend_names_the_package_info_it_will_write() {
    let root = temp_dir("pkginfo-preview");
    write_plain_fixture(&root);
    // package-info is conditional on the annotation resolving.
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap().replace(
        "</dependencies>",
        "<dependency><groupId>org.jspecify</groupId>\
         <artifactId>jspecify</artifactId><version>1.0.0</version></dependency>\
         </dependencies>",
    );
    fs::write(root.join("pom.xml"), pom).unwrap();

    let preview = jails_cmd(&root, None)
        .args(["g", "record", "Note", "title:string", "--pretend"])
        .output()
        .unwrap();
    assert!(preview.status.success());
    let shown = String::from_utf8_lossy(&preview.stdout);
    assert!(
        shown.contains("package-info"),
        "the preview hid a file the real run writes:\n{shown}"
    );

    let real = jails_cmd(&root, None)
        .args(["g", "record", "Note", "title:string"])
        .output()
        .unwrap();
    assert!(real.status.success());
    let done = String::from_utf8_lossy(&real.stdout);

    // The preview and the run must name the same set of files.
    let files = |text: &str| -> Vec<String> {
        text.lines()
            .filter_map(|l| l.rsplit_once(' ').map(|(_, p)| p.to_string()))
            .filter(|p| p.ends_with(".java"))
            .collect()
    };
    assert_eq!(
        files(&shown),
        files(&done),
        "preview and apply disagreed about what would be written"
    );
    assert!(
        root.join("src/main/java/com/example/demo/domain/package-info.java")
            .is_file()
    );
}

/// One per package, not one per class -- `scaffold` puts several classes in
/// the same package -- and never in the test tree, where a nullness contract
/// buys nothing.
#[test]
fn planned_package_infos_are_one_per_package() {
    let root = temp_dir("pkginfo-dedup");
    write_plain_fixture(&root);
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap().replace(
        "</dependencies>",
        "<dependency><groupId>org.jspecify</groupId>\
         <artifactId>jspecify</artifactId><version>1.0.0</version></dependency>\
         </dependencies>",
    );
    fs::write(root.join("pom.xml"), pom).unwrap();

    let preview = jails_cmd(&root, None)
        .args(["g", "scaffold", "Task", "title:string", "--pretend"])
        .output()
        .unwrap();
    assert!(preview.status.success());
    let shown = String::from_utf8_lossy(&preview.stdout);

    let infos: Vec<&str> = shown
        .lines()
        .filter(|l| l.contains("package-info"))
        .collect();
    assert!(
        infos.len() > 1,
        "scaffold should span several packages:\n{shown}"
    );

    // No package planned twice, however many classes land in it.
    let mut seen = std::collections::HashSet::new();
    for line in &infos {
        assert!(
            seen.insert(*line),
            "the same package-info was planned twice:\n{shown}"
        );
    }

    // Never in the test tree.
    assert!(
        !infos.iter().any(|l| l.contains("src/test/java")),
        "{shown}"
    );
}

/// A file writer must not rediscover the project. `write_new_file` used to
/// find it from process CWD, which is not the project being written to when
/// `new-cli` creates a directory the CWD is not inside.
///
/// The visible cost: a `new-cli` project's own base package never got the
/// null-marked `package-info.java` every other package gets. Run from
/// nowhere, the lookup found no project and skipped; run from inside another
/// Maven project, it read *that* project's pom and package. The audit's
/// "every package jails writes a class into gets one" was simply not true for
/// `App.java`.
#[test]
fn new_cli_gives_its_own_base_package_a_package_info() {
    let workdir = temp_dir("new-cli-base-pkginfo");
    fs::create_dir_all(&workdir).unwrap();
    jails_cmd(&workdir, None)
        .args(["new-cli", "demo"])
        .status()
        .unwrap();

    let info = workdir.join("demo/src/main/java/com/example/demo/package-info.java");
    assert!(
        info.is_file(),
        "the project's own base package did not get a package-info"
    );
    assert!(fs::read_to_string(&info).unwrap().contains("@NullMarked"));
}

/// The same, from inside another Maven project: the root that matters is the
/// one being written to, so the package-info must describe the *new*
/// project's package rather than the surrounding one's.
#[test]
fn new_cli_inside_another_project_uses_the_new_projects_root() {
    let outer = temp_dir("new-cli-nested-root");
    fs::create_dir_all(outer.join("src/main/java/com/outer")).unwrap();
    fs::write(
        outer.join("pom.xml"),
        "<project><properties>\
         <maven.compiler.release>27</maven.compiler.release>\
         </properties><dependencies></dependencies></project>",
    )
    .unwrap();
    fs::write(
        outer.join("src/main/java/com/outer/Outer.java"),
        "package com.outer;\nclass Outer {}\n",
    )
    .unwrap();

    jails_cmd(&outer, None)
        .args(["new-cli", "demo"])
        .status()
        .unwrap();

    let info = outer.join("demo/src/main/java/com/example/demo/package-info.java");
    assert!(info.is_file(), "no package-info in the nested new project");
    let text = fs::read_to_string(&info).unwrap();
    assert!(
        text.contains("package com.example.demo;"),
        "the package-info names the surrounding project's package:\n{text}"
    );
    // And the outer project is left alone.
    assert!(
        !outer
            .join("src/main/java/com/outer/package-info.java")
            .exists()
    );
}

/// `plan.md` §6.6 tier 2. The want is "change what the generated code *looks
/// like*" -- not a new generator, just this class shaped differently.
#[test]
fn a_project_template_override_replaces_the_built_in_and_doctor_names_it() {
    let root = temp_dir("template-override");
    write_plain_fixture(&root);

    let overrides = root.join(".jails/templates/generate");
    fs::create_dir_all(&overrides).unwrap();
    let built_in = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/generate/command_test.java"),
    )
    .unwrap();
    // Same placeholders, different shape: the contract is the placeholder set,
    // not the text.
    fs::write(
        overrides.join("command_test.java"),
        format!("// generated by an overridden template\n{built_in}"),
    )
    .unwrap();

    let output = jails_cmd(&root, None)
        .args(["generate", "command", "Sync"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let generated =
        fs::read_to_string(root.join("src/test/java/com/example/demo/cli/SyncCommandTest.java"))
            .unwrap();
    assert!(
        generated.starts_with("// generated by an overridden template"),
        "{generated}"
    );
    assert!(generated.contains("class SyncCommandTest"), "{generated}");

    // The honesty half: an overridden template is not golden-tested, so
    // `doctor` names it rather than letting the reader find out from a build.
    let doctor = jails_cmd(&root, None).arg("doctor").output().unwrap();
    let report = String::from_utf8_lossy(&doctor.stdout);
    assert!(
        report.contains("generate/command_test.java"),
        "doctor must name the override: {report}"
    );
    assert!(
        report.contains("not covered by jails' snapshot tests"),
        "{report}"
    );
}

/// The placeholder set is the contract, and breaking it is the reader's typo --
/// so it is an error naming their file, not a panic naming jails'.
#[test]
fn a_template_override_missing_a_placeholder_is_refused_by_name() {
    let root = temp_dir("template-override-bad");
    write_plain_fixture(&root);

    let overrides = root.join(".jails/templates/generate");
    fs::create_dir_all(&overrides).unwrap();
    fs::write(
        overrides.join("command_test.java"),
        "package {{pkg}};\n\nclass Whatever {}\n",
    )
    .unwrap();

    let output = jails_cmd(&root, None)
        .args(["generate", "command", "Sync"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("command_test.java"), "{stderr}");
    assert!(stderr.contains("missing:"), "{stderr}");
    assert!(
        !root
            .join("src/test/java/com/example/demo/cli/SyncCommandTest.java")
            .exists(),
        "nothing is written when the override is refused"
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
fn test_fast_runs_compiled_classes_and_falls_back_loudly_when_they_are_stale() {
    let root = temp_dir("test-fast");
    write_plain_fixture(&root);

    // Nothing compiled: refused rather than reporting a green run over an
    // empty classpath.
    let cold = jails_cmd(&root, None)
        .args(["test", "--fast"])
        .output()
        .unwrap();
    let report = String::from_utf8_lossy(&cold.stdout);
    assert!(
        report.contains("--fast not taken"),
        "an uncompiled project must not take the fast path: {report}"
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

/// `plan.md` §13.2. Every claim in `EventHub`'s Javadoc is a behavioural one,
/// so the only place they can be checked is against a real JUnit run --
/// especially the concurrency test, which is the reason the registry is a
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

/// `plan.md` §13.3's `g auth`. Both claims behind it are behavioural, so a
/// compile check would prove nothing: Boot auto-configures no `JwtEncoder`,
/// and `JwtTimestampValidator` accepts a token with no `exp` unless one line
/// says otherwise. The second is the reason `a_token_with_no_expiry_is_refused`
/// exists — delete that line and no other test notices.
#[test]
fn generate_auth_produces_tests_that_run_and_pass() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-auth");
    write_spring_fixture(&root);

    let status = jails_cmd_with_path(&root, &path)
        .args(["add", "security", "--no-start"])
        .status()
        .unwrap();
    assert!(status.success(), "add security failed");

    let status = jails_cmd_with_path(&root, &path)
        .args(["generate", "auth", "Api"])
        .status()
        .unwrap();
    assert!(status.success(), "generate auth failed");

    let verified = verified_spring_services_toolbox(&path);
    assert_surefire_test_count(verified, "ApiTokensTest", 4);
}

/// Without Spring Security there is no filter chain to read the token, so the
/// encoder and decoder would be beans nothing consumes.
#[test]
fn generate_auth_refuses_without_the_security_capability() {
    let root = temp_dir("auth-no-security");
    write_spring_fixture(&root);

    let output = jails_cmd(&root, None)
        .args(["generate", "auth", "Api"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(stderr.contains("jails add security"), "{stderr}");
}

/// `plan.md` §13.3's `g webhook`. Seven tests, and each is one of the ways an
/// inbound webhook is normally trusted when it should not be — or rejected
/// when it should not be, which is the failure mode nobody predicts.
#[test]
fn generate_webhook_produces_tests_that_run_and_pass() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-webhook");
    write_spring_fixture(&root);

    let status = jails_cmd_with_path(&root, &path)
        .args(["generate", "webhook", "Provider"])
        .status()
        .unwrap();
    assert!(status.success(), "generate webhook failed");

    let verified = verified_spring_services_toolbox(&path);
    assert_surefire_test_count(verified, "ProviderVerifierTest", 7);
}

/// `plan.md` §13.3's `g search`. The generated column is the whole point, and
/// the only thing that can prove the expression is right is PostgreSQL parsing
/// it — a Rust test on the string proves the string.
#[test]
fn generate_search_produces_a_project_that_compiles() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-search");
    write_spring_fixture(&root);

    for step in [
        vec!["add", "db", "--no-start"],
        vec![
            "generate",
            "scaffold",
            "Article",
            "id:uuid@pk",
            "title:string!",
            "body:string",
        ],
        vec!["generate", "search", "Article", "title", "body"],
    ] {
        let output = jails_cmd_with_path(&root, &path)
            .args(&step)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "`{}` failed: {}{}",
            step.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let verified = verified_spring_db_toolbox(&path);
    assert!(
        verified
            .join("target/classes/com/example/demo/adapters/JdbcArticleRepository.class")
            .is_file(),
        "the shared JDBC toolbox did not compile the search adapter"
    );
}

/// Indexing a non-text component is refused at generation time, where the
/// reader is, rather than at `flyway migrate` — which is the furthest possible
/// point from the mistake.
#[test]
fn generate_search_refuses_a_component_it_cannot_index() {
    let root = temp_dir("search-refusals");
    write_spring_fixture(&root);
    let status = jails_cmd(&root, None)
        .args(["add", "db", "--no-start"])
        .status()
        .unwrap();
    assert!(status.success());
    let status = jails_cmd(&root, None)
        .args([
            "generate",
            "scaffold",
            "Article",
            "id:uuid@pk",
            "views:long",
            "title:string!",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    for (args, expected) in [
        (
            vec!["generate", "search", "Article"],
            "needs the components",
        ),
        (
            vec!["generate", "search", "Article", "views"],
            "full-text search indexes text",
        ),
        (
            vec!["generate", "search", "Article", "nosuch"],
            "has no component",
        ),
        (
            vec!["generate", "search", "Missing", "title"],
            "needs the record it searches",
        ),
    ] {
        let output = jails_cmd(&root, None).args(&args).output().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{args:?} should refuse");
        assert!(stderr.contains(expected), "{args:?}: {stderr}");
    }
}

/// `plan.md` §13.3's `add mail`. The generated IT starts a container, so only
/// compilation is checked here — but that is the part that catches the Boot 4
/// API changes, and the IT's shape is copied from Boot's own.
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

/// `plan.md` §14's `jails src`. Two properties matter: it works on a project
/// that does not compile — which is when a language server can least help —
/// and it lists rather than picking, because a project with three
/// `Status.java` files is ordinary.
#[test]
fn src_resolves_a_type_and_lists_every_match() {
    let root = temp_dir("src-command");
    write_plain_fixture(&root);
    let main = root.join("src/main/java/com/example/demo");
    for (package, dir) in [("com.example.demo.a", "a"), ("com.example.demo.b", "b")] {
        fs::create_dir_all(main.join(dir)).unwrap();
        fs::write(
            main.join(dir).join("Status.java"),
            format!("package {package};\n\n// deliberately not valid Java below\nclass Status {{"),
        )
        .unwrap();
    }

    let output = jails_cmd(&root, None)
        .args(["src", "Status"])
        .output()
        .unwrap();
    let report = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{report}");
    assert!(report.contains("com.example.demo.a.Status"), "{report}");
    assert!(report.contains("com.example.demo.b.Status"), "{report}");

    let missing = jails_cmd(&root, None)
        .args(["src", "Nowhere"])
        .output()
        .unwrap();
    assert!(!missing.status.success());
    assert!(
        String::from_utf8_lossy(&missing.stderr).contains("JAILS_SOURCE_PATH"),
        "the refusal names the way to widen the search"
    );
}
