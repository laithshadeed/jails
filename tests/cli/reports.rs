//! The read-only commands: `about`, `routes`, `beans`, `doctor`, `why`,
//! `notes`, `stats`, `src`, `lint`, `rename` and `completion`. None of them
//! start anything, so each is a fixture on disk and one assertion on stdout.

use super::*;

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
        fs::create_dir_all(common::generated(&module_root, "src/main/java/dev/example")).unwrap();
        fs::write(
            common::generated(
                &module_root,
                "src/main/java/dev/example/DemoApplication.java",
            ),
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
    assert!(json.contains("\"schema_version\": 4"));
    assert!(json.contains("\"reactor\":"));
    // Named for the job rather than for Maven, and stating which build it is,
    // so a Gradle project's key does not hold a path to `gradlew` under a
    // Maven name.
    assert!(json.contains("\"build\": \"Maven\""), "{json}");
    assert!(json.contains("\"build_command\":"), "{json}");
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
    assert!(String::from_utf8_lossy(&output.stderr).contains("is not a project"));
}

/// `kind` is a `clap::ValueEnum` and the `g`/`d` aliases are `visible_alias`,
/// so `jails g <TAB>` offers the artifact-kind list: a plain `String` has
/// nothing to complete but filenames, and a hidden `alias` is invisible to
/// clap_complete's bash generator.
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

#[test]
fn lint_reports_closed_set_stale_apis_as_file_and_line() {
    let root = temp_dir("lint-stale-api");
    write_project_skeleton(&root);
    let source = common::generated(&root, "src/main/java/com/example/demo/Legacy.java");
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

/// `jails add <TAB>` only offers completions because `Capability` is a
/// `clap::ValueEnum` and the alias is `visible_alias` -- a hidden `alias` is
/// invisible to clap_complete's bash generator. This guards both.
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
        stdout.starts_with(r#"{"schema_version":3,"evidence":{"kind":"static-inference"#),
        "{stdout}"
    );
    assert!(stdout.contains(r#""limitations":["#), "{stdout}");
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
        stdout.starts_with(r#"{"schema_version":3,"evidence":{"kind":"static-inference"#),
        "{stdout}"
    );
    assert!(stdout.contains(r#""limitations":["#), "{stdout}");
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
fn doctor_reports_missing_and_changed_managed_outputs_with_repair_guidance() {
    let root = temp_dir("doctor-managed-drift");
    write_spring_fixture(&root);
    assert!(
        jails_cmd(&root, None)
            .args(["g", "scaffold", "Note", "id:uuid@pk", "title:string!"])
            .status()
            .unwrap()
            .success()
    );
    let controller = common::generated(
        &root,
        "src/main/java/com/example/demo/web/NoteController.java",
    );
    let service = common::generated(
        &root,
        "src/main/java/com/example/demo/service/NoteService.java",
    );
    fs::remove_file(&controller).unwrap();
    fs::write(
        &service,
        format!(
            "{}\n// changed after generation\n",
            fs::read_to_string(&service).unwrap()
        ),
    )
    .unwrap();

    let output = jails_cmd(&root, None).arg("doctor").output().unwrap();
    // Both rows are warnings, and neither is a failure: `sync` writes a
    // deleted managed file back from the model and merges an edited one
    // forward, so in both cases the project converges on the next run and the
    // reader is entitled to know rather than to be told they broke something.
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let report = String::from_utf8_lossy(&output.stdout);
    assert!(report.contains("managed output"), "{report}");
    assert!(report.contains("NoteController.java deleted"), "{report}");
    // The repair the row names is a jails command, because a deleted managed
    // file is drift `sync` undoes rather than a decision the reader owes.
    assert!(
        report.contains("`jails sync` writes it back from the model"),
        "{report}"
    );
    assert!(report.contains("managed edits"), "{report}");
    assert!(
        report.contains("NoteService.java changed since generation"),
        "{report}"
    );
}

/// A migration written by `jails entity field` carries no renderer stamp,
/// so it is not *managed output*; published schema history is sealed with its
/// content digest, and the seal is what `doctor` checks a deleted or edited
/// migration against.
#[test]
fn doctor_reports_a_sealed_migration_that_was_deleted_or_edited() {
    let root = temp_dir("doctor-migration-seals");
    write_spring_fixture(&root);
    common::declare_storage(&root);
    assert!(
        jails_cmd(&root, None)
            .args(["g", "scaffold", "Task", "id:uuid@pk", "title:string!"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        jails_cmd(&root, None)
            .args(["entity", "field", "add", "Task", "priority:int?"])
            .status()
            .unwrap()
            .success()
    );

    // The question is asked out loud even when the answer is yes: a check that
    // appears only on a fault is one a reader cannot tell was ever run.
    let clean = jails_cmd(&root, None).arg("doctor").output().unwrap();
    let report = String::from_utf8_lossy(&clean.stdout).to_string();
    assert!(report.contains("ok    sealed migrations"), "{report}");

    // The command's own migration, not the scaffold's.
    let evolution = root.join("src/main/resources/db/migration/V002__add_priority_to_tasks.sql");
    let sealed = fs::read(&evolution).unwrap();
    fs::remove_file(&evolution).unwrap();
    let output = jails_cmd(&root, None).arg("doctor").output().unwrap();
    assert!(!output.status.success());
    let report = String::from_utf8_lossy(&output.stdout);
    assert!(report.contains("sealed migrations"), "{report}");
    assert!(
        report.contains("V002__add_priority_to_tasks.sql` is missing"),
        "{report}"
    );
    // Never a repair verb: repair renders the managed tree from the model, and
    // schema history is not in it. Restoring the file is the only answer that
    // keeps a database that already ran it described by what is on disk.
    assert!(
        report.contains("restore the file from version control"),
        "{report}"
    );

    // An edit is a different fact from a deletion, and must not be answered
    // with a command that silently discards it.
    fs::write(&evolution, sealed).unwrap();
    let created = root.join("src/main/resources/db/migration/V001__create_tasks.sql");
    fs::write(&created, b"-- corrected by hand\nselect 1;\n").unwrap();
    let output = jails_cmd(&root, None).arg("doctor").output().unwrap();
    assert!(!output.status.success());
    let report = String::from_utf8_lossy(&output.stdout);
    assert!(
        report.contains("V001__create_tasks.sql` differs from the bytes jails published"),
        "{report}"
    );
    assert!(report.contains("append-only"), "{report}");
}

/// Whether this filesystem and user honour a read-only directory at all.
fn a_read_only_directory_refuses_a_write(under: &std::path::Path) -> bool {
    let probe = under.join("readonly-probe");
    fs::create_dir_all(&probe).unwrap();
    let mut mode = fs::metadata(&probe).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut mode, 0o555);
    fs::set_permissions(&probe, mode).unwrap();
    let refused = fs::write(probe.join("x"), b"x").is_err();
    let mut mode = fs::metadata(&probe).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut mode, 0o755);
    fs::set_permissions(&probe, mode).unwrap();
    fs::remove_dir_all(&probe).unwrap();
    refused
}

/// Make one path unwritable and the write phase cannot finish.
///
/// The executor has no half-applied state to report: every write is staged
/// under `.jails-staged-` and published under the lock, and a run that cannot
/// finish leaves the project exactly as it was. The reader fixes the
/// permission and runs the command again; the sweep removes its own debris
/// and the transition converges.
#[test]
fn an_unwritable_path_leaves_the_project_exactly_as_it_was() {
    let root = temp_dir("doctor-interrupted");
    if !a_read_only_directory_refuses_a_write(&root) {
        // Root ignores the mode bits, and so do some filesystems. Probing is
        // the honest test: asserting on the uid would claim to know why.
        //
        // Not `skip`: there is nothing to install here, so
        // `JAILS_TOOLCHAIN` must not turn it into a failure. See
        // `skip_unsupported_environment`.
        common::skip_unsupported_environment("this user can write into a read-only directory");
        return;
    }
    write_spring_fixture(&root);
    common::declare_storage(&root);
    assert!(
        jails_cmd(&root, None)
            .args(["g", "scaffold", "Task", "id:uuid@pk", "title:string!"])
            .status()
            .unwrap()
            .success()
    );
    let migrations = root.join("src/main/resources/db/migration");
    let before = snapshot_tree(&root);

    let sealed = fs::metadata(&migrations).unwrap().permissions();
    let mut locked = sealed.clone();
    std::os::unix::fs::PermissionsExt::set_mode(&mut locked, 0o555);
    fs::set_permissions(&migrations, locked).unwrap();
    let torn = jails_cmd(&root, None)
        .args(["entity", "field", "add", "Task", "priority:int?"])
        .output()
        .unwrap();
    assert!(!torn.status.success(), "the write was not refused");

    // Nothing published, including the model: the authoring source is written
    // by the same plan, so a run that cannot append its migration cannot claim
    // the column either.
    fs::set_permissions(&migrations, sealed).unwrap();
    assert_eq!(snapshot_tree(&root), before);

    let output = jails_cmd(&root, None).arg("doctor").output().unwrap();
    let report = String::from_utf8_lossy(&output.stdout);
    assert!(!report.contains("did not finish"), "{report}");
    assert!(report.contains("ok    sealed migrations"), "{report}");

    // And the same command, run again, converges.
    let again = jails_cmd(&root, None)
        .args(["entity", "field", "add", "Task", "priority:int?"])
        .output()
        .unwrap();
    assert!(again.status.success(), "{again:?}");
    assert!(
        root.join("src/main/resources/db/migration/V002__add_priority_to_tasks.sql")
            .exists(),
        "the retried transition did not publish its migration"
    );
    let cleared = jails_cmd(&root, None).arg("doctor").output().unwrap();
    let report = String::from_utf8_lossy(&cleared.stdout);
    // Named checks rather than the exit status, for the reason the sibling
    // test above records: a fixture that declares a compose service and starts
    // nothing makes `doctor`'s status a fact about the machine.
    assert!(!report.contains("did not finish"), "{report}");
    assert!(report.contains("ok    sealed migrations"), "{report}");
    assert!(report.contains("ok    managed output"), "{report}");
    assert!(report.contains("ok    model accepted"), "{report}");
}

/// A generated `@Disabled` test is honest about what it does not prove and
/// completely silent about existing, so `mvn test` reports green over it.
/// Both surfaces say so -- the plan when the file is about to be written, and
/// `doctor` afterwards, because a line in one command's summary scrolls away.
#[test]
fn a_generated_disabled_test_is_named_when_it_is_written_and_afterwards() {
    let root = temp_dir("doctor-disabled-tests");
    write_spring_fixture(&root);
    common::become_canonical(&root);
    // A component whose type jails owns nothing about. It cannot spell a
    // `SourceRef`, so the companion test is emitted whole and `@Disabled`
    // rather than guessed at -- a guess would not compile, and emitting
    // nothing would drop the coverage without saying so.
    let pkg = root.join("src/main/java/com/example/demo");
    fs::create_dir_all(&pkg).unwrap();
    fs::write(
        pkg.join("SourceRef.java"),
        "package com.example.demo;\n\npublic record SourceRef(String value) {}\n",
    )
    .unwrap();

    let planned = jails_cmd(&root, None)
        .args([
            "g",
            "record",
            "Clip",
            "ref:com.example.demo.SourceRef",
            "--pretend",
        ])
        .output()
        .unwrap();
    assert!(planned.status.success(), "{planned:?}");
    let plan = String::from_utf8_lossy(&planned.stdout);
    assert!(plan.contains("test-disabled"), "{plan}");
    assert!(plan.contains("ClipTest.java"), "{plan}");

    assert!(
        jails_cmd(&root, None)
            .args(["g", "record", "Clip", "ref:com.example.demo.SourceRef"])
            .status()
            .unwrap()
            .success()
    );
    let output = jails_cmd(&root, None).arg("doctor").output().unwrap();
    let report = String::from_utf8_lossy(&output.stdout);
    assert!(report.contains("generated tests"), "{report}");
    assert!(report.contains("ClipTest.java"), "{report}");
    // A warning, not a failure: the file is exactly what jails meant to write.
    assert!(output.status.success(), "{report}");
}

/// A migration jails wrote and nobody filled in is applied, checksummed, and
/// never mentioned again -- so the history asserts a change that did not
/// happen. Writing the file is right; jails cannot know the SQL and the value
/// of the command is a correctly numbered file at the right path. Leaving it
/// silent is the defect `doctor` reports.
#[test]
fn doctor_names_a_migration_that_was_written_and_never_filled_in() {
    let root = temp_dir("doctor-empty-migration");
    write_spring_fixture(&root);
    common::declare_storage(&root);
    assert!(
        jails_cmd(&root, None)
            .args(["g", "migration", "add_customer_id_index"])
            .status()
            .unwrap()
            .success()
    );

    let output = jails_cmd(&root, None).arg("doctor").output().unwrap();
    let report = String::from_utf8_lossy(&output.stdout);
    assert!(report.contains("contain no SQL"), "{report}");
    assert!(report.contains("add_customer_id_index.sql"), "{report}");
    // The reader's file to fill in, so a warning and not a failure -- and the
    // assertion is on the check rather than on the exit status, because this
    // fixture declares a compose service and never starts it, and `doctor`
    // exits non-zero when *any* check fails.
    assert!(report.contains("warn  migration bodies"), "{report}");

    let written = root.join("src/main/resources/db/migration");
    let file = fs::read_dir(&written)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|e| e == "sql"))
        .unwrap();
    let text = fs::read_to_string(&file).unwrap();
    fs::write(
        &file,
        format!("{text}create index messages_customer_id_idx on messages (customer_id);\n"),
    )
    .unwrap();
    let output = jails_cmd(&root, None).arg("doctor").output().unwrap();
    let report = String::from_utf8_lossy(&output.stdout);
    assert!(!report.contains("contain no SQL"), "{report}");
}

#[test]
fn doctor_reports_resolved_developer_tool_paths_and_versions() {
    let root = temp_dir("doctor-tools");
    write_project_skeleton(&root);
    fs::write(
        root.join("pom.xml"),
        "<project><properties><maven.compiler.release>26</maven.compiler.release></properties></project>\n",
    )
    .unwrap();
    fs::write(
        common::generated(&root, "src/main/java/com/example/demo/DemoApplication.java"),
        "package com.example.demo;\nimport org.springframework.boot.autoconfigure.SpringBootApplication;\n@SpringBootApplication public class DemoApplication {}\n",
    )
    .unwrap();
    let controller = common::generated(&root, "src/main/java/com/example/demo/NoteController.java");
    fs::write(
        controller,
        "package com.example.demo;\nclass NoteController { @GetMapping(\"/notes\") String get() { return \"ok\"; } }\n",
    )
    .unwrap();
    fs::write(
        root.join("compose.yaml"),
        "services:\n  postgres:\n    image: postgres:17\n",
    )
    .unwrap();

    let tools = temp_dir("doctor-tools-bin");
    let log = tools.join("log.txt");
    write_fake_maven(
        &tools,
        &["curl", "pgcli", "psql", "docker", "java", "jshell", "mvn"],
        &log,
    );
    for (name, output, stderr) in [
        ("curl", "curl 8.17.0", false),
        ("pgcli", "pgcli 4.3.0", false),
        ("psql", "psql (PostgreSQL) 17.6", false),
        ("docker", "Docker Compose version v5.0.0", false),
        ("java", "openjdk version \"26.0.2\"", true),
        ("jshell", "jshell 26.0.2", false),
        ("mvn", "Apache Maven 3.9.11", false),
    ] {
        fs::write(
            tools.join(name),
            format!(
                "#!/bin/sh\necho \"$0 $*\" >> \"{}\"\necho '{}' {}\nexit 0\n",
                log.display(),
                output,
                if stderr { ">&2" } else { "" }
            ),
        )
        .unwrap();
    }

    let output = jails_cmd(&root, Some(&tools))
        .env_remove("JAVA_HOME")
        .arg("doctor")
        .output()
        .unwrap();
    let report = String::from_utf8_lossy(&output.stdout);
    let canonical = tools.canonicalize().unwrap();
    for (name, version) in [
        ("curl", "8.17.0"),
        ("pgcli", "4.3.0"),
        ("psql", "17.6"),
        ("docker", "v5.0.0"),
        ("java", "26.0.2"),
        ("jshell", "26.0.2"),
        ("mvn", "3.9.11"),
    ] {
        let path = canonical.join(name).display().to_string();
        assert!(report.contains(&path), "missing path `{path}`:\n{report}");
        assert!(
            report.contains(version),
            "missing version `{version}`:\n{report}"
        );
    }
    assert!(read_log(&log).contains("docker compose version"));
}

/// **A probe that answers with an error is not `ok`.** Debian's `psql` is a
/// wrapper that picks a cluster binary, and with no cluster installed it
/// prints `Can't exec "--version"` and exits 0 -- which doctor reported as
/// `ok psql executable ... Can't exec "--version"`, an error message sitting
/// in the column a reader scans for a version number.
#[test]
fn doctor_warns_when_a_tool_answers_the_version_probe_with_an_error() {
    let root = temp_dir("doctor-broken-probe");
    write_project_skeleton(&root);
    fs::write(
        root.join("pom.xml"),
        "<project><properties><maven.compiler.release>26</maven.compiler.release></properties></project>\n",
    )
    .unwrap();
    // The PostgreSQL client rows are only asked for when the project declares
    // a database to talk to.
    fs::write(
        root.join("compose.yaml"),
        "services:\n  postgres:\n    image: postgres:17\n",
    )
    .unwrap();

    let tools = temp_dir("doctor-broken-probe-bin");
    let log = tools.join("log.txt");
    write_fake_maven(&tools, &["pgcli", "psql", "mvn"], &log);
    for (name, output) in [
        ("pgcli", "pgcli 4.3.0"),
        // Exit 0, no digit anywhere: the tool ran and could not answer.
        ("psql", "Cannot exec the requested version"),
        ("mvn", "Apache Maven 3.9.11"),
    ] {
        fs::write(
            tools.join(name),
            format!(
                "#!/bin/sh\necho \"$0 $*\" >> \"{}\"\necho '{}'\nexit 0\n",
                log.display(),
                output
            ),
        )
        .unwrap();
    }

    let output = jails_cmd(&root, Some(&tools))
        .args(["doctor", "--json"])
        .output()
        .unwrap();
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("doctor --json is JSON");
    let row = report["checks"]
        .as_array()
        .expect("checks is an array")
        .iter()
        .find(|check| check["title"] == "psql executable")
        .expect("a psql row");
    assert_eq!(row["status"], "warn", "{row}");
    assert!(
        row["detail"]
            .as_str()
            .unwrap()
            .contains("Cannot exec the requested version"),
        "the row should carry what the probe answered: {row}"
    );
    assert!(
        !row["fix"].as_str().unwrap_or_default().is_empty(),
        "a warn row names a fix: {row}"
    );
}

#[test]
fn doctor_reports_the_system_gradle_path_and_version_when_no_wrapper_exists() {
    let root = temp_dir("doctor-gradle-tool");
    write_project_skeleton(&root);
    fs::remove_file(root.join("pom.xml")).unwrap();
    fs::write(
        root.join("build.gradle"),
        "plugins { id 'java' }\nsourceCompatibility = 26\n",
    )
    .unwrap();
    let tools = temp_dir("doctor-gradle-tool-bin");
    let log = tools.join("log.txt");
    write_fake_maven(&tools, &["java", "gradle"], &log);
    fs::write(
        tools.join("java"),
        "#!/bin/sh\necho 'openjdk version \"26.0.2\"' >&2\nexit 0\n",
    )
    .unwrap();
    fs::write(
        tools.join("gradle"),
        "#!/bin/sh\necho 'Gradle 9.1.0'\nexit 0\n",
    )
    .unwrap();

    let output = jails_cmd(&root, Some(&tools))
        .env_remove("JAVA_HOME")
        .arg("doctor")
        .output()
        .unwrap();
    let report = String::from_utf8_lossy(&output.stdout);
    let gradle = tools.canonicalize().unwrap().join("gradle");
    assert!(report.contains(&gradle.display().to_string()), "{report}");
    assert!(report.contains("Gradle 9.1.0"), "{report}");
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
fn why_bean_emits_a_source_bounded_cause_graph() {
    let root = temp_dir("why-bean");
    write_inspectable_project(&root);
    let before = snapshot_tree(&root);

    let output = jails_cmd(&root, None)
        .args(["why", "bean", "OrderService", "--json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.starts_with(r#"{"schema_version":3,"subject":"bean:OrderService""#),
        "{stdout}"
    );
    assert!(
        stdout.contains(r#""subject":"bean:OrderService""#),
        "{stdout}"
    );
    assert!(stdout.contains(r#""kind":"static-inference""#), "{stdout}");
    assert!(stdout.contains(r#""cause_graph":{"nodes":["#), "{stdout}");
    assert!(stdout.contains("no source-visible provider"), "{stdout}");
    assert_eq!(snapshot_tree(&root), before, "why bean wrote project state");
}

#[test]
fn rename_moves_the_type_its_companion_and_every_reference() {
    let root = temp_dir("rename");
    write_inspectable_project(&root);
    let tests = common::generated(&root, "src/test/java/dev/example/shop/domain");
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

    let domain = common::generated(&root, "src/main/java/dev/example/shop/domain");
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
    assert!(common::generated(&root, "src/main/java/dev/example/shop/domain/Order.java").is_file());
}

#[test]
fn notes_reads_comments_and_ignores_string_literals() {
    let root = temp_dir("notes");
    write_spring_fixture(&root);
    let pkg = common::generated(&root, "src/main/java/com/example/demo");
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
        common::generated(&root, "src/main/java/com/example/demo/Probe.java"),
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

/// `stats` reads the layer list through the project's layout, so a layer
/// renamed in `jails.toml` is counted under its own name rather than "Other".
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

/// `jails src`. Two properties matter: it works on a project that does not
/// compile — which is when a language server can least help — and it lists
/// rather than picking, because a project with three `Status.java` files is
/// ordinary.
#[test]
fn src_resolves_a_type_and_lists_every_match() {
    let root = temp_dir("src-command");
    write_plain_fixture(&root);
    let main = common::generated(&root, "src/main/java/com/example/demo");
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

/// **One condition, one refusal.** Standing outside a project is the
/// commonest way to be told no, and it used to be told four ways: two
/// differing lists of build files, a paragraph about the base package, and a
/// missing-model error that never mentioned the directory. The answer is
/// decided in `model_command::root`, which is the one walk, so every command
/// that needs a project prints the same bytes and the same fix.
///
/// `src` is deliberately absent: it requires no build file, so "not a
/// project" is not the answer it owes the reader.
#[test]
fn every_command_that_needs_a_project_refuses_outside_one_in_the_same_words() {
    let workdir = temp_dir("outside-a-project");
    let commands: &[&[&str]] = &[
        &["about"],
        &["routes"],
        &["beans"],
        &["stats"],
        &["notes"],
        &["doctor"],
        &["sync"],
        &["g", "record", "Note"],
        &["add", "db"],
        &["remove", "db"],
        &["model", "status"],
        &["model", "explain"],
        &["test"],
        &["build"],
        &["run"],
        &["check"],
        &["start"],
        &["stop"],
        &["adopt"],
    ];
    let mut seen: Vec<(String, String)> = Vec::new();
    for command in commands {
        let output = jails_cmd(&workdir, None).args(*command).output().unwrap();
        assert!(
            !output.status.success(),
            "`jails {}` succeeded outside a project",
            command.join(" ")
        );
        seen.push((
            command.join(" "),
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }
    let (first, expected) = &seen[0];
    assert_eq!(
        expected.trim_end(),
        "jails: this directory is not a project: jails found no build file here or in any \
         parent directory\n       fix: run this inside a project, or create one with \
         `jails new` or `jails new-cli`",
        "`jails {first}` no longer prints the one refusal"
    );
    for (command, message) in &seen {
        assert_eq!(
            message, expected,
            "`jails {command}` refuses in its own words"
        );
    }
}
