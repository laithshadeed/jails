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

fn add_postgres_datasource(root: &Path, password: &str) {
    fs::write(
        root.join("compose.yaml"),
        format!(
            "services:\n  postgres:\n    # jails:db\n    image: postgres:17\n    environment:\n      POSTGRES_DB: app\n      POSTGRES_USER: app\n      POSTGRES_PASSWORD: {password}\n    ports:\n      - \"5432:5432\"\n"
        ),
    )
    .unwrap();
}

fn write_describing_psql(dir: &Path, log: &Path) {
    write_fake_maven(dir, &["psql"], log);
    fs::write(
        dir.join("psql"),
        format!(
            r#"#!/bin/sh
input=
while IFS= read -r line; do
  input="${{input}}${{line}}
"
done
printf '%s\n' "$*" >> '{}'
printf '%s' "$input" >> '{}'
case "$input" in
  *server_version_num*) printf '170004\n' ;;
  *gdesc*)
    printf 'id\tuuid\n'
    printf 'account_id\tuuid\n'
    printf 'total\tnumeric\n'
    printf 'status\torder_status\n'
    printf 'created_at\ttimestamp with time zone\n'
    ;;
  *) printf '1\n' ;;
esac
"#,
            log.display(),
            log.display()
        ),
    )
    .unwrap();
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
fn live_check_requires_and_describes_only_the_explicit_datasource() {
    let root = sql_fixture("sql-live");
    add_postgres_datasource(&root, "live-secret");
    let fake = temp_dir("sql-live-bin");
    let log = fake.join("psql.log");
    write_describing_psql(&fake, &log);
    let before = snapshot_tree(&root);

    let output = jails_cmd(&root, Some(&fake))
        .args([
            "--debug",
            "sql",
            "check",
            "--live",
            "--datasource",
            "postgres",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stdout.contains("verified-live (postgres 17)"), "{stdout}");
    assert!(stderr.contains("PGPASSWORD=<redacted>"), "{stderr}");
    assert!(!stderr.contains("live-secret"), "{stderr}");
    let invoked = read_log(&log);
    assert!(invoked.contains("BEGIN READ ONLY;"), "{invoked}");
    assert!(invoked.contains("NULL::public.order_status"), "{invoked}");
    assert!(invoked.contains("NULL::numeric"), "{invoked}");
    assert!(invoked.contains("NULL::int4"), "{invoked}");
    assert!(invoked.contains("\\gdesc"), "{invoked}");
    assert!(!invoked.contains(":status"), "{invoked}");
    assert_eq!(
        snapshot_tree(&root),
        before,
        "live check wrote project files"
    );
}

#[test]
fn live_check_without_a_datasource_refuses_before_starting_a_client() {
    let root = sql_fixture("sql-live-explicit");
    add_postgres_datasource(&root, "secret");
    let fake = temp_dir("sql-live-explicit-bin");
    let log = fake.join("psql.log");
    write_describing_psql(&fake, &log);
    let before = snapshot_tree(&root);
    let output = jails_cmd(&root, Some(&fake))
        .args(["sql", "check", "--live"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--datasource"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(read_log(&log).is_empty(), "psql ran without a datasource");
    assert_eq!(snapshot_tree(&root), before);
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
