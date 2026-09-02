use super::*;

/// Retire the stored entity and its table, as a canonical project does.
///
/// **No `--migrate`, no `--datasource`, and that is the change.** Canonical
/// retirement is model subtraction plus one appended forward migration: the
/// plan says what the schema becomes and `jails migrate` is what runs it
/// against a database. There is no post-commit effect to fail, and so no
/// effect ledger to retry from -- which is what the two tests below used to
/// be about, and what the strangler removed rather than reimplemented.
fn drop_the_table(command: &mut std::process::Command) -> &mut std::process::Command {
    command.args([
        "destroy",
        "scaffold",
        "Task",
        "--storage",
        "drop",
        "--confirm-table",
        "tasks",
        "--force",
    ])
}

fn migrations(root: &Path) -> Vec<String> {
    let mut found = fs::read_dir(root.join("src/main/resources/db/migration"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".sql"))
        .collect::<Vec<_>>();
    found.sort();
    found
}

/// Retiring a table twice appends one migration, not two.
///
/// **Schema history is append-only, so a second identical retirement has to
/// be a no-op rather than a second `drop table`.** Flyway would run both, and
/// the second fails against a table that is already gone -- which turns a
/// re-run of the same command, the thing convergence is supposed to make
/// safe, into a broken database.
#[test]
fn rerunning_the_same_retirement_appends_one_forward_migration() {
    let root = temp_dir("task-drop-idempotent");
    write_spring_fixture(&root);
    common::declare_storage(&root);
    assert!(
        jails_cmd(&root, None)
            .args(["g", "scaffold", "Task", "id:uuid@pk", "title:string!"])
            .status()
            .unwrap()
            .success()
    );
    let created = migrations(&root);
    assert_eq!(created.len(), 1, "{created:?}");

    let first = drop_the_table(&mut jails_cmd(&root, None))
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr)
    );
    let after_first = migrations(&root);
    assert_eq!(after_first.len(), 2, "{after_first:?}");
    assert!(
        after_first.iter().any(|name| name.contains("drop_tasks")),
        "{after_first:?}"
    );

    // The declaration is gone, so the second run has nothing to retire and
    // says so rather than appending a `drop table` for a table nothing
    // declares any more.
    let second = drop_the_table(&mut jails_cmd(&root, None))
        .output()
        .unwrap();
    let told = format!(
        "{}{}",
        String::from_utf8_lossy(&second.stdout),
        String::from_utf8_lossy(&second.stderr)
    );
    assert_eq!(migrations(&root), after_first, "{told}");
}

struct PostgresContainer(String);

impl Drop for PostgresContainer {
    fn drop(&mut self) {
        let _ = std::process::Command::new("docker")
            .args(["kill", &self.0])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

fn executable(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

#[test]
#[cfg(unix)]
fn postgres_17_accepts_the_retirement_jails_wrote() {
    if !real_docker_available() {
        skip("a running Docker-compatible container runtime is required");
        return;
    }
    let Some(psql) = executable("psql") else {
        skip("psql not found on PATH");
        return;
    };

    // Through the shared reservation helper rather than a third copy of
    // `bind(0)`, `read`, `close`: the copies are how the two-ports-one-number
    // bug in `AppSuiteServices` went unnoticed. See its docs.
    let port = common::reserve_loopback_ports(1)[0];
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let name = format!("jails-lifecycle-postgres-{}-{nonce}", std::process::id());
    let started = std::process::Command::new("docker")
        .args([
            "run",
            "-d",
            "--rm",
            "--name",
            &name,
            "-p",
            &format!("127.0.0.1:{port}:5432"),
            "-e",
            "POSTGRES_DB=app",
            "-e",
            "POSTGRES_USER=app",
            "-e",
            "POSTGRES_PASSWORD=app",
            "postgres:17-alpine",
        ])
        .output()
        .unwrap();
    assert!(
        started.status.success(),
        "could not start PostgreSQL: {}{}",
        String::from_utf8_lossy(&started.stdout),
        String::from_utf8_lossy(&started.stderr)
    );
    let _container = PostgresContainer(name.clone());
    let port_string = port.to_string();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !std::process::Command::new(&psql)
        .env("PGPASSWORD", "app")
        .args([
            "-h",
            "127.0.0.1",
            "-p",
            &port_string,
            "-U",
            "app",
            "-d",
            "app",
            "-c",
            "SELECT 1",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
    {
        assert!(
            std::time::Instant::now() < deadline,
            "PostgreSQL 17 did not become ready"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    let root = temp_dir("task-drop-postgres-17");
    write_spring_fixture(&root);
    common::declare_storage(&root);
    assert!(
        jails_cmd(&root, None)
            .args(["g", "scaffold", "Task", "id:uuid@pk", "title:string!"])
            .status()
            .unwrap()
            .success()
    );
    let v1 = root.join("src/main/resources/db/migration/V001__create_tasks.sql");
    let sql = |args: &[&str]| {
        std::process::Command::new(&psql)
            .env("PGPASSWORD", "app")
            .args([
                "-h",
                "127.0.0.1",
                "-p",
                &port_string,
                "-U",
                "app",
                "-d",
                "app",
            ])
            .args(args)
            .output()
            .unwrap()
    };
    let applied_v1 = sql(&["-v", "ON_ERROR_STOP=1", "-f", v1.to_str().unwrap()]);
    assert!(applied_v1.status.success(), "{applied_v1:?}");
    let checksum_v1 = jails_drive::live_sql::flyway_checksum(&fs::read(&v1).unwrap()).unwrap();
    let history = format!(
        "CREATE TABLE flyway_schema_history (installed_rank integer PRIMARY KEY, version varchar(50), description varchar(200) NOT NULL, type varchar(20) NOT NULL, script varchar(1000) NOT NULL, checksum integer, installed_by varchar(100) NOT NULL, installed_on timestamp NOT NULL DEFAULT now(), execution_time integer NOT NULL, success boolean NOT NULL); INSERT INTO flyway_schema_history VALUES (1, '1', 'create tasks', 'SQL', 'V001__create_tasks.sql', {checksum_v1}, 'app', now(), 0, true);"
    );
    let created_history = sql(&["-v", "ON_ERROR_STOP=1", "-c", &history]);
    assert!(created_history.status.success(), "{created_history:?}");

    let tools = temp_dir("task-drop-postgres-17-bin");
    std::os::unix::fs::symlink(&psql, tools.join("psql")).unwrap();

    // **The retirement writes SQL; running it is a separate act.** Canonical
    // removal is model subtraction plus one appended forward migration -- no
    // datasource, no post-commit effect, nothing that can half-succeed
    // against a live database while the plan says it committed. What this
    // test still answers is the only question a real PostgreSQL can: that the
    // statement jails wrote is one PostgreSQL 17 actually accepts.
    let dropped = drop_the_table(&mut jails_cmd(&root, Some(&tools)))
        .output()
        .unwrap();
    assert!(
        dropped.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&dropped.stdout),
        String::from_utf8_lossy(&dropped.stderr)
    );
    let present_before = sql(&[
        "-At",
        "-c",
        "SELECT pg_catalog.to_regclass('public.tasks') IS NOT NULL;",
    ]);
    assert_eq!(
        String::from_utf8_lossy(&present_before.stdout).trim(),
        "t",
        "the retirement touched the database"
    );

    let v2 = root.join("src/main/resources/db/migration/V002__drop_tasks.sql");
    let applied_v2 = sql(&["-v", "ON_ERROR_STOP=1", "-f", v2.to_str().unwrap()]);
    assert!(
        applied_v2.status.success(),
        "PostgreSQL 17 refused the retirement jails wrote: {}",
        String::from_utf8_lossy(&applied_v2.stderr)
    );
    let absent = sql(&[
        "-At",
        "-c",
        "SELECT pg_catalog.to_regclass('public.tasks') IS NULL;",
    ]);
    assert_eq!(String::from_utf8_lossy(&absent.stdout).trim(), "t");

    let checksum_v2 = jails_drive::live_sql::flyway_checksum(&fs::read(&v2).unwrap()).unwrap();
    let recorded_v2 = sql(&[
        "-v",
        "ON_ERROR_STOP=1",
        "-c",
        &format!(
            "INSERT INTO flyway_schema_history VALUES (2, '2', 'drop tasks', 'SQL', 'V002__drop_tasks.sql', {checksum_v2}, 'app', now(), 0, true);"
        ),
    ]);
    assert!(recorded_v2.status.success(), "{recorded_v2:?}");

    // The model's own view, with no datasource: the entity is retired and the
    // migration that retires it is in history. Comparing that against a live
    // catalog is `resource status --datasource`, which the canonical path
    // does not answer yet -- `plan.md` tracks it, and until then this asserts
    // the half jails is the authority on.
    let status = jails_cmd(&root, Some(&tools))
        .args(["resource", "status", "Task", "--output", "json"])
        .output()
        .unwrap();
    assert!(status.status.success(), "{status:?}");
    let json: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(json["state"], "retired", "{json}");
    let rendered = format!(
        "{}{}",
        String::from_utf8_lossy(&dropped.stdout),
        String::from_utf8_lossy(&dropped.stderr)
    );
    assert!(
        !rendered.contains("app:app"),
        "credential leaked: {rendered}"
    );
}
