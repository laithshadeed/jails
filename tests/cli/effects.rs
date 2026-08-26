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
