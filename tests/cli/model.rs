use super::*;

/// The smallest `jdl 1` model a Spring fixture can carry, for the tests that
/// build everything else with `jails g`.
///
/// `storage none`: every scenario that wants storage adds it with `add db` or
/// `add h2`, and a seed that declared it would hand forty tests a JDBC
/// adapter and a migration none of them asked for.
const DEMO_JDL: &str = "jdl 1\n\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  \
     java 26\n  platform spring\n  build maven\n  storage none\n}\n";

/// [`DEMO_JDL`] for the fixtures whose package is `com.example.notes`.
const NOTES_JDL: &str = "jdl 1\n\napp Notes @id(project_notes) {\n  pkg com.example.notes\n  \
     java 26\n  platform spring\n  build maven\n  storage none\n}\n";

/// A project seeded with one of the model fixtures below; the same project as
/// [`jdl_project`].
fn model_project(label: &str, source: &str) -> PathBuf {
    jdl_project(label, source)
}

/// [`NOTES_JDL`] with the resource these tests mutate.
///
/// `use repo`, `use service` and `use http` rather than `use scaffold`:
/// `scaffold` would add a DTO nothing here asked for.
const MODEL: &str = "jdl 1\n\napp Notes @id(project_notes) {\n  pkg com.example.notes\n  \
     java 26\n  platform spring\n  build maven\n  storage none\n}\n\n\
     entity Note @id(ent_note) {\n  use repo\n  use service\n  use http\n\n  \
     id: uuid @id(fld_note_id) @pk\n  title: string @id(fld_note_title) @notBlank\n\n  \
     command CreateNote(title) @id(op_create_note) {\n    route POST \"/notes\"\n  }\n}\n";

/// The same project with no resource in it, for the tests that declare their
/// own. Identical to [`NOTES_JDL`], and named for what it is used as.
const EMPTY_MODEL: &str = NOTES_JDL;

/// The same project with a Gradle build instead of Maven.
///
/// **The pom has to go rather than be joined by a second build file.** Capture
/// refuses a module with both by name, and the model's `build` axis has to
/// name what is on disk or the dependency adapter reconciles into a file the
/// project does not have.
fn gradle_model_project(label: &str, source: &str, build_file: &str, build: &str) -> PathBuf {
    let root = model_project(label, &source.replace("build maven", "build gradle"));
    fs::remove_file(root.join("pom.xml")).unwrap();
    fs::write(root.join(build_file), build).unwrap();
    root
}

/// A canonical project whose authoring source is written by hand.
///
/// It carries a real Spring build, because the models these tests write
/// declare `platform spring` -- the default -- and the compiler will not emit
/// a `@RestController` into a project whose build has no Spring Boot in it.
/// A bare directory holding only `.jails/model.jdl` is not a project any of
/// this could compile into, so proving JDL editing against one would prove it
/// against a shape nobody has.
fn jdl_project(label: &str, source: &str) -> PathBuf {
    let root = temp_dir(label);
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), source).unwrap();
    root
}

/// JDL v1 §18.4: convention must not mean hidden behaviour.
///
/// A generated project is full of names nobody typed, and `model explain` is
/// how a reader learns which rule produced one. This drives the command end
/// to end and asserts the three things it has to get right.
///
/// The reader's layer rename has to be applied, which is why the project
/// carries a `jails.toml`. The records live in the model and a linked model
/// carries the default packages -- the layout arrives with the workspace --
/// so a version of this command that reported on the parsed model would print
/// `com.example.notes.domain` to a project that has no such package.
///
/// And the §9.7 divergence has to be visible. Six of the twenty-three emitted
/// packages sit under a head §9.7 does not close, so a layer rename does not
/// reach them. Their rule says `convention.facet.*` where a layer's says
/// `convention.layer.*`: the same tree, two rules, and the difference is
/// something a reader can be shown.
#[test]
fn model_explain_shows_which_rule_produced_each_derived_name() {
    let root = jdl_project(
        "model-explain-derived",
        "jdl 1\napp Notes @id(project_notes) {\n  pkg com.example.notes\n  java 26\n  \
         platform plain\n  build maven\n  storage none\n}\n\n\
         entity SupportPerson @id(ent_person) {\n  use repo\n  \
         id: uuid @id(fld_person_id) @pk\n  \
         familyName: string @id(fld_person_family)\n}\n",
    );
    write_plain_fixture(&root);
    fs::write(root.join("jails.toml"), "[layout]\ndomain = \"core\"\n").unwrap();

    let explained = jails_cmd(&root, None)
        .args(["model", "explain"])
        .output()
        .unwrap();
    assert!(
        explained.status.success(),
        "{}",
        String::from_utf8_lossy(&explained.stderr)
    );
    let told = String::from_utf8_lossy(&explained.stdout);
    for row in [
        // The rename reached the layer...
        "com.example.notes.core  [convention.layer.domain]",
        // ...and did not reach a head §9.7 does not close.
        "com.example.notes.repository  [convention.facet.repository]",
        // The pluralizer, on §9.7's own irregular case.
        "support_people  [convention.sql-table.pluralize]",
        "family_name  [convention.sql-column.snake-case]",
    ] {
        assert!(told.contains(row), "missing `{row}`:\n{told}");
    }

    // **A filter narrows to one boundary, and a field's column counts as
    // inside it.** The record for `family_name` names `ent_person` in its
    // inputs, so asking about the entity answers with the entity's own names
    // *and* what they were derived for -- which is the question somebody
    // typing an entity id is asking. What it must not do is answer with the
    // project: none of the twenty-three package rows belongs to this entity.
    let filtered = jails_cmd(&root, None)
        .args(["model", "explain", "ent_person"])
        .output()
        .unwrap();
    assert!(filtered.status.success());
    let filtered = String::from_utf8_lossy(&filtered.stdout);
    assert!(filtered.contains("support_people"), "{filtered}");
    assert!(filtered.contains("family_name"), "{filtered}");
    assert!(!filtered.contains("java-package"), "{filtered}");

    let missed = jails_cmd(&root, None)
        .args(["model", "explain", "no-such-boundary"])
        .output()
        .unwrap();
    assert!(missed.status.success());
    assert!(
        String::from_utf8_lossy(&missed.stdout).contains("no derived value matches"),
        "a miss is an answer, not a failure"
    );

    fs::remove_dir_all(root).ok();
}

fn eject_model_project(label: &str) -> PathBuf {
    model_project(label, &format!("{MODEL}\ncap fake @id(cap_fake)\n"))
}

fn apply_canonical_model(root: &Path, label: &str) {
    let bundle = root.join(format!("{label}.json"));
    let planned = jails_cmd(root, None)
        .args(["model", "plan", "--bundle"])
        .arg(&bundle)
        .output()
        .unwrap();
    assert!(
        planned.status.success(),
        "{}",
        String::from_utf8_lossy(&planned.stderr)
    );
    let applied = jails_cmd(root, None)
        .args(["model", "apply", "--bundle"])
        .arg(&bundle)
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
}

#[test]
fn model_fmt_checks_then_atomically_formats_only_the_jdl_source() {
    let root = jdl_project(
        "jdl-v1-format",
        "jdl 1\r\n\r\napp Notes {\r\n\tpkg com.example.notes  \r\n java 26\r\n platform spring\r\n build maven\r\n storage postgres\r\n}\r\n\r\n// reader comment\r\nentity Task {\r\n\tid: uuid @pk  \r\n}\r\n",
    );
    write_spring_fixture(&root);
    let model_path = root.join(".jails/model.jdl");
    let before = fs::read(&model_path).unwrap();

    let checked = jails_cmd(&root, None)
        .args(["model", "fmt", "--check"])
        .output()
        .unwrap();
    assert!(!checked.status.success());
    assert!(
        String::from_utf8_lossy(&checked.stderr).contains("run `jails model fmt`"),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    assert_eq!(fs::read(&model_path).unwrap(), before, "check wrote source");

    let previewed = jails_cmd(&root, None)
        .args(["model", "fmt", "--pretend", "--diff"])
        .output()
        .unwrap();
    assert!(
        previewed.status.success(),
        "{}",
        String::from_utf8_lossy(&previewed.stderr)
    );
    assert_eq!(
        fs::read(&model_path).unwrap(),
        before,
        "preview wrote source"
    );

    let formatted = jails_cmd(&root, None)
        .args(["model", "fmt"])
        .output()
        .unwrap();
    assert!(
        formatted.status.success(),
        "{}",
        String::from_utf8_lossy(&formatted.stderr)
    );
    let after = fs::read_to_string(&model_path).unwrap();
    assert!(!after.contains('\r'), "{after:?}");
    assert!(!after.contains('\t'), "{after:?}");
    assert!(!after.lines().any(|line| line.ends_with(' ')), "{after:?}");
    assert!(after.contains("// reader comment\n"), "{after}");
    assert!(after.ends_with('\n'));

    let checked = jails_cmd(&root, None)
        .args(["model", "fmt", "--check"])
        .output()
        .unwrap();
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
}

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

/// A project can run its own formatter.
///
/// Reproducible output lives under `.jails/generated`, rendered from the
/// model, so the only thing in the formatter's path is the reader's own code,
/// which is what the command is for.
#[test]
fn a_canonical_project_runs_its_own_formatter() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    let root = jdl_project(
        "jdl-v1-canonical-fmt",
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
    let installed = jails_cmd(&root, None)
        .args(["add", "format"])
        .output()
        .unwrap();
    assert!(
        installed.status.success(),
        "{}",
        String::from_utf8_lossy(&installed.stderr)
    );

    let formatted = jails_cmd(&root, None).arg("fmt").output().unwrap();
    let told = String::from_utf8_lossy(&formatted.stderr);
    assert!(
        !told.contains("does not route `fmt`"),
        "a canonical project still refuses its own formatter: {told}"
    );
    assert!(formatted.status.success(), "{told}");
}

/// `model init`: the on-ramp for a repository jails did not create. `new`
/// seeds a model, so a project jails creates has one from its first command;
/// somebody else's repository has none.
///
/// What it writes is the app block and nothing else. The reader's Java is not
/// adopted, moved or rewritten; what changes is that the next `jails g`
/// renders through the compiler into `.jails/generated`.
#[test]
fn model_init_makes_a_foreign_project_canonical_without_touching_its_sources() {
    let root = temp_dir("model-init-foreign");
    write_plain_fixture(&root);
    let reader = root.join("src/main/java/com/example/demo/Existing.java");
    fs::create_dir_all(reader.parent().unwrap()).unwrap();
    let untouched = "package com.example.demo;\n\npublic class Existing {\n}\n";
    fs::write(&reader, untouched).unwrap();

    let created = jails_cmd(&root, None)
        .args(["model", "init"])
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );

    // Every field is read off the project rather than asked for, because each
    // is a fact the project already states.
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(model.starts_with("jdl 1\n"), "{model}");
    assert!(model.contains("pkg com.example.demo"), "{model}");
    assert!(model.contains("build maven"), "{model}");
    // `storage none` is the one judgement, and it is the same one `new` makes:
    // jails has installed no database here, so the model claims none.
    assert!(model.contains("storage none"), "{model}");

    // From here the generator renders into the managed tree.
    let generated = jails_cmd(&root, None)
        .args(["g", "record", "Note", "title:string!"])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    assert!(
        root.join(".jails/generated/main/java/com/example/demo/domain/Note.java")
            .is_file()
    );
    assert!(
        !root.join(".jails/ledger.toml").exists(),
        "a canonical project created a legacy ledger"
    );

    // And the reader's own file is exactly as they left it.
    assert_eq!(fs::read_to_string(&reader).unwrap(), untouched);
}

/// One editable source.
#[test]
fn model_init_refuses_a_project_that_already_has_a_model_or_a_ledger() {
    let modelled = jdl_project(
        "model-init-twice",
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
    write_plain_fixture(&modelled);
    let refused = jails_cmd(&modelled, None)
        .args(["model", "init"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    let told = String::from_utf8_lossy(&refused.stderr);
    assert!(told.contains("already has an application model"), "{told}");

    // A `.jails/ledger.toml` is refused by name rather than treated as an
    // absence: nothing here can read what it holds, and seeding a model
    // beside declarations this binary cannot see would strand a project's
    // whole contents outside the model that owns it.
    let legacy = temp_dir("model-init-legacy-ledger");
    write_plain_fixture(&legacy);
    fs::create_dir_all(legacy.join(".jails")).unwrap();
    fs::write(
        legacy.join(".jails/ledger.toml"),
        "written by another jails\n",
    )
    .unwrap();
    let refused = jails_cmd(&legacy, None)
        .args(["model", "init"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    let told = String::from_utf8_lossy(&refused.stderr);
    assert!(told.contains("legacy ledger"), "{told}");
    assert!(told.contains(".jails/ledger.toml"), "{told}");
}

#[test]
fn model_fmt_keeps_typed_field_semantics_and_refuses_invalid_rules_atomically() {
    let root = jdl_project(
        "jdl-v1-field-semantics",
        r#"jdl 1
app Notes {
 pkg com.example.notes
 java 26
 platform spring
 build maven
 storage postgres
}
entity Task {
 updatedAt: instant @updated @default(now())
 version: long @nonnegative @version
 tenantId: uuid @map("tenant_id") @scope(claim: "tenant") @id(fld_task_tenant)
 id: uuid @pk
}
"#,
    );
    write_spring_fixture(&root);
    let model_path = root.join(".jails/model.jdl");

    let formatted = jails_cmd(&root, None)
        .args(["model", "fmt"])
        .output()
        .unwrap();
    assert!(
        formatted.status.success(),
        "{}",
        String::from_utf8_lossy(&formatted.stderr)
    );
    let source = fs::read_to_string(&model_path).unwrap();
    assert!(source.contains("updatedAt: instant @default(now()) @updated"));
    assert!(source.contains("version: long @version @nonnegative"));
    assert!(source.contains(
        "tenantId: uuid @id(fld_task_tenant) @scope(claim: \"tenant\") @map(\"tenant_id\")"
    ));

    let model = jails_model::parse_jdl(&source).unwrap();
    let task = model
        .entities
        .values()
        .find(|entity| entity.label == "task")
        .unwrap();
    let version = task
        .fields
        .iter()
        .find(|field| field.label == "version")
        .unwrap();
    assert!(version.semantics.version);
    assert!(version.semantics.default.as_ref().unwrap().derived);
    let tenant = task
        .fields
        .iter()
        .find(|field| field.label == "tenant_id")
        .unwrap();
    let scope = tenant.semantics.scope.as_ref().unwrap();
    assert_eq!(scope.claim, "tenant");
    assert!(scope.pinned);

    let invalid = source.replace("tenantId: uuid ", "tenantId: uuid? ");
    fs::write(&model_path, &invalid).unwrap();
    let before = fs::read(&model_path).unwrap();
    let refused = jails_cmd(&root, None)
        .args(["model", "fmt"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("model-scope-required"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert_eq!(fs::read(&model_path).unwrap(), before);
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
fn scoped_execution_context_survives_evolution_and_binds_tenant_at_runtime() {
    let root = jdl_project(
        "jdl-v1-scoped-execution-context",
        r#"jdl 1
app Work {
  pkg com.example.work
  java 26
  platform spring
  build maven
  storage postgres
}

cap api
cap security

entity Task {
  use scaffold
  id: uuid @pk
  tenantId: uuid @scope(claim: "tenant")
  title: string
  version: long @version @nonnegative
  updatedAt: instant @default(now()) @updated

  command Create(title) {
    route POST "/tasks"
  }

  query All() {
    route GET "/tasks"
  }

  transition Rename(version, title) {
    update [title]
    if-match required
    route PATCH "/tasks/{id}"
  }
}
"#,
    );
    write_spring_fixture(&root);
    apply_canonical_model(&root, "scoped-context");

    let generated = root.join(".jails/generated/main/java/com/example/work");
    let context = generated.join("application/ExecutionContext.java");
    let query = generated.join("adapters/jdbc/JdbcAllQuery.java");
    let transition = generated.join("adapters/jdbc/JdbcRenameTransition.java");
    let controller = generated.join("adapters/http/CreateController.java");
    for path in [&context, &query, &transition, &controller] {
        assert!(path.is_file(), "missing {}", path.display());
    }
    assert!(
        fs::read_to_string(&query)
            .unwrap()
            .contains("tenant_id = :scope_tenant_id")
    );
    assert!(
        fs::read_to_string(&transition)
            .unwrap()
            .contains("tenant_id = :scope_tenant_id")
    );
    let controller_source = fs::read_to_string(&controller).unwrap();
    assert!(
        controller_source
            .contains("Map.entry(\"tenant\", scopes.claim(authentication, \"tenant\"))"),
        "{controller_source}"
    );

    let context_source = fs::read_to_string(&context).unwrap();
    let split = context_source.rfind("\n}").unwrap();
    fs::write(
        &context,
        format!(
            "{}\n\n    public String readerMarker() {{ return \"kept\"; }}{}",
            &context_source[..split],
            &context_source[split..]
        ),
    )
    .unwrap();
    let evolved = jails_cmd(&root, None)
        .args(["g", "field", "Task", "priority:int?"])
        .output()
        .unwrap();
    assert!(
        evolved.status.success(),
        "{}",
        String::from_utf8_lossy(&evolved.stderr)
    );
    assert!(
        fs::read_to_string(&context)
            .unwrap()
            .contains("readerMarker()"),
        "regeneration lost a clean edit in the managed execution-context ABI"
    );
    assert!(
        fs::read_to_string(&query)
            .unwrap()
            .contains("select id, tenant_id, title, version, updated_at, priority from task"),
        "model evolution did not reach the scoped query"
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
        let test_dir = common::generated(&root, "src/test/java/com/example/work");
        fs::create_dir_all(&test_dir).unwrap();
        fs::write(
            test_dir.join("ScopedExecutionTest.java"),
            r#"package com.example.work;

import com.example.work.adapters.http.CreateController;
import com.example.work.adapters.jdbc.JdbcAllQuery;
import com.example.work.application.ExecutionContext;
import com.example.work.application.commands.CreateCommand;
import com.example.work.application.queries.AllQuery;
import java.lang.reflect.Proxy;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import java.util.concurrent.atomic.AtomicReference;
import org.junit.jupiter.api.Test;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.mock.env.MockEnvironment;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

class ScopedExecutionTest {

    @Test
    void httpAuthenticationBecomesManagedExecutionContext() {
        var tenant = UUID.randomUUID();
        var captured = new AtomicReference<ExecutionContext>();
        CreateCommand operation = (context, input) -> {
            captured.set(context);
            return null;
        };
        var environment = new MockEnvironment()
                .withProperty("app.security.dev.scopes.tenant", tenant.toString());
        var controller = new CreateController(operation, new ScopeAuthorizer(environment));

        controller.execute(new CreateCommand.Input("one"), null);

        assertEquals(tenant.toString(), captured.get().claim("tenant"));
    }

    @Test
    @SuppressWarnings({"unchecked", "rawtypes"})
    void queryAlwaysBindsTenantFromContext() {
        var tenant = UUID.randomUUID();
        var sql = new AtomicReference<String>();
        var scoped = new AtomicReference<Object>();
        JdbcClient.MappedQuerySpec<com.example.work.domain.Task> rows =
                (JdbcClient.MappedQuerySpec<com.example.work.domain.Task>) Proxy.newProxyInstance(
                        getClass().getClassLoader(),
                        new Class<?>[] {JdbcClient.MappedQuerySpec.class},
                        (proxy, method, arguments) -> {
                            if (method.getName().equals("list")) {
                                return List.of();
                            }
                            throw new UnsupportedOperationException(method.toString());
                        });
        var statement = (JdbcClient.StatementSpec) Proxy.newProxyInstance(
                getClass().getClassLoader(),
                new Class<?>[] {JdbcClient.StatementSpec.class},
                (proxy, method, arguments) -> {
                    if (method.getName().equals("param")) {
                        if ("scope_tenant_id".equals(arguments[0])) {
                            scoped.set(arguments[1]);
                        }
                        return proxy;
                    }
                    if (method.getName().equals("query")) {
                        return rows;
                    }
                    throw new UnsupportedOperationException(method.toString());
                });
        var jdbc = (JdbcClient) Proxy.newProxyInstance(
                getClass().getClassLoader(),
                new Class<?>[] {JdbcClient.class},
                (proxy, method, arguments) -> {
                    if (method.getName().equals("sql")) {
                        sql.set((String) arguments[0]);
                        return statement;
                    }
                    throw new UnsupportedOperationException(method.toString());
                });

        var result = new JdbcAllQuery(jdbc).execute(
                new ExecutionContext(Map.of("tenant", tenant.toString())),
                new AllQuery.Input());

        assertTrue(sql.get().contains("tenant_id = :scope_tenant_id"));
        assertEquals(tenant, scoped.get());
        assertEquals(List.of(), result);
    }
}
"#,
        )
        .unwrap();
        let path = real_path_without_mvnd();
        let tested = real_maven_cmd(&root, &path)
            .args(["-q", "-B", "-Dtest=ScopedExecutionTest", "test"])
            .output()
            .unwrap();
        assert!(
            tested.status.success(),
            "generated scoped execution path failed its real Maven test:\n{}\n{}",
            String::from_utf8_lossy(&tested.stdout),
            String::from_utf8_lossy(&tested.stderr)
        );
    }
}

#[test]
fn compiler_managed_create_values_stay_out_of_the_request_and_compile_end_to_end() {
    let root = jdl_project(
        "jdl-v1-command-default-ownership",
        r#"jdl 1
app Jobs {
  pkg com.example.jobs
  java 26
  platform spring
  build maven
  storage postgres
}

entity Job {
  use scaffold
  id: uuid @pk
  title: string @notBlank
  status: string
  version: long @version @nonnegative
  createdAt: instant @default(now())
  updatedAt: instant @updated

  command CreateJob(title) {
    set status = QUEUED
  }

  transition Archive() {
    set status = ARCHIVED
    if-match none
  }
}
"#,
    );
    write_spring_fixture(&root);
    apply_canonical_model(&root, "command-default-ownership");

    let generated = root.join(".jails/generated/main/java/com/example/jobs");
    let command =
        fs::read_to_string(generated.join("application/commands/CreateJobCommand.java")).unwrap();
    assert!(command.contains("public record Input("), "{command}");
    assert!(command.contains("String title"), "{command}");
    for managed in [
        "UUID id",
        "long version",
        "Instant createdAt",
        "Instant updatedAt",
    ] {
        assert!(
            !command.contains(managed),
            "request exposed `{managed}`:\n{command}"
        );
    }

    let adapter =
        fs::read_to_string(generated.join("adapters/jdbc/JdbcCreateJobCommand.java")).unwrap();
    assert!(adapter.contains("TimeOrderedUuid.next()"), "{adapter}");
    assert!(
        adapter.contains(
            "insert into jobs (id, title, status, updated_at) values (:id, :title, 'QUEUED', current_timestamp) returning id, title, status, version, created_at, updated_at"
        ),
        "{adapter}"
    );
    assert!(!adapter.contains("param(\"status\""), "{adapter}");
    assert!(!adapter.contains("param(\"created_at\""), "{adapter}");
    assert!(!adapter.contains("param(\"version\""), "{adapter}");
    let transition =
        fs::read_to_string(generated.join("adapters/jdbc/JdbcArchiveTransition.java")).unwrap();
    assert!(
        transition.contains(
            "update jobs set status = 'ARCHIVED', version = version + 1, updated_at = current_timestamp where"
        ),
        "{transition}"
    );
    assert!(transition.contains("id = :id"), "{transition}");
    let uuid7 = generated.join("domain/TimeOrderedUuid.java");
    let mut helper = fs::read_to_string(&uuid7).unwrap();
    let closing = helper.rfind('}').unwrap();
    helper.insert_str(
        closing,
        "\n    public static String readerMarker() { return \"kept\"; }\n",
    );
    fs::write(&uuid7, helper).unwrap();
    let evolved = jails_cmd(&root, None)
        .args(["g", "field", "Job", "priority:int?"])
        .output()
        .unwrap();
    assert!(
        evolved.status.success(),
        "{}",
        String::from_utf8_lossy(&evolved.stderr)
    );
    assert!(
        fs::read_to_string(&uuid7).unwrap().contains("readerMarker"),
        "regeneration lost a clean edit in compiler default support"
    );
    let evolved_adapter =
        fs::read_to_string(generated.join("adapters/jdbc/JdbcCreateJobCommand.java")).unwrap();
    assert!(evolved_adapter.contains("'QUEUED'"), "{evolved_adapter}");
    let evolved_transition =
        fs::read_to_string(generated.join("adapters/jdbc/JdbcArchiveTransition.java")).unwrap();
    assert!(
        evolved_transition.contains("status = 'ARCHIVED'"),
        "{evolved_transition}"
    );

    if real_mvn_available() && real_java_supports_target_release() {
        common::assert_main_sources_compile(
            &root,
            &real_path_without_mvnd(),
            "generated default-owning command",
        );
    }
}

#[test]
fn reviewed_model_format_refuses_a_concurrent_source_edit() {
    let root = jdl_project(
        "jdl-v1-format-stale",
        "jdl 1\r\napp Notes {\r\n pkg com.example.notes\r\n java 26\r\n platform spring\r\n build maven\r\n storage postgres\r\n}\r\n",
    );
    write_spring_fixture(&root);
    let model_path = root.join(".jails/model.jdl");
    let bundle = root.join("format-plan.json");
    let planned = jails_cmd(&root, None)
        .args(["model", "fmt", "--plan-out"])
        .arg(&bundle)
        .output()
        .unwrap();
    assert!(
        planned.status.success(),
        "{}",
        String::from_utf8_lossy(&planned.stderr)
    );

    let mut changed = fs::read_to_string(&model_path).unwrap();
    changed.push_str("// concurrent reader edit\r\n");
    fs::write(&model_path, &changed).unwrap();
    let before = fs::read(&model_path).unwrap();
    let applied = jails_cmd(&root, None)
        .args(["model", "apply", "--bundle"])
        .arg(&bundle)
        .output()
        .unwrap();
    assert!(!applied.status.success());
    assert!(
        String::from_utf8_lossy(&applied.stderr).contains("stale exact plan"),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    assert_eq!(fs::read(&model_path).unwrap(), before);
}

#[test]
fn familiar_mutations_write_valid_jdl_v1_through_one_cst_pipeline() {
    let root = jdl_project(
        "jdl-v1-familiar-mutations",
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
    fs::write(
        root.join("acceptance.md"),
        "# Acceptance\n\n- creates a task\n- refuses an empty title\n",
    )
    .unwrap();
    let commands = [
        vec!["g", "scaffold", "Task", "id:uuid@pk", "title:string!"],
        vec!["g", "field", "Task", "done:boolean?"],
        vec!["g", "factory", "Task"],
        vec!["g", "dto", "Task"],
        vec!["g", "class", "Clock"],
        vec!["g", "interface", "TaskPort"],
        vec!["g", "service", "Billing"],
        vec!["g", "sealed", "Outcome", "Success", "Failed"],
        vec![
            "g", "strategy", "Policy", "Fast", "Safe", "--on", "Task", "--yields", "Task",
        ],
        // `--method post`, because `--on` declares a request body. The default
        // is GET, which carries none, and the component linker refuses that
        // pair by name.
        vec![
            "g",
            "controller",
            "TaskApi",
            "--method",
            "post",
            "--on",
            "Task",
            "--yields",
            "Task",
            "--path",
            "/task-api",
        ],
        vec!["g", "test", "Smoke"],
        vec!["g", "integration-test", "Database"],
        vec!["add", "fake"],
        vec![
            "add",
            "dependency",
            "org.jsoup:jsoup",
            "--version",
            "1.18.3",
            "--scope",
            "test",
        ],
        vec!["set", "server.port=8080"],
        vec!["g", "event", "TaskCreated", "id", "title", "--on", "Task"],
        // A *second* event, and the difference between the two is the point:
        // `id:uuid` is a component the row does not carry, minted when the
        // event is staged. `TaskCreated` projects the row's id and is
        // published directly -- an event cannot be both, because a staged
        // event keyed on the resource id deduplicates the wrong thing.
        vec![
            "g",
            "event",
            "TaskStaged",
            "id:uuid",
            "title",
            "--on",
            "Task",
        ],
        // The outbox store encodes the staged payload with `Json`.
        vec!["add", "json"],
        vec![
            "g",
            "usecase",
            "CreateTask",
            "title",
            "--on",
            "Task",
            "--path",
            "/tasks",
        ],
        vec![
            "g",
            "query",
            "OpenTasks",
            "title",
            "--on",
            "Task",
            "--limit",
            "50",
            "--path",
            "/tasks/search",
        ],
        vec![
            "g",
            "transition",
            "RenameTask",
            "title",
            "--on",
            "Task",
            "--yields",
            "TaskCreated",
            "--path",
            "/tasks/{id}",
            "--method",
            "patch",
        ],
        vec!["g", "handler", "Health", "--path", "/healthz"],
        vec!["g", "cli", "Admin"],
        vec!["g", "command", "Refresh", "--on", "AdminCli"],
        vec!["g", "cases", "acceptance.md"],
        vec![
            "g",
            "client",
            "Audit",
            "requestId:uuid",
            "--on",
            "Task",
            "--yields",
            "Task",
            "--path",
            "/v1/audit",
            "--method",
            "post",
        ],
        vec![
            "g",
            "usecase",
            "StageTask",
            "title",
            "--on",
            "Task",
            "--yields",
            "TaskStaged",
        ],
        vec!["g", "fetcher", "Remote"],
        vec!["g", "job", "Sweep"],
        vec!["g", "http-workflow", "Crawl", "--on", "RemoteFetcher"],
        vec![
            "g",
            "http-sink",
            "Delivery",
            "--on",
            "StageTask",
            "--yields",
            "TaskStaged",
        ],
        vec!["g", "idempotency", "Request"],
        // The encoder, the decoder and the filter chain that reads the token
        // are one story, so `g auth` refuses without the capability that
        // carries the other two.
        vec!["add", "security"],
        vec!["g", "auth", "Api"],
        vec![
            "g",
            "webhook",
            "Stripe",
            "signature:string!",
            "--path",
            "/hooks/stripe",
            "--method",
            "post",
            "--consumes",
            "form",
            "--bind",
            "signature=stripe_signature",
        ],
        // `--on` is the command it runs later and `--yields` the entity that
        // command creates -- the row whose presence tells a retry from a
        // repeat. The payload is the command's own `Input`, so there are no
        // fields to repeat here.
        vec![
            "g",
            "durable-job",
            "Dispatch",
            "--on",
            "CreateTask",
            "--yields",
            "Task",
        ],
        vec![
            "g", "socket", "Chat", "--path", "/ws/chat", "--method", "get",
        ],
        vec!["g", "presence", "Online"],
    ];
    // Empty, and the list stays: a kind added without a backend belongs here
    // rather than in the success path, because the loop above must not accept
    // a declaration that reports success and emits nothing.
    const UNSERVED: &[&str] = &[];
    for command in commands {
        let output = jails_cmd(&root, None).args(&command).output().unwrap();
        if UNSERVED.contains(&command[1]) {
            let before = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
            assert!(
                !output.status.success(),
                "`jails {}` has no compiler backend and must refuse",
                command.join(" ")
            );
            // The exact wording is pinned by
            // `every_component_kind_is_emitted_or_refused` in `jails-compiler`,
            // which reaches every unserved kind directly. Here the refusal can
            // also arrive second-hand -- `g command --on AdminCli` cannot
            // resolve a component `g cli Admin` was refused permission to
            // declare -- and what this test owns is the property that survives
            // either route: the command fails and the source is untouched.
            assert_eq!(
                before,
                fs::read_to_string(root.join(".jails/model.jdl")).unwrap(),
                "`jails {}` refused and still edited the source",
                command.join(" ")
            );
            continue;
        }
        assert!(
            output.status.success(),
            "`jails {}` failed:\n{}",
            command.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        let source = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
        jails_model::parse_jdl_cst(&source).unwrap_or_else(|diagnostics| {
            panic!(
                "`jails {}` wrote invalid CST:\n{diagnostics}",
                command.join(" ")
            )
        });
        jails_model::parse_jdl(&source).unwrap_or_else(|diagnostics| {
            panic!(
                "`jails {}` wrote invalid semantics:\n{diagnostics}",
                command.join(" ")
            )
        });
    }

    let source = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    for declaration in [
        "use scaffold",
        "use factory",
        // No `use dto`: the scaffold profile carries `Facet::Dto`, so `g dto`
        // on a scaffolded entity declares nothing it does not already have.
        // Recording it again would give one facet two spellings in one
        // entity, and the second is the one nothing removes.
        "component class Clock @id(cmp_class_clock)",
        "component interface TaskPort @id(cmp_interface_task_port)",
        "component service Billing @id(cmp_service_billing)",
        "component sealed Outcome @id(cmp_sealed_outcome)",
        "variant Success @id(var_cmp_sealed_outcome_success)",
        "component strategy Policy @id(cmp_strategy_policy)",
        "component controller TaskApi @id(cmp_controller_task_api)",
        "component test Smoke @id(cmp_test_smoke)",
        "component integration-test Database @id(cmp_integration_test_database)",
        "done: boolean? @id(fld_task_done)",
        "cap fake @id(cap_fake)",
        "dep org.jsoup:jsoup @id(dep_",
        "@version(\"1.18.3\") @scope(test)",
        "prop server.port = \"8080\" @id(set_",
        "event TaskCreated(id, title) @id(op_task_created)",
        "event TaskStaged(id: uuid, title) @id(op_task_staged)",
        "deliver outbox",
        "route POST \"/tasks\"",
        "limit 50",
        "emit task_created",
        "route PATCH \"/tasks/{id}\"",
        "component cases Acceptance @id(cmp_cases_acceptance)",
        "source \"acceptance.md\"",
        // A kind with no compiler backend never reaches the source: a mutation
        // compiles before it writes, so refusing to emit is refusing to
        // record.
    ] {
        assert!(
            source.contains(declaration),
            "missing `{declaration}`:\n{source}"
        );
    }

    for command in [
        vec!["destroy", "factory", "Task", "--force"],
        vec!["destroy", "class", "Clock", "--force"],
        vec!["destroy", "sealed", "Outcome", "--force"],
        vec!["destroy", "cases", "acceptance.md", "--force"],
        vec!["remove", "fake", "--force"],
        vec!["remove", "dependency", "org.jsoup:jsoup"],
        vec!["unset", "server.port"],
    ] {
        let output = jails_cmd(&root, None).args(&command).output().unwrap();
        assert!(
            output.status.success(),
            "`jails {}` failed:\n{}",
            command.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let source = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    jails_model::parse_jdl(&source).unwrap();
    assert!(!source.contains("use factory"), "{source}");
    assert!(!source.contains("component class Clock"), "{source}");
    assert!(!source.contains("component sealed Outcome"), "{source}");
    assert!(!source.contains("component cases Acceptance"), "{source}");
    assert!(!source.contains("cap fake"), "{source}");
    assert!(!source.contains("dep org.jsoup:jsoup"), "{source}");
    assert!(!source.contains("prop server.port"), "{source}");
}

#[test]
fn jdl_v1_cases_source_is_an_exact_plan_input() {
    let root = jdl_project(
        "jdl-v1-cases-stale-source",
        r#"jdl 1
app Notes {
  pkg com.example.notes
  java 26
  platform plain
  build maven
  storage none
}
"#,
    );
    let source_path = root.join("acceptance.md");
    fs::write(&source_path, "# Acceptance\n\n- first behavior\n").unwrap();
    let bundle = root.join("cases-plan.json");
    let planned = jails_cmd(&root, None)
        .args(["g", "cases", "acceptance.md", "--plan-out"])
        .arg(&bundle)
        .output()
        .unwrap();
    assert!(
        planned.status.success(),
        "{}",
        String::from_utf8_lossy(&planned.stderr)
    );
    assert!(bundle.is_file());
    assert!(
        !fs::read_to_string(root.join(".jails/model.jdl"))
            .unwrap()
            .contains("component cases"),
        "planning must not write the desired state"
    );

    fs::write(
        &source_path,
        "# Acceptance\n\n- first behavior\n- changed after review\n",
    )
    .unwrap();
    let before = snapshot_tree(&root);
    let applied = jails_cmd(&root, None)
        .args(["model", "apply", "--bundle"])
        .arg(&bundle)
        .output()
        .unwrap();
    assert!(!applied.status.success());
    assert!(
        String::from_utf8_lossy(&applied.stderr).contains("stale exact plan"),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let without_executor_lock = |mut tree: Vec<(PathBuf, Vec<u8>)>| {
        tree.retain(|(path, _)| !path.ends_with(".jails/apply.lock"));
        tree
    };
    assert_eq!(
        without_executor_lock(snapshot_tree(&root)),
        without_executor_lock(before),
        "a stale cases input wrote part of the component mutation"
    );
}

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
    let task = root.join(".jails/generated/main/java/com/example/notes/domain/Task.java");
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
    let moved = root.join(".jails/generated/main/java/com/example/notes/domain/WorkItem.java");
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
fn jdl_v1_drives_the_real_generate_edit_generate_loop() {
    let root = jdl_project(
        "jdl-v1-generate-edit-generate",
        r#"jdl 1

app Notes @id(project_notes) {
  pkg com.example.notes
  java 26
  platform spring
  build maven
  storage postgres
}

entity Task @id(ent_task) {
  id: uuid @id(fld_task_id) @pk
  title: string @id(fld_task_title) @notBlank
}
"#,
    );
    write_spring_fixture(&root);
    apply_canonical_model(&root, "jdl-v1-initial");

    let record = root.join(".jails/generated/main/java/com/example/notes/domain/Task.java");
    let source = fs::read_to_string(&record).unwrap();
    let split = source.rfind("\n}").unwrap();
    fs::write(
        &record,
        format!(
            "{}\n\n    public String readerMethod() {{ return title; }}{}",
            &source[..split],
            &source[split..]
        ),
    )
    .unwrap();

    let model_path = root.join(".jails/model.jdl");
    let model = fs::read_to_string(&model_path).unwrap();
    fs::write(
        &model_path,
        model.replace(
            "  title: string @id(fld_task_title) @notBlank\n",
            "  title: string @id(fld_task_title) @notBlank\n  done: boolean @id(fld_task_done)\n",
        ),
    )
    .unwrap();
    apply_canonical_model(&root, "jdl-v1-evolved");

    let evolved = fs::read_to_string(&record).unwrap();
    assert!(evolved.contains("readerMethod()"), "{evolved}");
    assert!(evolved.contains("boolean done"), "{evolved}");
}

#[test]
fn canonical_source_units_merge_every_main_and_test_file_and_wire_both_roots() {
    let root = temp_dir("canonical-source-units-loop");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();

    for command in [
        ["g", "class", "Clock"].as_slice(),
        ["g", "interface", "Port"].as_slice(),
        ["g", "service", "BillingService"].as_slice(),
    ] {
        let output = jails_cmd(&root, None).args(command).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let files = [
        ".jails/generated/main/java/com/example/demo/Clock.java",
        ".jails/generated/test/java/com/example/demo/ClockTest.java",
        ".jails/generated/main/java/com/example/demo/Port.java",
        ".jails/generated/main/java/com/example/demo/service/BillingService.java",
        ".jails/generated/test/java/com/example/demo/service/BillingServiceTest.java",
    ];
    for (index, relative) in files.iter().enumerate() {
        let path = root.join(relative);
        let source = fs::read_to_string(&path).unwrap();
        let split = source.rfind("\n}").unwrap();
        fs::write(
            &path,
            format!(
                "{}\n\n    // reader-edit-{index}{}",
                &source[..split],
                &source[split..]
            ),
        )
        .unwrap();
    }

    let rerun = jails_cmd(&root, None)
        .args(["g", "class", "Queue"])
        .output()
        .unwrap();
    assert!(
        rerun.status.success(),
        "{}",
        String::from_utf8_lossy(&rerun.stderr)
    );
    for (index, relative) in files.iter().enumerate() {
        let source = fs::read_to_string(root.join(relative)).unwrap();
        assert!(
            source.contains(&format!("reader-edit-{index}")),
            "{relative}: {source}"
        );
    }

    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("<source>.jails/generated/main/java</source>"));
    assert!(pom.contains("<source>.jails/generated/test/java</source>"));
    assert!(pom.contains("<goal>add-source</goal>"));
    assert!(pom.contains("<goal>add-test-source</goal>"));

    let jdl = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(
        jdl.contains("component class Clock @id(cmp_class_clock)"),
        "{jdl}"
    );
    assert!(
        jdl.contains("component interface Port @id(cmp_interface_port)"),
        "{jdl}"
    );
    assert!(
        jdl.contains("component service Billing @id(cmp_service_billing)"),
        "{jdl}"
    );

    // A component carries no package of its own, and the refusal is the
    // contract rather than a gap in the parser: v1 derives every managed
    // placement from the closed projection registry, so a reader-owned
    // destination is `model eject`'s job.
    let before = snapshot_tree(&root);
    fs::write(
        root.join(".jails/model.jdl"),
        jdl.replace(
            "component class Clock @id(cmp_class_clock)",
            "component class Clock @id(cmp_class_clock) @package(core)",
        ),
    )
    .unwrap();
    let refused = jails_cmd(&root, None)
        .args(["model", "plan"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    let told = String::from_utf8_lossy(&refused.stderr);
    assert!(told.contains("@package` is not valid here"), "{told}");
    assert!(told.contains("use only id"), "{told}");
    fs::write(root.join(".jails/model.jdl"), &jdl).unwrap();
    assert_eq!(
        snapshot_tree(&root),
        before,
        "the refused plan wrote part of itself"
    );

    fs::remove_dir_all(root).ok();
}

#[test]
fn canonical_standalone_tests_merge_reader_edits_and_refuse_edited_build_wiring() {
    let root = temp_dir("canonical-standalone-tests-loop");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();

    for command in [
        ["g", "test", "ParserTest"].as_slice(),
        ["g", "integration-test", "CheckoutIT"].as_slice(),
    ] {
        let output = jails_cmd(&root, None).args(command).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let files = [
        ".jails/generated/test/java/com/example/demo/ParserTest.java",
        ".jails/generated/test/java/com/example/demo/CheckoutIT.java",
    ];
    for (index, relative) in files.iter().enumerate() {
        let path = root.join(relative);
        let source = fs::read_to_string(&path).unwrap();
        let split = source.rfind("\n}").unwrap();
        fs::write(
            path,
            format!(
                "{}\n\n    // standalone-reader-edit-{index}{}",
                &source[..split],
                &source[split..]
            ),
        )
        .unwrap();
    }

    let rerun = jails_cmd(&root, None)
        .args(["g", "test", "Formatter"])
        .output()
        .unwrap();
    assert!(
        rerun.status.success(),
        "{}",
        String::from_utf8_lossy(&rerun.stderr)
    );
    for (index, relative) in files.iter().enumerate() {
        let source = fs::read_to_string(root.join(relative)).unwrap();
        assert!(
            source.contains(&format!("standalone-reader-edit-{index}")),
            "{relative}: {source}"
        );
    }

    let pom_path = root.join("pom.xml");
    let pom = fs::read_to_string(&pom_path).unwrap();
    assert_eq!(pom.matches("maven-failsafe-plugin").count(), 1, "{pom}");
    assert!(pom.contains("<goal>integration-test</goal>"), "{pom}");
    assert!(pom.contains("<goal>verify</goal>"), "{pom}");
    assert!(!pom.contains("<version>3.5.6</version>"), "{pom}");

    let jdl = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(
        jdl.contains("component test Parser @id(cmp_test_parser)"),
        "{jdl}"
    );
    assert!(
        jdl.contains("component integration-test Checkout @id(cmp_integration_test_checkout)"),
        "{jdl}"
    );

    fs::write(
        &pom_path,
        pom.replace("<goal>verify</goal>", "<goal>reader-edited</goal>"),
    )
    .unwrap();
    let before = snapshot_tree(&root);
    let refused = jails_cmd(&root, None)
        .args(["g", "test", "Later"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("was edited"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert_eq!(snapshot_tree(&root), before, "refusal wrote part of a plan");

    fs::write(&pom_path, &pom).unwrap();
    let integration_path = root.join(files[1]);
    let integration = fs::read_to_string(&integration_path).unwrap();
    fs::write(
        &integration_path,
        integration.replace(
            "throw new UnsupportedOperationException(\"todo\");",
            "throw new UnsupportedOperationException(\"reader wording\");",
        ),
    )
    .unwrap();
    let jdl_path = root.join(".jails/model.jdl");
    let jdl = fs::read_to_string(&jdl_path).unwrap();
    fs::write(
        &jdl_path,
        jdl.replace(
            "component integration-test Checkout @id(cmp_integration_test_checkout)",
            "component test Checkout @id(cmp_integration_test_checkout)",
        ),
    )
    .unwrap();
    let before = snapshot_tree(&root);
    let refused = jails_cmd(&root, None)
        .args(["model", "plan"])
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
        before,
        "Java overlap refusal wrote part of a plan"
    );

    fs::remove_dir_all(root).ok();
}

#[test]
fn canonical_sealed_types_evolve_through_merge_and_destroy_as_one_semantic_unit() {
    let root = temp_dir("canonical-sealed-loop");
    write_plain_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();

    let generated = jails_cmd(&root, None)
        .args(["g", "sealed", "Outcome", "Accepted", "Rejected"])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let files = [
        ".jails/generated/main/java/com/example/demo/domain/Outcome.java",
        ".jails/generated/test/java/com/example/demo/domain/OutcomeTest.java",
    ];
    for (index, relative) in files.iter().enumerate() {
        let path = root.join(relative);
        let source = fs::read_to_string(&path).unwrap();
        let anchor = if index == 0 {
            "public sealed interface Outcome permits Outcome.Accepted, Outcome.Rejected {\n"
        } else {
            "class OutcomeTest {\n"
        };
        assert!(source.contains(anchor), "{relative}: {source}");
        fs::write(
            path,
            source.replace(
                anchor,
                &format!("{anchor}\n    // sealed-reader-edit-{index}\n"),
            ),
        )
        .unwrap();
    }
    let jdl_path = root.join(".jails/model.jdl");
    let jdl = fs::read_to_string(&jdl_path).unwrap();
    fs::write(
        &jdl_path,
        jdl.replace(
            "component sealed Outcome @id(cmp_sealed_outcome) {",
            "// reader model note\ncomponent sealed Outcome @id(cmp_sealed_outcome) {",
        ),
    )
    .unwrap();

    let evolved = jails_cmd(&root, None)
        .args(["g", "sealed", "Outcome", "Accepted", "Rejected", "Pending"])
        .output()
        .unwrap();
    assert!(
        evolved.status.success(),
        "{}",
        String::from_utf8_lossy(&evolved.stderr)
    );
    for (index, relative) in files.iter().enumerate() {
        let source = fs::read_to_string(root.join(relative)).unwrap();
        assert!(
            source.contains(&format!("sealed-reader-edit-{index}")),
            "{relative}: {source}"
        );
        assert!(source.contains("Pending"), "{relative}: {source}");
    }
    let jdl = fs::read_to_string(&jdl_path).unwrap();
    // Above the declaration, not inside it. A v1 component is a block and
    // evolving it replaces the whole declaration span, so a comment *inside*
    // would need the editor to merge prose it did not write. The property:
    // the reader's wording in the model source outlives an evolve.
    assert!(
        jdl.contains("// reader model note\ncomponent sealed Outcome @id(cmp_sealed_outcome) {"),
        "{jdl}"
    );
    for variant in ["Accepted", "Rejected", "Pending"] {
        assert!(
            jdl.contains(&format!("variant {variant} @id(var_cmp_sealed_outcome_")),
            "{jdl}"
        );
    }

    let first = snapshot_tree(&root);
    let rerun = jails_cmd(&root, None)
        .args(["g", "sealed", "Outcome", "Accepted", "Rejected", "Pending"])
        .output()
        .unwrap();
    assert!(rerun.status.success());
    assert_eq!(snapshot_tree(&root), first, "identical rerun changed bytes");
    if real_mvn_available() && real_java_supports_target_release() {
        let path = real_path_without_mvnd();
        let compiled = real_maven_cmd(&root, &path)
            .args(["-q", "-B", "test"])
            .output()
            .unwrap();
        assert!(
            compiled.status.success(),
            "canonical sealed type did not compile and test:\n{}\n{}",
            String::from_utf8_lossy(&compiled.stdout),
            String::from_utf8_lossy(&compiled.stderr)
        );
    }

    let main_path = root.join(files[0]);
    let clean_reader_delta = fs::read_to_string(&main_path).unwrap();
    fs::write(
        &main_path,
        clean_reader_delta.replace(
            "record Pending() implements Outcome {}",
            "record Pending(String readerValue) implements Outcome {}",
        ),
    )
    .unwrap();
    let before = snapshot_tree(&root);
    let refused = jails_cmd(&root, None)
        .args(["g", "sealed", "Outcome", "Accepted", "Rejected"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("overlap"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert_eq!(snapshot_tree(&root), before, "overlap wrote part of a plan");

    fs::write(
        &main_path,
        clean_reader_delta
            .replace("\n    // sealed-reader-edit-0\n", "\n")
            .replace("{\n\n\n    /**", "{\n\n    /**"),
    )
    .unwrap();
    let test_path = root.join(files[1]);
    let test = fs::read_to_string(&test_path).unwrap();
    fs::write(
        &test_path,
        test.replace("\n    // sealed-reader-edit-1\n", "\n")
            .replace("{\n\n\n    private", "{\n\n    private"),
    )
    .unwrap();
    let destroyed = jails_cmd(&root, None)
        .args(["destroy", "sealed", "Outcome", "--force"])
        .output()
        .unwrap();
    assert!(
        destroyed.status.success(),
        "{}",
        String::from_utf8_lossy(&destroyed.stderr)
    );
    assert!(files.iter().all(|relative| !root.join(relative).exists()));

    fs::remove_dir_all(root).ok();
}

#[test]
fn canonical_factory_tracks_entity_fields_without_owning_the_record() {
    let root = temp_dir("canonical-factory-loop");
    write_plain_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();

    let record_output = jails_cmd(&root, None)
        .args(["g", "record", "Note", "title:string!"])
        .output()
        .unwrap();
    assert!(
        record_output.status.success(),
        "{}",
        String::from_utf8_lossy(&record_output.stderr)
    );
    let jdl_path = root.join(".jails/model.jdl");
    let jdl = fs::read_to_string(&jdl_path).unwrap();
    fs::write(
        &jdl_path,
        jdl.replace(
            "entity Note @id(ent_note) {",
            "entity Note @id(ent_note) { // reader model wording",
        ),
    )
    .unwrap();
    let factory_output = jails_cmd(&root, None)
        .args(["g", "factory", "Note"])
        .output()
        .unwrap();
    assert!(
        factory_output.status.success(),
        "{}",
        String::from_utf8_lossy(&factory_output.stderr)
    );
    // The facet is a `use` line inside the block in v1, so the reader's
    // wording on the header line is untouched by the insert rather than
    // carried along by it.
    let jdl = fs::read_to_string(&jdl_path).unwrap();
    assert!(
        jdl.contains("entity Note @id(ent_note) { // reader model wording"),
        "{jdl}"
    );
    assert!(jdl.contains("use factory"), "{jdl}");
    let factory = root.join(".jails/generated/test/java/com/example/demo/testkit/NoteFactory.java");
    let record = root.join(".jails/generated/main/java/com/example/demo/domain/Note.java");
    let source = fs::read_to_string(&factory).unwrap();
    fs::write(
        &factory,
        source.replace(
            "    public static NoteFactory aNote() {\n        return new NoteFactory();\n    }\n",
            "    public static NoteFactory aNote() {\n        return new NoteFactory();\n    }\n\n    public String readerMethod() { return \"reader\"; }\n",
        ),
    )
    .unwrap();

    let evolved = jails_cmd(&root, None)
        .args(["g", "field", "Note", "done:boolean"])
        .output()
        .unwrap();
    assert!(
        evolved.status.success(),
        "{}",
        String::from_utf8_lossy(&evolved.stderr)
    );
    let evolved_factory = fs::read_to_string(&factory).unwrap();
    assert!(
        evolved_factory.contains("readerMethod()"),
        "{evolved_factory}"
    );
    assert!(
        evolved_factory.contains("withDone(boolean value)"),
        "{evolved_factory}"
    );
    assert!(
        fs::read_to_string(&record)
            .unwrap()
            .contains("boolean done"),
        "record did not evolve with its factory"
    );
    let stable = snapshot_tree(&root);
    let rerun = jails_cmd(&root, None)
        .args(["g", "factory", "Note"])
        .output()
        .unwrap();
    assert!(rerun.status.success());
    assert_eq!(snapshot_tree(&root), stable, "factory rerun changed bytes");

    if real_mvn_available() && real_java_supports_target_release() {
        let path = real_path_without_mvnd();
        let compiled = real_maven_cmd(&root, &path)
            .args(["-q", "-B", "test"])
            .output()
            .unwrap();
        assert!(
            compiled.status.success(),
            "canonical factory did not compile:\n{}\n{}",
            String::from_utf8_lossy(&compiled.stdout),
            String::from_utf8_lossy(&compiled.stderr)
        );
    }

    fs::write(
        &factory,
        evolved_factory.replace(
            "private boolean done = false;",
            "private boolean done = true;",
        ),
    )
    .unwrap();
    let jdl = fs::read_to_string(&jdl_path).unwrap();
    fs::write(&jdl_path, jdl.replace("done: boolean", "done: string")).unwrap();
    let before = snapshot_tree(&root);
    let refused = jails_cmd(&root, None)
        .args(["model", "plan"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("overlap"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert_eq!(snapshot_tree(&root), before, "factory overlap wrote bytes");

    fs::write(&jdl_path, jdl).unwrap();
    fs::write(
        &factory,
        evolved_factory.replace(
            "\n    public String readerMethod() { return \"reader\"; }\n",
            "",
        ),
    )
    .unwrap();
    let destroyed = jails_cmd(&root, None)
        .args(["destroy", "factory", "Note", "--force"])
        .output()
        .unwrap();
    assert!(
        destroyed.status.success(),
        "{}",
        String::from_utf8_lossy(&destroyed.stderr)
    );
    assert!(!factory.exists());
    assert!(record.exists(), "factory destroy removed the managed ABI");
    let jdl = fs::read_to_string(jdl_path).unwrap();
    assert!(!jdl.contains("@factory"), "{jdl}");

    fs::remove_dir_all(root).ok();
}

#[test]
fn canonical_strategy_evolves_all_implementation_boundaries_in_one_plan() {
    let root = temp_dir("canonical-strategy-loop");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();
    for command in [
        ["g", "record", "Post", "title:string!"].as_slice(),
        ["g", "record", "Tag", "value:string!"].as_slice(),
        ["g", "record", "Other", "name:string!"].as_slice(),
        [
            "g", "strategy", "PostRule", "Featured", "Standard", "--on", "Post", "--yields", "Tag",
        ]
        .as_slice(),
    ] {
        let output = jails_cmd(&root, None).args(command).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let jdl_path = root.join(".jails/model.jdl");
    let jdl = fs::read_to_string(&jdl_path).unwrap();
    fs::write(
        &jdl_path,
        jdl.replace(
            "component strategy PostRule @id(cmp_strategy_post_rule) {",
            "// reader strategy wording\ncomponent strategy PostRule @id(cmp_strategy_post_rule) {",
        ),
    )
    .unwrap();
    let managed = root.join(".jails/generated");
    let existing = [
        managed.join("main/java/com/example/demo/domain/PostRule.java"),
        managed.join("main/java/com/example/demo/service/PostRuleEvaluator.java"),
        managed.join("main/java/com/example/demo/service/FeaturedPostRule.java"),
        managed.join("main/java/com/example/demo/service/StandardPostRule.java"),
        managed.join("test/java/com/example/demo/service/FeaturedPostRuleTest.java"),
        managed.join("test/java/com/example/demo/service/StandardPostRuleTest.java"),
    ];
    for (index, path) in existing.iter().enumerate() {
        let source = fs::read_to_string(path).unwrap();
        let split = source.rfind("\n}").unwrap();
        fs::write(
            path,
            format!(
                "{}\n\n    // strategy-reader-edit-{index}{}",
                &source[..split],
                &source[split..]
            ),
        )
        .unwrap();
    }

    let evolved = jails_cmd(&root, None)
        .args([
            "g", "strategy", "PostRule", "Featured", "Standard", "Premium", "--on", "Post",
            "--yields", "Tag",
        ])
        .output()
        .unwrap();
    assert!(
        evolved.status.success(),
        "{}",
        String::from_utf8_lossy(&evolved.stderr)
    );
    for (index, path) in existing.iter().enumerate() {
        assert!(
            fs::read_to_string(path)
                .unwrap()
                .contains(&format!("strategy-reader-edit-{index}")),
            "reader edit was lost from {}",
            path.display()
        );
    }
    let premium = [
        managed.join("main/java/com/example/demo/service/PremiumPostRule.java"),
        managed.join("test/java/com/example/demo/service/PremiumPostRuleTest.java"),
    ];
    assert!(premium.iter().all(|path| path.is_file()));
    assert!(
        fs::read_to_string(&jdl_path)
            .unwrap()
            .contains("// reader strategy wording\ncomponent strategy PostRule")
    );

    let port = &existing[0];
    let clean_port = fs::read_to_string(port).unwrap();
    fs::write(
        port,
        clean_port.replace("evaluate(Post value)", "evaluate(Post readerOwnedValue)"),
    )
    .unwrap();
    let before = snapshot_tree(&root);
    let refused = jails_cmd(&root, None)
        .args([
            "g", "strategy", "PostRule", "Featured", "Standard", "Premium", "--on", "Other",
            "--yields", "Tag",
        ])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("overlap"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert_eq!(snapshot_tree(&root), before, "strategy overlap wrote bytes");

    fs::write(port, clean_port).unwrap();
    let changed = jails_cmd(&root, None)
        .args([
            "g", "strategy", "PostRule", "Featured", "Standard", "Premium", "--on", "Other",
            "--yields", "Tag",
        ])
        .output()
        .unwrap();
    assert!(
        changed.status.success(),
        "{}",
        String::from_utf8_lossy(&changed.stderr)
    );
    assert!(
        fs::read_to_string(port)
            .unwrap()
            .contains("evaluate(Other value)")
    );
    for (index, path) in existing.iter().enumerate() {
        assert!(
            fs::read_to_string(path)
                .unwrap()
                .contains(&format!("strategy-reader-edit-{index}")),
            "signature evolution lost {}",
            path.display()
        );
    }

    let before_ejection = snapshot_tree(&root);
    let refused_ejection = jails_cmd(&root, None)
        .args(["model", "eject", "art_unit_strategy_post_rule_abi"])
        .output()
        .unwrap();
    assert!(!refused_ejection.status.success());
    assert!(
        String::from_utf8_lossy(&refused_ejection.stderr).contains("managed ABI"),
        "{}",
        String::from_utf8_lossy(&refused_ejection.stderr)
    );
    assert_eq!(snapshot_tree(&root), before_ejection);

    let stable = snapshot_tree(&root);
    let rerun = jails_cmd(&root, None)
        .args([
            "g", "strategy", "PostRule", "Featured", "Standard", "Premium", "--on", "Other",
            "--yields", "Tag",
        ])
        .output()
        .unwrap();
    assert!(rerun.status.success());
    assert_eq!(snapshot_tree(&root), stable, "strategy rerun changed bytes");

    if real_mvn_available() && real_java_supports_target_release() {
        let path = real_path_without_mvnd();
        let compiled = real_maven_cmd(&root, &path)
            .args(["-q", "-B", "test"])
            .output()
            .unwrap();
        assert!(
            compiled.status.success(),
            "canonical strategy did not compile:\n{}\n{}",
            String::from_utf8_lossy(&compiled.stdout),
            String::from_utf8_lossy(&compiled.stderr)
        );
    }

    for (index, path) in existing.iter().enumerate() {
        let source = fs::read_to_string(path).unwrap();
        fs::write(
            path,
            source.replace(&format!("\n\n    // strategy-reader-edit-{index}"), ""),
        )
        .unwrap();
    }
    let destroyed = jails_cmd(&root, None)
        .args(["destroy", "strategy", "PostRule", "--force"])
        .output()
        .unwrap();
    assert!(
        destroyed.status.success(),
        "{}",
        String::from_utf8_lossy(&destroyed.stderr)
    );
    assert!(existing.iter().chain(&premium).all(|path| !path.exists()));
    assert!(
        root.join(".jails/generated/main/java/com/example/demo/domain/Post.java")
            .is_file(),
        "strategy destroy removed an input ABI"
    );

    fs::remove_dir_all(root).ok();
}

#[test]
fn canonical_controller_merges_both_files_and_refuses_overlapping_route_edits() {
    let root = temp_dir("canonical-controller-loop");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();
    for command in [
        ["g", "record", "Request", "value:string!"].as_slice(),
        ["g", "record", "Response", "value:string!"].as_slice(),
        [
            "g",
            "controller",
            "Verify",
            "--method",
            "post",
            "--on",
            "Request",
            "--returns",
            "Response",
            "--path",
            "/v1/verify",
            "--consumes",
            "json",
        ]
        .as_slice(),
    ] {
        let output = jails_cmd(&root, None).args(command).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let jdl_path = root.join(".jails/model.jdl");
    let jdl = fs::read_to_string(&jdl_path).unwrap();
    fs::write(
        &jdl_path,
        jdl.replace(
            "component controller Verify @id(cmp_controller_verify) {",
            "// reader route wording\ncomponent controller Verify @id(cmp_controller_verify) {",
        ),
    )
    .unwrap();
    let files = [
        root.join(".jails/generated/main/java/com/example/demo/web/VerifyController.java"),
        root.join(".jails/generated/test/java/com/example/demo/web/VerifyControllerTest.java"),
    ];
    for (index, path) in files.iter().enumerate() {
        let source = fs::read_to_string(path).unwrap();
        let split = source.rfind("\n}").unwrap();
        fs::write(
            path,
            format!(
                "{}\n\n    // controller-reader-edit-{index}{}",
                &source[..split],
                &source[split..]
            ),
        )
        .unwrap();
    }

    let evolve = [
        "g",
        "controller",
        "Verify",
        "--method",
        "put",
        "--on",
        "Request",
        "--returns",
        "Response",
        "--path",
        "/v2/verify",
        "--consumes",
        "json",
    ];
    let evolved = jails_cmd(&root, None).args(evolve).output().unwrap();
    assert!(
        evolved.status.success(),
        "{}",
        String::from_utf8_lossy(&evolved.stderr)
    );
    for (index, path) in files.iter().enumerate() {
        let source = fs::read_to_string(path).unwrap();
        assert!(source.contains(&format!("controller-reader-edit-{index}")));
        assert!(
            source.contains("/v2/verify"),
            "{}: {source}",
            path.display()
        );
    }
    assert!(
        fs::read_to_string(&files[0])
            .unwrap()
            .contains("@PutMapping")
    );
    assert!(
        fs::read_to_string(&jdl_path)
            .unwrap()
            .contains("// reader route wording\ncomponent controller Verify")
    );

    let stable = snapshot_tree(&root);
    let rerun = jails_cmd(&root, None).args(evolve).output().unwrap();
    assert!(rerun.status.success());
    assert_eq!(
        snapshot_tree(&root),
        stable,
        "controller rerun changed bytes"
    );

    let clean_controller = fs::read_to_string(&files[0]).unwrap();
    fs::write(
        &files[0],
        clean_controller.replace("@PutMapping(path =", "@DeleteMapping(path ="),
    )
    .unwrap();
    let before = snapshot_tree(&root);
    let refused = jails_cmd(&root, None)
        .args([
            "g",
            "controller",
            "Verify",
            "--method",
            "patch",
            "--on",
            "Request",
            "--returns",
            "Response",
            "--path",
            "/v3/verify",
            "--consumes",
            "json",
        ])
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
        before,
        "controller overlap wrote bytes"
    );
    fs::write(&files[0], clean_controller).unwrap();

    let before_body_refusal = snapshot_tree(&root);
    let bodyless = jails_cmd(&root, None)
        .args([
            "g",
            "controller",
            "Verify",
            "--method",
            "get",
            "--on",
            "Request",
        ])
        .output()
        .unwrap();
    assert!(!bodyless.status.success());
    assert!(
        String::from_utf8_lossy(&bodyless.stderr).contains("does not carry"),
        "{}",
        String::from_utf8_lossy(&bodyless.stderr)
    );
    assert_eq!(snapshot_tree(&root), before_body_refusal);

    if real_mvn_available() && real_java_supports_target_release() {
        let path = real_path_without_mvnd();
        let compiled = real_maven_cmd(&root, &path)
            .args(["-q", "-B", "test"])
            .output()
            .unwrap();
        assert!(
            compiled.status.success(),
            "canonical controller did not compile:\n{}\n{}",
            String::from_utf8_lossy(&compiled.stdout),
            String::from_utf8_lossy(&compiled.stderr)
        );
    }

    for (index, path) in files.iter().enumerate() {
        let source = fs::read_to_string(path).unwrap();
        fs::write(
            path,
            source.replace(&format!("\n\n    // controller-reader-edit-{index}"), ""),
        )
        .unwrap();
    }
    let destroyed = jails_cmd(&root, None)
        .args(["destroy", "controller", "Verify", "--force"])
        .output()
        .unwrap();
    assert!(
        destroyed.status.success(),
        "{}",
        String::from_utf8_lossy(&destroyed.stderr)
    );
    assert!(files.iter().all(|path| !path.exists()));
    assert!(
        root.join(".jails/generated/main/java/com/example/demo/domain/Request.java")
            .exists()
    );

    fs::remove_dir_all(root).ok();
}

#[test]
fn canonical_controller_ejection_transfers_the_whole_http_adapter_boundary() {
    let root = temp_dir("canonical-controller-ejection");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();
    let generated = jails_cmd(&root, None)
        .args(["g", "controller", "Health", "--path", "/health"])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let managed = [
        root.join(".jails/generated/main/java/com/example/demo/web/HealthController.java"),
        root.join(".jails/generated/test/java/com/example/demo/web/HealthControllerTest.java"),
    ];
    for (index, path) in managed.iter().enumerate() {
        let source = fs::read_to_string(path).unwrap();
        let split = source.rfind("\n}").unwrap();
        fs::write(
            path,
            format!(
                "{}\n\n    // ejected-controller-edit-{index}{}",
                &source[..split],
                &source[split..]
            ),
        )
        .unwrap();
    }

    let ejected = jails_cmd(&root, None)
        .args(["model", "eject", "art_cmp_controller_health_http"])
        .output()
        .unwrap();
    assert!(
        ejected.status.success(),
        "{}",
        String::from_utf8_lossy(&ejected.stderr)
    );
    let reader = [
        common::generated(
            &root,
            "src/main/java/com/example/demo/web/HealthController.java",
        ),
        common::generated(
            &root,
            "src/test/java/com/example/demo/web/HealthControllerTest.java",
        ),
    ];
    assert!(managed.iter().all(|path| !path.exists()));
    for (index, path) in reader.iter().enumerate() {
        assert!(
            fs::read_to_string(path)
                .unwrap()
                .contains(&format!("ejected-controller-edit-{index}"))
        );
    }
    let exact = reader
        .iter()
        .map(|path| fs::read(path).unwrap())
        .collect::<Vec<_>>();

    let evolved = jails_cmd(&root, None)
        .args(["g", "controller", "Health", "--path", "/healthz"])
        .output()
        .unwrap();
    assert!(
        evolved.status.success(),
        "{}",
        String::from_utf8_lossy(&evolved.stderr)
    );
    for (index, path) in reader.iter().enumerate() {
        assert_eq!(fs::read(path).unwrap(), exact[index]);
    }
    let jdl = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(jdl.contains(r#"route GET "/healthz""#), "{jdl}");

    fs::remove_dir_all(root).ok();
}

#[test]
fn canonical_loadtest_merges_every_project_file_and_refuses_route_overlap_atomically() {
    let root = temp_dir("canonical-loadtest-loop");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();
    for arguments in [
        ["g", "controller", "Health", "--path", "/health"].as_slice(),
        ["add", "loadtest", "--no-start"].as_slice(),
    ] {
        let output = jails_cmd(&root, None).args(arguments).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let load_tests = root.join("load-tests");
    let api_path = load_tests.join("api.js");
    let readme_path = load_tests.join("README.md");
    let token_path = load_tests.join("token-cache.js");
    let initial_api = fs::read_to_string(&api_path).unwrap();
    assert!(
        initial_api
            .contains("{ method: \"GET\", path: \"/health\", handler: \"HealthController#get\" }")
    );
    for (path, edit) in [
        (&readme_path, "\nReader load-test notes.\n"),
        (&token_path, "\nexport const readerTokenHook = true;\n"),
    ] {
        let mut source = fs::read_to_string(path).unwrap();
        source.push_str(edit);
        fs::write(path, source).unwrap();
    }

    let evolved = jails_cmd(&root, None)
        .args(["g", "controller", "Health", "--path", "/healthz"])
        .output()
        .unwrap();
    assert!(
        evolved.status.success(),
        "{}",
        String::from_utf8_lossy(&evolved.stderr)
    );
    let clean_api = fs::read_to_string(&api_path).unwrap();
    assert!(clean_api.contains("path: \"/healthz\""), "{clean_api}");
    assert!(
        fs::read_to_string(&readme_path)
            .unwrap()
            .contains("Reader load-test notes.")
    );
    assert!(
        fs::read_to_string(&token_path)
            .unwrap()
            .contains("readerTokenHook")
    );

    let stable = snapshot_tree(&root);
    let rerun = jails_cmd(&root, None)
        .args(["g", "controller", "Health", "--path", "/healthz"])
        .output()
        .unwrap();
    assert!(rerun.status.success());
    assert_eq!(snapshot_tree(&root), stable);

    fs::write(
        &api_path,
        clean_api.replace("path: \"/healthz\"", "path: \"/reader-health\""),
    )
    .unwrap();
    let before = snapshot_tree(&root);
    let refused = jails_cmd(&root, None)
        .args(["g", "controller", "Health", "--path", "/health-next"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("overlap"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert_eq!(snapshot_tree(&root), before, "loadtest refusal wrote files");

    fs::write(&api_path, clean_api).unwrap();
    let before_remove = snapshot_tree(&root);
    let edited_remove = jails_cmd(&root, None)
        .args(["remove", "loadtest"])
        .output()
        .unwrap();
    assert!(!edited_remove.status.success());
    assert!(
        String::from_utf8_lossy(&edited_remove.stderr).contains("edited by you"),
        "{}",
        String::from_utf8_lossy(&edited_remove.stderr)
    );
    assert_eq!(snapshot_tree(&root), before_remove);

    let clean_readme = include_str!("../golden/cap-loadtest/load-tests/README.md");
    let clean_token = include_str!("../golden/cap-loadtest/load-tests/token-cache.js");
    fs::write(&readme_path, clean_readme).unwrap();
    fs::write(&token_path, clean_token).unwrap();
    let removed = jails_cmd(&root, None)
        .args(["remove", "loadtest", "--force"])
        .output()
        .unwrap();
    assert!(
        removed.status.success(),
        "{}",
        String::from_utf8_lossy(&removed.stderr)
    );
    assert!(!load_tests.exists() || snapshot_tree(&load_tests).is_empty());

    fs::remove_dir_all(root).ok();
}

#[test]
fn canonical_repository_is_a_managed_abi_facet_of_the_record() {
    let root = temp_dir("canonical-repository-loop");
    write_plain_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();

    let record_output = jails_cmd(&root, None)
        .args(["g", "record", "Note", "id:int@pk", "title:string!"])
        .output()
        .unwrap();
    assert!(
        record_output.status.success(),
        "{}",
        String::from_utf8_lossy(&record_output.stderr)
    );
    let jdl_path = root.join(".jails/model.jdl");
    let jdl = fs::read_to_string(&jdl_path).unwrap();
    fs::write(
        &jdl_path,
        jdl.replace(
            "entity Note @id(ent_note) {",
            "entity Note @id(ent_note) { // reader model wording",
        ),
    )
    .unwrap();

    let generated = jails_cmd(&root, None)
        .args(["g", "repo", "Note"])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let jdl = fs::read_to_string(&jdl_path).unwrap();
    assert!(
        jdl.contains("entity Note @id(ent_note) { // reader model wording"),
        "{jdl}"
    );
    assert!(jdl.contains("use repo"), "{jdl}");
    let repository =
        root.join(".jails/generated/main/java/com/example/demo/repository/NoteRepository.java");
    let record = root.join(".jails/generated/main/java/com/example/demo/domain/Note.java");
    let source = fs::read_to_string(&repository).unwrap();
    let reader_source = source.replace(
        "\n}\n",
        "\n\n    default String readerMethod() { return \"reader\"; }\n}\n",
    );
    fs::write(&repository, &reader_source).unwrap();

    let evolved = jails_cmd(&root, None)
        .args(["g", "field", "Note", "done:boolean"])
        .output()
        .unwrap();
    assert!(
        evolved.status.success(),
        "{}",
        String::from_utf8_lossy(&evolved.stderr)
    );
    let evolved_repository = fs::read_to_string(&repository).unwrap();
    assert!(evolved_repository.contains("readerMethod()"));
    assert!(
        fs::read_to_string(&record)
            .unwrap()
            .contains("boolean done"),
        "record did not evolve alongside its repository ABI"
    );
    let stable = snapshot_tree(&root);
    let rerun = jails_cmd(&root, None)
        .args(["g", "repo", "Note"])
        .output()
        .unwrap();
    assert!(rerun.status.success());
    assert_eq!(
        snapshot_tree(&root),
        stable,
        "repository rerun changed bytes"
    );

    fs::write(
        &repository,
        evolved_repository.replace("findById(int id)", "findById(int readerOwnedId)"),
    )
    .unwrap();
    let before = snapshot_tree(&root);
    let refused = jails_cmd(&root, None)
        .args([
            "resource",
            "field",
            "type",
            "Note",
            "id",
            "--to",
            "long",
            "--strategy",
            "safe",
        ])
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
        before,
        "repository overlap wrote bytes"
    );

    fs::write(&repository, &evolved_repository).unwrap();
    let changed = jails_cmd(&root, None)
        .args([
            "resource",
            "field",
            "type",
            "Note",
            "id",
            "--to",
            "long",
            "--strategy",
            "safe",
        ])
        .output()
        .unwrap();
    assert!(
        changed.status.success(),
        "{}",
        String::from_utf8_lossy(&changed.stderr)
    );
    let changed_repository = fs::read_to_string(&repository).unwrap();
    assert!(changed_repository.contains("findById(long id)"));
    assert!(changed_repository.contains("readerMethod()"));

    let before_ejection = snapshot_tree(&root);
    let refused_ejection = jails_cmd(&root, None)
        .args(["model", "eject", "art_ent_note_repository"])
        .output()
        .unwrap();
    assert!(!refused_ejection.status.success());
    assert!(
        String::from_utf8_lossy(&refused_ejection.stderr).contains("managed ABI"),
        "{}",
        String::from_utf8_lossy(&refused_ejection.stderr)
    );
    assert_eq!(
        snapshot_tree(&root),
        before_ejection,
        "repository ABI ejection wrote bytes"
    );

    fs::write(
        &repository,
        changed_repository.replace(
            "\n    default String readerMethod() { return \"reader\"; }\n",
            "",
        ),
    )
    .unwrap();
    let destroyed = jails_cmd(&root, None)
        .args(["destroy", "repo", "Note", "--force"])
        .output()
        .unwrap();
    assert!(
        destroyed.status.success(),
        "{}",
        String::from_utf8_lossy(&destroyed.stderr)
    );
    assert!(!repository.exists());
    assert!(record.exists(), "repository destroy removed the record ABI");
    let jdl = fs::read_to_string(jdl_path).unwrap();
    assert!(!jdl.contains("@repository"), "{jdl}");

    fs::remove_dir_all(root).ok();
}

#[test]
fn canonical_dto_evolves_three_merge_managed_abi_files_without_losing_reader_edits() {
    let root = temp_dir("canonical-dto-loop");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();

    for command in [
        [
            "g",
            "record",
            "Task",
            "id:int@pk",
            "title:string!",
            "note:string?",
        ]
        .as_slice(),
        ["g", "dto", "Task"].as_slice(),
    ] {
        let output = jails_cmd(&root, None).args(command).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let generated = root.join(".jails/generated");
    let request = generated.join("main/java/com/example/demo/web/TaskRequest.java");
    let response = generated.join("main/java/com/example/demo/web/TaskResponse.java");
    let test = generated.join("test/java/com/example/demo/web/TaskDtoTest.java");
    let record = generated.join("main/java/com/example/demo/domain/Task.java");
    for path in [&request, &response, &test] {
        assert!(path.is_file(), "missing {}", path.display());
    }
    let jdl = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(jdl.contains("entity Task @id(ent_task) {"), "{jdl}");
    assert!(jdl.contains("use dto"), "{jdl}");
    assert!(
        fs::read_to_string(root.join("pom.xml"))
            .unwrap()
            .contains("spring-boot-starter-validation")
    );

    for (path, method) in [
        (
            &request,
            "    public String readerRequestMethod() { return title; }",
        ),
        (
            &response,
            "    public String readerResponseMethod() { return title; }",
        ),
        (&test, "    private static void readerTestHelper() {}"),
    ] {
        let source = fs::read_to_string(path).unwrap();
        let split = source.rfind("\n}").unwrap();
        fs::write(
            path,
            format!("{}\n\n{method}{}", &source[..split], &source[split..]),
        )
        .unwrap();
    }

    let evolved = jails_cmd(&root, None)
        .args(["g", "field", "Task", "done:boolean"])
        .output()
        .unwrap();
    assert!(
        evolved.status.success(),
        "{}",
        String::from_utf8_lossy(&evolved.stderr)
    );
    for (path, reader_edit) in [
        (&request, "readerRequestMethod"),
        (&response, "readerResponseMethod"),
        (&test, "readerTestHelper"),
    ] {
        let source = fs::read_to_string(path).unwrap();
        assert!(source.contains(reader_edit), "{source}");
        assert!(source.contains("done"), "{source}");
    }
    let request_with_reader_edits = fs::read_to_string(&request).unwrap();

    let stable = snapshot_tree(&root);
    let rerun = jails_cmd(&root, None)
        .args(["g", "dto", "Task"])
        .output()
        .unwrap();
    assert!(rerun.status.success());
    assert_eq!(snapshot_tree(&root), stable, "DTO rerun changed bytes");

    // **Edited on the line the next render moves.** The component has to be
    // one the request record declares and the change has to reach its Java:
    // `id` is server-assigned and no longer appears in a request at all, and
    // widening a `string` renders the same `String`, so neither had anything
    // for the merge to conflict over. Relaxing `title` turns `@NotBlank
    // String title` into an `Optional<String>`, which is the line the reader
    // renamed.
    assert!(
        request_with_reader_edits.contains("@NotBlank String title"),
        "{request_with_reader_edits}"
    );
    fs::write(
        &request,
        request_with_reader_edits.replace(
            "@NotBlank String title",
            "@NotBlank String readerOwnedTitle",
        ),
    )
    .unwrap();
    let before = snapshot_tree(&root);
    let refused = jails_cmd(&root, None)
        .args([
            "resource",
            "field",
            "nullability",
            "Task",
            "title",
            "--nullable",
        ])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("overlap"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert_eq!(snapshot_tree(&root), before, "DTO overlap wrote bytes");

    fs::write(&request, &request_with_reader_edits).unwrap();
    let changed = jails_cmd(&root, None)
        .args([
            "resource",
            "field",
            "type",
            "Task",
            "id",
            "--to",
            "long",
            "--strategy",
            "safe",
        ])
        .output()
        .unwrap();
    assert!(
        changed.status.success(),
        "{}",
        String::from_utf8_lossy(&changed.stderr)
    );
    // `id` is server-assigned and so is not a request component; what the
    // widening reaches in this file is `toDomain`, which mints it.
    let changed_request = fs::read_to_string(&request).unwrap();
    assert!(changed_request.contains("0L"), "{changed_request}");
    assert!(
        changed_request.contains("readerRequestMethod"),
        "{changed_request}"
    );

    let before_ejection = snapshot_tree(&root);
    let refused_ejection = jails_cmd(&root, None)
        .args(["model", "eject", "art_ent_task_dto_request"])
        .output()
        .unwrap();
    assert!(!refused_ejection.status.success());
    assert!(
        String::from_utf8_lossy(&refused_ejection.stderr).contains("managed ABI"),
        "{}",
        String::from_utf8_lossy(&refused_ejection.stderr)
    );
    assert_eq!(snapshot_tree(&root), before_ejection);

    if real_mvn_available() && real_java_supports_target_release() {
        let path = real_path_without_mvnd();
        let verified = real_maven_cmd(&root, &path)
            .args(["-q", "-B", "test"])
            .output()
            .unwrap();
        assert!(
            verified.status.success(),
            "canonical DTO sources did not compile and test:\n{}\n{}",
            String::from_utf8_lossy(&verified.stdout),
            String::from_utf8_lossy(&verified.stderr)
        );
    }

    fs::write(
        &request,
        changed_request.replace(
            "\n    public String readerRequestMethod() { return title; }\n",
            "",
        ),
    )
    .unwrap();
    fs::write(
        &response,
        fs::read_to_string(&response).unwrap().replace(
            "\n    public String readerResponseMethod() { return title; }\n",
            "",
        ),
    )
    .unwrap();
    fs::write(
        &test,
        fs::read_to_string(&test)
            .unwrap()
            .replace("\n    private static void readerTestHelper() {}\n", ""),
    )
    .unwrap();
    let destroyed = jails_cmd(&root, None)
        .args(["destroy", "dto", "Task", "--force"])
        .output()
        .unwrap();
    assert!(
        destroyed.status.success(),
        "{}",
        String::from_utf8_lossy(&destroyed.stderr)
    );
    assert!(!request.exists());
    assert!(!response.exists());
    assert!(!test.exists());
    assert!(record.exists(), "DTO destroy removed its domain record");
    assert!(
        !fs::read_to_string(root.join(".jails/model.jdl"))
            .unwrap()
            .contains("@dto")
    );

    fs::remove_dir_all(root).ok();
}

#[test]
fn canonical_source_unit_destroy_removes_only_the_selected_artifacts() {
    let root = temp_dir("canonical-source-unit-destroy");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), DEMO_JDL).unwrap();
    let generated = jails_cmd(&root, None)
        .args(["g", "service", "BillingService"])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let destroyed = jails_cmd(&root, None)
        .args(["destroy", "service", "BillingService", "--force"])
        .output()
        .unwrap();
    assert!(
        destroyed.status.success(),
        "{}",
        String::from_utf8_lossy(&destroyed.stderr)
    );
    assert!(
        !root
            .join(".jails/generated/main/java/com/example/demo/service/BillingService.java")
            .exists()
    );
    assert!(
        !root
            .join(".jails/generated/test/java/com/example/demo/service/BillingServiceTest.java")
            .exists()
    );

    let generated = jails_cmd(&root, None)
        .args(["g", "integration-test", "CheckoutIT"])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    assert!(
        fs::read_to_string(root.join("pom.xml"))
            .unwrap()
            .contains("maven-failsafe-plugin")
    );
    let destroyed = jails_cmd(&root, None)
        .args(["destroy", "integration-test", "CheckoutIT", "--force"])
        .output()
        .unwrap();
    assert!(
        destroyed.status.success(),
        "{}",
        String::from_utf8_lossy(&destroyed.stderr)
    );
    assert!(
        !root
            .join(".jails/generated/test/java/com/example/demo/CheckoutIT.java")
            .exists()
    );
    assert!(
        !fs::read_to_string(root.join("pom.xml"))
            .unwrap()
            .contains("maven-failsafe-plugin")
    );
    fs::remove_dir_all(root).ok();
}

#[test]
fn model_check_links_the_real_model_through_the_binary() {
    let root = model_project("model-check", MODEL);
    let before = snapshot_tree(&root);
    let output = jails_cmd(&root, None)
        .args(["model", "check"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("model valid: .jails/model.jdl"), "{stdout}");
    assert!(
        stdout.contains("8 nodes, 1 entities, 1 operations"),
        "{stdout}"
    );
    assert_eq!(
        snapshot_tree(&root),
        before,
        "model check must be read-only"
    );
}

#[test]
fn jdl_is_compiled_directly_as_the_single_authoring_source() {
    let root = jdl_project(
        "model-jdl",
        r#"jdl 1

app Notes @id(project_notes) {
  pkg com.example.notes
  java 26
  platform spring
  build maven
  storage postgres
}

entity Task @id(ent_task) {
  title: string @id(fld_task_title) @notBlank
  done: boolean?
}

enum Status @id(ent_status) {
  OPEN
  CLOSED
}
"#,
    );
    let checked = jails_cmd(&root, None)
        .args(["model", "check"])
        .output()
        .unwrap();
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    assert!(
        String::from_utf8_lossy(&checked.stdout).contains(".jails/model.jdl"),
        "{}",
        String::from_utf8_lossy(&checked.stdout)
    );

    let synced = jails_cmd(&root, None).args(["sync"]).output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );
    let task = fs::read_to_string(
        root.join(".jails/generated/main/java/com/example/notes/domain/Task.java"),
    )
    .unwrap();
    assert!(task.contains("String title"), "{task}");
    assert!(task.contains("Optional<Boolean> done"), "{task}");
    assert!(
        root.join(".jails/generated/main/java/com/example/notes/domain/Status.java")
            .is_file()
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

#[test]
fn jdl_destroy_removes_nested_operations_and_entities_without_legacy_state() {
    let root = jdl_project("model-jdl-destroy", NOTES_JDL);
    for arguments in [
        vec!["g", "record", "Task", "title:string!"],
        vec!["g", "enum", "Status", "OPEN", "CLOSED"],
        vec!["g", "query", "OpenTasks", "title", "--on", "Task"],
    ] {
        let output = jails_cmd(&root, None).args(arguments).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let destroyed_query = jails_cmd(&root, None)
        .args(["destroy", "query", "OpenTasks", "--force"])
        .output()
        .unwrap();
    assert!(
        destroyed_query.status.success(),
        "{}",
        String::from_utf8_lossy(&destroyed_query.stderr)
    );
    let source = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(!source.contains("query OpenTasks"), "{source}");
    assert!(source.contains("entity Task"), "{source}");

    let destroyed_enum = jails_cmd(&root, None)
        .args(["destroy", "enum", "Status", "--force"])
        .output()
        .unwrap();
    assert!(
        destroyed_enum.status.success(),
        "{}",
        String::from_utf8_lossy(&destroyed_enum.stderr)
    );
    let source = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(!source.contains("enum Status"), "{source}");
    assert!(source.contains("entity Task"), "{source}");

    let destroyed_entity = jails_cmd(&root, None)
        .args(["destroy", "record", "Task", "--force"])
        .output()
        .unwrap();
    assert!(
        destroyed_entity.status.success(),
        "{}",
        String::from_utf8_lossy(&destroyed_entity.stderr)
    );
    let source = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(!source.contains("entity Task"), "{source}");
    assert!(!root.join(".jails/ledger.toml").exists());
    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(frozen.status.success());
}

#[test]
fn jdl_storage_preserve_and_revive_toggle_one_entity_declaration() {
    let root = jdl_project("model-jdl-retire-revive", NOTES_JDL);
    write_spring_fixture(&root);
    for arguments in [
        vec![
            "g",
            "scaffold",
            "Note",
            "id:uuid@pk",
            "title:string!(1..200)",
        ],
        vec!["add", "db", "--no-start"],
    ] {
        let output = jails_cmd(&root, None).args(arguments).output().unwrap();
        assert!(output.status.success());
    }
    let migration = root.join("src/main/resources/db/migration/V001__create_notes.sql");
    let migration_before = fs::read(&migration).unwrap();
    let record = root.join(".jails/generated/main/java/com/example/notes/domain/Note.java");
    let initial_migration =
        fs::read_to_string(root.join("src/main/resources/db/migration/V001__create_notes.sql"))
            .unwrap();
    assert!(
        initial_migration.contains("check (char_length(title) between 1 and 200)"),
        "{initial_migration}"
    );

    let retired = jails_cmd(&root, None)
        .args([
            "destroy",
            "scaffold",
            "Note",
            "--storage",
            "preserve",
            "--force",
        ])
        .output()
        .unwrap();
    assert!(
        retired.status.success(),
        "{}",
        String::from_utf8_lossy(&retired.stderr)
    );
    let retired_jdl = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(
        retired_jdl.contains("entity Note @id(ent_note) @retired {"),
        "{retired_jdl}"
    );
    assert!(!record.exists());
    assert_eq!(fs::read(&migration).unwrap(), migration_before);

    let revived = jails_cmd(&root, None)
        .args(["resource", "revive", "Note", "--table", "notes"])
        .output()
        .unwrap();
    assert!(
        revived.status.success(),
        "{}",
        String::from_utf8_lossy(&revived.stderr)
    );
    let active_jdl = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(
        active_jdl.contains("entity Note @id(ent_note) {"),
        "{active_jdl}"
    );
    assert!(!active_jdl.contains("@retired"), "{active_jdl}");
    assert!(record.is_file());
    assert_eq!(fs::read(&migration).unwrap(), migration_before);
}

#[test]
fn jdl_dependencies_and_settings_edit_one_source_and_reconcile_reader_files() {
    let root = jdl_project("model-jdl-dependency-setting", NOTES_JDL);
    fs::write(
        root.join("pom.xml"),
        "<project>\n    <modelVersion>4.0.0</modelVersion>\n</project>\n",
    )
    .unwrap();
    let properties = root.join("src/main/resources/application.properties");
    fs::create_dir_all(properties.parent().unwrap()).unwrap();
    fs::write(&properties, "reader.key=keep\n").unwrap();

    let dependency = jails_cmd(&root, None)
        .args([
            "add",
            "dependency",
            "org.jsoup:jsoup",
            "--version",
            "1.18.3",
            "--scope",
            "test",
        ])
        .output()
        .unwrap();
    assert!(
        dependency.status.success(),
        "{}",
        String::from_utf8_lossy(&dependency.stderr)
    );
    let set = jails_cmd(&root, None)
        .args(["set", "server.port=8080"])
        .output()
        .unwrap();
    assert!(
        set.status.success(),
        "{}",
        String::from_utf8_lossy(&set.stderr)
    );
    let first_jdl = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(
        first_jdl.contains("dep org.jsoup:jsoup @id(dep_"),
        "{first_jdl}"
    );
    assert!(
        first_jdl.contains("@version(\"1.18.3\") @scope(test)"),
        "{first_jdl}"
    );
    assert!(
        first_jdl.contains("prop server.port = \"8080\" @id(set_"),
        "{first_jdl}"
    );
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("<groupId>org.jsoup</groupId>"), "{pom}");
    assert!(pom.contains("<artifactId>jsoup</artifactId>"), "{pom}");
    assert!(pom.contains("<version>1.18.3</version>"), "{pom}");
    assert!(pom.contains("<scope>test</scope>"), "{pom}");
    assert_eq!(
        fs::read_to_string(&properties).unwrap(),
        "reader.key=keep\nserver.port=8080\n"
    );

    let first_model = jails_model::parse_jdl(&first_jdl).unwrap();
    let setting_id = first_model.settings.values().next().unwrap().id.clone();
    let updated = jails_cmd(&root, None)
        .args(["set", "server.port=9090"])
        .output()
        .unwrap();
    assert!(updated.status.success());
    let updated_model =
        jails_model::parse_jdl(&fs::read_to_string(root.join(".jails/model.jdl")).unwrap())
            .unwrap();
    assert_eq!(
        updated_model.settings.values().next().unwrap().id,
        setting_id
    );
    assert_eq!(
        fs::read_to_string(&properties).unwrap(),
        "reader.key=keep\nserver.port=9090\n"
    );

    let removed_dependency = jails_cmd(&root, None)
        .args(["remove", "dependency", "org.jsoup:jsoup"])
        .output()
        .unwrap();
    assert!(removed_dependency.status.success());
    let unset = jails_cmd(&root, None)
        .args(["unset", "server.port"])
        .output()
        .unwrap();
    assert!(unset.status.success());
    let final_jdl = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(!final_jdl.contains("org.jsoup:jsoup"), "{final_jdl}");
    assert!(!final_jdl.contains("prop server.port"), "{final_jdl}");
    assert!(
        !fs::read_to_string(root.join("pom.xml"))
            .unwrap()
            .contains("org.jsoup:jsoup")
    );
    assert_eq!(
        fs::read_to_string(&properties).unwrap(),
        "reader.key=keep\n"
    );
    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(frozen.status.success());
}

#[test]
fn jdl_capability_commands_edit_the_authoring_source_and_recompile() {
    let root = jdl_project("model-jdl-capability", NOTES_JDL);
    let scaffold = jails_cmd(&root, None)
        .args(["g", "scaffold", "Note", "id:uuid@pk", "title:string!"])
        .output()
        .unwrap();
    assert!(scaffold.status.success());

    let added = jails_cmd(&root, None)
        .args(["add", "fake"])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let jdl = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(jdl.contains("cap fake @id(cap_fake)"), "{jdl}");
    let adapter_path = root.join(
        ".jails/generated/main/java/com/example/notes/adapters/memory/InMemoryNoteRepository.java",
    );
    assert!(adapter_path.is_file());

    let removed = jails_cmd(&root, None)
        .args(["remove", "fake", "--force"])
        .output()
        .unwrap();
    assert!(
        removed.status.success(),
        "{}",
        String::from_utf8_lossy(&removed.stderr)
    );
    let jdl = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(!jdl.contains("cap fake"), "{jdl}");
    // The declaration goes; the adapter stays, because this project declares
    // no storage and the scaffold's repository port would then have no
    // implementation at all -- a context that compiles and cannot start. What
    // `remove fake` takes back is the capability, not the bean the resource
    // needs to run.
    assert!(adapter_path.is_file());
    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(frozen.status.success());
}

#[test]
fn jdl_generate_edit_generate_preserves_clean_edits_and_refuses_overlap() {
    let root = jdl_project("model-jdl-iterative-record", NOTES_JDL);
    let generated = root.join(".jails/generated/main/java/com/example/notes/domain/Task.java");

    let first = jails_cmd(&root, None)
        .args(["g", "record", "Task", "title:string!(1..200)"])
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let jdl = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(jdl.contains("entity Task @id(ent_task)"), "{jdl}");
    assert!(jdl.contains("@id(fld_task_title)"), "{jdl}");
    assert!(
        jdl.contains("title: string @id(fld_task_title) @length(1..200) @notBlank"),
        "{jdl}"
    );

    let source = fs::read_to_string(&generated).unwrap();
    assert!(
        source.contains("title length must be between 1 and 200"),
        "{source}"
    );
    let split = source.rfind("\n}").unwrap();
    fs::write(
        &generated,
        format!(
            "{}\n\n    public String handWritten() {{ return title; }}{}",
            &source[..split],
            &source[split..]
        ),
    )
    .unwrap();

    let second = jails_cmd(&root, None)
        .args(["g", "field", "Task", "done:boolean"])
        .output()
        .unwrap();
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    let source = fs::read_to_string(&generated).unwrap();
    assert!(source.contains("handWritten()"), "{source}");
    assert!(source.contains("boolean done"), "{source}");

    fs::write(
        &generated,
        source.replace("title must not be blank", "give me a useful title"),
    )
    .unwrap();
    let third = jails_cmd(&root, None)
        .args(["g", "field", "Task", "priority:int"])
        .output()
        .unwrap();
    assert!(
        third.status.success(),
        "{}",
        String::from_utf8_lossy(&third.stderr)
    );
    let source = fs::read_to_string(&generated).unwrap();
    assert!(source.contains("give me a useful title"), "{source}");
    assert!(source.contains("int priority"), "{source}");

    fs::write(&generated, source.replace("int priority", "long priority")).unwrap();
    let before = snapshot_tree(&root);
    let conflict = jails_cmd(&root, None)
        .args(["g", "field", "Task", "dueAt:instant"])
        .output()
        .unwrap();
    assert!(!conflict.status.success());
    let stderr = String::from_utf8(conflict.stderr).unwrap();
    assert!(stderr.contains("overlapping edit"), "{stderr}");
    assert_eq!(snapshot_tree(&root), before);
}

#[test]
fn compiler_upgrade_uses_the_exact_accepted_projection_as_merge_base() {
    let root = jdl_project("model-compiler-upgrade-base", NOTES_JDL);
    let generated = root.join(".jails/generated/main/java/com/example/notes/domain/Task.java");
    let first = jails_cmd(&root, None)
        .args(["g", "record", "Task", "title:string!"])
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );

    let current_projection = fs::read_to_string(&generated).unwrap();
    let old_projection = current_projection.replace(
        "title must not be blank",
        "title used to have old emitter wording",
    );
    assert_ne!(old_projection, current_projection);
    let split = old_projection.rfind("\n}").unwrap();
    let live = format!(
        "{}\n\n    public String handWritten() {{ return title; }}{}",
        &old_projection[..split],
        &old_projection[split..]
    );
    fs::write(&generated, live).unwrap();

    let lock_path = root.join(".jails/compiler.lock.json");
    let mut lock: serde_json::Value =
        serde_json::from_slice(&fs::read(&lock_path).unwrap()).unwrap();
    let mut projection: jails_contracts::RenderedTree =
        serde_json::from_value(lock["projection"].clone()).unwrap();
    let generated_path = jails_contracts::ProjectPath::parse(
        ".jails/generated/main/java/com/example/notes/domain/Task.java",
    )
    .unwrap();
    projection.files.get_mut(&generated_path).unwrap().bytes = old_projection.into_bytes();
    let projection_bytes = serde_json::to_vec(&projection).unwrap();
    lock["compiler"] = serde_json::Value::String("0.0.0-previous-emitter".to_string());
    lock["projection_digest"] = serde_json::Value::String(format!(
        "sha256:{}",
        jails_support::codec::hex(&jails_support::codec::sha256(&projection_bytes))
    ));
    lock["projection"] = serde_json::to_value(projection).unwrap();
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let evolved = jails_cmd(&root, None)
        .args(["g", "field", "Task", "done:boolean"])
        .output()
        .unwrap();
    assert!(
        evolved.status.success(),
        "{}",
        String::from_utf8_lossy(&evolved.stderr)
    );
    let merged = fs::read_to_string(&generated).unwrap();
    assert!(merged.contains("handWritten()"), "{merged}");
    assert!(merged.contains("boolean done"), "{merged}");
    assert!(merged.contains("title must not be blank"), "{merged}");
    assert!(!merged.contains("old emitter wording"), "{merged}");
}

#[test]
fn jdl_generate_writes_enum_and_scaffold_declarations() {
    let root = jdl_project("model-jdl-generate-profiles", NOTES_JDL);
    let enumeration = jails_cmd(&root, None)
        .args(["g", "enum", "Status", "OPEN", "IN_PROGRESS=in_progress"])
        .output()
        .unwrap();
    assert!(
        enumeration.status.success(),
        "{}",
        String::from_utf8_lossy(&enumeration.stderr)
    );
    let scaffold = jails_cmd(&root, None)
        .args(["g", "scaffold", "Task", "id:uuid@pk", "title:string!"])
        .output()
        .unwrap();
    assert!(
        scaffold.status.success(),
        "{}",
        String::from_utf8_lossy(&scaffold.stderr)
    );
    let jdl = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(jdl.contains("enum Status @id(ent_status)"), "{jdl}");
    assert!(jdl.contains(r#"IN_PROGRESS = "in_progress""#), "{jdl}");
    assert!(jdl.contains("entity Task @id(ent_task) {"), "{jdl}");
    assert!(jdl.contains("use scaffold"), "{jdl}");
    assert!(jdl.contains("id: uuid @id(fld_task_id) @pk"), "{jdl}");
}

#[test]
fn canonical_sync_recompiles_model_state_without_the_legacy_store() {
    let root = model_project("model-sync", MODEL);
    apply_canonical_model(&root, "initial-sync");
    let record = root.join(".jails/generated/main/java/com/example/notes/domain/Note.java");
    let contents = fs::read_to_string(&record).unwrap();
    let split = contents.rfind("\n}").unwrap();
    fs::write(
        &record,
        format!(
            "{}\n\n    public String handWritten() {{ return \"reader\"; }}{}",
            &contents[..split],
            &contents[split..]
        ),
    )
    .unwrap();
    let model_path = root.join(".jails/model.jdl");
    let mut source = fs::read_to_string(&model_path).unwrap();
    source.push_str("\ncap fake @id(cap_fake)\n");
    fs::write(&model_path, source).unwrap();

    let synced = jails_cmd(&root, None).arg("sync").output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );
    assert!(fs::read_to_string(&record).unwrap().contains("handWritten"));
    assert!(
        root.join(
            ".jails/generated/main/java/com/example/notes/adapters/memory/InMemoryNoteRepository.java"
        )
        .is_file()
    );
    for legacy in ["objects", "receipts", "journal", "state"] {
        assert!(
            !root.join(".jails").join(legacy).exists(),
            "canonical sync created legacy state `{legacy}`"
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
fn jdl_fast_test_is_a_capability_in_the_authoring_source() {
    let root = jdl_project("model-jdl-fast-test", NOTES_JDL);
    write_spring_fixture(&root);
    apply_canonical_model(&root, "initial-jdl-fast-test");
    let fake_dir = temp_dir("model-jdl-fast-test-bin");
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
    let jdl = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(jdl.contains("cap fast-test @id(cap_fast_test)"), "{jdl}");
    assert!(
        fs::read_to_string(root.join("pom.xml"))
            .unwrap()
            .contains("junit-platform-console")
    );

    let removed = jails_cmd(&root, Some(&fake_dir))
        .args(["remove", "fast-test", "--force"])
        .output()
        .unwrap();
    assert!(removed.status.success());
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
fn canonical_projects_fail_closed_before_legacy_mutation_routes() {
    let root = model_project("model-no-legacy-routes", MODEL);
    apply_canonical_model(&root, "initial-no-legacy-routes");
    for case in [
        // The textual rename is not routed anywhere -- it is a different
        // operation, and it works. What it refuses is a type the *model*
        // declares, because carrying only the Java would leave the next
        // compilation rendering the old name back.
        (
            "rename Note Memo",
            vec!["rename", "Note", "Memo"],
            "declared in this project's application model",
        ),
        // `fmt`, `app plan` and `app apply` all work on a modelled project:
        // `a_canonical_project_runs_its_own_formatter` and
        // `a_manifest_replays_into_the_model_and_converges` hold them. `init`
        // refuses: it *writes* the manifest, which beside a model is a second
        // editable source.
        ("app init", vec!["app", "init"], "does not route"),
    ] {
        // `adopt` and `modernize` are not refused: both run *before* a project
        // has a model and neither claims anything a later command reconciles,
        // so they write directly.
        let (label, arguments, told) = case;
        let before = snapshot_tree(&root);
        let output = jails_cmd(&root, None).args(&arguments).output().unwrap();
        assert!(!output.status.success(), "{label} unexpectedly passed");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(told), "{label}: {stderr}");
        assert_eq!(snapshot_tree(&root), before, "{label} wrote files");
    }
}

#[test]
fn model_check_json_exposes_the_resolved_stable_id_world() {
    let root = model_project("model-json", MODEL);
    let output = jails_cmd(&root, None)
        .args(["model", "check", "--output", "json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema"], "jails.model-check.v1");
    assert_eq!(report["valid"], true);
    assert_eq!(report["model"]["entities"]["ent_note"]["label"], "note");
    assert_eq!(
        report["model"]["operations"]["op_create_note"]["kind"]["Command"]["on"],
        "ent_note"
    );
}

#[test]
fn model_check_reports_all_semantic_failures_as_one_json_document() {
    // A field the entity does not declare, and an operation whose id is the
    // entity's: one unresolved reference and one collision, so the report has
    // to carry both rather than stopping at the first.
    let invalid = MODEL
        .replace("CreateNote(title)", "CreateNote(missing)")
        .replace("@id(op_create_note)", "@id(ent_note)");
    let root = model_project("model-invalid", &invalid);
    let output = jails_cmd(&root, None)
        .args(["--output", "json", "model", "check"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stderr.is_empty(), "failure was reported twice");
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["valid"], false);
    let diagnostics = report["diagnostics"].as_array().unwrap();
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic["code"] == "model-id-collision")
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic["code"] == "model-field-reference")
    );
}

#[test]
fn model_check_names_the_fix_when_the_default_model_is_missing() {
    let root = temp_dir("model-missing");
    let output = jails_cmd(&root, None)
        .args(["model", "check"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains(".jails/model.jdl"), "{stderr}");
    assert!(stderr.contains("--manifest <path>"), "{stderr}");
}

#[test]
fn model_plan_is_deterministic_and_writes_a_self_verifying_bundle() {
    let root = model_project("model-plan", MODEL);
    let first = root.join("first-plan.json");
    let second = root.join("second-plan.json");
    for path in [&first, &second] {
        let output = jails_cmd(&root, None)
            .args(["model", "plan", "--bundle"])
            .arg(path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(fs::read(&first).unwrap(), fs::read(&second).unwrap());
    let bundle: jails_contracts::PlanBundle =
        serde_json::from_slice(&fs::read(&first).unwrap()).unwrap();
    jails_workspace::verify_bundle(&bundle).unwrap();
    assert_eq!(bundle.plan.operations.len(), 3);
    // Fifteen: the record and its companion test, the repository port, the
    // in-memory adapter that implements it -- without which the service has a
    // port no bean satisfies -- that adapter's own test and the repository
    // contract they share, the service the controller delegates to, the HTTP
    // port, the controller and its test, and the project's `ArchitectureTest`
    // with the `archunit.properties` that points its freeze store at a
    // baseline the reader has to create deliberately. Then the three the
    // operation brings: its `Command` ABI, the `TimeOrderedUuid` its `uuid`
    // key is minted with, and the `.http` request file its route is callable
    // from.
    assert_eq!(bundle.plan.summary.managed_files, 15);
    assert_eq!(bundle.plan.id, bundle.plan.digest.as_str());
}

#[test]
fn model_apply_consumes_the_reviewed_plan_without_recompiling() {
    let root = model_project("model-apply", MODEL);
    let plan = root.join("plan.json");
    let planned = jails_cmd(&root, None)
        .args(["model", "plan", "--bundle"])
        .arg(&plan)
        .output()
        .unwrap();
    assert!(planned.status.success());

    let applied = jails_cmd(&root, None)
        .args(["model", "apply", "--bundle"])
        .arg(&plan)
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let record = root.join(".jails/generated/main/java/com/example/notes/domain/Note.java");
    let source = fs::read_to_string(record).unwrap();
    assert!(source.contains("public record Note"), "{source}");
    assert!(source.contains("UUID id"), "{source}");
    let command = fs::read_to_string(root.join(
        ".jails/generated/main/java/com/example/notes/application/commands/CreateNoteCommand.java",
    ))
    .unwrap();
    assert!(
        command.contains("String ROUTE = \"POST /notes\""),
        "{command}"
    );
    assert!(command.contains("Note execute(Input input)"), "{command}");
    assert!(command.contains("public record Input"), "{command}");

    let retried = jails_cmd(&root, None)
        .args(["--output", "json", "model", "apply", "--bundle"])
        .arg(&plan)
        .output()
        .unwrap();
    assert!(retried.status.success());
    let execution: serde_json::Value = serde_json::from_slice(&retried.stdout).unwrap();
    assert_eq!(execution["files_written"], 0);

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
fn model_apply_rejects_a_stale_plan_before_writing() {
    let root = model_project("model-stale", MODEL);
    let plan = root.join("plan.json");
    let planned = jails_cmd(&root, None)
        .args(["model", "plan", "--bundle"])
        .arg(&plan)
        .output()
        .unwrap();
    assert!(planned.status.success());
    fs::write(
        root.join(".jails/model.jdl"),
        format!("{MODEL}\n# changed\n"),
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
    assert!(!root.join(".jails/generated").exists());
}

#[test]
fn model_eject_transfers_generated_java_once_and_reader_edits_survive() {
    let root = eject_model_project("model-eject");
    apply_canonical_model(&root, "initial-plan");
    let generated = root.join(
        ".jails/generated/main/java/com/example/notes/adapters/memory/InMemoryNoteRepository.java",
    );
    let reader =
        root.join("src/main/java/com/example/notes/adapters/memory/InMemoryNoteRepository.java");
    let generated_bytes = fs::read(&generated).unwrap();
    let model_before = fs::read(root.join(".jails/model.jdl")).unwrap();

    let preview = jails_cmd(&root, None)
        .args([
            "model",
            "eject",
            "art_ent_note_repository_memory",
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
        fs::read(root.join(".jails/model.jdl")).unwrap(),
        model_before
    );
    assert_eq!(fs::read(&generated).unwrap(), generated_bytes);
    assert!(!reader.exists());

    let applied = jails_cmd(&root, None)
        .args(["model", "eject", "art_ent_note_repository_memory"])
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    assert!(!generated.exists());
    assert_eq!(fs::read(&reader).unwrap(), generated_bytes);
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(
        model.contains("eject art_ent_note_repository_memory @id(eject_"),
        "{model}"
    );
    assert!(
        root.join(".jails/generated/main/java/com/example/notes/domain/Note.java")
            .exists()
    );
    assert!(
        root.join(".jails/generated/main/java/com/example/notes/repository/NoteRepository.java")
            .exists()
    );

    let mut edited = fs::read_to_string(&reader).unwrap();
    edited.push_str("// reader-owned customization\n");
    fs::write(&reader, &edited).unwrap();
    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(
        frozen.status.success(),
        "{}",
        String::from_utf8_lossy(&frozen.stderr)
    );
    assert_eq!(fs::read_to_string(&reader).unwrap(), edited);

    let before_retry = snapshot_tree(&root);
    let retried = jails_cmd(&root, None)
        .args(["model", "eject", "art_ent_note_repository_memory"])
        .output()
        .unwrap();
    assert!(!retried.status.success());
    let stderr = String::from_utf8(retried.stderr).unwrap();
    assert!(stderr.contains("already reader-owned"), "{stderr}");
    assert_eq!(snapshot_tree(&root), before_retry);
}

#[test]
fn jdl_ejection_transfers_only_one_artifact_and_records_inline_ownership() {
    let root = jdl_project("model-jdl-eject", NOTES_JDL);
    let scaffold = jails_cmd(&root, None)
        .args(["g", "scaffold", "Note", "id:uuid@pk", "title:string!"])
        .output()
        .unwrap();
    assert!(scaffold.status.success());
    let fake = jails_cmd(&root, None)
        .args(["add", "fake"])
        .output()
        .unwrap();
    assert!(fake.status.success());

    let generated = root.join(
        ".jails/generated/main/java/com/example/notes/adapters/memory/InMemoryNoteRepository.java",
    );
    let reader =
        root.join("src/main/java/com/example/notes/adapters/memory/InMemoryNoteRepository.java");
    let generated_bytes = fs::read(&generated).unwrap();
    let ejected = jails_cmd(&root, None)
        .args(["model", "eject", "art_ent_note_repository_memory"])
        .output()
        .unwrap();
    assert!(
        ejected.status.success(),
        "{}",
        String::from_utf8_lossy(&ejected.stderr)
    );
    assert!(!generated.exists());
    assert_eq!(fs::read(&reader).unwrap(), generated_bytes);
    let jdl = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(
        jdl.contains("eject art_ent_note_repository_memory @id(eject_"),
        "{jdl}"
    );
    assert!(
        root.join(".jails/generated/main/java/com/example/notes/domain/Note.java")
            .is_file()
    );
    assert!(
        root.join(".jails/generated/main/java/com/example/notes/repository/NoteRepository.java")
            .is_file()
    );

    let mut edited = fs::read_to_string(&reader).unwrap();
    edited.push_str("// reader owns this implementation\n");
    fs::write(&reader, &edited).unwrap();
    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(frozen.status.success());
    assert_eq!(fs::read_to_string(&reader).unwrap(), edited);
}

#[test]
fn factory_ejection_transfers_only_the_testkit_implementation_boundary() {
    let root = jdl_project("model-jdl-factory-eject", NOTES_JDL);
    for command in [
        ["g", "record", "Note", "title:string!"].as_slice(),
        ["g", "factory", "Note"].as_slice(),
    ] {
        let output = jails_cmd(&root, None).args(command).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let generated =
        root.join(".jails/generated/test/java/com/example/notes/testkit/NoteFactory.java");
    let reader = root.join("src/test/java/com/example/notes/testkit/NoteFactory.java");
    let record = root.join(".jails/generated/main/java/com/example/notes/domain/Note.java");
    let ejected = jails_cmd(&root, None)
        .args(["model", "eject", "art_ent_note_factory"])
        .output()
        .unwrap();
    assert!(
        ejected.status.success(),
        "{}",
        String::from_utf8_lossy(&ejected.stderr)
    );
    assert!(!generated.exists());
    assert!(record.exists(), "factory ejection removed the record ABI");
    let mut owned = fs::read_to_string(&reader).unwrap();
    owned.push_str("// reader owns only this factory\n");
    fs::write(&reader, &owned).unwrap();

    let evolved = jails_cmd(&root, None)
        .args(["g", "field", "Note", "priority:int"])
        .output()
        .unwrap();
    assert!(
        evolved.status.success(),
        "{}",
        String::from_utf8_lossy(&evolved.stderr)
    );
    assert!(
        fs::read_to_string(&record)
            .unwrap()
            .contains("int priority"),
        "managed ABI did not evolve"
    );
    assert_eq!(fs::read_to_string(&reader).unwrap(), owned);
    assert!(!generated.exists());

    let before = snapshot_tree(&root);
    let refused = jails_cmd(&root, None)
        .args(["destroy", "factory", "Note", "--force"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("reader-owned"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert_eq!(snapshot_tree(&root), before, "ejected destroy wrote bytes");
}

#[test]
fn model_eject_refuses_a_reader_destination_collision_without_writing() {
    let root = eject_model_project("model-eject-collision");
    apply_canonical_model(&root, "initial-plan");
    let reader =
        root.join("src/main/java/com/example/notes/adapters/memory/InMemoryNoteRepository.java");
    fs::create_dir_all(reader.parent().unwrap()).unwrap();
    fs::write(&reader, "package com.example.notes.domain;\n// mine\n").unwrap();
    let before = snapshot_tree(&root);

    let output = jails_cmd(&root, None)
        .args(["model", "eject", "art_ent_note_repository_memory"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("already exists"), "{stderr}");
    assert!(stderr.contains("move or remove"), "{stderr}");
    assert_eq!(snapshot_tree(&root), before);
}

#[test]
fn model_eject_plan_refuses_a_destination_created_after_review() {
    let root = eject_model_project("model-eject-stale");
    apply_canonical_model(&root, "initial-plan");
    let plan = root.join("eject-plan.json");
    let planned = jails_cmd(&root, None)
        .args([
            "model",
            "eject",
            "art_ent_note_repository_memory",
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
    let model_before = fs::read(root.join(".jails/model.jdl")).unwrap();
    let generated = root.join(
        ".jails/generated/main/java/com/example/notes/adapters/memory/InMemoryNoteRepository.java",
    );
    let reader =
        root.join("src/main/java/com/example/notes/adapters/memory/InMemoryNoteRepository.java");
    fs::create_dir_all(reader.parent().unwrap()).unwrap();
    fs::write(
        &reader,
        "package com.example.notes.domain;\n// appeared later\n",
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
    assert_eq!(
        fs::read(root.join(".jails/model.jdl")).unwrap(),
        model_before
    );
    assert!(generated.exists());
    assert_eq!(
        fs::read_to_string(&reader).unwrap(),
        "package com.example.notes.domain;\n// appeared later\n"
    );
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
fn canonical_generate_edit_generate_preserves_clean_edits_and_refuses_overlap() {
    let root = model_project("model-iterative-record", EMPTY_MODEL);
    let generated = root.join(".jails/generated/main/java/com/example/notes/domain/Task.java");

    let first = jails_cmd(&root, None)
        .args(["g", "record", "Task", "title:string!"])
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );

    let source = fs::read_to_string(&generated).unwrap();
    let split = source.rfind("\n}").unwrap();
    let edited = format!(
        "{}\n\n    public String handWritten() {{ return title; }}{}",
        &source[..split],
        &source[split..]
    );
    fs::write(&generated, edited).unwrap();

    let second = jails_cmd(&root, None)
        .args(["g", "field", "Task", "done:boolean"])
        .output()
        .unwrap();
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    let source = fs::read_to_string(&generated).unwrap();
    assert!(source.contains("handWritten()"), "{source}");
    assert!(source.contains("boolean done"), "{source}");

    let edited = source.replace("title must not be blank", "give me a useful title");
    fs::write(&generated, edited).unwrap();
    let third = jails_cmd(&root, None)
        .args(["g", "field", "Task", "priority:int"])
        .output()
        .unwrap();
    assert!(
        third.status.success(),
        "{}",
        String::from_utf8_lossy(&third.stderr)
    );
    let source = fs::read_to_string(&generated).unwrap();
    assert!(source.contains("give me a useful title"), "{source}");
    assert!(source.contains("int priority"), "{source}");

    fs::write(&generated, source.replace("int priority", "long priority")).unwrap();
    let before = snapshot_tree(&root);
    let conflict = jails_cmd(&root, None)
        .args(["g", "field", "Task", "dueAt:instant"])
        .output()
        .unwrap();
    assert!(!conflict.status.success());
    let stderr = String::from_utf8(conflict.stderr).unwrap();
    assert!(stderr.contains("overlapping edit"), "{stderr}");
    assert_eq!(snapshot_tree(&root), before);
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
fn canonical_preserve_table_rename_moves_artifacts_and_keeps_hand_edits() {
    let root = model_project("model-rename-preserve-edits", EMPTY_MODEL);
    let old = root.join(".jails/generated/main/java/com/example/notes/domain/Task.java");
    let new = root.join(".jails/generated/main/java/com/example/notes/domain/WorkItem.java");
    let generated = jails_cmd(&root, None)
        .args(["g", "record", "Task", "title:string!"])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
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
    let model = jails_model::parse_jdl(&fs::read_to_string(root.join(".jails/model.jdl")).unwrap())
        .unwrap();
    let entity = model.entities.values().next().unwrap();
    assert_eq!(entity.id.to_string(), "ent_task");
    assert_eq!(entity.names.java_type, "WorkItem");
    assert_eq!(entity.names.sql_table, "tasks");
    assert!(
        fs::read_to_string(root.join(".jails/model.jdl"))
            .unwrap()
            .contains("entity WorkItem @id(ent_task)")
    );
}

#[test]
fn jdl_rename_keeps_the_stable_identity_and_reader_edits() {
    let root = jdl_project("model-jdl-rename-preserve-edits", NOTES_JDL);
    let old = root.join(".jails/generated/main/java/com/example/notes/domain/Task.java");
    let new = root.join(".jails/generated/main/java/com/example/notes/domain/WorkItem.java");
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
    let generated = root.join(".jails/generated/main/java/com/example/notes/domain/Task.java");
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
    let destination =
        root.join(".jails/generated/main/java/com/example/notes/domain/WorkItem.java");
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
fn every_operation_kind_lowers_to_a_typed_managed_abi() {
    // One of every routed kind beside [`MODEL`]'s command, plus the event the
    // transition emits.
    let source = MODEL.replace(
        "  }\n}\n",
        "  }\n\n  \
         event NoteCreated(id, title) @id(op_note_created) {\n  }\n\n  \
         query OpenNotes(title) @id(op_open_notes) {\n    order by [id]\n    limit 50\n    \
         route GET \"/notes\"\n  }\n\n  \
         transition RenameNote(title) @id(op_rename_note) {\n    select [id]\n    \
         update [title]\n    emit note_created\n    route PATCH \"/notes/{id}\"\n  }\n}\n",
    );
    assert_ne!(
        source, MODEL,
        "the entity block moved and this splice missed it"
    );
    let root = model_project("model-operation-abi", &source);
    let plan = root.join("plan.json");
    let planned = jails_cmd(&root, None)
        .args(["model", "plan", "--bundle"])
        .arg(&plan)
        .output()
        .unwrap();
    assert!(
        planned.status.success(),
        "{}",
        String::from_utf8_lossy(&planned.stderr)
    );
    let applied = jails_cmd(&root, None)
        .args(["model", "apply", "--bundle"])
        .arg(&plan)
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );

    let generated = root.join(".jails/generated/main/java/com/example/notes");
    let query =
        fs::read_to_string(generated.join("application/queries/OpenNotesQuery.java")).unwrap();
    assert!(query.contains("String ROUTE = \"GET /notes\""), "{query}");
    assert!(query.contains("int DEFAULT_LIMIT = 50"), "{query}");
    assert!(query.contains("List<Note> execute(Input input)"), "{query}");

    let transition =
        fs::read_to_string(generated.join("application/transitions/RenameNoteTransition.java"))
            .unwrap();
    assert!(
        transition.contains("String ROUTE = \"PATCH /notes/{id}\""),
        "{transition}"
    );
    assert!(
        transition.contains("Note execute(UUID id, Input input)"),
        "{transition}"
    );

    let event = fs::read_to_string(generated.join("domain/events/NoteCreatedEvent.java")).unwrap();
    assert!(event.contains("public record NoteCreatedEvent"), "{event}");
    assert!(event.contains("UUID id"), "{event}");
    assert!(event.contains("String title"), "{event}");

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
fn familiar_operation_commands_are_model_patches_not_legacy_routes() {
    let root = model_project("model-operation-frontends", EMPTY_MODEL);
    for arguments in [
        vec!["g", "scaffold", "Note", "id:uuid@pk", "title:string!"],
        vec!["g", "event", "NoteCreated", "id", "title", "--on", "Note"],
        vec![
            "g",
            "usecase",
            "CreateNote",
            "title",
            "--on",
            "Note",
            "--path",
            "/notes",
        ],
        vec![
            "g",
            "query",
            "OpenNotes",
            "title",
            "--on",
            "Note",
            "--limit",
            "50",
            "--path",
            "/notes/search",
        ],
        vec![
            "g",
            "transition",
            "RenameNote",
            "title",
            "--on",
            "Note",
            "--yields",
            "NoteCreated",
            "--path",
            "/notes/{id}",
            "--method",
            "patch",
        ],
    ] {
        let output = jails_cmd(&root, None).args(arguments).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    for declaration in [
        "event NoteCreated",
        "command CreateNote",
        "query OpenNotes",
        "transition RenameNote",
        r#"route POST "/notes""#,
        r#"route GET "/notes/search""#,
        r#"route PATCH "/notes/{id}""#,
        "emit note_created",
    ] {
        assert!(
            model.contains(declaration),
            "missing `{declaration}`:\n{model}"
        );
    }
    let generated = root.join(".jails/generated/main/java/com/example/notes");
    for relative in [
        "domain/events/NoteCreatedEvent.java",
        "application/commands/CreateNoteCommand.java",
        "application/queries/OpenNotesQuery.java",
        "application/transitions/RenameNoteTransition.java",
    ] {
        assert!(generated.join(relative).is_file(), "missing {relative}");
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
fn familiar_operation_commands_edit_nested_jdl_and_compile_typed_abis() {
    let root = jdl_project("model-jdl-operation-frontends", NOTES_JDL);
    for arguments in [
        vec!["g", "scaffold", "Note", "id:uuid@pk", "title:string!"],
        vec!["g", "event", "NoteCreated", "id", "title", "--on", "Note"],
        vec![
            "g",
            "usecase",
            "CreateNote",
            "title",
            "--on",
            "Note",
            "--path",
            "/notes",
        ],
        vec![
            "g",
            "query",
            "OpenNotes",
            "title",
            "--on",
            "Note",
            "--limit",
            "50",
            "--path",
            "/notes/search",
        ],
        vec![
            "g",
            "transition",
            "RenameNote",
            "title",
            "--on",
            "Note",
            "--yields",
            "NoteCreated",
            "--path",
            "/notes/{id}",
            "--method",
            "patch",
        ],
    ] {
        let output = jails_cmd(&root, None).args(arguments).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let jdl = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    for declaration in [
        "event NoteCreated(id, title) @id(op_note_created)",
        "command CreateNote(title) @id(op_create_note)",
        "query OpenNotes(title) @id(op_open_notes)",
        "transition RenameNote(title) @id(op_rename_note)",
        r#"route POST "/notes""#,
        r#"route GET "/notes/search""#,
        r#"route PATCH "/notes/{id}""#,
        "emit note_created",
    ] {
        assert!(jdl.contains(declaration), "missing `{declaration}`:\n{jdl}");
    }
    let generated = root.join(".jails/generated/main/java/com/example/notes");
    for relative in [
        "domain/events/NoteCreatedEvent.java",
        "application/commands/CreateNoteCommand.java",
        "application/queries/OpenNotesQuery.java",
        "application/transitions/RenameNoteTransition.java",
    ] {
        assert!(generated.join(relative).is_file(), "missing {relative}");
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
fn canonical_api_adapters_merge_then_eject_at_the_operation_boundary() {
    // [`MODEL`]'s command, and a query and a transition beside it, so the
    // `api` capability has one operation of each routed kind to adapt.
    let source = MODEL.replace(
        "  }\n}\n",
        "  }\n\n  \
         query OpenNotes(title) @id(op_open_notes) {\n    route GET \"/notes/search\"\n  }\n\n  \
         transition RenameNote(title) @id(op_rename_note) {\n    select [id]\n    \
         update [title]\n    route PATCH \"/notes/{id}\"\n  }\n}\n",
    );
    assert_ne!(
        source, MODEL,
        "the entity block moved and this splice missed it"
    );
    let root = model_project("model-api-operation-adapters", &source);
    write_spring_fixture(&root);

    // `db` first: an operation's controller takes a port that only the JDBC
    // adapter implements, so an `api` project without storage compiles and
    // fails to start.
    assert!(
        jails_cmd(&root, None)
            .args(["add", "db", "--no-start"])
            .status()
            .unwrap()
            .success()
    );
    let added = jails_cmd(&root, None)
        .args(["add", "api"])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let generated = root.join(".jails/generated/main/java/com/example/notes");
    let command = generated.join("adapters/http/CreateNoteController.java");
    let query =
        fs::read_to_string(generated.join("adapters/http/OpenNotesController.java")).unwrap();
    let transition =
        fs::read_to_string(generated.join("adapters/http/RenameNoteController.java")).unwrap();
    assert!(
        fs::read_to_string(&command)
            .unwrap()
            .contains("RequestMethod.POST")
    );
    assert!(query.contains("@ModelAttribute OpenNotesQuery.Input input"));
    assert!(query.contains("RequestMethod.GET"));
    assert!(
        transition.contains("@PathVariable(\"id\") UUID id"),
        "{transition}"
    );
    assert!(transition.contains("RequestMethod.PATCH"), "{transition}");
    assert!(
        fs::read_to_string(root.join("pom.xml"))
            .unwrap()
            .contains("spring-boot-starter-web")
    );

    let contents = fs::read_to_string(&command).unwrap();
    let split = contents.rfind("\n}").unwrap();
    fs::write(
        &command,
        format!(
            "{}\n\n    public String handWritten() {{ return \"reader\"; }}{}",
            &contents[..split],
            &contents[split..]
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
    assert!(
        fs::read_to_string(&command)
            .unwrap()
            .contains("handWritten")
    );

    let ejected = jails_cmd(&root, None)
        .args(["model", "eject", "art_op_create_note_http"])
        .output()
        .unwrap();
    assert!(
        ejected.status.success(),
        "{}",
        String::from_utf8_lossy(&ejected.stderr)
    );
    let reader =
        root.join("src/main/java/com/example/notes/adapters/http/CreateNoteController.java");
    assert!(!command.exists());
    assert!(fs::read_to_string(&reader).unwrap().contains("handWritten"));
    assert!(
        generated
            .join("application/commands/CreateNoteCommand.java")
            .is_file(),
        "ejecting the controller must not eject its managed ABI"
    );
    let reader_bytes = fs::read(&reader).unwrap();
    let evolved_again = jails_cmd(&root, None)
        .args(["resource", "field", "add", "Note", "priority:int?"])
        .output()
        .unwrap();
    assert!(
        evolved_again.status.success(),
        "{}",
        String::from_utf8_lossy(&evolved_again.stderr)
    );
    assert_eq!(fs::read(&reader).unwrap(), reader_bytes);
    assert!(!command.exists());

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
        common::assert_main_sources_compile(&root, &path, "canonical API adapters");
    }
}

#[test]
fn operation_frontend_refuses_field_type_drift_without_writing() {
    let root = model_project("model-operation-field-drift", EMPTY_MODEL);
    let scaffold = jails_cmd(&root, None)
        .args(["g", "scaffold", "Note", "id:uuid@pk", "title:string!"])
        .output()
        .unwrap();
    assert!(scaffold.status.success());
    let before = snapshot_tree(&root);
    let refused = jails_cmd(&root, None)
        .args(["g", "query", "OpenNotes", "title:int", "--on", "Note"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    let stderr = String::from_utf8(refused.stderr).unwrap();
    assert!(
        stderr.contains("disagrees with canonical entity field"),
        "{stderr}"
    );
    assert!(stderr.contains("fix:"), "{stderr}");
    assert_eq!(snapshot_tree(&root), before, "refusal mutated the project");
}

#[test]
fn canonical_destroy_is_model_subtraction_and_whole_tree_recompilation() {
    let root = model_project("model-destroy-entity", EMPTY_MODEL);
    let generated = jails_cmd(&root, None)
        .args(["g", "scaffold", "Note", "id:uuid@pk", "title:string!"])
        .output()
        .unwrap();
    assert!(generated.status.success());
    let before = snapshot_tree(&root);

    let preview = jails_cmd(&root, None)
        .args(["d", "scaffold", "Note", "--pretend", "--force"])
        .output()
        .unwrap();
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    assert_eq!(snapshot_tree(&root), before, "destroy preview wrote files");

    let plan_directory = temp_dir("model-destroy-plan");
    let plan = plan_directory.join("destroy.json");
    let planned = jails_cmd(&root, None)
        .args(["d", "scaffold", "Note", "--force", "--plan-out"])
        .arg(&plan)
        .output()
        .unwrap();
    assert!(
        planned.status.success(),
        "{}",
        String::from_utf8_lossy(&planned.stderr)
    );
    assert_eq!(snapshot_tree(&root), before, "plan-out applied destroy");
    let bundle: jails_contracts::PlanBundle =
        serde_json::from_slice(&fs::read(&plan).unwrap()).unwrap();
    assert!(bundle.plan.operations.iter().any(|operation| matches!(
        operation,
        jails_contracts::PlannedOperation::PublishMergedTree { .. }
    )));
    assert!(bundle.plan.operations.iter().any(|operation| matches!(
        operation,
        jails_contracts::PlannedOperation::ReplaceModelFile { .. }
    )));
    assert!(matches!(
        bundle.plan.operations.last(),
        Some(jails_contracts::PlannedOperation::ReplaceStateFile { path, .. })
            if path.as_str() == ".jails/compiler.lock.json"
    ));

    let applied = jails_cmd(&root, None)
        .args(["model", "apply", "--bundle"])
        .arg(&plan)
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(!model.contains("[entities.note]"), "{model}");
    assert!(
        !root
            .join(".jails/generated/main/java/com/example/notes/domain/Note.java")
            .exists()
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

#[test]
fn canonical_destroy_refuses_while_operations_reference_the_entity() {
    let root = model_project("model-destroy-referenced", EMPTY_MODEL);
    for arguments in [
        vec!["g", "scaffold", "Note", "id:uuid@pk", "title:string!"],
        vec!["g", "query", "OpenNotes", "title", "--on", "Note"],
    ] {
        let output = jails_cmd(&root, None).args(arguments).output().unwrap();
        assert!(output.status.success());
    }
    let before = snapshot_tree(&root);
    let refused = jails_cmd(&root, None)
        .args(["d", "scaffold", "Note", "--force"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    let stderr = String::from_utf8(refused.stderr).unwrap();
    assert!(stderr.contains("operation OpenNotes"), "{stderr}");
    assert!(stderr.contains("pointing at nothing"), "{stderr}");
    assert_eq!(snapshot_tree(&root), before, "refusal mutated the project");

    let removed_query = jails_cmd(&root, None)
        .args(["d", "query", "OpenNotes", "--force"])
        .output()
        .unwrap();
    assert!(
        removed_query.status.success(),
        "{}",
        String::from_utf8_lossy(&removed_query.stderr)
    );
    assert!(
        !root
            .join(".jails/generated/main/java/com/example/notes/application/queries/OpenNotesQuery.java")
            .exists()
    );
    let removed_entity = jails_cmd(&root, None)
        .args(["d", "scaffold", "Note", "--force"])
        .output()
        .unwrap();
    assert!(
        removed_entity.status.success(),
        "{}",
        String::from_utf8_lossy(&removed_entity.stderr)
    );
    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(frozen.status.success());
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
    let adapter_path = root.join(
        ".jails/generated/main/java/com/example/notes/adapters/memory/InMemoryNoteRepository.java",
    );
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
    assert!(!model.contains("[capabilities.fake]"), "{model}");
    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(frozen.status.success());
}

#[test]
fn dependency_is_semantic_model_data_and_one_exact_maven_projection() {
    let root = model_project("model-dependency", EMPTY_MODEL);
    fs::write(
        root.join("pom.xml"),
        "<project>\n    <modelVersion>4.0.0</modelVersion>\n    <!-- reader-owned -->\n</project>\n",
    )
    .unwrap();
    let before = snapshot_tree(&root);
    let preview = jails_cmd(&root, None)
        .args([
            "add",
            "dependency",
            "org.jsoup:jsoup",
            "--version",
            "1.18.3",
            "--scope",
            "runtime",
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
        snapshot_tree(&root),
        before,
        "dependency preview wrote files"
    );

    let added = jails_cmd(&root, None)
        .args([
            "add",
            "dependency",
            "org.jsoup:jsoup",
            "--version",
            "1.18.3",
            "--scope",
            "runtime",
        ])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(model.contains("dep org.jsoup:jsoup @id(dep_"), "{model}");
    assert!(model.contains("@scope(runtime)"), "{model}");
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("<!-- reader-owned -->"), "{pom}");
    assert!(pom.contains("<!-- jails:dependencies -->"), "{pom}");
    assert!(pom.contains("<groupId>org.jsoup</groupId>"), "{pom}");
    assert!(pom.contains("<artifactId>jsoup</artifactId>"), "{pom}");
    assert!(pom.contains("<version>1.18.3</version>"), "{pom}");
    assert!(pom.contains("<scope>runtime</scope>"), "{pom}");

    let reapplied = jails_cmd(&root, None)
        .args([
            "--output",
            "json",
            "add",
            "dependency",
            "org.jsoup:jsoup",
            "--version",
            "1.18.3",
            "--scope",
            "runtime",
        ])
        .output()
        .unwrap();
    assert!(reapplied.status.success());
    let execution: serde_json::Value = serde_json::from_slice(&reapplied.stdout).unwrap();
    assert_eq!(execution["files_written"], 0);

    let removed = jails_cmd(&root, None)
        .args(["remove", "dependency", "org.jsoup:jsoup"])
        .output()
        .unwrap();
    assert!(
        removed.status.success(),
        "{}",
        String::from_utf8_lossy(&removed.stderr)
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(!model.contains("org.jsoup"), "{model}");
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("<!-- reader-owned -->"), "{pom}");
    assert!(!pom.contains("jails:dependencies"), "{pom}");
    assert!(!pom.contains("jsoup"), "{pom}");
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
fn dependency_reconciliation_crosses_the_kotlin_gradle_binary_boundary() {
    let root = gradle_model_project(
        "model-dependency-gradle",
        EMPTY_MODEL,
        "build.gradle.kts",
        "plugins { java }\n",
    );
    let added = jails_cmd(&root, None)
        .args([
            "add",
            "dependency",
            "org.jsoup:jsoup",
            "--version",
            "1.18.3",
            "--scope",
            "test",
        ])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let build = fs::read_to_string(root.join("build.gradle.kts")).unwrap();
    assert!(build.contains("// jails:dependencies"), "{build}");
    assert!(
        build.contains("testImplementation(\"org.jsoup:jsoup:1.18.3\")"),
        "{build}"
    );
    // **No source root, because nothing is generated.** This project declares
    // one dependency and no Java, and a source root for a directory that may
    // stay empty is an edit to the reader's build with nothing behind it --
    // and one that then outlives every reason for it.
    assert!(
        !build.contains("java.srcDir(\".jails/generated/main/java\")"),
        "{build}"
    );

    let removed = jails_cmd(&root, None)
        .args(["remove", "dependency", "org.jsoup:jsoup"])
        .output()
        .unwrap();
    assert!(
        removed.status.success(),
        "{}",
        String::from_utf8_lossy(&removed.stderr)
    );
    let build = fs::read_to_string(root.join("build.gradle.kts")).unwrap();
    assert!(!build.contains("org.jsoup:jsoup"), "{build}");
    assert!(!build.contains("jails:dependencies"), "{build}");
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

#[test]
fn maven_source_root_is_an_exact_reader_patch_and_converges() {
    let root = model_project("model-maven-source-root", MODEL);
    fs::write(
        root.join("pom.xml"),
        "<project>\n    <modelVersion>4.0.0</modelVersion>\n    <groupId>com.example</groupId>\n    <artifactId>notes</artifactId>\n    <version>1</version>\n</project>\n",
    )
    .unwrap();
    let plan = root.join("plan.json");
    let planned = jails_cmd(&root, None)
        .args(["model", "plan", "--bundle"])
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
        jails_contracts::PlannedOperation::PatchReaderFile { path, .. }
            if path.as_str() == "pom.xml"
    )));

    let applied = jails_cmd(&root, None)
        .args(["model", "apply", "--bundle"])
        .arg(&plan)
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("jails:generated-source-root"), "{pom}");
    assert!(pom.contains("build-helper-maven-plugin"), "{pom}");
    assert!(
        pom.contains("<source>.jails/generated/main/java</source>"),
        "{pom}"
    );
    assert!(
        root.join(".jails/generated/main/java/com/example/notes/domain/Note.java")
            .is_file()
    );

    let converged = root.join("converged.json");
    let replanned = jails_cmd(&root, None)
        .args(["model", "plan", "--bundle"])
        .arg(&converged)
        .output()
        .unwrap();
    assert!(
        replanned.status.success(),
        "{}",
        String::from_utf8_lossy(&replanned.stderr)
    );
    let bundle: jails_contracts::PlanBundle =
        serde_json::from_slice(&fs::read(converged).unwrap()).unwrap();
    assert!(
        bundle.plan.operations.is_empty(),
        "{:#?}",
        bundle.plan.operations
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

#[test]
fn gradle_source_root_is_an_exact_reader_patch_and_converges() {
    let root = gradle_model_project(
        "model-gradle-source-root",
        MODEL,
        "build.gradle",
        "plugins { id 'java' }\n",
    );
    let plan = root.join("plan.json");
    let planned = jails_cmd(&root, None)
        .args(["model", "plan", "--bundle"])
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
        jails_contracts::PlannedOperation::PatchReaderFile { path, .. }
            if path.as_str() == "build.gradle"
    )));

    let applied = jails_cmd(&root, None)
        .args(["model", "apply", "--bundle"])
        .arg(&plan)
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let build = fs::read_to_string(root.join("build.gradle")).unwrap();
    assert!(build.contains("jails:generated-source-root"), "{build}");
    assert!(
        build.contains("java.srcDir('.jails/generated/main/java')"),
        "{build}"
    );

    let converged = root.join("converged.json");
    let replanned = jails_cmd(&root, None)
        .args(["model", "plan", "--bundle"])
        .arg(&converged)
        .output()
        .unwrap();
    assert!(
        replanned.status.success(),
        "{}",
        String::from_utf8_lossy(&replanned.stderr)
    );
    let bundle: jails_contracts::PlanBundle =
        serde_json::from_slice(&fs::read(converged).unwrap()).unwrap();
    assert!(
        bundle.plan.operations.is_empty(),
        "{:#?}",
        bundle.plan.operations
    );
}

#[test]
fn reader_build_file_precondition_blocks_all_writes_from_a_stale_plan() {
    let root = model_project("model-stale-reader-file", MODEL);
    fs::write(root.join("pom.xml"), "<project>\n</project>\n").unwrap();
    let plan = root.join("plan.json");
    let planned = jails_cmd(&root, None)
        .args(["model", "plan", "--bundle"])
        .arg(&plan)
        .output()
        .unwrap();
    assert!(planned.status.success());
    fs::write(
        root.join("pom.xml"),
        "<project>\n    <!-- reader edit -->\n</project>\n",
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
    assert!(stderr.contains("pom.xml"), "{stderr}");
    assert!(!root.join(".jails/generated").exists());
    assert_eq!(
        fs::read_to_string(root.join("pom.xml")).unwrap(),
        "<project>\n    <!-- reader edit -->\n</project>\n"
    );
}

#[test]
fn canonical_settings_preview_update_reconcile_and_unset_end_to_end() {
    let root = model_project("model-setting-main", EMPTY_MODEL);
    let properties = root.join("src/main/resources/application.properties");
    fs::create_dir_all(properties.parent().unwrap()).unwrap();
    fs::write(&properties, "# reader\nreader.key=keep\n").unwrap();
    let before = snapshot_tree(&root);

    let preview = jails_cmd(&root, None)
        .args(["set", "server.port=8080", "--pretend"])
        .output()
        .unwrap();
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    assert_eq!(snapshot_tree(&root), before, "setting preview wrote files");

    let added = jails_cmd(&root, None)
        .args(["set", "server.port=8080"])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let first_model =
        jails_model::parse_jdl(&fs::read_to_string(root.join(".jails/model.jdl")).unwrap())
            .unwrap();
    let first = first_model.settings.values().next().unwrap();
    let stable_id = first.id.clone();
    assert_eq!(first.key, "server.port");
    assert_eq!(first.value, "8080");
    assert_eq!(first.target, jails_model::SettingTarget::Main);
    assert_eq!(
        fs::read_to_string(&properties).unwrap(),
        "# reader\nreader.key=keep\nserver.port=8080\n"
    );

    let updated = jails_cmd(&root, None)
        .args(["set", "server.port=9090"])
        .output()
        .unwrap();
    assert!(
        updated.status.success(),
        "{}",
        String::from_utf8_lossy(&updated.stderr)
    );
    let updated_model =
        jails_model::parse_jdl(&fs::read_to_string(root.join(".jails/model.jdl")).unwrap())
            .unwrap();
    let updated_setting = updated_model.settings.values().next().unwrap();
    assert_eq!(updated_setting.id, stable_id);
    assert_eq!(updated_setting.value, "9090");
    assert_eq!(
        fs::read_to_string(&properties).unwrap(),
        "# reader\nreader.key=keep\nserver.port=9090\n"
    );

    let repeated = jails_cmd(&root, None)
        .args(["--output", "json", "set", "server.port=9090"])
        .output()
        .unwrap();
    assert!(repeated.status.success());
    let execution: serde_json::Value = serde_json::from_slice(&repeated.stdout).unwrap();
    assert_eq!(execution["files_written"], 0);

    let removed = jails_cmd(&root, None)
        .args(["unset", "server.port"])
        .output()
        .unwrap();
    assert!(
        removed.status.success(),
        "{}",
        String::from_utf8_lossy(&removed.stderr)
    );
    assert_eq!(
        fs::read_to_string(&properties).unwrap(),
        "# reader\nreader.key=keep\n"
    );
    let final_model =
        jails_model::parse_jdl(&fs::read_to_string(root.join(".jails/model.jdl")).unwrap())
            .unwrap();
    assert!(final_model.settings.is_empty());
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
fn canonical_test_setting_creates_the_additive_config_overlay() {
    let root = model_project("model-setting-test", EMPTY_MODEL);
    let output = jails_cmd(&root, None)
        .args(["set", "spring.datasource.url=jdbc:h2:mem:test", "--tests"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !root
            .join("src/main/resources/application.properties")
            .exists()
    );
    assert_eq!(
        fs::read_to_string(root.join("src/test/resources/config/application.properties")).unwrap(),
        "spring.datasource.url=jdbc:h2:mem:test\n"
    );
    let model = jails_model::parse_jdl(&fs::read_to_string(root.join(".jails/model.jdl")).unwrap())
        .unwrap();
    let setting = model.settings.values().next().unwrap();
    assert_eq!(setting.target, jails_model::SettingTarget::Test);
}

#[test]
fn canonical_setting_refuses_reader_owned_collision_without_writes() {
    let root = model_project("model-setting-collision", EMPTY_MODEL);
    let properties = root.join("src/main/resources/application.properties");
    fs::create_dir_all(properties.parent().unwrap()).unwrap();
    fs::write(&properties, "server.port=7000\n").unwrap();
    let before = snapshot_tree(&root);
    let output = jails_cmd(&root, None)
        .args(["set", "server.port=8080"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("reader-owned"), "{stderr}");
    assert_eq!(
        snapshot_tree(&root),
        before,
        "collision refusal wrote files"
    );
}

#[test]
fn canonical_setting_plan_is_stale_if_a_missing_reader_file_appears() {
    let root = model_project("model-setting-stale-missing", EMPTY_MODEL);
    let plan = root.join("setting-plan.json");
    let planned = jails_cmd(&root, None)
        .args(["set", "server.port=8080", "--plan-out"])
        .arg(&plan)
        .output()
        .unwrap();
    assert!(
        planned.status.success(),
        "{}",
        String::from_utf8_lossy(&planned.stderr)
    );
    let model_before = fs::read(root.join(".jails/model.jdl")).unwrap();
    let properties = root.join("src/main/resources/application.properties");
    fs::create_dir_all(properties.parent().unwrap()).unwrap();
    fs::write(&properties, "reader.key=late\n").unwrap();

    let applied = jails_cmd(&root, None)
        .args(["model", "apply", "--bundle"])
        .arg(&plan)
        .output()
        .unwrap();
    assert!(!applied.status.success());
    let stderr = String::from_utf8(applied.stderr).unwrap();
    assert!(
        stderr.contains("precondition") || stderr.contains("changed"),
        "{stderr}"
    );
    assert_eq!(
        fs::read(root.join(".jails/model.jdl")).unwrap(),
        model_before,
        "stale setting plan changed the model"
    );
    assert_eq!(fs::read_to_string(properties).unwrap(), "reader.key=late\n");
}

#[test]
fn maven_build_compiles_the_managed_source_root_end_to_end() {
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

    // The fixture's own Spring pom, not a bare one: `scaffold` means a DTO, a
    // controller and a service, so a build with nothing on the classpath is a
    // build the compiler refuses by name. Compiling the whole scaffold against
    // the dependencies it declares is the question the managed source root
    // has to answer.
    let root = model_project("model-maven-real-compile", EMPTY_MODEL);
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
    let fake = jails_cmd(&root, None)
        .args(["add", "fake"])
        .output()
        .unwrap();
    assert!(
        fake.status.success(),
        "{}",
        String::from_utf8_lossy(&fake.stderr)
    );

    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    // **The last closing brace, not the first.** `app` and `entity` both end
    // in `\n}\n`, and a plain `replace` puts four operations inside the app
    // block -- which refuses as `unknown app property `command``, about a
    // declaration the test meant for the entity.
    let close = model
        .rfind("\n}\n")
        .expect("the entity block ends the model");
    let with_operations = format!(
        "{}\n\n  command CreateNote(title) @id(op_create_note) {{\n    route POST \"/notes\"\n  }}\n\n  \
         query OpenNotes(title) @id(op_open_notes) {{\n    limit 25\n    route GET \"/notes\"\n  }}\n\n  \
         transition RenameNote(title) @id(op_rename_note) {{\n    select [id]\n    \
         update [title]\n    route PATCH \"/notes/{{id}}\"\n  }}\n\n  \
         event NoteCreated(id, title) @id(op_note_created) {{\n  }}{}",
        &model[..close],
        &model[close..],
    );
    fs::write(root.join(".jails/model.jdl"), with_operations).unwrap();
    let operation_plan = root.join("operations.json");
    let planned = jails_cmd(&root, None)
        .args(["model", "plan", "--bundle"])
        .arg(&operation_plan)
        .output()
        .unwrap();
    assert!(
        planned.status.success(),
        "{}",
        String::from_utf8_lossy(&planned.stderr)
    );
    let applied = jails_cmd(&root, None)
        .args(["model", "apply", "--bundle"])
        .arg(&operation_plan)
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );

    let path = real_path_without_mvnd();
    let compiled = real_maven_cmd(&root, &path)
        .args(["-q", "-B", "compile"])
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "managed source root did not compile through Maven:\n{}\n{}",
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&compiled.stderr)
    );
    assert!(
        root.join("target/classes/com/example/notes/domain/Note.class")
            .is_file(),
        "Maven succeeded without compiling the managed Java source"
    );
    for class in [
        "repository/NoteRepository.class",
        "service/NoteService.class",
        "ports/http/NoteHttpPort.class",
        "adapters/memory/InMemoryNoteRepository.class",
        "application/commands/CreateNoteCommand.class",
        "application/queries/OpenNotesQuery.class",
        "application/transitions/RenameNoteTransition.class",
        "domain/events/NoteCreatedEvent.class",
    ] {
        assert!(
            root.join("target/classes/com/example/notes")
                .join(class)
                .is_file(),
            "Maven did not compile semantic scaffold facet {class}"
        );
    }
}

fn canonical_database_project(label: &str) -> PathBuf {
    let root = model_project(label, EMPTY_MODEL);
    write_spring_fixture(&root);
    for arguments in [
        vec!["g", "scaffold", "Note", "id:uuid@pk", "title:string!"],
        vec!["add", "db", "--no-start"],
    ] {
        let output = jails_cmd(&root, None).args(arguments).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    root
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

    let csv_main =
        root.join(".jails/generated/main/java/com/example/notes/adapters/DatasetReader.java");
    let csv_test =
        root.join(".jails/generated/test/java/com/example/notes/adapters/DatasetReaderTest.java");
    let json_main = root.join(".jails/generated/main/java/com/example/notes/adapters/Json.java");
    let json_test =
        root.join(".jails/generated/test/java/com/example/notes/adapters/JsonTest.java");
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
    let record_main = root.join(".jails/generated/main/java/com/example/notes/domain/Feed.java");
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
    let moved_record = root.join(".jails/generated/main/java/com/example/notes/imports/Feed.java");
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
        fs::read_to_string(
            root.join(".jails/generated/main/java/com/example/notes/feeds/Feed.java")
        )
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
    assert!(!csv_main.exists());
    assert!(!csv_test.exists());
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

    let fake = root.join(".jails/generated/test/java/com/example/notes/testkit/Fake.java");
    let fake_test = root.join(".jails/generated/test/java/com/example/notes/testkit/FakeTest.java");
    let http = root.join(".jails/generated/main/java/com/example/notes/api/AdminServer.java");
    let http_test =
        root.join(".jails/generated/test/java/com/example/notes/api/AdminServerTest.java");
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
    assert!(!http.exists());
    assert!(!http_test.exists());

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
    assert!(!fake.exists());
    assert!(!fake_test.exists());

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

    let generated = root.join(".jails/generated");
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
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("<goal>add-test-resource</goal>"), "{pom}");
    assert!(
        pom.contains("<directory>.jails/generated/test/resources</directory>"),
        "{pom}"
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
        assert!(!generated.join(path).exists(), "managed {path} survived");
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

    let generated = root.join(".jails/generated");
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
    for (index, (path, managed)) in expected.iter().enumerate() {
        assert!(!managed.exists(), "managed {path} survived");
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

    let managed =
        root.join(".jails/generated/test/java/com/example/demo/adapters/H2DatabaseTest.java");
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
    assert!(!managed.exists());
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

    let managed =
        root.join(".jails/generated/test/java/com/example/demo/ActuatorEndpointsTest.java");
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
    assert!(!managed.exists());
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
        root.join(".jails/generated/main/java/com/example/demo/CacheConfig.java"),
        root.join(".jails/generated/test/java/com/example/demo/CacheConfigTest.java"),
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
    for ((managed, reader), expected) in managed.iter().zip(&reader).zip(live_bytes) {
        assert!(!managed.exists());
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
        root.join(".jails/generated/main/java/com/example/demo/CorsConfig.java"),
        root.join(".jails/generated/test/java/com/example/demo/CorsConfigTest.java"),
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
    for ((managed, reader), expected) in managed.iter().zip(&reader).zip(live_bytes) {
        assert!(!managed.exists());
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
        root.join(".jails/generated/main/java/com/example/demo/MetricsConfig.java"),
        root.join(".jails/generated/main/java/com/example/demo/AppMetrics.java"),
        root.join(".jails/generated/test/java/com/example/demo/AppMetricsTest.java"),
        root.join(".jails/generated/test/java/com/example/demo/PrometheusScrapeTest.java"),
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
    for ((managed, reader), expected) in managed.iter().zip(&reader).zip(live_bytes) {
        assert!(!managed.exists());
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
        root.join(".jails/generated/main/java/com/example/demo/SecurityConfig.java"),
        root.join(".jails/generated/main/java/com/example/demo/ProductionSecurityConfig.java"),
        root.join(".jails/generated/main/java/com/example/demo/ScopeAuthorizer.java"),
        root.join(".jails/generated/test/java/com/example/demo/SecurityConfigTest.java"),
        root.join(".jails/generated/test/java/com/example/demo/ScopeAuthorizerTest.java"),
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
    for ((managed, reader), expected) in managed.iter().zip(&reader).zip(live_bytes) {
        assert!(!managed.exists());
        assert_eq!(fs::read(reader).unwrap(), expected);
    }
    for cors in [
        root.join(".jails/generated/main/java/com/example/demo/CorsConfig.java"),
        root.join(".jails/generated/test/java/com/example/demo/CorsConfigTest.java"),
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
        root.join(".jails/generated/main/java/com/example/demo/EventHub.java"),
        root.join(".jails/generated/main/java/com/example/demo/SchedulingConfig.java"),
        root.join(".jails/generated/main/java/com/example/demo/web/EventStreamController.java"),
        root.join(".jails/generated/test/java/com/example/demo/EventHubTest.java"),
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
    for ((managed, reader), expected) in managed.iter().zip(&reader).zip(live_bytes) {
        assert!(!managed.exists());
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
        root.join(".jails/generated/main/java/com/example/demo/adapters/KeyValueStore.java"),
        root.join(".jails/generated/test/java/com/example/demo/adapters/KeyValueStoreIT.java"),
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
        root.join(".jails/generated/main/java/com/example/demo/messaging/KafkaConfig.java"),
        root.join(
            ".jails/generated/main/java/com/example/demo/messaging/NonRetryableException.java",
        ),
        root.join(".jails/generated/test/java/com/example/demo/messaging/KafkaConfigTest.java"),
        root.join(".jails/generated/test/java/com/example/demo/KafkaTestcontainersConfig.java"),
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
        root.join(".jails/generated/main/java/com/example/demo/Mailer.java"),
        root.join(".jails/generated/test/java/com/example/demo/MailerIT.java"),
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
        root.join(".jails/generated/test/java/com/example/demo/testkit/Faults.java"),
        root.join(".jails/generated/test/java/com/example/demo/testkit/FaultsTest.java"),
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
        root.join(".jails/generated/test/java/com/example/demo/testkit/Fake.java")
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
        root.join(".jails/generated/test/java/com/example/demo/testkit/Fake.java")
            .is_file(),
        "coverage removal touched the independent fake boundary"
    );
}

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

    let managed = root.join(".jails/generated/main/java/com/example/notes");
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
    assert!(!adapter.exists(), "ejected query adapter stayed managed");
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

    let managed = root.join(".jails/generated/main/java/com/example/notes");
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
    assert!(!command.exists());
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
    assert!(!transition.exists());
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

#[test]
fn canonical_preserve_table_rename_keeps_the_accepted_database_projection() {
    let root = canonical_database_project("model-db-preserve-table-rename");
    let migration = root.join("src/main/resources/db/migration/V001__create_notes.sql");
    let migration_before = fs::read(&migration).unwrap();
    let old = root.join(".jails/generated/main/java/com/example/notes/domain/Note.java");
    let new = root.join(".jails/generated/main/java/com/example/notes/domain/Memo.java");

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
        root.join(
            ".jails/generated/main/java/com/example/notes/adapters/jdbc/JdbcNoteRepository.java"
        )
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
    let record = fs::read_to_string(
        root.join(".jails/generated/main/java/com/example/notes/domain/Note.java"),
    )
    .unwrap();
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
    let record = root.join(".jails/generated/main/java/com/example/notes/domain/Note.java");
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
        fs::read(root.join(".jails/generated/main/java/com/example/notes/domain/Note.java"))
            .unwrap();
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
        fs::read(root.join(".jails/generated/main/java/com/example/notes/domain/Note.java"))
            .unwrap(),
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
    let record = root.join(".jails/generated/main/java/com/example/notes/domain/Note.java");
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
    let record = root.join(".jails/generated/main/java/com/example/notes/domain/Note.java");
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
    let record = root.join(".jails/generated/main/java/com/example/notes/domain/Note.java");
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
    let model = jails_model::parse_jdl(&fs::read_to_string(root.join(".jails/model.jdl")).unwrap())
        .unwrap();
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
fn jdl_composite_index_is_nested_model_data_and_preserves_record_edits() {
    let root = jdl_project("model-jdl-composite-index", NOTES_JDL);
    write_spring_fixture(&root);
    for arguments in [
        vec!["g", "scaffold", "Note", "id:uuid@pk", "title:string!"],
        vec!["add", "db", "--no-start"],
    ] {
        let output = jails_cmd(&root, None).args(arguments).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let record = root.join(".jails/generated/main/java/com/example/notes/domain/Note.java");
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
    let source = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(
        source.contains("index [title, id desc] @id(idx_note_"),
        "{source}"
    );
    let model = jails_model::parse_jdl(&source).unwrap();
    let index = model
        .entities
        .values()
        .next()
        .unwrap()
        .indexes
        .values()
        .next()
        .unwrap();
    assert_eq!(index.columns.len(), 2);
    assert!(
        fs::read_to_string(&record)
            .unwrap()
            .contains("handWritten()")
    );
    let migration = fs::read_dir(root.join("src/main/resources/db/migration"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("V002__add_")
        })
        .expect("JDL index migration");
    assert!(
        fs::read_to_string(migration)
            .unwrap()
            .contains(" on notes (title, id desc);")
    );
    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(frozen.status.success());
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

    let record = root.join(".jails/generated/main/java/com/example/notes/domain/Note.java");
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
    let record = root.join(".jails/generated/main/java/com/example/notes/domain/Note.java");

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
    let model = jails_model::parse_jdl(&fs::read_to_string(root.join(".jails/model.jdl")).unwrap())
        .unwrap();
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
    let model = jails_model::parse_jdl(&fs::read_to_string(root.join(".jails/model.jdl")).unwrap())
        .unwrap();
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
            .join(".jails/generated/main/java/com/example/notes/domain/Note.java")
            .exists()
    );
    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(frozen.status.success());
}

/// A transition's field roles come from `select`/`update`/`emit` and nowhere
/// else, and every emitted event is published.
///
/// The four assertions are the four roles JDL v1 §12.4 separates: `id`
/// selects, `version` guards and is incremented by the compiler, `status`
/// updates, and both events publish. It reads the emitted SQL rather than the
/// model, because the property is what reaches Java.
#[test]
fn a_transition_separates_selector_guard_and_update_and_emits_every_event() {
    let root = jdl_project(
        "model-transition-roles",
        r#"jdl 1

app Roles {
  pkg com.example.roles
  java 26
  platform spring
  build maven
  storage postgres
}

entity Task {
  use scaffold

  id:      uuid   @pk
  title:   string @notBlank
  status:  string
  version: long   @version @nonnegative

  transition Close(id, version, status) {
    select [id]
    if-match required
    emit TaskClosed
    emit TaskAudited
  }

  event TaskClosed(id, status) {}
  event TaskAudited(id, status) {}
}
"#,
    );
    write_spring_fixture(&root);
    let synced = jails_cmd(&root, None).args(["sync"]).output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );

    let adapter = fs::read_to_string(root.join(
        ".jails/generated/main/java/com/example/roles/adapters/jdbc/JdbcCloseTransition.java",
    ))
    .unwrap();
    assert!(
        adapter.contains(r#""id = :id""#),
        "the primary key selects the row: {adapter}"
    );
    assert!(
        adapter.contains(r#""version = :expected_version""#),
        "a version parameter guards rather than updates: {adapter}"
    );
    assert!(
        adapter.contains("set status = :status, version = version + 1"),
        "only `status` is written from input, and the compiler owns the version: {adapter}"
    );
    assert!(
        !adapter.contains("id = :id, ") && !adapter.contains("set id ="),
        "the selector is never an update target: {adapter}"
    );
    assert!(
        adapter.contains("new TaskClosedEvent(") && adapter.contains("new TaskAuditedEvent("),
        "every emitted event is published, not just the first: {adapter}"
    );
}

/// A command publishes the events it declares: `command Create(...) { emit
/// TaskCreated }` connects the payload record to the adapter. Commands and
/// transitions go through one `publications` helper so the rule cannot get
/// two answers.
#[test]
fn a_command_publishes_every_event_it_declares() {
    let root = jdl_project(
        "model-command-emits",
        r#"jdl 1

app Cmd {
  pkg com.example.cmd
  java 26
  platform spring
  build maven
  storage postgres
}

entity Task {
  use scaffold

  id:     uuid   @pk
  title:  string @notBlank
  status: string

  command Create(title, status) {
    emit TaskCreated
    emit TaskAudited
  }

  event TaskCreated(id, status) {}
  event TaskAudited(id, status) {}
}
"#,
    );
    write_spring_fixture(&root);
    let synced = jails_cmd(&root, None).args(["sync"]).output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );

    let adapter =
        fs::read_to_string(root.join(
            ".jails/generated/main/java/com/example/cmd/adapters/jdbc/JdbcCreateCommand.java",
        ))
        .unwrap();
    assert!(
        adapter.contains("new TaskCreatedEvent(result.id(), result.status())")
            && adapter.contains("new TaskAuditedEvent(result.id(), result.status())"),
        "both declared events publish, from the row the database returned: {adapter}"
    );
    assert!(
        adapter.contains("ApplicationEventPublisher events"),
        "the publisher is injected only where it is used: {adapter}"
    );
}

/// A declared sort direction reaches the SQL: `order by [createdAt desc, id]`
/// compiles to `order by created_at desc, id`, so a query declared
/// newest-first returns newest-first.
#[test]
fn a_query_orders_by_the_direction_it_declares() {
    let root = jdl_project(
        "model-query-ordering",
        r#"jdl 1

app Ord {
  pkg com.example.ord
  java 26
  platform spring
  build maven
  storage postgres
}

entity Task {
  use scaffold

  id:        uuid    @pk
  title:     string  @notBlank
  createdAt: instant @default(now())

  query Recent(title?) {
    order by [createdAt desc, id]
    limit 25
  }
}
"#,
    );
    write_spring_fixture(&root);
    let synced = jails_cmd(&root, None).args(["sync"]).output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );

    let adapter = fs::read_to_string(
        root.join(".jails/generated/main/java/com/example/ord/adapters/jdbc/JdbcRecentQuery.java"),
    )
    .unwrap();
    assert!(
        adapter.contains(r#"" order by created_at desc, id""#),
        "`desc` survives and `asc` stays implicit: {adapter}"
    );
    assert!(
        adapter.contains(r#"" limit 25""#),
        "the declared ceiling survives: {adapter}"
    );
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

/// Adding a field leaves the model and the source it just wrote agreeing.
///
/// `AddField` is positional, and "already sorted by label" must not be read
/// as "the source states no order": a JDL v1 entity can be declared
/// alphabetically by chance,
/// where appending `delta` to `alpha, beta, gamma` would put it third in the
/// model and fourth in the file.
///
/// The entities below are chosen so the two answers differ: each is in label
/// order, and each added field sorts into the middle. `model check --frozen`
/// is the assertion, because it is the one that recompiles from the file and
/// compares bytes -- exactly the check a divergence here breaks.
#[test]
fn adding_a_field_leaves_the_model_and_its_source_agreeing() {
    let jdl = jdl_project(
        "model-field-placement-jdl",
        r#"jdl 1

app Ord {
  pkg com.example.ord
  java 26
  platform plain
  build maven
  storage none
}

entity Task {
  alpha: uuid   @pk
  beta:  string
  gamma: string
}
"#,
    );
    write_plain_fixture(&jdl);
    let synced = jails_cmd(&jdl, None).args(["sync"]).output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );
    let added = jails_cmd(&jdl, None)
        .args(["g", "field", "Task", "delta:string?"])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let frozen = jails_cmd(&jdl, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(
        frozen.status.success(),
        "JDL v1 appends, so the record must too:\n{}",
        String::from_utf8_lossy(&frozen.stderr)
    );
    let record =
        fs::read_to_string(jdl.join(".jails/generated/main/java/com/example/ord/domain/Task.java"))
            .unwrap();
    let delta = record.find("delta").expect("the added component");
    let gamma = record.find("gamma").expect("the existing component");
    assert!(
        gamma < delta,
        "an appended declaration stays appended:\n{record}"
    );

    // The other side of the same rule: a TOML table states no order, so
    // re-parsing sorts by label and the patch has to place it there.
    let toml = model_project("model-field-placement-toml", MODEL);
    write_spring_fixture(&toml);
    apply_canonical_model(&toml, "initial-field-placement");
    let added = jails_cmd(&toml, None)
        .args(["g", "field", "Note", "body:string?"])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let frozen = jails_cmd(&toml, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(
        frozen.status.success(),
        "a TOML table re-parses by label, so the record must too:\n{}",
        String::from_utf8_lossy(&frozen.stderr)
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

/// `jails add db` on a JDL v1 project sets the storage axis, not a capability.
///
/// v1's closed capability registry has no `db`: `storage postgres` is what the
/// reader declares and the `db` capability is what the linker materializes
/// from it; appending `cap db` would write a model that does not parse.
#[test]
fn add_db_on_a_v1_model_sets_the_storage_axis() {
    let root = temp_dir("v1-add-db");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/model.jdl"),
        "jdl 1\n// a comment the edit must not disturb\napp Demo @id(project_demo) {\n  \
         pkg com.example.demo\n  java 26\n  platform spring\n  build maven\n  storage none\n}\n",
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

    let source = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(source.contains("storage postgres"), "{source}");
    assert!(!source.contains("storage none"), "{source}");
    assert!(!source.contains("cap db"), "{source}");
    // The edit is one line inside the `app` block; everything else survives.
    assert!(
        source.contains("// a comment the edit must not disturb"),
        "{source}"
    );
    assert!(source.contains("platform spring"), "{source}");

    // And it is the real capability: the JDBC adapter and the schema follow.
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("spring-boot-starter-jdbc"), "{pom}");

    // `sqlite` is deliberately not routed to the axis: v1 carries `cap sqlite`
    // as well as `storage sqlite`, and the capability is the primary spelling.
    let sqlite = jails_cmd(&root, None)
        .args(["add", "sqlite"])
        .output()
        .unwrap();
    assert!(
        sqlite.status.success(),
        "{}",
        String::from_utf8_lossy(&sqlite.stderr)
    );
    let source = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(source.contains("cap sqlite"), "{source}");
    assert!(source.contains("storage postgres"), "{source}");
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
    let container = fs::read_to_string(
        root.join(".jails/generated/test/java/com/example/demo/TestcontainersConfig.java"),
    )
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

/// A command run from a subdirectory is about the same project.
///
/// The dispatch switch and the build-file walk are one walk: `jails g record`
/// typed in `src/main/java` renders into `.jails/generated` and writes no
/// `.jails/ledger.toml`.
#[test]
fn a_canonical_command_run_from_a_subdirectory_is_still_canonical() {
    let root = temp_dir("canonical-subdirectory");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/model.jdl"),
        "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  \
         platform spring\n  build maven\n  storage none\n}\n",
    )
    .unwrap();
    let inside = common::generated(&root, "src/main/java/com/example/demo");

    let generated = jails_cmd(&inside, None)
        .args(["g", "record", "Sub", "name:String"])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    assert!(
        root.join(".jails/generated/main/java/com/example/demo/domain/Sub.java")
            .exists(),
        "the record was not written to the managed tree"
    );
    assert!(
        !root.join(".jails/ledger.toml").exists(),
        "a canonical project was given a legacy ledger"
    );
    assert!(
        !inside.join("Sub.java").exists(),
        "the record was written into the reader's own tree"
    );

    // The read-only commands answer about the same project, and name the
    // model project-relative rather than by whatever absolute path this
    // directory happens to have.
    let checked = jails_cmd(&inside, None)
        .args(["model", "check"])
        .output()
        .unwrap();
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    assert!(
        String::from_utf8(checked.stdout)
            .unwrap()
            .contains("model valid: .jails/model.jdl"),
        "`model check` did not report the project's own model"
    );
    let synced = jails_cmd(&inside, None).arg("sync").output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );
}

/// `model eject` resolves the boundary against the project it is in.
///
/// It re-emits the tree to find which files an ejection owns, and that
/// emission has to see the captured Boot version: a `BootCondition::Spring`
/// capability pack emits nothing under `spring_boot: None`, and an ejection
/// resolved that way refuses "emits no ejectable Java implementation" with
/// the files plainly on disk.
#[test]
fn canonical_eject_transfers_a_spring_only_capability_pack() {
    let root = temp_dir("canonical-eject-spring-pack");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/model.jdl"),
        "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  \
         platform spring\n  build maven\n  storage none\n}\n",
    )
    .unwrap();

    let added = jails_cmd(&root, None)
        .args(["add", "kafka", "--no-start"])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let managed =
        root.join(".jails/generated/main/java/com/example/demo/messaging/KafkaConfig.java");
    assert!(
        managed.exists(),
        "the pack emitted no managed configuration"
    );

    let ejected = jails_cmd(&root, None)
        .args(["model", "eject", "cap_kafka"])
        .output()
        .unwrap();
    assert!(
        ejected.status.success(),
        "{}",
        String::from_utf8_lossy(&ejected.stderr)
    );
    assert!(
        common::generated(
            &root,
            "src/main/java/com/example/demo/messaging/KafkaConfig.java"
        )
        .exists(),
        "the implementation was not transferred to reader source"
    );
    assert!(
        !managed.exists(),
        "an ejected artifact is still in the managed tree"
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

/// `g usecase --yields <Event>` writes both lines an outbox needs.
///
/// The flag is a delivery policy, not just an event: `--yields` asks for a
/// transactional outbox, so writing `emit E` alone would honour it with
/// direct publication -- a write and a publish that can fail independently --
/// under a flag that asked for the stronger guarantee. That substitution is
/// exactly what `deliver` exists to make impossible, so it is what this pins.
#[test]
fn canonical_usecase_yields_writes_an_outbox_delivery_policy() {
    let root = temp_dir("canonical-usecase-yields");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/model.jdl"),
        "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  \
         platform spring\n  build maven\n  storage postgres\n}\n",
    )
    .unwrap();
    for command in [
        ["g", "scaffold", "Task", "id:uuid@pk", "title:string!"].as_slice(),
        // The store encodes the staged payload with `Json`, so the capability
        // that writes it is a prerequisite.
        ["add", "json"].as_slice(),
        // `id: uuid` rather than `id`: the event's own identity, minted, not
        // the row's. An outbox staged on the row's id makes
        // `on conflict (id) do nothing` discard the second event about it.
        [
            "g",
            "event",
            "TaskCreated",
            "id:uuid",
            "title",
            "--on",
            "Task",
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
    let generated = jails_cmd(&root, None)
        .args([
            "g",
            "usecase",
            "CreateTask",
            "title",
            "--on",
            "Task",
            "--yields",
            "TaskCreated",
        ])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(model.contains("emit task_created"), "{model}");
    assert!(model.contains("deliver outbox"), "{model}");
    // And the policy is honoured rather than recorded: the adapter stages.
    let command = fs::read_to_string(root.join(
        ".jails/generated/main/java/com/example/demo/adapters/jdbc/JdbcCreateTaskCommand.java",
    ))
    .unwrap();
    assert!(command.contains("outbox.stage("), "{command}");
    assert!(command.contains("@Transactional"), "{command}");
}

/// `deliver outbox` end to end: the model says it, the project has it.
///
/// The compiler's own tests pin what is rendered; this pins what reaches disk,
/// and one property only the executor can show -- **the table is written
/// once**. A migration is irreproducible, so a second `sync` that re-emitted
/// `create <name>_outbox` would leave a project that was working yesterday
/// failing its next `flyway migrate`, and nothing between here and there says
/// so.
#[test]
fn canonical_outbox_delivery_reaches_disk_and_its_table_is_written_once() {
    let root = temp_dir("canonical-outbox-sync");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/model.jdl"),
        "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  \
         platform spring\n  build maven\n  storage postgres\n}\n\ncap json\n\n\
         entity Task @id(ent_task) {\n  use repo\n  id: uuid @pk\n  title: string @notBlank\n\n  \
         command Create(title) {\n    emit TaskCreated\n    deliver outbox\n  }\n\n  \
         event TaskCreated(id: uuid, title)\n}\n",
    )
    .unwrap();

    let synced = jails_cmd(&root, None).arg("sync").output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );

    let generated = root.join(".jails/generated/main/java/com/example/demo");
    let read = |relative: &str| {
        fs::read_to_string(generated.join(relative))
            .unwrap_or_else(|error| panic!("{relative}: {error}"))
    };
    // The staging happens in the transaction that made the row, which is the
    // entire guarantee; a store, a port and a relay to carry it out of there.
    let command = read("adapters/jdbc/JdbcCreateCommand.java");
    assert!(command.contains("@Transactional"), "{command}");
    assert!(
        command.contains("outbox.stage(new TaskCreatedEvent("),
        "{command}"
    );
    read("jobs/JdbcCreateOutbox.java");
    read("jobs/CreateOutboxSink.java");
    read("jobs/CreateLoggingOutboxSink.java");
    read("jobs/CreateOutboxWorker.java");
    read("jobs/SchedulingConfig.java");
    // The minted identity, and the class that mints it.
    read("domain/TimeOrderedUuid.java");
    let event = read("domain/events/TaskCreatedEvent.java");
    assert!(event.contains("UUID id"), "{event}");

    let migrations = || {
        let mut names = fs::read_dir(root.join("src/main/resources/db/migration"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        names.sort();
        names
    };
    let after_first = migrations();
    assert_eq!(
        after_first
            .iter()
            .filter(|name| name.contains("create_outbox"))
            .count(),
        1,
        "{after_first:?}"
    );

    let resynced = jails_cmd(&root, None).arg("sync").output().unwrap();
    assert!(
        resynced.status.success(),
        "{}",
        String::from_utf8_lossy(&resynced.stderr)
    );
    assert_eq!(migrations(), after_first, "sync re-emitted a migration");
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

/// The digest a reader reviews is the digest that gets applied: preview, plan
/// export, confirmation and apply reference the same digest, which is the
/// whole reason apply may execute a bundle rather than recompute one.
///
/// Four surfaces, one number. `--pretend` is what a human reads, `--plan-out`
/// is what leaves the machine, and the execution report is what happened; a
/// disagreement between any two would mean the review was of something else.
#[test]
fn preview_export_and_apply_all_name_one_plan_digest() {
    let root = temp_dir("one-plan-digest");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/model.jdl"),
        "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  \
         platform spring\n  build maven\n  storage postgres\n}\n",
    )
    .unwrap();
    let synced = jails_cmd(&root, None).arg("sync").output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );

    let mutation = ["g", "record", "Note", "id:uuid@pk", "title:string"];

    // 1. Preview, as a human reads it.
    let previewed = jails_cmd(&root, None)
        .args(mutation)
        .arg("--pretend")
        .output()
        .unwrap();
    assert!(
        previewed.status.success(),
        "{}",
        String::from_utf8_lossy(&previewed.stderr)
    );
    let preview = String::from_utf8_lossy(&previewed.stdout);
    let previewed_digest = preview
        .split_whitespace()
        .nth(1)
        .expect("`plan <digest>: …`")
        .trim_end_matches(':')
        .to_string();
    // `sha256:` plus 64 hex.
    assert_eq!(previewed_digest.len(), 71, "{preview}");

    // 2. Export, as it leaves the machine. `--plan-out` reports too, so this
    //    also covers the human line staying in step with the file.
    let bundle_path = root.join("plan.json");
    let exported = jails_cmd(&root, None)
        .args(mutation)
        .arg("--plan-out")
        .arg(&bundle_path)
        .output()
        .unwrap();
    assert!(
        exported.status.success(),
        "{}",
        String::from_utf8_lossy(&exported.stderr)
    );
    let bundle: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&bundle_path).unwrap()).unwrap();
    assert_eq!(bundle["plan"]["digest"].as_str().unwrap(), previewed_digest);

    // 3. Apply. Nothing has changed in between, so replanning -- if that is
    //    what it were doing -- would have to land on the same number, and the
    //    contract is that it does not replan at all.
    let applied = jails_cmd(&root, None)
        .args(mutation)
        .args(["--output", "json"])
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let execution: serde_json::Value =
        serde_json::from_slice(&applied.stdout).expect("an execution report");
    assert_eq!(
        execution["plan_digest"].as_str().unwrap(),
        previewed_digest,
        "the applied plan is not the reviewed one"
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

/// `jails resource status` answers about an entity the model describes, from
/// every authority it names. This drives all four -- declaration, generated,
/// migration history, and the SQL table -- and the two states that are not
/// `consistent`.
#[test]
fn resource_status_answers_from_the_model_when_the_project_is_canonical() {
    let root = jdl_project(
        "jdl-v1-resource-status",
        r#"jdl 1
app Shop {
 pkg com.example.shop
 java 26
 platform spring
 build maven
 storage postgres
}
entity Order {
 id: uuid @pk
 total: long
 use repo
}
entity Memo {
 title: string
}
"#,
    );
    write_spring_fixture(&root);

    // Declared and not yet accepted: an ordinary state, not a fault.
    let pending = jails_cmd(&root, None)
        .args(["resource", "status", "Order"])
        .output()
        .unwrap();
    let pending = String::from_utf8_lossy(&pending.stdout).to_string();
    assert!(
        pending.contains("state: pending") && pending.contains("next: jails sync"),
        "an entity the lock has not accepted should say so and name the fix:\n{pending}"
    );

    let synced = jails_cmd(&root, None).arg("sync").output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );

    let stored = jails_cmd(&root, None)
        .args(["resource", "status", "Order"])
        .output()
        .unwrap();
    let stored = String::from_utf8_lossy(&stored.stdout).to_string();
    for expected in [
        "resource: Order",
        "state: consistent",
        "declaration: present",
        "generated: present",
        "migration-history: present",
        "table: orders",
    ] {
        assert!(
            stored.contains(expected),
            "expected `{expected}` in:\n{stored}"
        );
    }
    assert!(
        stored.contains("Order.java"),
        "the entity's own generated file is attributed to it:\n{stored}"
    );

    // A source-only entity has no table, so the migration authority was never
    // consulted -- `unknown`, which widens, rather than `absent`, which would
    // claim a missing migration nobody asked for.
    let source_only = jails_cmd(&root, None)
        .args(["resource", "status", "Memo"])
        .output()
        .unwrap();
    let source_only = String::from_utf8_lossy(&source_only.stdout).to_string();
    assert!(
        source_only.contains("migration-history: unknown") && !source_only.contains("table:"),
        "a source-only entity reports no table and an unconsulted migration authority:\n{source_only}"
    );

    // Editing a managed file is drift against the accepted image, and is
    // reported as such rather than being re-rendered away.
    let generated = root.join(".jails/generated/main/java/com/example/shop/domain/Order.java");
    let edited = format!(
        "{}\n// touched by the reader\n",
        fs::read_to_string(&generated).unwrap()
    );
    fs::write(&generated, edited).unwrap();
    let drifted = jails_cmd(&root, None)
        .args(["resource", "status", "Order"])
        .output()
        .unwrap();
    let drifted = String::from_utf8_lossy(&drifted.stdout).to_string();
    assert!(
        drifted.contains("state: drifted") && drifted.contains("finding: generated-drift"),
        "an edited managed file is drift against the accepted image:\n{drifted}"
    );

    // `--json` is the same report, and names its schema.
    let json = jails_cmd(&root, None)
        .args(["--output", "json", "resource", "status", "Order"])
        .output()
        .unwrap();
    let json: serde_json::Value =
        serde_json::from_slice(&json.stdout).expect("resource status --json is JSON");
    assert_eq!(json["schema"], "jails.resource-status.v1");
    assert_eq!(json["table"], "orders");
    assert_eq!(json["state"], "drifted");

    let unknown = jails_cmd(&root, None)
        .args(["resource", "status", "Nobody"])
        .output()
        .unwrap();
    let unknown = String::from_utf8_lossy(&unknown.stdout).to_string();
    assert!(
        unknown.contains("declaration: absent") && unknown.contains("resource-not-declared"),
        "a selector naming nothing says so rather than reporting an empty resource:\n{unknown}"
    );
}

/// `jails doctor` reports a generated tree that has been edited and one that
/// is partly deleted.
///
/// Both rows are worded from what the binary does: `sync` refuses while an
/// accepted file is missing, and merges an edited one forward without writing
/// anything.
#[test]
fn doctor_reports_the_generated_tree_of_a_canonical_project() {
    let root = jdl_project(
        "jdl-v1-doctor-managed",
        r#"jdl 1
app Clinic {
 pkg com.example.clinic
 java 26
 platform spring
 build maven
 storage postgres
}
entity Visit {
 id: uuid @pk
 reason: string
 use repo
}
"#,
    );
    write_spring_fixture(&root);
    let synced = jails_cmd(&root, None).arg("sync").output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );

    let clean = jails_cmd(&root, None).arg("doctor").output().unwrap();
    let clean = String::from_utf8_lossy(&clean.stdout).to_string();
    assert!(
        clean.contains("every file the lock accepted is on disk")
            && clean.contains("no generated file has been changed"),
        "a freshly synced project reports both managed rows clean:\n{clean}"
    );

    // The `jails.toml` capability row says where a modelled project's
    // capabilities live, and the accepted-model row is what reconciles them.
    let capability = jails_cmd(&root, None)
        .args(["add", "json"])
        .output()
        .unwrap();
    assert!(
        capability.status.success(),
        "{}",
        String::from_utf8_lossy(&capability.stderr)
    );
    let recorded = jails_cmd(&root, None).arg("doctor").output().unwrap();
    let recorded = String::from_utf8_lossy(&recorded.stdout).to_string();
    assert!(
        recorded.contains("declared in the model"),
        "the capability row must not claim a model-declared capability is absent:\n{recorded}"
    );
    assert!(
        !recorded.contains("jails.toml records none"),
        "and must not report the legacy manifest as the authority:\n{recorded}"
    );
    assert!(
        recorded.contains("the lock has accepted everything the model declares"),
        "a synced capability is accepted, entities and capabilities alike:\n{recorded}"
    );

    // Every column the record carries exists in the migrations. `doctor`
    // answers "are these the bytes jails wrote"; this is the half that answers
    // "is this project coherent".
    assert!(
        clean.contains("`visits` has every column the record carries"),
        "the stored entity's lineage should be checked:\n{clean}"
    );

    let record = root.join(".jails/generated/main/java/com/example/clinic/domain/Visit.java");
    fs::write(
        &record,
        format!(
            "{}\n// a reader's note\n",
            fs::read_to_string(&record).unwrap()
        ),
    )
    .unwrap();
    let edited = jails_cmd(&root, None).arg("doctor").output().unwrap();
    let edited = String::from_utf8_lossy(&edited.stdout).to_string();
    assert!(
        edited.contains("changed since generation")
            && edited.contains("merges the edit forward on every sync"),
        "an edited managed file is a warning that says what happens next:\n{edited}"
    );
    // A record is managed ABI: `jails model eject` refuses for it, so the row
    // must not offer ejection as the fix.
    assert!(
        !edited.contains("jails model eject art_ent_visit_record"),
        "an artifact that cannot be ejected must not be offered as one:\n{edited}"
    );

    let test = root.join(".jails/generated/test/java/com/example/clinic/domain/VisitTest.java");
    fs::remove_file(&test).unwrap();
    let deleted = jails_cmd(&root, None).arg("doctor").output().unwrap();
    let deleted_out = String::from_utf8_lossy(&deleted.stdout).to_string();
    assert!(
        deleted_out.contains("deleted") && deleted_out.contains("refuses while it is gone"),
        "a deleted managed file is a failure, because sync cannot converge past it:\n{deleted_out}"
    );
    assert!(
        !deleted.status.success(),
        "a FAIL row must leave doctor with a non-zero status"
    );

    // And the refusal the row describes is the one `sync` actually gives.
    let refused = jails_cmd(&root, None).arg("sync").output().unwrap();
    assert!(
        !refused.status.success(),
        "sync should refuse while an accepted file is missing:\n{}",
        String::from_utf8_lossy(&refused.stdout)
    );
}

/// `jails routes` and `jails beans` read the managed tree as well as
/// `src/main/java`: a route jails emitted and cannot see is worse than a gap,
/// because the reader cannot tell an unlisted route from an absent one.
#[test]
fn routes_and_beans_see_the_tree_a_canonical_project_generates_into() {
    let root = jdl_project(
        "jdl-v1-inspect-generated",
        r#"jdl 1
app Depot {
 pkg com.example.depot
 java 26
 platform spring
 build maven
 storage postgres
}
"#,
    );
    write_spring_fixture(&root);
    let scaffolded = jails_cmd(&root, None)
        .args(["g", "scaffold", "Crate", "id:uuid@pk", "label:string"])
        .output()
        .unwrap();
    assert!(
        scaffolded.status.success(),
        "{}",
        String::from_utf8_lossy(&scaffolded.stderr)
    );

    // The scaffold emits the port; a *route* is a routed semantic operation's,
    // and the Spring adapter carrying the mapping annotation is the `api`
    // capability's. Both are needed before there is anything to list.
    let query = jails_cmd(&root, None)
        .args([
            "g",
            "usecase",
            "AddCrate",
            "label",
            "--on",
            "Crate",
            "--path",
            "/depot/crates",
        ])
        .output()
        .unwrap();
    assert!(
        query.status.success(),
        "{}",
        String::from_utf8_lossy(&query.stderr)
    );
    let api = jails_cmd(&root, None)
        .args(["add", "api"])
        .output()
        .unwrap();
    assert!(
        api.status.success(),
        "{}",
        String::from_utf8_lossy(&api.stderr)
    );

    let routes = jails_cmd(&root, None).arg("routes").output().unwrap();
    let routes = String::from_utf8_lossy(&routes.stdout).to_string();
    assert!(
        routes.contains("/depot/crates"),
        "the operation's own route should be listed:\n{routes}"
    );
    assert!(
        routes.contains("AddCrateController"),
        "named by the controller the compiler wrote into `.jails/generated`:\n{routes}"
    );

    let beans = jails_cmd(&root, None).arg("beans").output().unwrap();
    let beans = String::from_utf8_lossy(&beans.stdout).to_string();
    assert!(
        beans.contains("CrateController") || beans.contains("CrateService"),
        "the scaffold's beans should be listed:\n{beans}"
    );

    // `jails src` walks all four roots. It is the one command that
    // deliberately works outside a build file, so it is the last place a
    // reader would expect to be told their own generated type does not exist.
    let located = jails_cmd(&root, None)
        .args(["src", "Crate"])
        .output()
        .unwrap();
    let located = String::from_utf8_lossy(&located.stdout).to_string();
    assert!(
        located.contains(".jails/generated/main/java") && located.contains("Crate.java"),
        "`jails src` should find a type the compiler wrote:\n{located}"
    );

    // `stats` counts the same tree, summed per layer across both roots.
    let stats = jails_cmd(&root, None).arg("stats").output().unwrap();
    let stats = String::from_utf8_lossy(&stats.stdout).to_string();
    assert!(
        !stats.contains("No Java sources under"),
        "a project whose every Java file is generated is not an empty project:\n{stats}"
    );

    // An empty report has to say where it looked, or the reader cannot tell a
    // searched-and-empty tree from an unopened one.
    let bare = jdl_project(
        "jdl-v1-inspect-bare",
        r#"jdl 1
app Bare {
 pkg com.example.bare
 java 26
 platform spring
 build maven
 storage none
}
"#,
    );
    write_spring_fixture(&bare);
    let empty = jails_cmd(&bare, None).arg("routes").output().unwrap();
    let empty = String::from_utf8_lossy(&empty.stdout).to_string();
    assert!(
        empty.contains("No routes found under src/main/java."),
        "a project with no generated tree reports exactly the roots it walked:\n{empty}"
    );
}

/// An incoherence `doctor` reports: the Java carries a component the schema
/// history does not.
///
/// Every file is byte-identical to what jails wrote, so nothing else has a
/// reason to complain, and only a query at runtime would find it.
#[test]
fn doctor_names_a_column_the_record_carries_and_the_migrations_do_not() {
    let root = jdl_project(
        "jdl-v1-doctor-lineage",
        r#"jdl 1
app Ward {
 pkg com.example.ward
 java 26
 platform spring
 build maven
 storage postgres
}
entity Bed {
 id: uuid @pk
 label: string
 use repo
}
"#,
    );
    write_spring_fixture(&root);
    let synced = jails_cmd(&root, None).arg("sync").output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );

    let migrations = root.join("src/main/resources/db/migration");
    let migration = fs::read_dir(&migrations)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|extension| extension == "sql"))
        .expect("the stored entity's migration");
    let sql = fs::read_to_string(&migration).unwrap();
    assert!(
        sql.contains("label"),
        "the migration declares the column:\n{sql}"
    );

    // Drop the column from the history while the record keeps the component.
    let torn = sql
        .lines()
        .filter(|line| !line.contains("label"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&migration, format!("{torn}\n")).unwrap();

    let torn = jails_cmd(&root, None).arg("doctor").output().unwrap();
    let reported = String::from_utf8_lossy(&torn.stdout).to_string();
    assert!(
        reported.contains("is missing label") && reported.contains("which `Bed` carries"),
        "the missing column should be named, with the record that needs it:\n{reported}"
    );
    // A failure, not a note. Asserted on the row rather than on the exit
    // status, which this fixture also fails for an unrelated reason.
    assert!(
        reported
            .lines()
            .any(|line| line.contains("is missing label") && line.starts_with("FAIL")),
        "the incoherence is a failure, not a note:\n{reported}"
    );

    // A lineage this reader cannot fold is not an accusation. Unknown widens.
    fs::write(
        &migration,
        "create table bed_things using something_else;\n",
    )
    .unwrap();
    let unreadable = jails_cmd(&root, None).arg("doctor").output().unwrap();
    let unreadable = String::from_utf8_lossy(&unreadable.stdout).to_string();
    assert!(
        !unreadable.contains("schema Bed"),
        "a migration outside the statements jails emits produces no check at all, \
         neither a pass nor an accusation:\n{unreadable}"
    );
}

/// `jails adopt` and `jails model init` are each other's obvious next step in
/// a foreign repository, so `adopt` must leave nothing `model init` refuses
/// on: a layout row is configuration, not a transition.
#[test]
fn a_project_that_only_recorded_its_layout_can_still_become_canonical() {
    let root = temp_dir("adopt-then-model-init");
    write_plain_fixture(&root);
    // A directory name jails' synonym table maps onto a layer, so `adopt` has
    // something to record.
    let renamed = common::generated(&root, "src/main/java/com/example/demo/persistence");
    fs::create_dir_all(&renamed).unwrap();
    fs::write(
        renamed.join("Repo.java"),
        "package com.example.demo.persistence;\npublic class Repo {}\n",
    )
    .unwrap();

    let adopted = jails_cmd(&root, None).arg("adopt").output().unwrap();
    assert!(
        adopted.status.success(),
        "{}",
        String::from_utf8_lossy(&adopted.stderr)
    );
    // `adopt` writes the layout row and nothing else -- it is configuration,
    // not a transition, so it leaves no machine state of jails' own for
    // `model init` to have an opinion about.
    assert!(!root.join(".jails/ledger.toml").exists(), "{adopted:?}");
    let layout = fs::read_to_string(root.join("jails.toml")).unwrap();
    assert!(
        layout.contains("adapters = \"persistence\""),
        "the layout row is what adopt is for:\n{layout}"
    );

    let initialised = jails_cmd(&root, None)
        .args(["model", "init"])
        .output()
        .unwrap();
    assert!(
        initialised.status.success(),
        "an adopted layout must not block the on-ramp:\n{}",
        String::from_utf8_lossy(&initialised.stderr)
    );

    // And the layout the reader adopted is the one the compiler projects with.
    let explained = jails_cmd(&root, None)
        .args(["model", "explain", "java-package"])
        .output()
        .unwrap();
    let explained = String::from_utf8_lossy(&explained.stdout).to_string();
    assert!(
        explained.contains("com.example.demo.persistence"),
        "the adopted layer name should reach the compiler's projection:\n{explained}"
    );
}

/// `.jails/app.toml` is an import format, not a second editable source.
///
/// A `[[generate]]` row is a `GenerateArgs` -- the same value `jails g`
/// parses -- so every row goes through the frontend that already knows how to
/// declare it, and the manifest's own syntax is the only thing its parser
/// knows that the CLI does not.
///
/// Row by row rather than one transition, which costs atomicity and buys
/// convergence: each frontend is idempotent, so an interrupted replay
/// converges by being run again.
#[test]
fn a_manifest_replays_into_the_model_and_converges() {
    let root = jdl_project(
        "jdl-v1-app-replay",
        r#"jdl 1
app Books {
 pkg com.example.books
 java 26
 platform plain
 build maven
 storage none
}
"#,
    );
    write_plain_fixture(&root);
    fs::write(
        root.join(".jails/app.toml"),
        r#"schema = 1
capabilities = ["json"]

[[generate]]
kind = "record"
name = "Loan"
fields = ["id:uuid", "days:int"]

[[generate]]
kind = "enum"
name = "Shelf"
fields = ["OPEN", "CLOSED"]
"#,
    )
    .unwrap();

    let applied = jails_cmd(&root, None)
        .args(["app", "apply"])
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    for declaration in ["cap json", "entity Loan", "enum Shelf"] {
        assert!(
            model.contains(declaration),
            "the manifest's rows should reach the model, missing `{declaration}`:\n{model}"
        );
    }
    assert!(
        root.join(".jails/generated/main/java/com/example/books/domain/Loan.java")
            .is_file(),
        "and be compiled into the managed tree"
    );
    assert!(
        !root.join(".jails/ledger.toml").exists(),
        "a canonical replay must not create a legacy ledger"
    );

    // Convergent: every row is already declared, so nothing is written.
    let again = jails_cmd(&root, None)
        .args(["app", "apply"])
        .output()
        .unwrap();
    let again = String::from_utf8_lossy(&again.stdout).to_string();
    assert!(
        again
            .matches("nothing to do, the project already matches the model")
            .count()
            >= 3,
        "a second replay declares nothing new:\n{again}"
    );

    // `plan` is the same replay, pretending.
    let planned = jails_cmd(&root, None)
        .args(["app", "plan"])
        .output()
        .unwrap();
    assert!(
        planned.status.success(),
        "{}",
        String::from_utf8_lossy(&planned.stderr)
    );

    // `app init` refuses: it writes the manifest, and a manifest beside a
    // model is a second editable source.
    let refused = jails_cmd(&root, None)
        .args(["app", "init"])
        .output()
        .unwrap();
    assert!(
        !refused.status.success()
            && String::from_utf8_lossy(&refused.stderr).contains("does not route"),
        "`app init` writes a second editable source and must still refuse"
    );
}

/// `resource index add` writes a declaration its own parser accepts: v1 reads
/// `index [ user_id, created_at desc ]` and allows only `@id` and `@map`, and
/// the renderer picks the form by dialect rather than by filename.
#[test]
fn an_index_on_a_v1_model_uses_the_grammar_that_model_is_written_in() {
    let root = jdl_project(
        "jdl-v1-index-grammar",
        r#"jdl 1
app Depot {
 pkg com.example.depot
 java 26
 platform spring
 build maven
 storage postgres
}
entity Crate {
 id: uuid @pk
 label: string
 slot: int
 use repo
}
"#,
    );
    write_spring_fixture(&root);
    let synced = jails_cmd(&root, None).arg("sync").output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );

    let added = jails_cmd(&root, None)
        .args(["resource", "index", "add", "Crate", "label, slot desc"])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "an index on a v1 model must use v1's own grammar:\n{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(
        model.contains("index [label, slot desc]"),
        "the bracketed field list is the v1 form:\n{model}"
    );
    assert!(
        !model.contains("@as("),
        "and a v1 index takes its label from its columns, so `@as` is rejected:\n{model}"
    );

    // One forward migration, which is the half that makes an index a stable
    // entity child rather than a rendering.
    let migrations = fs::read_dir(root.join("src/main/resources/db/migration"))
        .unwrap()
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|e| e == "sql"))
        .count();
    assert_eq!(migrations, 2, "create table, then add index");

    // And `g scaffold --index` reaches the same node rather than refusing:
    // the entity lands first, then the index, because the columns resolve
    // against model field identity.
    let scaffolded = jails_cmd(&root, None)
        .args([
            "g",
            "scaffold",
            "Bin",
            "id:uuid@pk",
            "code:string",
            "--index",
            "code",
        ])
        .output()
        .unwrap();
    assert!(
        scaffolded.status.success(),
        "`g scaffold --index` should apply the index rather than refuse:\n{}",
        String::from_utf8_lossy(&scaffolded.stderr)
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(
        model.contains("index [code]"),
        "the scaffold's index should be in the model:\n{model}"
    );
}

/// Five operation flags the frontend translates into the model: `set x = 1`,
/// `select [a]`, `if-match optional`, `bind p from form "wire"` and `consumes
/// form`, which `TransitionSemantics` carries as `select`, `assignments` and
/// `precondition`.
///
/// The selector is subtracted from the update, which is the one part that
/// is not a straight pass-through: a transition does not write the column it
/// selects by, and naming a primary key in both is what the compiler refuses
/// as rewriting a key.
#[test]
fn a_canonical_transition_carries_its_selector_pins_and_precondition() {
    let root = jdl_project(
        "jdl-v1-transition-flags",
        r#"jdl 1
app Shop {
 pkg com.example.shop
 java 26
 platform spring
 build maven
 storage postgres
}
entity Note {
 id: uuid @pk
 seen: boolean
 version: long @version
 body: string
 use repo
}
"#,
    );
    // The pom the canonical JDBC adapters need: `storage postgres` renders
    // Spring JDBC, and refuses without a captured Spring Boot project.
    write_spring_fixture(&root);
    let synced = jails_cmd(&root, None).arg("sync").output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );
    let applied = jails_cmd(&root, None)
        .args([
            "g",
            "transition",
            "MarkSeen",
            "id:uuid",
            "seen:boolean",
            "version:long",
            "--on",
            "Note",
            "--select",
            "id",
            "--set",
            "seen=true",
            "--if-match",
            "optional",
            "--consumes",
            "form",
            "--bind",
            "id=note_id",
        ])
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "the five flags should reach the model rather than be refused:\n{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    for expected in [
        "select [id]",
        "set seen = true",
        "if-match optional",
        "bind id from form \"note_id\"",
        "consumes form",
    ] {
        assert!(
            model.contains(expected),
            "`{expected}` is missing from the model:\n{model}"
        );
    }
    // Subtracted, not merely listed: `id` selects the row and `seen` is
    // pinned, so neither is a caller-supplied update. The linker refuses the
    // overlap by name, which is what makes the three roles distinct.
    assert!(
        !model.contains("update ["),
        "with the selector pinned and the rest managed there is nothing to update:\n{model}"
    );
}

/// Four operation flags the frontend translates into nodes the model, the JDL
/// grammar and the compiler support -- `--via`, `--order-by`, an optional
/// filter and `--on-conflict` -- asked for together, as a proof-application
/// manifest does.
#[test]
fn a_canonical_query_carries_its_join_ordering_and_optional_filters() {
    let root = jdl_project(
        "jdl-v1-operation-flags",
        r#"jdl 1
app Post {
 pkg com.example.post
 java 26
 platform spring
 build maven
 storage postgres
}
enum Channel {
 EMAIL
 SMS
}
entity Sender {
 id: uuid @pk
 email: string
 use repo
}
entity Note {
 id: uuid @pk
 senderId: uuid
 body: string
 channel: Channel
 use repo
}
"#,
    );
    write_spring_fixture(&root);
    let synced = jails_cmd(&root, None).arg("sync").output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );

    // `--via` is a join, and its filter may name a column of the *joined*
    // entity: that is what the flag is for.
    let via = jails_cmd(&root, None)
        .args([
            "g", "query", "ByEmail", "email", "--on", "Note", "--via", "Sender",
        ])
        .output()
        .unwrap();
    assert!(
        via.status.success(),
        "`--via` should reach the model's own join:\n{}",
        String::from_utf8_lossy(&via.stderr)
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(
        model.contains("join Sender as sender on sender_id -> sender.id"),
        "the join is derived from the two entities:\n{model}"
    );
    assert!(
        model.contains("sender.email"),
        "and a joined filter is qualified by the alias:\n{model}"
    );

    // `--order-by` keeps its direction, and `?` marks an optional filter
    // rather than a nullable column.
    let listing = jails_cmd(&root, None)
        .args([
            "g",
            "query",
            "Recent",
            "channel:Channel?",
            "--on",
            "Note",
            "--order-by",
            "id desc",
            "--limit",
            "20",
        ])
        .output()
        .unwrap();
    assert!(
        listing.status.success(),
        "`--order-by ... desc` and an optional filter should both translate:\n{}",
        String::from_utf8_lossy(&listing.stderr)
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(
        model.contains("order by [id desc]"),
        "the direction rides beside the field:\n{model}"
    );
    assert!(
        model.contains("channel?"),
        "and `?` is the optional-filter marker the grammar already parsed:\n{model}"
    );

    // `--on-conflict` is the retained-result insert.
    let upsert = jails_cmd(&root, None)
        .args([
            "g",
            "usecase",
            "EnsureSender",
            "email",
            "--on",
            "Sender",
            "--on-conflict",
            "email",
        ])
        .output()
        .unwrap();
    assert!(
        upsert.status.success(),
        "`--on-conflict` should reach `conflict_key`:\n{}",
        String::from_utf8_lossy(&upsert.stderr)
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(
        model.contains("conflict on [email]"),
        "the conflict key is a declaration, not a flag jails drops:\n{model}"
    );

    // Declaring the same association twice is a no-op, not a duplicate
    // relation the parser then refuses.
    for _ in 0..2 {
        let association = jails_cmd(&root, None)
            .args([
                "g",
                "association",
                "NoteSender",
                "--on",
                "Note",
                "--yields",
                "Sender",
                "senderId=id",
            ])
            .output()
            .unwrap();
        assert!(
            association.status.success(),
            "re-declaring an association must be idempotent:\n{}",
            String::from_utf8_lossy(&association.stderr)
        );
    }
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert_eq!(
        model.matches("relation noteSender").count(),
        1,
        "and must not append a second relation under the same name:\n{model}"
    );
}

/// A marked block jails owns stays where it is.
///
/// Stripping the source-roots block and re-inserting it before `</plugins>`
/// is position-stable only while it is the last thing in there; once the
/// integration-test plugin lands beside it, every plan would move one block
/// past the other, `jails model check --frozen` would report a pending
/// operation on a project just synchronised, and the pom would churn by a
/// whole block on every run.
///
/// Two blocks is the smallest case that can show it, which is why this needs
/// an operation: the failsafe plugin arrives with the first emitted `*IT`.
#[test]
fn two_marked_build_blocks_keep_their_places_across_replans() {
    let root = jdl_project(
        "jdl-v1-marked-block-order",
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
        vec!["g", "scaffold", "Note", "id:uuid@pk", "title:string!"],
        vec!["add", "db", "--no-start"],
        vec![
            "g",
            "query",
            "NotesByTitle",
            "title:string!",
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

    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    let roots = pom.find("jails:generated-source-roots").unwrap();
    let tests = pom.find("jails:integration-tests").unwrap();
    assert!(roots < tests, "{pom}");

    // Frozen on the *first* ask, not after a repairing sync: a plan that has
    // to be applied before the tree matches the model is a plan that never
    // settles.
    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(
        frozen.status.success(),
        "{}{}",
        String::from_utf8_lossy(&frozen.stdout),
        String::from_utf8_lossy(&frozen.stderr)
    );

    // And a sync moves nothing, which is the same property from the other
    // side.
    let synced = jails_cmd(&root, None).args(["sync"]).output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );
    assert_eq!(fs::read_to_string(root.join("pom.xml")).unwrap(), pom);
}

/// A scaffold serves its resource: the `http` facet emits a controller behind
/// `<Name>HttpPort`, not a one-method interface with no implementation, no
/// route and no caller. An unimplemented interface compiles, so this is held
/// at the file level.
///
/// It speaks the domain record rather than a request/response pair, which is
/// the shape the operation controllers already use -- one wire convention per
/// project rather than two, and `scaffold` stays the four-facet profile it is
/// documented to be.
#[test]
fn a_deleted_managed_file_is_repaired_from_the_model() {
    let root = jdl_project(
        "jdl-v1-repair-deleted",
        r#"jdl 1
app Demo {
  pkg com.example.demo
  java 26
  platform plain
  build maven
  storage none
}

entity Widget {
 id: long @pk
 title: string
}
"#,
    );
    let sync = jails_cmd(&root, None).arg("sync").output().unwrap();
    assert!(
        sync.status.success(),
        "{}",
        String::from_utf8_lossy(&sync.stderr)
    );
    let widget = root.join(".jails/generated/main/java/com/example/demo/domain/Widget.java");
    let rendered = fs::read_to_string(&widget).unwrap();

    // A reader deletes a managed file -- a half-finished `git checkout`, or a
    // deletion meant as "stop generating this". Every ordinary plan refuses,
    // and that is the guard working.
    fs::remove_file(&widget).unwrap();
    let refused = jails_cmd(&root, None).arg("sync").output().unwrap();
    assert!(!refused.status.success());
    let message = String::from_utf8_lossy(&refused.stderr);
    assert!(message.contains("was deleted by you"), "{message}");
    // The fix line has to name a command that writes it back, not `jails
    // sync`, which is the command that just refused.
    assert!(message.contains("jails resource repair"), "{message}");

    let repaired = jails_cmd(&root, None)
        .args(["resource", "repair"])
        .output()
        .unwrap();
    assert!(
        repaired.status.success(),
        "{}",
        String::from_utf8_lossy(&repaired.stderr)
    );
    assert_eq!(fs::read_to_string(&widget).unwrap(), rendered);

    // Repaired means converged, not merely present: the next ordinary plan is
    // empty and the project is frozen against its own model again.
    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(
        frozen.status.success(),
        "{}",
        String::from_utf8_lossy(&frozen.stderr)
    );

    // Repair waives one guard and no others. A hand edit is still merged, not
    // overwritten -- a repair that reverted the reader's work would be a worse
    // answer than the refusal it replaces.
    let edited = format!(
        "{}\n    // reader's own note\n",
        rendered.trim_end().trim_end_matches('}').trim_end()
    );
    fs::write(&widget, format!("{edited}}}\n")).unwrap();
    let again = jails_cmd(&root, None)
        .args(["resource", "repair"])
        .output()
        .unwrap();
    assert!(
        again.status.success(),
        "{}",
        String::from_utf8_lossy(&again.stderr)
    );
    assert!(
        fs::read_to_string(&widget)
            .unwrap()
            .contains("reader's own note"),
        "repair overwrote a hand edit"
    );

    // Compilation is whole-model, so a selector is refused rather than
    // silently ignored.
    let scoped = jails_cmd(&root, None)
        .args(["resource", "repair", "Widget", "--strategy", "roll-forward"])
        .output()
        .unwrap();
    assert!(!scoped.status.success());
    let scoped = String::from_utf8_lossy(&scoped.stderr);
    assert!(scoped.contains("takes no selector"), "{scoped}");
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

    let jdbc = root.join(".jails/generated/main/java/com/example/demo/adapters/jdbc");
    // An enum reaches a `text` column as its constant name. Bound raw, pgjdbc
    // refuses it at run time.
    let command = fs::read_to_string(jdbc.join("JdbcPublishNoteCommand.java")).unwrap();
    assert!(command.contains("input.shelf().name()"), "{command}");

    let tests = root.join(".jails/generated/test/java/com/example/demo/adapters/jdbc");
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

/// JDL v1 §21's first conformance family: the §4 complete example is a
/// fixture, not prose.
///
/// §21 says the complete example "is an executable conformance fixture" and
/// that documentation examples MUST be extracted in CI rather than copied into
/// disconnected test strings; otherwise the flagship example of the language
/// could stop linking and the only thing that would notice is a reader typing
/// it in.
///
/// The example is read out of `docs/01-jdl-v1.md` rather than pasted here, which is
/// the whole point -- a copy is a second document that drifts silently, and a
/// test asserting a copy links proves nothing about what a reader sees.
///
/// One line is a recorded gap rather than a passing assertion. §16.4 says
/// the *preferred* ejection reference is a readable boundary path --
/// `Entity.repo.fake` -- resolved by a boundary registry. There is no boundary
/// registry: `known_targets` in the linker is the set of stable IDs, and
/// `jails model eject` takes a "stable entity, operation, or capability id".
/// So `eject Task.repo.fake` refuses, and this test pins both halves: the rest
/// of the example links, and that line still refuses with the recorded
/// diagnostic. When the registry lands, the second assertion fails and this
/// test is how you find out the first one can absorb it.
#[test]
fn the_specification_complete_example_links_except_its_one_recorded_gap() {
    let document =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/01-jdl-v1.md"))
            .expect("docs/01-jdl-v1.md is checked in");
    let section = document
        .split("## 4. Complete example")
        .nth(1)
        .and_then(|rest| rest.split("\n## 5.").next())
        .expect("docs/01-jdl-v1.md still has a §4 complete example");
    let example = section
        .split("```jdl\n")
        .nth(1)
        .and_then(|rest| rest.split("```").next())
        .expect("§4 still carries one jdl block");
    assert!(
        example.starts_with("jdl 1"),
        "the extracted block is not a v1 document:\n{example}"
    );
    assert!(
        example.contains("eject Task.repo.fake"),
        "§4 no longer carries the readable ejection path this test pins:\n{example}"
    );

    // The whole example, as written. This is the assertion that fails when the
    // language and the linker drift apart.
    let whole = jdl_project("spec-section-4-whole", example);
    let refused = jails_cmd(&whole, None)
        .args(["model", "check"])
        .output()
        .unwrap();
    let said = String::from_utf8_lossy(&refused.stderr).into_owned();
    assert!(
        !refused.status.success() && said.contains("model-ejection-target"),
        "§16.4's readable boundary path resolves now -- delete this half and the \
         `known_targets` note in `docs/01-jdl-v1.md` §16.4's entry:\n{said}"
    );
    assert!(
        said.matches("] $.").count() == 1,
        "the example has a second diagnostic beyond the recorded ejection gap:\n{said}"
    );
    fs::remove_dir_all(&whole).ok();

    // Everything else, which must link cleanly.
    let without = example.replace("eject Task.repo.fake\n", "");
    let root = jdl_project("spec-section-4-linked", &without);
    let checked = jails_cmd(&root, None)
        .args(["model", "check"])
        .output()
        .unwrap();
    assert!(
        checked.status.success(),
        "the §4 example does not link:\n{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    fs::remove_dir_all(&root).ok();
}

/// One model language. A `.jails/model.jdl` that does not open with `jdl 1`,
/// and a project that still has only `.jails/model.toml`, are each refused by
/// name with the file the model has to be.
#[test]
fn a_model_that_is_not_jdl_1_is_refused_by_name() {
    let root = jdl_project("model-not-jdl-1", "application Notes\n");
    let refused = jails_cmd(&root, None)
        .args(["g", "record", "Task", "id:uuid"])
        .output()
        .unwrap();
    assert!(!refused.status.success(), "{refused:?}");
    let told = String::from_utf8_lossy(&refused.stderr);
    assert!(
        told.contains("`.jails/model.jdl` does not start with `jdl 1`"),
        "{told}"
    );
    assert!(told.contains("fix: the model must be `jdl 1`"), "{told}");
    fs::remove_dir_all(&root).ok();

    let root = temp_dir("model-only-toml");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/model.toml"),
        "schema = \"jails.model.v1\"\n",
    )
    .unwrap();
    for command in [
        ["g", "record", "Task", "id:uuid"].as_slice(),
        ["model", "check"].as_slice(),
    ] {
        let refused = jails_cmd(&root, None).args(command).output().unwrap();
        assert!(!refused.status.success(), "{command:?}: {refused:?}");
        let told = String::from_utf8_lossy(&refused.stderr);
        assert!(
            told.contains("`.jails/model.toml` is not a model this jails reads"),
            "{command:?}: {told}"
        );
        assert!(told.contains("write the model as `jdl 1`"), "{told}");
    }
    assert!(!root.join(".jails/model.jdl").exists());
    fs::remove_dir_all(&root).ok();
}
