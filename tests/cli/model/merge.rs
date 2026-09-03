//! Generate, edit, generate: the accepted model renders BASE, capture supplies
//! OURS, the next model renders THEIRS. Clean merges are frozen into the plan
//! and conflicts refuse without writes.
//!
use super::*;

/// **A lost merge base looks exactly like a collision, and the refusal says
/// so.**
///
/// The lock is BASE. Without it there is no base for *any* managed file, so
/// the first path the compiler renders lands on one that is already there and
/// the reader was told, about their own generated code, to move it or import
/// it. The capture cannot tell which happened -- a project that has never
/// generated and one whose lock was deleted are the same capture -- so the
/// refusal names both repairs rather than guessing, and `doctor` carries a
/// row for the condition before a mutation runs into it.
#[test]
fn a_missing_lock_is_named_as_the_missing_merge_base() {
    let root = jdl_project("merge-lost-base", MODEL);
    let first = jails_cmd(&root, None).arg("sync").output().unwrap();
    assert!(first.status.success(), "{first:?}");
    assert!(root.join(".jails/compiler.lock.json").is_file());

    fs::remove_file(root.join(".jails/compiler.lock.json")).unwrap();

    let doctor = jails_cmd(&root, None).arg("doctor").output().unwrap();
    let report = String::from_utf8_lossy(&doctor.stdout);
    assert!(
        report.contains("merge base") && report.contains("compiler.lock.json"),
        "doctor should name the missing merge base: {report}"
    );

    let refused = jails_cmd(&root, None)
        .args(["g", "field", "Note", "body:string", "--pretend"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    let told = String::from_utf8_lossy(&refused.stderr);
    assert!(told.contains("already reader-owned"), "{told}");
    // Both repairs, because the capture cannot tell a deleted lock from a
    // file the reader wrote: a project that has never generated and one whose
    // lock is gone look identical from here.
    assert!(
        told.contains("restore `.jails/compiler.lock.json` from version control"),
        "{told}"
    );
    assert!(told.contains("if you wrote this file"), "{told}");
}

#[test]
fn jdl_v1_drives_the_real_generate_edit_generate_loop() {
    let root = jdl_project(
        "jdl-v1-generate-edit-generate",
        r#"jdl 1

app Notes @id(project_notes) {
  pkg com.example.notes
  java 26
  platform spring
  build maven
  storage postgres
}

entity Task @id(ent_task) {
  id: uuid @id(fld_task_id) @pk
  title: string @id(fld_task_title) @notBlank
}
"#,
    );
    write_spring_fixture(&root);
    apply_canonical_model(&root, "jdl-v1-initial");

    let record = root.join("src/main/java/com/example/notes/domain/Task.java");
    let source = fs::read_to_string(&record).unwrap();
    let split = source.rfind("\n}").unwrap();
    fs::write(
        &record,
        format!(
            "{}\n\n    public String readerMethod() {{ return title; }}{}",
            &source[..split],
            &source[split..]
        ),
    )
    .unwrap();

    let model_path = root.join(".jails/model.jdl");
    let model = fs::read_to_string(&model_path).unwrap();
    fs::write(
        &model_path,
        model.replace(
            "  title: string @id(fld_task_title) @notBlank\n",
            "  title: string @id(fld_task_title) @notBlank\n  done: boolean @id(fld_task_done)\n",
        ),
    )
    .unwrap();
    apply_canonical_model(&root, "jdl-v1-evolved");

    let evolved = fs::read_to_string(&record).unwrap();
    assert!(evolved.contains("readerMethod()"), "{evolved}");
    assert!(evolved.contains("boolean done"), "{evolved}");
}

#[test]
fn jdl_generate_edit_generate_preserves_clean_edits_and_refuses_overlap() {
    let root = jdl_project("model-jdl-iterative-record", NOTES_JDL);
    let generated = root.join("src/main/java/com/example/notes/domain/Task.java");

    let first = jails_cmd(&root, None)
        .args(["g", "record", "Task", "title:string!(1..200)"])
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let jdl = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(jdl.contains("entity Task {"), "{jdl}");
    assert!(
        unaligned(&jdl).contains("title: string @length(1..200) @notBlank"),
        "{jdl}"
    );

    let source = fs::read_to_string(&generated).unwrap();
    assert!(
        source.contains("title length must be between 1 and 200"),
        "{source}"
    );
    let split = source.rfind("\n}").unwrap();
    fs::write(
        &generated,
        format!(
            "{}\n\n    public String handWritten() {{ return title; }}{}",
            &source[..split],
            &source[split..]
        ),
    )
    .unwrap();

    let second = jails_cmd(&root, None)
        .args(["g", "field", "Task", "done:boolean"])
        .output()
        .unwrap();
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    let source = fs::read_to_string(&generated).unwrap();
    assert!(source.contains("handWritten()"), "{source}");
    assert!(source.contains("boolean done"), "{source}");

    fs::write(
        &generated,
        source.replace("title must not be blank", "give me a useful title"),
    )
    .unwrap();
    let third = jails_cmd(&root, None)
        .args(["g", "field", "Task", "priority:int"])
        .output()
        .unwrap();
    assert!(
        third.status.success(),
        "{}",
        String::from_utf8_lossy(&third.stderr)
    );
    let source = fs::read_to_string(&generated).unwrap();
    assert!(source.contains("give me a useful title"), "{source}");
    assert!(source.contains("int priority"), "{source}");

    fs::write(&generated, source.replace("int priority", "long priority")).unwrap();
    let before = snapshot_tree(&root);
    let conflict = jails_cmd(&root, None)
        .args(["g", "field", "Task", "dueAt:instant"])
        .output()
        .unwrap();
    assert!(!conflict.status.success());
    let stderr = String::from_utf8(conflict.stderr).unwrap();
    assert!(stderr.contains("overlapping edit"), "{stderr}");
    assert_eq!(snapshot_tree(&root), before);
}

#[test]
fn compiler_upgrade_uses_the_exact_accepted_projection_as_merge_base() {
    let root = jdl_project("model-compiler-upgrade-base", NOTES_JDL);
    let generated = root.join("src/main/java/com/example/notes/domain/Task.java");
    let first = jails_cmd(&root, None)
        .args(["g", "record", "Task", "title:string!"])
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );

    let current_projection = fs::read_to_string(&generated).unwrap();
    let old_projection = current_projection.replace(
        "title must not be blank",
        "title used to have old emitter wording",
    );
    assert_ne!(old_projection, current_projection);
    let split = old_projection.rfind("\n}").unwrap();
    let live = format!(
        "{}\n\n    public String handWritten() {{ return title; }}{}",
        &old_projection[..split],
        &old_projection[split..]
    );
    fs::write(&generated, live).unwrap();

    let lock_path = root.join(".jails/compiler.lock.json");
    // **The merge base is a file, so an older emitter's output is written
    // as one.** `reseal_base` then makes the lock say exactly what is there,
    // which is what capture checks on the way in.
    let generated_path = "src/main/java/com/example/notes/domain/Task.java";
    fs::write(
        root.join(".jails/base").join(generated_path),
        old_projection.as_bytes(),
    )
    .unwrap();
    let mut lock: serde_json::Value =
        serde_json::from_slice(&fs::read(&lock_path).unwrap()).unwrap();
    lock["compiler"] = serde_json::Value::String("0.0.0-previous-emitter".to_string());
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();
    reseal_base(&root);

    let evolved = jails_cmd(&root, None)
        .args(["g", "field", "Task", "done:boolean"])
        .output()
        .unwrap();
    assert!(
        evolved.status.success(),
        "{}",
        String::from_utf8_lossy(&evolved.stderr)
    );
    let merged = fs::read_to_string(&generated).unwrap();
    assert!(merged.contains("handWritten()"), "{merged}");
    assert!(merged.contains("boolean done"), "{merged}");
    assert!(merged.contains("title must not be blank"), "{merged}");
    assert!(!merged.contains("old emitter wording"), "{merged}");
}

#[test]
fn canonical_source_units_merge_every_main_and_test_file_and_wire_both_roots() {
    let root = temp_dir("canonical-source-units-loop");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();

    for command in [
        ["g", "class", "Clock"].as_slice(),
        ["g", "interface", "Port"].as_slice(),
        ["g", "service", "BillingService"].as_slice(),
    ] {
        let output = jails_cmd(&root, None).args(command).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let files = [
        "src/main/java/com/example/demo/Clock.java",
        "src/test/java/com/example/demo/ClockTest.java",
        "src/main/java/com/example/demo/Port.java",
        "src/main/java/com/example/demo/service/BillingService.java",
        "src/test/java/com/example/demo/service/BillingServiceTest.java",
    ];
    for (index, relative) in files.iter().enumerate() {
        let path = root.join(relative);
        let source = fs::read_to_string(&path).unwrap();
        let split = source.rfind("\n}").unwrap();
        fs::write(
            &path,
            format!(
                "{}\n\n    // reader-edit-{index}{}",
                &source[..split],
                &source[split..]
            ),
        )
        .unwrap();
    }

    let rerun = jails_cmd(&root, None)
        .args(["g", "class", "Queue"])
        .output()
        .unwrap();
    assert!(
        rerun.status.success(),
        "{}",
        String::from_utf8_lossy(&rerun.stderr)
    );
    for (index, relative) in files.iter().enumerate() {
        let source = fs::read_to_string(root.join(relative)).unwrap();
        assert!(
            source.contains(&format!("reader-edit-{index}")),
            "{relative}: {source}"
        );
    }

    // One source root per set, and nothing declared for it: the build file
    // carries no `build-helper-maven-plugin` block.
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(!pom.contains("build-helper-maven-plugin"), "{pom}");

    let jdl = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(jdl.contains("component class Clock "), "{jdl}");
    assert!(jdl.contains("component interface Port "), "{jdl}");
    assert!(jdl.contains("component service Billing "), "{jdl}");

    // A component carries no package of its own, and the refusal is the
    // contract rather than a gap in the parser: v1 derives every managed
    // placement from the closed projection registry, so a reader-owned
    // destination is `model eject`'s job.
    let before = snapshot_tree(&root);
    fs::write(
        root.join(".jails/model.jdl"),
        jdl.replace(
            "component class Clock {",
            "component class Clock @package(core) {",
        ),
    )
    .unwrap();
    let refused = jails_cmd(&root, None)
        .args(["model", "plan"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    let told = String::from_utf8_lossy(&refused.stderr);
    assert!(told.contains("@package` is not valid here"), "{told}");
    assert!(told.contains("use only id"), "{told}");
    fs::write(root.join(".jails/model.jdl"), &jdl).unwrap();
    assert_eq!(
        snapshot_tree(&root),
        before,
        "the refused plan wrote part of itself"
    );

    fs::remove_dir_all(root).ok();
}

#[test]
fn canonical_standalone_tests_merge_reader_edits_and_refuse_edited_build_wiring() {
    let root = temp_dir("canonical-standalone-tests-loop");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();

    for command in [
        ["g", "test", "ParserTest"].as_slice(),
        ["g", "integration-test", "CheckoutIT"].as_slice(),
    ] {
        let output = jails_cmd(&root, None).args(command).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let files = [
        "src/test/java/com/example/demo/ParserTest.java",
        "src/test/java/com/example/demo/CheckoutIT.java",
    ];
    for (index, relative) in files.iter().enumerate() {
        let path = root.join(relative);
        let source = fs::read_to_string(&path).unwrap();
        let split = source.rfind("\n}").unwrap();
        fs::write(
            path,
            format!(
                "{}\n\n    // standalone-reader-edit-{index}{}",
                &source[..split],
                &source[split..]
            ),
        )
        .unwrap();
    }

    let rerun = jails_cmd(&root, None)
        .args(["g", "test", "Formatter"])
        .output()
        .unwrap();
    assert!(
        rerun.status.success(),
        "{}",
        String::from_utf8_lossy(&rerun.stderr)
    );
    for (index, relative) in files.iter().enumerate() {
        let source = fs::read_to_string(root.join(relative)).unwrap();
        assert!(
            source.contains(&format!("standalone-reader-edit-{index}")),
            "{relative}: {source}"
        );
    }

    let pom_path = root.join("pom.xml");
    let pom = fs::read_to_string(&pom_path).unwrap();
    assert_eq!(pom.matches("maven-failsafe-plugin").count(), 1, "{pom}");
    assert!(pom.contains("<goal>integration-test</goal>"), "{pom}");
    assert!(pom.contains("<goal>verify</goal>"), "{pom}");
    assert!(!pom.contains("<version>3.5.6</version>"), "{pom}");

    let jdl = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(jdl.contains("component test Parser "), "{jdl}");
    assert!(
        jdl.contains("component integration-test Checkout "),
        "{jdl}"
    );

    fs::write(
        &pom_path,
        pom.replace("<goal>verify</goal>", "<goal>reader-edited</goal>"),
    )
    .unwrap();
    let before = snapshot_tree(&root);
    let refused = jails_cmd(&root, None)
        .args(["g", "test", "Later"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("was edited"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert_eq!(snapshot_tree(&root), before, "refusal wrote part of a plan");

    fs::write(&pom_path, &pom).unwrap();
    let integration_path = root.join(files[1]);
    let integration = fs::read_to_string(&integration_path).unwrap();
    fs::write(
        &integration_path,
        integration.replace(
            "throw new UnsupportedOperationException(\"todo\");",
            "throw new UnsupportedOperationException(\"reader wording\");",
        ),
    )
    .unwrap();
    let jdl_path = root.join(".jails/model.jdl");
    let jdl = fs::read_to_string(&jdl_path).unwrap();
    fs::write(
        &jdl_path,
        jdl.replace(
            "component integration-test Checkout {",
            "component test Checkout @id(cmp_integration_test_checkout) {",
        ),
    )
    .unwrap();
    let before = snapshot_tree(&root);
    let refused = jails_cmd(&root, None)
        .args(["model", "plan"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("overlap"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert_eq!(
        snapshot_tree(&root),
        before,
        "Java overlap refusal wrote part of a plan"
    );

    fs::remove_dir_all(root).ok();
}

#[test]
fn canonical_sealed_types_evolve_through_merge_and_destroy_as_one_semantic_unit() {
    let root = temp_dir("canonical-sealed-loop");
    write_plain_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();

    let generated = jails_cmd(&root, None)
        .args(["g", "sealed", "Outcome", "Accepted", "Rejected"])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let files = [
        "src/main/java/com/example/demo/domain/Outcome.java",
        "src/test/java/com/example/demo/domain/OutcomeTest.java",
    ];
    for (index, relative) in files.iter().enumerate() {
        let path = root.join(relative);
        let source = fs::read_to_string(&path).unwrap();
        let anchor = if index == 0 {
            "public sealed interface Outcome permits Outcome.Accepted, Outcome.Rejected {\n"
        } else {
            "class OutcomeTest {\n"
        };
        assert!(source.contains(anchor), "{relative}: {source}");
        fs::write(
            path,
            source.replace(
                anchor,
                &format!("{anchor}\n    // sealed-reader-edit-{index}\n"),
            ),
        )
        .unwrap();
    }
    let jdl_path = root.join(".jails/model.jdl");
    let jdl = fs::read_to_string(&jdl_path).unwrap();
    fs::write(
        &jdl_path,
        jdl.replace(
            "component sealed Outcome {",
            "// reader model note\ncomponent sealed Outcome {",
        ),
    )
    .unwrap();

    let evolved = jails_cmd(&root, None)
        .args(["g", "sealed", "Outcome", "Accepted", "Rejected", "Pending"])
        .output()
        .unwrap();
    assert!(
        evolved.status.success(),
        "{}",
        String::from_utf8_lossy(&evolved.stderr)
    );
    for (index, relative) in files.iter().enumerate() {
        let source = fs::read_to_string(root.join(relative)).unwrap();
        assert!(
            source.contains(&format!("sealed-reader-edit-{index}")),
            "{relative}: {source}"
        );
        assert!(source.contains("Pending"), "{relative}: {source}");
    }
    let jdl = fs::read_to_string(&jdl_path).unwrap();
    // Above the declaration, not inside it. A v1 component is a block and
    // evolving it replaces the whole declaration span, so a comment *inside*
    // would need the editor to merge prose it did not write. The property:
    // the reader's wording in the model source outlives an evolve.
    assert!(
        jdl.contains("// reader model note\ncomponent sealed Outcome {"),
        "{jdl}"
    );
    for variant in ["Accepted", "Rejected", "Pending"] {
        assert!(jdl.contains(&format!("variant {variant}")), "{jdl}");
    }

    let first = snapshot_tree(&root);
    let rerun = jails_cmd(&root, None)
        .args(["g", "sealed", "Outcome", "Accepted", "Rejected", "Pending"])
        .output()
        .unwrap();
    assert!(rerun.status.success());
    assert_eq!(snapshot_tree(&root), first, "identical rerun changed bytes");
    if real_mvn_available() && real_java_supports_target_release() {
        let path = real_path_without_mvnd();
        let compiled = real_maven_cmd(&root, &path)
            .args(["-q", "-B", "test"])
            .output()
            .unwrap();
        assert!(
            compiled.status.success(),
            "canonical sealed type did not compile and test:\n{}\n{}",
            String::from_utf8_lossy(&compiled.stdout),
            String::from_utf8_lossy(&compiled.stderr)
        );
    }

    let main_path = root.join(files[0]);
    let clean_reader_delta = fs::read_to_string(&main_path).unwrap();
    fs::write(
        &main_path,
        clean_reader_delta.replace(
            "record Pending() implements Outcome {}",
            "record Pending(String readerValue) implements Outcome {}",
        ),
    )
    .unwrap();
    let before = snapshot_tree(&root);
    let refused = jails_cmd(&root, None)
        .args(["g", "sealed", "Outcome", "Accepted", "Rejected"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("overlap"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert_eq!(snapshot_tree(&root), before, "overlap wrote part of a plan");

    fs::write(
        &main_path,
        clean_reader_delta
            .replace("\n    // sealed-reader-edit-0\n", "\n")
            .replace("{\n\n\n    /**", "{\n\n    /**"),
    )
    .unwrap();
    let test_path = root.join(files[1]);
    let test = fs::read_to_string(&test_path).unwrap();
    fs::write(
        &test_path,
        test.replace("\n    // sealed-reader-edit-1\n", "\n")
            .replace("{\n\n\n    private", "{\n\n    private"),
    )
    .unwrap();
    let destroyed = jails_cmd(&root, None)
        .args(["destroy", "sealed", "Outcome", "--force"])
        .output()
        .unwrap();
    assert!(
        destroyed.status.success(),
        "{}",
        String::from_utf8_lossy(&destroyed.stderr)
    );
    assert!(files.iter().all(|relative| !root.join(relative).exists()));

    fs::remove_dir_all(root).ok();
}

#[test]
fn canonical_factory_tracks_entity_fields_without_owning_the_record() {
    let root = temp_dir("canonical-factory-loop");
    write_plain_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();

    let record_output = jails_cmd(&root, None)
        .args(["g", "record", "Note", "title:string!"])
        .output()
        .unwrap();
    assert!(
        record_output.status.success(),
        "{}",
        String::from_utf8_lossy(&record_output.stderr)
    );
    let jdl_path = root.join(".jails/model.jdl");
    let jdl = fs::read_to_string(&jdl_path).unwrap();
    fs::write(
        &jdl_path,
        jdl.replace("entity Note {", "entity Note { // reader model wording"),
    )
    .unwrap();
    let factory_output = jails_cmd(&root, None)
        .args(["g", "factory", "Note"])
        .output()
        .unwrap();
    assert!(
        factory_output.status.success(),
        "{}",
        String::from_utf8_lossy(&factory_output.stderr)
    );
    // The facet is a `use` line inside the block in v1, so the reader's
    // wording on the header line is untouched by the insert rather than
    // carried along by it.
    let jdl = fs::read_to_string(&jdl_path).unwrap();
    assert!(
        jdl.contains("entity Note { // reader model wording"),
        "{jdl}"
    );
    assert!(jdl.contains("use factory"), "{jdl}");
    let factory = root.join("src/test/java/com/example/demo/testkit/NoteFactory.java");
    let record = root.join("src/main/java/com/example/demo/domain/Note.java");
    let source = fs::read_to_string(&factory).unwrap();
    fs::write(
        &factory,
        source.replace(
            "    public static NoteFactory aNote() {\n        return new NoteFactory();\n    }\n",
            "    public static NoteFactory aNote() {\n        return new NoteFactory();\n    }\n\n    public String readerMethod() { return \"reader\"; }\n",
        ),
    )
    .unwrap();

    let evolved = jails_cmd(&root, None)
        .args(["g", "field", "Note", "done:boolean"])
        .output()
        .unwrap();
    assert!(
        evolved.status.success(),
        "{}",
        String::from_utf8_lossy(&evolved.stderr)
    );
    let evolved_factory = fs::read_to_string(&factory).unwrap();
    assert!(
        evolved_factory.contains("readerMethod()"),
        "{evolved_factory}"
    );
    assert!(
        evolved_factory.contains("withDone(boolean value)"),
        "{evolved_factory}"
    );
    assert!(
        fs::read_to_string(&record)
            .unwrap()
            .contains("boolean done"),
        "record did not evolve with its factory"
    );
    let stable = snapshot_tree(&root);
    let rerun = jails_cmd(&root, None)
        .args(["g", "factory", "Note"])
        .output()
        .unwrap();
    assert!(rerun.status.success());
    assert_eq!(snapshot_tree(&root), stable, "factory rerun changed bytes");

    if real_mvn_available() && real_java_supports_target_release() {
        let path = real_path_without_mvnd();
        let compiled = real_maven_cmd(&root, &path)
            .args(["-q", "-B", "test"])
            .output()
            .unwrap();
        assert!(
            compiled.status.success(),
            "canonical factory did not compile:\n{}\n{}",
            String::from_utf8_lossy(&compiled.stdout),
            String::from_utf8_lossy(&compiled.stderr)
        );
    }

    fs::write(
        &factory,
        evolved_factory.replace(
            "private boolean done = false;",
            "private boolean done = true;",
        ),
    )
    .unwrap();
    // The formatter lines the type column up, so the edit is made on the
    // line rather than on a spelling that moves when a sibling field does.
    let jdl = fs::read_to_string(&jdl_path).unwrap();
    let done = jdl
        .lines()
        .find(|line| line.trim_start().starts_with("done:"))
        .expect("the entity declares `done`")
        .to_string();
    fs::write(
        &jdl_path,
        jdl.replace(&done, &done.replace("boolean", "string")),
    )
    .unwrap();
    let before = snapshot_tree(&root);
    let refused = jails_cmd(&root, None)
        .args(["model", "plan"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("overlap"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert_eq!(snapshot_tree(&root), before, "factory overlap wrote bytes");

    fs::write(&jdl_path, jdl).unwrap();
    fs::write(
        &factory,
        evolved_factory.replace(
            "\n    public String readerMethod() { return \"reader\"; }\n",
            "",
        ),
    )
    .unwrap();
    let destroyed = jails_cmd(&root, None)
        .args(["destroy", "factory", "Note", "--force"])
        .output()
        .unwrap();
    assert!(
        destroyed.status.success(),
        "{}",
        String::from_utf8_lossy(&destroyed.stderr)
    );
    assert!(!factory.exists());
    assert!(record.exists(), "factory destroy removed the managed ABI");
    let jdl = fs::read_to_string(jdl_path).unwrap();
    assert!(!jdl.contains("@factory"), "{jdl}");

    fs::remove_dir_all(root).ok();
}

#[test]
fn canonical_strategy_evolves_all_implementation_boundaries_in_one_plan() {
    let root = temp_dir("canonical-strategy-loop");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();
    for command in [
        ["g", "record", "Post", "title:string!"].as_slice(),
        ["g", "record", "Tag", "value:string!"].as_slice(),
        ["g", "record", "Other", "name:string!"].as_slice(),
        [
            "g", "strategy", "PostRule", "Featured", "Standard", "--on", "Post", "--yields", "Tag",
        ]
        .as_slice(),
    ] {
        let output = jails_cmd(&root, None).args(command).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let jdl_path = root.join(".jails/model.jdl");
    let jdl = fs::read_to_string(&jdl_path).unwrap();
    fs::write(
        &jdl_path,
        jdl.replace(
            "component strategy PostRule {",
            "// reader strategy wording\ncomponent strategy PostRule {",
        ),
    )
    .unwrap();
    let managed = root.join("src");
    let existing = [
        managed.join("main/java/com/example/demo/domain/PostRule.java"),
        managed.join("main/java/com/example/demo/service/PostRuleEvaluator.java"),
        managed.join("main/java/com/example/demo/service/FeaturedPostRule.java"),
        managed.join("main/java/com/example/demo/service/StandardPostRule.java"),
        managed.join("test/java/com/example/demo/service/FeaturedPostRuleTest.java"),
        managed.join("test/java/com/example/demo/service/StandardPostRuleTest.java"),
    ];
    for (index, path) in existing.iter().enumerate() {
        let source = fs::read_to_string(path).unwrap();
        let split = source.rfind("\n}").unwrap();
        fs::write(
            path,
            format!(
                "{}\n\n    // strategy-reader-edit-{index}{}",
                &source[..split],
                &source[split..]
            ),
        )
        .unwrap();
    }

    let evolved = jails_cmd(&root, None)
        .args([
            "g", "strategy", "PostRule", "Featured", "Standard", "Premium", "--on", "Post",
            "--yields", "Tag",
        ])
        .output()
        .unwrap();
    assert!(
        evolved.status.success(),
        "{}",
        String::from_utf8_lossy(&evolved.stderr)
    );
    for (index, path) in existing.iter().enumerate() {
        assert!(
            fs::read_to_string(path)
                .unwrap()
                .contains(&format!("strategy-reader-edit-{index}")),
            "reader edit was lost from {}",
            path.display()
        );
    }
    let premium = [
        managed.join("main/java/com/example/demo/service/PremiumPostRule.java"),
        managed.join("test/java/com/example/demo/service/PremiumPostRuleTest.java"),
    ];
    assert!(premium.iter().all(|path| path.is_file()));
    assert!(
        fs::read_to_string(&jdl_path)
            .unwrap()
            .contains("// reader strategy wording\ncomponent strategy PostRule")
    );

    let port = &existing[0];
    let clean_port = fs::read_to_string(port).unwrap();
    fs::write(
        port,
        clean_port.replace("evaluate(Post value)", "evaluate(Post readerOwnedValue)"),
    )
    .unwrap();
    let before = snapshot_tree(&root);
    let refused = jails_cmd(&root, None)
        .args([
            "g", "strategy", "PostRule", "Featured", "Standard", "Premium", "--on", "Other",
            "--yields", "Tag",
        ])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("overlap"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert_eq!(snapshot_tree(&root), before, "strategy overlap wrote bytes");

    fs::write(port, clean_port).unwrap();
    let changed = jails_cmd(&root, None)
        .args([
            "g", "strategy", "PostRule", "Featured", "Standard", "Premium", "--on", "Other",
            "--yields", "Tag",
        ])
        .output()
        .unwrap();
    assert!(
        changed.status.success(),
        "{}",
        String::from_utf8_lossy(&changed.stderr)
    );
    assert!(
        fs::read_to_string(port)
            .unwrap()
            .contains("evaluate(Other value)")
    );
    for (index, path) in existing.iter().enumerate() {
        assert!(
            fs::read_to_string(path)
                .unwrap()
                .contains(&format!("strategy-reader-edit-{index}")),
            "signature evolution lost {}",
            path.display()
        );
    }

    let before_ejection = snapshot_tree(&root);
    let refused_ejection = jails_cmd(&root, None)
        .args(["model", "eject", "art_unit_strategy_post_rule_abi"])
        .output()
        .unwrap();
    assert!(!refused_ejection.status.success());
    assert!(
        String::from_utf8_lossy(&refused_ejection.stderr).contains("managed ABI"),
        "{}",
        String::from_utf8_lossy(&refused_ejection.stderr)
    );
    assert_eq!(snapshot_tree(&root), before_ejection);

    let stable = snapshot_tree(&root);
    let rerun = jails_cmd(&root, None)
        .args([
            "g", "strategy", "PostRule", "Featured", "Standard", "Premium", "--on", "Other",
            "--yields", "Tag",
        ])
        .output()
        .unwrap();
    assert!(rerun.status.success());
    assert_eq!(snapshot_tree(&root), stable, "strategy rerun changed bytes");

    if real_mvn_available() && real_java_supports_target_release() {
        let path = real_path_without_mvnd();
        let compiled = real_maven_cmd(&root, &path)
            .args(["-q", "-B", "test"])
            .output()
            .unwrap();
        assert!(
            compiled.status.success(),
            "canonical strategy did not compile:\n{}\n{}",
            String::from_utf8_lossy(&compiled.stdout),
            String::from_utf8_lossy(&compiled.stderr)
        );
    }

    for (index, path) in existing.iter().enumerate() {
        let source = fs::read_to_string(path).unwrap();
        fs::write(
            path,
            source.replace(&format!("\n\n    // strategy-reader-edit-{index}"), ""),
        )
        .unwrap();
    }
    let destroyed = jails_cmd(&root, None)
        .args(["destroy", "strategy", "PostRule", "--force"])
        .output()
        .unwrap();
    assert!(
        destroyed.status.success(),
        "{}",
        String::from_utf8_lossy(&destroyed.stderr)
    );
    assert!(existing.iter().chain(&premium).all(|path| !path.exists()));
    assert!(
        root.join("src/main/java/com/example/demo/domain/Post.java")
            .is_file(),
        "strategy destroy removed an input ABI"
    );

    fs::remove_dir_all(root).ok();
}

#[test]
fn canonical_controller_merges_both_files_and_refuses_overlapping_route_edits() {
    let root = temp_dir("canonical-controller-loop");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();
    for command in [
        ["g", "record", "Request", "value:string!"].as_slice(),
        ["g", "record", "Response", "value:string!"].as_slice(),
        [
            "g",
            "controller",
            "Verify",
            "--method",
            "post",
            "--on",
            "Request",
            "--returns",
            "Response",
            "--path",
            "/v1/verify",
            "--consumes",
            "json",
        ]
        .as_slice(),
    ] {
        let output = jails_cmd(&root, None).args(command).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let jdl_path = root.join(".jails/model.jdl");
    let jdl = fs::read_to_string(&jdl_path).unwrap();
    fs::write(
        &jdl_path,
        jdl.replace(
            "component controller Verify {",
            "// reader route wording\ncomponent controller Verify {",
        ),
    )
    .unwrap();
    let files = [
        root.join("src/main/java/com/example/demo/web/VerifyController.java"),
        root.join("src/test/java/com/example/demo/web/VerifyControllerTest.java"),
    ];
    for (index, path) in files.iter().enumerate() {
        let source = fs::read_to_string(path).unwrap();
        let split = source.rfind("\n}").unwrap();
        fs::write(
            path,
            format!(
                "{}\n\n    // controller-reader-edit-{index}{}",
                &source[..split],
                &source[split..]
            ),
        )
        .unwrap();
    }

    let evolve = [
        "g",
        "controller",
        "Verify",
        "--method",
        "put",
        "--on",
        "Request",
        "--returns",
        "Response",
        "--path",
        "/v2/verify",
        "--consumes",
        "json",
    ];
    let evolved = jails_cmd(&root, None).args(evolve).output().unwrap();
    assert!(
        evolved.status.success(),
        "{}",
        String::from_utf8_lossy(&evolved.stderr)
    );
    for (index, path) in files.iter().enumerate() {
        let source = fs::read_to_string(path).unwrap();
        assert!(source.contains(&format!("controller-reader-edit-{index}")));
        assert!(
            source.contains("/v2/verify"),
            "{}: {source}",
            path.display()
        );
    }
    assert!(
        fs::read_to_string(&files[0])
            .unwrap()
            .contains("@PutMapping")
    );
    assert!(
        fs::read_to_string(&jdl_path)
            .unwrap()
            .contains("// reader route wording\ncomponent controller Verify")
    );

    let stable = snapshot_tree(&root);
    let rerun = jails_cmd(&root, None).args(evolve).output().unwrap();
    assert!(rerun.status.success());
    assert_eq!(
        snapshot_tree(&root),
        stable,
        "controller rerun changed bytes"
    );

    let clean_controller = fs::read_to_string(&files[0]).unwrap();
    fs::write(
        &files[0],
        clean_controller.replace("@PutMapping(path =", "@DeleteMapping(path ="),
    )
    .unwrap();
    let before = snapshot_tree(&root);
    let refused = jails_cmd(&root, None)
        .args([
            "g",
            "controller",
            "Verify",
            "--method",
            "patch",
            "--on",
            "Request",
            "--returns",
            "Response",
            "--path",
            "/v3/verify",
            "--consumes",
            "json",
        ])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("overlap"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert_eq!(
        snapshot_tree(&root),
        before,
        "controller overlap wrote bytes"
    );
    fs::write(&files[0], clean_controller).unwrap();

    let before_body_refusal = snapshot_tree(&root);
    let bodyless = jails_cmd(&root, None)
        .args([
            "g",
            "controller",
            "Verify",
            "--method",
            "get",
            "--on",
            "Request",
        ])
        .output()
        .unwrap();
    assert!(!bodyless.status.success());
    assert!(
        String::from_utf8_lossy(&bodyless.stderr).contains("does not carry"),
        "{}",
        String::from_utf8_lossy(&bodyless.stderr)
    );
    assert_eq!(snapshot_tree(&root), before_body_refusal);

    if real_mvn_available() && real_java_supports_target_release() {
        let path = real_path_without_mvnd();
        let compiled = real_maven_cmd(&root, &path)
            .args(["-q", "-B", "test"])
            .output()
            .unwrap();
        assert!(
            compiled.status.success(),
            "canonical controller did not compile:\n{}\n{}",
            String::from_utf8_lossy(&compiled.stdout),
            String::from_utf8_lossy(&compiled.stderr)
        );
    }

    for (index, path) in files.iter().enumerate() {
        let source = fs::read_to_string(path).unwrap();
        fs::write(
            path,
            source.replace(&format!("\n\n    // controller-reader-edit-{index}"), ""),
        )
        .unwrap();
    }
    let destroyed = jails_cmd(&root, None)
        .args(["destroy", "controller", "Verify", "--force"])
        .output()
        .unwrap();
    assert!(
        destroyed.status.success(),
        "{}",
        String::from_utf8_lossy(&destroyed.stderr)
    );
    assert!(files.iter().all(|path| !path.exists()));
    assert!(
        root.join("src/main/java/com/example/demo/domain/Request.java")
            .exists()
    );

    fs::remove_dir_all(root).ok();
}

#[test]
fn canonical_loadtest_merges_every_project_file_and_refuses_route_overlap_atomically() {
    let root = temp_dir("canonical-loadtest-loop");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();
    for arguments in [
        ["g", "controller", "Health", "--path", "/health"].as_slice(),
        ["add", "loadtest", "--no-start"].as_slice(),
    ] {
        let output = jails_cmd(&root, None).args(arguments).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let load_tests = root.join("load-tests");
    let api_path = load_tests.join("api.js");
    let readme_path = load_tests.join("README.md");
    let token_path = load_tests.join("token-cache.js");
    let initial_api = fs::read_to_string(&api_path).unwrap();
    assert!(
        initial_api
            .contains("{ method: \"GET\", path: \"/health\", handler: \"HealthController#get\" }")
    );
    for (path, edit) in [
        (&readme_path, "\nReader load-test notes.\n"),
        (&token_path, "\nexport const readerTokenHook = true;\n"),
    ] {
        let mut source = fs::read_to_string(path).unwrap();
        source.push_str(edit);
        fs::write(path, source).unwrap();
    }

    let evolved = jails_cmd(&root, None)
        .args(["g", "controller", "Health", "--path", "/healthz"])
        .output()
        .unwrap();
    assert!(
        evolved.status.success(),
        "{}",
        String::from_utf8_lossy(&evolved.stderr)
    );
    let clean_api = fs::read_to_string(&api_path).unwrap();
    assert!(clean_api.contains("path: \"/healthz\""), "{clean_api}");
    assert!(
        fs::read_to_string(&readme_path)
            .unwrap()
            .contains("Reader load-test notes.")
    );
    assert!(
        fs::read_to_string(&token_path)
            .unwrap()
            .contains("readerTokenHook")
    );

    let stable = snapshot_tree(&root);
    let rerun = jails_cmd(&root, None)
        .args(["g", "controller", "Health", "--path", "/healthz"])
        .output()
        .unwrap();
    assert!(rerun.status.success());
    assert_eq!(snapshot_tree(&root), stable);

    fs::write(
        &api_path,
        clean_api.replace("path: \"/healthz\"", "path: \"/reader-health\""),
    )
    .unwrap();
    let before = snapshot_tree(&root);
    let refused = jails_cmd(&root, None)
        .args(["g", "controller", "Health", "--path", "/health-next"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("overlap"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert_eq!(snapshot_tree(&root), before, "loadtest refusal wrote files");

    fs::write(&api_path, clean_api).unwrap();
    let before_remove = snapshot_tree(&root);
    let edited_remove = jails_cmd(&root, None)
        .args(["remove", "loadtest"])
        .output()
        .unwrap();
    assert!(!edited_remove.status.success());
    assert!(
        String::from_utf8_lossy(&edited_remove.stderr).contains("edited by you"),
        "{}",
        String::from_utf8_lossy(&edited_remove.stderr)
    );
    assert_eq!(snapshot_tree(&root), before_remove);

    let clean_readme = include_str!("../../golden/cap-loadtest/load-tests/README.md");
    let clean_token = include_str!("../../golden/cap-loadtest/load-tests/token-cache.js");
    fs::write(&readme_path, clean_readme).unwrap();
    fs::write(&token_path, clean_token).unwrap();
    let removed = jails_cmd(&root, None)
        .args(["remove", "loadtest", "--force"])
        .output()
        .unwrap();
    assert!(
        removed.status.success(),
        "{}",
        String::from_utf8_lossy(&removed.stderr)
    );
    assert!(!load_tests.exists() || snapshot_tree(&load_tests).is_empty());

    fs::remove_dir_all(root).ok();
}

#[test]
fn canonical_repository_is_a_managed_abi_facet_of_the_record() {
    let root = temp_dir("canonical-repository-loop");
    write_plain_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();

    let record_output = jails_cmd(&root, None)
        .args(["g", "record", "Note", "id:int@pk", "title:string!"])
        .output()
        .unwrap();
    assert!(
        record_output.status.success(),
        "{}",
        String::from_utf8_lossy(&record_output.stderr)
    );
    let jdl_path = root.join(".jails/model.jdl");
    let jdl = fs::read_to_string(&jdl_path).unwrap();
    fs::write(
        &jdl_path,
        jdl.replace("entity Note {", "entity Note { // reader model wording"),
    )
    .unwrap();

    let generated = jails_cmd(&root, None)
        .args(["g", "repo", "Note"])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let jdl = fs::read_to_string(&jdl_path).unwrap();
    assert!(
        jdl.contains("entity Note { // reader model wording"),
        "{jdl}"
    );
    assert!(jdl.contains("use repo"), "{jdl}");
    let repository = root.join("src/main/java/com/example/demo/repository/NoteRepository.java");
    let record = root.join("src/main/java/com/example/demo/domain/Note.java");
    let source = fs::read_to_string(&repository).unwrap();
    let reader_source = source.replace(
        "\n}\n",
        "\n\n    default String readerMethod() { return \"reader\"; }\n}\n",
    );
    fs::write(&repository, &reader_source).unwrap();

    let evolved = jails_cmd(&root, None)
        .args(["g", "field", "Note", "done:boolean"])
        .output()
        .unwrap();
    assert!(
        evolved.status.success(),
        "{}",
        String::from_utf8_lossy(&evolved.stderr)
    );
    let evolved_repository = fs::read_to_string(&repository).unwrap();
    assert!(evolved_repository.contains("readerMethod()"));
    assert!(
        fs::read_to_string(&record)
            .unwrap()
            .contains("boolean done"),
        "record did not evolve alongside its repository ABI"
    );
    let stable = snapshot_tree(&root);
    let rerun = jails_cmd(&root, None)
        .args(["g", "repo", "Note"])
        .output()
        .unwrap();
    assert!(rerun.status.success());
    assert_eq!(
        snapshot_tree(&root),
        stable,
        "repository rerun changed bytes"
    );

    fs::write(
        &repository,
        evolved_repository.replace("findById(int id)", "findById(int readerOwnedId)"),
    )
    .unwrap();
    let before = snapshot_tree(&root);
    let refused = jails_cmd(&root, None)
        .args([
            "entity",
            "field",
            "type",
            "Note",
            "id",
            "--to",
            "long",
            "--strategy",
            "safe",
        ])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("overlap"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert_eq!(
        snapshot_tree(&root),
        before,
        "repository overlap wrote bytes"
    );

    fs::write(&repository, &evolved_repository).unwrap();
    let changed = jails_cmd(&root, None)
        .args([
            "entity",
            "field",
            "type",
            "Note",
            "id",
            "--to",
            "long",
            "--strategy",
            "safe",
        ])
        .output()
        .unwrap();
    assert!(
        changed.status.success(),
        "{}",
        String::from_utf8_lossy(&changed.stderr)
    );
    let changed_repository = fs::read_to_string(&repository).unwrap();
    assert!(changed_repository.contains("findById(long id)"));
    assert!(changed_repository.contains("readerMethod()"));

    let before_ejection = snapshot_tree(&root);
    let refused_ejection = jails_cmd(&root, None)
        .args(["model", "eject", "art_ent_note_repository"])
        .output()
        .unwrap();
    assert!(!refused_ejection.status.success());
    assert!(
        String::from_utf8_lossy(&refused_ejection.stderr).contains("managed ABI"),
        "{}",
        String::from_utf8_lossy(&refused_ejection.stderr)
    );
    assert_eq!(
        snapshot_tree(&root),
        before_ejection,
        "repository ABI ejection wrote bytes"
    );

    fs::write(
        &repository,
        changed_repository.replace(
            "\n    default String readerMethod() { return \"reader\"; }\n",
            "",
        ),
    )
    .unwrap();
    let destroyed = jails_cmd(&root, None)
        .args(["destroy", "repo", "Note", "--force"])
        .output()
        .unwrap();
    assert!(
        destroyed.status.success(),
        "{}",
        String::from_utf8_lossy(&destroyed.stderr)
    );
    assert!(!repository.exists());
    assert!(record.exists(), "repository destroy removed the record ABI");
    let jdl = fs::read_to_string(jdl_path).unwrap();
    assert!(!jdl.contains("@repository"), "{jdl}");

    fs::remove_dir_all(root).ok();
}

#[test]
fn canonical_dto_evolves_three_merge_managed_abi_files_without_losing_reader_edits() {
    let root = temp_dir("canonical-dto-loop");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();

    for command in [
        [
            "g",
            "record",
            "Task",
            "id:int@pk",
            "title:string!",
            "note:string?",
        ]
        .as_slice(),
        ["g", "dto", "Task"].as_slice(),
    ] {
        let output = jails_cmd(&root, None).args(command).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let generated = root.join("src");
    let request = generated.join("main/java/com/example/demo/web/TaskRequest.java");
    let response = generated.join("main/java/com/example/demo/web/TaskResponse.java");
    let test = generated.join("test/java/com/example/demo/web/TaskDtoTest.java");
    let record = generated.join("main/java/com/example/demo/domain/Task.java");
    for path in [&request, &response, &test] {
        assert!(path.is_file(), "missing {}", path.display());
    }
    let jdl = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(jdl.contains("entity Task {"), "{jdl}");
    assert!(jdl.contains("use dto"), "{jdl}");
    assert!(
        fs::read_to_string(root.join("pom.xml"))
            .unwrap()
            .contains("spring-boot-starter-validation")
    );

    for (path, method) in [
        (
            &request,
            "    public String readerRequestMethod() { return title; }",
        ),
        (
            &response,
            "    public String readerResponseMethod() { return title; }",
        ),
        (&test, "    private static void readerTestHelper() {}"),
    ] {
        let source = fs::read_to_string(path).unwrap();
        let split = source.rfind("\n}").unwrap();
        fs::write(
            path,
            format!("{}\n\n{method}{}", &source[..split], &source[split..]),
        )
        .unwrap();
    }

    let evolved = jails_cmd(&root, None)
        .args(["g", "field", "Task", "done:boolean"])
        .output()
        .unwrap();
    assert!(
        evolved.status.success(),
        "{}",
        String::from_utf8_lossy(&evolved.stderr)
    );
    for (path, reader_edit) in [
        (&request, "readerRequestMethod"),
        (&response, "readerResponseMethod"),
        (&test, "readerTestHelper"),
    ] {
        let source = fs::read_to_string(path).unwrap();
        assert!(source.contains(reader_edit), "{source}");
        assert!(source.contains("done"), "{source}");
    }
    let request_with_reader_edits = fs::read_to_string(&request).unwrap();

    let stable = snapshot_tree(&root);
    let rerun = jails_cmd(&root, None)
        .args(["g", "dto", "Task"])
        .output()
        .unwrap();
    assert!(rerun.status.success());
    assert_eq!(snapshot_tree(&root), stable, "DTO rerun changed bytes");

    // **Edited on the line the next render moves.** The component has to be
    // one the request record declares and the change has to reach its Java:
    // `id` is server-assigned and no longer appears in a request at all, and
    // widening a `string` renders the same `String`, so neither had anything
    // for the merge to conflict over. Relaxing `title` turns `@NotBlank
    // String title` into an `Optional<String>`, which is the line the reader
    // renamed.
    assert!(
        request_with_reader_edits.contains("@NotBlank String title"),
        "{request_with_reader_edits}"
    );
    fs::write(
        &request,
        request_with_reader_edits.replace(
            "@NotBlank String title",
            "@NotBlank String readerOwnedTitle",
        ),
    )
    .unwrap();
    let before = snapshot_tree(&root);
    let refused = jails_cmd(&root, None)
        .args([
            "entity",
            "field",
            "nullability",
            "Task",
            "title",
            "--nullable",
        ])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("overlap"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert_eq!(snapshot_tree(&root), before, "DTO overlap wrote bytes");

    fs::write(&request, &request_with_reader_edits).unwrap();
    let changed = jails_cmd(&root, None)
        .args([
            "entity",
            "field",
            "type",
            "Task",
            "id",
            "--to",
            "long",
            "--strategy",
            "safe",
        ])
        .output()
        .unwrap();
    assert!(
        changed.status.success(),
        "{}",
        String::from_utf8_lossy(&changed.stderr)
    );
    // `id` is server-assigned and so is not a request component; what the
    // widening reaches in this file is `toDomain`, which mints it.
    let changed_request = fs::read_to_string(&request).unwrap();
    assert!(changed_request.contains("0L"), "{changed_request}");
    assert!(
        changed_request.contains("readerRequestMethod"),
        "{changed_request}"
    );

    let before_ejection = snapshot_tree(&root);
    let refused_ejection = jails_cmd(&root, None)
        .args(["model", "eject", "art_ent_task_dto_request"])
        .output()
        .unwrap();
    assert!(!refused_ejection.status.success());
    assert!(
        String::from_utf8_lossy(&refused_ejection.stderr).contains("managed ABI"),
        "{}",
        String::from_utf8_lossy(&refused_ejection.stderr)
    );
    assert_eq!(snapshot_tree(&root), before_ejection);

    if real_mvn_available() && real_java_supports_target_release() {
        let path = real_path_without_mvnd();
        let verified = real_maven_cmd(&root, &path)
            .args(["-q", "-B", "test"])
            .output()
            .unwrap();
        assert!(
            verified.status.success(),
            "canonical DTO sources did not compile and test:\n{}\n{}",
            String::from_utf8_lossy(&verified.stdout),
            String::from_utf8_lossy(&verified.stderr)
        );
    }

    fs::write(
        &request,
        changed_request.replace(
            "\n    public String readerRequestMethod() { return title; }\n",
            "",
        ),
    )
    .unwrap();
    fs::write(
        &response,
        fs::read_to_string(&response).unwrap().replace(
            "\n    public String readerResponseMethod() { return title; }\n",
            "",
        ),
    )
    .unwrap();
    fs::write(
        &test,
        fs::read_to_string(&test)
            .unwrap()
            .replace("\n    private static void readerTestHelper() {}\n", ""),
    )
    .unwrap();
    let destroyed = jails_cmd(&root, None)
        .args(["destroy", "dto", "Task", "--force"])
        .output()
        .unwrap();
    assert!(
        destroyed.status.success(),
        "{}",
        String::from_utf8_lossy(&destroyed.stderr)
    );
    assert!(!request.exists());
    assert!(!response.exists());
    assert!(!test.exists());
    assert!(record.exists(), "DTO destroy removed its domain record");
    assert!(
        !fs::read_to_string(root.join(".jails/model.jdl"))
            .unwrap()
            .contains("@dto")
    );

    fs::remove_dir_all(root).ok();
}

/// A scaffold serves its resource: the `http` facet emits a controller behind
/// `<Name>HttpPort`, not a one-method interface with no implementation, no
/// route and no caller. An unimplemented interface compiles, so this is held
/// at the file level.
///
/// It speaks the domain record rather than a request/response pair, which is
/// the shape the operation controllers already use -- one wire convention per
/// project rather than two, and `scaffold` stays the four-facet profile it is
/// documented to be.
#[test]
fn a_deleted_managed_file_is_repaired_from_the_model() {
    let root = jdl_project(
        "jdl-v1-repair-deleted",
        r#"jdl 1
app Demo {
  pkg com.example.demo
  java 26
  platform plain
  build maven
  storage none
}

entity Widget {
 id: long @pk
 title: string
}
"#,
    );
    let sync = jails_cmd(&root, None).arg("sync").output().unwrap();
    assert!(
        sync.status.success(),
        "{}",
        String::from_utf8_lossy(&sync.stderr)
    );
    let widget = root.join("src/main/java/com/example/demo/domain/Widget.java");
    let rendered = fs::read_to_string(&widget).unwrap();

    // A reader deletes a managed file -- a half-finished `git checkout`, or a
    // deletion meant as "stop generating this". `sync` is the verb that makes
    // the tree match the model, and a file that is simply gone has an exact
    // answer, so it writes it back rather than teaching the reader a second
    // command they will use once.
    fs::remove_file(&widget).unwrap();
    let healed = jails_cmd(&root, None).arg("sync").output().unwrap();
    assert!(
        healed.status.success(),
        "{}",
        String::from_utf8_lossy(&healed.stderr)
    );
    assert!(
        String::from_utf8_lossy(&healed.stdout).contains("Widget.java"),
        "{}",
        String::from_utf8_lossy(&healed.stdout)
    );
    assert_eq!(fs::read_to_string(&widget).unwrap(), rendered);

    // `resource repair` does the same thing, and is still the command for the
    // one case `sync` leaves alone: a sealed migration whose bytes changed.
    fs::remove_file(&widget).unwrap();
    let repaired = jails_cmd(&root, None)
        .args(["entity", "repair"])
        .output()
        .unwrap();
    assert!(
        repaired.status.success(),
        "{}",
        String::from_utf8_lossy(&repaired.stderr)
    );
    assert_eq!(fs::read_to_string(&widget).unwrap(), rendered);

    // Repaired means converged, not merely present: the next ordinary plan is
    // empty and the project is frozen against its own model again.
    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(
        frozen.status.success(),
        "{}",
        String::from_utf8_lossy(&frozen.stderr)
    );

    // Repair waives one guard and no others. A hand edit is still merged, not
    // overwritten -- a repair that reverted the reader's work would be a worse
    // answer than the refusal it replaces.
    let edited = format!(
        "{}\n    // reader's own note\n",
        rendered.trim_end().trim_end_matches('}').trim_end()
    );
    fs::write(&widget, format!("{edited}}}\n")).unwrap();
    let again = jails_cmd(&root, None)
        .args(["entity", "repair"])
        .output()
        .unwrap();
    assert!(
        again.status.success(),
        "{}",
        String::from_utf8_lossy(&again.stderr)
    );
    assert!(
        fs::read_to_string(&widget)
            .unwrap()
            .contains("reader's own note"),
        "repair overwrote a hand edit"
    );

    // Compilation is whole-model, so a selector is refused rather than
    // silently ignored.
    let scoped = jails_cmd(&root, None)
        .args(["entity", "repair", "Widget"])
        .output()
        .unwrap();
    assert!(!scoped.status.success());
    let scoped = String::from_utf8_lossy(&scoped.stderr);
    assert!(scoped.contains("takes no selector"), "{scoped}");
}
