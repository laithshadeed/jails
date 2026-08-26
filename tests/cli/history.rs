use super::*;

#[test]
fn history_and_show_project_authenticated_receipts() {
    let root = temp_dir("history-show-receipts");
    write_spring_fixture(&root);
    let generated = jails_cmd(&root, None)
        .args(["g", "record", "Note", "text:string!"])
        .output()
        .unwrap();
    assert!(generated.status.success(), "{generated:?}");

    let history = jails_cmd(&root, None)
        .args(["history", "--limit", "1", "--output", "json"])
        .output()
        .unwrap();
    assert!(history.status.success(), "{history:?}");
    let value: serde_json::Value = serde_json::from_slice(&history.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    let receipts = value["receipts"].as_array().unwrap();
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0]["undo_eligible"], true);
    assert_eq!(receipts[0]["reason"], "reconcile");
    assert_eq!(receipts[0]["risk"], serde_json::json!(["ordinary"]));
    assert_eq!(receipts[0]["external_effect"], "none");
    assert_eq!(
        receipts[0]["evidence"]["snapshot"].as_str().unwrap().len(),
        64
    );
    assert!(receipts[0]["files"][0]["owners"].is_array());
    assert_eq!(receipts[0]["files"][0]["after"].as_str().unwrap().len(), 64);
    let transaction = receipts[0]["transaction_id"].as_str().unwrap();

    let shown = jails_cmd(&root, None)
        .args(["show", transaction, "--diff", "--why", "--output", "json"])
        .output()
        .unwrap();
    assert!(shown.status.success(), "{shown:?}");
    let shown: serde_json::Value = serde_json::from_slice(&shown.stdout).unwrap();
    assert_eq!(shown["schema_version"], 1);
    assert_eq!(shown["receipt"]["transaction_id"], transaction);
    assert_eq!(shown["receipt"]["files"][0]["kind"], "create");
    assert_eq!(
        shown["receipt"]["files"][0]["before"],
        serde_json::Value::Null
    );
    assert!(shown["why"]["toolchain_records"].is_number());
    assert!(shown["diff"].as_str().unwrap().contains("Note.java"));
    assert_eq!(shown["why"]["semantics"], "apply");
}

#[test]
fn migration_receipts_explain_why_file_undo_is_refused() {
    let root = temp_dir("history-migration-undo-refusal");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();
    let generated = jails_cmd(&root, None)
        .args(["g", "scaffold", "Task", "id:uuid@pk"])
        .output()
        .unwrap();
    assert!(generated.status.success(), "{generated:?}");

    let history = jails_cmd(&root, None)
        .args(["history", "--limit", "1", "--output", "json"])
        .output()
        .unwrap();
    assert!(history.status.success(), "{history:?}");
    let value: serde_json::Value = serde_json::from_slice(&history.stdout).unwrap();
    let receipt = &value["receipts"][0];
    assert_eq!(receipt["undo_eligible"], false);
    assert!(
        receipt["undo_reason"]
            .as_str()
            .unwrap()
            .starts_with("contains-migration:")
    );
    let transaction = receipt["transaction_id"].as_str().unwrap();
    let before = snapshot_tree(&root);
    let refused = jails_cmd(&root, None)
        .args(["undo", transaction])
        .output()
        .unwrap();
    assert!(!refused.status.success(), "{refused:?}");
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("forward corrective migration"),
        "{refused:?}"
    );
    assert_eq!(snapshot_tree(&root), before);
}

#[test]
fn undo_restores_exact_preimages_as_a_new_forward_transaction() {
    let root = temp_dir("receipt-file-undo");
    write_spring_fixture(&root);
    let generated = jails_cmd(&root, None)
        .args(["g", "record", "Note", "text:string!"])
        .output()
        .unwrap();
    assert!(generated.status.success(), "{generated:?}");
    let history = jails_cmd(&root, None)
        .args(["history", "--limit", "1", "--output", "json"])
        .output()
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&history.stdout).unwrap();
    let transaction = value["receipts"][0]["transaction_id"].as_str().unwrap();
    let source = root.join("src/main/java/com/example/demo/domain/Note.java");
    assert!(source.is_file());

    let preview = jails_cmd(&root, None)
        .args(["undo", transaction, "--pretend", "--output", "json"])
        .output()
        .unwrap();
    assert!(preview.status.success(), "{preview:?}");
    assert!(source.is_file(), "preview removed a project file");
    let preview: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert_eq!(preview["status"], "preview");

    let undone = jails_cmd(&root, None)
        .args(["undo", transaction])
        .output()
        .unwrap();
    assert!(undone.status.success(), "{undone:?}");
    assert!(!source.exists());
    assert!(
        !root
            .join("src/test/java/com/example/demo/domain/NoteTest.java")
            .exists()
    );

    let regenerated = jails_cmd(&root, None)
        .args(["g", "record", "Note", "text:string!"])
        .output()
        .unwrap();
    assert!(regenerated.status.success(), "{regenerated:?}");
    assert!(source.is_file());
}

#[test]
fn undo_refuses_a_user_edited_after_image_without_writing() {
    let root = temp_dir("receipt-file-undo-edited");
    write_spring_fixture(&root);
    let generated = jails_cmd(&root, None)
        .args(["g", "record", "Note", "text:string!"])
        .output()
        .unwrap();
    assert!(generated.status.success(), "{generated:?}");
    let history = jails_cmd(&root, None)
        .args(["history", "--limit", "1", "--output", "json"])
        .output()
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&history.stdout).unwrap();
    let transaction = value["receipts"][0]["transaction_id"].as_str().unwrap();
    let source = root.join("src/main/java/com/example/demo/domain/Note.java");
    fs::write(&source, "// reader edit\n").unwrap();
    let before = snapshot_tree(&root);

    let refused = jails_cmd(&root, None)
        .args(["undo", transaction, "--merge"])
        .output()
        .unwrap();
    assert!(!refused.status.success(), "{refused:?}");
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("undo-after-image-changed"),
        "{refused:?}"
    );
    assert_eq!(snapshot_tree(&root), before);
}
