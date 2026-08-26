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
type_mappings = { "public.order_status" = "java.lang.String" }
[slices.Sample]
[slices.Sample.queries.FindPayableOrders]
source = "src/main/resources/db/queries/FindPayableOrders.sql"
"#,
    )
    .unwrap();
    fs::write(
        root.join("src/main/resources/db/migration/V001__orders.sql"),
        "CREATE TABLE orders (id uuid PRIMARY KEY, account_id uuid NOT NULL, total numeric NOT NULL, status public.order_status NOT NULL, created_at timestamptz NOT NULL);",
    )
    .unwrap();
    fs::write(
        root.join("src/main/resources/db/queries/FindPayableOrders.sql"),
        "-- jails:name FindPayableOrders\n-- jails:cardinality many\n-- jails:param status public.order_status\n-- jails:param minimum numeric\n-- jails:param limit int4\nSELECT id, account_id, total, status, created_at\nFROM orders\nWHERE status = :status AND total >= :minimum\nORDER BY created_at, id\nLIMIT :limit;\n",
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
    assert!(stdout.contains("Sample.FindPayableOrders"), "{stdout}");
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
        "src/main/java/org/example/sample/sample/application/query/FindPayableOrders.java",
        "src/main/java/org/example/sample/sample/adapter/jdbc/JdbcFindPayableOrders.java",
        "src/test/java/org/example/sample/sample/adapter/query/FakeFindPayableOrders.java",
        "src/test/java/org/example/sample/sample/adapter/query/FindPayableOrdersContractTest.java",
        ".jails/sql-contracts/sample/find-payable-orders.json",
    ];
    let before = snapshot_tree(&root);
    let preview = jails_cmd(&root, None)
        .args(["--pretend", "sql", "generate", "FindPayableOrders"])
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
        .args(["sql", "generate", "FindPayableOrders"])
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

    let jdbc = fs::read_to_string(root.join(paths[1])).unwrap();
    let source =
        fs::read_to_string(root.join("src/main/resources/db/queries/FindPayableOrders.sql"))
            .unwrap();
    let sql = source
        .lines()
        .skip_while(|line| line.starts_with("-- jails:"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    assert!(jdbc.contains(&sql), "generated SQL changed reader bytes");
    let contract = fs::read_to_string(root.join(paths[4])).unwrap();
    assert_eq!(contract.matches("\"evidence\":").count(), 9, "{contract}");

    let after_first_apply = paths.map(|path| fs::read(root.join(path)).unwrap());
    let reapplied = jails_cmd(&root, None)
        .args(["sql", "generate", "FindPayableOrders"])
        .output()
        .unwrap();
    assert!(
        reapplied.status.success(),
        "{}{}",
        String::from_utf8_lossy(&reapplied.stdout),
        String::from_utf8_lossy(&reapplied.stderr)
    );
    assert_eq!(
        paths.map(|path| fs::read(root.join(path)).unwrap()),
        after_first_apply
    );

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

#[test]
fn frozen_offline_check_detects_every_complete_input_drift_without_writing() {
    for drift in ["query", "migration-order", "dialect", "catalog", "mapping"] {
        let root = sql_fixture(&format!("sql-frozen-{drift}"));
        let generated = jails_cmd(&root, None)
            .args(["sql", "generate", "FindPayableOrders"])
            .output()
            .unwrap();
        assert!(
            generated.status.success(),
            "{drift}: {}{}",
            String::from_utf8_lossy(&generated.stdout),
            String::from_utf8_lossy(&generated.stderr)
        );

        match drift {
            "query" => {
                let path = root.join("src/main/resources/db/queries/FindPayableOrders.sql");
                let source = fs::read_to_string(&path).unwrap();
                fs::write(
                    &path,
                    source.replace("ORDER BY", "-- reader change\nORDER BY"),
                )
                .unwrap();
            }
            "migration-order" => fs::rename(
                root.join("src/main/resources/db/migration/V001__orders.sql"),
                root.join("src/main/resources/db/migration/V002__orders.sql"),
            )
            .unwrap(),
            "dialect" => {
                let path = root.join(".jails/app.toml");
                let manifest = fs::read_to_string(&path).unwrap();
                fs::write(&path, manifest.replace("postgresql", "mysql")).unwrap();
            }
            "catalog" => {
                let path = root.join("src/main/resources/db/migration/V001__orders.sql");
                let migration = fs::read_to_string(&path).unwrap();
                fs::write(
                    &path,
                    migration.replace(
                        "created_at timestamptz NOT NULL",
                        "created_at timestamptz NOT NULL, note text",
                    ),
                )
                .unwrap();
            }
            "mapping" => {
                let path = root.join(".jails/app.toml");
                let manifest = fs::read_to_string(&path).unwrap();
                fs::write(
                    &path,
                    manifest.replace("java.lang.String", "java.util.UUID"),
                )
                .unwrap();
            }
            _ => unreachable!(),
        }

        let before = snapshot_tree(&root);
        let checked = jails_cmd(&root, None)
            .env("DATABASE_URL", "postgresql://127.0.0.1:1/must-not-open")
            .args(["sql", "check", "--offline", "--frozen"])
            .output()
            .unwrap();
        assert!(
            !checked.status.success(),
            "{drift} drift unexpectedly passed: {}",
            String::from_utf8_lossy(&checked.stdout)
        );
        assert_eq!(snapshot_tree(&root), before, "{drift} drift wrote files");
    }
}
