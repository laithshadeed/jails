//! `destroy` is subtraction: the declaration goes, the tree is recompiled, and
//! an operation edge still pointing at the declaration refuses.
//!
use super::*;

#[test]
fn canonical_source_unit_destroy_removes_only_the_selected_artifacts() {
    let root = temp_dir("canonical-source-unit-destroy");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();
    let generated = jails_cmd(&root, None)
        .args(["g", "service", "BillingService"])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let destroyed = jails_cmd(&root, None)
        .args(["destroy", "service", "BillingService", "--force"])
        .output()
        .unwrap();
    assert!(
        destroyed.status.success(),
        "{}",
        String::from_utf8_lossy(&destroyed.stderr)
    );
    assert!(
        !root
            .join("src/main/java/com/example/demo/service/BillingService.java")
            .exists()
    );
    assert!(
        !root
            .join("src/test/java/com/example/demo/service/BillingServiceTest.java")
            .exists()
    );

    let generated = jails_cmd(&root, None)
        .args(["g", "integration-test", "CheckoutIT"])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    assert!(
        fs::read_to_string(root.join("pom.xml"))
            .unwrap()
            .contains("maven-failsafe-plugin")
    );
    let destroyed = jails_cmd(&root, None)
        .args(["destroy", "integration-test", "CheckoutIT", "--force"])
        .output()
        .unwrap();
    assert!(
        destroyed.status.success(),
        "{}",
        String::from_utf8_lossy(&destroyed.stderr)
    );
    assert!(
        !root
            .join("src/test/java/com/example/demo/CheckoutIT.java")
            .exists()
    );
    assert!(
        !fs::read_to_string(root.join("pom.xml"))
            .unwrap()
            .contains("maven-failsafe-plugin")
    );
    fs::remove_dir_all(root).ok();
}

#[test]
fn jdl_destroy_removes_nested_operations_and_entities_without_legacy_state() {
    let root = jdl_project("model-jdl-destroy", NOTES_JDL);
    for arguments in [
        vec!["g", "record", "Task", "title:string!"],
        vec!["g", "enum", "Status", "OPEN", "CLOSED"],
        vec!["g", "query", "OpenTasks", "title", "--on", "Task"],
    ] {
        let output = jails_cmd(&root, None).args(arguments).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let destroyed_query = jails_cmd(&root, None)
        .args(["destroy", "query", "OpenTasks", "--force"])
        .output()
        .unwrap();
    assert!(
        destroyed_query.status.success(),
        "{}",
        String::from_utf8_lossy(&destroyed_query.stderr)
    );
    let source = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(!source.contains("query OpenTasks"), "{source}");
    assert!(source.contains("entity Task"), "{source}");

    let destroyed_enum = jails_cmd(&root, None)
        .args(["destroy", "enum", "Status", "--force"])
        .output()
        .unwrap();
    assert!(
        destroyed_enum.status.success(),
        "{}",
        String::from_utf8_lossy(&destroyed_enum.stderr)
    );
    let source = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(!source.contains("enum Status"), "{source}");
    assert!(source.contains("entity Task"), "{source}");

    let destroyed_entity = jails_cmd(&root, None)
        .args(["destroy", "record", "Task", "--force"])
        .output()
        .unwrap();
    assert!(
        destroyed_entity.status.success(),
        "{}",
        String::from_utf8_lossy(&destroyed_entity.stderr)
    );
    let source = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(!source.contains("entity Task"), "{source}");
    assert!(!root.join(".jails/ledger.toml").exists());
    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(frozen.status.success());
}

#[test]
fn canonical_destroy_is_model_subtraction_and_whole_tree_recompilation() {
    let root = model_project("model-destroy-entity", EMPTY_MODEL);
    let generated = jails_cmd(&root, None)
        .args(["g", "scaffold", "Note", "id:uuid@pk", "title:string!"])
        .output()
        .unwrap();
    assert!(generated.status.success());
    let before = snapshot_tree(&root);

    let preview = jails_cmd(&root, None)
        .args(["d", "scaffold", "Note", "--pretend", "--force"])
        .output()
        .unwrap();
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    assert_eq!(snapshot_tree(&root), before, "destroy preview wrote files");

    let plan_directory = temp_dir("model-destroy-plan");
    let plan = plan_directory.join("destroy.json");
    let planned = jails_cmd(&root, None)
        .args(["d", "scaffold", "Note", "--force", "--plan-out"])
        .arg(&plan)
        .output()
        .unwrap();
    assert!(
        planned.status.success(),
        "{}",
        String::from_utf8_lossy(&planned.stderr)
    );
    assert_eq!(snapshot_tree(&root), before, "plan-out applied destroy");
    let bundle: jails_contracts::PlanBundle =
        serde_json::from_slice(&fs::read(&plan).unwrap()).unwrap();
    assert!(bundle.plan.operations.iter().any(|operation| matches!(
        operation,
        jails_contracts::PlannedOperation::PublishMergedTree { .. }
    )));
    assert!(bundle.plan.operations.iter().any(|operation| matches!(
        operation,
        jails_contracts::PlannedOperation::ReplaceModelFile { .. }
    )));
    assert!(matches!(
        bundle.plan.operations.last(),
        Some(jails_contracts::PlannedOperation::ReplaceStateFile { path, .. })
            if path.as_str() == ".jails/compiler.lock.json"
    ));

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
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(!model.contains("entity Note"), "{model}");
    assert!(
        !root
            .join("src/main/java/com/example/notes/domain/Note.java")
            .exists()
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
fn canonical_destroy_refuses_while_operations_reference_the_entity() {
    let root = model_project("model-destroy-referenced", EMPTY_MODEL);
    for arguments in [
        vec!["g", "scaffold", "Note", "id:uuid@pk", "title:string!"],
        vec!["g", "query", "OpenNotes", "title", "--on", "Note"],
    ] {
        let output = jails_cmd(&root, None).args(arguments).output().unwrap();
        assert!(output.status.success());
    }
    let before = snapshot_tree(&root);
    let refused = jails_cmd(&root, None)
        .args(["d", "scaffold", "Note", "--force"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    let stderr = String::from_utf8(refused.stderr).unwrap();
    assert!(stderr.contains("operation OpenNotes"), "{stderr}");
    assert!(stderr.contains("pointing at nothing"), "{stderr}");
    assert_eq!(snapshot_tree(&root), before, "refusal mutated the project");

    let removed_query = jails_cmd(&root, None)
        .args(["d", "query", "OpenNotes", "--force"])
        .output()
        .unwrap();
    assert!(
        removed_query.status.success(),
        "{}",
        String::from_utf8_lossy(&removed_query.stderr)
    );
    assert!(
        !root
            .join("src/main/java/com/example/notes/application/queries/OpenNotesQuery.java")
            .exists()
    );
    let removed_entity = jails_cmd(&root, None)
        .args(["d", "scaffold", "Note", "--force"])
        .output()
        .unwrap();
    assert!(
        removed_entity.status.success(),
        "{}",
        String::from_utf8_lossy(&removed_entity.stderr)
    );
    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(frozen.status.success());
}
