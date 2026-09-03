//! `model eject`: one ejectable implementation leaves the accepted projection
//! and stays exactly where it is, which is already reader source.
//!
//! **Ejection is a lock edit, not a move.** Managed files live beside the
//! reader's own under `src/`, so there is no destination, no transfer and no
//! collision to refuse: what changes is that `.jails/compiler.lock.json` stops
//! naming the file, and from then on the compiler neither rewrites nor deletes
//! it.
use super::*;

/// Whether the accepted projection in the lock names this project path.
fn lock_names(root: &Path, relative: &str) -> bool {
    let lock = fs::read_to_string(root.join(".jails/compiler.lock.json")).unwrap();
    let lock: serde_json::Value = serde_json::from_str(&lock).unwrap();
    lock["projection"]["files"]
        .as_object()
        .is_some_and(|files| files.contains_key(relative))
}

#[test]
fn model_eject_leaves_the_file_in_place_and_the_lock_stops_naming_it() {
    let root = eject_model_project("model-eject");
    apply_canonical_model(&root, "initial-plan");
    const FAKE: &str =
        "src/main/java/com/example/notes/adapters/memory/InMemoryNoteRepository.java";
    let file = root.join(FAKE);
    let bytes = fs::read(&file).unwrap();
    let model_before = fs::read(root.join(".jails/model.jdl")).unwrap();
    assert!(lock_names(&root, FAKE));

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
    assert_eq!(fs::read(&file).unwrap(), bytes);
    assert!(lock_names(&root, FAKE));

    let applied = jails_cmd(&root, None)
        .args(["model", "eject", "art_ent_note_repository_memory"])
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    // The file did not move and was not rewritten; the lock let go of it.
    assert_eq!(fs::read(&file).unwrap(), bytes);
    assert!(
        !lock_names(&root, FAKE),
        "the lock still names the ejected file"
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(
        model.contains("eject art_ent_note_repository_memory @id(eject_"),
        "{model}"
    );
    // The managed ABI beside it is still managed.
    for relative in [
        "src/main/java/com/example/notes/domain/Note.java",
        "src/main/java/com/example/notes/repository/NoteRepository.java",
    ] {
        assert!(root.join(relative).exists(), "{relative} is gone");
        assert!(lock_names(&root, relative), "{relative} left the lock");
    }

    // A reader edit to the ejected file is nobody's drift.
    let mut edited = fs::read_to_string(&file).unwrap();
    edited.push_str("// reader-owned customization\n");
    fs::write(&file, &edited).unwrap();
    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(
        frozen.status.success(),
        "{}",
        String::from_utf8_lossy(&frozen.stderr)
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), edited);

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

/// JDL v1 §16.4: the preferred reference is a readable boundary path. The
/// path is what the source keeps and what the linker resolves on every read;
/// the artifact it releases is the one the id would have.
#[test]
fn model_eject_resolves_a_readable_boundary_path_to_the_same_artifact() {
    let root = eject_model_project("model-eject-path");
    apply_canonical_model(&root, "initial-plan");
    const FAKE: &str =
        "src/main/java/com/example/notes/adapters/memory/InMemoryNoteRepository.java";
    let bytes = fs::read(root.join(FAKE)).unwrap();

    let applied = jails_cmd(&root, None)
        .args(["model", "eject", "Note.repo.fake"])
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    assert_eq!(fs::read(root.join(FAKE)).unwrap(), bytes);
    assert!(!lock_names(&root, FAKE));
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(model.contains("eject Note.repo.fake @id(eject_"), "{model}");

    // The id and the path name one boundary, so the second ejection refuses
    // as the first one's.
    let retried = jails_cmd(&root, None)
        .args(["model", "eject", "art_ent_note_repository_memory"])
        .output()
        .unwrap();
    assert!(!retried.status.success());
    let stderr = String::from_utf8(retried.stderr).unwrap();
    assert!(stderr.contains("already reader-owned"), "{stderr}");

    // A path the registry does not carry refuses before anything is planned,
    // naming what the entity does have.
    let unknown = jails_cmd(&root, None)
        .args(["model", "eject", "Note.repo.mysql"])
        .output()
        .unwrap();
    assert!(!unknown.status.success());
    let stderr = String::from_utf8(unknown.stderr).unwrap();
    assert!(stderr.contains("`Note.repo.postgres`"), "{stderr}");
}

/// A boundary that emits nothing ejectable refuses by name, before any write.
#[test]
fn model_eject_refuses_managed_abi_without_writing() {
    let root = eject_model_project("model-eject-abi");
    apply_canonical_model(&root, "initial-plan");
    let before = snapshot_tree(&root);

    let output = jails_cmd(&root, None)
        .args(["model", "eject", "art_ent_note_record"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("managed ABI"), "{stderr}");
    assert_eq!(snapshot_tree(&root), before);
}

#[test]
fn canonical_controller_ejection_releases_the_whole_http_adapter_boundary() {
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
    const BOUNDARY: [&str; 2] = [
        "src/main/java/com/example/demo/web/HealthController.java",
        "src/test/java/com/example/demo/web/HealthControllerTest.java",
    ];
    for (index, relative) in BOUNDARY.iter().enumerate() {
        let path = root.join(relative);
        let source = fs::read_to_string(&path).unwrap();
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
    for (index, relative) in BOUNDARY.iter().enumerate() {
        assert!(!lock_names(&root, relative), "{relative} is still managed");
        assert!(
            fs::read_to_string(root.join(relative))
                .unwrap()
                .contains(&format!("ejected-controller-edit-{index}"))
        );
    }
    let exact = BOUNDARY
        .iter()
        .map(|relative| fs::read(root.join(relative)).unwrap())
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
    for (index, relative) in BOUNDARY.iter().enumerate() {
        assert_eq!(fs::read(root.join(relative)).unwrap(), exact[index]);
    }
    let jdl = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(jdl.contains(r#"route GET "/healthz""#), "{jdl}");

    fs::remove_dir_all(root).ok();
}

#[test]
fn factory_ejection_releases_only_the_testkit_implementation_boundary() {
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
    const FACTORY: &str = "src/test/java/com/example/notes/testkit/NoteFactory.java";
    const RECORD: &str = "src/main/java/com/example/notes/domain/Note.java";
    let factory = root.join(FACTORY);
    let record = root.join(RECORD);
    let ejected = jails_cmd(&root, None)
        .args(["model", "eject", "art_ent_note_factory"])
        .output()
        .unwrap();
    assert!(
        ejected.status.success(),
        "{}",
        String::from_utf8_lossy(&ejected.stderr)
    );
    assert!(factory.exists(), "factory ejection removed the factory");
    assert!(!lock_names(&root, FACTORY));
    assert!(record.exists(), "factory ejection removed the record ABI");
    assert!(lock_names(&root, RECORD));
    let mut owned = fs::read_to_string(&factory).unwrap();
    owned.push_str("// reader owns only this factory\n");
    fs::write(&factory, &owned).unwrap();

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
    assert_eq!(fs::read_to_string(&factory).unwrap(), owned);

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

/// `model eject` resolves the boundary against the project it is in: a
/// `BootCondition::Spring` capability pack emits nothing under
/// `spring_boot: None`, and an ejection resolved that way would refuse
/// "no ejectable Java implementation" with the files plainly on disk.
#[test]
fn canonical_eject_releases_a_spring_only_capability_pack() {
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
    const CONFIG: &str = "src/main/java/com/example/demo/messaging/KafkaConfig.java";
    let config = root.join(CONFIG);
    assert!(config.exists(), "the pack emitted no managed configuration");
    assert!(lock_names(&root, CONFIG));
    let bytes = fs::read(&config).unwrap();

    let ejected = jails_cmd(&root, None)
        .args(["model", "eject", "cap_kafka"])
        .output()
        .unwrap();
    assert!(
        ejected.status.success(),
        "{}",
        String::from_utf8_lossy(&ejected.stderr)
    );
    assert_eq!(
        fs::read(&config).unwrap(),
        bytes,
        "the implementation was moved or rewritten"
    );
    assert!(
        !lock_names(&root, CONFIG),
        "an ejected artifact is still in the accepted projection"
    );
}
