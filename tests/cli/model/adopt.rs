//! `adopt resource`: a type the reader wrote, registered in the model as an
//! entity whose record is theirs, so the commands that evolve a declared
//! entity work on it and none of them writes the reader's file.
//!
use super::*;

/// A record a reader wrote before jails knew the project, in the package the
/// convention would have chosen.
const MESSAGE: &str = "package com.example.notes.domain;\n\nimport java.time.Instant;\nimport java.util.Optional;\nimport java.util.UUID;\n\n/** A message somebody wrote by hand. */\npublic record Message(UUID id, String title, Optional<String> body, Instant at, int hits) {}\n";

/// [`NOTES_JDL`] beside one reader-owned record at `relative`.
fn adopting_project(label: &str, relative: &str, source: &str) -> PathBuf {
    let root = model_project(label, NOTES_JDL);
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, source).unwrap();
    root
}

const MESSAGE_PATH: &str = "src/main/java/com/example/notes/domain/Message.java";

fn adopt(root: &Path, name: &str) -> std::process::Output {
    jails_cmd(root, None)
        .args(["adopt", "resource", name])
        .output()
        .unwrap()
}

#[test]
fn adopt_resource_registers_a_hand_written_record_as_the_readers_own() {
    let root = adopting_project("adopt-resource", MESSAGE_PATH, MESSAGE);
    let model_before = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    let before = snapshot_tree(&root);

    // The preview says what it read and writes nothing but the bundle it
    // was asked for.
    let bundle = temp_dir("adopt-resource-bundle").join("preview.json");
    let preview = jails_cmd(&root, None)
        .args(["adopt", "resource", "Message", "--pretend", "--plan-out"])
        .arg(&bundle)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&preview.stdout).to_string();
    assert!(
        preview.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    assert!(stdout.contains("field   body:string?"), "{stdout}");
    assert!(stdout.contains("field   hits:int"), "{stdout}");
    assert!(stdout.contains("nothing was written"), "{stdout}");
    assert_eq!(snapshot_tree(&root), before, "the preview wrote something");

    let applied = adopt(&root, "Message");
    assert!(
        applied.status.success(),
        "{}{}",
        String::from_utf8_lossy(&applied.stdout),
        String::from_utf8_lossy(&applied.stderr)
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(model.starts_with(&model_before), "{model}");
    assert!(
        model.contains("entity Message @id(ent_message) {"),
        "{model}"
    );
    assert!(
        model.contains("  id: uuid @id(fld_message_id)\n"),
        "{model}"
    );
    assert!(
        model.contains("  body: string? @id(fld_message_body)\n"),
        "{model}"
    );
    assert!(
        model.contains("  at: instant @id(fld_message_at)\n"),
        "{model}"
    );
    assert!(
        model.contains("  hits: int @id(fld_message_hits)\n"),
        "{model}"
    );
    assert!(
        model.contains("eject Message.record @id(eject_") && model.contains(") @adopted\n"),
        "{model}"
    );
    // The reader's file is untouched, and jails rendered neither a record
    // nor a companion test over a type it did not write.
    assert_eq!(
        fs::read_to_string(root.join(MESSAGE_PATH)).unwrap(),
        MESSAGE
    );
    // Managed output lives beside the reader's sources, so "jails did not
    // render it" is the lock's answer: the accepted projection never names
    // the reader's path.
    let lock = fs::read_to_string(root.join(".jails/compiler.lock.json")).unwrap();
    assert!(
        !lock.contains("domain/Message.java"),
        "the lock claims the reader's record: {lock}"
    );
    assert!(
        !root
            .join("src/test/java/com/example/notes/domain/MessageTest.java")
            .exists()
    );
    // The file rode in the plan as an exact input -- a `Present`
    // precondition with its digest -- and never as an output.
    let plan: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&bundle).unwrap()).unwrap();
    let precondition = &plan["plan"]["base"]["files"][MESSAGE_PATH];
    assert!(
        precondition["Present"]["digest"]
            .as_str()
            .is_some_and(|digest| digest.starts_with("sha256:")),
        "{precondition}"
    );
    let outputs = plan["plan"]["operations"].to_string();
    assert!(!outputs.contains(MESSAGE_PATH), "{outputs}");

    // Adopting twice is a no-op with a sentence, and the model is unchanged.
    let again = adopt(&root, "Message");
    assert!(again.status.success());
    let stdout = String::from_utf8_lossy(&again.stdout).to_string();
    assert!(stdout.contains("already adopted"), "{stdout}");
    assert_eq!(
        fs::read_to_string(root.join(".jails/model.jdl")).unwrap(),
        model
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
}

/// The model records where the record is: a package the convention would
/// not have chosen is pinned rather than moved.
#[test]
fn adopt_resource_pins_a_record_outside_the_domain_layer_and_passes_project_types_through() {
    let root = adopting_project(
        "adopt-resource-package",
        "src/main/java/com/example/notes/model/Message.java",
        "package com.example.notes.model;\n\npublic record Message(String title, Priority priority, java.util.Optional<Priority> fallback) {}\n",
    );
    fs::write(
        root.join("src/main/java/com/example/notes/model/Priority.java"),
        "package com.example.notes.model;\n\npublic enum Priority { LOW, HIGH }\n",
    )
    .unwrap();
    let applied = adopt(&root, "Message");
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(
        model.contains("entity Message @id(ent_message) @package(model) {"),
        "{model}"
    );
    assert!(
        model.contains("  priority: Priority @id(fld_message_priority)\n"),
        "{model}"
    );
    assert!(
        model.contains("  fallback: Priority? @id(fld_message_fallback)\n"),
        "{model}"
    );
    let lock = fs::read_to_string(root.join(".jails/compiler.lock.json")).unwrap();
    assert!(
        !lock.contains("model/Message.java"),
        "the lock claims the reader's record: {lock}"
    );
}

/// Each refusal names what was found and what to do, and writes nothing.
#[test]
fn adopt_resource_refuses_by_name_and_writes_nothing() {
    let root = adopting_project("adopt-resource-refusals", MESSAGE_PATH, MESSAGE);
    let base = root.join("src/main/java/com/example/notes");
    // A component whose type no row of the table renders to.
    fs::write(
        base.join("domain/Clock.java"),
        "package com.example.notes.domain;\n\npublic record Clock(java.time.LocalTime at) {}\n",
    )
    .unwrap();
    // A type with nothing to read.
    fs::write(
        base.join("domain/Empty.java"),
        "package com.example.notes.domain;\n\npublic interface Empty {}\n",
    )
    .unwrap();
    // One simple name, two files.
    fs::create_dir_all(base.join("web")).unwrap();
    fs::write(
        base.join("web/Twin.java"),
        "package com.example.notes.web;\n\npublic record Twin(String a) {}\n",
    )
    .unwrap();
    fs::write(
        base.join("domain/Twin.java"),
        "package com.example.notes.domain;\n\npublic record Twin(String a) {}\n",
    )
    .unwrap();
    let before = snapshot_tree(&root);

    for (name, expected) in [
        ("Missing", "no `Missing.java` under src/main/java"),
        ("Clock", "component `at` of `Clock` has type `LocalTime`"),
        (
            "Empty",
            "declares no record components or constructor parameters",
        ),
        ("Twin", "`Twin.java` is declared in more than one place"),
    ] {
        let refused = adopt(&root, name);
        assert!(!refused.status.success(), "{name} was adopted");
        let stderr = String::from_utf8_lossy(&refused.stderr).to_string();
        assert!(stderr.contains(expected), "{name}: {stderr}");
        assert!(stderr.contains("fix:"), "{name}: {stderr}");
    }
    assert_eq!(snapshot_tree(&root), before, "a refusal wrote something");

    // Generating over the reader's record is the collision the reader has
    // to resolve: the render wants the path their file holds.
    let collided = jails_cmd(&root, None)
        .args(["g", "record", "Message", "title:string!"])
        .output()
        .unwrap();
    assert!(
        !collided.status.success(),
        "generated over the reader's file"
    );
    assert!(
        String::from_utf8_lossy(&collided.stderr).contains("already reader-owned"),
        "{}",
        String::from_utf8_lossy(&collided.stderr)
    );
    // And a type jails already renders is not adopted over.
    fs::remove_file(root.join(MESSAGE_PATH)).unwrap();
    let generated = jails_cmd(&root, None)
        .args(["g", "record", "Message", "title:string!"])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let refused = adopt(&root, "Message");
    assert!(!refused.status.success());
    let stderr = String::from_utf8_lossy(&refused.stderr).to_string();
    assert!(stderr.contains("already declared"), "{stderr}");
    assert!(stderr.contains("jails destroy record Message"), "{stderr}");
}

/// The point of the command: an adopted entity is one the resource commands
/// evolve, rename and remove exactly as they do a generated one -- and none
/// of them writes the reader's record.
#[test]
fn an_adopted_resource_evolves_renames_and_destroys_like_a_generated_one() {
    let root = adopting_project("adopt-resource-lifecycle", MESSAGE_PATH, MESSAGE);
    let adopted = adopt(&root, "Message");
    assert!(
        adopted.status.success(),
        "{}",
        String::from_utf8_lossy(&adopted.stderr)
    );

    let added = jails_cmd(&root, None)
        .args(["resource", "field", "add", "Message", "summary:string!"])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(
        model.contains("  summary: string @id(fld_message_summary) @notBlank\n"),
        "{model}"
    );
    assert_eq!(
        fs::read_to_string(root.join(MESSAGE_PATH)).unwrap(),
        MESSAGE
    );

    // A generated operation compiles against the reader's record, and holds
    // the entity in place until it goes.
    let query = jails_cmd(&root, None)
        .args(["g", "query", "OpenMessages", "title", "--on", "Message"])
        .output()
        .unwrap();
    assert!(
        query.status.success(),
        "{}",
        String::from_utf8_lossy(&query.stderr)
    );
    let port = fs::read_to_string(
        root.join("src/main/java/com/example/notes/application/queries/OpenMessagesQuery.java"),
    )
    .unwrap();
    assert!(
        port.contains("import com.example.notes.domain.Message;"),
        "{port}"
    );
    let refused = jails_cmd(&root, None)
        .args(["destroy", "record", "Message", "--force"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    let stderr = String::from_utf8_lossy(&refused.stderr).to_string();
    assert!(stderr.contains("operation OpenMessages"), "{stderr}");
    assert!(stderr.contains("pointing at nothing"), "{stderr}");
    let removed = jails_cmd(&root, None)
        .args(["destroy", "query", "OpenMessages", "--force"])
        .output()
        .unwrap();
    assert!(removed.status.success());

    // A rename is the model following the reader, not leading them: while
    // their file still says `Message` the rename refuses naming it, and
    // once they have renamed it the model moves and the boundary path
    // moves with it.
    let refused = jails_cmd(&root, None)
        .args([
            "rename",
            "resource",
            "Message",
            "Note",
            "--strategy",
            "preserve-table",
        ])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    let stderr = String::from_utf8_lossy(&refused.stderr).to_string();
    assert!(stderr.contains("manual-edit-required"), "{stderr}");
    assert!(stderr.contains(MESSAGE_PATH), "{stderr}");
    let note_path = "src/main/java/com/example/notes/domain/Note.java";
    fs::write(
        root.join(note_path),
        MESSAGE.replace("record Message(", "record Note("),
    )
    .unwrap();
    fs::remove_file(root.join(MESSAGE_PATH)).unwrap();
    let renamed = jails_cmd(&root, None)
        .args([
            "rename",
            "resource",
            "Message",
            "Note",
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
    assert!(model.contains("entity Note @id(ent_message) {"), "{model}");
    assert!(model.contains("eject Note.record @id(eject_"), "{model}");
    assert!(
        !root
            .join(".jails/generated/main/java/com/example/notes/domain/Note.java")
            .exists()
    );

    // Destroy is subtraction: the declaration and its adopted line go, the
    // reader's file stays, and the tree is exactly what it was.
    let destroyed = jails_cmd(&root, None)
        .args(["destroy", "record", "Note", "--force"])
        .output()
        .unwrap();
    assert!(
        destroyed.status.success(),
        "{}",
        String::from_utf8_lossy(&destroyed.stderr)
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(!model.contains("entity Note"), "{model}");
    assert!(!model.contains("eject "), "{model}");
    assert!(root.join(note_path).is_file());
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

/// Tier 3: the adopted record is the one the generated code compiles
/// against. A hand-written record in a Spring fixture is adopted, a query is
/// generated over it, a field is added on both sides -- jails' half in the
/// model, the reader's half in their file -- and `jails check` passes.
#[test]
fn an_adopted_record_is_what_generated_code_compiles_against() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    if !real_java_supports_target_release() {
        skip("the JDK on PATH does not accept the target release");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("adopt-resource-real");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();
    let record = root.join("src/main/java/com/example/demo/domain/Message.java");
    fs::create_dir_all(record.parent().unwrap()).unwrap();
    fs::write(
        &record,
        MESSAGE.replace("com.example.notes", "com.example.demo"),
    )
    .unwrap();

    for arguments in [
        vec!["adopt", "resource", "Message"],
        vec!["g", "query", "OpenMessages", "title", "--on", "Message"],
        vec!["resource", "field", "add", "Message", "summary:string?"],
    ] {
        let output = jails_cmd_with_path(&root, &path)
            .args(&arguments)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    // The reader's half of the field: jails does not write their record.
    let source = fs::read_to_string(&record).unwrap();
    assert!(!source.contains("summary"), "{source}");
    fs::write(
        &record,
        source.replace("int hits)", "int hits, Optional<String> summary)"),
    )
    .unwrap();

    let checked = jails_cmd_with_path(&root, &path)
        .arg("check")
        .output()
        .unwrap();
    assert!(
        checked.status.success(),
        "jails check: {}\n{}",
        String::from_utf8_lossy(&checked.stdout),
        String::from_utf8_lossy(&checked.stderr)
    );
    assert!(
        root.join("target/classes/com/example/demo/application/queries/OpenMessagesQuery.class")
            .is_file()
    );
    assert!(
        !root
            .join("target/classes/com/example/demo/domain/MessageTest.class")
            .exists()
    );
}
