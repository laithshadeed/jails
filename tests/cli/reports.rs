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
    // Named for the job rather than for Maven, and stating which build it is:
    // `maven_command` holding a path to `gradlew` was a lie the JSON repeated
    // to every consumer.
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
    assert!(!output.status.success());
    let report = String::from_utf8_lossy(&output.stdout);
    assert!(report.contains("recorded output"), "{report}");
    assert!(
        report.contains("NoteController.java` is missing"),
        "{report}"
    );
    assert!(report.contains("NoteService.java` changed"), "{report}");
    assert!(
        report.contains("jails resource repair Note --strategy roll-forward"),
        "{report}"
    );
}

/// The verified blind spot in `bugs.md` B5/B14: a migration written by
/// `jails resource field` carries no renderer stamp, so it is not *managed
/// output* and deleting it left `doctor` reporting all clear -- while deleting
/// the neighbouring create migration, written by `g scaffold`, was caught.
/// Published schema history is sealed with its content digest, and the seal is
/// the authority the check was missing.
#[test]
fn doctor_reports_a_sealed_migration_that_was_deleted_or_edited() {
    let root = temp_dir("doctor-migration-seals");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();
    assert!(
        jails_cmd(&root, None)
            .args(["g", "scaffold", "Task", "id:uuid@pk", "title:string!"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        jails_cmd(&root, None)
            .args(["resource", "field", "add", "Task", "priority:int?"])
            .status()
            .unwrap()
            .success()
    );

    let clean = jails_cmd(&root, None).arg("doctor").output().unwrap();
    let report = String::from_utf8_lossy(&clean.stdout).to_string();
    assert!(!report.contains("sealed migration"), "{report}");

    // The command's own migration, not the scaffold's -- the exact file the
    // old check could not see.
    let evolution = root.join("src/main/resources/db/migration/V002__add_priority_to_tasks.sql");
    let sealed = fs::read(&evolution).unwrap();
    fs::remove_file(&evolution).unwrap();
    let output = jails_cmd(&root, None).arg("doctor").output().unwrap();
    assert!(!output.status.success());
    let report = String::from_utf8_lossy(&output.stdout);
    assert!(report.contains("migrations Task"), "{report}");
    assert!(
        report.contains("V002__add_priority_to_tasks.sql` is missing"),
        "{report}"
    );
    assert!(
        report.contains("jails resource repair Task --strategy roll-forward"),
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

/// `bugs.md` B18, as the reproduction that found it: make one path unwritable
/// mid-transaction and the write phase stops half-applied.
///
/// The two answers this pins are the ones that were wrong. `doctor` used to
/// describe the five half-written files as the developer's edits and point at
/// `resource repair --strategy roll-forward`, which adopts them as the
/// recorded truth -- a green `doctor` over a project whose every insert names
/// a column no migration created. There is one fact to report, and the repair
/// verb must decline while it is true.
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

#[test]
fn doctor_names_an_interrupted_transaction_and_repair_declines_to_adopt_it() {
    let root = temp_dir("doctor-interrupted");
    if !a_read_only_directory_refuses_a_write(&root) {
        // Root ignores the mode bits, and so do some filesystems. Probing is
        // the honest test: asserting on the uid would claim to know why.
        //
        // Not `skip`: there is nothing to install here, so
        // `JAILS_REQUIRE_TOOLCHAIN` must not turn it into a failure. See
        // `skip_unsupported_environment`.
        common::skip_unsupported_environment("this user can write into a read-only directory");
        return;
    }
    write_spring_fixture(&root);
    let migrations = root.join("src/main/resources/db/migration");
    fs::create_dir_all(&migrations).unwrap();
    assert!(
        jails_cmd(&root, None)
            .args(["g", "scaffold", "Task", "id:uuid@pk", "title:string!"])
            .status()
            .unwrap()
            .success()
    );

    let sealed = fs::metadata(&migrations).unwrap().permissions();
    let mut locked = sealed.clone();
    std::os::unix::fs::PermissionsExt::set_mode(&mut locked, 0o555);
    fs::set_permissions(&migrations, locked).unwrap();
    let torn = jails_cmd(&root, None)
        .args(["resource", "field", "add", "Task", "priority:int?"])
        .output()
        .unwrap();
    assert!(!torn.status.success(), "the write was not torn");

    let output = jails_cmd(&root, None).arg("doctor").output().unwrap();
    let report = String::from_utf8_lossy(&output.stdout);
    assert!(!output.status.success(), "{report}");
    assert!(report.contains("started and did not finish"), "{report}");
    assert!(report.contains("run the same command again"), "{report}");
    assert!(
        report.contains("Do not run `jails resource repair`"),
        "{report}"
    );

    // The advertised repair used to adopt the half-applied state as the
    // recorded truth. It must not get that far: recovery is attempted first
    // and cannot finish while the path is unwritable, so the command stops
    // with the reason and the project is left exactly as it was.
    let repair = jails_cmd(&root, None)
        .args(["resource", "repair", "Task", "--strategy", "roll-forward"])
        .output()
        .unwrap();
    assert!(!repair.status.success(), "{repair:?}");
    let still = jails_cmd(&root, None).arg("doctor").output().unwrap();
    let report = String::from_utf8_lossy(&still.stdout);
    assert!(report.contains("started and did not finish"), "{report}");

    // Unblocked, the ordinary next command finishes what was interrupted --
    // an unrelated one, because the point is that nobody has to know which
    // command was torn.
    fs::set_permissions(&migrations, sealed).unwrap();
    let again = jails_cmd(&root, None)
        .args(["g", "record", "Note", "body:string!"])
        .output()
        .unwrap();
    assert!(again.status.success(), "{again:?}");
    let said = String::from_utf8_lossy(&again.stdout);
    assert!(said.contains("recovered"), "{said}");
    assert!(
        root.join("src/main/resources/db/migration/V002__add_priority_to_tasks.sql")
            .exists(),
        "the interrupted transaction's migration was not published"
    );
    let cleared = jails_cmd(&root, None).arg("doctor").output().unwrap();
    let report = String::from_utf8_lossy(&cleared.stdout);
    assert!(cleared.status.success(), "{report}");
    assert!(!report.contains("did not finish"), "{report}");
}

/// A generated `@Disabled` test is honest about what it does not prove and
/// completely silent about existing, so `mvn test` reports green over it.
///
/// modern.md §13.8: one real project shipped five of its nine tests disabled,
/// including both controller tests, and passed. `CLAUDE.md` already names this
/// for skipped tier-3 tests; a generated `@Disabled` is the same failure one
/// level down. Both surfaces answer now -- the plan says it when the file is
/// about to be written, and `doctor` keeps saying it afterwards, because a
/// line in one command's summary scrolls away.
#[test]
fn a_generated_disabled_test_is_named_when_it_is_written_and_afterwards() {
    let root = temp_dir("doctor-disabled-tests");
    write_spring_fixture(&root);
    assert!(
        jails_cmd(&root, None)
            .args(["g", "record", "Post", "title:string!"])
            .status()
            .unwrap()
            .success()
    );

    let planned = jails_cmd(&root, None)
        .args([
            "g",
            "strategy",
            "PostRule",
            "Featured",
            "--on",
            "Post",
            "--pretend",
        ])
        .output()
        .unwrap();
    let plan = String::from_utf8_lossy(&planned.stdout);
    assert!(plan.contains("test-disabled"), "{plan}");

    assert!(
        jails_cmd(&root, None)
            .args(["g", "strategy", "PostRule", "Featured", "--on", "Post"])
            .status()
            .unwrap()
            .success()
    );
    let output = jails_cmd(&root, None).arg("doctor").output().unwrap();
    let report = String::from_utf8_lossy(&output.stdout);
    assert!(report.contains("generated tests"), "{report}");
    assert!(report.contains("FeaturedPostRuleTest.java"), "{report}");
    // A warning, not a failure: the file is exactly what jails meant to write.
    assert!(output.status.success(), "{report}");
}

/// A migration jails wrote and nobody filled in is applied, checksummed, and
/// never mentioned again -- so the history asserts a change that did not
/// happen.
///
/// modern.md §13.7: `V003__add_customer_id_index.sql` was one comment line,
/// and `messages.customer_id` had no index. Writing the file is right; jails
/// cannot know the SQL and the value of the command is a correctly numbered
/// file at the right path. Leaving it silent is the defect.
#[test]
fn doctor_names_a_migration_that_was_written_and_never_filled_in() {
    let root = temp_dir("doctor-empty-migration");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();
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
    // The reader's file to fill in, so a warning and not a failure.
    assert!(output.status.success(), "{report}");

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

/// `migrate lint` asked for a manifest to learn one thing -- the dialect --
/// and so refused on every project `jails new` produces.
///
/// The question is answerable from the migrations and the driver the project
/// declares, which is the authority `Project::sql_dialect` already uses.
#[test]
fn migrate_lint_reads_the_migrations_and_the_driver_without_a_manifest() {
    let root = temp_dir("migrate-lint-no-manifest");
    write_spring_fixture(&root);
    let migrations = root.join("src/main/resources/db/migration");
    fs::create_dir_all(&migrations).unwrap();
    assert!(!root.join(".jails/app.toml").exists());

    let clean = jails_cmd(&root, None)
        .args(["migrate", "lint"])
        .output()
        .unwrap();
    assert!(clean.status.success(), "{clean:?}");
    assert!(
        String::from_utf8_lossy(&clean.stdout).contains("no destructive"),
        "{clean:?}"
    );

    fs::write(
        migrations.join("V001__drop_orders.sql"),
        "drop table orders;\n",
    )
    .unwrap();
    let output = jails_cmd(&root, None)
        .args(["migrate", "lint"])
        .output()
        .unwrap();
    let report = String::from_utf8_lossy(&output.stdout);
    assert!(report.contains("destructive"), "{report}");
    assert!(report.contains("V001__drop_orders.sql"), "{report}");
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
fn why_migration_and_query_report_offline_evidence_without_writes() {
    let root = temp_dir("why-sql-subjects");
    write_why_sql_project(&root);
    let before = snapshot_tree(&root);

    let migration = jails_cmd(&root, None)
        .args(["why", "migration", "V001", "--json"])
        .output()
        .unwrap();
    assert!(migration.status.success(), "{:?}", migration);
    let stdout = String::from_utf8_lossy(&migration.stdout);
    assert!(stdout.contains(r#""subject":"migration:V001""#), "{stdout}");
    assert!(stdout.contains("normalized schema object"), "{stdout}");

    let query = jails_cmd(&root, None)
        .args(["why", "query", "FindOrder", "--json"])
        .output()
        .unwrap();
    assert!(query.status.success(), "{:?}", query);
    let stdout = String::from_utf8_lossy(&query.stdout);
    assert!(
        stdout.contains(r#""subject":"query:FindOrder""#),
        "{stdout}"
    );
    assert!(stdout.contains("verified-offline"), "{stdout}");
    assert_eq!(
        snapshot_tree(&root),
        before,
        "why SQL subjects wrote project state"
    );
}

fn write_why_sql_project(root: &Path) {
    write_project_skeleton(root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();
    fs::create_dir_all(root.join("src/main/resources/db/queries")).unwrap();
    fs::write(
        root.join(".jails/app.toml"),
        r#"schema = "jails.app.v1"
[application]
name = "Example"
base_package = "com.example.demo"
java_release = 26
dialect = "postgresql"
[slices.Orders]
[slices.Orders.queries.FindOrder]
source = "src/main/resources/db/queries/FindOrder.sql"
"#,
    )
    .unwrap();
    fs::write(
        root.join("src/main/resources/db/migration/V001__orders.sql"),
        "CREATE TABLE orders (id uuid PRIMARY KEY, title text NOT NULL);\n",
    )
    .unwrap();
    fs::write(
        root.join("src/main/resources/db/queries/FindOrder.sql"),
        "-- jails:name FindOrder\n-- jails:cardinality optional\n-- jails:param id uuid\nSELECT id, title FROM orders WHERE id = :id;\n",
    )
    .unwrap();
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

/// `plan.md` §14's `jails src`. Two properties matter: it works on a project
/// that does not compile — which is when a language server can least help —
/// and it lists rather than picking, because a project with three
/// `Status.java` files is ordinary.
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
