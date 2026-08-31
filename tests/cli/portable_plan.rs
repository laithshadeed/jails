use super::*;

#[test]
fn exported_plan_applies_the_exact_prepared_transaction_without_replanning() {
    let root = temp_dir("portable-plan-exact");
    let plan_dir = temp_dir("portable-plan-file");
    let plan = plan_dir.join("record.json");
    write_spring_fixture(&root);

    let preview = jails_cmd(&root, None)
        .args([
            "g",
            "record",
            "Note",
            "text:string!",
            "--pretend",
            "--plan-out",
            plan.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(preview.status.success(), "{preview:?}");
    assert!(plan.is_file());
    assert!(
        !root
            .join("src/main/java/com/example/demo/domain/Note.java")
            .exists(),
        "plan-out mutated the project"
    );
    let wire: serde_json::Value = serde_json::from_slice(&fs::read(&plan).unwrap()).unwrap();
    assert_eq!(wire["schema"], "jails.prepared-plan.v1");
    assert_eq!(wire["protocol_version"], 1);
    assert_eq!(wire["project_root_digest"].as_str().unwrap().len(), 64);
    assert_eq!(wire["prepared_after"].as_str().unwrap().len(), 64);
    assert_eq!(wire["plan_digest"].as_str().unwrap().len(), 64);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&plan).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    // No semantic arguments are supplied. The authenticated plan is the only
    // source of the Note transaction.
    let applied = jails_cmd(&root, None)
        .args(["g", "record", "--plan-in", plan.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(applied.status.success(), "{applied:?}");
    assert!(common::generated(&root, "src/main/java/com/example/demo/domain/Note.java").is_file());
}

#[test]
fn imported_plan_refuses_another_root_without_writing() {
    let first = temp_dir("portable-plan-root-first");
    let second = temp_dir("portable-plan-root-second");
    let plan_dir = temp_dir("portable-plan-root-file");
    let plan = plan_dir.join("record.json");
    write_spring_fixture(&first);
    write_spring_fixture(&second);
    let exported = jails_cmd(&first, None)
        .args([
            "g",
            "record",
            "Note",
            "--pretend",
            "--plan-out",
            plan.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(exported.status.success(), "{exported:?}");

    let before = snapshot_tree(&second);
    let refused = jails_cmd(&second, None)
        .args(["g", "record", "--plan-in", plan.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!refused.status.success(), "{refused:?}");
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("different canonical project root"),
        "{refused:?}"
    );
    assert_eq!(snapshot_tree(&second), before);
}

#[test]
fn imported_plan_refuses_a_changed_preimage_without_writing() {
    let root = temp_dir("portable-plan-stale-preimage");
    let plan_dir = temp_dir("portable-plan-stale-file");
    let plan = plan_dir.join("add-csv.json");
    write_spring_fixture(&root);
    let exported = jails_cmd(&root, None)
        .args([
            "add",
            "csv",
            "--pretend",
            "--plan-out",
            plan.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(exported.status.success(), "{exported:?}");

    let pom = root.join("pom.xml");
    let mut edited = fs::read_to_string(&pom).unwrap();
    edited.push_str("\n<!-- reader edit after planning -->\n");
    fs::write(&pom, edited).unwrap();
    let before_pom = fs::read(&pom).unwrap();
    let refused = jails_cmd(&root, None)
        .args(["add", "csv", "--plan-in", plan.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!refused.status.success(), "{refused:?}");
    assert_eq!(fs::read(&pom).unwrap(), before_pom);
    assert!(
        !root
            .join("src/main/java/com/example/demo/Csv.java")
            .exists()
    );
    assert!(!root.join(".jails/state").exists());
}

#[test]
fn imported_plan_refuses_protocol_and_toolchain_mismatches() {
    let root = temp_dir("portable-plan-version-mismatch");
    let plan_dir = temp_dir("portable-plan-version-file");
    let plan = plan_dir.join("record.json");
    write_spring_fixture(&root);
    let exported = jails_cmd(&root, None)
        .args([
            "g",
            "record",
            "Note",
            "--pretend",
            "--plan-out",
            plan.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(exported.status.success(), "{exported:?}");
    let original = fs::read(&plan).unwrap();
    let before = snapshot_tree(&root);

    let mut protocol: serde_json::Value = serde_json::from_slice(&original).unwrap();
    protocol["protocol_version"] = serde_json::json!(2);
    fs::write(&plan, serde_json::to_vec_pretty(&protocol).unwrap()).unwrap();
    let refused = jails_cmd(&root, None)
        .args(["g", "record", "--plan-in", plan.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!refused.status.success(), "{refused:?}");
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("protocol 2"),
        "{refused:?}"
    );
    assert_eq!(snapshot_tree(&root), before);

    let mut toolchain: serde_json::Value = serde_json::from_slice(&original).unwrap();
    toolchain["tool_version"] = serde_json::json!("999.0.0");
    fs::write(&plan, serde_json::to_vec_pretty(&toolchain).unwrap()).unwrap();
    let refused = jails_cmd(&root, None)
        .args(["g", "record", "--plan-in", plan.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!refused.status.success(), "{refused:?}");
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("999.0.0"),
        "{refused:?}"
    );
    assert_eq!(snapshot_tree(&root), before);
}
