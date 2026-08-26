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

fn hex(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn catalog_row(kind: &str, fields: [&str; 10]) -> String {
    format!(
        "{kind}\t{}",
        fields.map(hex).into_iter().collect::<Vec<_>>().join("\t")
    )
}

fn write_catalog_psql(dir: &Path, log: &Path) {
    let enum_labels = format!("{},{}", hex("due"), hex("paid"));
    let catalog = [
        catalog_row(
            "schema",
            ["public", "public", "", "", "", "", "", "", "", ""],
        ),
        catalog_row(
            "table",
            ["public", "orders", "", "", "", "", "", "", "", ""],
        ),
        catalog_row(
            "column",
            [
                "public",
                "id",
                "orders",
                "uuid",
                "false",
                "1",
                "",
                "",
                "",
                "identifier",
            ],
        ),
        catalog_row(
            "primary_key",
            [
                "public",
                "orders_pkey",
                "orders",
                "id",
                "",
                "",
                "",
                "",
                "",
                "",
            ],
        ),
        catalog_row(
            "foreign_key",
            [
                "public",
                "orders_account_fk",
                "orders",
                "FOREIGN KEY (account_id) REFERENCES accounts(id)",
                "public",
                "accounts",
                "",
                "",
                "",
                "",
            ],
        ),
        catalog_row(
            "unique",
            [
                "public",
                "orders_id_key",
                "orders",
                "UNIQUE (id)",
                "",
                "",
                "",
                "",
                "",
                "",
            ],
        ),
        catalog_row(
            "index",
            [
                "public",
                "orders_id_idx",
                "orders",
                "CREATE INDEX orders_id_idx ON public.orders USING btree (id)",
                "",
                "",
                "",
                "",
                "",
                "",
            ],
        ),
        catalog_row(
            "check",
            [
                "public",
                "orders_total_check",
                "orders",
                "CHECK ((total >= (0)::numeric))",
                "",
                "",
                "",
                "",
                "",
                "",
            ],
        ),
        catalog_row(
            "enum",
            [
                "public",
                "order_status",
                "",
                &enum_labels,
                "",
                "",
                "",
                "",
                "",
                "",
            ],
        ),
        catalog_row(
            "domain",
            [
                "public",
                "money",
                "",
                "numeric NOT NULL",
                "",
                "",
                "",
                "",
                "",
                "",
            ],
        ),
        catalog_row(
            "view",
            [
                "public",
                "payable_orders",
                "",
                " SELECT orders.id FROM orders;",
                "",
                "",
                "",
                "",
                "",
                "",
            ],
        ),
        catalog_row(
            "routine",
            [
                "public",
                "find_order_01234567",
                "",
                "CREATE FUNCTION public.find_order() RETURNS uuid LANGUAGE sql",
                "",
                "",
                "",
                "",
                "",
                "",
            ],
        ),
        catalog_row(
            "policy",
            [
                "public",
                "tenant_orders",
                "orders",
                "PERMISSIVE=true;COMMAND=r;ROLES=0;USING=true;CHECK=",
                "",
                "",
                "",
                "",
                "",
                "",
            ],
        ),
    ]
    .join("\n");
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
printf '%s' "$input" >> '{}'
case "$input" in
  *server_version_num*) printf '170004\n' ;;
  *"WITH observed"*) printf '%s\n' '{}' ;;
  *) printf '1\n' ;;
esac
"#,
            log.display(),
            catalog
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
fn introspect_and_pull_observe_every_catalog_kind_without_writing() {
    let root = sql_fixture("schema-observe");
    add_postgres_datasource(&root, "catalog-secret");
    let fake = temp_dir("schema-observe-bin");
    let log = fake.join("psql.log");
    write_catalog_psql(&fake, &log);
    let before = snapshot_tree(&root);

    let introspected = jails_cmd(&root, Some(&fake))
        .args([
            "--debug",
            "introspect",
            "db",
            "--datasource",
            "postgres",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        introspected.status.success(),
        "{}{}",
        String::from_utf8_lossy(&introspected.stdout),
        String::from_utf8_lossy(&introspected.stderr)
    );
    let json = String::from_utf8_lossy(&introspected.stdout);
    for kind in [
        "schema",
        "table",
        "column",
        "primary-key",
        "foreign-key",
        "unique",
        "index",
        "check",
        "enum",
        "domain",
        "view",
        "routine",
        "policy",
    ] {
        assert!(
            json.contains(&format!("\"kind\":\"{kind}\"")),
            "{kind}: {json}"
        );
    }
    let diagnostics = String::from_utf8_lossy(&introspected.stderr);
    assert!(
        diagnostics.contains("PGPASSWORD=<redacted>"),
        "{diagnostics}"
    );
    assert!(!diagnostics.contains("catalog-secret"), "{diagnostics}");
    assert!(!json.contains("catalog-secret"), "{json}");
    assert_eq!(snapshot_tree(&root), before, "introspection wrote files");

    let first = jails_cmd(&root, Some(&fake))
        .args([
            "pull",
            "--datasource",
            "postgres",
            "--into-slice",
            "Billing",
        ])
        .output()
        .unwrap();
    let second = jails_cmd(&root, Some(&fake))
        .args([
            "pull",
            "--datasource",
            "postgres",
            "--into-slice",
            "Billing",
        ])
        .output()
        .unwrap();
    assert!(first.status.success() && second.status.success());
    assert_eq!(
        first.stdout, second.stdout,
        "no-change pull was not byte-idempotent"
    );
    let pulled = String::from_utf8_lossy(&first.stdout);
    assert!(pulled.contains("jails.schema-import.v1"), "{pulled}");
    assert!(pulled.contains("public.orders"), "{pulled}");
    assert_eq!(snapshot_tree(&root), before, "pull wrote files");
}

#[test]
fn schema_diff_and_migration_lint_report_typed_risks_read_only() {
    let root = sql_fixture("schema-diff");
    add_postgres_datasource(&root, "secret");
    let fake = temp_dir("schema-diff-bin");
    let log = fake.join("psql.log");
    write_catalog_psql(&fake, &log);
    let before = snapshot_tree(&root);
    let diff = jails_cmd(&root, Some(&fake))
        .args([
            "schema",
            "diff",
            "--from",
            "migrations",
            "--to",
            "live",
            "--datasource",
            "postgres",
        ])
        .output()
        .unwrap();
    assert!(
        diff.status.success(),
        "{}{}",
        String::from_utf8_lossy(&diff.stdout),
        String::from_utf8_lossy(&diff.stderr)
    );
    let report = String::from_utf8_lossy(&diff.stdout);
    assert!(
        report.contains("schema diff Migrations -> Live"),
        "{report}"
    );
    assert!(
        report.contains("additive") || report.contains("data-dependent"),
        "{report}"
    );
    assert_eq!(snapshot_tree(&root), before, "schema diff wrote files");

    fs::write(
        root.join("src/main/resources/db/migration/V002__drop_note.sql"),
        "ALTER TABLE orders DROP COLUMN note;",
    )
    .unwrap();
    let lint_before = snapshot_tree(&root);
    let linted = jails_cmd(&root, None)
        .args(["migrate", "lint"])
        .output()
        .unwrap();
    assert!(linted.status.success());
    let lint = String::from_utf8_lossy(&linted.stdout);
    assert!(lint.contains("destructive"), "{lint}");
    assert!(lint.contains("V002__drop_note.sql"), "{lint}");
    assert_eq!(
        snapshot_tree(&root),
        lint_before,
        "migration lint wrote files"
    );
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
