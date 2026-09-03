//! `rename resource` and `resource field|index|revive`: projection patches over
//! a live entity, each with exactly one typed policy.
//!
use super::*;

#[test]
fn jdl_v1_rename_materializes_identity_and_physical_pins_without_losing_edits() {
    let root = jdl_project(
        "jdl-v1-rename",
        r#"jdl 1
app Notes {
  pkg com.example.notes
  java 26
  platform spring
  build maven
  storage postgres
}

entity Task {
  // keep entity note
  id: uuid @pk
  title: string
}
"#,
    );
    write_spring_fixture(&root);
    apply_canonical_model(&root, "jdl-v1-rename-initial");
    let task = root.join("src/main/java/com/example/notes/domain/Task.java");
    let source = fs::read_to_string(&task).unwrap();
    let split = source.rfind("\n}").unwrap();
    fs::write(
        &task,
        format!(
            "{}\n\n    public String readerMethod() {{ return title; }}{}",
            &source[..split],
            &source[split..]
        ),
    )
    .unwrap();

    let renamed = jails_cmd(&root, None)
        .args([
            "rename",
            "resource",
            "Task",
            "WorkItem",
            "--strategy",
            "preserve-table",
        ])
        .output()
        .unwrap();
    assert!(
        renamed.status.success(),
        "{}",
        String::from_utf8_lossy(&renamed.stderr)
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(model.contains("entity WorkItem @id(ent_task) {"), "{model}");
    assert!(model.contains("table \"tasks\""), "{model}");
    assert!(model.contains("// keep entity note"), "{model}");
    let linked = jails_model::parse_jdl(&model).unwrap();
    let entity = linked.entities.values().next().unwrap();
    assert_eq!(entity.id.to_string(), "ent_task");
    assert_eq!(entity.label, "work_item");
    assert_eq!(entity.names.sql_table, "tasks");
    let moved = root.join("src/main/java/com/example/notes/domain/WorkItem.java");
    assert!(!task.exists());
    assert!(
        fs::read_to_string(&moved)
            .unwrap()
            .contains("readerMethod()"),
        "reader edit was lost during rename"
    );

    let field_renamed = jails_cmd(&root, None)
        .args([
            "resource", "field", "rename", "WorkItem", "title", "summary", "--column", "preserve",
        ])
        .output()
        .unwrap();
    assert!(
        field_renamed.status.success(),
        "{}",
        String::from_utf8_lossy(&field_renamed.stderr)
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(
        model.contains("summary: string @id(fld_ent_task_title) @map(title)"),
        "{model}"
    );
    let linked = jails_model::parse_jdl(&model).unwrap();
    let field = linked
        .entities
        .values()
        .next()
        .unwrap()
        .fields
        .iter()
        .find(|field| field.label == "summary")
        .unwrap();
    assert_eq!(field.names.sql_column, "title");
    assert!(
        fs::read_to_string(moved)
            .unwrap()
            .contains("readerMethod()")
    );
}

#[test]
fn jdl_rename_keeps_the_stable_identity_and_reader_edits() {
    let root = jdl_project("model-jdl-rename-preserve-edits", NOTES_JDL);
    let old = root.join("src/main/java/com/example/notes/domain/Task.java");
    let new = root.join("src/main/java/com/example/notes/domain/WorkItem.java");
    let generated = jails_cmd(&root, None)
        .args(["g", "record", "Task", "title:string!"])
        .output()
        .unwrap();
    assert!(generated.status.success());
    let source = fs::read_to_string(&old).unwrap();
    let split = source.rfind("\n}").unwrap();
    fs::write(
        &old,
        format!(
            "{}\n\n    public String handWritten() {{ return title; }}{}",
            &source[..split],
            &source[split..]
        ),
    )
    .unwrap();

    let renamed = jails_cmd(&root, None)
        .args([
            "rename",
            "resource",
            "Task",
            "WorkItem",
            "--strategy",
            "preserve-table",
            "--force",
        ])
        .output()
        .unwrap();
    assert!(
        renamed.status.success(),
        "{}",
        String::from_utf8_lossy(&renamed.stderr)
    );
    assert!(!old.exists());
    let source = fs::read_to_string(&new).unwrap();
    assert!(source.contains("public record WorkItem("), "{source}");
    assert!(source.contains("handWritten()"), "{source}");
    let jdl_source = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(
        jdl_source.contains("entity WorkItem @id(ent_task)"),
        "{jdl_source}"
    );
    // The table is pinned, and the label is not: the label is a projection
    // off the name, and the two things a rename must not move -- the stable
    // id and the SQL table -- are stated outright. `preserve-table` writes
    // the table, which is what the strategy is named for.
    assert!(jdl_source.contains(r#"table "tasks""#), "{jdl_source}");
    let model = jails_model::parse_jdl(&jdl_source).unwrap();
    let entity = model.entities.values().next().unwrap();
    assert_eq!(entity.id.to_string(), "ent_task");
    assert_eq!(entity.names.sql_table, "tasks");
    assert_eq!(entity.names.java_type, "WorkItem");
}

#[test]
fn canonical_preserve_table_rename_refuses_overlap_without_writes() {
    let root = model_project("model-rename-conflict", EMPTY_MODEL);
    let generated = root.join("src/main/java/com/example/notes/domain/Task.java");
    let first = jails_cmd(&root, None)
        .args(["g", "record", "Task", "title:string!"])
        .output()
        .unwrap();
    assert!(first.status.success());
    let source = fs::read_to_string(&generated).unwrap();
    fs::write(
        &generated,
        source.replace("public record Task(", "public record ManualTask("),
    )
    .unwrap();
    let before = snapshot_tree(&root);

    let renamed = jails_cmd(&root, None)
        .args([
            "rename",
            "resource",
            "Task",
            "WorkItem",
            "--strategy",
            "preserve-table",
            "--force",
        ])
        .output()
        .unwrap();
    assert!(!renamed.status.success());
    let stderr = String::from_utf8(renamed.stderr).unwrap();
    assert!(stderr.contains("overlapping edit"), "{stderr}");
    assert_eq!(snapshot_tree(&root), before);
}

#[test]
fn canonical_preserve_table_rename_refuses_a_destination_collision() {
    let root = model_project("model-rename-collision", EMPTY_MODEL);
    let generated = jails_cmd(&root, None)
        .args(["g", "record", "Task", "title:string!"])
        .output()
        .unwrap();
    assert!(generated.status.success());
    let destination = root.join("src/main/java/com/example/notes/domain/WorkItem.java");
    fs::write(&destination, "// reader-owned collision\n").unwrap();
    let before = snapshot_tree(&root);

    let renamed = jails_cmd(&root, None)
        .args([
            "rename",
            "resource",
            "Task",
            "WorkItem",
            "--strategy",
            "preserve-table",
            "--force",
        ])
        .output()
        .unwrap();
    assert!(!renamed.status.success());
    let stderr = String::from_utf8(renamed.stderr).unwrap();
    assert!(stderr.contains("already exists"), "{stderr}");
    assert_eq!(snapshot_tree(&root), before);
}

#[test]
fn canonical_preserve_table_rename_keeps_the_accepted_database_projection() {
    let root = canonical_database_project("model-db-preserve-table-rename");
    let migration = root.join("src/main/resources/db/migration/V001__create_notes.sql");
    let migration_before = fs::read(&migration).unwrap();
    let old = root.join("src/main/java/com/example/notes/domain/Note.java");
    let new = root.join("src/main/java/com/example/notes/domain/Memo.java");

    let renamed = jails_cmd(&root, None)
        .args([
            "rename",
            "resource",
            "Note",
            "Memo",
            "--strategy",
            "preserve-table",
            "--force",
        ])
        .output()
        .unwrap();
    assert!(
        renamed.status.success(),
        "{}",
        String::from_utf8_lossy(&renamed.stderr)
    );
    assert_eq!(fs::read(&migration).unwrap(), migration_before);
    assert!(
        !root
            .join("src/main/resources/db/migration/V002__rename_note.sql")
            .exists()
    );
    assert!(!old.exists());
    assert!(new.exists());
    let model = jails_model::parse_jdl(&fs::read_to_string(root.join(".jails/model.jdl")).unwrap())
        .unwrap();
    let entity = model.entities.values().next().unwrap();
    assert_eq!(entity.id.to_string(), "ent_note");
    assert_eq!(entity.names.java_type, "Memo");
    assert_eq!(entity.names.sql_table, "notes");
    let lock: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join(".jails/compiler.lock.json")).unwrap()).unwrap();
    assert_eq!(
        lock["model"]["entities"]["ent_note"]["names"]["sql_table"],
        "notes"
    );
    assert_eq!(
        lock["model"]["entities"]["ent_note"]["names"]["java_type"],
        "Memo"
    );
}

#[test]
fn canonical_database_and_safe_field_evolution_are_one_exact_compiler_path() {
    let root = model_project("model-db-evolution", EMPTY_MODEL);
    write_spring_fixture(&root);
    let scaffold = jails_cmd(&root, None)
        .args(["g", "scaffold", "Note", "id:uuid@pk", "title:string!"])
        .output()
        .unwrap();
    assert!(
        scaffold.status.success(),
        "{}",
        String::from_utf8_lossy(&scaffold.stderr)
    );

    let before_db = snapshot_tree(&root);
    let preview = jails_cmd(&root, None)
        .args(["add", "db", "--pretend", "--ast"])
        .output()
        .unwrap();
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    assert_eq!(snapshot_tree(&root), before_db, "db preview wrote files");
    let db = jails_cmd(&root, None)
        .args(["add", "db", "--no-start"])
        .output()
        .unwrap();
    assert!(
        db.status.success(),
        "{}",
        String::from_utf8_lossy(&db.stderr)
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(model.contains("storage postgres"), "{model}");
    let initial = root.join("src/main/resources/db/migration/V001__create_notes.sql");
    let initial_sql = fs::read_to_string(&initial).unwrap();
    assert!(initial_sql.contains("create table notes"), "{initial_sql}");
    assert!(
        initial_sql.contains("id uuid not null primary key"),
        "{initial_sql}"
    );
    assert!(root.join(".jails/compiler.lock.json").is_file());
    assert!(
        root.join("src/main/java/com/example/notes/adapters/jdbc/JdbcNoteRepository.java")
            .is_file()
    );
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    for artifact in [
        "spring-boot-starter-jdbc",
        "postgresql",
        "flyway-core",
        "flyway-database-postgresql",
    ] {
        assert!(pom.contains(artifact), "missing {artifact} in {pom}");
    }

    let before_refusal = snapshot_tree(&root);
    let refused = jails_cmd(&root, None)
        .args(["resource", "field", "add", "Note", "status:string!"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    let stderr = String::from_utf8(refused.stderr).unwrap();
    assert!(stderr.contains("needs a backfill"), "{stderr}");
    assert_eq!(
        snapshot_tree(&root),
        before_refusal,
        "required-field refusal mutated the project"
    );

    let before_preview = snapshot_tree(&root);
    let preview = jails_cmd(&root, None)
        .args([
            "resource",
            "field",
            "add",
            "Note",
            "summary:string?",
            "--pretend",
            "--diff",
        ])
        .output()
        .unwrap();
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    assert_eq!(
        snapshot_tree(&root),
        before_preview,
        "field preview wrote files"
    );
    let added = jails_cmd(&root, None)
        .args(["resource", "field", "add", "Note", "summary:string?"])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let second = fs::read_to_string(
        root.join("src/main/resources/db/migration/V002__add_summary_to_notes.sql"),
    )
    .unwrap();
    assert_eq!(
        second,
        "-- Generated by jails from the accepted semantic schema.\nalter table notes add column summary text;\n"
    );
    let record =
        fs::read_to_string(root.join("src/main/java/com/example/notes/domain/Note.java")).unwrap();
    assert!(record.contains("Optional<String> summary"), "{record}");

    let required = jails_cmd(&root, None)
        .args([
            "resource",
            "field",
            "add",
            "Note",
            "status:string!",
            "--default-literal",
            "new",
        ])
        .output()
        .unwrap();
    assert!(
        required.status.success(),
        "{}",
        String::from_utf8_lossy(&required.stderr)
    );
    let third = fs::read_to_string(
        root.join("src/main/resources/db/migration/V003__add_status_to_notes.sql"),
    )
    .unwrap();
    for statement in [
        "alter table notes add column status text;",
        "update notes set status = 'new' where status is null;",
        "alter table notes alter column status set not null;",
        "chk_notes_status_non_blank",
    ] {
        assert!(
            third.contains(statement),
            "missing `{statement}` in {third}"
        );
    }

    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(
        frozen.status.success(),
        "{}",
        String::from_utf8_lossy(&frozen.stderr)
    );
    if real_mvn_available() && real_java_supports_target_release() {
        let path = real_path_without_mvnd();
        common::assert_main_sources_compile(&root, &path, "canonical JDBC source");
        assert!(
            root.join("target/classes/com/example/notes/adapters/jdbc/JdbcNoteRepository.class")
                .is_file()
        );
    }
}

#[test]
fn canonical_field_evolution_preserves_hand_edits_and_lowers_explicit_sql_policies() {
    let root = canonical_database_project("model-db-field-evolution");
    let record = root.join("src/main/java/com/example/notes/domain/Note.java");
    let source = fs::read_to_string(&record).unwrap();
    let split = source.rfind("\n}").unwrap();
    fs::write(
        &record,
        format!(
            "{}\n\n    public String handWritten() {{ return \"reader\"; }}{}",
            &source[..split],
            &source[split..]
        ),
    )
    .unwrap();

    let preserved = jails_cmd(&root, None)
        .args([
            "resource", "field", "rename", "Note", "title", "headline", "--column", "preserve",
        ])
        .output()
        .unwrap();
    assert!(
        preserved.status.success(),
        "{}",
        String::from_utf8_lossy(&preserved.stderr)
    );
    assert!(
        !root
            .join("src/main/resources/db/migration/V002__evolve_title.sql")
            .exists(),
        "preserving the physical column must not emit SQL"
    );
    let source = fs::read_to_string(&record).unwrap();
    assert!(source.contains("String headline"), "{source}");
    assert!(source.contains("handWritten()"), "{source}");
    let lock: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join(".jails/compiler.lock.json")).unwrap()).unwrap();
    // `fields` is an ordered array, not a map: the lock keeps a record's
    // components in the order its entity declares them, so the field is found
    // by its stable id rather than by a key.
    let title = lock["model"]["entities"]["ent_note"]["fields"]
        .as_array()
        .expect("the lock keeps fields in declaration order")
        .iter()
        .find(|field| field["id"] == "fld_note_title")
        .expect("the preserved rename keeps the field's stable id");
    assert_eq!(title["names"]["sql_column"], "title");
    assert_eq!(title["names"]["java_member"], "headline");

    let cutover = jails_cmd(&root, None)
        .args([
            "resource",
            "field",
            "rename",
            "Note",
            "headline",
            "subject",
            "--column",
            "single-cutover",
        ])
        .output()
        .unwrap();
    assert!(
        cutover.status.success(),
        "{}",
        String::from_utf8_lossy(&cutover.stderr)
    );
    // Named for the change rather than for the fact that one happened: a
    // column relaxed and then made required again produces two migrations, and
    // `evolve_title` twice is a history nobody can read.
    let rename_sql = fs::read_to_string(
        root.join("src/main/resources/db/migration/V002__rename_title_to_subject.sql"),
    )
    .unwrap();
    assert!(
        rename_sql.contains("alter table notes rename column title to subject;"),
        "{rename_sql}"
    );
    let source = fs::read_to_string(&record).unwrap();
    assert!(source.contains("String subject"), "{source}");
    assert!(source.contains("handWritten()"), "{source}");

    let add_priority = jails_cmd(&root, None)
        .args(["g", "field", "Note", "priority:int?"])
        .output()
        .unwrap();
    assert!(
        add_priority.status.success(),
        "{}",
        String::from_utf8_lossy(&add_priority.stderr)
    );
    let widened = jails_cmd(&root, None)
        .args([
            "resource",
            "field",
            "type",
            "Note",
            "priority",
            "--to",
            "long",
            "--strategy",
            "safe",
        ])
        .output()
        .unwrap();
    assert!(
        widened.status.success(),
        "{}",
        String::from_utf8_lossy(&widened.stderr)
    );
    let type_sql =
        fs::read_to_string(root.join("src/main/resources/db/migration/V004__retype_priority.sql"))
            .unwrap();
    assert!(
        type_sql.contains("alter table notes alter column priority type bigint;"),
        "{type_sql}"
    );
    let before_unsafe = snapshot_tree(&root);
    let unsafe_change = jails_cmd(&root, None)
        .args([
            "resource",
            "field",
            "type",
            "Note",
            "priority",
            "--to",
            "string",
            "--strategy",
            "safe",
        ])
        .output()
        .unwrap();
    assert!(!unsafe_change.status.success());
    assert!(String::from_utf8_lossy(&unsafe_change.stderr).contains("not a proven safe widening"));
    assert_eq!(snapshot_tree(&root), before_unsafe);

    let added_status = jails_cmd(&root, None)
        .args(["g", "field", "Note", "status:string?"])
        .output()
        .unwrap();
    assert!(
        added_status.status.success(),
        "{}",
        String::from_utf8_lossy(&added_status.stderr)
    );
    fs::create_dir_all(root.join("backfills")).unwrap();
    fs::write(
        root.join("backfills/status.sql"),
        "update notes set status = 'new' where status is null;\n",
    )
    .unwrap();
    let required = jails_cmd(&root, None)
        .args([
            "resource",
            "field",
            "nullability",
            "Note",
            "status",
            "--required",
            "--backfill-file",
            "backfills/status.sql",
        ])
        .output()
        .unwrap();
    assert!(
        required.status.success(),
        "{}",
        String::from_utf8_lossy(&required.stderr)
    );
    let required_sql = fs::read_to_string(
        root.join("src/main/resources/db/migration/V006__make_status_required.sql"),
    )
    .unwrap();
    let update = required_sql.find("update notes set status").unwrap();
    let constraint = required_sql
        .find("alter column status set not null")
        .unwrap();
    assert!(update < constraint, "{required_sql}");

    let nullable = jails_cmd(&root, None)
        .args([
            "resource",
            "field",
            "nullability",
            "Note",
            "status",
            "--nullable",
        ])
        .output()
        .unwrap();
    assert!(
        nullable.status.success(),
        "{}",
        String::from_utf8_lossy(&nullable.stderr)
    );
    let nullable_sql = fs::read_to_string(
        root.join("src/main/resources/db/migration/V007__make_status_nullable.sql"),
    )
    .unwrap();
    assert!(nullable_sql.contains("alter column status drop not null"));

    let before_wrong_drop = snapshot_tree(&root);
    let wrong_drop = jails_cmd(&root, None)
        .args([
            "resource",
            "field",
            "drop",
            "Note",
            "priority",
            "--confirm-column",
            "wrong",
        ])
        .output()
        .unwrap();
    assert!(!wrong_drop.status.success());
    assert!(String::from_utf8_lossy(&wrong_drop.stderr).contains("does not match"));
    assert_eq!(snapshot_tree(&root), before_wrong_drop);

    let dropped = jails_cmd(&root, None)
        .args([
            "resource",
            "field",
            "drop",
            "Note",
            "priority",
            "--confirm-column",
            "priority",
        ])
        .output()
        .unwrap();
    assert!(
        dropped.status.success(),
        "{}",
        String::from_utf8_lossy(&dropped.stderr)
    );
    let drop_sql =
        fs::read_to_string(root.join("src/main/resources/db/migration/V008__drop_priority.sql"))
            .unwrap();
    assert!(drop_sql.contains("alter table notes drop column priority;"));
    let source = fs::read_to_string(&record).unwrap();
    assert!(!source.contains("priority"), "{source}");
    assert!(source.contains("handWritten()"), "{source}");

    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(
        frozen.status.success(),
        "{}",
        String::from_utf8_lossy(&frozen.stderr)
    );
}

#[test]
fn canonical_field_evolution_refuses_campaigns_and_referenced_field_drop_atomically() {
    let root = model_project("model-field-policy-refusals", EMPTY_MODEL);
    for arguments in [
        vec!["g", "record", "Note", "title:string!"],
        vec!["g", "query", "OpenNotes", "title", "--on", "Note"],
    ] {
        let output = jails_cmd(&root, None).args(arguments).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    for (arguments, expected) in [
        (
            vec![
                "resource", "field", "rename", "Note", "title", "headline", "--column", "rolling",
            ],
            "multi-release campaign",
        ),
        (
            vec![
                "resource",
                "field",
                "type",
                "Note",
                "title",
                "--to",
                "long",
                "--strategy",
                "expand-contract",
            ],
            "multi-release campaign",
        ),
        (
            vec![
                "resource",
                "field",
                "drop",
                "Note",
                "title",
                "--confirm-column",
                "title",
            ],
            "still referenced by operations",
        ),
    ] {
        let before = snapshot_tree(&root);
        let refused = jails_cmd(&root, None).args(arguments).output().unwrap();
        assert!(!refused.status.success());
        assert!(
            String::from_utf8_lossy(&refused.stderr).contains(expected),
            "{}",
            String::from_utf8_lossy(&refused.stderr)
        );
        assert_eq!(snapshot_tree(&root), before, "refusal mutated the project");
    }
}

#[test]
fn jdl_field_evolution_keeps_ids_edits_and_forward_schema_history() {
    let root = jdl_project("model-jdl-field-evolution", NOTES_JDL);
    write_spring_fixture(&root);
    for arguments in [
        vec!["g", "scaffold", "Note", "id:uuid@pk", "title:string!"],
        vec!["add", "db", "--no-start"],
    ] {
        let output = jails_cmd(&root, None).args(arguments).output().unwrap();
        assert!(output.status.success());
    }
    let record = root.join("src/main/java/com/example/notes/domain/Note.java");
    let source = fs::read_to_string(&record).unwrap();
    let split = source.rfind("\n}").unwrap();
    fs::write(
        &record,
        format!(
            "{}\n\n    public String handWritten() {{ return \"reader\"; }}{}",
            &source[..split],
            &source[split..]
        ),
    )
    .unwrap();

    let renamed = jails_cmd(&root, None)
        .args([
            "resource", "field", "rename", "Note", "title", "headline", "--column", "preserve",
        ])
        .output()
        .unwrap();
    assert!(
        renamed.status.success(),
        "{}",
        String::from_utf8_lossy(&renamed.stderr)
    );
    let renamed_jdl = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(
        renamed_jdl.contains("headline: string @id(fld_note_title) @notBlank @map(title)"),
        "{renamed_jdl}"
    );
    let renamed_model = jails_model::parse_jdl(&renamed_jdl).unwrap();
    let title = renamed_model
        .entities
        .values()
        .next()
        .unwrap()
        .fields
        .iter()
        .find(|field| field.id.to_string() == "fld_note_title")
        .unwrap();
    // The label follows the name; the identity and the column do not. v1
    // derives a field's label from what it is called, so a preserve-column
    // rename moves the label and pins the column with `@map(title)` -- which
    // is the whole point of the strategy.
    assert_eq!(title.label, "headline");
    assert_eq!(title.names.java_member, "headline");
    assert_eq!(title.names.sql_column, "title");

    let added = jails_cmd(&root, None)
        .args(["g", "field", "Note", "priority:int?"])
        .output()
        .unwrap();
    assert!(added.status.success());
    let widened = jails_cmd(&root, None)
        .args([
            "resource",
            "field",
            "type",
            "Note",
            "priority",
            "--to",
            "long",
            "--strategy",
            "safe",
        ])
        .output()
        .unwrap();
    assert!(
        widened.status.success(),
        "{}",
        String::from_utf8_lossy(&widened.stderr)
    );
    fs::create_dir_all(root.join("backfills")).unwrap();
    fs::write(
        root.join("backfills/priority.sql"),
        "update notes set priority = 0 where priority is null;\n",
    )
    .unwrap();
    let required = jails_cmd(&root, None)
        .args([
            "resource",
            "field",
            "nullability",
            "Note",
            "priority",
            "--required",
            "--backfill-file",
            "backfills/priority.sql",
        ])
        .output()
        .unwrap();
    assert!(
        required.status.success(),
        "{}",
        String::from_utf8_lossy(&required.stderr)
    );
    let evolved_jdl = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(evolved_jdl.contains("priority: long @id(fld_note_priority)"));

    let dropped = jails_cmd(&root, None)
        .args([
            "resource",
            "field",
            "drop",
            "Note",
            "priority",
            "--confirm-column",
            "priority",
        ])
        .output()
        .unwrap();
    assert!(
        dropped.status.success(),
        "{}",
        String::from_utf8_lossy(&dropped.stderr)
    );
    let final_jdl = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(!final_jdl.contains("priority:"), "{final_jdl}");
    let record_source = fs::read_to_string(&record).unwrap();
    assert!(record_source.contains("handWritten()"), "{record_source}");
    assert!(record_source.contains("String headline"), "{record_source}");
    assert!(!record_source.contains("priority"), "{record_source}");
    let migrations = fs::read_dir(root.join("src/main/resources/db/migration"))
        .unwrap()
        .map(|entry| fs::read_to_string(entry.unwrap().path()).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    for sql in [
        "alter column priority type bigint",
        "alter column priority set not null",
        "drop column priority",
    ] {
        assert!(migrations.contains(sql), "missing `{sql}`:\n{migrations}");
    }
    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(frozen.status.success());
}

#[test]
fn canonical_composite_index_is_model_data_and_one_forward_migration() {
    let root = canonical_database_project("model-db-composite-index");
    let record = root.join("src/main/java/com/example/notes/domain/Note.java");
    let source = fs::read_to_string(&record).unwrap();
    let split = source.rfind("\n}").unwrap();
    fs::write(
        &record,
        format!(
            "{}\n\n    public String handWritten() {{ return \"reader\"; }}{}",
            &source[..split],
            &source[split..]
        ),
    )
    .unwrap();

    let added = jails_cmd(&root, None)
        .args(["resource", "index", "add", "Note", "title, id desc"])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let migration_path = fs::read_dir(root.join("src/main/resources/db/migration"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("V002__add_idx_notes_title_id_desc")
        })
        .expect("canonical index migration");
    let migration = fs::read_to_string(migration_path).unwrap();
    assert!(
        migration.contains("create index idx_notes_title_id_desc"),
        "{migration}"
    );
    assert!(
        migration.contains(" on notes (title, id desc);"),
        "{migration}"
    );
    let source = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(
        source.contains("index [title, id desc] @id(idx_note_"),
        "{source}"
    );
    let model = jails_model::parse_jdl(&source).unwrap();
    let entity = model.entities.values().next().unwrap();
    let index = entity.indexes.values().next().unwrap();
    assert_eq!(index.columns.len(), 2);
    assert_eq!(index.columns[0].field.to_string(), "fld_note_title");
    assert_eq!(index.columns[1].field.to_string(), "fld_note_id");
    assert!(
        fs::read_to_string(&record)
            .unwrap()
            .contains("handWritten()")
    );

    for arguments in [
        vec!["resource", "index", "add", "Note", "title, id desc"],
        vec!["resource", "index", "add", "Note", "missing"],
        vec!["resource", "index", "add", "Note", "title nulls first"],
    ] {
        let before = snapshot_tree(&root);
        let refused = jails_cmd(&root, None).args(arguments).output().unwrap();
        assert!(!refused.status.success());
        assert_eq!(
            snapshot_tree(&root),
            before,
            "index refusal mutated project"
        );
    }
    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(
        frozen.status.success(),
        "{}",
        String::from_utf8_lossy(&frozen.stderr)
    );
}

#[test]
fn jdl_index_removal_is_forward_only_atomic_and_preserves_reader_edits() {
    let root = jdl_project("model-jdl-index-remove", NOTES_JDL);
    write_spring_fixture(&root);
    for arguments in [
        vec!["g", "scaffold", "Note", "id:uuid@pk", "title:string!"],
        vec!["add", "db", "--no-start"],
        vec!["resource", "index", "add", "Note", "title, id desc"],
    ] {
        let output = jails_cmd(&root, None).args(arguments).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let record = root.join("src/main/java/com/example/notes/domain/Note.java");
    let source = fs::read_to_string(&record).unwrap();
    let split = source.rfind('\n').unwrap();
    fs::write(
        &record,
        format!(
            "{}\n\n    public String handWritten() {{ return \"reader\"; }}{}",
            &source[..split],
            &source[split..]
        ),
    )
    .unwrap();

    let model_source = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    let model = jails_model::parse_jdl(&model_source).unwrap();
    let index = model
        .entities
        .values()
        .next()
        .unwrap()
        .indexes
        .values()
        .next()
        .unwrap();
    let index_id = index.id.to_string();
    let sql_name = index.sql_name.clone();

    let before_wrong_confirmation = snapshot_tree(&root);
    let wrong = jails_cmd(&root, None)
        .args([
            "resource",
            "index",
            "remove",
            "Note",
            "title, id desc",
            "--confirm-index",
            "wrong_index",
        ])
        .output()
        .unwrap();
    assert!(!wrong.status.success());
    assert!(
        String::from_utf8_lossy(&wrong.stderr).contains(&sql_name),
        "{}",
        String::from_utf8_lossy(&wrong.stderr)
    );
    assert_eq!(snapshot_tree(&root), before_wrong_confirmation);

    let directly_removed = model_source
        .split_inclusive('\n')
        .filter(|line| !line.contains(&index_id))
        .collect::<String>();
    fs::write(root.join(".jails/model.jdl"), directly_removed).unwrap();
    let before_direct_sync = snapshot_tree(&root);
    let direct = jails_cmd(&root, None).arg("sync").output().unwrap();
    assert!(!direct.status.success());
    assert!(
        String::from_utf8_lossy(&direct.stderr).contains("removed without a drop policy"),
        "{}",
        String::from_utf8_lossy(&direct.stderr)
    );
    assert_eq!(snapshot_tree(&root), before_direct_sync);
    fs::write(root.join(".jails/model.jdl"), &model_source).unwrap();

    let removed = jails_cmd(&root, None)
        .args([
            "resource",
            "index",
            "remove",
            "Note",
            "title, id desc",
            "--confirm-index",
            &sql_name,
        ])
        .output()
        .unwrap();
    assert!(
        removed.status.success(),
        "{}",
        String::from_utf8_lossy(&removed.stderr)
    );
    let final_jdl = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(!final_jdl.contains(&index_id), "{final_jdl}");
    assert!(
        jails_model::parse_jdl(&final_jdl)
            .unwrap()
            .entities
            .values()
            .next()
            .unwrap()
            .indexes
            .is_empty()
    );
    assert!(
        fs::read_to_string(&record)
            .unwrap()
            .contains("handWritten()")
    );

    let migrations = fs::read_dir(root.join("src/main/resources/db/migration"))
        .unwrap()
        .map(|entry| fs::read_to_string(entry.unwrap().path()).unwrap())
        .collect::<Vec<_>>();
    assert!(
        migrations
            .iter()
            .any(|migration| migration.contains(&format!("create index {sql_name}")))
    );
    assert!(
        migrations
            .iter()
            .any(|migration| migration.contains(&format!("drop index {sql_name};")))
    );

    let before_missing = snapshot_tree(&root);
    let missing = jails_cmd(&root, None)
        .args([
            "resource",
            "index",
            "remove",
            "Note",
            "title, id desc",
            "--confirm-index",
            &sql_name,
        ])
        .output()
        .unwrap();
    assert!(!missing.status.success());
    assert_eq!(snapshot_tree(&root), before_missing);

    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(
        frozen.status.success(),
        "{}",
        String::from_utf8_lossy(&frozen.stderr)
    );
}

#[test]
fn canonical_storage_preserve_removes_projections_and_revive_reuses_the_table() {
    let root = canonical_database_project("model-db-preserve-revive");
    let initial = root.join("src/main/resources/db/migration/V001__create_notes.sql");
    let initial_bytes = fs::read(&initial).unwrap();
    let record = root.join("src/main/java/com/example/notes/domain/Note.java");

    let retired = jails_cmd(&root, None)
        .args(["d", "scaffold", "Note", "--storage", "preserve", "--force"])
        .output()
        .unwrap();
    assert!(
        retired.status.success(),
        "{}",
        String::from_utf8_lossy(&retired.stderr)
    );
    assert!(!record.exists());
    assert_eq!(fs::read(&initial).unwrap(), initial_bytes);
    assert!(
        !root
            .join("src/main/resources/db/migration/V002__drop_notes.sql")
            .exists()
    );
    let retired_source = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(
        retired_source.contains("entity Note @id(ent_note) @retired {"),
        "{retired_source}"
    );
    let model = jails_model::parse_jdl(&retired_source).unwrap();
    let entity = model.entities.values().next().unwrap();
    assert!(!entity.active);
    assert_eq!(entity.names.sql_table, "notes");
    assert_eq!(entity.fields.len(), 2);

    for mutation in [
        ["resource", "field", "add", "Note", "body:string?"].as_slice(),
        ["resource", "index", "add", "Note", "title"].as_slice(),
    ] {
        let before_mutation = snapshot_tree(&root);
        let rejected = jails_cmd(&root, None).args(mutation).output().unwrap();
        assert!(!rejected.status.success());
        assert!(
            String::from_utf8_lossy(&rejected.stderr).contains("is retired"),
            "{}",
            String::from_utf8_lossy(&rejected.stderr)
        );
        assert_eq!(snapshot_tree(&root), before_mutation);
    }

    let before_wrong = snapshot_tree(&root);
    let wrong = jails_cmd(&root, None)
        .args(["resource", "revive", "Note", "--table", "wrong"])
        .output()
        .unwrap();
    assert!(!wrong.status.success());
    assert_eq!(snapshot_tree(&root), before_wrong);

    let revived = jails_cmd(&root, None)
        .args(["resource", "revive", "Note", "--table", "notes"])
        .output()
        .unwrap();
    assert!(
        revived.status.success(),
        "{}",
        String::from_utf8_lossy(&revived.stderr)
    );
    assert!(record.exists());
    assert_eq!(fs::read(&initial).unwrap(), initial_bytes);
    // Migrations, not directory entries: `add db` puts a `.gitkeep` beside
    // them so an empty history survives a clone.
    assert_eq!(
        fs::read_dir(root.join("src/main/resources/db/migration"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "sql"))
            .count(),
        1,
        "revive must not recreate preserved storage"
    );
    let active_source = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(
        active_source.contains("entity Note @id(ent_note) {"),
        "{active_source}"
    );
    assert!(!active_source.contains("@retired"), "{active_source}");
    let model = jails_model::parse_jdl(&active_source).unwrap();
    assert!(model.entities.values().next().unwrap().active);
    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(frozen.status.success());
}

#[test]
fn canonical_storage_drop_needs_exact_confirmation_and_appends_one_drop_table() {
    let root = canonical_database_project("model-db-drop-table");
    for arguments in [
        vec!["d", "scaffold", "Note"],
        vec!["d", "scaffold", "Note", "--storage", "drop"],
        vec![
            "d",
            "scaffold",
            "Note",
            "--storage",
            "drop",
            "--confirm-table",
            "wrong",
        ],
    ] {
        let before = snapshot_tree(&root);
        let refused = jails_cmd(&root, None).args(arguments).output().unwrap();
        assert!(!refused.status.success());
        assert_eq!(snapshot_tree(&root), before, "drop refusal mutated project");
    }

    let dropped = jails_cmd(&root, None)
        .args([
            "d",
            "scaffold",
            "Note",
            "--storage",
            "drop",
            "--confirm-table",
            "notes",
            "--force",
        ])
        .output()
        .unwrap();
    assert!(
        dropped.status.success(),
        "{}",
        String::from_utf8_lossy(&dropped.stderr)
    );
    let migration =
        fs::read_to_string(root.join("src/main/resources/db/migration/V002__drop_notes.sql"))
            .unwrap();
    assert_eq!(
        migration,
        "-- Generated by jails from the accepted semantic schema.\ndrop table notes;\n"
    );
    let model = jails_model::parse_jdl(&fs::read_to_string(root.join(".jails/model.jdl")).unwrap())
        .unwrap();
    assert!(model.entities.is_empty());
    assert!(
        !root
            .join("src/main/java/com/example/notes/domain/Note.java")
            .exists()
    );
    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(frozen.status.success());
}
