//! `add` and `remove`: every capability pack, its merge boundary and what
//! removal takes back.
//!
use super::*;

#[test]
fn canonical_fast_test_is_model_owned_and_never_journaled() {
    let root = model_project("model-fast-test", MODEL);
    write_spring_fixture(&root);
    apply_canonical_model(&root, "initial-fast-test");
    let fake_dir = temp_dir("model-fast-test-bin");
    let log = fake_dir.join("maven.log");
    write_fake_maven(&fake_dir, &["mvn"], &log);

    let installed = jails_cmd(&root, Some(&fake_dir))
        .args(["test", "NoteTest", "--fast", "--explain-selection"])
        .output()
        .unwrap();
    assert!(
        installed.status.success(),
        "{}",
        String::from_utf8_lossy(&installed.stderr)
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(
        model.contains("cap fast-test @id(cap_fast_test)"),
        "{model}"
    );
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("junit-platform-console"), "{pom}");
    for legacy in ["objects", "receipts", "journal", "state"] {
        assert!(!root.join(".jails").join(legacy).exists());
    }

    let removed = jails_cmd(&root, Some(&fake_dir))
        .args(["remove", "fast-test", "--force"])
        .output()
        .unwrap();
    assert!(
        removed.status.success(),
        "{}",
        String::from_utf8_lossy(&removed.stderr)
    );
    assert!(
        !fs::read_to_string(root.join(".jails/model.jdl"))
            .unwrap()
            .contains("fast-test")
    );
    assert!(
        !fs::read_to_string(root.join("pom.xml"))
            .unwrap()
            .contains("junit-platform-console")
    );
}

#[test]
fn fake_capability_is_a_global_compiler_profile_and_remove_is_recompilation() {
    let root = model_project("model-capability-fake", EMPTY_MODEL);
    let scaffold = jails_cmd(&root, None)
        .args(["g", "scaffold", "Note", "id:uuid@pk", "title:string!"])
        .output()
        .unwrap();
    assert!(scaffold.status.success());
    let before = snapshot_tree(&root);
    let preview = jails_cmd(&root, None)
        .args(["add", "fake", "--pretend"])
        .output()
        .unwrap();
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    assert_eq!(
        snapshot_tree(&root),
        before,
        "capability preview wrote files"
    );

    let added = jails_cmd(&root, None)
        .args(["add", "fake"])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(model.contains("cap fake @id(cap_fake)"), "{model}");
    let adapter_path =
        root.join("src/main/java/com/example/notes/adapters/memory/InMemoryNoteRepository.java");
    let adapter = fs::read_to_string(&adapter_path).unwrap();
    assert!(adapter.contains("implements NoteRepository"), "{adapter}");
    assert!(adapter.contains("Map<UUID, Note> rows"), "{adapter}");
    assert!(adapter.contains("rows.put(value.id(), value)"), "{adapter}");

    let reapplied = jails_cmd(&root, None)
        .args(["--output", "json", "add", "fake"])
        .output()
        .unwrap();
    assert!(reapplied.status.success());
    let execution: serde_json::Value = serde_json::from_slice(&reapplied.stdout).unwrap();
    assert_eq!(execution["files_written"], 0);

    let removed = jails_cmd(&root, None)
        .args(["remove", "fake", "--force"])
        .output()
        .unwrap();
    assert!(
        removed.status.success(),
        "{}",
        String::from_utf8_lossy(&removed.stderr)
    );
    // The adapter stays, and that is the capability's meaning rather than
    // removal failing. A repository port always has exactly one
    // implementation: with `db` declared it is the JDBC adapter, and without
    // it this one, or the scaffold's service would be constructor-injecting a
    // port no bean satisfies. So `fake` does not *add* the in-memory adapter
    // to a project that has no storage -- it is already there -- and what
    // removal takes back is the declaration.
    assert!(adapter_path.exists());
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(!model.contains("cap fake"), "{model}");
    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(frozen.status.success());
}

#[test]
fn unsupported_capabilities_refuse_before_the_legacy_engine_can_write() {
    // `format` on Gradle, because it is the one capability the Gradle backend
    // cannot install: Spotless needs its plugin entry inside `plugins { }`,
    // which is legal only as the script's first statement, and the Gradle
    // backend's whole contract is that it appends a marked block and touches
    // nothing else.
    let root = gradle_model_project(
        "model-capability-refusal",
        EMPTY_MODEL,
        "build.gradle",
        "plugins { id 'java' }\n",
    );
    let before = snapshot_tree(&root);
    let refused = jails_cmd(&root, None)
        .args(["add", "format"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    let stderr = String::from_utf8(refused.stderr).unwrap();
    assert!(stderr.contains("canonical"), "{stderr}");
    assert!(stderr.contains("fix:"), "{stderr}");
    assert_eq!(
        snapshot_tree(&root),
        before,
        "unsupported capability entered a mutation path"
    );
}

#[test]
fn canonical_data_capability_packs_keep_the_iterative_loop_and_eject_as_one_boundary() {
    let root = temp_dir("model-data-capability-packs");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), NOTES_JDL).unwrap();

    let csv = jails_cmd(&root, None)
        // **No `--package`.** v1 derives every managed destination from the
        // closed projection registry and refuses a reader-named one by name;
        // a reader-owned destination is what `model eject` is for, which is
        // the second half of this test.
        .args(["add", "csv", "--name", "Dataset"])
        .output()
        .unwrap();
    assert!(
        csv.status.success(),
        "{}",
        String::from_utf8_lossy(&csv.stderr)
    );
    let json = jails_cmd(&root, None)
        .args(["add", "json"])
        .output()
        .unwrap();
    assert!(
        json.status.success(),
        "{}",
        String::from_utf8_lossy(&json.stderr)
    );

    let csv_main = root.join("src/main/java/com/example/notes/adapters/DatasetReader.java");
    let csv_test = root.join("src/test/java/com/example/notes/adapters/DatasetReaderTest.java");
    let json_main = root.join("src/main/java/com/example/notes/adapters/Json.java");
    let json_test = root.join("src/test/java/com/example/notes/adapters/JsonTest.java");
    for path in [&csv_main, &csv_test, &json_main, &json_test] {
        assert!(path.is_file(), "missing {}", path.display());
    }
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(
        model.contains("cap csv Dataset @id(cap_csv_dataset)"),
        "{model}"
    );
    assert!(model.contains("cap json @id(cap_json)"), "{model}");
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("<artifactId>commons-csv</artifactId>"));
    assert!(pom.contains("<version>1.14.1</version>"));
    assert!(pom.contains("<artifactId>jackson-databind</artifactId>"));
    let clean_json_main = fs::read(&json_main).unwrap();
    let clean_json_test = fs::read(&json_test).unwrap();

    for (path, marker) in [
        (&csv_main, "readerCsvMethod"),
        (&csv_test, "readerCsvTestHelper"),
        (&json_main, "readerJsonMethod"),
        (&json_test, "readerJsonTestHelper"),
    ] {
        let source = fs::read_to_string(path).unwrap();
        let at = source.rfind("\n}").unwrap();
        fs::write(
            path,
            format!(
                "{}\n\n    void {marker}() {{}}{}",
                &source[..at],
                &source[at..]
            ),
        )
        .unwrap();
    }

    let rerun = jails_cmd(&root, None)
        .args(["add", "csv", "--name", "Dataset"])
        .output()
        .unwrap();
    assert!(
        rerun.status.success(),
        "{}",
        String::from_utf8_lossy(&rerun.stderr)
    );
    assert!(
        fs::read_to_string(&csv_main)
            .unwrap()
            .contains("readerCsvMethod")
    );
    assert!(
        fs::read_to_string(&csv_test)
            .unwrap()
            .contains("readerCsvTestHelper")
    );
    assert!(
        fs::read_to_string(&json_main)
            .unwrap()
            .contains("readerJsonMethod")
    );
    assert!(
        fs::read_to_string(&json_test)
            .unwrap()
            .contains("readerJsonTestHelper")
    );

    // A capability carries no package of its own: v1 derives its destination
    // from the closed projection registry, so the only reader-owned
    // destination is the one `model eject` produces -- which is what the rest
    // of this test exercises.
    let model_path = root.join(".jails/model.jdl");
    let declared = fs::read_to_string(&model_path).unwrap();
    fs::write(
        &model_path,
        declared.replace(
            "cap csv Dataset @id(cap_csv_dataset)",
            "cap csv Dataset @id(cap_csv_dataset) @package(imports)",
        ),
    )
    .unwrap();
    let before_package = snapshot_tree(&root);
    let refused_package = jails_cmd(&root, None).args(["sync"]).output().unwrap();
    assert!(!refused_package.status.success());
    let told = String::from_utf8_lossy(&refused_package.stderr);
    assert!(told.contains("@package` is not valid here"), "{told}");
    assert_eq!(snapshot_tree(&root), before_package);
    fs::write(&model_path, &declared).unwrap();

    // The move that v1 does state is an entity's, and it is the same
    // machinery: the managed tree relocates, the reader's delta rides across,
    // and an overlapping edit refuses before anything is written.
    let recorded = jails_cmd(&root, None)
        .args(["g", "record", "Feed", "title:string!"])
        .output()
        .unwrap();
    assert!(
        recorded.status.success(),
        "{}",
        String::from_utf8_lossy(&recorded.stderr)
    );
    let record_main = root.join("src/main/java/com/example/notes/domain/Feed.java");
    let clean_record = fs::read_to_string(&record_main).unwrap();
    let at = clean_record.rfind("\n}").unwrap();
    fs::write(
        &record_main,
        format!(
            "{}\n\n    void readerRecordMethod() {{}}{}",
            &clean_record[..at],
            &clean_record[at..]
        ),
    )
    .unwrap();
    let repackaged = fs::read_to_string(&model_path).unwrap().replace(
        "entity Feed @id(ent_feed) {",
        "entity Feed @id(ent_feed) @package(imports) {",
    );
    fs::write(&model_path, &repackaged).unwrap();
    let moved = jails_cmd(&root, None).args(["sync"]).output().unwrap();
    assert!(
        moved.status.success(),
        "{}",
        String::from_utf8_lossy(&moved.stderr)
    );
    let moved_record = root.join("src/main/java/com/example/notes/imports/Feed.java");
    assert!(!record_main.exists());
    let moved_source = fs::read_to_string(&moved_record).unwrap();
    assert!(
        moved_source.contains("readerRecordMethod"),
        "{moved_source}"
    );

    fs::write(
        &moved_record,
        moved_source.replace(
            "package com.example.notes.imports;",
            "package com.example.notes.imports; // reader changed compiler-owned line",
        ),
    )
    .unwrap();
    fs::write(
        &model_path,
        repackaged.replace("@package(imports)", "@package(feeds)"),
    )
    .unwrap();
    let before_overlap = snapshot_tree(&root);
    let refused = jails_cmd(&root, None).args(["sync"]).output().unwrap();
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("overlap"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert_eq!(snapshot_tree(&root), before_overlap);

    fs::write(&moved_record, &moved_source).unwrap();
    let retried = jails_cmd(&root, None).args(["sync"]).output().unwrap();
    assert!(
        retried.status.success(),
        "{}",
        String::from_utf8_lossy(&retried.stderr)
    );
    assert!(
        fs::read_to_string(root.join("src/main/java/com/example/notes/feeds/Feed.java"))
            .unwrap()
            .contains("readerRecordMethod")
    );

    let ejected = jails_cmd(&root, None)
        .args(["model", "eject", "cap_csv_dataset"])
        .output()
        .unwrap();
    assert!(
        ejected.status.success(),
        "{}",
        String::from_utf8_lossy(&ejected.stderr)
    );
    let reader_main = common::generated(
        &root,
        "src/main/java/com/example/notes/adapters/DatasetReader.java",
    );
    let reader_test = common::generated(
        &root,
        "src/test/java/com/example/notes/adapters/DatasetReaderTest.java",
    );
    assert!(reader_main.is_file());
    assert!(reader_test.is_file());
    let reader_main_bytes = fs::read(&reader_main).unwrap();
    let reader_test_bytes = fs::read(&reader_test).unwrap();

    let before_edited_remove = snapshot_tree(&root);
    let refused_remove = jails_cmd(&root, None)
        .args(["remove", "json"])
        .output()
        .unwrap();
    assert!(!refused_remove.status.success());
    assert!(
        String::from_utf8_lossy(&refused_remove.stderr).contains("edited by you"),
        "{}",
        String::from_utf8_lossy(&refused_remove.stderr)
    );
    assert_eq!(snapshot_tree(&root), before_edited_remove);

    fs::write(&json_main, clean_json_main).unwrap();
    fs::write(&json_test, clean_json_test).unwrap();
    let removed_json = jails_cmd(&root, None)
        .args(["remove", "json", "--force"])
        .output()
        .unwrap();
    assert!(
        removed_json.status.success(),
        "{}",
        String::from_utf8_lossy(&removed_json.stderr)
    );
    assert!(!json_main.exists());
    assert!(!json_test.exists());
    assert_eq!(fs::read(&reader_main).unwrap(), reader_main_bytes);
    assert_eq!(fs::read(&reader_test).unwrap(), reader_test_bytes);
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("<artifactId>commons-csv</artifactId>"));
    assert!(!pom.contains("<artifactId>jackson-databind</artifactId>"));

    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(
        frozen.status.success(),
        "{}",
        String::from_utf8_lossy(&frozen.stderr)
    );
    // The build check for this pack lives in
    // `every_orthogonal_capability_pack_compiles_and_tests_in_one_project`,
    // which compiles it beside the other orthogonal packs in one project.
    // What *this* test asserts is what the pack writes, and that needs no JVM.
}

#[test]
fn canonical_http_and_fake_packs_merge_and_eject_as_complete_boundaries() {
    let root = temp_dir("model-http-fake-capability-packs");
    write_plain_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    let model_path = root.join(".jails/model.jdl");
    fs::write(&model_path, NOTES_JDL).unwrap();

    for arguments in [
        vec!["add", "fake"],
        // No `--package`: v1 derives a capability's destination from the
        // closed projection registry, and `model eject` below is what
        // produces a reader-owned one.
        vec!["add", "http", "--name", "Admin"],
    ] {
        let output = jails_cmd(&root, None).args(arguments).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let fake = root.join("src/test/java/com/example/notes/testkit/Fake.java");
    let fake_test = root.join("src/test/java/com/example/notes/testkit/FakeTest.java");
    let http = root.join("src/main/java/com/example/notes/api/AdminServer.java");
    let http_test = root.join("src/test/java/com/example/notes/api/AdminServerTest.java");
    for (index, path) in [&fake, &fake_test, &http, &http_test].iter().enumerate() {
        let source = fs::read_to_string(path).unwrap();
        let at = source.rfind("\n}").unwrap();
        fs::write(
            path,
            format!(
                "{}\n\n    // reader-owned-pack-edit-{index}{}",
                &source[..at],
                &source[at..]
            ),
        )
        .unwrap();
    }

    let trigger = jails_cmd(&root, None)
        .args(["add", "csv"])
        .output()
        .unwrap();
    assert!(
        trigger.status.success(),
        "{}",
        String::from_utf8_lossy(&trigger.stderr)
    );
    for (index, path) in [&fake, &fake_test, &http, &http_test].iter().enumerate() {
        assert!(
            fs::read_to_string(path)
                .unwrap()
                .contains(&format!("reader-owned-pack-edit-{index}")),
            "lost edit in {}",
            path.display()
        );
    }

    let http_bytes = fs::read(&http).unwrap();
    let http_test_bytes = fs::read(&http_test).unwrap();
    let ejected_http = jails_cmd(&root, None)
        .args(["model", "eject", "cap_http_admin"])
        .output()
        .unwrap();
    assert!(
        ejected_http.status.success(),
        "{}",
        String::from_utf8_lossy(&ejected_http.stderr)
    );
    let reader_http = common::generated(
        &root,
        "src/main/java/com/example/notes/api/AdminServer.java",
    );
    let reader_http_test = common::generated(
        &root,
        "src/test/java/com/example/notes/api/AdminServerTest.java",
    );
    assert_eq!(fs::read(&reader_http).unwrap(), http_bytes);
    assert_eq!(fs::read(&reader_http_test).unwrap(), http_test_bytes);

    let fake_bytes = fs::read(&fake).unwrap();
    let fake_test_bytes = fs::read(&fake_test).unwrap();
    let ejected_fake = jails_cmd(&root, None)
        .args(["model", "eject", "cap_fake"])
        .output()
        .unwrap();
    assert!(
        ejected_fake.status.success(),
        "{}",
        String::from_utf8_lossy(&ejected_fake.stderr)
    );
    let reader_fake = common::generated(&root, "src/test/java/com/example/notes/testkit/Fake.java");
    let reader_fake_test = common::generated(
        &root,
        "src/test/java/com/example/notes/testkit/FakeTest.java",
    );
    assert_eq!(fs::read(reader_fake).unwrap(), fake_bytes);
    assert_eq!(fs::read(reader_fake_test).unwrap(), fake_test_bytes);

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
fn canonical_testkit_merges_and_ejects_java_and_resources_as_one_boundary() {
    let root = temp_dir("model-testkit-capability-pack");
    write_plain_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), NOTES_JDL).unwrap();
    let added = jails_cmd(&root, None)
        .args(["add", "testkit"])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );

    let generated = root.join("src");
    let java = generated.join("test/java/com/example/notes/testkit/Clocks.java");
    let fixture = generated.join("test/resources/fixtures/example.json");
    let source = fs::read_to_string(&java).unwrap();
    let at = source.rfind("\n}").unwrap();
    fs::write(
        &java,
        format!(
            "{}\n\n    // reader-owned-testkit-java{}",
            &source[..at],
            &source[at..]
        ),
    )
    .unwrap();
    fs::write(
        &fixture,
        fs::read_to_string(&fixture)
            .unwrap()
            .replace("bolt", "reader-bolt"),
    )
    .unwrap();

    let trigger = jails_cmd(&root, None)
        .args(["add", "fake"])
        .output()
        .unwrap();
    assert!(
        trigger.status.success(),
        "{}",
        String::from_utf8_lossy(&trigger.stderr)
    );
    assert!(
        fs::read_to_string(&java)
            .unwrap()
            .contains("reader-owned-testkit-java")
    );
    assert!(
        fs::read_to_string(&fixture)
            .unwrap()
            .contains("reader-bolt")
    );

    let expected = [
        "test/java/com/example/notes/testkit/Clocks.java",
        "test/java/com/example/notes/testkit/Ids.java",
        "test/java/com/example/notes/testkit/Fixtures.java",
        "test/java/com/example/notes/testkit/Cli.java",
        "test/java/com/example/notes/testkit/TestkitTest.java",
        "test/resources/fixtures/example.json",
    ];
    let bytes = expected
        .iter()
        .map(|path| fs::read(generated.join(path)).unwrap())
        .collect::<Vec<_>>();
    let ejected = jails_cmd(&root, None)
        .args(["model", "eject", "cap_testkit"])
        .output()
        .unwrap();
    assert!(
        ejected.status.success(),
        "{}",
        String::from_utf8_lossy(&ejected.stderr)
    );
    for (index, path) in expected.iter().enumerate() {
        assert_eq!(
            fs::read(root.join("src").join(path)).unwrap(),
            bytes[index],
            "ejection changed {path}"
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
fn canonical_sqlite_pack_moves_merges_ejects_and_builds_as_one_boundary() {
    let root = temp_dir("model-sqlite-capability-pack");
    write_plain_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    let model_path = root.join(".jails/model.jdl");
    fs::write(&model_path, NOTES_JDL).unwrap();
    let added = jails_cmd(&root, None)
        // No `--package`, and no move below: v1 derives a capability's
        // destination from the closed projection registry, and the ejection at
        // the end of this test is what produces a reader-owned one.
        .args(["add", "sqlite", "--name", "Store"])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );

    let generated = root.join("src");
    let database = generated.join("main/java/com/example/notes/adapters/StoreDatabase.java");
    let migrations = generated.join("main/java/com/example/notes/adapters/StoreMigrations.java");
    let test = generated.join("test/java/com/example/notes/adapters/StoreDatabaseTest.java");
    let migration = root.join("src/main/resources/db/migration/V001__sqlite_init.sql");
    for (index, path) in [&database, &migrations, &test].iter().enumerate() {
        let source = fs::read_to_string(path).unwrap();
        let at = source.rfind("\n}").unwrap();
        fs::write(
            path,
            format!(
                "{}\n\n    // reader-owned-sqlite-{index}{}",
                &source[..at],
                &source[at..]
            ),
        )
        .unwrap();
    }
    fs::write(
        &migration,
        fs::read_to_string(&migration)
            .unwrap()
            .replace("Applied once", "Reader wording survives; applied once"),
    )
    .unwrap();

    let trigger = jails_cmd(&root, None)
        .args(["add", "fake"])
        .output()
        .unwrap();
    assert!(
        trigger.status.success(),
        "{}",
        String::from_utf8_lossy(&trigger.stderr)
    );
    let managed_files = [&database, &migrations, &test];
    for (index, path) in managed_files.iter().enumerate() {
        assert!(
            fs::read_to_string(path)
                .unwrap()
                .contains(&format!("reader-owned-sqlite-{index}")),
            "lost edit in {}",
            path.display()
        );
    }
    assert!(
        fs::read_to_string(&migration)
            .unwrap()
            .contains("Reader wording survives")
    );
    let expected = [
        (
            "main/java/com/example/notes/adapters/StoreDatabase.java",
            &database,
        ),
        (
            "main/java/com/example/notes/adapters/StoreMigrations.java",
            &migrations,
        ),
        (
            "test/java/com/example/notes/adapters/StoreDatabaseTest.java",
            &test,
        ),
    ];
    let bytes = expected
        .iter()
        .map(|(_, path)| fs::read(path).unwrap())
        .collect::<Vec<_>>();
    let ejected = jails_cmd(&root, None)
        .args(["model", "eject", "cap_sqlite_store"])
        .output()
        .unwrap();
    assert!(
        ejected.status.success(),
        "{}",
        String::from_utf8_lossy(&ejected.stderr)
    );
    for (index, (path, _)) in expected.iter().enumerate() {
        assert_eq!(
            fs::read(root.join("src").join(path)).unwrap(),
            bytes[index],
            "ejection changed {path}"
        );
    }
    assert!(migration.exists(), "ejection deleted migration history");
    assert!(
        fs::read_to_string(&migration)
            .unwrap()
            .contains("Reader wording survives"),
        "ejection changed the reader-edited migration"
    );
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(
        pom.contains("<artifactId>sqlite-jdbc</artifactId>"),
        "{pom}"
    );
    assert!(!pom.contains("<goal>add-resource</goal>"), "{pom}");

    // The build check for this pack lives in
    // `every_orthogonal_capability_pack_compiles_and_tests_in_one_project`,
    // which compiles it beside the other orthogonal packs in one project.
    // What *this* test asserts is what the pack writes, and that needs no JVM.
}

#[test]
fn canonical_h2_pack_merges_ejects_and_builds() {
    let root = temp_dir("model-h2-capability-pack");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();
    let added = jails_cmd(&root, None).args(["add", "h2"]).output().unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );

    let managed = root.join("src/test/java/com/example/demo/adapters/H2DatabaseTest.java");
    let source = fs::read_to_string(&managed).unwrap();
    let at = source.rfind("\n}").unwrap();
    let edited = format!(
        "{}\n\n    // reader-owned-h2-helper{}",
        &source[..at],
        &source[at..]
    );
    fs::write(&managed, &edited).unwrap();
    let main_properties = root.join("src/main/resources/application.properties");
    let test_properties = root.join("src/test/resources/config/application.properties");
    fs::write(
        &main_properties,
        format!(
            "{}reader.h2.main=survives\n",
            fs::read_to_string(&main_properties).unwrap()
        ),
    )
    .unwrap();
    fs::write(
        &test_properties,
        format!(
            "{}reader.h2.test=survives\n",
            fs::read_to_string(&test_properties).unwrap()
        ),
    )
    .unwrap();

    let synced = jails_cmd(&root, None).args(["sync"]).output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );
    assert!(
        fs::read_to_string(&managed)
            .unwrap()
            .contains("reader-owned-h2-helper")
    );
    assert!(
        fs::read_to_string(&main_properties)
            .unwrap()
            .contains("reader.h2.main=survives")
    );
    assert!(
        fs::read_to_string(&test_properties)
            .unwrap()
            .contains("reader.h2.test=survives")
    );

    let live_bytes = fs::read(&managed).unwrap();
    let ejected = jails_cmd(&root, None)
        .args(["model", "eject", "cap_h2"])
        .output()
        .unwrap();
    assert!(
        ejected.status.success(),
        "{}",
        String::from_utf8_lossy(&ejected.stderr)
    );
    let reader = root.join("src/test/java/com/example/demo/adapters/H2DatabaseTest.java");
    assert_eq!(fs::read(&reader).unwrap(), live_bytes);

    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    for artifact in ["spring-boot-starter-jdbc", "h2", "spring-boot-h2console"] {
        assert!(
            pom.contains(&format!("<artifactId>{artifact}</artifactId>")),
            "{pom}"
        );
    }
    let main = fs::read_to_string(&main_properties).unwrap();
    assert!(main.contains("jdbc:h2:file:./data/app;AUTO_SERVER=TRUE"));
    assert!(main.contains("spring.persistence.exceptiontranslation.enabled=false"));
    let test = fs::read_to_string(&test_properties).unwrap();
    assert!(test.contains("jdbc:h2:mem:test;DB_CLOSE_DELAY=-1"));

    if real_mvn_available() && real_java_supports_target_release() {
        let path = real_path_without_mvnd();
        let built = real_maven_cmd(&root, &path)
            .args(["-q", "-B", "test"])
            .output()
            .unwrap();
        assert!(
            built.status.success(),
            "canonical H2 pack did not compile and test:\n{}\n{}",
            String::from_utf8_lossy(&built.stdout),
            String::from_utf8_lossy(&built.stderr)
        );
    }
}

#[test]
fn canonical_actuator_pack_merges_ejects_only_java_and_builds() {
    let root = temp_dir("model-actuator-capability-pack");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();
    let added = jails_cmd(&root, None)
        .args(["add", "actuator"])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );

    let managed = root.join("src/test/java/com/example/demo/ActuatorEndpointsTest.java");
    let source = fs::read_to_string(&managed).unwrap();
    let at = source.rfind("\n}").unwrap();
    let edited = format!(
        "{}\n\n    // reader-owned-actuator-helper{}",
        &source[..at],
        &source[at..]
    );
    fs::write(&managed, &edited).unwrap();
    let properties = root.join("src/main/resources/application.properties");
    fs::write(
        &properties,
        format!(
            "{}reader.actuator=survives\n",
            fs::read_to_string(&properties).unwrap()
        ),
    )
    .unwrap();

    let synced = jails_cmd(&root, None).args(["sync"]).output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );
    assert!(
        fs::read_to_string(&managed)
            .unwrap()
            .contains("reader-owned-actuator-helper")
    );
    assert!(
        fs::read_to_string(&properties)
            .unwrap()
            .contains("reader.actuator=survives")
    );

    let live_bytes = fs::read(&managed).unwrap();
    let ejected = jails_cmd(&root, None)
        .args(["model", "eject", "cap_actuator"])
        .output()
        .unwrap();
    assert!(
        ejected.status.success(),
        "{}",
        String::from_utf8_lossy(&ejected.stderr)
    );
    let reader = root.join("src/test/java/com/example/demo/ActuatorEndpointsTest.java");
    assert_eq!(fs::read(&reader).unwrap(), live_bytes);

    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(
        pom.contains("<artifactId>spring-boot-starter-actuator</artifactId>"),
        "{pom}"
    );
    let properties = fs::read_to_string(&properties).unwrap();
    for owned in [
        "management.endpoints.web.exposure.include=health,info,prometheus,threaddump",
        "management.server.port=8081",
        "management.endpoints.web.base-path=/management",
        "management.endpoint.health.group.readiness.include=ping",
        "info.app.version=@project.version@",
    ] {
        assert!(properties.contains(owned), "missing {owned}: {properties}");
    }
    assert!(properties.contains("reader.actuator=survives"));
    assert!(!properties.contains("management.endpoints.web.exposure.include=*"));

    // The build check for this pack lives in
    // `every_orthogonal_capability_pack_compiles_and_tests_in_one_project`,
    // which compiles it beside the other orthogonal packs in one project.
    // What *this* test asserts is what the pack writes, and that needs no JVM.
}

#[test]
fn canonical_cache_pack_merges_ejects_the_java_boundary_and_builds() {
    let root = temp_dir("model-cache-capability-pack");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();
    let added = jails_cmd(&root, None)
        .args(["add", "cache"])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );

    let managed = [
        root.join("src/main/java/com/example/demo/CacheConfig.java"),
        root.join("src/test/java/com/example/demo/CacheConfigTest.java"),
    ];
    for (index, path) in managed.iter().enumerate() {
        let source = fs::read_to_string(path).unwrap();
        let edited = if let Some(at) = source.rfind("\n}") {
            format!(
                "{}\n\n    // reader-owned-cache-helper-{index}{}",
                &source[..at],
                &source[at..]
            )
        } else {
            let at = source.rfind("{}").expect("Java type has no closing body");
            format!(
                "{}{{\n\n    // reader-owned-cache-helper-{index}\n}}{}",
                &source[..at],
                &source[at + 2..]
            )
        };
        fs::write(path, edited).unwrap();
    }
    let properties = root.join("src/main/resources/application.properties");
    fs::write(
        &properties,
        format!(
            "{}reader.cache=survives\n",
            fs::read_to_string(&properties).unwrap()
        ),
    )
    .unwrap();

    let synced = jails_cmd(&root, None).args(["sync"]).output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );
    for (index, path) in managed.iter().enumerate() {
        assert!(
            fs::read_to_string(path)
                .unwrap()
                .contains(&format!("reader-owned-cache-helper-{index}"))
        );
    }

    let live_bytes = managed.each_ref().map(|path| fs::read(path).unwrap());
    let ejected = jails_cmd(&root, None)
        .args(["model", "eject", "cap_cache"])
        .output()
        .unwrap();
    assert!(
        ejected.status.success(),
        "{}",
        String::from_utf8_lossy(&ejected.stderr)
    );
    let reader = [
        common::generated(&root, "src/main/java/com/example/demo/CacheConfig.java"),
        common::generated(&root, "src/test/java/com/example/demo/CacheConfigTest.java"),
    ];
    for (reader, expected) in reader.iter().zip(live_bytes) {
        assert_eq!(fs::read(reader).unwrap(), expected);
    }

    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    for artifact in ["spring-boot-starter-cache", "caffeine"] {
        assert!(
            pom.contains(&format!("<artifactId>{artifact}</artifactId>")),
            "{pom}"
        );
    }
    let properties = fs::read_to_string(&properties).unwrap();
    for required in [
        "reader.cache=survives",
        "spring.cache.type=caffeine",
        "spring.cache.caffeine.spec=maximumSize=1000,expireAfterWrite=60s",
    ] {
        assert!(
            properties.contains(required),
            "missing {required}: {properties}"
        );
    }

    // The build check for this pack lives in
    // `every_orthogonal_capability_pack_compiles_and_tests_in_one_project`,
    // which compiles it beside the other orthogonal packs in one project.
    // What *this* test asserts is what the pack writes, and that needs no JVM.
}

#[test]
fn canonical_cors_pack_merges_ejects_the_java_boundary_and_builds() {
    let root = temp_dir("model-cors-capability-pack");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();
    let added = jails_cmd(&root, None)
        .args(["add", "cors"])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );

    let managed = [
        root.join("src/main/java/com/example/demo/CorsConfig.java"),
        root.join("src/test/java/com/example/demo/CorsConfigTest.java"),
    ];
    let modern_test = fs::read_to_string(&managed[1]).unwrap();
    assert!(
        modern_test.contains("servlet.assertj.MockMvcTester"),
        "the default Boot project did not compile the modern CORS branch: {modern_test}"
    );
    for (index, path) in managed.iter().enumerate() {
        let source = fs::read_to_string(path).unwrap();
        let at = source.rfind("\n}").expect("CORS Java type has no body");
        fs::write(
            path,
            format!(
                "{}\n\n    // reader-owned-cors-helper-{index}{}",
                &source[..at],
                &source[at..]
            ),
        )
        .unwrap();
    }
    let properties = root.join("src/main/resources/application.properties");
    fs::write(
        &properties,
        format!(
            "{}reader.cors=survives\n",
            fs::read_to_string(&properties).unwrap()
        ),
    )
    .unwrap();

    let synced = jails_cmd(&root, None).args(["sync"]).output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );
    for (index, path) in managed.iter().enumerate() {
        assert!(
            fs::read_to_string(path)
                .unwrap()
                .contains(&format!("reader-owned-cors-helper-{index}"))
        );
    }

    let live_bytes = managed.each_ref().map(|path| fs::read(path).unwrap());
    let ejected = jails_cmd(&root, None)
        .args(["model", "eject", "cap_cors"])
        .output()
        .unwrap();
    assert!(
        ejected.status.success(),
        "{}",
        String::from_utf8_lossy(&ejected.stderr)
    );
    let reader = [
        common::generated(&root, "src/main/java/com/example/demo/CorsConfig.java"),
        common::generated(&root, "src/test/java/com/example/demo/CorsConfigTest.java"),
    ];
    for (reader, expected) in reader.iter().zip(live_bytes) {
        assert_eq!(fs::read(reader).unwrap(), expected);
    }
    let properties = fs::read_to_string(&properties).unwrap();
    assert!(properties.contains("app.cors.allowed-origins=https://example.invalid"));
    assert!(properties.contains("reader.cors=survives"));

    // The build check for this pack lives in
    // `every_orthogonal_capability_pack_compiles_and_tests_in_one_project`,
    // which compiles it beside the other orthogonal packs in one project.
    // What *this* test asserts is what the pack writes, and that needs no JVM.
}

#[test]
fn canonical_observability_pack_merges_ejects_and_serves_prometheus() {
    let root = temp_dir("model-observability-capability-pack");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();
    let added = jails_cmd(&root, None)
        .args(["add", "observability"])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );

    let managed = [
        root.join("src/main/java/com/example/demo/MetricsConfig.java"),
        root.join("src/main/java/com/example/demo/AppMetrics.java"),
        root.join("src/test/java/com/example/demo/AppMetricsTest.java"),
        root.join("src/test/java/com/example/demo/PrometheusScrapeTest.java"),
    ];
    assert!(
        fs::read_to_string(&managed[0])
            .unwrap()
            .contains("boot.micrometer.metrics.autoconfigure.MeterRegistryCustomizer")
    );
    for (index, path) in managed.iter().enumerate() {
        let source = fs::read_to_string(path).unwrap();
        let at = source
            .rfind("\n}")
            .expect("observability Java type has no body");
        fs::write(
            path,
            format!(
                "{}\n\n    // reader-owned-observability-helper-{index}{}",
                &source[..at],
                &source[at..]
            ),
        )
        .unwrap();
    }
    let properties = root.join("src/main/resources/application.properties");
    fs::write(
        &properties,
        format!(
            "{}reader.observability=survives\n",
            fs::read_to_string(&properties).unwrap()
        ),
    )
    .unwrap();

    let synced = jails_cmd(&root, None).args(["sync"]).output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );
    for (index, path) in managed.iter().enumerate() {
        assert!(
            fs::read_to_string(path)
                .unwrap()
                .contains(&format!("reader-owned-observability-helper-{index}"))
        );
    }

    let live_bytes = managed.each_ref().map(|path| fs::read(path).unwrap());
    let ejected = jails_cmd(&root, None)
        .args(["model", "eject", "cap_observability"])
        .output()
        .unwrap();
    assert!(
        ejected.status.success(),
        "{}",
        String::from_utf8_lossy(&ejected.stderr)
    );
    let reader = [
        common::generated(&root, "src/main/java/com/example/demo/MetricsConfig.java"),
        common::generated(&root, "src/main/java/com/example/demo/AppMetrics.java"),
        common::generated(&root, "src/test/java/com/example/demo/AppMetricsTest.java"),
        common::generated(
            &root,
            "src/test/java/com/example/demo/PrometheusScrapeTest.java",
        ),
    ];
    for (reader, expected) in reader.iter().zip(live_bytes) {
        assert_eq!(fs::read(reader).unwrap(), expected);
    }
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    for artifact in [
        "spring-boot-starter-actuator",
        "micrometer-registry-prometheus",
    ] {
        assert!(pom.contains(&format!("<artifactId>{artifact}</artifactId>")));
    }
    let properties = fs::read_to_string(&properties).unwrap();
    for required in [
        "reader.observability=survives",
        "management.endpoints.web.exposure.include=health,info,prometheus,threaddump",
        "management.metrics.distribution.percentiles-histogram.http.server.requests=false",
        "management.tracing.baggage.local-fields=request-id",
        "server.tomcat.accesslog.directory=/dev",
    ] {
        assert!(
            properties.contains(required),
            "missing {required}: {properties}"
        );
    }

    // The build check for this pack lives in
    // `every_orthogonal_capability_pack_compiles_and_tests_in_one_project`,
    // which compiles it beside the other orthogonal packs in one project.
    // What *this* test asserts is what the pack writes, and that needs no JVM.
}

#[test]
fn canonical_security_pack_merges_ejects_and_keeps_cors_buildable() {
    let root = temp_dir("model-security-capability-pack");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();
    for capability in ["security", "cors"] {
        let added = jails_cmd(&root, None)
            .args(["add", capability])
            .output()
            .unwrap();
        assert!(
            added.status.success(),
            "{}",
            String::from_utf8_lossy(&added.stderr)
        );
    }

    let managed = [
        root.join("src/main/java/com/example/demo/SecurityConfig.java"),
        root.join("src/main/java/com/example/demo/ProductionSecurityConfig.java"),
        root.join("src/main/java/com/example/demo/ScopeAuthorizer.java"),
        root.join("src/test/java/com/example/demo/SecurityConfigTest.java"),
        root.join("src/test/java/com/example/demo/ScopeAuthorizerTest.java"),
    ];
    assert!(
        fs::read_to_string(&managed[3])
            .unwrap()
            .contains("boot.webmvc.test.autoconfigure.WebMvcTest")
    );
    for (index, path) in managed.iter().enumerate() {
        let source = fs::read_to_string(path).unwrap();
        let at = source.rfind("\n}").expect("security Java type has no body");
        fs::write(
            path,
            format!(
                "{}\n\n    // reader-owned-security-helper-{index}{}",
                &source[..at],
                &source[at..]
            ),
        )
        .unwrap();
    }

    let synced = jails_cmd(&root, None).args(["sync"]).output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );
    for (index, path) in managed.iter().enumerate() {
        assert!(
            fs::read_to_string(path)
                .unwrap()
                .contains(&format!("reader-owned-security-helper-{index}"))
        );
    }

    let live_bytes = managed.each_ref().map(|path| fs::read(path).unwrap());
    let ejected = jails_cmd(&root, None)
        .args(["model", "eject", "cap_security"])
        .output()
        .unwrap();
    assert!(
        ejected.status.success(),
        "{}",
        String::from_utf8_lossy(&ejected.stderr)
    );
    let reader = [
        common::generated(&root, "src/main/java/com/example/demo/SecurityConfig.java"),
        common::generated(
            &root,
            "src/main/java/com/example/demo/ProductionSecurityConfig.java",
        ),
        common::generated(&root, "src/main/java/com/example/demo/ScopeAuthorizer.java"),
        common::generated(
            &root,
            "src/test/java/com/example/demo/SecurityConfigTest.java",
        ),
        common::generated(
            &root,
            "src/test/java/com/example/demo/ScopeAuthorizerTest.java",
        ),
    ];
    for (reader, expected) in reader.iter().zip(live_bytes) {
        assert_eq!(fs::read(reader).unwrap(), expected);
    }
    for cors in [
        root.join("src/main/java/com/example/demo/CorsConfig.java"),
        root.join("src/test/java/com/example/demo/CorsConfigTest.java"),
    ] {
        assert!(
            cors.is_file(),
            "security ejection removed {}",
            cors.display()
        );
    }
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    for artifact in [
        "spring-boot-starter-security",
        "spring-boot-starter-oauth2-resource-server",
        "spring-security-test",
        "spring-boot-starter-webmvc-test",
    ] {
        assert!(pom.contains(&format!("<artifactId>{artifact}</artifactId>")));
    }

    // The build check for this pack lives in
    // `every_orthogonal_capability_pack_compiles_and_tests_in_one_project`,
    // which compiles it beside the other orthogonal packs in one project.
    // What *this* test asserts is what the pack writes, and that needs no JVM.
}

#[test]
fn canonical_sse_pack_merges_ejects_across_packages_and_runs_its_proof() {
    let root = temp_dir("model-sse-capability-pack");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();
    let added = jails_cmd(&root, None)
        .args(["add", "sse"])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );

    let managed = [
        root.join("src/main/java/com/example/demo/EventHub.java"),
        root.join("src/main/java/com/example/demo/SchedulingConfig.java"),
        root.join("src/main/java/com/example/demo/web/EventStreamController.java"),
        root.join("src/test/java/com/example/demo/EventHubTest.java"),
    ];
    let controller = fs::read_to_string(&managed[2]).unwrap();
    assert!(controller.contains("import com.example.demo.EventHub;"));
    assert!(controller.contains("/events/{topic}/stream"));
    for (index, path) in managed.iter().enumerate() {
        let source = fs::read_to_string(path).unwrap();
        let edited = if let Some(at) = source.rfind("\n}") {
            format!(
                "{}\n\n    // reader-owned-sse-helper-{index}{}",
                &source[..at],
                &source[at..]
            )
        } else {
            let at = source.rfind("{}").expect("SSE Java type has no body");
            format!(
                "{}{{\n\n    // reader-owned-sse-helper-{index}\n}}{}",
                &source[..at],
                &source[at + 2..]
            )
        };
        fs::write(path, edited).unwrap();
    }
    let properties = root.join("src/main/resources/application.properties");
    fs::write(
        &properties,
        format!(
            "{}reader.sse=survives\n",
            fs::read_to_string(&properties).unwrap()
        ),
    )
    .unwrap();

    let synced = jails_cmd(&root, None).args(["sync"]).output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );
    for (index, path) in managed.iter().enumerate() {
        assert!(
            fs::read_to_string(path)
                .unwrap()
                .contains(&format!("reader-owned-sse-helper-{index}"))
        );
    }

    let live_bytes = managed.each_ref().map(|path| fs::read(path).unwrap());
    let ejected = jails_cmd(&root, None)
        .args(["model", "eject", "cap_sse"])
        .output()
        .unwrap();
    assert!(
        ejected.status.success(),
        "{}",
        String::from_utf8_lossy(&ejected.stderr)
    );
    let reader = [
        common::generated(&root, "src/main/java/com/example/demo/EventHub.java"),
        common::generated(
            &root,
            "src/main/java/com/example/demo/SchedulingConfig.java",
        ),
        common::generated(
            &root,
            "src/main/java/com/example/demo/web/EventStreamController.java",
        ),
        common::generated(&root, "src/test/java/com/example/demo/EventHubTest.java"),
    ];
    for (reader, expected) in reader.iter().zip(live_bytes) {
        assert_eq!(fs::read(reader).unwrap(), expected);
    }
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("<artifactId>spring-boot-starter-web</artifactId>"));
    let properties = fs::read_to_string(&properties).unwrap();
    for required in ["reader.sse=survives", "spring.task.scheduling.pool.size=4"] {
        assert!(
            properties.contains(required),
            "missing {required}: {properties}"
        );
    }

    // The build check for this pack lives in
    // `every_orthogonal_capability_pack_compiles_and_tests_in_one_project`,
    // which compiles it beside the other orthogonal packs in one project.
    // What *this* test asserts is what the pack writes, and that needs no JVM.
}

#[test]
fn canonical_redis_pack_keeps_source_and_compose_in_the_iterative_loop() {
    let root = temp_dir("model-redis-capability-pack");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();
    let added = jails_cmd(&root, None)
        .args(["add", "redis", "--no-start"])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}{}",
        String::from_utf8_lossy(&added.stdout),
        String::from_utf8_lossy(&added.stderr)
    );

    let managed = [
        root.join("src/main/java/com/example/demo/adapters/KeyValueStore.java"),
        root.join("src/test/java/com/example/demo/adapters/KeyValueStoreIT.java"),
    ];
    for (index, path) in managed.iter().enumerate() {
        let source = fs::read_to_string(path).unwrap();
        let at = source.rfind("\n}").unwrap();
        fs::write(
            path,
            format!(
                "{}\n\n    // reader-owned-redis-helper-{index}{}",
                &source[..at],
                &source[at..]
            ),
        )
        .unwrap();
    }
    let compose = root.join("compose.yaml");
    let source = fs::read_to_string(&compose).unwrap();
    fs::write(
        &compose,
        source
            .replace(
                "    healthcheck:\n",
                "    restart: unless-stopped\n    healthcheck:\n",
            )
            .replace(
                "services:\n",
                "services:\n  reader-service:\n    image: reader:latest\n",
            ),
    )
    .unwrap();

    let synced = jails_cmd(&root, None).args(["sync"]).output().unwrap();
    assert!(
        synced.status.success(),
        "{}{}",
        String::from_utf8_lossy(&synced.stdout),
        String::from_utf8_lossy(&synced.stderr)
    );
    for (index, path) in managed.iter().enumerate() {
        assert!(
            fs::read_to_string(path)
                .unwrap()
                .contains(&format!("reader-owned-redis-helper-{index}"))
        );
    }
    let compose_source = fs::read_to_string(&compose).unwrap();
    for expected in [
        "image: redis:7-alpine",
        "restart: unless-stopped",
        "reader-service:",
    ] {
        assert!(
            compose_source.contains(expected),
            "missing {expected}: {compose_source}"
        );
    }
    assert!(!compose_source.contains("redis-data"), "{compose_source}");

    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    for artifact in [
        "spring-boot-starter-data-redis",
        "testcontainers",
        "spring-boot-testcontainers",
        "maven-failsafe-plugin",
    ] {
        assert!(pom.contains(artifact), "missing {artifact}: {pom}");
    }
    let properties =
        fs::read_to_string(root.join("src/main/resources/application.properties")).unwrap();
    for property in [
        "spring.data.redis.host=localhost",
        "spring.data.redis.port=6379",
        "app.redis.default-ttl=PT10M",
    ] {
        assert!(
            properties.contains(property),
            "missing {property}: {properties}"
        );
    }

    // The build check for this pack lives in
    // `the_health_indicator_capability_packs_compile_and_test_in_one_project`,
    // which compiles it beside the other health-indicator pack in one project.
    // What *this* test asserts is what the pack writes, and that needs no JVM.
}

#[test]
fn canonical_kafka_pack_keeps_source_and_compose_in_the_iterative_loop() {
    let root = temp_dir("model-kafka-capability-pack");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();
    let added = jails_cmd(&root, None)
        .args(["add", "kafka", "--no-start"])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}{}",
        String::from_utf8_lossy(&added.stdout),
        String::from_utf8_lossy(&added.stderr)
    );

    let managed = [
        root.join("src/main/java/com/example/demo/messaging/KafkaConfig.java"),
        root.join("src/main/java/com/example/demo/messaging/NonRetryableException.java"),
        root.join("src/test/java/com/example/demo/messaging/KafkaConfigTest.java"),
        root.join("src/test/java/com/example/demo/KafkaTestcontainersConfig.java"),
    ];
    for (index, path) in managed.iter().enumerate() {
        let source = fs::read_to_string(path).unwrap();
        let at = source.rfind("\n}").unwrap();
        fs::write(
            path,
            format!(
                "{}\n\n    // reader-owned-kafka-helper-{index}{}",
                &source[..at],
                &source[at..]
            ),
        )
        .unwrap();
    }
    let compose = root.join("compose.yaml");
    let source = fs::read_to_string(&compose).unwrap();
    fs::write(
        &compose,
        source
            .replace(
                "    healthcheck:\n",
                "    restart: unless-stopped\n    healthcheck:\n",
            )
            .replace(
                "services:\n",
                "services:\n  reader-service:\n    image: reader:latest\n",
            ),
    )
    .unwrap();

    let synced = jails_cmd(&root, None).args(["sync"]).output().unwrap();
    assert!(
        synced.status.success(),
        "{}{}",
        String::from_utf8_lossy(&synced.stdout),
        String::from_utf8_lossy(&synced.stderr)
    );
    for (index, path) in managed.iter().enumerate() {
        assert!(
            fs::read_to_string(path)
                .unwrap()
                .contains(&format!("reader-owned-kafka-helper-{index}"))
        );
    }
    let compose_source = fs::read_to_string(&compose).unwrap();
    for expected in [
        "image: apache/kafka:4.1.0",
        "restart: unless-stopped",
        "reader-service:",
    ] {
        assert!(
            compose_source.contains(expected),
            "missing {expected}: {compose_source}"
        );
    }

    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    for artifact in [
        "spring-boot-starter-kafka",
        "micrometer-core",
        "spring-boot-testcontainers",
        "testcontainers-kafka",
        "testcontainers-junit-jupiter",
        "awaitility",
    ] {
        assert!(pom.contains(artifact), "missing {artifact}: {pom}");
    }
    let properties =
        fs::read_to_string(root.join("src/main/resources/application.properties")).unwrap();
    for property in [
        "spring.kafka.consumer.group-id=demo",
        "spring.kafka.consumer.auto-offset-reset=earliest",
        "spring.kafka.consumer.properties.spring.json.trusted.packages=com.example.demo,com.example.demo.*",
        "spring.kafka.consumer.properties.group.protocol=consumer",
    ] {
        assert!(
            properties.contains(property),
            "missing {property}: {properties}"
        );
    }

    // The build check for this pack lives in
    // `the_health_indicator_capability_packs_compile_and_test_in_one_project`,
    // which compiles it beside the other health-indicator pack in one project.
    // What *this* test asserts is what the pack writes, and that needs no JVM.
}

#[test]
fn canonical_mail_pack_keeps_source_and_compose_in_the_iterative_loop() {
    let root = temp_dir("model-mail-capability-pack");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();
    let added = jails_cmd(&root, None)
        .args(["add", "mail", "--no-start"])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}{}",
        String::from_utf8_lossy(&added.stdout),
        String::from_utf8_lossy(&added.stderr)
    );

    let managed = [
        root.join("src/main/java/com/example/demo/Mailer.java"),
        root.join("src/test/java/com/example/demo/MailerIT.java"),
    ];
    for (index, path) in managed.iter().enumerate() {
        let source = fs::read_to_string(path).unwrap();
        let at = source.rfind("\n}").unwrap();
        fs::write(
            path,
            format!(
                "{}\n\n    // reader-owned-mail-helper-{index}{}",
                &source[..at],
                &source[at..]
            ),
        )
        .unwrap();
    }
    let compose = root.join("compose.yaml");
    let source = fs::read_to_string(&compose).unwrap();
    fs::write(
        &compose,
        source
            .replace("    ports:\n", "    restart: unless-stopped\n    ports:\n")
            .replace(
                "services:\n",
                "services:\n  reader-service:\n    image: reader:latest\n",
            ),
    )
    .unwrap();

    let synced = jails_cmd(&root, None).args(["sync"]).output().unwrap();
    assert!(
        synced.status.success(),
        "{}{}",
        String::from_utf8_lossy(&synced.stdout),
        String::from_utf8_lossy(&synced.stderr)
    );
    for (index, path) in managed.iter().enumerate() {
        assert!(
            fs::read_to_string(path)
                .unwrap()
                .contains(&format!("reader-owned-mail-helper-{index}"))
        );
    }
    let compose_source = fs::read_to_string(&compose).unwrap();
    for expected in [
        "image: axllent/mailpit:v1.21",
        "restart: unless-stopped",
        "reader-service:",
    ] {
        assert!(
            compose_source.contains(expected),
            "missing {expected}: {compose_source}"
        );
    }

    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    for artifact in [
        "spring-boot-starter-mail",
        "spring-boot-starter-mail-test",
        "awaitility",
        "testcontainers",
        "testcontainers-junit-jupiter",
        "maven-failsafe-plugin",
    ] {
        assert!(pom.contains(artifact), "missing {artifact}: {pom}");
    }
    let properties =
        fs::read_to_string(root.join("src/main/resources/application.properties")).unwrap();
    for property in [
        "spring.mail.host=localhost",
        "spring.mail.port=1025",
        "app.mail.from=no-reply@example.com",
    ] {
        assert!(
            properties.contains(property),
            "missing {property}: {properties}"
        );
    }

    if real_mvn_available() && real_java_supports_target_release() {
        let path = real_path_without_mvnd();
        let built = real_maven_cmd(&root, &path)
            .args(["-q", "-B", "verify"])
            .output()
            .unwrap();
        assert!(
            built.status.success(),
            "canonical Mail pack did not verify with real Maven:\n{}\n{}",
            String::from_utf8_lossy(&built.stdout),
            String::from_utf8_lossy(&built.stderr)
        );
        assert!(
            root.join("target/test-classes/com/example/demo/MailerIT.class")
                .is_file(),
            "real Maven did not compile MailerIT"
        );
    }
}

#[test]
fn canonical_toxiproxy_pack_keeps_testkit_edits_and_runs_with_real_maven() {
    let root = temp_dir("model-toxiproxy-capability-pack");
    write_plain_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();
    let added = jails_cmd(&root, None)
        .args(["add", "toxiproxy", "--no-start"])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}{}",
        String::from_utf8_lossy(&added.stdout),
        String::from_utf8_lossy(&added.stderr)
    );

    let managed = [
        root.join("src/test/java/com/example/demo/testkit/Faults.java"),
        root.join("src/test/java/com/example/demo/testkit/FaultsTest.java"),
    ];
    let originals = managed
        .each_ref()
        .map(|path| fs::read_to_string(path).unwrap());
    for (index, path) in managed.iter().enumerate() {
        let source = fs::read_to_string(path).unwrap();
        let at = source.rfind("\n}").unwrap();
        fs::write(
            path,
            format!(
                "{}\n\n    // reader-owned-toxiproxy-helper-{index}{}",
                &source[..at],
                &source[at..]
            ),
        )
        .unwrap();
    }

    let generated_again = jails_cmd(&root, None)
        .args(["add", "fake"])
        .output()
        .unwrap();
    assert!(
        generated_again.status.success(),
        "{}{}",
        String::from_utf8_lossy(&generated_again.stdout),
        String::from_utf8_lossy(&generated_again.stderr)
    );
    for (index, path) in managed.iter().enumerate() {
        assert!(
            fs::read_to_string(path)
                .unwrap()
                .contains(&format!("reader-owned-toxiproxy-helper-{index}"))
        );
    }
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    for (artifact, version) in [
        ("testcontainers-toxiproxy", "2.0.5"),
        ("toxiproxy-java", "2.1.11"),
    ] {
        assert!(pom.contains(artifact), "missing {artifact}: {pom}");
        assert!(
            pom.contains(version),
            "missing {artifact} version {version}: {pom}"
        );
    }

    if real_mvn_available() && real_java_supports_target_release() && real_docker_available() {
        let path = real_path_without_mvnd();
        let tested = real_maven_cmd(&root, &path)
            .args(["-q", "-B", "test"])
            .output()
            .unwrap();
        assert!(
            tested.status.success(),
            "canonical Toxiproxy pack did not pass real Maven tests:\n{}\n{}",
            String::from_utf8_lossy(&tested.stdout),
            String::from_utf8_lossy(&tested.stderr)
        );
        assert!(
            root.join("target/test-classes/com/example/demo/testkit/FaultsTest.class")
                .is_file(),
            "real Maven did not compile FaultsTest"
        );
    }

    for (path, original) in managed.iter().zip(originals) {
        fs::write(path, original).unwrap();
    }
    let removed = jails_cmd(&root, None)
        .args(["remove", "toxiproxy", "--force"])
        .output()
        .unwrap();
    assert!(
        removed.status.success(),
        "{}{}",
        String::from_utf8_lossy(&removed.stdout),
        String::from_utf8_lossy(&removed.stderr)
    );
    assert!(managed.iter().all(|path| !path.exists()));
    assert!(
        root.join("src/test/java/com/example/demo/testkit/Fake.java")
            .is_file(),
        "Toxiproxy ejection removed the independent fake boundary"
    );
}

#[test]
fn canonical_coverage_is_lossless_refuses_owned_edits_and_passes_real_verify() {
    let root = temp_dir("model-coverage-capability-pack");
    write_plain_fixture(&root);
    let test_dir = common::generated(&root, "src/test/java/com/example/demo");
    fs::create_dir_all(&test_dir).unwrap();
    fs::write(
        test_dir.join("DemoApplicationTest.java"),
        "package com.example.demo;\n\nimport static org.junit.jupiter.api.Assertions.assertNotNull;\n\nimport org.junit.jupiter.api.Test;\n\nclass DemoApplicationTest {\n    @Test\n    void constructs() {\n        assertNotNull(new DemoApplication());\n    }\n}\n",
    )
    .unwrap();
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();

    let added = jails_cmd(&root, None)
        .args(["add", "coverage"])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}{}",
        String::from_utf8_lossy(&added.stdout),
        String::from_utf8_lossy(&added.stderr)
    );
    let pom_path = root.join("pom.xml");
    let installed = fs::read_to_string(&pom_path).unwrap();
    for expected in [
        "jails:coverage",
        "jacoco-maven-plugin",
        "<version>0.8.15</version>",
        "<minimum>0.80</minimum>",
    ] {
        assert!(
            installed.contains(expected),
            "missing {expected}: {installed}"
        );
    }
    fs::write(
        &pom_path,
        installed.replace(
            "</project>",
            "    <!-- reader-owned-coverage-note -->\n</project>",
        ),
    )
    .unwrap();

    let generated_again = jails_cmd(&root, None)
        .args(["add", "fake"])
        .output()
        .unwrap();
    assert!(
        generated_again.status.success(),
        "{}{}",
        String::from_utf8_lossy(&generated_again.stdout),
        String::from_utf8_lossy(&generated_again.stderr)
    );
    let clean = fs::read_to_string(&pom_path).unwrap();
    assert!(clean.contains("reader-owned-coverage-note"), "{clean}");

    fs::write(
        &pom_path,
        clean.replace("<minimum>0.80</minimum>", "<minimum>0.75</minimum>"),
    )
    .unwrap();
    let before = snapshot_tree(&root);
    let conflict = jails_cmd(&root, None)
        .args(["add", "json"])
        .output()
        .unwrap();
    assert!(!conflict.status.success());
    assert!(
        String::from_utf8_lossy(&conflict.stderr).contains("coverage block was edited"),
        "{}",
        String::from_utf8_lossy(&conflict.stderr)
    );
    assert_eq!(snapshot_tree(&root), before, "coverage refusal wrote files");
    fs::write(&pom_path, &clean).unwrap();

    if real_mvn_available() && real_java_supports_target_release() {
        let path = real_path_without_mvnd();
        let verified = real_maven_cmd(&root, &path)
            // `-DforkCount=1` overrides the suite-wide `forkCount=0`, and this is
            // the one place that must: JaCoCo measures coverage by attaching a
            // `-javaagent` to the *forked* Surefire JVM. Run the tests inside
            // the Maven JVM instead and the agent never attaches, every class
            // reads as uncovered, and `jacoco:check` fails a threshold that
            // nothing in the project got wrong.
            .args(["-q", "-B", "-DforkCount=1", "verify"])
            .output()
            .unwrap();
        assert!(
            verified.status.success(),
            "canonical coverage did not pass real Maven verify:\n{}\n{}",
            String::from_utf8_lossy(&verified.stdout),
            String::from_utf8_lossy(&verified.stderr)
        );
        assert!(
            root.join("target/site/jacoco/jacoco.xml").is_file(),
            "JaCoCo did not publish its report"
        );
    }

    let removed = jails_cmd(&root, None)
        .args(["remove", "coverage", "--force"])
        .output()
        .unwrap();
    assert!(
        removed.status.success(),
        "{}{}",
        String::from_utf8_lossy(&removed.stdout),
        String::from_utf8_lossy(&removed.stderr)
    );
    let pom = fs::read_to_string(&pom_path).unwrap();
    assert!(!pom.contains("jails:coverage"), "{pom}");
    assert!(!pom.contains("jacoco-maven-plugin"), "{pom}");
    assert!(pom.contains("reader-owned-coverage-note"), "{pom}");
    assert!(
        root.join("src/test/java/com/example/demo/testkit/Fake.java")
            .is_file(),
        "coverage removal touched the independent fake boundary"
    );
}

/// `storage postgres` wires the test half, not just the main half.
///
/// Once `spring-boot-starter-jdbc` is present, JDBC auto-configuration demands
/// a `DataSource` for **every** `@SpringBootTest` — including the
/// `contextLoads` test that shipped with the project and never touches a
/// database — so a starter with none of the wiring fails `mvn verify` on a
/// test nobody wrote, with "Failed to determine a suitable driver class".
#[test]
fn canonical_storage_postgres_writes_the_container_compose_and_datasource() {
    let root = temp_dir("canonical-db-wiring");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/model.jdl"),
        "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  \
         platform spring\n  build maven\n  storage postgres\n}\n",
    )
    .unwrap();
    let generated = jails_cmd(&root, None)
        .args(["g", "scaffold", "Task", "id:uuid@pk", "title:string!"])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );

    // The container is a `@Bean` with `@ServiceConnection`, not a
    // `@Testcontainers`/`@Container` static field: Spring caches the context
    // past the container's JUnit-managed lifetime, and later tests then fail
    // against a stopped container.
    let container =
        fs::read_to_string(root.join("src/test/java/com/example/demo/TestcontainersConfig.java"))
            .unwrap();
    assert!(container.contains("@ServiceConnection"), "{container}");
    assert!(container.contains("@TestConfiguration"), "{container}");
    assert!(!container.contains("{{"), "{container}");

    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    // Testcontainers 2.0 renamed every module, so the coordinate is the new
    // one and it is pinned -- the Boot parent does not manage these.
    assert!(pom.contains("testcontainers-postgresql"), "{pom}");
    assert!(pom.contains("spring-boot-testcontainers"), "{pom}");

    let properties =
        fs::read_to_string(root.join("src/main/resources/application.properties")).unwrap();
    assert!(
        properties.contains("spring.datasource.url=jdbc:postgresql://localhost:5432/app"),
        "{properties}"
    );
    // Not tuning: JDBC auto-config CGLIB-proxies every `@Repository` for
    // exception translation and fails on a `final` class, and jails writes raw
    // SQL with no ORM for it to translate.
    assert!(
        properties.contains("spring.persistence.exceptiontranslation.enabled=false"),
        "{properties}"
    );
    // Also not tuning: jails starts compose itself, and Boot's module shells
    // out with Docker Compose v2 syntax podman-compose rejects.
    assert!(
        properties.contains("spring.docker.compose.enabled=false"),
        "{properties}"
    );

    let compose = fs::read_to_string(root.join("compose.yaml")).unwrap();
    assert!(compose.contains("postgres:17-alpine"), "{compose}");
    assert!(compose.contains("pg_isready"), "{compose}");

    // **The half that decides whether `mvn verify` passes.** The
    // `contextLoads` test `jails new` wrote never touches a database, and
    // without the container imported into it JDBC auto-configuration fails the
    // context with "Failed to determine a suitable driver class".
    let shipped = common::read_generated(
        &root,
        "src/test/java/com/example/demo/DemoApplicationTests.java",
    );
    assert!(
        shipped.contains("@Import(TestcontainersConfig.class)"),
        "{shipped}"
    );
    assert!(
        shipped.contains("import org.springframework.context.annotation.Import;"),
        "{shipped}"
    );
    // Same package as the config, so there is no import statement for it --
    // importing a sibling does not compile.
    assert!(
        !shipped.contains("import com.example.demo.TestcontainersConfig;"),
        "{shipped}"
    );

    // Splicing twice must not stack: the annotation is rewritten member by
    // member rather than appended.
    let before = shipped.clone();
    let resync = jails_cmd(&root, None).arg("sync").output().unwrap();
    assert!(
        resync.status.success(),
        "{}",
        String::from_utf8_lossy(&resync.stderr)
    );
    assert_eq!(
        common::read_generated(
            &root,
            "src/test/java/com/example/demo/DemoApplicationTests.java"
        ),
        before
    );
}

/// The command that declares a capability is the command that wires it.
///
/// Capture decides which reader trees to read from the *intended* model, not
/// the one on disk: on the command that *introduces* `storage postgres` the
/// shipped `contextLoads` test has to be visible for the `@Import` splice to
/// land. A second, unrelated command would repair the omission, which is why
/// each command is asserted on its own.
#[test]
fn canonical_add_db_wires_the_shipped_test_on_the_command_that_declares_it() {
    let root = temp_dir("canonical-db-declaring-command");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/model.jdl"),
        "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  \
         platform spring\n  build maven\n  storage none\n}\n",
    )
    .unwrap();

    let added = jails_cmd(&root, None)
        .args(["add", "db", "--no-start"])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );

    let shipped = common::read_generated(
        &root,
        "src/test/java/com/example/demo/DemoApplicationTests.java",
    );
    assert!(
        shipped.contains("@Import(TestcontainersConfig.class)"),
        "the shipped test was not wired by the command that declared storage:\n{shipped}"
    );
}

/// `remove <storage>` is the exact inverse of `add <storage>`: `add h2` on a
/// JDL v1 project sets `storage h2` rather than appending `cap h2` -- storage
/// is an axis in v1 and the closed capability registry has no `h2` in it --
/// so removal clears the axis rather than looking for a declaration `add`
/// deliberately did not write.
#[test]
fn canonical_storage_add_and_remove_are_inverses() {
    let root = temp_dir("canonical-storage-inverse");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    let source = "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  \
         platform spring\n  build maven\n  storage none\n}\n";
    fs::write(root.join(".jails/model.jdl"), source).unwrap();

    // `h2`, not `db`. Both are axis kinds, but removing `db` is refused by a
    // *different* rule -- it would
    // abandon accepted storage, and retiring tables is an explicit schema
    // policy with its own tests. Asserting through it here would be asserting
    // two things at once and reporting the wrong one when either moved.
    //
    // `sqlite` is deliberately a *capability* in v1: the registry carries
    // `cap sqlite` beside `storage sqlite` and the linker materializes the
    // axis from it, so routing `add sqlite` to the axis would change what a
    // working command writes. It round-trips below, by the ordinary path.
    {
        let storage = "h2";
        let added = jails_cmd(&root, None)
            .args(["add", storage])
            .output()
            .unwrap();
        assert!(
            added.status.success(),
            "`jails add {storage}`: {}",
            String::from_utf8_lossy(&added.stderr)
        );
        let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
        assert!(
            !model.contains(&format!("cap {storage}")),
            "`add {storage}` wrote a capability where v1 has an axis:\n{model}"
        );

        let removed = jails_cmd(&root, None)
            .args(["remove", storage, "--force"])
            .output()
            .unwrap();
        assert!(
            removed.status.success(),
            "`jails remove {storage}`: {}",
            String::from_utf8_lossy(&removed.stderr)
        );
        // Back to `none`, and `none` rather than an absent line: `storage` is
        // a required member of `app`, so an axis with no value is not a model.
        let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
        assert!(
            model.contains("storage none"),
            "after `remove {storage}`:\n{model}"
        );
    }

    // The capability spelling round-trips too, by the ordinary path. Asserted
    // beside the axis so the difference between them is visible rather than
    // implied.
    let added = jails_cmd(&root, None)
        .args(["add", "sqlite"])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    assert!(
        fs::read_to_string(root.join(".jails/model.jdl"))
            .unwrap()
            .contains("cap sqlite")
    );
    let removed = jails_cmd(&root, None)
        .args(["remove", "sqlite", "--force"])
        .output()
        .unwrap();
    assert!(
        removed.status.success(),
        "{}",
        String::from_utf8_lossy(&removed.stderr)
    );
    assert!(
        !fs::read_to_string(root.join(".jails/model.jdl"))
            .unwrap()
            .contains("cap sqlite")
    );
}

/// Every orthogonal capability pack, in one project, built once.
///
/// Each `canonical_*_pack_*` test above proves what its capability *writes* --
/// the merged tree, the ejected boundary, the properties the reader keeps --
/// which is filesystem work costing milliseconds. Whether the result compiles
/// is proved here, once: the cost of a Maven run is per *project*, not per
/// assertion, because the Spring context boot that dominates it is cached per
/// configuration inside one JVM, measured.
///
/// It is also the *stronger* check, for the reason `spring-core-toolbox`
/// gives: a capability that only contradicts another in company -- two owners
/// of `management.endpoints.web.exposure.include`, two beans qualifying for
/// one injection point -- cannot be caught by a suite where every pack sits
/// alone in its own project.
///
/// The dialect-specific packs (`h2`, `sqlite`) and the resource-entangled ones
/// (`csv`/`json`, `testkit`, `toxiproxy`, `security`) keep their own build.
/// They are not orthogonal to this set, and folding them in would mean
/// asserting less rather than more.
///
/// `redis`, `kafka` and `mail` are excluded. Each registers a Spring health
/// indicator, and `actuator` exposes health --
/// so in one project the actuator test asks a `MailHealthIndicator` for its
/// verdict, it tries `localhost:1025`, and the build fails on
/// `MailConnectException` rather than on anything either capability got wrong.
/// They are orthogonal to each other but not to this set, so they belong in a
/// second toolbox of their own rather than in this one.
///
/// `coverage` is excluded for a different reason, and a sharper one. Its
/// check is `verify`, where JaCoCo enforces a *ratio over the whole project*.
/// That is not a property of the capability at all: drop five other packs'
/// largely-untested generated code into the same tree and the same coverage
/// rule fails, having measured something no capability got wrong. A threshold
/// over a shared project measures the project, so it has to keep its own.
#[test]
fn every_orthogonal_capability_pack_compiles_and_tests_in_one_project() {
    if !real_mvn_available() || !real_java_supports_target_release() {
        common::skip("real Maven and a JDK that accepts TARGET_RELEASE");
        return;
    }
    let root = temp_dir("model-orthogonal-capability-toolbox");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();

    for capability in [
        "actuator",
        "cache",
        "cors",
        "observability",
        "sse",
        "security",
        "csv",
        "json",
        "sqlite",
        "fake",
    ] {
        let added = jails_cmd(&root, None)
            .args(["add", capability])
            .output()
            .unwrap();
        assert!(
            added.status.success(),
            "add {capability} failed in the orthogonal capability toolbox:\n{}",
            String::from_utf8_lossy(&added.stderr)
        );
    }

    // The per-pack tests each assert the tree *after* ejecting their own
    // boundary, so this has to eject too or it would be proving a different
    // tree.
    for artifact in [
        "cap_actuator",
        "cap_cache",
        "cap_cors",
        "cap_observability",
        "cap_sse",
        "cap_security",
        "cap_csv",
        "cap_sqlite",
    ] {
        let ejected = jails_cmd(&root, None)
            .args(["model", "eject", artifact])
            .output()
            .unwrap();
        assert!(
            ejected.status.success(),
            "model eject {artifact} failed in the orthogonal capability toolbox:\n{}",
            String::from_utf8_lossy(&ejected.stderr)
        );
    }

    let path = real_path_without_mvnd();
    let built = real_maven_cmd(&root, &path)
        .args(["-q", "-B", "test"])
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "the orthogonal capability packs did not compile and test together:\n{}\n{}",
        String::from_utf8_lossy(&built.stdout),
        String::from_utf8_lossy(&built.stderr)
    );
}

/// The three capabilities that register a health indicator, in one project.
///
/// `redis`, `kafka` and `mail` are excluded from
/// `every_orthogonal_capability_pack_compiles_and_tests_in_one_project`
/// because each contributes a Spring health indicator and `actuator` exposes
/// health: together they make the actuator test ask a `MailHealthIndicator`
/// for a verdict, it dials `localhost:1025`, and the build fails on
/// `MailConnectException` having proved nothing about either capability.
///
/// `redis` and `kafka` are orthogonal to *each other*, so they share a project
/// of their own with no `actuator` in it.
///
/// `mail` keeps its own build. Its check is `verify`, not `test`, so folding
/// it in here would mean running `verify` -- which runs the Failsafe `*IT`s,
/// and `redis` generates a `KeyValueStoreIT` that starts a Testcontainers
/// Redis. `mail` alone needs no container; `mail` beside `redis` under
/// `verify` does, and merging them would quietly add a Docker requirement to
/// a test that has none.
#[test]
fn the_health_indicator_capability_packs_compile_and_test_in_one_project() {
    if !real_mvn_available() || !real_java_supports_target_release() {
        common::skip("real Maven and a JDK that accepts TARGET_RELEASE");
        return;
    }
    let root = temp_dir("model-health-indicator-capability-toolbox");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();

    for capability in ["redis", "kafka"] {
        let added = jails_cmd(&root, None)
            .args(["add", capability, "--no-start"])
            .output()
            .unwrap();
        assert!(
            added.status.success(),
            "add {capability} failed in the health-indicator toolbox:\n{}",
            String::from_utf8_lossy(&added.stderr)
        );
    }

    let path = real_path_without_mvnd();
    let built = real_maven_cmd(&root, &path)
        .args(["-q", "-B", "test"])
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "the health-indicator capability packs did not compile and test together:\n{}\n{}",
        String::from_utf8_lossy(&built.stdout),
        String::from_utf8_lossy(&built.stderr)
    );
}
