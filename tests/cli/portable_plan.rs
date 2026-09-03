//! `--plan-out` / `--plan-in`: the exact reviewed transition, moved as a file.
//!
//! Every assertion here is about `PlanBundle`'s contract rather than about any
//! one command: the bundle *is* the review, so applying it never replans, and
//! the executor refuses anything the reviewer did not see — a tree that has
//! moved on, a different project, a tampered file.

use super::*;

/// Read the exported bundle, asserting the envelope every other test leans on.
fn exported_bundle(plan: &Path) -> serde_json::Value {
    let wire: serde_json::Value = serde_json::from_slice(&fs::read(plan).unwrap()).unwrap();
    assert_eq!(wire["schema"], "jails.plan-bundle.v1");
    assert_eq!(wire["plan"]["schema"], "jails.plan.v1");
    let digest = wire["plan"]["digest"].as_str().unwrap();
    assert!(digest.starts_with("sha256:"), "{digest}");
    assert_eq!(digest.len(), "sha256:".len() + 64);
    // The plan's id *is* its digest, which is what makes preview, export,
    // confirmation and apply able to name one transition.
    assert_eq!(wire["plan"]["id"].as_str().unwrap(), digest);
    assert!(!wire["plan"]["compiler"].as_str().unwrap().is_empty());
    assert!(
        !wire["plan"]["base"]["files"]
            .as_object()
            .unwrap()
            .is_empty(),
        "an exported plan carries the preimages it was reviewed against"
    );
    wire
}

#[test]
fn exported_plan_applies_the_exact_prepared_transaction_without_replanning() {
    let root = temp_dir("portable-plan-exact");
    let plan_dir = temp_dir("portable-plan-file");
    let plan = plan_dir.join("record.json");
    write_spring_fixture(&root);
    common::become_canonical(&root);

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
        !common::generated(&root, "src/main/java/com/example/demo/domain/Note.java").exists(),
        "plan-out mutated the project"
    );
    exported_bundle(&plan);
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
    common::become_canonical(&first);
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

    // A plan is portable by *content*, not by path: the second project never
    // had the model the first one was reviewed against, so every captured
    // precondition is about a file that is not there.
    let before = snapshot_tree(&second);
    let refused = jails_cmd(&second, None)
        .args(["g", "record", "--plan-in", plan.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!refused.status.success(), "{refused:?}");
    assert!(
        String::from_utf8_lossy(&refused.stderr)
            .contains("no longer matches what this plan was reviewed against"),
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
    common::become_canonical(&root);
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
    assert!(!common::generated(&root, "src/main/java/com/example/demo/Csv.java").exists());
}

#[test]
fn imported_plan_refuses_a_tampered_bundle_without_writing() {
    let root = temp_dir("portable-plan-tampered");
    let plan_dir = temp_dir("portable-plan-tampered-file");
    let plan = plan_dir.join("record.json");
    write_spring_fixture(&root);
    common::become_canonical(&root);
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
    exported_bundle(&plan);
    let before = snapshot_tree(&root);

    // The protocol half: a bundle written by a jails that speaks a different
    // exact-plan schema is refused rather than read optimistically.
    let mut schema: serde_json::Value = serde_json::from_slice(&original).unwrap();
    schema["schema"] = serde_json::json!("jails.plan-bundle.v2");
    fs::write(&plan, serde_json::to_vec_pretty(&schema).unwrap()).unwrap();
    let refused = jails_cmd(&root, None)
        .args(["g", "record", "--plan-in", plan.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!refused.status.success(), "{refused:?}");
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("schema"),
        "{refused:?}"
    );
    assert_eq!(snapshot_tree(&root), before);

    // The toolchain half: the compiler that produced the plan is inside the
    // digest, so restamping it is tampering and the executor says so.
    let mut toolchain: serde_json::Value = serde_json::from_slice(&original).unwrap();
    toolchain["plan"]["compiler"] = serde_json::json!("999.0.0");
    fs::write(&plan, serde_json::to_vec_pretty(&toolchain).unwrap()).unwrap();
    let refused = jails_cmd(&root, None)
        .args(["g", "record", "--plan-in", plan.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!refused.status.success(), "{refused:?}");
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("digest"),
        "{refused:?}"
    );
    assert_eq!(snapshot_tree(&root), before);

    // And an operation whose content-addressed blob was swapped underneath it.
    let mut blobs: serde_json::Value = serde_json::from_slice(&original).unwrap();
    let id = blobs["blobs"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    blobs["blobs"][&id] = serde_json::json!([0, 1, 2]);
    fs::write(&plan, serde_json::to_vec_pretty(&blobs).unwrap()).unwrap();
    let refused = jails_cmd(&root, None)
        .args(["g", "record", "--plan-in", plan.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!refused.status.success(), "{refused:?}");
    assert_eq!(snapshot_tree(&root), before);
}
