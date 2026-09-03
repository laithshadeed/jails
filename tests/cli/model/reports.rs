//! What the read-only commands say about a modelled project: `doctor`,
//! `resource status`, `routes` and `beans`.
//!
use super::*;

/// `jails entity status` answers about an entity the model describes, from
/// every authority it names. This drives all four -- declaration, generated,
/// migration history, and the SQL table -- and the two states that are not
/// `consistent`.
/// `--output json` carries the human report's value: the same status, the
/// same file list, the same declaration.
///
/// **One projection, two encodings.** The JSON used to be the execution
/// receipt -- four counts and a digest -- so a caller could not learn from it
/// what a reader learns from the screen, and a preview printed the whole
/// bundle, a third shape again. The bundle is what `--plan-out` writes.
/// **One path style, and one plural behind it.**
///
/// A REST path is spelled with hyphens -- `/crawl-runs`, not `/crawl_runs` --
/// and it derives from the same plural as the table, so the route and the
/// table it reads cannot drift. `model explain` carries the row, so a project
/// that pinned a path with `use scaffold(path: …)` reads as pinned rather
/// than as a convention that moved.
#[test]
fn a_multi_word_entity_is_served_at_a_hyphenated_path() {
    let root = jdl_project("http-path-style", NOTES_JDL);
    write_spring_fixture(&root);
    let generated = jails_cmd(&root, None)
        .args(["g", "scaffold", "CrawlRun", "id:uuid@pk", "title:string"])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );

    let routes = jails_cmd(&root, None).arg("routes").output().unwrap();
    let printed = String::from_utf8_lossy(&routes.stdout);
    assert!(printed.contains("/crawl-runs"), "{printed}");
    assert!(
        !printed.contains("/crawl_runs"),
        "an underscore in a URL is a word break nobody outside SQL writes: {printed}"
    );

    // The same plural as the table, and said out loud.
    let explained = jails_cmd(&root, None)
        .args(["model", "explain"])
        .output()
        .unwrap();
    let explained = String::from_utf8_lossy(&explained.stdout);
    assert!(
        explained.contains("http-path") && explained.contains("/crawl-runs"),
        "{explained}"
    );
    assert!(
        explained.contains("crawl_runs"),
        "the table keeps its underscore: {explained}"
    );
}

#[test]
fn the_json_encoding_carries_the_same_report_as_the_screen() {
    let root = model_project("model-one-json", EMPTY_MODEL);

    let machine = jails_cmd(&root, None)
        .args(["--output", "json", "g", "record", "Money", "amount:long"])
        .output()
        .unwrap();
    assert!(
        machine.status.success(),
        "{}",
        String::from_utf8_lossy(&machine.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&machine.stdout).unwrap();
    assert_eq!(report["schema"], "jails.command-result.v2");
    assert_eq!(report["status"], "applied");
    assert!(
        report.get("blobs").is_none() && report.get("trees").is_none(),
        "the report is not the bundle: {report}"
    );
    let files = report["files"].as_array().unwrap();
    assert!(
        files
            .iter()
            .all(|entry| entry["verb"].is_string() && entry["path"].is_string())
    );
    assert_eq!(
        report["model"].as_array().unwrap()[0],
        "entity Money {",
        "the declaration the mutation wrote is in the report: {report}"
    );

    // The same mutation on a second entity, read off the screen: the list the
    // human sees and the list JSON carries are one value.
    let human = jails_cmd(&root, None)
        .args(["g", "record", "Note", "title:string!"])
        .output()
        .unwrap();
    assert!(human.status.success());
    let printed = String::from_utf8_lossy(&human.stdout);
    let listed = printed
        .lines()
        .filter(|line| {
            ["create", "write", "patch", "delete", "append"]
                .iter()
                .any(|verb| line.starts_with(&format!("  {verb}")))
        })
        .count();
    let machine = jails_cmd(&root, None)
        .args(["--output", "json", "g", "record", "Third", "n:int"])
        .output()
        .unwrap();
    assert!(machine.status.success());
    let second: serde_json::Value = serde_json::from_slice(&machine.stdout).unwrap();
    assert_eq!(
        second["files"].as_array().unwrap().len(),
        listed,
        "a record's file list is the same length either way:\n{printed}\n{second}"
    );

    // A preview reports too, rather than printing the reviewed transition.
    let preview = jails_cmd(&root, None)
        .args([
            "--output",
            "json",
            "--pretend",
            "g",
            "record",
            "Fourth",
            "n:int",
        ])
        .output()
        .unwrap();
    assert!(preview.status.success());
    let preview: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert_eq!(preview["status"], "planned");
    assert!(preview.get("blobs").is_none(), "{preview}");
}

/// No report line begins with an identifier and a colon.
///
/// `Note: nothing to do…` reads as a label, and a reader scanning output for
/// `jails:` — the refusal prefix — finds a sentence that is not one. The
/// entity's name stays in the line; it just stops leading it.
#[test]
fn no_report_line_reads_as_a_label() {
    let root = model_project("model-no-label-lines", EMPTY_MODEL);
    let mut printed = String::new();
    for arguments in [
        vec!["g", "record", "Note", "title:string!"],
        vec!["g", "record", "Note", "title:string!"],
        vec!["set", "server.port=9090"],
        vec!["sync"],
    ] {
        let output = jails_cmd(&root, None).args(&arguments).output().unwrap();
        assert!(
            output.status.success(),
            "`jails {}`: {}",
            arguments.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        printed.push_str(&String::from_utf8_lossy(&output.stdout));
    }
    let labels: Vec<&str> = printed
        .lines()
        .filter(|line| {
            let Some((head, _)) = line.split_once(':') else {
                return false;
            };
            !head.is_empty()
                && !line.starts_with(' ')
                && head
                    .chars()
                    .all(|character| character.is_alphanumeric() || character == '_')
        })
        .collect();
    assert!(
        labels.is_empty(),
        "these report lines begin with an identifier and a colon: {labels:?}"
    );
}

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
        .args(["entity", "status", "Order"])
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
        .args(["entity", "status", "Order"])
        .output()
        .unwrap();
    let stored = String::from_utf8_lossy(&stored.stdout).to_string();
    for expected in [
        "entity: Order",
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
        .args(["entity", "status", "Memo"])
        .output()
        .unwrap();
    let source_only = String::from_utf8_lossy(&source_only.stdout).to_string();
    assert!(
        source_only.contains("migration-history: unknown") && !source_only.contains("table:"),
        "a source-only entity reports no table and an unconsulted migration authority:\n{source_only}"
    );

    // Editing a managed file is drift against the accepted image, and is
    // reported as such rather than being re-rendered away.
    let generated = root.join("src/main/java/com/example/shop/domain/Order.java");
    let edited = format!(
        "{}\n// touched by the reader\n",
        fs::read_to_string(&generated).unwrap()
    );
    fs::write(&generated, edited).unwrap();
    let drifted = jails_cmd(&root, None)
        .args(["entity", "status", "Order"])
        .output()
        .unwrap();
    let drifted = String::from_utf8_lossy(&drifted.stdout).to_string();
    assert!(
        drifted.contains("state: drifted") && drifted.contains("finding: generated-drift"),
        "an edited managed file is drift against the accepted image:\n{drifted}"
    );

    // `--json` is the same report, and names its schema.
    let json = jails_cmd(&root, None)
        .args(["--output", "json", "entity", "status", "Order"])
        .output()
        .unwrap();
    let json: serde_json::Value =
        serde_json::from_slice(&json.stdout).expect("resource status --json is JSON");
    assert_eq!(json["schema"], "jails.entity-status.v1");
    assert_eq!(json["table"], "orders");
    assert_eq!(json["state"], "drifted");

    let unknown = jails_cmd(&root, None)
        .args(["entity", "status", "Nobody"])
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

    let record = root.join("src/main/java/com/example/clinic/domain/Visit.java");
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

    let test = root.join("src/test/java/com/example/clinic/domain/VisitTest.java");
    fs::remove_file(&test).unwrap();
    let deleted = jails_cmd(&root, None).arg("doctor").output().unwrap();
    let deleted_out = String::from_utf8_lossy(&deleted.stdout).to_string();
    assert!(
        deleted_out.contains("deleted since the lock accepted it")
            && deleted_out.contains("`jails sync` writes it back"),
        "a deleted managed file is drift `sync` undoes, and the row says so:\n{deleted_out}"
    );

    // And the repair the row names is the one `sync` actually does.
    let healed = jails_cmd(&root, None).arg("sync").output().unwrap();
    assert!(
        healed.status.success(),
        "sync should write the file back:\n{}",
        String::from_utf8_lossy(&healed.stderr)
    );
    assert!(test.is_file(), "the deleted managed file is back");
    assert!(
        String::from_utf8_lossy(&healed.stdout).contains("VisitTest.java"),
        "a file that comes back should not come back silently:\n{}",
        String::from_utf8_lossy(&healed.stdout)
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
        "named by the controller the compiler wrote into `src/main/java`:\n{routes}"
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
        located.contains("src/main/java") && located.contains("Crate.java"),
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

/// An incoherence `doctor` reports: the record carries a component the
/// accepted schema does not.
///
/// Every file is byte-identical to what jails wrote, so nothing else has a
/// reason to complain, and only a query at runtime would find it. Both sides
/// are `jails_compiler::storage_columns` -- asked of the declared model and of
/// the same entity in the model the lock accepted -- rather than a reader of
/// the migration text, which was a second description of the same decision.
#[test]
fn doctor_names_a_column_the_record_carries_and_the_accepted_schema_does_not() {
    let model = |components: &str| {
        format!(
            r#"jdl 1
app Ward {{
 pkg com.example.ward
 java 26
 platform spring
 build maven
 storage postgres
}}
entity Bed {{
 id: uuid @pk
 label: string
{components} use repo
}}
"#
        )
    };
    let root = jdl_project("jdl-v1-doctor-lineage", &model(""));
    write_spring_fixture(&root);
    let synced = jails_cmd(&root, None).arg("sync").output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );

    let accepted = jails_cmd(&root, None).arg("doctor").output().unwrap();
    let accepted = String::from_utf8_lossy(&accepted.stdout).to_string();
    assert!(
        accepted
            .lines()
            .any(|line| line.starts_with("ok") && line.contains("schema Bed")),
        "a synced project is coherent:\n{accepted}"
    );

    // A component the lock has not accepted is a column the table does not
    // have, whatever the Java says.
    fs::write(root.join(".jails/model.jdl"), model(" occupant: string\n")).unwrap();
    let torn = jails_cmd(&root, None).arg("doctor").output().unwrap();
    let reported = String::from_utf8_lossy(&torn.stdout).to_string();
    assert!(
        reported.contains("is missing occupant") && reported.contains("which `Bed` carries"),
        "the missing column should be named, with the record that needs it:\n{reported}"
    );
    // A failure, not a note. Asserted on the row rather than on the exit
    // status, which this fixture also fails for an unrelated reason.
    assert!(
        reported
            .lines()
            .any(|line| line.contains("is missing occupant") && line.starts_with("FAIL")),
        "the incoherence is a failure, not a note:\n{reported}"
    );
}

/// A migration whose bytes were changed by hand is the *seal's* question, and
/// it is reported there rather than by re-reading the SQL for a column list.
#[test]
fn doctor_names_a_migration_edited_after_it_was_published() {
    let root = jdl_project(
        "jdl-v1-doctor-seal",
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

    let edited = jails_cmd(&root, None).arg("doctor").output().unwrap();
    let reported = String::from_utf8_lossy(&edited.stdout).to_string();
    assert!(
        reported.lines().any(|line| line.starts_with("FAIL")
            && line.contains("sealed migrations")
            && line.contains("V001__create_beds.sql")),
        "an edited migration is named by the seal, as a failure:\n{reported}"
    );
}

/// `jails model status` is the replacement for listing a generated root: the
/// lock's list of managed files, each against its accepted image.
#[test]
fn model_status_lists_the_lock_and_tells_edited_from_missing() {
    let root = model_project("model-status", MODEL);
    let unowned = jails_cmd(&root, None)
        .args(["model", "status"])
        .output()
        .unwrap();
    assert!(unowned.status.success());
    assert!(
        String::from_utf8_lossy(&unowned.stdout).contains("nothing is generated yet"),
        "{}",
        String::from_utf8_lossy(&unowned.stdout)
    );

    apply_canonical_model(&root, "initial-plan");
    const RECORD: &str = "src/main/java/com/example/notes/domain/Note.java";
    const SERVICE: &str = "src/main/java/com/example/notes/service/NoteService.java";
    let mut edited = fs::read_to_string(root.join(RECORD)).unwrap();
    edited.push_str("// reader edit\n");
    fs::write(root.join(RECORD), edited).unwrap();
    fs::remove_file(root.join(SERVICE)).unwrap();

    let status = jails_cmd(&root, None)
        .args(["model", "status", "--output", "json"])
        .output()
        .unwrap();
    assert!(status.status.success());
    let report: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    let state = |path: &str| {
        report["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|file| file["path"] == path)
            .unwrap_or_else(|| panic!("{path} is not listed"))["state"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert_eq!(state(RECORD), "edited");
    assert_eq!(state(SERVICE), "missing");
    assert_eq!(
        state("src/main/java/com/example/notes/repository/NoteRepository.java"),
        "managed"
    );
    assert!(
        report["files"]
            .as_array()
            .unwrap()
            .iter()
            .all(|file| file["artifact"].as_str().unwrap().starts_with("art_")),
        "{report}"
    );

    let human = jails_cmd(&root, None)
        .args(["model", "status"])
        .output()
        .unwrap();
    let shown = String::from_utf8_lossy(&human.stdout);
    assert!(shown.contains(&format!("edited   {RECORD}")), "{shown}");
    assert!(shown.contains(&format!("missing  {SERVICE}")), "{shown}");
}

/// A warning is news once, and the report says it without dressing as a
/// refusal.
///
/// **The most-seen two lines in the tool used to be on every command.** A
/// project that declares `storage none` -- which is what `jails new` writes,
/// and what a reader who has not run `add db` yet has -- printed one
/// `storage-absent` warning *per entity*, with the `jails:` prefix a failure
/// wears, on stderr, above the report, on `set`, `unset`, `rename`, `model
/// plan` and every `g`. The reader wrote `storage none`; the model states it;
/// repeating it teaches them that the lines jails prints can be skipped.
///
/// Three things are asserted, because each is a different way for this to
/// come back: the fact is said on the transition that makes it true, it is
/// said about the *new* entity only when a second arrives, and no later
/// command says it at all.
#[test]
fn a_storage_warning_is_said_once_and_never_wears_the_refusal_prefix() {
    let root = model_project("warning-said-once", EMPTY_MODEL);
    write_spring_fixture(&root);

    let first = jails_cmd(&root, None)
        .args(["g", "scaffold", "Note", "id:uuid@pk", "title:string!"])
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let told = String::from_utf8_lossy(&first.stdout).to_string();
    // A `note` row in the file list, in the same column as the file verbs.
    assert!(
        told.contains("  note    `Note` is stored in memory only"),
        "{told}"
    );
    assert!(told.contains("fix: run `jails add db`"), "{told}");
    assert!(
        !String::from_utf8_lossy(&first.stderr).contains("jails:"),
        "a shape jails generated on purpose is not a failure: {}",
        String::from_utf8_lossy(&first.stderr)
    );

    // A second resource is news about the second resource, and only that.
    let second = jails_cmd(&root, None)
        .args(["g", "scaffold", "Task", "id:uuid@pk", "name:string!"])
        .output()
        .unwrap();
    let told = String::from_utf8_lossy(&second.stdout).to_string();
    assert!(told.contains("`Task` is stored in memory only"), "{told}");
    assert!(
        !told.contains("`Note` is stored in memory only"),
        "the model has said this since the last command: {told}"
    );

    // And every later command is quiet about a fact the source states.
    for arguments in [
        vec!["set", "server.port=8081"],
        vec!["model", "plan"],
        vec!["sync"],
        vec!["g", "record", "Money", "amount:long"],
    ] {
        let later = jails_cmd(&root, None).args(&arguments).output().unwrap();
        let stdout = String::from_utf8_lossy(&later.stdout).to_string();
        let stderr = String::from_utf8_lossy(&later.stderr).to_string();
        assert!(
            !stdout.contains("stored in memory only") && !stderr.contains("stored in memory only"),
            "`jails {}` repeated a warning: {stdout}{stderr}",
            arguments.join(" ")
        );
        assert!(
            !stderr.contains("jails:"),
            "`jails {}` printed a refusal prefix over a clean run: {stderr}",
            arguments.join(" ")
        );
    }
}
