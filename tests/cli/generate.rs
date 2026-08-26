//! `jails generate` and `jails destroy`: the per-kind artifacts.

use super::*;

#[test]
fn generate_standalone_and_destroy_roundtrip() {
    let root = temp_dir("standalone-roundtrip");
    write_project_skeleton(&root);

    let status = jails_cmd(&root, None)
        .args(["generate", "controller", "comment"])
        .status()
        .unwrap();
    assert!(status.success());
    let file = root.join("src/main/java/com/example/demo/web/CommentController.java");
    assert!(file.is_file());
    let contents = fs::read_to_string(&file).unwrap();
    assert!(contents.contains("class CommentController"));
    assert!(
        !contents.contains("public class"),
        "spring.md §2: a controller is an entry point, not module API"
    );
    // Rails generates a test alongside `generate controller`; jails matches that.
    let test_file = root.join("src/test/java/com/example/demo/web/CommentControllerTest.java");
    assert!(test_file.is_file(), "expected {}", test_file.display());

    let status = jails_cmd(&root, None)
        .args(["destroy", "controller", "comment", "--force"])
        .status()
        .unwrap();
    assert!(status.success());
    assert!(!file.is_file());
    assert!(!test_file.is_file());
}

#[test]
fn generate_scaffold_writes_a_raw_jdbc_slice() {
    let root = temp_dir("scaffold-files");
    write_spring_fixture(&root);

    let status = jails_cmd(&root, None)
        .args(["generate", "scaffold", "Post", "title:string", "body:text"])
        .status()
        .unwrap();
    assert!(status.success());

    let pkg = root.join("src/main/java/com/example/demo");
    assert!(pkg.join("domain/Post.java").is_file());
    assert!(pkg.join("app/PostRepository.java").is_file());
    assert!(pkg.join("adapters/JdbcPostRepository.java").is_file());
    assert!(pkg.join("service/PostService.java").is_file());
    assert!(pkg.join("web/PostController.java").is_file());
    assert!(
        root.join("src/test/java/com/example/demo/adapters/JdbcPostRepositoryIT.java")
            .is_file()
    );
    assert!(
        root.join("src/test/java/com/example/demo/web/PostControllerTest.java")
            .is_file()
    );
}

#[test]
fn plain_project_scaffold_refuses_without_writing_uncompilable_spring_java() {
    let root = temp_dir("scaffold-plain-refusal");
    write_plain_fixture(&root);
    let before = snapshot_tree(&root);

    let output = jails_cmd(&root, None)
        .args([
            "generate",
            "scaffold",
            "Note",
            "id:uuid@pk",
            "title:string!",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("`scaffold` is a Spring Boot capability"),
        "{stderr}"
    );
    assert_eq!(
        snapshot_tree(&root),
        before,
        "a refused scaffold wrote project or machine state"
    );
}

#[test]
fn prepared_diff_and_ast_show_create_replace_and_three_way_without_writing() {
    let root = temp_dir("prepared-review");
    write_spring_fixture(&root);
    let generated = jails_cmd(&root, None)
        .args(["g", "record", "Note", "title:string!"])
        .output()
        .unwrap();
    assert!(generated.status.success(), "{generated:?}");

    let test = root.join("src/test/java/com/example/demo/domain/NoteTest.java");
    let reader_edit = format!(
        "// reader-owned context\n{}",
        fs::read_to_string(&test).unwrap()
    );
    fs::write(&test, reader_edit).unwrap();
    let before = snapshot_tree(&root);

    let changed = jails_cmd(&root, None)
        .args([
            "g",
            "field",
            "Note",
            "createdAt:instant",
            "--default-literal",
            "2026-08-25T12:00:00Z",
            "--pretend",
            "--diff",
            "--ast",
        ])
        .output()
        .unwrap();
    assert!(changed.status.success(), "{changed:?}");
    let shown = String::from_utf8_lossy(&changed.stdout);
    assert!(
        shown.contains("diff --jails replace src/main/java/com/example/demo/domain/Note.java"),
        "{shown}"
    );
    assert!(shown.contains("+import java.time.Instant;"), "{shown}");
    assert!(
        shown.contains("@@ -") && shown.contains(" three-way\n"),
        "{shown}"
    );
    assert!(
        shown.contains("MergeFile { path: src/test/java/com/example/demo/domain/NoteTest.java }"),
        "{shown}"
    );
    assert!(
        shown.contains("ReplaceFile { path: src/main/java/com/example/demo/domain/Note.java }"),
        "{shown}"
    );
    assert_eq!(snapshot_tree(&root), before, "review preview wrote files");

    let created = jails_cmd(&root, None)
        .args([
            "g",
            "record",
            "Fresh",
            "id:uuid",
            "--pretend",
            "--diff",
            "--ast",
        ])
        .output()
        .unwrap();
    assert!(created.status.success(), "{created:?}");
    let shown = String::from_utf8_lossy(&created.stdout);
    assert!(shown.contains("--- /dev/null\n+++ b/"), "{shown}");
    assert!(shown.contains("CreateFile { path:"), "{shown}");
    assert!(
        !shown.contains("  timing  "),
        "ordinary human output exposed debug timings: {shown}"
    );
    assert_eq!(snapshot_tree(&root), before, "create preview wrote files");

    let debug = jails_cmd(&root, None)
        .args(["g", "record", "Fresh", "id:uuid", "--pretend", "--debug"])
        .output()
        .unwrap();
    assert!(debug.status.success(), "{debug:?}");
    let debug = String::from_utf8(debug.stdout).unwrap();
    for phase in [
        "discover", "observe", "parse", "project", "prepare", "verify",
    ] {
        assert!(
            debug.contains(&format!("timing  {phase}")),
            "missing {phase} timing in {debug}"
        );
    }
    assert!(!debug.contains("timing  commit"), "{debug}");
    assert_eq!(snapshot_tree(&root), before, "debug preview wrote files");

    let json = jails_cmd(&root, None)
        .args([
            "g",
            "record",
            "Fresh",
            "id:uuid",
            "--pretend",
            "--diff",
            "--ast",
            "--output",
            "json",
        ])
        .output()
        .unwrap();
    assert!(json.status.success(), "{json:?}");
    let json = String::from_utf8(json.stdout).unwrap();
    assert!(json.starts_with('{') && json.ends_with('}'), "{json}");
    assert!(json.contains("\"diffs\": ["), "{json}");
    assert!(json.contains("\"ast\": ["), "{json}");
    assert!(json.contains("\"timings\": ["), "{json}");
    for phase in [
        "discover", "observe", "parse", "project", "prepare", "verify",
    ] {
        assert!(
            json.contains(&format!("\"phase\": \"{phase}\"")),
            "missing {phase} timing in {json}"
        );
    }
    assert!(!json.contains("\"phase\": \"commit\""), "{json}");
    assert!(json.contains("\"patch\": \"diff --jails create"), "{json}");
    assert!(
        json.contains("src/main/java/com/example/demo/domain/Fresh.java"),
        "{json}"
    );
    assert!(!json.contains("nothing was written"), "{json}");
    assert_eq!(
        snapshot_tree(&root),
        before,
        "JSON review preview wrote files"
    );

    let merged_json = jails_cmd(&root, None)
        .args([
            "g",
            "field",
            "Note",
            "createdAt:instant",
            "--default-literal",
            "2026-08-25T12:00:00Z",
            "--pretend",
            "--output",
            "json",
        ])
        .output()
        .unwrap();
    assert!(merged_json.status.success(), "{merged_json:?}");
    let merged_json = String::from_utf8(merged_json.stdout).unwrap();
    assert!(
        merged_json.contains("\"phase\": \"process\""),
        "three-way merge process was not timed: {merged_json}"
    );
    assert_eq!(
        snapshot_tree(&root),
        before,
        "timed merge preview wrote files"
    );

    let applied = jails_cmd(&root, None)
        .args(["g", "record", "Fresh", "id:uuid", "--diff", "--ast"])
        .output()
        .unwrap();
    assert!(applied.status.success(), "{applied:?}");
    let applied = String::from_utf8_lossy(&applied.stdout);
    assert!(
        applied.contains("diff --jails create src/main/java/com/example/demo/domain/Fresh.java"),
        "{applied}"
    );
    assert!(applied.contains("CreateFile { path:"), "{applied}");
    assert!(
        root.join("src/main/java/com/example/demo/domain/Fresh.java")
            .is_file(),
        "an applied reviewed transition did not commit"
    );

    let committed = jails_cmd(&root, None)
        .args(["g", "record", "Timed", "id:uuid", "--output", "json"])
        .output()
        .unwrap();
    assert!(committed.status.success(), "{committed:?}");
    let committed = String::from_utf8(committed.stdout).unwrap();
    assert!(
        committed.contains("\"phase\": \"commit\""),
        "committed JSON omitted commit timing: {committed}"
    );
}

#[test]
fn task_scaffold_cannot_rewrite_or_delete_its_published_v001() {
    let root = temp_dir("task-migration-seal");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();

    let generated = jails_cmd(&root, None)
        .args(["g", "scaffold", "Task", "id:uuid@pk", "title:string!"])
        .output()
        .unwrap();
    assert!(generated.status.success(), "{generated:?}");
    let migration = root.join("src/main/resources/db/migration/V001__create_tasks.sql");
    let sealed = fs::read_to_string(&migration).unwrap();
    let before_resync = snapshot_tree(&root);
    let resync = jails_cmd(&root, None)
        .args([
            "g",
            "scaffold",
            "Task",
            "id:uuid@pk",
            "title:string!",
            "completed:boolean",
        ])
        .output()
        .unwrap();
    assert!(!resync.status.success(), "{resync:?}");
    assert!(
        String::from_utf8_lossy(&resync.stderr).contains("migration-edited-after-seal"),
        "{}",
        String::from_utf8_lossy(&resync.stderr)
    );
    assert_eq!(snapshot_tree(&root), before_resync, "refusal wrote files");

    let before_retirement = snapshot_tree(&root);
    let missing_policy = jails_cmd(&root, None)
        .args(["destroy", "scaffold", "Task", "--force"])
        .output()
        .unwrap();
    assert!(!missing_policy.status.success(), "{missing_policy:?}");
    let missing_policy_stderr = String::from_utf8_lossy(&missing_policy.stderr);
    assert!(
        missing_policy_stderr.contains("storage-policy-required"),
        "{missing_policy_stderr}"
    );
    assert!(
        missing_policy_stderr.contains("--storage preserve"),
        "{missing_policy_stderr}"
    );
    assert!(
        missing_policy_stderr.contains("--storage drop --confirm-table tasks"),
        "{missing_policy_stderr}"
    );
    assert_eq!(
        snapshot_tree(&root),
        before_retirement,
        "refusal wrote files"
    );

    let destroyed = jails_cmd(&root, None)
        .args([
            "destroy",
            "scaffold",
            "Task",
            "--storage",
            "preserve",
            "--force",
        ])
        .output()
        .unwrap();
    assert!(destroyed.status.success(), "{destroyed:?}");
    assert_eq!(fs::read_to_string(&migration).unwrap(), sealed);
    assert!(migration.is_file());
    assert!(
        !root
            .join("src/main/java/com/example/demo/web/TaskController.java")
            .exists()
    );
    let store = jails_commit::store::Store::at(&root)
        .observe()
        .unwrap()
        .ledger
        .unwrap();
    assert!(store.applied.is_empty(), "{:?}", store.applied);
    assert_eq!(store.lifecycles.len(), 1, "{:?}", store.lifecycles);
    let lifecycle = &store.lifecycles[0];
    assert!(matches!(
        lifecycle.state,
        jails_protocol::lifecycle::ResourceState::RetiredPreservingStorage { .. }
    ));
    assert_eq!(lifecycle.table.as_ref().unwrap().table.as_str(), "tasks");
    assert_eq!(lifecycle.migrations.len(), 1);
    assert_eq!(lifecycle.migrations[0].version.get(), 1);
    let entity_before_revive = lifecycle.entity.clone();
    let seal_before_revive = lifecycle.migrations[0].clone();

    let status = jails_cmd(&root, None)
        .args(["resource", "status", "Task", "--output", "json"])
        .output()
        .unwrap();
    assert!(status.status.success(), "{status:?}");
    let status_json = String::from_utf8(status.stdout).unwrap();
    assert!(
        status_json.contains("\"state\":\"retired-storage-present\""),
        "{status_json}"
    );
    assert!(
        status_json.contains("jails resource revive Task --table tasks"),
        "{status_json}"
    );

    let before_wrong_table = snapshot_tree(&root);
    let wrong_table = jails_cmd(&root, None)
        .args(["resource", "revive", "Task", "--table", "task"])
        .output()
        .unwrap();
    assert!(!wrong_table.status.success(), "{wrong_table:?}");
    assert!(
        String::from_utf8_lossy(&wrong_table.stderr).contains("pass `--table tasks` exactly"),
        "{}",
        String::from_utf8_lossy(&wrong_table.stderr)
    );
    assert_eq!(
        snapshot_tree(&root),
        before_wrong_table,
        "refusal wrote files"
    );

    let revived = jails_cmd(&root, None)
        .args(["resource", "revive", "Task", "--table", "tasks"])
        .output()
        .unwrap();
    assert!(revived.status.success(), "{revived:?}");
    assert_eq!(fs::read_to_string(&migration).unwrap(), sealed);
    assert!(
        !root
            .join("src/main/resources/db/migration/V002__create_tasks.sql")
            .exists(),
        "revive published a second create migration"
    );
    assert!(
        root.join("src/main/java/com/example/demo/web/TaskController.java")
            .is_file()
    );
    let revived_store = jails_commit::store::Store::at(&root)
        .observe()
        .unwrap()
        .ledger
        .unwrap();
    let revived_lifecycle = &revived_store.lifecycles[0];
    assert!(matches!(
        revived_lifecycle.state,
        jails_protocol::lifecycle::ResourceState::Active
    ));
    assert_eq!(revived_lifecycle.entity, entity_before_revive);
    assert_eq!(revived_lifecycle.migrations, vec![seal_before_revive]);
    assert_eq!(revived_store.applied.len(), 1);

    let active_status = jails_cmd(&root, None)
        .args(["resource", "status", "Task", "--output", "json"])
        .output()
        .unwrap();
    assert!(active_status.status.success(), "{active_status:?}");
    assert!(
        String::from_utf8_lossy(&active_status.stdout).contains("\"state\":\"consistent\""),
        "{}",
        String::from_utf8_lossy(&active_status.stdout)
    );
}

#[test]
fn coordinated_preserve_table_rename_keeps_storage_and_moves_lifecycle_lineage() {
    let root = temp_dir("resource-rename-preserve-table");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();

    let generated = jails_cmd(&root, None)
        .args(["g", "scaffold", "Task", "id:uuid@pk", "title:string!"])
        .output()
        .unwrap();
    assert!(generated.status.success(), "{generated:?}");
    let migration = root.join("src/main/resources/db/migration/V001__create_tasks.sql");
    let sealed = fs::read(&migration).unwrap();

    let renamed = jails_cmd(&root, None)
        .args([
            "rename",
            "resource",
            "Billing.Task",
            "WorkItem",
            "--strategy",
            "preserve-table",
            "--force",
        ])
        .output()
        .unwrap();
    assert!(renamed.status.success(), "{renamed:?}");
    let stdout = String::from_utf8_lossy(&renamed.stdout);
    assert!(
        stdout.contains("physical-table-preserved: tasks"),
        "{stdout}"
    );
    assert_eq!(fs::read(&migration).unwrap(), sealed);
    assert!(
        !root
            .join("src/main/resources/db/migration/V002__rename_tasks.sql")
            .exists()
    );
    assert!(
        root.join("src/main/java/com/example/demo/domain/WorkItem.java")
            .is_file()
    );
    assert!(
        !root
            .join("src/main/java/com/example/demo/domain/Task.java")
            .exists()
    );
    let controller =
        fs::read_to_string(root.join("src/main/java/com/example/demo/web/WorkItemController.java"))
            .unwrap();
    assert!(controller.contains("/tasks"), "{controller}");

    let store = jails_commit::store::Store::at(&root)
        .observe()
        .unwrap()
        .ledger
        .unwrap();
    let [lifecycle] = store.lifecycles.as_slice() else {
        panic!("expected one lifecycle: {:?}", store.lifecycles);
    };
    assert_eq!(lifecycle.expected_path.name().as_str(), "WorkItem");
    assert_eq!(lifecycle.table.as_ref().unwrap().table.as_str(), "tasks");
    assert_eq!(lifecycle.migrations.len(), 1);
    let jails_protocol::entity::EntityId::Intent(id) = &lifecycle.entity else {
        panic!("expected direct intent identity: {:?}", lifecycle.entity);
    };
    assert_eq!(id.name.as_str(), "WorkItem");
}

#[test]
fn coordinated_single_cutover_appends_one_migration_and_switches_the_binding() {
    let root = temp_dir("resource-rename-single-cutover");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();

    let generated = jails_cmd(&root, None)
        .args(["g", "scaffold", "Task", "id:uuid@pk", "title:string!"])
        .output()
        .unwrap();
    assert!(generated.status.success(), "{generated:?}");
    let first = root.join("src/main/resources/db/migration/V001__create_tasks.sql");
    let sealed = fs::read(&first).unwrap();

    let renamed = jails_cmd(&root, None)
        .args([
            "rename",
            "resource",
            "Billing.Task",
            "WorkItem",
            "--strategy",
            "single-cutover",
            "--force",
        ])
        .output()
        .unwrap();
    assert!(renamed.status.success(), "{renamed:?}");
    let stdout = String::from_utf8_lossy(&renamed.stdout);
    assert!(
        stdout.contains("physical-table-cutover: tasks -> work_items"),
        "{stdout}"
    );
    assert_eq!(fs::read(first).unwrap(), sealed);
    let cutover = root.join("src/main/resources/db/migration/V002__rename_tasks_to_work_items.sql");
    assert_eq!(
        fs::read_to_string(&cutover).unwrap(),
        "alter table public.\"tasks\" rename to \"work_items\";\n"
    );
    let adapter = fs::read_to_string(
        root.join("src/main/java/com/example/demo/adapters/JdbcWorkItemRepository.java"),
    )
    .unwrap();
    assert!(adapter.contains("work_items"), "{adapter}");
    assert!(!adapter.contains("from tasks"), "{adapter}");
    assert!(!adapter.contains("into tasks"), "{adapter}");
    let controller =
        fs::read_to_string(root.join("src/main/java/com/example/demo/web/WorkItemController.java"))
            .unwrap();
    assert!(controller.contains("/tasks"), "{controller}");

    let store = jails_commit::store::Store::at(&root)
        .observe()
        .unwrap()
        .ledger
        .unwrap();
    let [lifecycle] = store.lifecycles.as_slice() else {
        panic!("expected one lifecycle: {:?}", store.lifecycles);
    };
    assert_eq!(lifecycle.expected_path.name().as_str(), "WorkItem");
    assert_eq!(
        lifecycle.table.as_ref().unwrap().table.as_str(),
        "work_items"
    );
    assert_eq!(
        lifecycle
            .migrations
            .iter()
            .map(|seal| seal.version.get())
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
}

#[test]
fn single_cutover_reports_reader_owned_sql_without_writing() {
    let root = temp_dir("resource-rename-manual-sql");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();
    let generated = jails_cmd(&root, None)
        .args(["g", "scaffold", "Task", "id:uuid@pk", "title:string!"])
        .output()
        .unwrap();
    assert!(generated.status.success(), "{generated:?}");
    let query = root.join("src/main/resources/db/queries/manual.sql");
    fs::create_dir_all(query.parent().unwrap()).unwrap();
    fs::write(&query, "select id from tasks where id = :id;\n").unwrap();
    let before = snapshot_tree(&root);

    let refused = jails_cmd(&root, None)
        .args([
            "rename",
            "resource",
            "Billing.Task",
            "WorkItem",
            "--strategy",
            "single-cutover",
            "--force",
        ])
        .output()
        .unwrap();
    assert!(!refused.status.success(), "{refused:?}");
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains("manual-edit-required"), "{stderr}");
    assert!(stderr.contains("manual.sql"), "{stderr}");
    assert_eq!(snapshot_tree(&root), before, "refusal wrote project files");
}

#[test]
fn coordinated_resource_rename_reports_reader_owned_java_without_rewriting_it() {
    let root = temp_dir("resource-rename-manual-java");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();
    let generated = jails_cmd(&root, None)
        .args(["g", "scaffold", "Task", "id:uuid@pk", "title:string!"])
        .output()
        .unwrap();
    assert!(generated.status.success(), "{generated:?}");
    let manual = root.join("src/main/java/com/example/demo/Manual.java");
    fs::write(
        &manual,
        "package com.example.demo;\nimport com.example.demo.domain.Task;\nfinal class Manual { Task task; }\n",
    )
    .unwrap();
    let before = snapshot_tree(&root);

    let refused = jails_cmd(&root, None)
        .args([
            "rename",
            "resource",
            "Billing.Task",
            "WorkItem",
            "--strategy",
            "preserve-table",
            "--force",
        ])
        .output()
        .unwrap();
    assert!(!refused.status.success(), "{refused:?}");
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains("manual-edit-required"), "{stderr}");
    assert!(stderr.contains("Manual.java"), "{stderr}");
    assert_eq!(snapshot_tree(&root), before, "refusal wrote project files");
    assert!(fs::read_to_string(manual).unwrap().contains("Task task"));
}

#[test]
fn resource_repair_restores_sealed_history_and_missing_owned_projections() {
    let root = temp_dir("resource-repair");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();
    let generated = jails_cmd(&root, None)
        .args(["g", "scaffold", "Task", "id:uuid@pk", "title:string!"])
        .output()
        .unwrap();
    assert!(generated.status.success(), "{generated:?}");
    let evolved = jails_cmd(&root, None)
        .args(["resource", "field", "add", "Task", "priority:int?"])
        .output()
        .unwrap();
    assert!(evolved.status.success(), "{evolved:?}");

    let migration = root.join("src/main/resources/db/migration/V001__create_tasks.sql");
    let sealed = fs::read(&migration).unwrap();
    fs::write(&migration, b"-- accidentally edited\n").unwrap();
    let controller = root.join("src/main/java/com/example/demo/web/TaskController.java");
    fs::remove_file(&controller).unwrap();

    let repaired = jails_cmd(&root, None)
        .args(["resource", "repair", "Task", "--strategy", "roll-forward"])
        .output()
        .unwrap();
    assert!(
        repaired.status.success(),
        "{}{}",
        String::from_utf8_lossy(&repaired.stdout),
        String::from_utf8_lossy(&repaired.stderr)
    );
    assert_eq!(fs::read(&migration).unwrap(), sealed);
    assert!(
        controller.is_file(),
        "repair did not recreate {controller:?}"
    );

    fs::remove_file(&migration).unwrap();
    let repaired_missing = jails_cmd(&root, None)
        .args(["resource", "repair", "Task", "--strategy", "roll-forward"])
        .output()
        .unwrap();
    assert!(repaired_missing.status.success(), "{repaired_missing:?}");
    assert_eq!(fs::read(&migration).unwrap(), sealed);

    let store = jails_commit::store::Store::at(&root)
        .observe()
        .unwrap()
        .ledger
        .unwrap();
    assert_eq!(store.lifecycles.len(), 1);
    assert_eq!(store.lifecycles[0].migrations.len(), 2);
}

#[test]
fn task_drop_keeps_v001_and_appends_an_exact_forward_migration() {
    let root = temp_dir("task-drop-migration");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();
    assert!(
        jails_cmd(&root, None)
            .args(["g", "scaffold", "Task", "id:uuid@pk", "title:string!"])
            .status()
            .unwrap()
            .success()
    );

    let create = root.join("src/main/resources/db/migration/V001__create_tasks.sql");
    let sealed = fs::read_to_string(&create).unwrap();
    let before_wrong_confirmation = snapshot_tree(&root);
    let wrong = jails_cmd(&root, None)
        .args([
            "destroy",
            "scaffold",
            "Task",
            "--storage",
            "drop",
            "--confirm-table",
            "task",
            "--force",
        ])
        .output()
        .unwrap();
    assert!(!wrong.status.success(), "{wrong:?}");
    let wrong_stderr = String::from_utf8_lossy(&wrong.stderr);
    assert!(wrong_stderr.contains("is not `tasks`"), "{wrong_stderr}");
    assert_eq!(
        snapshot_tree(&root),
        before_wrong_confirmation,
        "wrong confirmation wrote files"
    );

    let dropped = jails_cmd(&root, None)
        .args([
            "destroy",
            "scaffold",
            "Task",
            "--storage",
            "drop",
            "--confirm-table",
            "tasks",
            "--force",
        ])
        .output()
        .unwrap();
    assert!(dropped.status.success(), "{dropped:?}");
    assert_eq!(fs::read_to_string(&create).unwrap(), sealed);
    assert_eq!(
        fs::read_to_string(root.join("src/main/resources/db/migration/V002__drop_tasks.sql"))
            .unwrap(),
        "drop table tasks;\n"
    );
    assert!(
        !root
            .join("src/main/java/com/example/demo/web/TaskController.java")
            .exists()
    );
    let store = jails_commit::store::Store::at(&root)
        .observe()
        .unwrap()
        .ledger
        .unwrap();
    assert_eq!(store.lifecycles.len(), 1, "{:?}", store.lifecycles);
    let lifecycle = &store.lifecycles[0];
    assert!(matches!(
        &lifecycle.state,
        jails_protocol::lifecycle::ResourceState::RetiredDropPlanned { migration, .. }
            if migration.as_str() == "src/main/resources/db/migration/V002__drop_tasks.sql"
    ));
    assert_eq!(
        lifecycle
            .migrations
            .iter()
            .map(|seal| seal.version.get())
            .collect::<Vec<_>>(),
        vec![1, 2]
    );

    let before_revive = snapshot_tree(&root);
    let revive = jails_cmd(&root, None)
        .args(["resource", "revive", "Task", "--table", "tasks"])
        .output()
        .unwrap();
    assert!(!revive.status.success(), "{revive:?}");
    assert!(
        String::from_utf8_lossy(&revive.stderr).contains("append-only drop planned"),
        "{}",
        String::from_utf8_lossy(&revive.stderr)
    );
    assert_eq!(snapshot_tree(&root), before_revive, "refusal wrote files");
}

#[test]
fn scaffold_reuses_an_existing_record_and_destroy_preserves_it() {
    let root = temp_dir("scaffold-model-first");
    write_spring_fixture(&root);

    let record = jails_cmd(&root, None)
        .args(["generate", "record", "Post", "id:uuid", "title:string!"])
        .status()
        .unwrap();
    assert!(record.success());

    let scaffold = jails_cmd(&root, None)
        .args(["generate", "scaffold", "Post"])
        .status()
        .unwrap();
    assert!(scaffold.success());

    let record_path = root.join("src/main/java/com/example/demo/domain/Post.java");
    let source = fs::read_to_string(&record_path).unwrap();
    assert!(source.contains("UUID id"), "{source}");
    assert!(source.contains("String title"), "{source}");

    let destroy = jails_cmd(&root, None)
        .args(["destroy", "scaffold", "Post", "--force"])
        .status()
        .unwrap();
    assert!(destroy.success());
    assert!(
        record_path.is_file(),
        "destroy scaffold must not remove the record created by a prior intent"
    );
    assert!(
        root.join("src/test/java/com/example/demo/domain/PostTest.java")
            .is_file(),
        "the record intent itself remains tracked and intact"
    );
    assert!(
        !root
            .join("src/main/java/com/example/demo/web/PostController.java")
            .exists()
    );
}

#[test]
fn field_driven_generators_refuse_an_absent_model_with_a_fix() {
    let root = temp_dir("missing-model-fix");
    write_spring_fixture(&root);

    for kind in ["scaffold", "dto", "repo"] {
        let output = jails_cmd(&root, None)
            .args(["generate", kind, "Missing"])
            .output()
            .unwrap();
        assert!(!output.status.success(), "{kind} unexpectedly succeeded");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("fix:"), "{kind}: {stderr}");
        assert!(stderr.contains("g record Missing"), "{kind}: {stderr}");
    }
}

#[test]
fn generate_field_updates_unchanged_derivatives_preserves_edits_and_adds_a_migration() {
    let root = temp_dir("generate-field");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();

    let scaffold = jails_cmd(&root, None)
        .args(["g", "scaffold", "Note", "id:uuid@pk", "title:string!"])
        .status()
        .unwrap();
    assert!(scaffold.success());

    let request = root.join("src/main/java/com/example/demo/web/NoteRequest.java");
    let edited = format!(
        "{}// user-owned validation\n",
        fs::read_to_string(&request).unwrap()
    );
    fs::write(&request, &edited).unwrap();

    let refused = jails_cmd(&root, None)
        .args(["resource", "field", "add", "Note", "createdAt:instant"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("needs a data plan"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert!(
        !root
            .join("src/main/resources/db/migration/V002__add_created_at_to_notes.sql")
            .exists(),
        "a refused data plan must not mutate the project"
    );

    let output = jails_cmd(&root, None)
        .args([
            "resource",
            "field",
            "add",
            "Note",
            "createdAt:instant",
            "--default-literal",
            "2026-08-25T12:00:00Z",
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
    // The plan names the files a new component touches: the record, its test
    // and the migration that adds the column. V1 printed a per-derivative
    // `skipped`/`add component` line from a walk of its own.
    assert!(stdout.contains("replace "), "{stdout}");
    assert!(stdout.contains(".sql"), "{stdout}");

    let record =
        fs::read_to_string(root.join("src/main/java/com/example/demo/domain/Note.java")).unwrap();
    assert!(record.contains("Instant createdAt"), "{record}");
    let jdbc = fs::read_to_string(
        root.join("src/main/java/com/example/demo/adapters/JdbcNoteRepository.java"),
    )
    .unwrap();
    assert!(jdbc.contains("created_at"), "{jdbc}");
    // The edited derivative is *merged*, not skipped. V1 left it alone, which
    // preserved the edit and left the request record missing the component the
    // record had just grown -- a DTO that no longer describes its own domain
    // type. Both halves survive here: the reader's line and the new field.
    let merged = fs::read_to_string(&request).unwrap();
    assert!(merged.contains("// user-owned validation"), "{merged}");
    assert!(merged.contains("Instant createdAt"), "{merged}");
    let _ = &edited;

    let migration = fs::read_to_string(
        root.join("src/main/resources/db/migration/V002__add_created_at_to_notes.sql"),
    )
    .unwrap();
    assert!(
        migration.contains("add column created_at timestamptz"),
        "{migration}"
    );
    assert!(
        migration.contains("set created_at = '2026-08-25T12:00:00Z'"),
        "{migration}"
    );
    assert!(
        migration.contains("alter column created_at set not null"),
        "{migration}"
    );
    assert!(
        !migration.contains("default current_timestamp"),
        "{migration}"
    );

    assert!(
        common::ledger_mentions(
            &root,
            "src/main/resources/db/migration/V002__add_created_at_to_notes.sql"
        ),
        "the new migration is recorded against the intent that wrote it"
    );
    assert!(
        common::ledger_mentions(&root, env!("CARGO_PKG_VERSION")),
        "the ledger records which jails wrote it"
    );

    let duplicate = jails_cmd(&root, None)
        .args(["g", "field", "Note", "createdAt:instant"])
        .output()
        .unwrap();
    assert!(!duplicate.status.success());
    assert!(
        String::from_utf8_lossy(&duplicate.stderr).contains("already has a `createdAt` component")
    );

    let alias = jails_cmd(&root, None)
        .args([
            "g",
            "field",
            "Note",
            "priority:int",
            "--default-literal",
            "7",
        ])
        .output()
        .unwrap();
    assert!(alias.status.success(), "{alias:?}");
    let alias_migration = fs::read_to_string(
        root.join("src/main/resources/db/migration/V003__add_priority_to_notes.sql"),
    )
    .unwrap();
    assert!(
        alias_migration.contains("set priority = 7"),
        "{alias_migration}"
    );
}

#[test]
fn resource_field_commands_use_the_risk_specific_cli_contracts() {
    let root = temp_dir("resource-field-commands");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();
    let generated = jails_cmd(&root, None)
        .args([
            "g",
            "record",
            "Task",
            "title:string!",
            "priority:int",
            "description:string",
            "legacyCode:string?",
        ])
        .output()
        .unwrap();
    assert!(generated.status.success(), "{generated:?}");
    fs::create_dir_all(root.join("db/backfills")).unwrap();
    fs::write(
        root.join("db/backfills/task_description.sql"),
        "update tasks set description = 'unknown' where description is null;\n",
    )
    .unwrap();

    for args in [
        vec![
            "resource",
            "field",
            "rename",
            "Task",
            "title",
            "headline",
            "--column",
            "single-cutover",
        ],
        vec![
            "resource",
            "field",
            "type",
            "Task",
            "priority",
            "--to",
            "long",
            "--strategy",
            "safe",
        ],
        vec![
            "resource",
            "field",
            "nullability",
            "Task",
            "description",
            "--nullable",
        ],
        vec![
            "resource",
            "field",
            "drop",
            "Task",
            "legacyCode",
            "--confirm-column",
            "legacy_code",
        ],
    ] {
        let output = jails_cmd(&root, None).args(args).output().unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let refused = jails_cmd(&root, None)
        .args([
            "resource",
            "field",
            "nullability",
            "Task",
            "description",
            "--required",
        ])
        .output()
        .unwrap();
    assert!(!refused.status.success(), "{refused:?}");
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("--backfill-file"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    let required = jails_cmd(&root, None)
        .args([
            "resource",
            "field",
            "nullability",
            "Task",
            "description",
            "--required",
            "--backfill-file",
            "db/backfills/task_description.sql",
        ])
        .output()
        .unwrap();
    assert!(required.status.success(), "{required:?}");
    let migration = fs::read_to_string(
        root.join("src/main/resources/db/migration/V005__make_description_required.sql"),
    )
    .unwrap();
    assert!(
        migration.contains("set description = 'unknown'"),
        "{migration}"
    );
    assert!(
        migration.contains("alter column description set not null"),
        "{migration}"
    );

    let record =
        fs::read_to_string(root.join("src/main/java/com/example/demo/domain/Task.java")).unwrap();
    assert!(record.contains("String headline"), "{record}");
    assert!(record.contains("long priority"), "{record}");
    assert!(record.contains("String description"), "{record}");
    assert!(!record.contains("legacyCode"), "{record}");
}

#[test]
fn scaffold_refuses_to_silently_flatten_a_project_record_component() {
    let root = temp_dir("scaffold-project-record");
    write_spring_fixture(&root);
    assert!(
        jails_cmd(&root, None)
            .args(["g", "record", "User", "id:uuid@pk", "name:string!"])
            .status()
            .unwrap()
            .success()
    );

    let output = jails_cmd(&root, None)
        .args(["g", "scaffold", "Post", "id:uuid@pk", "author:User"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("cannot be persisted"), "{stderr}");
    assert!(stderr.contains("author:UUID"), "{stderr}");
    assert!(stderr.contains("g association"), "{stderr}");
    assert!(stderr.contains("--on Post --yields User"), "{stderr}");
    assert!(
        !root
            .join("src/main/java/com/example/demo/domain/Post.java")
            .exists(),
        "the refusal must happen before the first write"
    );
}

#[test]
fn scaffold_timestamps_flow_through_ddl_create_and_optimistic_updates() {
    let root = temp_dir("scaffold-timestamps");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();

    let scaffold = jails_cmd(&root, None)
        .args([
            "g",
            "scaffold",
            "Note",
            "id:uuid@pk",
            "title:string!",
            "version:long",
            "--timestamps",
        ])
        .status()
        .unwrap();
    assert!(scaffold.success());
    let record =
        fs::read_to_string(root.join("src/main/java/com/example/demo/domain/Note.java")).unwrap();
    assert!(record.contains("Instant createdAt"), "{record}");
    assert!(record.contains("Instant updatedAt"), "{record}");
    let ddl =
        fs::read_to_string(root.join("src/main/resources/db/migration/V001__create_notes.sql"))
            .unwrap();
    assert!(ddl.contains("created_at"), "{ddl}");
    assert!(ddl.contains("updated_at"), "{ddl}");

    let transition = jails_cmd(&root, None)
        .args([
            "g",
            "transition",
            "RenameNote",
            "id:uuid",
            "title:string!",
            "version:long",
            "--on",
            "Note",
        ])
        .status()
        .unwrap();
    assert!(transition.success());
    let adapter = fs::read_to_string(
        root.join("src/main/java/com/example/demo/adapters/JdbcRenameNoteTransition.java"),
    )
    .unwrap();
    assert!(
        adapter.contains("updated_at = current_timestamp"),
        "{adapter}"
    );
}

#[test]
fn scaffold_writes_http_requests_and_factory_builds_typed_test_data() {
    let root = temp_dir("requests-factory");
    write_spring_fixture(&root);

    assert!(
        jails_cmd(&root, None)
            .args([
                "g",
                "scaffold",
                "Note",
                "id:uuid@pk",
                "title:string!",
                "createdAt:instant",
            ])
            .status()
            .unwrap()
            .success()
    );
    let requests = fs::read_to_string(root.join("requests/note.http")).unwrap();
    assert!(requests.contains("POST {{baseUrl}}/notes"), "{requests}");
    assert!(
        requests.contains("\"createdAt\": \"2026-01-01T00:00:00Z\""),
        "{requests}"
    );

    assert!(
        jails_cmd(&root, None)
            .args(["g", "factory", "Note"])
            .status()
            .unwrap()
            .success()
    );
    let factory =
        fs::read_to_string(root.join("src/test/java/com/example/demo/testkit/NoteFactory.java"))
            .unwrap();
    assert!(
        factory.contains("public static NoteFactory aNote()"),
        "{factory}"
    );
    assert!(factory.contains("withTitle(String value)"), "{factory}");
    assert!(factory.contains("Instant.parse("), "{factory}");
    assert!(factory.contains("return new Note("), "{factory}");
}

/// The generated guard has to *run*, not merely compile.
///
/// Every claim this generator makes is behavioural -- a retry replays rather
/// than conflicts, a reused key is refused, an in-flight key is told to wait --
/// and none of it is visible to a compile check. The generated unit test
/// asserts all four outcomes, so the thing to verify here is that Surefire
/// actually executes it: `plan.md` §18's standing warning is that a skipped
/// tier-3 test reports as a pass.
#[test]
fn generate_idempotency_produces_tests_that_run_and_pass() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    if !real_java_supports_target_release() {
        skip("javac does not accept the target release");
        return;
    }
    let root = temp_dir("idempotency-real");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();

    assert!(
        jails_cmd(&root, None)
            .args(["add", "db", "--no-start"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        jails_cmd(&root, None)
            .args(["g", "idempotency", "Request"])
            .status()
            .unwrap()
            .success()
    );

    let path = real_path_without_mvnd();
    let verified = verified_spring_db_toolbox(&path);
    assert_surefire_test_count(verified, "RequestGuardTest", 5);
}

/// Reading is not writing.
///
/// `app plan`, `--pretend` and inspection all reach the store, and a reader
/// that also tidied up would make asking jails what it would do change what it
/// had done. It is not a hypothetical: the provenance reader used to fold a
/// pre-ledger `.jails/` and delete the old files from inside the read, so
/// asking "is there anything to destroy" consumed the evidence for the answer.
#[test]
fn planning_pretending_and_inspecting_leave_machine_state_byte_for_byte() {
    let root = temp_dir("read-purity");
    write_plain_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/app.toml"),
        "schema = 1\ncapabilities = []\n\n[[generate]]\nkind = \"record\"\nname = \"Note\"\n\
         fields = [\"title:string!\"]\n",
    )
    .unwrap();

    // A project with no store yet. No read may create one -- an empty `.jails`
    // would make a project that has never been touched look like one that has.
    let arguments = [
        vec!["app", "plan"],
        vec!["destroy", "record", "Note", "--pretend"],
        vec!["generate", "record", "Other", "title:string!", "--pretend"],
        vec!["routes"],
    ];
    let before = snapshot_tree(&root.join(".jails"));
    for argv in &arguments {
        jails_cmd(&root, None).args(argv).output().unwrap();
        assert_eq!(
            before,
            snapshot_tree(&root.join(".jails")),
            "`jails {}` changed machine state while only being asked to report",
            argv.join(" ")
        );
    }
    assert!(
        !root.join(".jails/ledger.toml").exists(),
        "and no read created the ledger either"
    );

    // The same once there *is* a store, which is the case with something to
    // lose: a read that rewrote it would move the generation nothing committed.
    let applied = jails_cmd(&root, None)
        .args(["app", "apply", "--no-start"])
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}{}",
        String::from_utf8_lossy(&applied.stdout),
        String::from_utf8_lossy(&applied.stderr)
    );
    assert!(root.join(".jails/ledger.toml").is_file());
    let before = snapshot_tree(&root.join(".jails"));
    for argv in &arguments {
        jails_cmd(&root, None).args(argv).output().unwrap();
        assert_eq!(
            before,
            snapshot_tree(&root.join(".jails")),
            "`jails {}` changed a committed store while only being asked to report",
            argv.join(" ")
        );
    }
}

#[test]
fn a_timestamped_scaffold_does_not_ask_the_caller_for_its_audit_columns() {
    // `jails g scaffold --help` promises "the generated create path supplies
    // both". It did not: they arrived as `@NotNull` wire components, so the
    // documented POST answered 400 naming two fields the caller has no
    // business setting -- found by sending it at a running application.
    let root = temp_dir("scaffold-timestamps-wire");
    write_spring_fixture(&root);
    assert!(
        jails_cmd(&root, None)
            .args([
                "g",
                "scaffold",
                "Note",
                "id:uuid@pk",
                "title:string!",
                "--timestamps",
            ])
            .status()
            .unwrap()
            .success()
    );

    let request =
        fs::read_to_string(root.join("src/main/java/com/example/demo/web/NoteRequest.java"))
            .unwrap();
    assert!(!request.contains("Instant createdAt"), "{request}");
    assert!(!request.contains("Instant updatedAt"), "{request}");
    assert!(
        request.contains("Instant now = Instant.now();"),
        "{request}"
    );

    // The record still declares them, and the response still returns them: the
    // server sets these, it does not hide them.
    let record =
        fs::read_to_string(root.join("src/main/java/com/example/demo/domain/Note.java")).unwrap();
    assert!(record.contains("Instant createdAt"), "{record}");
    let response =
        fs::read_to_string(root.join("src/main/java/com/example/demo/web/NoteResponse.java"))
            .unwrap();
    assert!(response.contains("Instant createdAt"), "{response}");

    // And the sendable collection describes a request that can be made -- as
    // does the generated controller test, which sends that same body.
    let requests = fs::read_to_string(root.join("requests/note.http")).unwrap();
    assert!(!requests.contains("createdAt"), "{requests}");
    let controller_test =
        fs::read_to_string(root.join("src/test/java/com/example/demo/web/NoteControllerTest.java"))
            .unwrap();
    assert!(
        controller_test.contains("theDocumentedCreateRequestIsAccepted"),
        "{controller_test}"
    );
    // The JSON key, not the word: the test's own Javadoc names the defect.
    assert!(
        !controller_test.contains("\"createdAt\":"),
        "{controller_test}"
    );
}

#[test]
fn a_scoped_scaffold_documents_only_the_request_its_controller_answers() {
    // A scoped resource is create-only: every read has to carry the tenant, so
    // it is a `jails g query`. The collection used to end with a `### List`
    // block regardless, and the generated controller test already asserted
    // that same GET is a 405 -- the test knew and the collection did not.
    let root = temp_dir("scoped-scaffold-requests");
    write_spring_fixture(&root);
    assert!(
        jails_cmd(&root, None)
            .args(["add", "security"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        jails_cmd(&root, None)
            .args(["g", "scaffold", "Note", "id:uuid@pk@scope", "title:string!"])
            .status()
            .unwrap()
            .success()
    );

    let requests = fs::read_to_string(root.join("requests/note.http")).unwrap();
    assert!(requests.contains("POST {{baseUrl}}/notes"), "{requests}");
    assert!(!requests.contains("GET {{baseUrl}}"), "{requests}");

    let controller_test =
        fs::read_to_string(root.join("src/test/java/com/example/demo/web/NoteControllerTest.java"))
            .unwrap();
    assert!(
        controller_test.contains("hasStatus(405)"),
        "{controller_test}"
    );
    assert!(
        controller_test.contains("theDocumentedCreateRequestIsAccepted"),
        "{controller_test}"
    );
}

#[test]
fn scaffold_refuses_an_unmapped_project_type_before_writing() {
    let root = temp_dir("scaffold-unmapped-type");
    write_spring_fixture(&root);
    let output = jails_cmd(&root, None)
        .args(["g", "scaffold", "Book", "id:uuid@pk", "author:Author"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("author:Author"), "{stderr}");
    assert!(stderr.contains("cannot persist"), "{stderr}");
    assert!(stderr.contains("fix:"), "{stderr}");
    assert!(
        !root
            .join("src/main/java/com/example/demo/domain/Book.java")
            .exists()
    );
    assert!(!root.join("src/main/resources/db/migration").exists());
}

#[test]
fn generated_http_sink_delivers_typed_json_with_a_stable_idempotency_key() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    if !real_java_available() {
        skip("java/javac not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("http-outbox-sink-real");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/app.toml"),
        include_str!("../../examples/support-inbox/.jails/app.toml"),
    )
    .unwrap();

    let output = jails_cmd_with_path(&root, &path)
        .args(["app", "apply", "--no-start"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "apply: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let support = verified_app_unit_fixtures(&path)
        .iter()
        .find(|(name, _)| *name == "support-inbox")
        .map(|(_, root)| root)
        .unwrap();
    assert_surefire_test_count(support, "ProviderHttpOutboxSinkTest", 1);
}

/// The other direction: a dispatcher somebody is already using keeps the
/// entry point.
///
/// `App.java` with a command registered in it is the project's real CLI, and
/// a second `generate cli` must not move the jar out from under it.
#[test]
fn a_dispatcher_already_in_use_keeps_the_entry_point() {
    let workdir = temp_dir("cli-entry-point-kept");
    assert!(
        jails_cmd(&workdir, None)
            .args(["new-cli", "demo", "--no-git"])
            .status()
            .unwrap()
            .success()
    );
    let root = workdir.join("demo");

    // A command registered into `App` makes it the project's own CLI.
    assert!(
        jails_cmd(&root, None)
            .args(["g", "command", "Import"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        jails_cmd(&root, None)
            .args(["g", "cli", "Admin"])
            .status()
            .unwrap()
            .success()
    );

    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(
        pom.contains("<mainClass>com.example.demo.App</mainClass>"),
        "the entry point moved out from under a dispatcher in use:\n{pom}"
    );
}

#[test]
fn destroy_without_force_prompts_and_aborts_on_no() {
    let root = temp_dir("destroy-prompt");
    write_project_skeleton(&root);
    jails_cmd(&root, None)
        .args(["generate", "controller", "comment"])
        .status()
        .unwrap();
    let file = root.join("src/main/java/com/example/demo/web/CommentController.java");
    assert!(file.is_file());

    let mut child = jails_cmd(&root, None)
        .args(["destroy", "controller", "comment"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"n\n").unwrap();
    let status = child.wait().unwrap();

    assert!(status.success());
    assert!(
        file.is_file(),
        "file should survive a declined confirmation"
    );
}

#[test]
fn generate_twice_writes_nothing_the_second_time() {
    let root = temp_dir("duplicate");
    write_project_skeleton(&root);
    let service = root.join("src/main/java/com/example/demo/service/CommentService.java");
    jails_cmd(&root, None)
        .args(["generate", "service", "comment"])
        .status()
        .unwrap();
    let before = fs::read_to_string(&service).unwrap();
    let output = jails_cmd(&root, None)
        .args(["generate", "service", "comment"])
        .output()
        .unwrap();
    // A second identical generate is a **no-op**, not a refusal. V1 refused
    // because it would otherwise clobber whatever was there; here the file is
    // owned by the intent that wrote it, so "nothing changed" is the honest
    // answer -- and an edited file is three-way merged rather than refused,
    // which `app_manifest_merges_an_edited_intent_over_user_changes` pins.
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("nothing to do"),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(fs::read_to_string(&service).unwrap(), before);
}

#[test]
fn generate_errors_on_unknown_field_type() {
    let root = temp_dir("unknown-field-type");
    write_project_skeleton(&root);
    let output = jails_cmd(&root, None)
        .args(["generate", "record", "widget", "id:nope"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown field type"));
}

#[test]
fn generate_errors_outside_a_project() {
    let root = temp_dir("no-project");
    let output = jails_cmd(&root, None)
        .args(["generate", "record", "widget"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("pom.xml"));
}

#[test]
fn short_generators_cover_raw_sql_and_test_seams() {
    let root = temp_dir("simple-generators");
    write_project_skeleton(&root);

    for args in [
        vec!["g", "interface", "IdGenerator"],
        vec!["g", "integration-test", "DatabaseSmoke"],
        vec!["g", "migration", "createRewardCore"],
        vec!["g", "mig", "add_outbox"],
        vec!["g", "repository", "Reward", "id:uuid"],
    ] {
        let output = jails_cmd(&root, None).args(&args).output().unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    assert!(
        root.join("src/main/java/com/example/demo/IdGenerator.java")
            .is_file()
    );
    assert!(
        root.join("src/test/java/com/example/demo/DatabaseSmokeIT.java")
            .is_file()
    );
    assert!(
        root.join("src/main/resources/db/migration/V001__create_reward_core.sql")
            .is_file()
    );
    assert!(
        root.join("src/main/resources/db/migration/V002__add_outbox.sql")
            .is_file()
    );
    assert!(
        root.join("src/main/java/com/example/demo/app/RewardRepository.java")
            .is_file()
    );
    let adapter = fs::read_to_string(
        root.join("src/main/java/com/example/demo/adapters/JdbcRewardRepository.java"),
    )
    .unwrap();
    assert!(adapter.contains("PreparedStatement") || adapter.contains("prepareStatement"));
    assert!(
        adapter.contains("\"\"\""),
        "raw SQL should be emitted as text blocks: {adapter}"
    );
    assert!(!adapter.contains("org.springframework"));
}

#[test]
fn generate_scaffold_produces_a_project_that_compiles_and_passes_tests() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-scaffold-compiles");
    write_spring_fixture(&root);

    let status = jails_cmd_with_path(&root, &path)
        .args([
            "generate",
            "scaffold",
            "Post",
            "title:string",
            "body:text",
            "published:boolean",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let verified = verified_spring_toolbox(&path);
    assert!(
        verified
            .join("target/test-classes/com/example/demo/web/PostControllerTest.class")
            .is_file(),
        "the shared Spring toolbox did not compile the scaffold tests"
    );
}

/// Regression coverage for the reported bug (standalone `generate
/// controller` not producing a test) plus real-compile verification of the
/// new controller/service/record companion test templates.
#[test]
fn standalone_generators_companion_tests_compile_and_pass() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-standalone-companion-tests");
    write_spring_fixture(&root);

    for args in [
        vec!["generate", "controller", "Health"],
        vec!["generate", "service", "Billing"],
        vec![
            "generate",
            "record",
            "Tag",
            "name:string",
            "createdAt:datetime",
        ],
    ] {
        let status = jails_cmd_with_path(&root, &path)
            .args(&args)
            .status()
            .unwrap();
        assert!(status.success(), "{args:?} failed");
    }

    let verified = verified_spring_toolbox(&path);
    for class in [
        "com/example/demo/web/HealthControllerTest.class",
        "com/example/demo/service/BillingServiceTest.class",
        "com/example/demo/domain/TagTest.class",
    ] {
        assert!(
            verified.join("target/test-classes").join(class).is_file(),
            "the Spring toolbox did not compile {class}"
        );
    }
}

/// `record`, `command` and `class` are the plain-Java kinds, so the bar for
/// them is a `new-cli` project -- no Spring anywhere -- that still compiles and
/// passes the tests they generate.
#[test]
fn record_and_command_compile_and_pass_in_a_plain_cli_project() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    if !real_java_supports_target_release() {
        skip(&format!(
            "javac on PATH does not support --release {TARGET_RELEASE}"
        ));
        return;
    }
    let path = real_path_without_mvnd();
    let workdir = temp_dir("real-record-command");
    jails_cmd_with_path(&workdir, &path)
        .args(["new-cli", "demo"])
        .status()
        .unwrap();
    let root = workdir.join("demo");

    for args in [
        vec![
            "generate",
            "record",
            "Money",
            "amount:long",
            "currency:string",
            "on:date",
        ],
        vec!["generate", "command", "Greet"],
        vec!["generate", "class", "MoneyMoved"],
    ] {
        let status = jails_cmd_with_path(&root, &path)
            .args(&args)
            .status()
            .unwrap();
        assert!(status.success(), "{args:?} failed");
    }

    // `class` is the one kind that lands in the base package rather than a
    // subpackage -- a wrong `place()` here would compile and still be wrong.
    assert!(
        root.join("src/main/java/com/example/demo/MoneyMoved.java")
            .exists()
    );
    assert!(
        root.join("src/test/java/com/example/demo/MoneyMovedTest.java")
            .exists()
    );

    let verified = verified_plain_toolbox(&path);
    for class in ["MoneyMoved", "cli/GreetCommand", "domain/Tally"] {
        assert!(
            verified
                .join(format!("target/classes/com/example/demo/{class}.class"))
                .is_file(),
            "shared toolchain matrix did not compile {class}"
        );
    }
}

/// The whole toolbox at once: every capability and every generator in one
/// project, then its own suite. This is the only tier that answers "does what
/// jails writes actually compile and pass" for the generated *test* code as
/// well as the main code -- a template that emits an uncompilable assertion
/// looks perfectly fine to every other tier.
#[test]
fn every_generator_and_capability_together_compiles_and_passes_tests() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    if !real_java_supports_target_release() {
        skip(&format!(
            "javac on PATH does not support --release {TARGET_RELEASE}"
        ));
        return;
    }
    let path = real_path_without_mvnd();
    let root = verified_plain_toolbox(&path);
    for path in [
        "target/classes/com/example/demo/cli/GreetCommand.class",
        "target/classes/com/example/demo/domain/Money.class",
        "target/classes/com/example/demo/domain/Tally.class",
        "target/test-classes/com/example/demo/BriefTest.class",
    ] {
        assert!(root.join(path).is_file(), "matrix did not compile {path}");
    }
}

/// The generators composing: an enum and a record, then a value type that
/// references both by name. Proves the three halves of the field syntax --
/// capitalised = a type this project owns, `!`/`?` optionality, and the
/// enum-aware sample values -- produce a project that actually compiles.
#[test]
fn generators_compose_through_user_owned_field_types() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    if !real_java_supports_target_release() {
        skip(&format!(
            "javac on PATH does not support --release {TARGET_RELEASE}"
        ));
        return;
    }
    let path = real_path_without_mvnd();
    let workdir = temp_dir("real-compose");
    jails_cmd_with_path(&workdir, &path)
        .args(["new-cli", "gym"])
        .status()
        .unwrap();
    let root = workdir.join("gym");

    for args in [
        vec!["generate", "enum", "currency", "GBP", "EUR"],
        vec![
            "generate",
            "record",
            "sourceRef",
            "system:string",
            "externalId:string",
        ],
        vec![
            "generate",
            "value",
            "canonicalTransaction",
            "id:string!",
            "date:date",
            "amountMinor:long",
            "currency:Currency",
            "source:SourceRef",
            "note:string?",
        ],
    ] {
        let status = jails_cmd_with_path(&root, &path)
            .args(&args)
            .status()
            .unwrap();
        assert!(status.success(), "{args:?} failed");
    }

    let value = fs::read_to_string(
        root.join("src/main/java/com/example/gym/domain/CanonicalTransaction.java"),
    )
    .unwrap();
    assert!(
        value.contains("Currency currency"),
        "an owned type is used verbatim: {value}"
    );
    assert!(value.contains("SourceRef source"), "{value}");
    assert!(
        value.contains("long amountMinor"),
        "built-ins stay primitive: {value}"
    );
    assert!(
        value.contains(r#"throw new IllegalArgumentException("id must not be blank")"#),
        "! means non-blank: {value}"
    );
    assert!(
        value.contains("Optional<String> note"),
        "? puts absence in the type: {value}"
    );
    assert!(
        value.contains("requireNonNullElse(note, Optional.empty())"),
        "a null Optional is normalised: {value}"
    );

    // `!` is a text rule; asking for it on a date is a mistake worth naming
    // rather than silently ignoring.
    //
    // The wording is the surviving parser's. `pending.md` §6.3 merged the two,
    // and the one that lives carries a `fix:` line -- which is why this asserts
    // on the sentence rather than on the older "only applies to text".
    let output = jails_cmd_with_path(&root, &path)
        .args(["generate", "value", "bad", "when:date!"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let refusal = String::from_utf8_lossy(&output.stderr);
    assert!(refusal.contains("is not text"), "{refusal}");
    assert!(refusal.contains("fix: drop the `!`"), "{refusal}");

    // An enum-typed component can be sampled by reading the enum, and a
    // component whose type is a record *this project already has* by reading
    // the record: `SourceRef` was generated two commands ago, so refusing to
    // build one would be the tool forgetting what it just wrote.
    let test = fs::read_to_string(
        root.join("src/test/java/com/example/gym/domain/CanonicalTransactionTest.java"),
    )
    .unwrap();
    assert!(test.contains("Currency.values()[0]"), "{test}");
    assert!(
        test.contains("new SourceRef("),
        "a component whose type is a record on disk is sampled from it: {test}"
    );
    assert!(
        !test.contains("@Disabled"),
        "every component is fabricable now, so nothing should be disabled: {test}"
    );

    // A generated sealed interface has no constructor, but its generated
    // variants are zero-component records. Any one is a valid non-null sample
    // for testing Stamped's own validation, so Jails can construct it without
    // guessing at business data.
    let status = jails_cmd_with_path(&root, &path)
        .args(["generate", "sealed", "outcome", "Accepted", "Rejected"])
        .status()
        .unwrap();
    assert!(status.success(), "generate sealed failed");
    let status = jails_cmd_with_path(&root, &path)
        .args([
            "generate",
            "value",
            "stamped",
            "at:string!",
            "result:Outcome",
        ])
        .status()
        .unwrap();
    assert!(status.success(), "generate value with a sealed type failed");
    let stamped =
        fs::read_to_string(root.join("src/test/java/com/example/gym/domain/StampedTest.java"))
            .unwrap();
    assert!(stamped.contains("new Outcome.Accepted()"), "{stamped}");
    assert!(
        !stamped.contains("@Disabled"),
        "a generated zero-component variant is a complete sample: {stamped}"
    );

    let verified = verified_plain_toolbox(&path);
    for class in ["Currency", "SourceRef", "CanonicalTransaction", "Stamped"] {
        assert!(
            verified
                .join(format!(
                    "target/classes/com/example/demo/domain/{class}.class"
                ))
                .is_file(),
            "shared toolchain matrix did not compile {class}"
        );
    }
}

#[test]
fn a_scaffold_with_database_types_compiles_including_its_derived_jdbc_adapter() {
    // The tier that answers the question the tool exists for. A unit test on
    // the SQL mapping cannot catch a generated expression that is merely
    // *nearly* right: `Timestamp.from(x.createdAt())` has the receiver in
    // the middle, so gluing the receiver on the front yields
    // `x.Timestamp.from(createdAt())` -- which reads fine and does not
    // compile. Only javac finds that.
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-scaffold-jdbc");
    write_spring_fixture(&root);

    // The enum has to exist before the record names it, or the mapping falls
    // back to "unmappable" and the interesting branch never runs.
    let status = jails_cmd_with_path(&root, &path)
        .args(["generate", "enum", "Currency", "GBP", "USD"])
        .status()
        .unwrap();
    assert!(status.success());

    let status = jails_cmd_with_path(&root, &path)
        .args([
            "generate",
            "scaffold",
            "Payout",
            "id:uuid",
            "amount:bigdecimal",
            "currency:Currency",
            "paidAt:instant",
            "note:string?",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let adapter = fs::read_to_string(
        root.join("src/main/java/com/example/demo/adapters/JdbcPayoutRepository.java"),
    )
    .unwrap();
    // Derived, not left as a TODO.
    assert!(
        !adapter.contains("UnsupportedOperationException"),
        "{adapter}"
    );
    assert!(
        adapter.contains("Timestamp.from(payout.paidAt())"),
        "{adapter}"
    );
    assert!(
        adapter.contains("Currency.valueOf(rows.getString(\"currency\"))"),
        "{adapter}"
    );
    // An Optional component is unwrapped on the way out and rebuilt on the way in.
    assert!(
        adapter.contains("Optional.ofNullable(rows.getString(\"note\"))"),
        "{adapter}"
    );
    assert!(adapter.contains("payout.note().orElse(null)"), "{adapter}");
    // The column list is shared by the select and the insert, so they agree.
    assert!(
        adapter.contains("insert into payouts (id, amount, currency, paid_at, note)"),
        "{adapter}"
    );

    // The DTOs name the project's own enum, so they have to import it --
    // `field.imports` only carries the built-in types' packages.
    let request =
        fs::read_to_string(root.join("src/main/java/com/example/demo/web/PayoutRequest.java"))
            .unwrap();
    assert!(
        request.contains("import com.example.demo.domain.Currency;"),
        "{request}"
    );

    let verified = verified_spring_db_toolbox(&path);
    assert!(
        verified
            .join("target/classes/com/example/demo/adapters/JdbcPayoutRepository.class")
            .is_file(),
        "the shared JDBC toolbox did not compile the derived adapter"
    );
}

#[test]
fn a_scaffold_emits_a_migration_whose_columns_match_the_adapter() {
    let root = temp_dir("scaffold-migration");
    write_spring_fixture(&root);
    // `add db` is what creates this directory; jails emits a migration only
    // when the project has somewhere to put one.
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();

    let output = jails_cmd(&root, None)
        .args([
            "generate",
            "scaffold",
            "Payout",
            "id:uuid",
            "amount:bigdecimal",
            "paidAt:instant",
            "note:string?",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");

    let migration =
        fs::read_to_string(root.join("src/main/resources/db/migration/V001__create_payouts.sql"))
            .unwrap();
    assert!(migration.contains("create table payouts ("), "{migration}");
    assert!(
        migration.contains("uuid") && migration.contains("numeric"),
        "{migration}"
    );
    // An Instant needs a zone-aware column or it comes back reinterpreted.
    assert!(migration.contains("timestamptz not null"), "{migration}");
    // The nullable component is the only one without `not null`.
    assert!(migration.contains("text,"), "{migration}");
    assert_eq!(
        migration.matches("not null").count(),
        3,
        "only the nullable component may lack `not null`: {migration}"
    );
    assert!(migration.contains("primary key (id)"), "{migration}");

    // The same column names the adapter selects and inserts.
    let adapter = fs::read_to_string(
        root.join("src/main/java/com/example/demo/adapters/JdbcPayoutRepository.java"),
    )
    .unwrap();
    for column in ["id", "amount", "paid_at", "note"] {
        assert!(migration.contains(column), "migration missing {column}");
        assert!(adapter.contains(column), "adapter missing {column}");
    }
}

#[test]
fn a_project_without_a_migration_directory_gets_no_migration() {
    let root = temp_dir("scaffold-no-migration");
    write_spring_fixture(&root);

    let output = jails_cmd(&root, None)
        .args(["generate", "scaffold", "Payout", "id:uuid"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(!root.join("src/main/resources/db/migration").exists());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("created migration"), "{stdout}");
}

#[test]
fn pretend_writes_nothing_but_still_reports_the_whole_plan() {
    let root = temp_dir("pretend");
    write_spring_fixture(&root);

    let output = jails_cmd(&root, None)
        .args(["generate", "scaffold", "Payout", "id:uuid", "--pretend"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("create "), "{stdout}");
    assert!(stdout.contains("nothing was written"), "{stdout}");
    assert!(
        !root
            .join("src/main/java/com/example/demo/domain/Payout.java")
            .exists()
    );
}

#[test]
fn pretend_is_global_and_reaches_destroy_too() {
    let root = temp_dir("pretend-destroy");
    write_spring_fixture(&root);
    let created = jails_cmd(&root, None)
        .args(["generate", "record", "Payout", "id:uuid"])
        .output()
        .unwrap();
    assert!(created.status.success());
    let file = root.join("src/main/java/com/example/demo/domain/Payout.java");
    assert!(file.is_file());

    let output = jails_cmd(&root, None)
        .args(["destroy", "record", "Payout", "--pretend"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("delete "), "{stdout}");
    // --pretend must not stop to ask for confirmation: nothing is at risk.
    assert!(!stdout.contains("proceed?"), "{stdout}");
    assert!(file.is_file(), "--pretend deleted a file");
}

#[test]
fn a_scaffold_writes_a_two_row_fixture_keyed_by_column_name() {
    let root = temp_dir("scaffold-fixture");
    write_spring_fixture(&root);
    // `new`/`new-cli` seed this directory; `add testkit` writes the loader
    // that reads it.
    fs::create_dir_all(root.join("src/test/resources/fixtures")).unwrap();

    let status = jails_cmd(&root, None)
        .args(["generate", "enum", "Currency", "GBP", "USD"])
        .status()
        .unwrap();
    assert!(status.success());

    let output = jails_cmd(&root, None)
        .args([
            "generate",
            "scaffold",
            "Payout",
            "paidAt:instant",
            "currency:Currency",
            "note:string?",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");

    let fixture =
        fs::read_to_string(root.join("src/test/resources/fixtures/payouts.json")).unwrap();
    // Column names, not component names -- the fixture describes what the
    // database holds, next to a JDBC adapter that reads those same columns.
    assert!(fixture.contains("\"paid_at\""), "{fixture}");
    assert!(!fixture.contains("paidAt"), "{fixture}");
    // A real constant read off the generated enum, not a guess.
    assert!(fixture.contains("\"currency\": \"GBP\""), "{fixture}");
    // Two rows, and the nullable one is absent in the second.
    assert!(fixture.contains("\"note\": \"sample-1\""), "{fixture}");
    assert!(fixture.contains("\"note\": null"), "{fixture}");
}

#[test]
fn a_project_without_a_fixtures_directory_gets_no_fixture() {
    let root = temp_dir("scaffold-no-fixture");
    write_spring_fixture(&root);

    let output = jails_cmd(&root, None)
        .args(["generate", "scaffold", "Payout", "id:uuid"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("created fixture"), "{stdout}");
}

#[test]
fn the_generated_controller_test_uses_the_assertj_mockmvc_entry_point() {
    // Spring Framework 7 / Boot 4 favour MockMvcTester over plain MockMvc:
    // one fluent chain instead of two families of static imports, and no
    // `throws Exception` on the test method.
    let root = temp_dir("controller-test-style");
    write_spring_fixture(&root);

    let status = jails_cmd(&root, None)
        .args(["generate", "controller", "Payout"])
        .status()
        .unwrap();
    assert!(status.success());

    let test = fs::read_to_string(
        root.join("src/test/java/com/example/demo/web/PayoutControllerTest.java"),
    )
    .unwrap();
    assert!(
        test.contains("org.springframework.test.web.servlet.assertj.MockMvcTester"),
        "{test}"
    );
    assert!(test.contains("assertThat(mvc.get().uri("), "{test}");
    assert!(test.contains("hasStatusOk()"), "{test}");
    // The old style would need these; the new one does not.
    assert!(!test.contains("MockMvcResultMatchers"), "{test}");
    assert!(!test.contains("throws Exception"), "{test}");
}

#[test]
fn generate_dto_client_and_job_compile_and_pass_against_real_spring() {
    // These target Spring Boot 4 / Framework 7 APIs that moved recently
    // (@ImportHttpServices, ProblemDetail, MockMvcTester), so a unit test on
    // the template text proves nothing worth knowing. javac and the real
    // context are the check.
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-spring-generators");
    write_spring_fixture(&root);

    // A domain record for the DTO to describe.
    assert!(
        jails_cmd_with_path(&root, &path)
            .args([
                "generate",
                "record",
                "Payout",
                "id:uuid",
                "amount:long",
                "note:string?"
            ])
            .status()
            .unwrap()
            .success()
    );

    for args in [
        vec!["generate", "dto", "Payout"],
        vec!["generate", "client", "Billing"],
        vec!["generate", "job", "Sweep"],
    ] {
        assert!(
            jails_cmd_with_path(&root, &path)
                .args(&args)
                .status()
                .unwrap()
                .success(),
            "{args:?} failed"
        );
    }

    let request =
        fs::read_to_string(root.join("src/main/java/com/example/demo/web/PayoutRequest.java"))
            .unwrap();
    // Constraints come from the field spec, so a bad request is rejected at
    // the edge rather than deep in the domain.
    assert!(request.contains("@NotNull UUID id"), "{request}");
    // An Optional domain component is a plain nullable field on the wire, and
    // carries no constraint -- `?` said it was optional.
    assert!(request.contains("String note"), "{request}");
    assert!(!request.contains("@NotNull String note"), "{request}");
    assert!(request.contains("Optional.ofNullable(note)"), "{request}");

    let client =
        fs::read_to_string(root.join("src/main/java/com/example/demo/clients/BillingClient.java"))
            .unwrap();
    assert!(client.contains("@GetExchange"), "{client}");
    // No base URL in the annotation: it belongs to the group's configuration.
    assert!(!client.contains("@HttpExchange(url"), "{client}");
    let config = fs::read_to_string(
        root.join("src/main/java/com/example/demo/clients/HttpClientsConfig.java"),
    )
    .unwrap();
    assert!(
        config.contains("@ImportHttpServices(group = \"billing\""),
        "{config}"
    );

    let job =
        fs::read_to_string(root.join("src/main/java/com/example/demo/jobs/SweepJob.java")).unwrap();
    // fixedDelay, not fixedRate: a slow run must not queue another on top.
    assert!(job.contains("fixedDelayString"), "{job}");
    // An exception escaping a @Scheduled method cancels every future run.
    assert!(job.contains("catch (RuntimeException"), "{job}");

    let verified = verified_spring_toolbox(&path);
    assert!(verified.join("target/test-classes").is_dir());
}

#[test]
fn generating_an_integration_test_also_configures_something_to_run_it() {
    // Surefire runs `*Test`; `*IT` belongs to Failsafe, and Failsafe is not
    // part of the Spring Boot parent's default build. Without this, every
    // generated IT is dead code and `mvn verify` still reports success --
    // a test that silently does not run is worse than no test, because the
    // green build claims it passed.
    let root = temp_dir("failsafe");
    write_spring_fixture(&root);
    assert!(
        !fs::read_to_string(root.join("pom.xml"))
            .unwrap()
            .contains("failsafe")
    );

    assert!(
        jails_cmd(&root, None)
            .args(["generate", "integration-test", "Payment"])
            .status()
            .unwrap()
            .success()
    );

    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("maven-failsafe-plugin"), "{pom}");
    // Both goals: `integration-test` runs them, `verify` is what makes a
    // failure fail the build. Binding only the first ignores the result.
    assert!(pom.contains("<goal>integration-test</goal>"), "{pom}");
    assert!(pom.contains("<goal>verify</goal>"), "{pom}");
    // No version: the Spring Boot parent manages it, and pinning one here
    // would drift from the platform.
    let plugin_block = &pom[pom.find("maven-failsafe-plugin").unwrap()..];
    let block_end = plugin_block.find("</plugin>").unwrap();
    assert!(!plugin_block[..block_end].contains("<version>"), "{pom}");

    // Idempotent: a second IT must not splice a second plugin block.
    assert!(
        jails_cmd(&root, None)
            .args(["generate", "integration-test", "Refund"])
            .status()
            .unwrap()
            .success()
    );
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert_eq!(pom.matches("maven-failsafe-plugin").count(), 1, "{pom}");
}

#[test]
fn generate_help_documents_the_field_syntax_at_the_point_of_typing() {
    // The field grammar is the thing you need while typing the command, and
    // it lived only in the README. clap reflows a doc comment into one
    // paragraph unless told not to, which turns the table and the examples
    // into a run-on -- so the formatting is worth asserting, not just the
    // presence of the words.
    let workdir = temp_dir("generate-help");
    let output = jails_cmd(&workdir, None)
        .args(["generate", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);

    assert!(help.contains("name:string!"), "{help}");
    assert!(help.contains("name:string?"), "{help}");
    assert!(help.contains("Case is the rule"), "{help}");
    assert!(help.contains("list<string>"), "{help}");
    // Line breaks survived: the table is indented lines, not one paragraph.
    assert!(help.contains("\n  name:string      required"), "{help}");
    assert!(help.contains("\n  jails g sealed Outcome"), "{help}");
    // Every kind carries a description rather than a bare name.
    assert!(help.contains("- scaffold:"), "{help}");
    assert!(help.contains("- sealed:"), "{help}");
}

/// The tier that answers what the tool is for. A strategy's interface,
/// implementations and tests have to agree on one method signature across
/// five files, and a mismatch is a compile error the user did not write.
///
/// Both modes are covered in one project because they generate different
/// signatures: `--yields` returns `Optional<T>`, a bare strategy `boolean`.
#[test]
fn generate_strategy_produces_a_project_that_compiles_and_passes_tests() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    if !real_java_supports_target_release() {
        skip(&format!(
            "javac on PATH does not support --release {TARGET_RELEASE}"
        ));
        return;
    }
    let path = real_path_without_mvnd();
    let workdir = temp_dir("real-strategy-compiles");
    jails_cmd_with_path(&workdir, &path)
        .args(["new-cli", "demo"])
        .status()
        .unwrap();
    let root = workdir.join("demo");

    // The types the generated signature names. Without them the strategy
    // would not compile, which is what the note at generation time says.
    for record in ["Transaction", "Reward"] {
        assert!(
            jails_cmd_with_path(&root, &path)
                .args(["g", "record", record, "id:uuid", "amount:long"])
                .status()
                .unwrap()
                .success()
        );
    }

    assert!(
        jails_cmd_with_path(&root, &path)
            .args([
                "g",
                "strategy",
                "RewardRule",
                "Coffee",
                "LargeTransaction",
                "--on",
                "Transaction",
                "--yields",
                "Reward",
            ])
            .status()
            .unwrap()
            .success()
    );
    // The predicate mode, whose method returns `boolean` rather than Optional.
    assert!(
        jails_cmd_with_path(&root, &path)
            .args([
                "g",
                "strategy",
                "Eligibility",
                "Domestic",
                "--on",
                "Transaction"
            ])
            .status()
            .unwrap()
            .success()
    );

    let verified = verified_plain_toolbox(&path);
    for class in ["Eligibility", "DomesticEligibility"] {
        assert!(
            verified
                .join(format!(
                    "target/classes/com/example/demo/domain/{class}.class"
                ))
                .is_file(),
            "shared toolchain matrix did not compile {class}"
        );
    }
}

/// `destroy strategy` reads the implementations back off disk rather than
/// rebuilding a variant list it was never given, so it takes out every class
/// implementing the interface -- including one added by hand afterwards.
/// Leaving that behind implementing a deleted interface stops the project
/// compiling, which is the failure the generate/destroy inverse rule exists
/// to prevent.
#[test]
fn destroy_strategy_removes_the_implementations_it_did_not_name() {
    let root = temp_dir("destroy-strategy");
    write_plain_fixture(&root);

    assert!(
        jails_cmd(&root, None)
            .args([
                "g",
                "strategy",
                "RewardRule",
                "Coffee",
                "--on",
                "Transaction"
            ])
            .status()
            .unwrap()
            .success()
    );

    // An implementation the generate call never knew about.
    let domain = root.join("src/main/java/com/example/demo/domain");
    fs::write(
        domain.join("HandWrittenRewardRule.java"),
        "package com.example.demo.domain;\n\n\
         public final class HandWrittenRewardRule implements RewardRule {\n\
         \x20   @Override\n\
         \x20   public boolean matches(Transaction transaction) {\n\
         \x20       return false;\n\
         \x20   }\n\
         }\n",
    )
    .unwrap();

    assert!(
        jails_cmd(&root, None)
            .args(["destroy", "strategy", "RewardRule", "--force"])
            .status()
            .unwrap()
            .success()
    );

    assert!(!domain.join("RewardRule.java").exists());
    assert!(!domain.join("CoffeeRewardRule.java").exists());
    assert!(
        !domain.join("HandWrittenRewardRule.java").exists(),
        "an implementation of a deleted interface was left behind"
    );
}

/// `--pretend` has to name every write. `package-info.java` was created as a
/// side effect of writing a class, so the preview listed two files and the
/// real run wrote three -- on the one command whose entire job is to tell you
/// what is about to happen.
///
/// The fix is that the preview and the apply consume the same list, rather
/// than the preview learning to predict a side effect: a second piece of code
/// guessing what the first will do is the drift this costs everywhere else.
#[test]
fn pretend_names_the_package_info_it_will_write() {
    let root = temp_dir("pkginfo-preview");
    write_plain_fixture(&root);
    // package-info is conditional on the annotation resolving.
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap().replace(
        "</dependencies>",
        "<dependency><groupId>org.jspecify</groupId>\
         <artifactId>jspecify</artifactId><version>1.0.0</version></dependency>\
         </dependencies>",
    );
    fs::write(root.join("pom.xml"), pom).unwrap();

    let preview = jails_cmd(&root, None)
        .args(["g", "record", "Note", "title:string", "--pretend"])
        .output()
        .unwrap();
    assert!(preview.status.success());
    let shown = String::from_utf8_lossy(&preview.stdout);
    assert!(
        shown.contains("package-info"),
        "the preview hid a file the real run writes:\n{shown}"
    );

    let real = jails_cmd(&root, None)
        .args(["g", "record", "Note", "title:string"])
        .output()
        .unwrap();
    assert!(real.status.success());
    let done = String::from_utf8_lossy(&real.stdout);

    // The preview and the run must name the same set of files.
    let files = |text: &str| -> Vec<String> {
        text.lines()
            .filter_map(|l| l.rsplit_once(' ').map(|(_, p)| p.to_string()))
            .filter(|p| p.ends_with(".java"))
            .collect()
    };
    assert_eq!(
        files(&shown),
        files(&done),
        "preview and apply disagreed about what would be written"
    );
    assert!(
        root.join("src/main/java/com/example/demo/domain/package-info.java")
            .is_file()
    );
}

/// One per package, not one per class -- `scaffold` puts several classes in
/// the same package -- and never in the test tree, where a nullness contract
/// buys nothing.
#[test]
fn planned_package_infos_are_one_per_package() {
    let root = temp_dir("pkginfo-dedup");
    write_spring_fixture(&root);
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap().replace(
        "</dependencies>",
        "<dependency><groupId>org.jspecify</groupId>\
         <artifactId>jspecify</artifactId><version>1.0.0</version></dependency>\
         </dependencies>",
    );
    fs::write(root.join("pom.xml"), pom).unwrap();

    let preview = jails_cmd(&root, None)
        .args(["g", "scaffold", "Task", "title:string", "--pretend"])
        .output()
        .unwrap();
    assert!(preview.status.success());
    let shown = String::from_utf8_lossy(&preview.stdout);

    let infos: Vec<&str> = shown
        .lines()
        .filter(|l| l.contains("package-info"))
        .collect();
    assert!(
        infos.len() > 1,
        "scaffold should span several packages:\n{shown}"
    );

    // No package planned twice, however many classes land in it.
    let mut seen = std::collections::HashSet::new();
    for line in &infos {
        assert!(
            seen.insert(*line),
            "the same package-info was planned twice:\n{shown}"
        );
    }

    // Never in the test tree.
    assert!(
        !infos.iter().any(|l| l.contains("src/test/java")),
        "{shown}"
    );
}

/// `plan.md` §6.6 tier 2. The want is "change what the generated code *looks
/// like*" -- not a new generator, just this class shaped differently.
#[test]
fn a_project_template_override_replaces_the_built_in_and_doctor_names_it() {
    let root = temp_dir("template-override");
    write_plain_fixture(&root);

    let overrides = root.join(".jails/templates/generate");
    fs::create_dir_all(&overrides).unwrap();
    let built_in = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/generate/command_test.java"),
    )
    .unwrap();
    // Same placeholders, different shape: the contract is the placeholder set,
    // not the text.
    fs::write(
        overrides.join("command_test.java"),
        format!("// generated by an overridden template\n{built_in}"),
    )
    .unwrap();

    let output = jails_cmd(&root, None)
        .args(["generate", "command", "Sync"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let generated =
        fs::read_to_string(root.join("src/test/java/com/example/demo/cli/SyncCommandTest.java"))
            .unwrap();
    assert!(
        generated.starts_with("// generated by an overridden template"),
        "{generated}"
    );
    assert!(generated.contains("class SyncCommandTest"), "{generated}");

    // The honesty half: an overridden template is not golden-tested, so
    // `doctor` names it rather than letting the reader find out from a build.
    let doctor = jails_cmd(&root, None).arg("doctor").output().unwrap();
    let report = String::from_utf8_lossy(&doctor.stdout);
    assert!(
        report.contains("generate/command_test.java"),
        "doctor must name the override: {report}"
    );
    assert!(
        report.contains("not covered by jails' snapshot tests"),
        "{report}"
    );
}

/// The placeholder set is the contract, and breaking it is the reader's typo --
/// so it is an error naming their file, not a panic naming jails'.
#[test]
fn a_template_override_missing_a_placeholder_is_refused_by_name() {
    let root = temp_dir("template-override-bad");
    write_plain_fixture(&root);

    let overrides = root.join(".jails/templates/generate");
    fs::create_dir_all(&overrides).unwrap();
    fs::write(
        overrides.join("command_test.java"),
        "package {{pkg}};\n\nclass Whatever {}\n",
    )
    .unwrap();

    let output = jails_cmd(&root, None)
        .args(["generate", "command", "Sync"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("command_test.java"), "{stderr}");
    assert!(stderr.contains("missing:"), "{stderr}");
    assert!(
        !root
            .join("src/test/java/com/example/demo/cli/SyncCommandTest.java")
            .exists(),
        "nothing is written when the override is refused"
    );
}

/// The generated *code* had a Boot floor the build file does not, and every
/// generator that hits it now writes the classic `MockMvc` form.
///
/// `pending.md` §1.2. Nine companion tests are written against `MockMvcTester`
/// (Spring Framework 6.2, Boot 3.4); two had a classic variant and seven
/// refused. The refusal was the right failure and the wrong feature —
/// `jails new --gradle --boot 2.7.18` exists so that a Boot 2 project can be
/// worked in — so all nine pick their form by version now.
#[test]
fn a_boot_2_project_gets_the_classic_mockmvc_for_every_generated_web_test() {
    let parent = temp_dir("gradle-boot2-generators");
    let created = jails_cmd(&parent, None)
        .args([
            "new",
            "svc",
            "--gradle",
            "--offline",
            "--boot",
            "2.7.18",
            "--java",
            "21",
            "--package",
            "com.acme.svc",
            "--no-devtools",
            "--no-git",
        ])
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let root = parent.join("svc");

    let generated = jails_cmd(&root, None)
        .args(["g", "controller", "Foo", "--method", "post"])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}{}",
        String::from_utf8_lossy(&generated.stdout),
        String::from_utf8_lossy(&generated.stderr)
    );
    let test =
        fs::read_to_string(root.join("src/test/java/com/acme/svc/web/FooControllerTest.java"))
            .unwrap();
    // MockMvcTester is Spring Framework 6.2; this project is Framework 5.3.
    // Asserted on the import rather than the word: the classic templates name
    // the type in a comment saying why it is not used.
    assert!(!test.contains("servlet.assertj.MockMvcTester"), "{test}");
    assert!(
        test.contains("import org.springframework.test.web.servlet.MockMvc;"),
        "{test}"
    );
    assert!(test.contains("mvc.perform(post(\"/foo\"))"), "{test}");
    assert!(test.contains("throws Exception"), "{test}");

    // `add cors` has a classic variant too.
    let cors = jails_cmd(&root, None)
        .args(["add", "cors", "--no-start"])
        .output()
        .unwrap();
    assert!(
        cors.status.success(),
        "{}",
        String::from_utf8_lossy(&cors.stderr)
    );
    let cors_test =
        fs::read_to_string(root.join("src/test/java/com/acme/svc/CorsConfigTest.java")).unwrap();
    assert!(
        !cors_test.contains("servlet.assertj.MockMvcTester"),
        "{cors_test}"
    );
    assert!(cors_test.contains("andExpect(status()"), "{cors_test}");

    // `add h2` must not declare a Boot 4 module or write a Boot 4 property
    // name. The first is unresolvable; the second is worse, because nothing
    // rejects it and exception translation simply stays on.
    let h2 = jails_cmd(&root, None)
        .args(["add", "h2", "--no-start"])
        .output()
        .unwrap();
    assert!(
        h2.status.success(),
        "{}",
        String::from_utf8_lossy(&h2.stderr)
    );
    let build = fs::read_to_string(root.join("build.gradle")).unwrap();
    assert!(
        !build.contains("spring-boot-h2console"),
        "Boot 4 module: {build}"
    );
    let properties =
        fs::read_to_string(root.join("src/main/resources/application.properties")).unwrap();
    assert!(
        properties.contains("spring.dao.exceptiontranslation.enabled=false"),
        "{properties}"
    );
    assert!(
        !properties.contains("spring.persistence.exceptiontranslation"),
        "{properties}"
    );

    // `g scaffold` used to refuse and writes the classic form now, and the
    // whole of what it generates compiles -- proved against real Maven by
    // `what_jails_generates_for_boot_2_compiles_and_what_cannot_refuses_by_name`.
    // `add api` and `add security` still refuse, for a reason that is not
    // about MockMvc at all: their *main* source set is Boot 3 code, which
    // `pending.md` §1.2's premise missed.
    let scaffolded = jails_cmd(&root, None)
        .args([
            "g",
            "scaffold",
            "Note",
            "id:uuid@pk",
            "title:string!",
            "version:long",
        ])
        .output()
        .unwrap();
    assert!(
        scaffolded.status.success(),
        "{}",
        String::from_utf8_lossy(&scaffolded.stderr)
    );
    let source =
        fs::read_to_string(root.join("src/test/java/com/acme/svc/web/NoteControllerTest.java"))
            .unwrap();
    assert!(
        !source.contains("servlet.assertj.MockMvcTester"),
        "the Framework 6.2 entry point: {source}"
    );
    assert!(
        source.contains("import org.springframework.test.web.servlet.MockMvc;"),
        "{source}"
    );
    assert!(source.contains("andExpect(status()"), "{source}");

    for (command, needs) in [
        (vec!["add", "api", "--no-start"], "ProblemDetail"),
        (vec!["add", "security", "--no-start"], "requestMatchers"),
    ] {
        let refused = jails_cmd(&root, None).args(&command).output().unwrap();
        let stderr = String::from_utf8_lossy(&refused.stderr);
        assert!(!refused.status.success(), "{command:?} should refuse");
        assert!(stderr.contains(needs), "{command:?}: {stderr}");
    }
}

/// `plan.md` §13.3's `g auth`. Both claims behind it are behavioural, so a
/// compile check would prove nothing: Boot auto-configures no `JwtEncoder`,
/// and `JwtTimestampValidator` accepts a token with no `exp` unless one line
/// says otherwise. The second is the reason `a_token_with_no_expiry_is_refused`
/// exists — delete that line and no other test notices.
#[test]
fn generate_auth_produces_tests_that_run_and_pass() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-auth");
    write_spring_fixture(&root);

    let status = jails_cmd_with_path(&root, &path)
        .args(["add", "security", "--no-start"])
        .status()
        .unwrap();
    assert!(status.success(), "add security failed");

    let status = jails_cmd_with_path(&root, &path)
        .args(["generate", "auth", "Api"])
        .status()
        .unwrap();
    assert!(status.success(), "generate auth failed");

    let verified = verified_spring_services_toolbox(&path);
    assert_surefire_test_count(verified, "ApiTokensTest", 4);
}

/// Without Spring Security there is no filter chain to read the token, so the
/// encoder and decoder would be beans nothing consumes.
#[test]
fn generate_auth_refuses_without_the_security_capability() {
    let root = temp_dir("auth-no-security");
    write_spring_fixture(&root);

    let output = jails_cmd(&root, None)
        .args(["generate", "auth", "Api"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(stderr.contains("jails add security"), "{stderr}");
}

/// `plan.md` §13.3's `g webhook`. Seven tests, and each is one of the ways an
/// inbound webhook is normally trusted when it should not be — or rejected
/// when it should not be, which is the failure mode nobody predicts.
#[test]
fn generate_webhook_produces_tests_that_run_and_pass() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-webhook");
    write_spring_fixture(&root);

    let status = jails_cmd_with_path(&root, &path)
        .args(["generate", "webhook", "Provider"])
        .status()
        .unwrap();
    assert!(status.success(), "generate webhook failed");

    let verified = verified_spring_services_toolbox(&path);
    assert_surefire_test_count(verified, "ProviderVerifierTest", 7);
}

/// `plan.md` §13.3's `g search`. The generated column is the whole point, and
/// the only thing that can prove the expression is right is PostgreSQL parsing
/// it — a Rust test on the string proves the string.
#[test]
fn generate_search_produces_a_project_that_compiles() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-search");
    write_spring_fixture(&root);

    for step in [
        vec!["add", "db", "--no-start"],
        vec![
            "generate",
            "scaffold",
            "Article",
            "id:uuid@pk",
            "title:string!",
            "body:string",
        ],
        vec!["generate", "search", "Article", "title", "body"],
    ] {
        let output = jails_cmd_with_path(&root, &path)
            .args(&step)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "`{}` failed: {}{}",
            step.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let verified = verified_spring_db_toolbox(&path);
    assert!(
        verified
            .join("target/classes/com/example/demo/adapters/JdbcArticleRepository.class")
            .is_file(),
        "the shared JDBC toolbox did not compile the search adapter"
    );
}

/// Indexing a non-text component is refused at generation time, where the
/// reader is, rather than at `flyway migrate` — which is the furthest possible
/// point from the mistake.
#[test]
fn generate_search_refuses_a_component_it_cannot_index() {
    let root = temp_dir("search-refusals");
    write_spring_fixture(&root);
    let status = jails_cmd(&root, None)
        .args(["add", "db", "--no-start"])
        .status()
        .unwrap();
    assert!(status.success());
    let status = jails_cmd(&root, None)
        .args([
            "generate",
            "scaffold",
            "Article",
            "id:uuid@pk",
            "views:long",
            "title:string!",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    for (args, expected) in [
        (
            vec!["generate", "search", "Article"],
            "needs the components",
        ),
        (
            vec!["generate", "search", "Article", "views"],
            "full-text search indexes text",
        ),
        (
            vec!["generate", "search", "Article", "nosuch"],
            "has no component",
        ),
        (
            vec!["generate", "search", "Missing", "title"],
            "needs the record it searches",
        ),
    ] {
        let output = jails_cmd(&root, None).args(&args).output().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{args:?} should refuse");
        assert!(stderr.contains(expected), "{args:?}: {stderr}");
    }
}

/// What jails generates for a real Boot 2 project compiles and passes, and
/// what cannot refuses by naming the type.
///
/// **This is the test `pending.md` §1.2 said the fix was waiting for, and it
/// contradicted the item.** §1.2 read the Boot floor as living in seven
/// generated *tests* written against `MockMvcTester`; the first real Boot
/// 2.7.18 compile said the tests were the smaller half. `add api` writes
/// `ProblemDetail` (Spring Framework 6), `add security` writes
/// `requestMatchers` (Spring Security 6), and `g query`/`g transition` write a
/// `JdbcClient` adapter (Framework 6.1) — all in the *main* source set, where
/// no test variant helps.
///
/// So the split is what compiles and what refuses, both asserted here:
/// `add cors`, `g enum`, `g scaffold` and `g usecase` are generated and run
/// through real `mvn test`; the other four must refuse and must say which type
/// is missing.
///
/// One project rather than four, which is also the arrangement that catches a
/// template compiling alone and colliding in company — two probe controllers
/// on one path, two classes with one name.
#[test]
fn what_jails_generates_for_boot_2_compiles_and_what_cannot_refuses_by_name() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-boot2-classic");
    write_spring2_fixture(&root);

    // The same shape as the `usecase-query-transition` golden scenario, which
    // is what these three generators need: an enum, a scaffold declaring the
    // components they reference, and a `version` for optimistic locking.
    for command in [
        vec!["add", "cors", "--no-start"],
        vec!["g", "enum", "NoteStatus", "DRAFT", "PUBLISHED"],
        vec![
            "g",
            "scaffold",
            "Note",
            "id:uuid@pk",
            "title:string!",
            "status:NoteStatus@index",
            "version:long@nonnegative",
        ],
        vec![
            "g",
            "usecase",
            "DraftNote",
            "id:uuid",
            "title:string!",
            "--on",
            "Note",
        ],
    ] {
        let output = jails_cmd_with_path(&root, &path)
            .args(&command)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{command:?}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // The four that cannot, and why the item's premise was incomplete: their
    // *main* source set is Boot 3 code, which no test variant can help. The
    // refusal names the type the compiler would have named.
    for (command, needs) in [
        (vec!["add", "api", "--no-start"], "ProblemDetail"),
        (vec!["add", "security", "--no-start"], "requestMatchers"),
        (
            vec![
                "g",
                "query",
                "NotesByStatus",
                "status:NoteStatus",
                "--on",
                "Note",
            ],
            "JdbcClient",
        ),
        (
            vec![
                "g",
                "transition",
                "ChangeNoteStatus",
                "id:uuid",
                "status:NoteStatus",
                "version:long@nonnegative",
                "--on",
                "Note",
            ],
            "JdbcClient",
        ),
    ] {
        let refused = jails_cmd_with_path(&root, &path)
            .args(&command)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&refused.stderr);
        assert!(!refused.status.success(), "{command:?} should refuse");
        assert!(stderr.contains(needs), "{command:?}: {stderr}");
        assert!(stderr.contains("Spring Boot 2"), "{command:?}: {stderr}");
        assert!(stderr.contains("jails g scaffold"), "{command:?}: {stderr}");
    }

    // Not one of them may name the Framework 6.2 entry point.
    let mut checked = 0;
    let mut stack = vec![root.join("src/test/java")];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let entry = entry.unwrap().path();
            if entry.is_dir() {
                stack.push(entry);
                continue;
            }
            let source = fs::read_to_string(&entry).unwrap();
            assert!(
                !source.contains("servlet.assertj.MockMvcTester"),
                "{}: MockMvcTester is Spring Framework 6.2, and this project is 5.3\n{source}",
                entry.display()
            );
            checked += 1;
        }
    }
    assert!(
        checked >= 4,
        "expected the generated tests plus the fixture's, found {checked}"
    );

    let output = std::process::Command::new("mvn")
        .current_dir(&root)
        .env("PATH", &path)
        .args(["-q", "-B", "test"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "mvn test failed on a Boot 2 project:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
