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
}
