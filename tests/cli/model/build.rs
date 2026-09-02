//! The reader-owned build and configuration files: dependencies, settings,
//! source roots and the marked blocks jails reconciles into them.
//!
use super::*;

#[test]
fn dependency_is_semantic_model_data_and_one_exact_maven_projection() {
    let root = model_project("model-dependency", EMPTY_MODEL);
    fs::write(
        root.join("pom.xml"),
        "<project>\n    <modelVersion>4.0.0</modelVersion>\n    <!-- reader-owned -->\n</project>\n",
    )
    .unwrap();
    let before = snapshot_tree(&root);
    let preview = jails_cmd(&root, None)
        .args([
            "add",
            "dependency",
            "org.jsoup:jsoup",
            "--version",
            "1.18.3",
            "--scope",
            "runtime",
            "--pretend",
        ])
        .output()
        .unwrap();
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    assert_eq!(
        snapshot_tree(&root),
        before,
        "dependency preview wrote files"
    );

    let added = jails_cmd(&root, None)
        .args([
            "add",
            "dependency",
            "org.jsoup:jsoup",
            "--version",
            "1.18.3",
            "--scope",
            "runtime",
        ])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(model.contains("dep org.jsoup:jsoup @id(dep_"), "{model}");
    assert!(model.contains("@version(\"1.18.3\")"), "{model}");
    assert!(model.contains("@scope(runtime)"), "{model}");
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("<!-- reader-owned -->"), "{pom}");
    assert!(pom.contains("<!-- jails:dependencies -->"), "{pom}");
    assert!(pom.contains("<groupId>org.jsoup</groupId>"), "{pom}");
    assert!(pom.contains("<artifactId>jsoup</artifactId>"), "{pom}");
    assert!(pom.contains("<version>1.18.3</version>"), "{pom}");
    assert!(pom.contains("<scope>runtime</scope>"), "{pom}");

    let reapplied = jails_cmd(&root, None)
        .args([
            "--output",
            "json",
            "add",
            "dependency",
            "org.jsoup:jsoup",
            "--version",
            "1.18.3",
            "--scope",
            "runtime",
        ])
        .output()
        .unwrap();
    assert!(reapplied.status.success());
    let execution: serde_json::Value = serde_json::from_slice(&reapplied.stdout).unwrap();
    assert_eq!(execution["files_written"], 0);

    let removed = jails_cmd(&root, None)
        .args(["remove", "dependency", "org.jsoup:jsoup"])
        .output()
        .unwrap();
    assert!(
        removed.status.success(),
        "{}",
        String::from_utf8_lossy(&removed.stderr)
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(!model.contains("org.jsoup"), "{model}");
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("<!-- reader-owned -->"), "{pom}");
    assert!(!pom.contains("jails:dependencies"), "{pom}");
    assert!(!pom.contains("jsoup"), "{pom}");
    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(
        frozen.status.success(),
        "{}",
        String::from_utf8_lossy(&frozen.stderr)
    );
}

#[test]
fn dependency_reconciliation_crosses_the_kotlin_gradle_binary_boundary() {
    let root = gradle_model_project(
        "model-dependency-gradle",
        EMPTY_MODEL,
        "build.gradle.kts",
        "plugins { java }\n",
    );
    let added = jails_cmd(&root, None)
        .args([
            "add",
            "dependency",
            "org.jsoup:jsoup",
            "--version",
            "1.18.3",
            "--scope",
            "test",
        ])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let build = fs::read_to_string(root.join("build.gradle.kts")).unwrap();
    assert!(build.contains("// jails:dependencies"), "{build}");
    assert!(
        build.contains("testImplementation(\"org.jsoup:jsoup:1.18.3\")"),
        "{build}"
    );
    // **No source root, because nothing is generated.** This project declares
    // one dependency and no Java, and a source root for a directory that may
    // stay empty is an edit to the reader's build with nothing behind it --
    // and one that then outlives every reason for it.
    assert!(
        !build.contains("java.srcDir(\".jails/generated/main/java\")"),
        "{build}"
    );

    let removed = jails_cmd(&root, None)
        .args(["remove", "dependency", "org.jsoup:jsoup"])
        .output()
        .unwrap();
    assert!(
        removed.status.success(),
        "{}",
        String::from_utf8_lossy(&removed.stderr)
    );
    let build = fs::read_to_string(root.join("build.gradle.kts")).unwrap();
    assert!(!build.contains("org.jsoup:jsoup"), "{build}");
    assert!(!build.contains("jails:dependencies"), "{build}");
    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(
        frozen.status.success(),
        "{}",
        String::from_utf8_lossy(&frozen.stderr)
    );
}

#[test]
fn maven_source_root_is_an_exact_reader_patch_and_converges() {
    let root = model_project("model-maven-source-root", MODEL);
    fs::write(
        root.join("pom.xml"),
        "<project>\n    <modelVersion>4.0.0</modelVersion>\n    <groupId>com.example</groupId>\n    <artifactId>notes</artifactId>\n    <version>1</version>\n</project>\n",
    )
    .unwrap();
    let plan = root.join("plan.json");
    let planned = jails_cmd(&root, None)
        .args(["model", "plan", "--bundle"])
        .arg(&plan)
        .output()
        .unwrap();
    assert!(
        planned.status.success(),
        "{}",
        String::from_utf8_lossy(&planned.stderr)
    );
    let bundle: jails_contracts::PlanBundle =
        serde_json::from_slice(&fs::read(&plan).unwrap()).unwrap();
    assert!(bundle.plan.operations.iter().any(|operation| matches!(
        operation,
        jails_contracts::PlannedOperation::PatchReaderFile { path, .. }
            if path.as_str() == "pom.xml"
    )));

    let applied = jails_cmd(&root, None)
        .args(["model", "apply", "--bundle"])
        .arg(&plan)
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("jails:generated-source-root"), "{pom}");
    assert!(pom.contains("build-helper-maven-plugin"), "{pom}");
    assert!(
        pom.contains("<source>.jails/generated/main/java</source>"),
        "{pom}"
    );
    assert!(
        root.join(".jails/generated/main/java/com/example/notes/domain/Note.java")
            .is_file()
    );

    let converged = root.join("converged.json");
    let replanned = jails_cmd(&root, None)
        .args(["model", "plan", "--bundle"])
        .arg(&converged)
        .output()
        .unwrap();
    assert!(
        replanned.status.success(),
        "{}",
        String::from_utf8_lossy(&replanned.stderr)
    );
    let bundle: jails_contracts::PlanBundle =
        serde_json::from_slice(&fs::read(converged).unwrap()).unwrap();
    assert!(
        bundle.plan.operations.is_empty(),
        "{:#?}",
        bundle.plan.operations
    );
    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(
        frozen.status.success(),
        "{}",
        String::from_utf8_lossy(&frozen.stderr)
    );
}

#[test]
fn gradle_source_root_is_an_exact_reader_patch_and_converges() {
    let root = gradle_model_project(
        "model-gradle-source-root",
        MODEL,
        "build.gradle",
        "plugins { id 'java' }\n",
    );
    let plan = root.join("plan.json");
    let planned = jails_cmd(&root, None)
        .args(["model", "plan", "--bundle"])
        .arg(&plan)
        .output()
        .unwrap();
    assert!(
        planned.status.success(),
        "{}",
        String::from_utf8_lossy(&planned.stderr)
    );
    let bundle: jails_contracts::PlanBundle =
        serde_json::from_slice(&fs::read(&plan).unwrap()).unwrap();
    assert!(bundle.plan.operations.iter().any(|operation| matches!(
        operation,
        jails_contracts::PlannedOperation::PatchReaderFile { path, .. }
            if path.as_str() == "build.gradle"
    )));

    let applied = jails_cmd(&root, None)
        .args(["model", "apply", "--bundle"])
        .arg(&plan)
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let build = fs::read_to_string(root.join("build.gradle")).unwrap();
    assert!(build.contains("jails:generated-source-root"), "{build}");
    assert!(
        build.contains("java.srcDir('.jails/generated/main/java')"),
        "{build}"
    );

    let converged = root.join("converged.json");
    let replanned = jails_cmd(&root, None)
        .args(["model", "plan", "--bundle"])
        .arg(&converged)
        .output()
        .unwrap();
    assert!(
        replanned.status.success(),
        "{}",
        String::from_utf8_lossy(&replanned.stderr)
    );
    let bundle: jails_contracts::PlanBundle =
        serde_json::from_slice(&fs::read(converged).unwrap()).unwrap();
    assert!(
        bundle.plan.operations.is_empty(),
        "{:#?}",
        bundle.plan.operations
    );
}

#[test]
fn reader_build_file_precondition_blocks_all_writes_from_a_stale_plan() {
    let root = model_project("model-stale-reader-file", MODEL);
    fs::write(root.join("pom.xml"), "<project>\n</project>\n").unwrap();
    let plan = root.join("plan.json");
    let planned = jails_cmd(&root, None)
        .args(["model", "plan", "--bundle"])
        .arg(&plan)
        .output()
        .unwrap();
    assert!(planned.status.success());
    fs::write(
        root.join("pom.xml"),
        "<project>\n    <!-- reader edit -->\n</project>\n",
    )
    .unwrap();

    let applied = jails_cmd(&root, None)
        .args(["model", "apply", "--bundle"])
        .arg(&plan)
        .output()
        .unwrap();
    assert!(!applied.status.success());
    let stderr = String::from_utf8(applied.stderr).unwrap();
    assert!(stderr.contains("stale exact plan"), "{stderr}");
    assert!(stderr.contains("pom.xml"), "{stderr}");
    assert!(!root.join(".jails/generated").exists());
    assert_eq!(
        fs::read_to_string(root.join("pom.xml")).unwrap(),
        "<project>\n    <!-- reader edit -->\n</project>\n"
    );
}

#[test]
fn maven_build_compiles_the_managed_source_root_end_to_end() {
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

    // The fixture's own Spring pom, not a bare one: `scaffold` means a DTO, a
    // controller and a service, so a build with nothing on the classpath is a
    // build the compiler refuses by name. Compiling the whole scaffold against
    // the dependencies it declares is the question the managed source root
    // has to answer.
    let root = model_project("model-maven-real-compile", EMPTY_MODEL);
    let applied = jails_cmd(&root, None)
        .args([
            "g",
            "scaffold",
            "Note",
            "id:uuid@pk",
            "title:string!",
            "--timestamps",
        ])
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let fake = jails_cmd(&root, None)
        .args(["add", "fake"])
        .output()
        .unwrap();
    assert!(
        fake.status.success(),
        "{}",
        String::from_utf8_lossy(&fake.stderr)
    );

    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    // **The last closing brace, not the first.** `app` and `entity` both end
    // in `\n}\n`, and a plain `replace` puts four operations inside the app
    // block -- which refuses as `unknown app property `command``, about a
    // declaration the test meant for the entity.
    let close = model
        .rfind("\n}\n")
        .expect("the entity block ends the model");
    let with_operations = format!(
        "{}\n\n  command CreateNote(title) @id(op_create_note) {{\n    route POST \"/notes\"\n  }}\n\n  \
         query OpenNotes(title) @id(op_open_notes) {{\n    limit 25\n    route GET \"/notes\"\n  }}\n\n  \
         transition RenameNote(title) @id(op_rename_note) {{\n    select [id]\n    \
         update [title]\n    route PATCH \"/notes/{{id}}\"\n  }}\n\n  \
         event NoteCreated(id, title) @id(op_note_created) {{\n  }}{}",
        &model[..close],
        &model[close..],
    );
    fs::write(root.join(".jails/model.jdl"), with_operations).unwrap();
    let operation_plan = root.join("operations.json");
    let planned = jails_cmd(&root, None)
        .args(["model", "plan", "--bundle"])
        .arg(&operation_plan)
        .output()
        .unwrap();
    assert!(
        planned.status.success(),
        "{}",
        String::from_utf8_lossy(&planned.stderr)
    );
    let applied = jails_cmd(&root, None)
        .args(["model", "apply", "--bundle"])
        .arg(&operation_plan)
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );

    let path = real_path_without_mvnd();
    let compiled = real_maven_cmd(&root, &path)
        .args(["-q", "-B", "compile"])
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "managed source root did not compile through Maven:\n{}\n{}",
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&compiled.stderr)
    );
    assert!(
        root.join("target/classes/com/example/notes/domain/Note.class")
            .is_file(),
        "Maven succeeded without compiling the managed Java source"
    );
    for class in [
        "repository/NoteRepository.class",
        "service/NoteService.class",
        "ports/http/NoteHttpPort.class",
        "adapters/memory/InMemoryNoteRepository.class",
        "application/commands/CreateNoteCommand.class",
        "application/queries/OpenNotesQuery.class",
        "application/transitions/RenameNoteTransition.class",
        "domain/events/NoteCreatedEvent.class",
    ] {
        assert!(
            root.join("target/classes/com/example/notes")
                .join(class)
                .is_file(),
            "Maven did not compile semantic scaffold facet {class}"
        );
    }
}

/// A marked block jails owns stays where it is.
///
/// Stripping the source-roots block and re-inserting it before `</plugins>`
/// is position-stable only while it is the last thing in there; once the
/// integration-test plugin lands beside it, every plan would move one block
/// past the other, `jails model check --frozen` would report a pending
/// operation on a project just synchronised, and the pom would churn by a
/// whole block on every run.
///
/// Two blocks is the smallest case that can show it, which is why this needs
/// an operation: the failsafe plugin arrives with the first emitted `*IT`.
#[test]
fn two_marked_build_blocks_keep_their_places_across_replans() {
    let root = jdl_project(
        "jdl-v1-marked-block-order",
        r#"jdl 1
app Demo {
  pkg com.example.demo
  java 26
  platform spring
  build maven
  storage postgres
}
"#,
    );
    write_spring_fixture(&root);
    for arguments in [
        vec!["g", "scaffold", "Note", "id:uuid@pk", "title:string!"],
        vec!["add", "db", "--no-start"],
        vec![
            "g",
            "query",
            "NotesByTitle",
            "title:string!",
            "--on",
            "Note",
        ],
    ] {
        let output = jails_cmd(&root, None).args(&arguments).output().unwrap();
        assert!(
            output.status.success(),
            "{arguments:?}:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    let roots = pom.find("jails:generated-source-roots").unwrap();
    let tests = pom.find("jails:integration-tests").unwrap();
    assert!(roots < tests, "{pom}");

    // Frozen on the *first* ask, not after a repairing sync: a plan that has
    // to be applied before the tree matches the model is a plan that never
    // settles.
    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(
        frozen.status.success(),
        "{}{}",
        String::from_utf8_lossy(&frozen.stdout),
        String::from_utf8_lossy(&frozen.stderr)
    );

    // And a sync moves nothing, which is the same property from the other
    // side.
    let synced = jails_cmd(&root, None).args(["sync"]).output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );
    assert_eq!(fs::read_to_string(root.join("pom.xml")).unwrap(), pom);
}

#[test]
fn canonical_settings_preview_update_reconcile_and_unset_end_to_end() {
    let root = model_project("model-setting-main", EMPTY_MODEL);
    let properties = root.join("src/main/resources/application.properties");
    fs::create_dir_all(properties.parent().unwrap()).unwrap();
    fs::write(&properties, "# reader\nreader.key=keep\n").unwrap();
    let before = snapshot_tree(&root);

    let preview = jails_cmd(&root, None)
        .args(["set", "server.port=8080", "--pretend"])
        .output()
        .unwrap();
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    assert_eq!(snapshot_tree(&root), before, "setting preview wrote files");

    let added = jails_cmd(&root, None)
        .args(["set", "server.port=8080"])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let first_source = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(
        first_source.contains("prop server.port = \"8080\" @id(set_"),
        "{first_source}"
    );
    let first_model = jails_model::parse_jdl(&first_source).unwrap();
    let first = first_model.settings.values().next().unwrap();
    let stable_id = first.id.clone();
    assert_eq!(first.key, "server.port");
    assert_eq!(first.value, "8080");
    assert_eq!(first.target, jails_model::SettingTarget::Main);
    assert_eq!(
        fs::read_to_string(&properties).unwrap(),
        "# reader\nreader.key=keep\nserver.port=8080\n"
    );

    let updated = jails_cmd(&root, None)
        .args(["set", "server.port=9090"])
        .output()
        .unwrap();
    assert!(
        updated.status.success(),
        "{}",
        String::from_utf8_lossy(&updated.stderr)
    );
    let updated_model =
        jails_model::parse_jdl(&fs::read_to_string(root.join(".jails/model.jdl")).unwrap())
            .unwrap();
    let updated_setting = updated_model.settings.values().next().unwrap();
    assert_eq!(updated_setting.id, stable_id);
    assert_eq!(updated_setting.value, "9090");
    assert_eq!(
        fs::read_to_string(&properties).unwrap(),
        "# reader\nreader.key=keep\nserver.port=9090\n"
    );

    let repeated = jails_cmd(&root, None)
        .args(["--output", "json", "set", "server.port=9090"])
        .output()
        .unwrap();
    assert!(repeated.status.success());
    let execution: serde_json::Value = serde_json::from_slice(&repeated.stdout).unwrap();
    assert_eq!(execution["files_written"], 0);

    let removed = jails_cmd(&root, None)
        .args(["unset", "server.port"])
        .output()
        .unwrap();
    assert!(
        removed.status.success(),
        "{}",
        String::from_utf8_lossy(&removed.stderr)
    );
    assert_eq!(
        fs::read_to_string(&properties).unwrap(),
        "# reader\nreader.key=keep\n"
    );
    let final_model =
        jails_model::parse_jdl(&fs::read_to_string(root.join(".jails/model.jdl")).unwrap())
            .unwrap();
    assert!(final_model.settings.is_empty());
    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(
        frozen.status.success(),
        "{}",
        String::from_utf8_lossy(&frozen.stderr)
    );
}

#[test]
fn canonical_test_setting_creates_the_additive_config_overlay() {
    let root = model_project("model-setting-test", EMPTY_MODEL);
    let output = jails_cmd(&root, None)
        .args(["set", "spring.datasource.url=jdbc:h2:mem:test", "--tests"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !root
            .join("src/main/resources/application.properties")
            .exists()
    );
    assert_eq!(
        fs::read_to_string(root.join("src/test/resources/config/application.properties")).unwrap(),
        "spring.datasource.url=jdbc:h2:mem:test\n"
    );
    let model = jails_model::parse_jdl(&fs::read_to_string(root.join(".jails/model.jdl")).unwrap())
        .unwrap();
    let setting = model.settings.values().next().unwrap();
    assert_eq!(setting.target, jails_model::SettingTarget::Test);
}

#[test]
fn canonical_setting_refuses_reader_owned_collision_without_writes() {
    let root = model_project("model-setting-collision", EMPTY_MODEL);
    let properties = root.join("src/main/resources/application.properties");
    fs::create_dir_all(properties.parent().unwrap()).unwrap();
    fs::write(&properties, "server.port=7000\n").unwrap();
    let before = snapshot_tree(&root);
    let output = jails_cmd(&root, None)
        .args(["set", "server.port=8080"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("reader-owned"), "{stderr}");
    assert_eq!(
        snapshot_tree(&root),
        before,
        "collision refusal wrote files"
    );
}

#[test]
fn canonical_setting_plan_is_stale_if_a_missing_reader_file_appears() {
    let root = model_project("model-setting-stale-missing", EMPTY_MODEL);
    let plan = root.join("setting-plan.json");
    let planned = jails_cmd(&root, None)
        .args(["set", "server.port=8080", "--plan-out"])
        .arg(&plan)
        .output()
        .unwrap();
    assert!(
        planned.status.success(),
        "{}",
        String::from_utf8_lossy(&planned.stderr)
    );
    let model_before = fs::read(root.join(".jails/model.jdl")).unwrap();
    let properties = root.join("src/main/resources/application.properties");
    fs::create_dir_all(properties.parent().unwrap()).unwrap();
    fs::write(&properties, "reader.key=late\n").unwrap();

    let applied = jails_cmd(&root, None)
        .args(["model", "apply", "--bundle"])
        .arg(&plan)
        .output()
        .unwrap();
    assert!(!applied.status.success());
    let stderr = String::from_utf8(applied.stderr).unwrap();
    assert!(
        stderr.contains("precondition") || stderr.contains("changed"),
        "{stderr}"
    );
    assert_eq!(
        fs::read(root.join(".jails/model.jdl")).unwrap(),
        model_before,
        "stale setting plan changed the model"
    );
    assert_eq!(fs::read_to_string(properties).unwrap(), "reader.key=late\n");
}
