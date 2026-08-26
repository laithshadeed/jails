//! Named SQL contracts through the real CLI binary.

use super::*;

fn sql_fixture(label: &str) -> PathBuf {
    let root = temp_dir(label);
    write_project_skeleton(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();
    fs::create_dir_all(root.join("src/main/resources/db/queries")).unwrap();
    fs::write(
        root.join(".jails/app.toml"),
        r#"schema = "jails.app.v1"
[application]
name = "Example"
base_package = "org.example.sample"
java_release = 26
dialect = "postgresql"
[slices.Sample]
[slices.Sample.queries.FindEntries]
source = "src/main/resources/db/queries/FindEntries.sql"
"#,
    )
    .unwrap();
    fs::write(
        root.join("src/main/resources/db/migration/V001__entries.sql"),
        "CREATE TABLE entries (id uuid PRIMARY KEY, state text NOT NULL);",
    )
    .unwrap();
    fs::write(
        root.join("src/main/resources/db/queries/FindEntries.sql"),
        "-- jails:name FindEntries\n-- jails:cardinality many\n-- jails:param state text\nSELECT id, state FROM entries WHERE state = :state;\n",
    )
    .unwrap();
    root
}

#[test]
fn sql_check_compiles_a_manifest_query_offline() {
    let root = sql_fixture("sql-check");
    let output = jails_cmd(&root, None)
        .args(["sql", "check", "--offline"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Sample.FindEntries"), "{stdout}");
    assert!(stdout.contains("verified-offline"), "{stdout}");
}

#[test]
fn frozen_offline_check_refuses_a_missing_contract_without_writing() {
    let root = sql_fixture("sql-frozen");
    let before = snapshot_tree(&root);
    let output = jails_cmd(&root, None)
        .args(["sql", "check", "--offline", "--frozen"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("frozen SQL contract"), "{stderr}");
    assert!(stderr.contains("fix:"), "{stderr}");
    assert_eq!(snapshot_tree(&root), before);
}

#[test]
fn sql_generate_preview_and_apply_share_one_transaction_then_frozen_passes() {
    let root = sql_fixture("sql-generate");
    let paths = [
        "src/main/java/org/example/sample/sample/application/query/FindEntries.java",
        "src/main/java/org/example/sample/sample/adapter/jdbc/JdbcFindEntries.java",
        "src/test/java/org/example/sample/sample/adapter/query/FakeFindEntries.java",
        "src/test/java/org/example/sample/sample/adapter/query/FindEntriesContractTest.java",
        ".jails/sql-contracts/sample/find-entries.json",
    ];
    let before = snapshot_tree(&root);
    let preview = jails_cmd(&root, None)
        .args(["--pretend", "sql", "generate", "FindEntries"])
        .output()
        .unwrap();
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    assert_eq!(snapshot_tree(&root), before);
    let preview_text = String::from_utf8_lossy(&preview.stdout);
    for path in paths {
        assert!(
            preview_text.contains(path),
            "{path} missing from {preview_text}"
        );
    }

    let applied = jails_cmd(&root, None)
        .args(["sql", "generate", "FindEntries"])
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}{}",
        String::from_utf8_lossy(&applied.stdout),
        String::from_utf8_lossy(&applied.stderr)
    );
    let applied_text = String::from_utf8_lossy(&applied.stdout);
    for path in paths {
        assert!(
            applied_text.contains(path),
            "{path} missing from {applied_text}"
        );
        assert!(root.join(path).is_file(), "{path} was not committed");
    }

    let frozen = jails_cmd(&root, None)
        .args(["sql", "check", "--offline", "--frozen"])
        .output()
        .unwrap();
    assert!(
        frozen.status.success(),
        "{}",
        String::from_utf8_lossy(&frozen.stderr)
    );
}
