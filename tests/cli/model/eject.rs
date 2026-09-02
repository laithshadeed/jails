//! `model eject`: one ejectable implementation transferred into reader source
//! with a `Missing` before-image, and the refusals that keep the transfer from
//! overwriting anything.
//!
use super::*;

#[test]
fn model_eject_transfers_generated_java_once_and_reader_edits_survive() {
    let root = eject_model_project("model-eject");
    apply_canonical_model(&root, "initial-plan");
    let generated = root.join(
        ".jails/generated/main/java/com/example/notes/adapters/memory/InMemoryNoteRepository.java",
    );
    let reader =
        root.join("src/main/java/com/example/notes/adapters/memory/InMemoryNoteRepository.java");
    let generated_bytes = fs::read(&generated).unwrap();
    let model_before = fs::read(root.join(".jails/model.jdl")).unwrap();

    let preview = jails_cmd(&root, None)
        .args([
            "model",
            "eject",
            "art_ent_note_repository_memory",
            "--pretend",
            "--diff",
        ])
        .output()
        .unwrap();
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    assert_eq!(
        fs::read(root.join(".jails/model.jdl")).unwrap(),
        model_before
    );
    assert_eq!(fs::read(&generated).unwrap(), generated_bytes);
    assert!(!reader.exists());

    let applied = jails_cmd(&root, None)
        .args(["model", "eject", "art_ent_note_repository_memory"])
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    assert!(!generated.exists());
    assert_eq!(fs::read(&reader).unwrap(), generated_bytes);
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(
        model.contains("eject art_ent_note_repository_memory @id(eject_"),
        "{model}"
    );
    assert!(
        root.join(".jails/generated/main/java/com/example/notes/domain/Note.java")
            .exists()
    );
    assert!(
        root.join(".jails/generated/main/java/com/example/notes/repository/NoteRepository.java")
            .exists()
    );

    let mut edited = fs::read_to_string(&reader).unwrap();
    edited.push_str("// reader-owned customization\n");
    fs::write(&reader, &edited).unwrap();
    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(
        frozen.status.success(),
        "{}",
        String::from_utf8_lossy(&frozen.stderr)
    );
    assert_eq!(fs::read_to_string(&reader).unwrap(), edited);

    let before_retry = snapshot_tree(&root);
    let retried = jails_cmd(&root, None)
        .args(["model", "eject", "art_ent_note_repository_memory"])
        .output()
        .unwrap();
    assert!(!retried.status.success());
    let stderr = String::from_utf8(retried.stderr).unwrap();
    assert!(stderr.contains("already reader-owned"), "{stderr}");
    assert_eq!(snapshot_tree(&root), before_retry);
}

#[test]
fn model_eject_refuses_a_reader_destination_collision_without_writing() {
    let root = eject_model_project("model-eject-collision");
    apply_canonical_model(&root, "initial-plan");
    let reader =
        root.join("src/main/java/com/example/notes/adapters/memory/InMemoryNoteRepository.java");
    fs::create_dir_all(reader.parent().unwrap()).unwrap();
    fs::write(&reader, "package com.example.notes.domain;\n// mine\n").unwrap();
    let before = snapshot_tree(&root);

    let output = jails_cmd(&root, None)
        .args(["model", "eject", "art_ent_note_repository_memory"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("already exists"), "{stderr}");
    assert!(stderr.contains("move or remove"), "{stderr}");
    assert_eq!(snapshot_tree(&root), before);
}

#[test]
fn model_eject_plan_refuses_a_destination_created_after_review() {
    let root = eject_model_project("model-eject-stale");
    apply_canonical_model(&root, "initial-plan");
    let plan = root.join("eject-plan.json");
    let planned = jails_cmd(&root, None)
        .args([
            "model",
            "eject",
            "art_ent_note_repository_memory",
            "--plan-out",
        ])
        .arg(&plan)
        .output()
        .unwrap();
    assert!(
        planned.status.success(),
        "{}",
        String::from_utf8_lossy(&planned.stderr)
    );
    let model_before = fs::read(root.join(".jails/model.jdl")).unwrap();
    let generated = root.join(
        ".jails/generated/main/java/com/example/notes/adapters/memory/InMemoryNoteRepository.java",
    );
    let reader =
        root.join("src/main/java/com/example/notes/adapters/memory/InMemoryNoteRepository.java");
    fs::create_dir_all(reader.parent().unwrap()).unwrap();
    fs::write(
        &reader,
        "package com.example.notes.domain;\n// appeared later\n",
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
    assert_eq!(
        fs::read(root.join(".jails/model.jdl")).unwrap(),
        model_before
    );
    assert!(generated.exists());
    assert_eq!(
        fs::read_to_string(&reader).unwrap(),
        "package com.example.notes.domain;\n// appeared later\n"
    );
}

#[test]
fn canonical_controller_ejection_transfers_the_whole_http_adapter_boundary() {
    let root = temp_dir("canonical-controller-ejection");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();
    let generated = jails_cmd(&root, None)
        .args(["g", "controller", "Health", "--path", "/health"])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let managed = [
        root.join(".jails/generated/main/java/com/example/demo/web/HealthController.java"),
        root.join(".jails/generated/test/java/com/example/demo/web/HealthControllerTest.java"),
    ];
    for (index, path) in managed.iter().enumerate() {
        let source = fs::read_to_string(path).unwrap();
        let split = source.rfind("\n}").unwrap();
        fs::write(
            path,
            format!(
                "{}\n\n    // ejected-controller-edit-{index}{}",
                &source[..split],
                &source[split..]
            ),
        )
        .unwrap();
    }

    let ejected = jails_cmd(&root, None)
        .args(["model", "eject", "art_cmp_controller_health_http"])
        .output()
        .unwrap();
    assert!(
        ejected.status.success(),
        "{}",
        String::from_utf8_lossy(&ejected.stderr)
    );
    let reader = [
        common::generated(
            &root,
            "src/main/java/com/example/demo/web/HealthController.java",
        ),
        common::generated(
            &root,
            "src/test/java/com/example/demo/web/HealthControllerTest.java",
        ),
    ];
    assert!(managed.iter().all(|path| !path.exists()));
    for (index, path) in reader.iter().enumerate() {
        assert!(
            fs::read_to_string(path)
                .unwrap()
                .contains(&format!("ejected-controller-edit-{index}"))
        );
    }
    let exact = reader
        .iter()
        .map(|path| fs::read(path).unwrap())
        .collect::<Vec<_>>();

    let evolved = jails_cmd(&root, None)
        .args(["g", "controller", "Health", "--path", "/healthz"])
        .output()
        .unwrap();
    assert!(
        evolved.status.success(),
        "{}",
        String::from_utf8_lossy(&evolved.stderr)
    );
    for (index, path) in reader.iter().enumerate() {
        assert_eq!(fs::read(path).unwrap(), exact[index]);
    }
    let jdl = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(jdl.contains(r#"route GET "/healthz""#), "{jdl}");

    fs::remove_dir_all(root).ok();
}

#[test]
fn factory_ejection_transfers_only_the_testkit_implementation_boundary() {
    let root = jdl_project("model-jdl-factory-eject", NOTES_JDL);
    for command in [
        ["g", "record", "Note", "title:string!"].as_slice(),
        ["g", "factory", "Note"].as_slice(),
    ] {
        let output = jails_cmd(&root, None).args(command).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let generated =
        root.join(".jails/generated/test/java/com/example/notes/testkit/NoteFactory.java");
    let reader = root.join("src/test/java/com/example/notes/testkit/NoteFactory.java");
    let record = root.join(".jails/generated/main/java/com/example/notes/domain/Note.java");
    let ejected = jails_cmd(&root, None)
        .args(["model", "eject", "art_ent_note_factory"])
        .output()
        .unwrap();
    assert!(
        ejected.status.success(),
        "{}",
        String::from_utf8_lossy(&ejected.stderr)
    );
    assert!(!generated.exists());
    assert!(record.exists(), "factory ejection removed the record ABI");
    let mut owned = fs::read_to_string(&reader).unwrap();
    owned.push_str("// reader owns only this factory\n");
    fs::write(&reader, &owned).unwrap();

    let evolved = jails_cmd(&root, None)
        .args(["g", "field", "Note", "priority:int"])
        .output()
        .unwrap();
    assert!(
        evolved.status.success(),
        "{}",
        String::from_utf8_lossy(&evolved.stderr)
    );
    assert!(
        fs::read_to_string(&record)
            .unwrap()
            .contains("int priority"),
        "managed ABI did not evolve"
    );
    assert_eq!(fs::read_to_string(&reader).unwrap(), owned);
    assert!(!generated.exists());

    let before = snapshot_tree(&root);
    let refused = jails_cmd(&root, None)
        .args(["destroy", "factory", "Note", "--force"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("reader-owned"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert_eq!(snapshot_tree(&root), before, "ejected destroy wrote bytes");
}

/// `model eject` resolves the boundary against the project it is in.
///
/// It re-emits the tree to find which files an ejection owns, and that
/// emission has to see the captured Boot version: a `BootCondition::Spring`
/// capability pack emits nothing under `spring_boot: None`, and an ejection
/// resolved that way refuses "emits no ejectable Java implementation" with
/// the files plainly on disk.
#[test]
fn canonical_eject_transfers_a_spring_only_capability_pack() {
    let root = temp_dir("canonical-eject-spring-pack");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/model.jdl"),
        "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  \
         platform spring\n  build maven\n  storage none\n}\n",
    )
    .unwrap();

    let added = jails_cmd(&root, None)
        .args(["add", "kafka", "--no-start"])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let managed =
        root.join(".jails/generated/main/java/com/example/demo/messaging/KafkaConfig.java");
    assert!(
        managed.exists(),
        "the pack emitted no managed configuration"
    );

    let ejected = jails_cmd(&root, None)
        .args(["model", "eject", "cap_kafka"])
        .output()
        .unwrap();
    assert!(
        ejected.status.success(),
        "{}",
        String::from_utf8_lossy(&ejected.stderr)
    );
    assert!(
        common::generated(
            &root,
            "src/main/java/com/example/demo/messaging/KafkaConfig.java"
        )
        .exists(),
        "the implementation was not transferred to reader source"
    );
    assert!(
        !managed.exists(),
        "an ejected artifact is still in the managed tree"
    );
}
