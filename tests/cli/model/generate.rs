//! The component kinds over the model -- record, enum, scaffold and the
//! generators that emit one boundary each -- and the field syntax they read.
//!
use super::*;

/// A source-only record evolves on a project that also has a database.
///
/// Storedness is the entity's, not the project's: `Note` has no table, so its
/// new field is a pure source change and emits no migration, while `Task`
/// keeps the backfill contract.
#[test]
fn a_source_only_record_gains_a_field_in_a_project_that_has_a_database() {
    let root = jdl_project(
        "jdl-v1-source-only-field",
        r#"jdl 1
app Notes {
 pkg com.example.notes
 java 26
 platform spring
 build maven
 storage postgres
}
entity Task {
 id: uuid @pk
 title: string
 use repo
}
entity Note {
 title: string
}
"#,
    );
    write_spring_fixture(&root);

    let evolved = jails_cmd(&root, None)
        .args(["g", "field", "Note", "done:boolean"])
        .output()
        .unwrap();
    assert!(
        evolved.status.success(),
        "{}",
        String::from_utf8_lossy(&evolved.stderr)
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(model.contains("done: boolean"), "{model}");

    // No table, so no migration: the only one is the stored entity's create.
    let migrations = root.join("src/main/resources/db/migration");
    let mut names: Vec<String> = fs::read_dir(&migrations)
        .map(|entries| {
            entries
                .filter_map(|entry| Some(entry.ok()?.file_name().to_string_lossy().into_owned()))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    assert!(
        names.iter().all(|name| !name.contains("note")),
        "a source-only record emitted schema: {names:?}"
    );

    // A backfill for rows that do not exist is still refused.
    let refused = jails_cmd(&root, None)
        .args([
            "g",
            "field",
            "Note",
            "flag:boolean",
            "--default-literal",
            "true",
        ])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    let told = String::from_utf8_lossy(&refused.stderr);
    assert!(
        told.contains("source-only record has no rows to backfill"),
        "{told}"
    );

    // And the stored entity keeps its contract.
    let stored = jails_cmd(&root, None)
        .args(["g", "field", "Task", "extra:string"])
        .output()
        .unwrap();
    assert!(!stored.status.success());
    let told = String::from_utf8_lossy(&stored.stderr);
    assert!(
        told.contains("needs a backfill for existing rows"),
        "{told}"
    );
}

/// A strategy is the one Spring-shaped unit a plain project also gets: a
/// service and a controller are an annotation with a class around them, and
/// refuse without a captured Spring Boot project, while a strategy is the
/// same layout with no annotation.
///
/// The port, the implementations and the evaluator all compile without Spring:
/// `@Component` is only how Spring *collects* the implementations into the
/// evaluator's `List<Port>`, and without it the reader passes the list to the
/// constructor the evaluator already has. `@Order` goes the same way, and the
/// evaluator's own Javadoc already says the caller's order decides the answer.
#[test]
fn a_strategy_lowers_without_spring_on_a_plain_project() {
    let root = jdl_project(
        "jdl-v1-plain-strategy",
        r#"jdl 1
app Ledger {
 pkg com.example.ledger
 java 26
 platform plain
 build maven
 storage none
}
entity Tx {
 id: uuid @pk
 amount: long
}
"#,
    );
    write_plain_fixture(&root);

    let generated = jails_cmd(&root, None)
        .args(["g", "strategy", "Elig", "Domestic", "--on", "Tx"])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );

    let base = root.join(".jails/generated/main/java/com/example/ledger");
    let port = fs::read_to_string(base.join("domain/Elig.java")).unwrap();
    assert!(port.contains("public interface Elig"), "{port}");

    let implementation = fs::read_to_string(base.join("service/DomesticElig.java")).unwrap();
    assert!(
        implementation.contains("public final class DomesticElig implements Elig"),
        "{implementation}"
    );
    assert!(
        !implementation.contains("org.springframework"),
        "a plain project got a Spring import: {implementation}"
    );
    assert!(
        !implementation.contains("@Component") && !implementation.contains("@Order"),
        "a plain project got a Spring annotation: {implementation}"
    );

    // The evaluator still takes the whole set, which is what makes the plain
    // shape work at all: without component scanning the reader passes it.
    let evaluator = fs::read_to_string(base.join("service/EligEvaluator.java")).unwrap();
    assert!(
        evaluator.contains("public EligEvaluator(List<Elig> eligs)"),
        "{evaluator}"
    );
    assert!(
        !evaluator.contains("org.springframework"),
        "a plain project got a Spring import: {evaluator}"
    );
}

/// A CLI name in lower camel case is the Java type it names: `jails g enum
/// currency GBP EUR` writes `Currency.java`, and every later generator saying
/// `currency:Currency` resolves against it.
///
/// Resolved in the frontend rather than by loosening the model: `java_name`
/// is a projection the model is right to hold to, and resolving what the
/// reader typed is what CLI sugar is for.
#[test]
fn a_lower_camel_name_becomes_the_java_type_it_names() {
    let root = jdl_project(
        "jdl-v1-lower-camel-name",
        r#"jdl 1
app Gym {
 pkg com.example.gym
 java 26
 platform plain
 build maven
 storage none
}
"#,
    );
    write_plain_fixture(&root);

    for arguments in [
        &["g", "enum", "currency", "GBP", "EUR"][..],
        &["g", "record", "sourceRef", "system:string"][..],
    ] {
        let output = jails_cmd(&root, None).args(arguments).output().unwrap();
        assert!(
            output.status.success(),
            "{arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let base = root.join(".jails/generated/main/java/com/example/gym/domain");
    assert!(base.join("Currency.java").is_file());
    assert!(base.join("SourceRef.java").is_file());

    // And the point of the capitalisation: another generator can name those
    // types, which is what it means for generators to compose.
    let composed = jails_cmd(&root, None)
        .args(["g", "value", "money", "amount:long", "currency:Currency"])
        .output()
        .unwrap();
    assert!(
        composed.status.success(),
        "{}",
        String::from_utf8_lossy(&composed.stderr)
    );
    let value = fs::read_to_string(base.join("Money.java")).unwrap();
    assert!(value.contains("Currency currency"), "{value}");
}

/// Every generated domain type ships a test, and which test is a fact about
/// the type: emitting a guess produces a test that does not compile, and
/// emitting nothing leaves the suite green over a type nobody asserted
/// anything about.
///
/// Three shapes here, and the third is the one that is easy to get wrong: a
/// component jails cannot sample disables the *class*, because every
/// construction in the file would fail to compile.
#[test]
fn every_generated_domain_type_ships_the_test_its_shape_allows() {
    let root = jdl_project(
        "jdl-v1-companion-tests",
        r#"jdl 1
app Demo {
 pkg com.example.demo
 java 26
 platform plain
 build maven
 storage none
}
"#,
    );
    write_plain_fixture(&root);
    // A type the reader owns and jails knows nothing about: it exists, so the
    // record naming it compiles, and jails cannot invent a value for it, so
    // the companion test below has to say so rather than guess.
    fs::write(
        root.join("src/main/java/com/example/demo/SomeUnknown.java"),
        "package com.example.demo;\n\npublic final class SomeUnknown {}\n",
    )
    .unwrap();

    for arguments in [
        &["g", "record", "Note", "title:string!", "count:int"][..],
        &["g", "record", "Plain", "count:int", "amount:long"][..],
        &["g", "record", "Odd", "ref:SomeUnknown"][..],
        &["g", "enum", "Status", "OPEN", "CLOSED"][..],
    ] {
        let output = jails_cmd(&root, None).args(arguments).output().unwrap();
        assert!(
            output.status.success(),
            "{arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let tests = root.join(".jails/generated/test/java/com/example/demo/domain");

    // A component that can be null-checked gives the test something real.
    let note = fs::read_to_string(tests.join("NoteTest.java")).unwrap();
    assert!(note.contains("void rejectsANullComponent()"), "{note}");
    assert!(note.contains("new Note(null, 1)"), "{note}");
    assert!(note.contains(r#"contains("title")"#), "{note}");
    assert!(!note.contains("@Disabled"), "{note}");

    // None to check: disabled, and saying what to write instead rather than
    // asserting that javac generated an accessor.
    let plain = fs::read_to_string(tests.join("PlainTest.java")).unwrap();
    assert!(
        plain.contains("@Disabled(\"todo: state what Plain guarantees"),
        "{plain}"
    );
    assert!(plain.contains("new Plain(1, 1L)"), "{plain}");

    // A type jails cannot build disables the class: the constructor call in
    // the body would not compile, so no half of the file still runs.
    let odd = fs::read_to_string(tests.join("OddTest.java")).unwrap();
    assert!(
        odd.contains("@Disabled(\"todo: supply a sample for ref"),
        "{odd}"
    );

    // An enum can be asked three things without knowing the domain, and
    // `valueOf` throwing rather than returning null is the one worth pinning.
    let status = fs::read_to_string(tests.join("StatusTest.java")).unwrap();
    for assertion in [
        "assertEquals(Status.OPEN, Status.valueOf(\"OPEN\"))",
        "assertThrows(IllegalArgumentException.class",
        "assertEquals(2, Status.values().length)",
    ] {
        assert!(status.contains(assertion), "{assertion} missing:\n{status}");
    }

    // JUnit's own assertions, not AssertJ: a project is not guaranteed to
    // declare AssertJ, and the other test emitters write JUnit. A companion
    // test that drags in a dependency would
    // be a generator supplying one for a file the reader did not ask for.
    for source in [&note, &plain, &odd, &status] {
        assert!(!source.contains("assertj"), "{source}");
    }
}

#[test]
fn familiar_field_syntax_materializes_typed_jdl_v1_semantics_end_to_end() {
    let root = jdl_project(
        "jdl-v1-familiar-field-semantics",
        r#"jdl 1
app Notes {
  pkg com.example.notes
  java 26
  platform spring
  build maven
  storage postgres
}
"#,
    );
    write_spring_fixture(&root);
    let generated = jails_cmd(&root, None)
        .args([
            "g",
            "scaffold",
            "Task",
            "id:long@pk",
            "title:string!(1..200)",
            "tenantId:uuid@scope",
            "attempts:int@positive",
            "credits:decimal?@nonnegative",
            "externalId:string@column(external_id)",
            "--timestamps",
        ])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );

    let source = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    for expected in [
        "tenantId: uuid @id(fld_task_tenant_id) @scope",
        "attempts: int @id(fld_task_attempts) @positive",
        "credits: decimal? @id(fld_task_credits) @nonnegative",
        "externalId: string @id(fld_task_external_id) @map(\"external_id\")",
        "createdAt: instant @id(fld_task_created_at) @default(now())",
        "updatedAt: instant @id(fld_task_updated_at) @default(now()) @updated",
    ] {
        assert!(source.contains(expected), "missing `{expected}`:\n{source}");
    }
    let format_check = jails_cmd(&root, None)
        .args(["model", "fmt", "--check"])
        .output()
        .unwrap();
    assert!(
        format_check.status.success(),
        "familiar field edit was not canonical JDL v1:\n{}",
        String::from_utf8_lossy(&format_check.stderr)
    );
    let linked = jails_model::parse_jdl(&source).unwrap();
    let task = linked
        .entities
        .values()
        .find(|entity| entity.label == "task")
        .unwrap();
    let field = |label: &str| {
        task.fields
            .iter()
            .find(|field| field.label == label)
            .unwrap()
    };
    assert!(field("tenant_id").semantics.scope.is_some());
    assert!(field("attempts").semantics.positive);
    assert!(field("credits").semantics.nonnegative);
    assert_eq!(field("external_id").names.sql_column, "external_id");
    assert!(field("created_at").semantics.default.is_some());
    assert!(field("updated_at").semantics.updated);

    let record = fs::read_to_string(
        root.join(".jails/generated/main/java/com/example/notes/domain/Task.java"),
    )
    .unwrap();
    assert!(record.contains("attempts must be positive"), "{record}");
    assert!(record.contains("credits.isPresent()"), "{record}");
    let migration =
        fs::read_to_string(root.join("src/main/resources/db/migration/V001__create_tasks.sql"))
            .unwrap();
    assert!(migration.contains("attempts > 0"), "{migration}");
    assert!(migration.contains("credits >= 0"), "{migration}");
    assert!(
        migration.contains("check (char_length(title) between 1 and 200)"),
        "{migration}"
    );
    assert!(
        migration.contains("generated always as identity"),
        "{migration}"
    );
    assert!(
        migration.contains("default current_timestamp"),
        "{migration}"
    );

    if real_mvn_available() && real_java_supports_target_release() {
        common::assert_main_sources_compile(
            &root,
            &real_path_without_mvnd(),
            "generated rich-field project",
        );
    }
}

#[test]
fn familiar_record_generation_is_a_model_patch_in_canonical_projects() {
    let root = model_project("model-generate-record", EMPTY_MODEL);
    let preview = jails_cmd(&root, None)
        .args([
            "g",
            "record",
            "Note",
            "title:string!",
            "body:string?",
            "at:instant",
            "--pretend",
        ])
        .output()
        .unwrap();
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    assert_eq!(
        fs::read_to_string(root.join(".jails/model.jdl")).unwrap(),
        EMPTY_MODEL
    );
    assert!(!root.join(".jails/generated").exists());

    let applied = jails_cmd(&root, None)
        .args([
            "g",
            "record",
            "Note",
            "title:string!",
            "body:string?",
            "at:instant",
        ])
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(model.contains("entity Note @id(ent_note)"), "{model}");
    assert!(model.contains("@id(fld_note_title)"), "{model}");

    let source = fs::read_to_string(
        root.join(".jails/generated/main/java/com/example/notes/domain/Note.java"),
    )
    .unwrap();
    assert!(source.contains("Optional<String> body"), "{source}");
    assert!(source.contains("Objects.requireNonNull(title"), "{source}");
    assert!(source.contains("title = title.trim()"), "{source}");
    assert!(source.contains("title must not be blank"), "{source}");

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
fn record_compiler_preserves_the_legacy_semantic_contract() {
    let legacy = temp_dir("model-record-legacy");
    write_spring_fixture(&legacy);
    let legacy_output = jails_cmd(&legacy, None)
        .args([
            "g",
            "record",
            "Note",
            "title:string!",
            "body:string?",
            "at:instant",
        ])
        .output()
        .unwrap();
    assert!(
        legacy_output.status.success(),
        "{}",
        String::from_utf8_lossy(&legacy_output.stderr)
    );

    let compiled = model_project("model-record-compiled", EMPTY_MODEL);
    let compiled_output = jails_cmd(&compiled, None)
        .args([
            "g",
            "record",
            "Note",
            "title:string!",
            "body:string?",
            "at:instant",
        ])
        .output()
        .unwrap();
    assert!(
        compiled_output.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled_output.stderr)
    );

    let old = common::read_generated(&legacy, "src/main/java/com/example/demo/domain/Note.java");
    let new = fs::read_to_string(
        compiled.join(".jails/generated/main/java/com/example/notes/domain/Note.java"),
    )
    .unwrap();
    for contract in [
        "Optional<String> body",
        "Objects.requireNonNull(title",
        "Objects.requireNonNull(at",
        "Objects.requireNonNullElse(body, Optional.empty())",
        "title = title.trim()",
        "title must not be blank",
    ] {
        assert!(
            old.contains(contract),
            "legacy oracle lost `{contract}`:\n{old}"
        );
        assert!(
            new.contains(contract),
            "new compiler lost `{contract}`:\n{new}"
        );
    }
}

/// A record's components come out in the order the entity declared them.
///
/// JDL v1 §7.3 lists entity fields first among the orders that MUST be
/// retained, and the reason is not aesthetic -- a caller compiled against the
/// positional constructor keeps compiling against a re-sorted one and
/// silently passes the wrong arguments.
///
/// The labels here sort differently from the way they are written on purpose;
/// alphabetical and declaration order have to disagree or this proves nothing.
#[test]
fn a_record_keeps_the_field_order_its_entity_declares() {
    let root = jdl_project(
        "model-field-order",
        r#"jdl 1

app Ord {
  pkg com.example.ord
  java 26
  platform plain
  build maven
  storage none
}

entity Task {
  zulu:  string
  id:    uuid   @pk
  alpha: string
}
"#,
    );
    write_plain_fixture(&root);
    let synced = jails_cmd(&root, None).args(["sync"]).output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );

    let record = fs::read_to_string(
        root.join(".jails/generated/main/java/com/example/ord/domain/Task.java"),
    )
    .unwrap();
    let components = record
        .split_once("public record Task(")
        .expect("a record declaration")
        .1
        .split_once(')')
        .expect("a closed parameter list")
        .0;
    let order = components
        .split(',')
        .map(|component| {
            component
                .split_whitespace()
                .next_back()
                .expect("a component name")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(order, ["zulu", "id", "alpha"], "{record}");
}

#[test]
fn canonical_enum_frontend_writes_a_typed_wire_vocabulary() {
    let root = model_project("model-enum", EMPTY_MODEL);
    let generated = jails_cmd(&root, None)
        .args(["g", "enum", "Status", "OPEN", "IN_PROGRESS=in_progress"])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(model.contains("enum Status @id(ent_status)"), "{model}");
    assert!(model.contains("  OPEN\n"), "{model}");
    assert!(model.contains(r#"IN_PROGRESS = "in_progress""#), "{model}");
    let source = fs::read_to_string(
        root.join(".jails/generated/main/java/com/example/notes/domain/Status.java"),
    )
    .unwrap();
    assert!(source.contains("IN_PROGRESS(\"in_progress\")"), "{source}");
    assert!(source.contains("Status fromWire(String value)"), "{source}");
}

#[test]
fn familiar_scaffold_generation_is_one_semantic_entity_profile() {
    let root = model_project("model-generate-scaffold", EMPTY_MODEL);
    let applied = jails_cmd(&root, None)
        .args([
            "g",
            "scaffold",
            "Note",
            "id:uuid@pk",
            "title:string!",
            "--timestamps",
        ])
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );

    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(model.contains("use scaffold"), "{model}");
    assert!(model.contains("id: uuid @id(fld_note_id) @pk"), "{model}");
    assert!(model.contains("@id(fld_note_created_at)"), "{model}");
    assert!(model.contains("@id(fld_note_updated_at)"), "{model}");

    for relative in [
        "domain/Note.java",
        "repository/NoteRepository.java",
        "service/NoteService.java",
        "ports/http/NoteHttpPort.java",
    ] {
        assert!(
            root.join(".jails/generated/main/java/com/example/notes")
                .join(relative)
                .is_file(),
            "semantic scaffold profile did not emit {relative}"
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
fn scaffold_profile_preserves_the_legacy_domain_and_repository_contracts() {
    let legacy = temp_dir("model-scaffold-legacy");
    write_spring_fixture(&legacy);
    let legacy_output = jails_cmd(&legacy, None)
        .args(["g", "scaffold", "Note", "id:uuid@pk", "title:string!"])
        .output()
        .unwrap();
    assert!(
        legacy_output.status.success(),
        "{}",
        String::from_utf8_lossy(&legacy_output.stderr)
    );

    let compiled = model_project("model-scaffold-compiled", EMPTY_MODEL);
    let compiled_output = jails_cmd(&compiled, None)
        .args(["g", "scaffold", "Note", "id:uuid@pk", "title:string!"])
        .output()
        .unwrap();
    assert!(
        compiled_output.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled_output.stderr)
    );

    let old_record =
        common::read_generated(&legacy, "src/main/java/com/example/demo/domain/Note.java");
    let new_record = fs::read_to_string(
        compiled.join(".jails/generated/main/java/com/example/notes/domain/Note.java"),
    )
    .unwrap();
    for contract in [
        "UUID id",
        "String title",
        "Objects.requireNonNull(title",
        "title = title.trim()",
        "title must not be blank",
    ] {
        assert!(old_record.contains(contract), "legacy lost `{contract}`");
        assert!(new_record.contains(contract), "compiler lost `{contract}`");
    }

    let old_repository = common::read_generated(
        &legacy,
        "src/main/java/com/example/demo/app/NoteRepository.java",
    );
    let new_repository = fs::read_to_string(
        compiled
            .join(".jails/generated/main/java/com/example/notes/repository/NoteRepository.java"),
    )
    .unwrap();
    for contract in [
        "Optional<Note> findById(UUID id)",
        "List<Note> findAll()",
        "Note save(Note note)",
        "boolean deleteById(UUID id)",
    ] {
        assert!(
            old_repository.contains(contract),
            "legacy lost `{contract}`"
        );
        assert!(
            new_repository.contains(contract),
            "compiler lost `{contract}`"
        );
    }
}

#[test]
fn a_canonical_scaffold_serves_its_resource_over_http() {
    let root = jdl_project(
        "jdl-v1-scaffold-http",
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
        vec!["g", "scaffold", "Note", "id:long@pk", "title:string!"],
    ] {
        let output = jails_cmd(&root, None).args(&arguments).output().unwrap();
        assert!(
            output.status.success(),
            "{arguments:?}:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let web = root.join(".jails/generated/main/java/com/example/demo/web");
    let controller = fs::read_to_string(web.join("NoteController.java")).unwrap();
    assert!(controller.contains("@RestController"), "{controller}");
    // The table name, not a second pluraliser: a route that does not match the
    // table it reads is the drift `sql_table` has one owner to prevent.
    assert!(
        controller.contains("public static final String PATH = \"/notes\";"),
        "{controller}"
    );
    for method in [
        "public List<Note> list()",
        "public ResponseEntity<Note> byId(@PathVariable(\"id\") long id)",
        // The request record, not the domain row: the caller is asked for
        // what they own and the server mints the rest.
        "public ResponseEntity<Note> create(@Valid @RequestBody NoteRequest request)",
        "public ResponseEntity<Void> delete(@PathVariable(\"id\") long id)",
    ] {
        assert!(
            controller.contains(method),
            "missing {method}:\n{controller}"
        );
    }
    // The service, not the repository port: the suite jails generates beside
    // this controller forbids a `*Controller` depending on the repository
    // package, so injecting the port would fail a freshly scaffolded project's
    // own `ArchitectureTest`.
    assert!(controller.contains("NoteService service"), "{controller}");
    assert!(
        !controller.contains("NoteRepository"),
        "the controller reaches past the service into persistence:\n{controller}"
    );

    // The port stays: it is managed ABI, whatever now serves the resource.
    assert!(
        root.join(".jails/generated/main/java/com/example/demo/ports/http/NoteHttpPort.java")
            .is_file()
    );

    let test = fs::read_to_string(
        root.join(".jails/generated/test/java/com/example/demo/web/NoteControllerTest.java"),
    )
    .unwrap();
    // An anonymous class, not a lambda: the port has four methods and is not a
    // functional interface.
    assert!(test.contains("new NoteRepository() {"), "{test}");
    assert!(
        test.contains("MockMvcTester.of(new NoteController("),
        "{test}"
    );

    // The starter that serves HTTP is declared, or the controller is a compile
    // error for a file the reader did not write.
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("spring-boot-starter-web"), "{pom}");
}

/// The scaffold's controller and its test compile and pass on real Maven.
///
/// The file-level assertions cannot tell a controller Spring can dispatch from
/// one it cannot: a wrong `@PathVariable` binding, a route that collides, a
/// body Jackson will not serialise all compile.
#[test]
fn canonical_scaffold_http_compiles_and_passes_on_real_maven() {
    if !real_mvn_available() || !real_java_supports_target_release() {
        common::skip("real Maven and a JDK that accepts TARGET_RELEASE");
        return;
    }
    let root = jdl_project(
        "jdl-v1-scaffold-http-maven",
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
        vec![
            "g",
            "scaffold",
            "Note",
            "id:long@pk",
            "title:string!",
            "seenAt:instant@default(now())",
        ],
    ] {
        let output = jails_cmd(&root, None).args(&arguments).output().unwrap();
        assert!(
            output.status.success(),
            "{arguments:?}:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let tested = real_maven_cmd(&root, &real_path_without_mvnd())
        .args(["-q", "-B", "-Dtest=NoteControllerTest", "test"])
        .output()
        .unwrap();
    assert!(
        tested.status.success(),
        "the generated scaffold controller failed real Maven:\n{}\n{}",
        String::from_utf8_lossy(&tested.stdout),
        String::from_utf8_lossy(&tested.stderr)
    );
    assert_eq!(
        maven_report_summary(&root, "surefire-reports"),
        MavenReportSummary {
            reports: 1,
            tests: 1,
            failures: 0,
            errors: 0,
            skipped: 0,
        }
    );
}

/// `g client` on a canonical project.
///
/// The three files are one declaration and the two the project cannot start
/// without are the ones easiest to forget: the `spring-boot-starter-restclient`
/// dependency and the base-url property. `@ImportHttpServices` builds the
/// proxies without either, so the project compiles, starts, and dies on the
/// first call with `URI with undefined scheme` -- a message that names neither.
#[test]
fn canonical_client_emits_its_registration_dependency_and_base_url() {
    let root = temp_dir("canonical-client");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/model.jdl"),
        "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  \
         platform spring\n  build maven\n  storage none\n}\n",
    )
    .unwrap();

    let generated = jails_cmd(&root, None)
        .args(["g", "client", "Ledger"])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );

    let source = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(source.contains("component client Ledger"), "{source}");

    for relative in [
        ".jails/generated/main/java/com/example/demo/clients/LedgerClient.java",
        ".jails/generated/main/java/com/example/demo/clients/LedgerClientConfig.java",
        ".jails/generated/test/java/com/example/demo/clients/LedgerClientTest.java",
    ] {
        assert!(root.join(relative).exists(), "`{relative}` was not written");
    }

    let interface = fs::read_to_string(
        root.join(".jails/generated/main/java/com/example/demo/clients/LedgerClient.java"),
    )
    .unwrap();
    assert!(
        interface.contains("package com.example.demo.clients;"),
        "{interface}"
    );
    assert!(
        interface.contains("public interface LedgerClient"),
        "{interface}"
    );
    // The collection route the frontend materialized.
    assert!(
        interface.contains("@GetExchange(\"/ledgers\")"),
        "{interface}"
    );
    assert!(source.contains("route GET \"/ledgers\""), "{source}");
    // Nothing of the template's own placeholder syntax survives into the Java.
    assert!(!interface.contains("{{"), "{interface}");

    let config = fs::read_to_string(
        root.join(".jails/generated/main/java/com/example/demo/clients/LedgerClientConfig.java"),
    )
    .unwrap();
    assert!(
        config.contains("@ImportHttpServices(group = \"ledger\", types = LedgerClient.class)"),
        "{config}"
    );

    // The half that is silent when it is missing.
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("spring-boot-starter-restclient"), "{pom}");
    // The properties go into the reader's own file, spliced key by key -- they
    // are a `ReconcileProperties` document intent, not a managed file.
    let properties =
        fs::read_to_string(root.join("src/main/resources/application.properties")).unwrap();
    assert!(
        properties.contains("spring.http.serviceclient.ledger.base-url=https://example.invalid"),
        "{properties}"
    );
    assert!(
        properties.contains("spring.http.serviceclient.ledger.connect-timeout=2s"),
        "{properties}"
    );
    assert!(
        properties.contains("spring.http.serviceclient.ledger.read-timeout=5s"),
        "{properties}"
    );

    // Compiling again writes the same bytes: the emitter is a function of the
    // model, not of what it found on disk.
    let synced = jails_cmd(&root, None).arg("sync").output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );
    assert_eq!(
        fs::read_to_string(
            root.join(".jails/generated/main/java/com/example/demo/clients/LedgerClient.java")
        )
        .unwrap(),
        interface
    );
}

/// `g fetcher` on a canonical project.
///
/// The generated adapter is the whole artifact: fetching a URL a caller
/// supplies is the one outbound call that can be aimed at the host it runs on,
/// so every bound it pins is asserted here. A fetcher that lost its redirect
/// limit or its content-type list would still compile and still pass a
/// happy-path test.
#[test]
fn canonical_fetcher_emits_the_bounds_that_make_it_safe() {
    let root = temp_dir("canonical-fetcher");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/model.jdl"),
        "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  \
         platform spring\n  build maven\n  storage none\n}\n",
    )
    .unwrap();

    let generated = jails_cmd(&root, None)
        .args(["g", "fetcher", "Page"])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );

    for relative in [
        ".jails/generated/main/java/com/example/demo/clients/PageFetcher.java",
        ".jails/generated/main/java/com/example/demo/clients/SafePageFetcher.java",
        ".jails/generated/test/java/com/example/demo/clients/SafePageFetcherTest.java",
    ] {
        assert!(root.join(relative).exists(), "`{relative}` was not written");
    }

    let adapter = fs::read_to_string(
        root.join(".jails/generated/main/java/com/example/demo/clients/SafePageFetcher.java"),
    )
    .unwrap();
    assert!(!adapter.contains("{{"), "{adapter}");
    for bound in [
        "jails.fetchers.page.connect-timeout",
        "jails.fetchers.page.response-timeout",
        "jails.fetchers.page.max-response-size",
        "jails.fetchers.page.max-redirects",
        "jails.fetchers.page.allowed-content-types",
    ] {
        assert!(
            adapter.contains(bound),
            "`{bound}` is missing from\n{adapter}"
        );
    }

    // Both dependencies are load-bearing: the JDK client follows a redirect to
    // a private address without asking, and the adapter's counters need a
    // registry to record into.
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("httpclient5"), "{pom}");
    assert!(pom.contains("spring-boot-starter-actuator"), "{pom}");
}

/// `g job` on a canonical project, twice.
///
/// The second job is the point. `SchedulingConfig` belongs to every job in the
/// model rather than to one, so it is emitted once — and a managed tree
/// refuses two units writing the same path, which is exactly what a per-job
/// emitter would have done on the second declaration.
#[test]
fn canonical_jobs_share_one_scheduling_config() {
    let root = temp_dir("canonical-job");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/model.jdl"),
        "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  \
         platform spring\n  build maven\n  storage none\n}\n",
    )
    .unwrap();

    for name in ["Reconcile", "Expire"] {
        let generated = jails_cmd(&root, None)
            .args(["g", "job", name])
            .output()
            .unwrap();
        assert!(
            generated.status.success(),
            "`g job {name}`: {}",
            String::from_utf8_lossy(&generated.stderr)
        );
    }

    for relative in [
        ".jails/generated/main/java/com/example/demo/jobs/ReconcileJob.java",
        ".jails/generated/main/java/com/example/demo/jobs/ExpireJob.java",
        ".jails/generated/main/java/com/example/demo/jobs/SchedulingConfig.java",
        ".jails/generated/test/java/com/example/demo/jobs/ReconcileJobTest.java",
        ".jails/generated/test/java/com/example/demo/jobs/ExpireJobTest.java",
    ] {
        assert!(root.join(relative).exists(), "`{relative}` was not written");
    }

    // The default `spring.task.scheduling.pool.size` is 1, so a second job
    // waits for the first however unrelated they are -- and nothing reports
    // it, the jobs simply do not run. This file is generated to fix that, so
    // the fix is what is asserted.
    let config = fs::read_to_string(
        root.join(".jails/generated/main/java/com/example/demo/jobs/SchedulingConfig.java"),
    )
    .unwrap();
    assert!(!config.contains("{{"), "{config}");
    assert!(
        config.contains("package com.example.demo.jobs;"),
        "{config}"
    );
}

/// `g socket` and `g webhook` on a canonical project.
///
/// Both are inbound surfaces that split their framework half from their
/// testable half, and both put something in the build the project cannot work
/// without: the WebSocket starter is not in the web starter, and a webhook's
/// shared secret is a property with no default, because one that silently
/// defaults is a webhook anybody can call.
#[test]
fn canonical_socket_and_webhook_split_their_framework_half_from_their_testable_one() {
    let root = temp_dir("canonical-socket-webhook");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/model.jdl"),
        "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  \
         platform spring\n  build maven\n  storage none\n}\n",
    )
    .unwrap();

    for command in [
        [
            "g", "socket", "Chat", "--path", "/ws/chat", "--method", "get",
        ]
        .as_slice(),
        ["g", "webhook", "Stripe"].as_slice(),
    ] {
        let output = jails_cmd(&root, None).args(command).output().unwrap();
        assert!(
            output.status.success(),
            "`jails {}`: {}",
            command.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    for relative in [
        ".jails/generated/main/java/com/example/demo/web/ChatSocketHandler.java",
        ".jails/generated/main/java/com/example/demo/web/ChatSocketConfig.java",
        ".jails/generated/test/java/com/example/demo/web/ChatSocketHandlerTest.java",
        ".jails/generated/main/java/com/example/demo/StripeVerifier.java",
        ".jails/generated/main/java/com/example/demo/web/StripeWebhookController.java",
        ".jails/generated/test/java/com/example/demo/StripeVerifierTest.java",
    ] {
        assert!(root.join(relative).exists(), "`{relative}` was not written");
    }

    let config = fs::read_to_string(
        root.join(".jails/generated/main/java/com/example/demo/web/ChatSocketConfig.java"),
    )
    .unwrap();
    assert!(config.contains("/ws/chat"), "{config}");
    assert!(!config.contains("{{"), "{config}");

    // The controller lives in `web` and the verifier in the base package, so
    // the import is real rather than a sibling one that would not compile.
    let controller = fs::read_to_string(
        root.join(".jails/generated/main/java/com/example/demo/web/StripeWebhookController.java"),
    )
    .unwrap();
    assert!(
        controller.contains("import com.example.demo.StripeVerifier;"),
        "{controller}"
    );
    assert!(controller.contains("X-Stripe-Signature"), "{controller}");
    assert!(!controller.contains("{{"), "{controller}");

    let verifier = fs::read_to_string(
        root.join(".jails/generated/main/java/com/example/demo/StripeVerifier.java"),
    )
    .unwrap();
    assert!(verifier.contains("app.stripe.secret"), "{verifier}");

    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("spring-boot-starter-websocket"), "{pom}");
}

/// `g auth` on a canonical project, refused first and then served.
///
/// The refusal is half the feature: the encoder, the decoder and the filter
/// chain that reads the token are one story, and two thirds of it live in
/// `cap security`. It is checked against the model rather than the pom,
/// because in one transition a capability this same model declares has not
/// reached the build file yet.
///
/// The `exp` claim is the other half. `JwtTimestampValidator` accepts a token
/// without one, so an issuer that forgets it mints credentials that never
/// expire and the application works -- which is why the generated test is what
/// keeps the fix in place.
#[test]
fn canonical_auth_refuses_without_security_then_pins_the_expiry_nothing_else_would() {
    let root = temp_dir("canonical-auth");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/model.jdl"),
        "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  \
         platform spring\n  build maven\n  storage none\n}\n",
    )
    .unwrap();

    let refused = jails_cmd(&root, None)
        .args(["g", "auth", "Api"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains("needs Spring Security"), "{stderr}");
    assert!(stderr.contains("cap security"), "{stderr}");
    assert!(
        !root
            .join(".jails/generated/main/java/com/example/demo/ApiTokens.java")
            .exists(),
        "the refusal wrote a file"
    );

    let secured = jails_cmd(&root, None)
        .args(["add", "security"])
        .output()
        .unwrap();
    assert!(
        secured.status.success(),
        "{}",
        String::from_utf8_lossy(&secured.stderr)
    );
    let generated = jails_cmd(&root, None)
        .args(["g", "auth", "Api"])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );

    let tokens =
        fs::read_to_string(root.join(".jails/generated/main/java/com/example/demo/ApiTokens.java"))
            .unwrap();
    assert!(!tokens.contains("{{"), "{tokens}");
    assert!(tokens.contains("urn:com.example.demo"), "{tokens}");
    let test = fs::read_to_string(
        root.join(".jails/generated/test/java/com/example/demo/ApiTokensTest.java"),
    )
    .unwrap();
    assert!(test.contains("exp"), "{test}");
}

/// `g idempotency` on a canonical project.
///
/// The migration is the part with a rule of its own: it is an irreproducible
/// operation, so it is appended once for a guard that is new and never again.
/// A second `sync` that re-emitted it would append a `create table` the next
/// `flyway migrate` fails on, and the failure would arrive in production
/// rather than here.
#[test]
fn canonical_idempotency_appends_its_table_once_and_only_when_new() {
    let root = temp_dir("canonical-idempotency");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/model.jdl"),
        "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  \
         platform spring\n  build maven\n  storage none\n}\n",
    )
    .unwrap();

    // Receipts that do not outlive a restart are not receipts.
    let refused = jails_cmd(&root, None)
        .args(["g", "idempotency", "Payment"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains("keep receipts across restarts"), "{stderr}");

    let stored = jails_cmd(&root, None)
        .args(["add", "db", "--no-start"])
        .output()
        .unwrap();
    assert!(
        stored.status.success(),
        "{}",
        String::from_utf8_lossy(&stored.stderr)
    );
    let generated = jails_cmd(&root, None)
        .args(["g", "idempotency", "Payment"])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );

    for relative in [
        ".jails/generated/main/java/com/example/demo/domain/PaymentReceipt.java",
        ".jails/generated/main/java/com/example/demo/application/PaymentReceipts.java",
        ".jails/generated/main/java/com/example/demo/adapters/jdbc/JdbcPaymentReceipts.java",
        ".jails/generated/main/java/com/example/demo/service/PaymentGuard.java",
        ".jails/generated/test/java/com/example/demo/service/PaymentGuardTest.java",
    ] {
        assert!(root.join(relative).exists(), "`{relative}` was not written");
    }

    let migrations = |root: &Path| {
        let directory = root.join("src/main/resources/db/migration");
        let mut names: Vec<String> = fs::read_dir(&directory)
            .map(|entries| {
                entries
                    .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    };
    let after_generate = migrations(&root);
    assert!(
        after_generate
            .iter()
            .any(|name| name.contains("create_payment_receipts")),
        "{after_generate:?}"
    );

    // The claim the guard is built on, which select-then-insert would lose.
    let store = fs::read_to_string(root.join(
        ".jails/generated/main/java/com/example/demo/adapters/jdbc/JdbcPaymentReceipts.java",
    ))
    .unwrap();
    assert!(store.contains("on conflict do nothing"), "{store}");
    assert!(store.contains("payment_receipts"), "{store}");
    assert!(!store.contains("{{"), "{store}");

    // Compiling again must not append a second `create table`.
    let synced = jails_cmd(&root, None).arg("sync").output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );
    assert_eq!(migrations(&root), after_generate);
}

/// `g handler` on a canonical project, twice, and on a plain one.
///
/// It is the framework-free HTTP kind, so the test uses a `platform plain`
/// project — the case it exists for. The second handler is the point again:
/// `ApiError` belongs to every handler in the model, and a per-handler
/// emitter would have compiled and then refused the second declaration.
#[test]
fn canonical_handlers_share_one_error_envelope_without_a_framework() {
    let root = temp_dir("canonical-handler");
    write_plain_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/model.jdl"),
        "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  \
         platform plain\n  build maven\n  storage none\n}\n",
    )
    .unwrap();

    for name in ["WorkItem", "Note"] {
        let generated = jails_cmd(&root, None)
            .args(["g", "handler", name])
            .output()
            .unwrap();
        assert!(
            generated.status.success(),
            "`g handler {name}`: {}",
            String::from_utf8_lossy(&generated.stderr)
        );
    }

    for relative in [
        ".jails/generated/main/java/com/example/demo/api/WorkItemHandler.java",
        ".jails/generated/main/java/com/example/demo/api/NoteHandler.java",
        ".jails/generated/main/java/com/example/demo/domain/ApiError.java",
        ".jails/generated/test/java/com/example/demo/domain/ApiErrorTest.java",
        ".jails/generated/test/java/com/example/demo/api/WorkItemHandlerTest.java",
    ] {
        assert!(root.join(relative).exists(), "`{relative}` was not written");
    }

    let handler = fs::read_to_string(
        root.join(".jails/generated/main/java/com/example/demo/api/WorkItemHandler.java"),
    )
    .unwrap();
    assert!(!handler.contains("{{"), "{handler}");
    // Nothing framework-shaped: the JDK's own handler is the whole surface.
    assert!(
        handler.contains("com.sun.net.httpserver.HttpHandler"),
        "{handler}"
    );
    assert!(!handler.contains("org.springframework"), "{handler}");
    assert!(
        handler.contains("import com.example.demo.domain.ApiError;"),
        "{handler}"
    );

    // The envelope is rendered from the shared template, so this is the shape
    // the golden trees already pin.
    let envelope = fs::read_to_string(
        root.join(".jails/generated/main/java/com/example/demo/domain/ApiError.java"),
    )
    .unwrap();
    assert!(
        envelope.contains(
            "public record ApiError(String code, String message, Map<String, String> details)"
        ),
        "{envelope}"
    );
}

/// `g presence` on a canonical project.
///
/// Presence held in one process's memory is correct on one node and wrong on
/// two, with nothing to say which — so PostgreSQL is a precondition, and the
/// refusal is asserted before the artifacts are.
///
/// It also shares `SchedulingConfig` with `g job`, because its sweep is
/// scheduled: without `@EnableScheduling` the annotation is inert and nothing
/// says so, and the table just grows a row per crashed node forever.
#[test]
fn canonical_presence_refuses_without_storage_then_shares_the_scheduler() {
    let root = temp_dir("canonical-presence");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/model.jdl"),
        "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  \
         platform spring\n  build maven\n  storage none\n}\n",
    )
    .unwrap();

    let refused = jails_cmd(&root, None)
        .args(["g", "presence", "Online"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("correct on one node and wrong on two"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );

    for command in [
        ["add", "db", "--no-start"].as_slice(),
        ["g", "presence", "Online"].as_slice(),
        ["g", "job", "Reconcile"].as_slice(),
    ] {
        let output = jails_cmd(&root, None).args(command).output().unwrap();
        assert!(
            output.status.success(),
            "`jails {}`: {}",
            command.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    for relative in [
        ".jails/generated/main/java/com/example/demo/application/OnlinePresence.java",
        ".jails/generated/main/java/com/example/demo/adapters/jdbc/JdbcOnlinePresence.java",
        ".jails/generated/test/java/com/example/demo/adapters/jdbc/JdbcOnlinePresenceIT.java",
        // One config, and presence declared it before the job did.
        ".jails/generated/main/java/com/example/demo/jobs/SchedulingConfig.java",
    ] {
        assert!(root.join(relative).exists(), "`{relative}` was not written");
    }

    let store =
        fs::read_to_string(root.join(
            ".jails/generated/main/java/com/example/demo/adapters/jdbc/JdbcOnlinePresence.java",
        ))
        .unwrap();
    assert!(store.contains("online_presence"), "{store}");
    assert!(!store.contains("{{"), "{store}");

    // The integration test imports the container the model already knows it
    // has, rather than being `@Disabled` for want of reading the test tree.
    let integration = fs::read_to_string(root.join(
        ".jails/generated/test/java/com/example/demo/adapters/jdbc/JdbcOnlinePresenceIT.java",
    ))
    .unwrap();
    assert!(
        integration.contains("@Import(TestcontainersConfig.class)"),
        "{integration}"
    );
    assert!(!integration.contains("@Disabled"), "{integration}");
    assert!(!integration.contains("{{"), "{integration}");

    // A departure is a delete, so a row exists only while somebody is there.
    let migration = fs::read_dir(root.join("src/main/resources/db/migration"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.to_string_lossy().contains("create_online_presence"))
        .expect("the presence table was not migrated");
    let sql = fs::read_to_string(&migration).unwrap();
    assert!(sql.contains("primary key (scope, member, node)"), "{sql}");
    assert!(!sql.contains("left_at"), "{sql}");
}

/// `g cli` and `g command` on a canonical project.
///
/// Two reader-owned files are edited here and both are surgical: the command
/// registers itself in the dispatcher rather than leaving a paste instruction
/// in a Javadoc, and the packaged jar is pointed at the new dispatcher — but
/// only because the POM still names the `App` stub jails wrote and that stub
/// registers nothing. A project with two dispatchers has two `main` methods,
/// and a search of the source picks whichever the walk reaches first, which is
/// how a jar and `jails run` came to start different classes.
#[test]
fn canonical_cli_registers_its_commands_and_claims_the_entry_point() {
    let root = temp_dir("canonical-cli");
    write_plain_fixture(&root);
    // The fixture declares no entry point, and a POM that names none is a
    // decision jails leaves alone. This one names the `App` stub, which is the
    // only case the claim applies to.
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    fs::write(
        root.join("pom.xml"),
        pom.replace(
            "<properties>",
            "<properties>\n        <mainClass>com.example.demo.App</mainClass>",
        ),
    )
    .unwrap();
    fs::create_dir_all(common::generated(&root, "src/main/java/com/example/demo")).unwrap();
    fs::write(
        common::generated(&root, "src/main/java/com/example/demo/App.java"),
        "package com.example.demo;\n\n\
         import java.util.SequencedMap;\n\n\
         public final class App {\n\
         \x20   static SequencedMap<String, Command> commands() {\n\
         \x20       var commands = new java.util.LinkedHashMap<String, Command>();\n\
         \x20       return commands;\n\
         \x20   }\n\
         }\n",
    )
    .unwrap();
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/model.jdl"),
        "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  \
         platform plain\n  build maven\n  storage none\n}\n",
    )
    .unwrap();

    // **Each command is checked on its own, and that is the whole point.**
    // Running both and then reading the tree passes over a compiler that
    // ignores the patch entirely: `g command` would repair `g cli`'s omission
    // from the model it left behind, and the second-to-last command would
    // always look like it worked. Each edit has to land on the command that
    // declares it.
    let generated_cli = jails_cmd(&root, None)
        .args(["g", "cli", "Admin"])
        .output()
        .unwrap();
    assert!(
        generated_cli.status.success(),
        "{}",
        String::from_utf8_lossy(&generated_cli.stderr)
    );
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(
        pom.contains("<mainClass>com.example.demo.cli.AdminCli</mainClass>"),
        "the jar still starts the stub the CLI replaced:\n{pom}"
    );

    let generated_command = jails_cmd(&root, None)
        .args(["g", "command", "Greet"])
        .output()
        .unwrap();
    assert!(
        generated_command.status.success(),
        "{}",
        String::from_utf8_lossy(&generated_command.stderr)
    );
    let app = common::read_generated(&root, "src/main/java/com/example/demo/App.java");
    assert!(
        app.contains("commands.put(GreetCommand.NAME, GreetCommand::run);"),
        "the command did not register itself on the command that declared it:\n{app}"
    );

    for relative in [
        ".jails/generated/main/java/com/example/demo/cli/AdminCli.java",
        ".jails/generated/test/java/com/example/demo/cli/AdminCliTest.java",
        ".jails/generated/main/java/com/example/demo/cli/GreetCommand.java",
        ".jails/generated/test/java/com/example/demo/cli/GreetCommandTest.java",
    ] {
        assert!(root.join(relative).exists(), "`{relative}` was not written");
    }

    // The dispatcher is in another package, so the registration needs an
    // import statement as well as the line.
    assert!(
        app.contains("import com.example.demo.cli.GreetCommand;"),
        "{app}"
    );

    // Splicing twice must not stack.
    let before = app.clone();
    let resync = jails_cmd(&root, None).arg("sync").output().unwrap();
    assert!(
        resync.status.success(),
        "{}",
        String::from_utf8_lossy(&resync.stderr)
    );
    assert_eq!(
        common::read_generated(&root, "src/main/java/com/example/demo/App.java"),
        before
    );
}

/// `g search` and `g seed` write the projection the compiler reads.
///
/// The interesting half is `search`: it is the only projection carrying an
/// argument, because *which*
/// components are indexed is a decision rather than a derivation. A `tsvector`
/// over every text column indexes ids and status codes as if they were prose,
/// and the reader then cannot tell why a search for "active" returns
/// everything.
#[test]
fn canonical_search_and_seed_write_their_projections_and_compile() {
    let root = temp_dir("canonical-search-seed");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/model.jdl"),
        "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  \
         platform spring\n  build maven\n  storage postgres\n}\n",
    )
    .unwrap();
    for command in [
        [
            "g",
            "scaffold",
            "Note",
            "id:uuid@pk",
            "title:string",
            "body:string",
        ]
        .as_slice(),
        ["add", "json"].as_slice(),
        ["g", "search", "Note", "title", "body"].as_slice(),
        ["g", "seed", "Note"].as_slice(),
    ] {
        let output = jails_cmd(&root, None).args(command).output().unwrap();
        assert!(
            output.status.success(),
            "`jails {}`: {}",
            command.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(
        model.contains("use search(fields: [title, body])"),
        "{model}"
    );
    assert!(model.contains("use seed"), "{model}");

    let generated = root.join(".jails/generated");
    let read = |relative: &str| {
        fs::read_to_string(generated.join(relative))
            .unwrap_or_else(|error| panic!("{relative}: {error}"))
    };
    // Search: the port, its JDBC adapter, and the generated column both use.
    let adapter = read("main/java/com/example/demo/adapters/jdbc/JdbcNoteSearch.java");
    assert!(adapter.contains("websearch_to_tsquery"), "{adapter}");
    read("main/java/com/example/demo/ports/search/NoteSearch.java");
    // Seed: the data, the guarded loader, and the test that reads it.
    read("main/resources/db/seeds/notes.json");
    let seeder = read("main/java/com/example/demo/adapters/NoteSeeder.java");
    assert!(seeder.contains("@Profile(\"seed\")"), "{seeder}");
    read("test/java/com/example/demo/adapters/NoteSeederTest.java");

    let migrations = fs::read_dir(root.join("src/main/resources/db/migration"))
        .unwrap()
        .map(|entry| fs::read_to_string(entry.unwrap().path()).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    // The indexed columns are the two named, not every text column.
    assert!(migrations.contains("search_vector"), "{migrations}");
    assert!(migrations.contains("coalesce(title, '')"), "{migrations}");
    assert!(!migrations.contains("coalesce(id, '')"), "{migrations}");

    // A component the entity does not have is caught here rather than at
    // `flyway migrate`, which is the furthest point from the mistake.
    let refused = jails_cmd(&root, None)
        .args(["g", "search", "Note", "headline"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("`Note` has no component `headline`"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
}

/// `g association` writes the relation the compiler already lowers.
///
/// **The declaration goes in the child**, because the foreign key column
/// does. A relation named on the parent would read as ownership and compile
/// to a column on the wrong table; `map <child> -> <parent>` says which way
/// round it goes and the block's position says whose column it is.
#[test]
fn canonical_association_writes_a_relation_and_its_foreign_key() {
    let root = temp_dir("canonical-association");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/model.jdl"),
        "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  \
         platform spring\n  build maven\n  storage postgres\n}\n",
    )
    .unwrap();
    for command in [
        ["g", "scaffold", "Owner", "id:uuid@pk", "name:string"].as_slice(),
        [
            "g",
            "scaffold",
            "Note",
            "id:uuid@pk",
            "ownerId:uuid",
            "title:string",
        ]
        .as_slice(),
        [
            "g",
            "association",
            "Owner",
            "ownerId=id",
            "--on",
            "Note",
            "--yields",
            "Owner",
        ]
        .as_slice(),
    ] {
        let output = jails_cmd(&root, None).args(command).output().unwrap();
        assert!(
            output.status.success(),
            "`jails {}`: {}",
            command.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // The block is inside `entity Note`, the child -- not `entity Owner`.
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    let note = model
        .split("entity Note")
        .nth(1)
        .expect("the child entity is declared");
    // lowerCamel, because a relation is a member and not a type -- and the
    // mapping is spelled the way the field list three lines above spells it.
    assert!(note.contains("relation owner to Owner"), "{model}");
    assert!(note.contains("map ownerId -> id"), "{model}");

    let migrations = fs::read_dir(root.join("src/main/resources/db/migration"))
        .unwrap()
        .map(|entry| fs::read_to_string(entry.unwrap().path()).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(migrations.contains("foreign key"), "{migrations}");
    assert!(migrations.contains("references owner"), "{migrations}");

    // A column that is not on the side it names is caught here rather than at
    // `flyway migrate`, which is the furthest point from the mistake.
    let refused = jails_cmd(&root, None)
        .args([
            "g",
            "association",
            "Missing",
            "ownerId=headline",
            "--on",
            "Note",
            "--yields",
            "Owner",
        ])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("is not a field on `owner`"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
}
