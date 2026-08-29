use super::*;

fn drop_with_migration(command: &mut std::process::Command) -> &mut std::process::Command {
    command.args([
        "destroy",
        "scaffold",
        "Task",
        "--storage",
        "drop",
        "--confirm-table",
        "tasks",
        "--force",
        "--migrate",
        "--datasource",
        "DATABASE_URL",
    ])
}

#[test]
fn rerunning_the_same_destroy_retries_only_its_failed_migration_effect() {
    let root = temp_dir("task-drop-effect-retry");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();
    assert!(
        jails_cmd(&root, None)
            .args(["g", "scaffold", "Task", "id:uuid@pk", "title:string!"])
            .status()
            .unwrap()
            .success()
    );

    let fake = temp_dir("task-drop-effect-retry-bin");
    let log = fake.join("flyway.log");
    write_fake_maven(&fake, &["flyway"], &log);
    fs::write(fake.join("flyway"), "#!/bin/sh\nexit 9\n").unwrap();
    let database = "postgresql://app:secret@127.0.0.1:5432/demo";
    let first = drop_with_migration(jails_cmd(&root, Some(&fake)).env("DATABASE_URL", database))
        .output()
        .unwrap();
    assert!(!first.status.success(), "{first:?}");

    let before = jails_commit::store::Store::at(&root)
        .read_receipts()
        .unwrap();
    let original = before.first().unwrap();
    assert!(matches!(
        original.post_commit[0].state,
        jails_protocol::effect::EffectState::Failed { attempt: 1, .. }
    ));
    let transaction = original.transaction;
    let generation = original.generation;

    write_fake_maven(&fake, &["flyway"], &log);
    let preview = drop_with_migration(jails_cmd(&root, Some(&fake)).env("DATABASE_URL", database))
        .args(["--pretend", "--output", "json"])
        .output()
        .unwrap();
    assert!(preview.status.success(), "{preview:?}");
    assert!(read_log(&log).is_empty(), "preview invoked Flyway");
    let json: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert_eq!(json["status"], "preview", "{json}");
    assert_eq!(json["report"]["kind"], "effect-retry", "{json}");
    assert_eq!(
        json["report"]["data"]["transaction"],
        transaction.to_hex(),
        "{json}"
    );

    let retried = drop_with_migration(jails_cmd(&root, Some(&fake)).env("DATABASE_URL", database))
        .args(["--output", "json"])
        .output()
        .unwrap();
    assert!(
        retried.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&retried.stdout),
        String::from_utf8_lossy(&retried.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&retried.stdout).unwrap();
    assert_eq!(json["status"], "effect-retried", "{json}");
    let invoked = read_log(&log);
    assert!(invoked.contains("flyway migrate"), "{invoked}");
    assert!(!invoked.contains("secret"), "credential leaked: {invoked}");

    let after = jails_commit::store::Store::at(&root)
        .read_receipts()
        .unwrap();
    assert_eq!(
        after.len(),
        before.len(),
        "retry created another transaction"
    );
    assert_eq!(after[0].transaction, transaction);
    assert_eq!(after[0].generation, generation);
    assert_eq!(
        after[0].post_commit[0].state,
        jails_protocol::effect::EffectState::Succeeded
    );
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
fn postgres_17_observes_the_explicit_drop_effect_as_applied() {
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
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();
    assert!(
        jails_cmd(&root, None)
            .args(["g", "scaffold", "Task", "id:uuid@pk", "title:string!"])
            .status()
            .unwrap()
            .success()
    );
    let v1 = root.join("src/main/resources/db/migration/V001__create_tasks.sql");
    let database = format!("postgresql://app:app@127.0.0.1:{port}/app");
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
    let log = tools.join("flyway.log");
    write_fake_maven(&tools, &["flyway"], &log);
    fs::write(
        tools.join("flyway"),
        format!(
            "#!/bin/sh\nset -eu\necho \"$0 $*\" >> \"{}\"\nlocation=${{FLYWAY_LOCATIONS#filesystem:}}\nPGPASSWORD=app psql -h 127.0.0.1 -p {port} -U app -d app -v ON_ERROR_STOP=1 -f \"$location/V002__drop_tasks.sql\"\n",
            log.display()
        ),
    )
    .unwrap();

    let dropped =
        drop_with_migration(jails_cmd(&root, Some(&tools)).env("DATABASE_URL", &database))
            .output()
            .unwrap();
    assert!(
        dropped.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&dropped.stdout),
        String::from_utf8_lossy(&dropped.stderr)
    );
    let absent = sql(&[
        "-At",
        "-c",
        "SELECT pg_catalog.to_regclass('public.tasks') IS NULL;",
    ]);
    assert_eq!(String::from_utf8_lossy(&absent.stdout).trim(), "t");

    let v2 = root.join("src/main/resources/db/migration/V002__drop_tasks.sql");
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

    let status = jails_cmd(&root, Some(&tools))
        .env("DATABASE_URL", &database)
        .args([
            "resource",
            "status",
            "Task",
            "--datasource",
            "DATABASE_URL",
            "--output",
            "json",
        ])
        .output()
        .unwrap();
    assert!(status.status.success(), "{status:?}");
    let json: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(json["state"], "drop-observed-applied", "{json}");
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
