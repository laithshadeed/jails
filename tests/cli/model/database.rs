//! The storage axis: migrations, the SQL projection, and the adapters a stored
//! entity gets.
//!
use super::*;

#[test]
fn canonical_database_query_keeps_the_iterative_loop_and_ejects_only_its_adapter() {
    let root = jdl_project("model-db-query-loop", NOTES_JDL);
    write_spring_fixture(&root);
    for arguments in [
        vec![
            "g",
            "scaffold",
            "Note",
            "id:uuid@pk",
            "title:string!",
            "status:string?",
        ],
        vec![
            "g",
            "query",
            "OpenNotes",
            "title",
            "status",
            "--on",
            "Note",
            "--order-by",
            "title",
            "--limit",
            "25",
        ],
        vec!["add", "db", "--no-start"],
    ] {
        let output = jails_cmd(&root, None).args(arguments).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let managed = root.join("src/main/java/com/example/notes");
    let adapter = managed.join("adapters/jdbc/JdbcOpenNotesQuery.java");
    let abi = managed.join("application/queries/OpenNotesQuery.java");
    let original = fs::read_to_string(&adapter).unwrap();
    // The column list is declaration order, not alphabetical. One column list
    // feeds the DDL, the select, the insert and the row mapper, so this is the
    // same property the record's positional constructor is.
    for contract in [
        "implements OpenNotesQuery",
        "select id, title, status from notes",
        "new ArrayList<String>(List.of(\"title = :title\"))",
        "if (input.status().isPresent())",
        "predicates.add(\"status = :status\")",
        "order by title",
        "limit 25",
    ] {
        assert!(
            original.contains(contract),
            "missing `{contract}`:\n{original}"
        );
    }
    assert!(abi.is_file(), "query ABI was not generated");

    let split = original.rfind("\n}").unwrap();
    let hand_edited = format!(
        "{}\n\n    public String handWritten() {{ return \"reader\"; }}{}",
        &original[..split],
        &original[split..]
    );
    fs::write(&adapter, &hand_edited).unwrap();
    let evolved = jails_cmd(&root, None)
        .args(["resource", "field", "add", "Note", "summary:string?"])
        .output()
        .unwrap();
    assert!(
        evolved.status.success(),
        "{}",
        String::from_utf8_lossy(&evolved.stderr)
    );
    let evolved_source = fs::read_to_string(&adapter).unwrap();
    assert!(evolved_source.contains("handWritten()"), "{evolved_source}");
    assert!(
        evolved_source.contains("select id, title, status, summary from notes"),
        "{evolved_source}"
    );

    let stable = snapshot_tree(&root);
    let synced = jails_cmd(&root, None).args(["sync"]).output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );
    assert_eq!(snapshot_tree(&root), stable, "query rerun changed bytes");

    let edited_adapter = evolved_source.replace(
        "select id, title, status, summary from notes",
        "select id, title, status, summary from notes /* reader owns this line */",
    );
    assert_ne!(
        edited_adapter, evolved_source,
        "the reader edit matched nothing, so the overlap below would not exist: {evolved_source}"
    );
    fs::write(&adapter, edited_adapter).unwrap();
    let before_overlap = snapshot_tree(&root);
    let refused = jails_cmd(&root, None)
        .args(["resource", "field", "add", "Note", "priority:int?"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("overlap"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert_eq!(
        snapshot_tree(&root),
        before_overlap,
        "query overlap refusal wrote bytes"
    );

    fs::write(&adapter, &evolved_source).unwrap();
    let retried = jails_cmd(&root, None)
        .args(["resource", "field", "add", "Note", "priority:int?"])
        .output()
        .unwrap();
    assert!(
        retried.status.success(),
        "{}",
        String::from_utf8_lossy(&retried.stderr)
    );
    let before_ejection = fs::read(&adapter).unwrap();
    let before_ejection_source = String::from_utf8_lossy(&before_ejection).to_string();
    assert!(
        before_ejection_source.contains("select id, title, status, summary, priority from notes"),
        "{before_ejection_source}"
    );

    let ejected = jails_cmd(&root, None)
        .args(["model", "eject", "art_cap_db_op_open_notes_query"])
        .output()
        .unwrap();
    assert!(
        ejected.status.success(),
        "{}",
        String::from_utf8_lossy(&ejected.stderr)
    );
    let reader = root.join("src/main/java/com/example/notes/adapters/jdbc/JdbcOpenNotesQuery.java");
    assert_eq!(fs::read(&reader).unwrap(), before_ejection);
    assert!(abi.is_file(), "ejecting an adapter removed its managed ABI");

    let reader_bytes = fs::read(&reader).unwrap();
    let evolved_after_ejection = jails_cmd(&root, None)
        .args(["resource", "field", "add", "Note", "archived:boolean?"])
        .output()
        .unwrap();
    assert!(
        evolved_after_ejection.status.success(),
        "{}",
        String::from_utf8_lossy(&evolved_after_ejection.stderr)
    );
    assert_eq!(
        fs::read(&reader).unwrap(),
        reader_bytes,
        "generation rewrote the ejected query adapter"
    );
    assert!(
        abi.is_file(),
        "managed ABI disappeared after model evolution"
    );

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
        common::assert_main_sources_compile(&root, &path, "canonical JDBC query");
    }
}

#[test]
fn canonical_database_commands_and_transitions_are_independent_iterative_boundaries() {
    let root = jdl_project("model-db-write-operation-loop", NOTES_JDL);
    write_spring_fixture(&root);
    for arguments in [
        vec![
            "g",
            "scaffold",
            "Note",
            "id:uuid@pk",
            "title:string!",
            "status:string?",
        ],
        vec!["g", "event", "NoteRenamed", "id", "title", "--on", "Note"],
        vec!["g", "usecase", "CreateNote", "title", "--on", "Note"],
        vec![
            "g",
            "transition",
            "RenameNote",
            "title",
            "--on",
            "Note",
            "--yields",
            "NoteRenamed",
        ],
        vec!["add", "db", "--no-start"],
    ] {
        let output = jails_cmd(&root, None).args(arguments).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let managed = root.join("src/main/java/com/example/notes");
    let command = managed.join("adapters/jdbc/JdbcCreateNoteCommand.java");
    let transition = managed.join("adapters/jdbc/JdbcRenameNoteTransition.java");
    let command_abi = managed.join("application/commands/CreateNoteCommand.java");
    let transition_abi = managed.join("application/transitions/RenameNoteTransition.java");
    let command_source = fs::read_to_string(&command).unwrap();
    for contract in [
        "implements CreateNoteCommand",
        "insert into notes (id, title, status) values (:id, :title, :status)",
        "TimeOrderedUuid.next()",
        "statement.query(Note.class).single()",
    ] {
        assert!(
            command_source.contains(contract),
            "missing `{contract}`:\n{command_source}"
        );
    }
    let transition_source = fs::read_to_string(&transition).unwrap();
    for contract in [
        "implements RenameNoteTransition",
        "update notes set title = :title where",
        "id = :id",
        "@Transactional",
        "events.publishEvent(new NoteRenamedEvent(result.id(), result.title()))",
    ] {
        assert!(
            transition_source.contains(contract),
            "missing `{contract}`:\n{transition_source}"
        );
    }

    let split = command_source.rfind("\n}").unwrap();
    fs::write(
        &command,
        format!(
            "{}\n\n    public String readerCommandMethod() {{ return \"reader\"; }}{}",
            &command_source[..split],
            &command_source[split..]
        ),
    )
    .unwrap();
    let split = transition_source.rfind("\n}").unwrap();
    fs::write(
        &transition,
        format!(
            "{}\n\n    public String readerTransitionMethod() {{ return \"reader\"; }}{}",
            &transition_source[..split],
            &transition_source[split..]
        ),
    )
    .unwrap();

    let evolved = jails_cmd(&root, None)
        .args(["resource", "field", "add", "Note", "summary:string?"])
        .output()
        .unwrap();
    assert!(
        evolved.status.success(),
        "{}",
        String::from_utf8_lossy(&evolved.stderr)
    );
    let evolved_command = fs::read_to_string(&command).unwrap();
    let evolved_transition = fs::read_to_string(&transition).unwrap();
    assert!(evolved_command.contains("readerCommandMethod()"));
    assert!(evolved_transition.contains("readerTransitionMethod()"));
    assert!(
        evolved_command.contains(
            "insert into notes (id, title, status, summary) values (:id, :title, :status, :summary)"
        ),
        "{evolved_command}"
    );
    assert!(
        evolved_transition.contains("returning id, title, status, summary"),
        "{evolved_transition}"
    );

    let edited_command = evolved_command.replace(
        "insert into notes (id, title, status, summary)",
        "insert into notes /* reader owns this SQL */ (id, title, status, summary)",
    );
    assert_ne!(
        edited_command, evolved_command,
        "the reader edit matched nothing, so the overlap below would not exist: {evolved_command}"
    );
    fs::write(&command, edited_command).unwrap();
    let before_overlap = snapshot_tree(&root);
    let refused = jails_cmd(&root, None)
        .args(["resource", "field", "add", "Note", "priority:int?"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("overlap"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert_eq!(
        snapshot_tree(&root),
        before_overlap,
        "one command overlap partially rewrote the transition or model"
    );

    fs::write(&command, &evolved_command).unwrap();
    let retried = jails_cmd(&root, None)
        .args(["resource", "field", "add", "Note", "priority:int?"])
        .output()
        .unwrap();
    assert!(
        retried.status.success(),
        "{}",
        String::from_utf8_lossy(&retried.stderr)
    );
    let stable = snapshot_tree(&root);
    let synced = jails_cmd(&root, None).args(["sync"]).output().unwrap();
    assert!(synced.status.success());
    assert_eq!(
        snapshot_tree(&root),
        stable,
        "write-operation sync changed bytes"
    );

    let ejected_command = jails_cmd(&root, None)
        .args(["model", "eject", "art_cap_db_op_create_note_command"])
        .output()
        .unwrap();
    assert!(
        ejected_command.status.success(),
        "{}",
        String::from_utf8_lossy(&ejected_command.stderr)
    );
    let reader_command = common::generated(
        &root,
        "src/main/java/com/example/notes/adapters/jdbc/JdbcCreateNoteCommand.java",
    );
    assert!(
        transition.exists(),
        "command ejection moved the transition too"
    );
    assert!(command_abi.exists());
    assert!(transition_abi.exists());
    let reader_command_bytes = fs::read(&reader_command).unwrap();

    let evolved_after_command_ejection = jails_cmd(&root, None)
        .args(["resource", "field", "add", "Note", "category:string?"])
        .output()
        .unwrap();
    assert!(
        evolved_after_command_ejection.status.success(),
        "{}",
        String::from_utf8_lossy(&evolved_after_command_ejection.stderr)
    );
    assert_eq!(fs::read(&reader_command).unwrap(), reader_command_bytes);
    let transition_before_ejection = fs::read(&transition).unwrap();
    let transition_before_ejection_source =
        String::from_utf8_lossy(&transition_before_ejection).to_string();
    assert!(
        transition_before_ejection_source
            .contains("returning id, title, status, summary, priority, category"),
        "{transition_before_ejection_source}"
    );

    let ejected_transition = jails_cmd(&root, None)
        .args(["model", "eject", "art_cap_db_op_rename_note_transition"])
        .output()
        .unwrap();
    assert!(
        ejected_transition.status.success(),
        "{}",
        String::from_utf8_lossy(&ejected_transition.stderr)
    );
    let reader_transition = common::generated(
        &root,
        "src/main/java/com/example/notes/adapters/jdbc/JdbcRenameNoteTransition.java",
    );
    assert_eq!(
        fs::read(&reader_transition).unwrap(),
        transition_before_ejection
    );
    let reader_transition_bytes = fs::read(&reader_transition).unwrap();

    let evolved_after_both_ejections = jails_cmd(&root, None)
        .args(["resource", "field", "add", "Note", "tag:string?"])
        .output()
        .unwrap();
    assert!(
        evolved_after_both_ejections.status.success(),
        "{}",
        String::from_utf8_lossy(&evolved_after_both_ejections.stderr)
    );
    assert_eq!(fs::read(&reader_command).unwrap(), reader_command_bytes);
    assert_eq!(
        fs::read(&reader_transition).unwrap(),
        reader_transition_bytes
    );
    assert!(command_abi.exists());
    assert!(transition_abi.exists());

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
        common::assert_main_sources_compile(&root, &path, "canonical write operations");
    }
}

/// `g migration` allocates a file and writes no SQL into it.
///
/// The one generator that is deliberately not a model declaration. JDL v1
/// §2.1 puts ordered migration files outside JDL -- "immutable,
/// append-only history" -- §12.6 says authors never name managed migrations in
/// JDL, and §2 lists writing one among the *non-model* actions a familiar
/// command may map to. So what this pins is that the model is untouched and
/// the plan carries the file anyway: it is an ordinary `AppendMigration`, with
/// its version allocated from the observed history, not a side effect.
#[test]
fn canonical_migration_allocates_a_file_without_declaring_anything() {
    let root = temp_dir("canonical-migration");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    let source = "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  \
         platform spring\n  build maven\n  storage postgres\n}\n\n\
         entity Note @id(ent_note) {\n  use repo\n  id: uuid @id(fld_note_id) @pk\n\
         }\n";
    fs::write(root.join(".jails/model.jdl"), source).unwrap();
    let synced = jails_cmd(&root, None).arg("sync").output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );
    let before = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();

    let output = jails_cmd(&root, None)
        .args(["g", "migration", "add_note_archived_at"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Nothing was declared: history is not desired state.
    assert_eq!(
        before,
        fs::read_to_string(root.join(".jails/model.jdl")).unwrap()
    );

    // The version comes after the one `use repo` already allocated.
    let migration = root.join("src/main/resources/db/migration/V002__add_note_archived_at.sql");
    let body = fs::read_to_string(&migration).unwrap();
    assert_eq!(
        body,
        "-- Forward-only migration. Write explicit SQL below.\n"
    );

    // A reader's edit survives the next sync: an authored migration is not
    // rendered from the model, so nothing recomputes it.
    fs::write(
        &migration,
        "alter table notes add column archived_at timestamptz;\n",
    )
    .unwrap();
    let resynced = jails_cmd(&root, None).arg("sync").output().unwrap();
    assert!(
        resynced.status.success(),
        "{}",
        String::from_utf8_lossy(&resynced.stderr)
    );
    assert_eq!(
        fs::read_to_string(&migration).unwrap(),
        "alter table notes add column archived_at timestamptz;\n"
    );

    // A readable name is normalised into the Flyway description, and one that
    // cannot be is refused *here* -- rather than shown back as a
    // compiler-produced-invalid message about a name the reader chose.
    let normalised = jails_cmd(&root, None)
        .args(["g", "migration", "Add Note Body"])
        .output()
        .unwrap();
    assert!(
        normalised.status.success(),
        "{}",
        String::from_utf8_lossy(&normalised.stderr)
    );
    assert!(
        root.join("src/main/resources/db/migration/V003__add_note_body.sql")
            .is_file()
    );
    let refused = jails_cmd(&root, None)
        .args(["g", "migration", "add/note"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
}

#[test]
fn canonical_migration_plan_refuses_a_concurrent_history_append_without_writes() {
    let root = canonical_database_project("model-db-migration-stale");
    let plan = root.join("field-plan.json");
    let planned = jails_cmd(&root, None)
        .args([
            "resource",
            "field",
            "add",
            "Note",
            "summary:string?",
            "--plan-out",
        ])
        .arg(&plan)
        .output()
        .unwrap();
    assert!(
        planned.status.success(),
        "{}",
        String::from_utf8_lossy(&planned.stderr)
    );
    let bundle: jails_contracts::PlanBundle =
        serde_json::from_slice(&fs::read(&plan).unwrap()).unwrap();
    assert!(bundle.plan.operations.iter().any(|operation| matches!(
        operation,
        jails_contracts::PlannedOperation::AppendMigration { path, .. }
            if path.as_str().ends_with("V002__add_summary_to_notes.sql")
    )));
    assert!(bundle.plan.operations.iter().any(|operation| matches!(
        operation,
        jails_contracts::PlannedOperation::ReplaceStateFile { path, .. }
            if path.as_str() == ".jails/compiler.lock.json"
    )));
    let model_before = fs::read(root.join(".jails/model.jdl")).unwrap();
    let generated_before =
        fs::read(root.join("src/main/java/com/example/notes/domain/Note.java")).unwrap();
    fs::write(
        root.join("src/main/resources/db/migration/V002__reader_change.sql"),
        "select 1;\n",
    )
    .unwrap();
    let applied = jails_cmd(&root, None)
        .args(["model", "apply", "--bundle"])
        .arg(&plan)
        .output()
        .unwrap();
    assert!(!applied.status.success());
    let stderr = String::from_utf8(applied.stderr).unwrap();
    assert!(stderr.contains("stale exact plan"), "{stderr}");
    assert!(stderr.contains("directory"), "{stderr}");
    assert_eq!(
        fs::read(root.join(".jails/model.jdl")).unwrap(),
        model_before
    );
    assert_eq!(
        fs::read(root.join("src/main/java/com/example/notes/domain/Note.java")).unwrap(),
        generated_before
    );
    assert!(
        !root
            .join("src/main/resources/db/migration/V002__add_summary_to_notes.sql")
            .exists()
    );
}

#[test]
fn canonical_reader_sql_is_exact_plan_input_and_stale_changes_refuse_all_writes() {
    let root = canonical_database_project("model-db-reader-backfill-stale");
    fs::create_dir_all(root.join("backfills")).unwrap();
    let backfill = root.join("backfills/status.sql");
    fs::write(
        &backfill,
        "update notes set status = 'new' where status is null;\n",
    )
    .unwrap();
    let plan = root.join("field-plan.json");
    let planned = jails_cmd(&root, None)
        .args([
            "resource",
            "field",
            "add",
            "Note",
            "status:string!",
            "--backfill-file",
            "backfills/status.sql",
            "--plan-out",
        ])
        .arg(&plan)
        .output()
        .unwrap();
    assert!(
        planned.status.success(),
        "{}",
        String::from_utf8_lossy(&planned.stderr)
    );
    let bundle: jails_contracts::PlanBundle =
        serde_json::from_slice(&fs::read(&plan).unwrap()).unwrap();
    assert!(
        String::from_utf8_lossy(&bundle.plan.input.bytes).contains("reader-owned-sql"),
        "reader SQL policy was not represented in canonical input"
    );
    let model_before = fs::read(root.join(".jails/model.jdl")).unwrap();
    let record = root.join("src/main/java/com/example/notes/domain/Note.java");
    let record_before = fs::read(&record).unwrap();
    fs::write(
        &backfill,
        "update notes set status = 'ready' where status is null;\n",
    )
    .unwrap();
    let stale = jails_cmd(&root, None)
        .args(["model", "apply", "--bundle"])
        .arg(&plan)
        .output()
        .unwrap();
    assert!(!stale.status.success());
    assert!(
        String::from_utf8_lossy(&stale.stderr).contains("stale exact plan"),
        "{}",
        String::from_utf8_lossy(&stale.stderr)
    );
    assert_eq!(
        fs::read(root.join(".jails/model.jdl")).unwrap(),
        model_before
    );
    assert_eq!(fs::read(&record).unwrap(), record_before);
    assert!(
        !root
            .join("src/main/resources/db/migration/V002__add_status_to_notes.sql")
            .exists()
    );

    let applied = jails_cmd(&root, None)
        .args([
            "resource",
            "field",
            "add",
            "Note",
            "status:string!",
            "--backfill-file",
            "backfills/status.sql",
        ])
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let migration = fs::read_to_string(
        root.join("src/main/resources/db/migration/V002__add_status_to_notes.sql"),
    )
    .unwrap();
    let update = migration.find("update notes set status = 'ready'").unwrap();
    let constraint = migration.find("alter column status set not null").unwrap();
    assert!(update < constraint, "{migration}");
}

/// A command's and a transition's JDBC adapter each run against a real
/// database.
///
/// A command's `insert ... returning` and a transition's `update ...
/// returning` must pass every parameter through `bound_value`: a bind the
/// driver will not take compiles, and an enum reaching PostgreSQL raw fails
/// with `Can't infer the SQL type to use for an instance of Shelf`, naming
/// neither the column nor the statement.
#[test]
fn canonical_write_adapters_run_against_real_postgres() {
    if !real_mvn_available() || !real_java_supports_target_release() {
        common::skip("real Maven and a JDK that accepts TARGET_RELEASE");
        return;
    }
    if !real_docker_available() {
        common::skip("a running Docker-compatible container runtime is required");
        return;
    }
    let root = jdl_project(
        "jdl-v1-write-adapter-it",
        r#"jdl 1
app Demo {
  pkg com.example.demo
  java 26
  platform spring
  build maven
  storage postgres
}
"#,
    );
    write_spring_fixture(&root);
    for arguments in [
        vec!["add", "db", "--no-start"],
        vec!["g", "enum", "Shelf", "OPEN", "ARCHIVED"],
        vec![
            "g",
            "scaffold",
            "Note",
            "id:long@pk",
            "title:string!",
            "shelf:Shelf",
            "archived:boolean@default(false)",
        ],
        vec![
            "g",
            "usecase",
            "PublishNote",
            "title:string!",
            "shelf:Shelf",
            "--on",
            "Note",
        ],
        vec![
            "g",
            "transition",
            "ArchiveNote",
            "id:long",
            "archived:boolean",
            "--on",
            "Note",
        ],
    ] {
        let output = jails_cmd(&root, None).args(&arguments).output().unwrap();
        assert!(
            output.status.success(),
            "{arguments:?}:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let jdbc = root.join("src/main/java/com/example/demo/adapters/jdbc");
    // An enum reaches a `text` column as its constant name. Bound raw, pgjdbc
    // refuses it at run time.
    let command = fs::read_to_string(jdbc.join("JdbcPublishNoteCommand.java")).unwrap();
    assert!(command.contains("input.shelf().name()"), "{command}");

    let tests = root.join("src/test/java/com/example/demo/adapters/jdbc");
    for name in [
        "JdbcPublishNoteCommandIT.java",
        "JdbcArchiveNoteTransitionIT.java",
    ] {
        assert!(tests.join(name).is_file(), "{name} was not emitted");
    }

    let verified = real_maven_cmd(&root, &real_path_without_mvnd())
        .args(["-q", "-B", "verify"])
        .output()
        .unwrap();
    assert!(
        verified.status.success(),
        "the generated write-adapter integration tests failed real Maven:\n{}\n{}",
        String::from_utf8_lossy(&verified.stdout),
        String::from_utf8_lossy(&verified.stderr)
    );
    // Three, not two: the repository adapter every scaffold emits proves its
    // own round trip beside the two operations.
    assert_eq!(
        maven_report_summary(&root, "failsafe-reports"),
        MavenReportSummary {
            reports: 3,
            tests: 3,
            failures: 0,
            errors: 0,
            skipped: 0,
        }
    );
}
